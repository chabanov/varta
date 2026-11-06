//! HTTP registration publishes only its checked internal adapter, independently of tools.
use super::*;
use crate::modules::Graph;
use std::net::{IpAddr, SocketAddr};
use varta_common::http::{HttpContract, HttpLimits};

fn call(item: &ast::ModuleItem) -> Option<&ast::CallExpr> {
    let ast::ModuleItem::Stmt(ast::Stmt::Expr(stmt)) = item else {
        return None;
    };
    let ast::Expr::Call(call) = &*stmt.expr else {
        return None;
    };
    let ast::Callee::Expr(callee) = &call.callee else {
        return None;
    };
    let ast::Expr::Member(member) = &**callee else {
        return None;
    };
    let ast::MemberProp::Ident(method) = &member.prop else {
        return None;
    };
    let ast::Expr::Member(namespace) = &*member.obj else {
        return None;
    };
    let ast::MemberProp::Ident(http) = &namespace.prop else {
        return None;
    };
    matches!(&*namespace.obj, ast::Expr::Ident(v) if v.sym == "varta")
        .then_some(())
        .filter(|_| http.sym == "http" && method.sym == "serve")
        .map(|_| call)
}
pub(super) fn is_declaration(item: &ast::ModuleItem) -> bool {
    call(item).is_some()
}

pub(super) fn builtin(name: &str, types: &mut ir::ValueTypes) -> Result<Option<Type>> {
    if !matches!(
        name,
        "VartaHttpRequest" | "VartaHttpResponse" | "VartaHttpHeader"
    ) {
        return Ok(None);
    }
    if varta_common::http::version_for(types.profile).is_none() {
        return Err(unsupported(
            "HTTP v1 types in binary64 profile; adapters are not implemented",
        ));
    }
    let header = records::intern(
        vec![
            ir::RecordField {
                optional: false,
                name: "name".into(),
                ty: Type::String,
            },
            ir::RecordField {
                optional: false,
                name: "value".into(),
                ty: Type::String,
            },
        ],
        types,
    )?;
    if name == "VartaHttpHeader" {
        return Ok(Some(header));
    }
    let headers = arrays::intern(header, types)?;
    let mut fields = vec![
        ir::RecordField {
            optional: false,
            name: "body".into(),
            ty: Type::String,
        },
        ir::RecordField {
            optional: false,
            name: "headers".into(),
            ty: headers,
        },
    ];
    if name == "VartaHttpRequest" {
        fields.extend([
            ir::RecordField {
                optional: false,
                name: "method".into(),
                ty: Type::String,
            },
            ir::RecordField {
                optional: false,
                name: "target".into(),
                ty: Type::String,
            },
        ]);
    } else {
        fields.push(ir::RecordField {
            optional: false,
            name: "status".into(),
            ty: types.number(),
        });
    }
    records::intern(fields, types).map(Some)
}

pub(super) struct Service {
    pub entry: (usize, HttpContract),
    pub poll: Option<(usize, usize)>,
    /// A streaming service's `pull` and its chunk promise's state/value
    /// accessors (`varta_common::http::PULL`, `CHUNK_STATE`, `CHUNK_VALUE`).
    pub stream: Option<[usize; 3]>,
}

/// The fetch prelude's function that turns a handler's `Response` into the
/// streaming adapter's pull function.
const RESPONSE_BODY: &str = "httpResponseBody";
/// The fetch prelude's function that makes a Web `Request` handler's
/// argument of its envelope (`varta_common::http::WebRequestHead`).
const REQUEST_HEAD: &str = "httpRequest";

/// How an adapter calls the handler: with which argument, and whether it
/// awaits the handler's result.
#[derive(Clone, Copy)]
struct Call {
    /// The handler takes a Web `Request` (`httpRequest`), not a
    /// `VartaHttpRequest` parsed from the envelope.
    web: bool,
    awaited: bool,
}
impl Call {
    /// The adapter statements before the response is used: `request` of the
    /// envelope `text`, then `response` of the handler.
    fn prologue(self) -> String {
        let request = if self.web {
            "httpRequest(text)"
        } else {
            "JSON.parse(text) as VartaHttpRequest"
        };
        let awaited = if self.awaited { "await " } else { "" };
        format!("const request={request};const response={awaited}handler(request);")
    }
}

