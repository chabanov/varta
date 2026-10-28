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
    if types.profile.binary64() {
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
            ty: Type::Number,
        });
    }
    records::intern(fields, types).map(Some)
}

pub(super) fn collect(
    graph: &Graph,
    scopes: &BTreeMap<std::path::PathBuf, BTreeMap<String, Signature>>,
    functions: &mut Vec<ir::Function>,
    types: &mut ir::ValueTypes,
    main: usize,
) -> Result<Option<(usize, HttpContract)>> {
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
                if handler.index == main
                    || handler.receiver.is_some()
                    || handler.shape.extended()
                    || handler.params != [request]
                    || handler.result != response
                {
                    return Err(type_error(
                        "HTTP handler must have signature (VartaHttpRequest) => VartaHttpResponse",
                    ));
                }
                let mut effects = tools::function_effects(handler.index, functions);
                effects.insert("net.listen".into());
                let contract = HttpContract {
                    version: varta_common::http::VERSION,
                    bind: addr.to_string(),
                    export: varta_common::http::HANDLER.into(),
                    required_effects: effects.into_iter().collect(),
                    limits: HttpLimits::default(),
                };
                contract.validate().map_err(|e| invalid(e.to_string()))?;
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
                    source: unit.source.clone(),
                    span: call.span,
                    params: vec![Type::String],
                    result: Type::String,
                    locals: vec![Type::String],
                    body: vec![S::Return(Some(output))],
                });
                Ok((index, contract))
            })()
            .map_err(|e| unit.source.locate(e, call.span))?;
            service = Some(result);
        }
    }
    Ok(service)
}