pub(super) fn collect(
    graph: &Graph,
    scopes: &BTreeMap<std::path::PathBuf, BTreeMap<String, Signature>>,
    functions: &mut Vec<ir::Function>,
    types: &mut ir::ValueTypes,
    main: usize,
) -> Result<Option<Service>> {
    let mut service = None;
    for path in &graph.order {
        let unit = &graph.units[path];
        for item in &unit.ast.body {
            let Some(call) = call(item) else {
                continue;
            };
            let result = (|| {
                if path != &graph.entry || service.is_some() {
                    return Err(invalid(
                        "one HTTP service declaration is allowed, in the entry module",
                    ));
                }
                if call.type_args.is_some()
                    || call.args.len() != 2
                    || call.args.iter().any(|a| a.spread.is_some())
                {
                    return Err(unsupported(
                        "HTTP serve requires literal options and a named handler",
                    ));
                }
                let ast::Expr::Object(options) = &*call.args[0].expr else {
                    return Err(unsupported("literal HTTP options"));
                };
                let mut hostname = None;
                let mut port = None;
                for prop in &options.props {
                    let ast::PropOrSpread::Prop(prop) = prop else {
                        return Err(unsupported("HTTP option spread"));
                    };
                    let ast::Prop::KeyValue(prop) = &**prop else {
                        return Err(unsupported("HTTP option property"));
                    };
                    let key = match &prop.key {
                        ast::PropName::Ident(k) => k.sym.as_ref(),
                        _ => return Err(unsupported("HTTP option key")),
                    };
                    match (key, &*prop.value) {
                        ("hostname", ast::Expr::Lit(ast::Lit::Str(s))) if hostname.is_none() => {
                            hostname = Some(
                                s.value
                                    .as_str()
                                    .ok_or_else(|| invalid("HTTP hostname must be UTF-8"))?
                                    .parse::<IpAddr>()
                                    .map_err(|_| {
                                        unit.source.locate(
                                            invalid("HTTP hostname must be a literal IP address"),
                                            prop.value.span(),
                                        )
                                    })?,
                            );
                        }
                        ("port", ast::Expr::Lit(ast::Lit::Num(n)))
                            if port.is_none()
                                && n.value.fract() == 0.0
                                && n.value >= 0.0
                                && n.value <= u16::MAX as f64 =>
                        {
                            port = Some(n.value as u16)
                        }
                        _ => return Err(invalid("unknown, duplicate or nonliteral HTTP option")),
                    }
                }
                let addr = SocketAddr::new(
                    hostname.ok_or_else(|| invalid("HTTP hostname is required"))?,
                    port.ok_or_else(|| invalid("HTTP port is required"))?,
                );
                let ast::Expr::Ident(name) = &*call.args[1].expr else {
                    return Err(unsupported("named HTTP handler"));
                };
                let handler = scopes[path]
                    .get(name.sym.as_ref())
                    .ok_or_else(|| invalid("unknown HTTP handler"))?;
                let request = builtin("VartaHttpRequest", types)?
                    .ok_or_else(|| invalid("missing HTTP request type"))?;
                let response = builtin("VartaHttpResponse", types)?
                    .ok_or_else(|| invalid("missing HTTP response type"))?;
                let resolves = |ty: Type, target: Type| {
                    matches!(ty, Type::Promise(id) if types.promises[id as usize].value == target)
                };
                let asynchronous =
                    types.profile.turn_budgets() && resolves(handler.result, response);
                // V2 streaming: a Web `Response`, synchronously or through a
                // promise (the fetch prelude is loaded once a module names it).
                let body = types
                    .profile
                    .turn_budgets()
                    .then(|| {
                        types
                            .engine
                            .function(&crate::modules::engine::FETCH, RESPONSE_BODY)
                            .cloned()
                    })
                    .flatten();
                let streaming = body.as_ref().and_then(|body| {
                    let web = *body.params.first()?;
                    (handler.result == web || resolves(handler.result, web))
                        .then_some(handler.result == web)
                });
                // V2 Web `Request` handlers (the prelude is loaded once a
                // module names `Request`).
                let inbound = types
                    .profile
                    .turn_budgets()
                    .then(|| {
                        types
                            .engine
                            .function(&crate::modules::engine::FETCH, REQUEST_HEAD)
                            .cloned()
                    })
                    .flatten();
                let web = inbound
                    .as_ref()
                    .is_some_and(|inbound| handler.params == [inbound.result]);
                if handler.index == main
                    || handler.receiver.is_some()
                    || handler.shape.extended()
                    || handler.params != [request] && !web
                    || handler.result != response && !asynchronous && streaming.is_none()
                {
                    return Err(type_error(if types.profile.turn_budgets() {
                        "HTTP handler must have signature (VartaHttpRequest or Request) => VartaHttpResponse, Promise<VartaHttpResponse>, Response or Promise<Response>"
                    } else {
                        "HTTP handler must have signature (VartaHttpRequest) => VartaHttpResponse"
                    }));
                }
                let mut effects = tools::function_effects(handler.index, functions);
                effects.insert(varta_common::http::LISTEN_CAPABILITY.into());
                let version = varta_common::http::version_for(types.profile)
                    .ok_or_else(|| unsupported("HTTP adapter for this profile"))?;
                let contract = HttpContract {
                    version,
                    asynchronous: asynchronous || streaming.is_some() || web,
                    streaming: streaming.is_some(),
                    web_request: web,
                    bind: addr.to_string(),
                    export: varta_common::http::HANDLER.into(),
                    required_effects: effects.into_iter().collect(),
                    limits: HttpLimits::for_version(version),
                };
                contract.validate().map_err(|e| invalid(e.to_string()))?;
                let mut symbols = BTreeMap::from([("handler".into(), handler.clone())]);
                if let (true, Some(inbound)) = (web, &inbound) {
                    symbols.insert(REQUEST_HEAD.into(), inbound.clone());
                }
                if let (Some(synchronous), Some(body)) = (streaming, &body) {
                    symbols.insert("body".into(), body.clone());
                    let call = Call {
                        web,
                        awaited: !synchronous,
                    };
                    let (index, poll, stream) =
                        stream_adapter(call, &symbols, functions, types, main)?;
                    return Ok(Service {
                        entry: (index, contract),
                        poll: Some(poll),
                        stream: Some(stream),
                    });
                }
                if asynchronous || web {
                    let call = Call {
                        web,
                        awaited: asynchronous,
                    };
                    let (index, poll) = async_adapter(call, &symbols, functions, types, main)?;
                    return Ok(Service {
                        entry: (index, contract),
                        poll: Some(poll),
                        stream: None,
                    });
                }
                let text = Expr {
                    span: call.span,
                    ty: Type::String,
                    kind: E::Local(0),
                };
                let request = Expr {
                    span: call.span,
                    ty: request,
                    kind: E::ParseJson(Box::new(text)),
                };
                let response = Expr {
                    span: call.span,
                    ty: response,
                    kind: E::Call(handler.index, vec![request]),
                };
                let output = Expr {
                    span: call.span,
                    ty: Type::String,
                    kind: E::Json(Box::new(response), None),
                };
                let index = functions.len();
                functions.push(ir::Function {
                    shape: ir::ParameterShape::fixed(1),
                    self_slot: None,
                    receiver: None,
                    cells: Default::default(),
                    captures: vec![],
                    environment: false,
                    generator: None,
                    source: unit.source.clone(),
                    span: call.span,
                    params: vec![Type::String],
                    result: Type::String,
                    locals: vec![Type::String],
                    body: vec![S::Return(Some(output))],
                });
                Ok(Service {
                    entry: (index, contract),
                    poll: None,
                    stream: None,
                })
            })()
            .map_err(|e| unit.source.locate(e, call.span))?;
            service = Some(result);
        }
    }
    Ok(service)
}

/// Reuse the language's async lowering, normalization, exception handling and
/// frame liveness. The synthetic source has only a private handler binding.
fn async_adapter(
    call: Call,
    symbols: &BTreeMap<String, Signature>,
    functions: &mut Vec<ir::Function>,
    types: &mut ir::ValueTypes,
    main: usize,
) -> Result<(usize, (usize, usize))> {
    let source = format!(
        "async function adapter(text:string):Promise<string>{{{}return JSON.stringify(response);}}",
        call.prologue()
    );
    let (index, promise, source, span) = synthesize(&source, symbols, functions, types, main)?;
    let poll = accessors(promise, &source, span, functions, types)?;
    Ok((index, poll))
}

/// The v2 streaming adapter: `handle` resolves to a record holding the pull
/// function of the handler's `Response` (the fetch prelude's
/// [`RESPONSE_BODY`], `body` in `symbols`); `pull` calls it, and its
/// `Promise<string>` has accessors of its own.
fn stream_adapter(
    call: Call,
    symbols: &BTreeMap<String, Signature>,
    functions: &mut Vec<ir::Function>,
    types: &mut ir::ValueTypes,
    main: usize,
) -> Result<(usize, (usize, usize), [usize; 3])> {
    // A promise holds no function value: the pull function travels in a
    // record, which the host roots once. `pull` keeps the chunk promise it
    // returns in that record's `current`, so the promise stays reachable
    // until the next pull without a host root per chunk.
    const PULL: &str = "function pull(body:{next:() => Promise<string>;current:Promise<string>}):Promise<string>{body.current=body.next();return body.current;}";
    let adapter = format!(
        "async function adapter(text:string):Promise<{{next:() => Promise<string>;current:Promise<string>}}>{{{}return {{next:body(response),current:Promise.resolve('')}};}}",
        call.prologue()
    );
    let (index, promise, source, span) = synthesize(&adapter, symbols, functions, types, main)?;
    let poll = accessors(promise, &source, span, functions, types)?;
    let (pull, chunk, source, span) = synthesize(PULL, &BTreeMap::new(), functions, types, main)?;
    let (state, value) = accessors(chunk, &source, span, functions, types)?;
    Ok((index, poll, [pull, state, value]))
}

/// Compile the one function of a synthetic `text` against `symbols`: its
/// index, result type, source and span.
fn synthesize(
    text: &str,
    symbols: &BTreeMap<String, Signature>,
    functions: &mut Vec<ir::Function>,
    types: &mut ir::ValueTypes,
    main: usize,
) -> Result<(usize, Type, Source, swc_common::Span)> {
    let source = Source::new(std::path::Path::new("__varta_http_async.ts"), text);
    let module = crate::parse::parse_module(&source, false)?;
    let declaration = declarations::collect(&module, &source, types.profile)?
        .into_iter()
        .next()
        .ok_or_else(|| invalid("missing HTTP adapter function"))?;
    let index = functions.len();
    let signature = signature::check(declaration, index, &source, types)?;
    let result = signature.result;
    let mut generated = BTreeMap::new();
    let mut next = index + 1;
    let mut impacts = BTreeMap::new();
    let function = link::body(
        declaration,
        (&signature, &Default::default()),
        symbols,
        &source,
        main,
        types,
        (&mut generated, &mut next, &mut impacts),
    )?;
    functions.push(function);
    functions.extend(generated.into_values());
    Ok((index, result, source.clone(), declaration.span()))
}

/// Pure state and value accessors of `promise`.
fn accessors(
    promise: Type,
    source: &Source,
    span: swc_common::Span,
    functions: &mut Vec<ir::Function>,
    types: &mut ir::ValueTypes,
) -> Result<(usize, usize)> {
    let Type::Promise(id) = promise else {
        return Err(invalid("async HTTP result is not a promise"));
    };
    let shape = types.promises[id as usize].shape;
    let mut accessors = Vec::new();
    for name in [promises::fields::STATE, promises::fields::VALUE] {
        let (slot, field) = types.records[shape as usize]
            .fields
            .iter()
            .enumerate()
            .find(|(_, f)| f.name == name)
            .ok_or_else(|| invalid("missing HTTP promise field"))?;
        let result = field.ty;
        let record = Expr {
            span,
            ty: Type::Record(shape),
            kind: E::Reinterpret(Box::new(Expr {
                span,
                ty: promise,
                kind: E::Local(0),
            })),
        };
        let value = Expr {
            span,
            ty: result,
            kind: E::Field(Box::new(record), slot as u32),
        };
        accessors.push(functions.len());
        functions.push(ir::Function {
            shape: ir::ParameterShape::fixed(1),
            self_slot: None,
            receiver: None,
            cells: Default::default(),
            captures: vec![],
            environment: false,
            generator: None,
            source: source.clone(),
            span,
            params: vec![promise],
            result,
            locals: vec![promise],
            body: vec![S::Return(Some(value))],
        });
    }
    Ok((accessors[0], accessors[1]))
}
