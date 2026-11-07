//! Generic construct signatures (v38 `type_level`, step 4: `new <T>(value:
//! T) => Box<T>`, `{ new <T>(value: T): Box<T> }`), by the coordinator's
//! decision under the owner's delegated authority (2026-11-06): a
//! whole-program pass gives each constructor value a table of the
//! instantiations the program uses, and a construction dispatches
//! statically where its type arguments are known.
//!
//! A monomorphized class has one layout per instantiation, so one value
//! cannot hold a single constructing closure. A value of a generic construct
//! type is instead a function `(selector: number) => unknown`: the selector
//! names one instantiation (type argument list) of the program, and the
//! result is the closure that constructs it, `(params at T) => result at T`,
//! held by `unknown` and unboxed at its exact function type (the compiler
//! built both sides). `new ctor<T>(args)` instantiates the signature with
//! explicit or inferred type arguments, takes the instantiation's selector
//! and calls the closure with the arguments checked against the instance.
//!
//! A class named where such a type is expected becomes the table of its
//! class: a selector function whose body this module builds after every
//! body is lowered (`fill`), with one entry per instantiation that the
//! program constructs through any signature of the same family (generic
//! construct types a value converts between are joined). Each entry is an
//! instance of a synthesized generic function
//! `function __vartaDeclNew$n<P…, R>(a…: P…): R { return new C(a…); }`
//! declared in the module that named the class, so the class instance is
//! inferred and checked exactly as a source `new C(a…)` with the instance's
//! parameter and result types. The fill and the generic bodies run to a
//! fixpoint, since a lowered body may construct further instantiations.
use super::*;
use std::rc::Rc;

/// Prefix of the synthesized declarations (reserved by K9 B: no program
/// identifier starts with `__vartaDecl`).
const PREFIX: &str = "__vartaDeclNew";

/// One generic construct signature declaration (with the type parameter
/// frame it was resolved in).
pub(crate) struct Entry {
    params: Rc<ast::TsTypeParamDecl>,
    function: Rc<ast::TsFnType>,
    module: usize,
    source: Option<Source>,
    frame: generics::Frame,
    key: (usize, u32, u32),
    /// The marked function type of the signature.
    ty: Type,
}

/// The table of one class converted to a generic construct type.
pub(crate) struct Table {
    entry: u32,
    module: usize,
    /// The synthesized generic function constructing the class.
    generic: usize,
    /// The selector function, the table's value.
    function: usize,
    source: Source,
    span: swc_common::Span,
    /// The instance function of each selector built so far.
    instances: BTreeMap<u32, usize>,
}

#[derive(Default)]
pub(crate) struct Constructors {
    entries: Vec<Entry>,
    /// Union-find over entries joined by a conversion (recorded while a
    /// value is widened, which sees the types immutably).
    parent: std::cell::RefCell<Vec<u32>>,
    /// Every instantiation constructed, by selector.
    instantiations: Vec<Vec<Type>>,
    /// (entry, selector) of every construction.
    uses: BTreeSet<(u32, u32)>,
    tables: Vec<Table>,
    by_class: BTreeMap<(u32, u32, usize), u32>,
}

impl Constructors {
    fn root(&self, mut entry: u32) -> u32 {
        let parent = self.parent.borrow();
        while parent[entry as usize] != entry {
            entry = parent[entry as usize];
        }
        entry
    }
    fn join(&self, a: u32, b: u32) {
        let (a, b) = (self.root(a), self.root(b));
        if a != b {
            self.parent.borrow_mut()[a.max(b) as usize] = a.min(b);
        }
    }
}

/// The marked function type of a generic construct signature.
pub(super) fn signature(
    span: swc_common::Span,
    params: &[ast::TsFnParam],
    result: Option<&ast::TsTypeAnn>,
    type_params: &ast::TsTypeParamDecl,
    types: &mut ir::ValueTypes,
) -> Result<Type> {
    let result = result
        .cloned()
        .ok_or_else(|| type_error("a call or construct signature requires a result type"))?;
    for param in params {
        if matches!(param, ast::TsFnParam::Ident(id) if id.id.sym == "this") {
            return Err(type_error(
                "a construct signature cannot declare a this parameter",
            ));
        }
    }
    for param in &type_params.params {
        if param.is_in || param.is_out || param.is_const {
            return Err(unsupported("in/out/const type parameter modifier"));
        }
    }
    let module = named::current(types);
    let frame = generics::visible(types);
    let key = (module, span.lo.0, span.hi.0);
    let registry = &types.type_level.constructors;
    if let Some(entry) = registry
        .entries
        .iter()
        .find(|entry| entry.key == key && entry.frame == frame)
    {
        return Ok(entry.ty);
    }
    let id = registry.entries.len() as u32;
    let selector = types.number();
    let mut signature = ir::FunctionType {
        shape: ir::ParameterShape::fixed(1),
        receiver: None,
        params: vec![selector],
        result: Type::Unknown,
        generic: None,
        construct: true,
        overloads: None,
        constructors: Some(id),
        properties: None,
    };
    if types.functions.len() >= types.profile.limits().function_types {
        return Err(invalid("function type count exceeds limit"));
    }
    signature.shape = ir::ParameterShape::fixed(1);
    types.functions.push(signature);
    let ty = Type::Function(types.functions.len() as u32 - 1);
    let function = ast::TsFnType {
        span,
        params: params.to_vec(),
        type_params: None,
        type_ann: Box::new(result),
    };
    types.type_level.constructors.entries.push(Entry {
        params: Rc::new(type_params.clone()),
        function: Rc::new(function),
        module,
        source: named::current_source(types),
        frame,
        key,
        ty,
    });
    types.type_level.constructors.parent.borrow_mut().push(id);
    Ok(ty)
}

/// The generic constructor entry of a function type, if any.
pub(in crate::lower) fn entry_of(ty: Type, types: &ir::ValueTypes) -> Option<u32> {
    match ty {
        Type::Function(id) => types.functions[id as usize].constructors,
        _ => None,
    }
}

/// The instance call type of `entry` for the type arguments `arguments`:
/// `(params at T) => result at T`.
fn instance(entry: u32, arguments: &[Type], types: &mut ir::ValueTypes) -> Result<Type> {
    let declaration = &types.type_level.constructors.entries[entry as usize];
    let (params, function, module, frame) = (
        Rc::clone(&declaration.params),
        Rc::clone(&declaration.function),
        declaration.module,
        declaration.frame.clone(),
    );
    let source = declaration.source.clone();
    let supplied: Vec<Option<Type>> = arguments.iter().copied().map(Some).collect();
    let current = named::current(types);
    named::enter(types, module);
    let result = generics::with_frame(types, frame, |t| {
        let Some(source) = source.as_ref() else {
            return Err(invalid("missing generic construct signature source"));
        };
        let bound = generics::bind(
            &params,
            &supplied,
            "generic construct signature",
            generics::Missing::Inference,
            source,
            t,
        )?;
        let mut visible = generics::visible(t);
        visible.extend(bound);
        generics::with_frame(t, visible, |t| functions::type_annotation(&function, t))
    });
    named::enter(types, current);
    result
}

impl Lower<'_> {
    /// A class named where the generic construct type `expected` is
    /// expected: the selector function of its table (one per class, family
    /// and module, so the class names one value as in JavaScript).
    pub(in crate::lower) fn generic_constructor_value(
        &mut self,
        id: &ast::Ident,
        class: u32,
        expected: Type,
    ) -> Result<Expr> {
        let Some(entry) = entry_of(expected, self.types) else {
            return Err(invalid("missing generic construct signature"));
        };
        let module = named::current(self.types);
        let root = self.types.type_level.constructors.root(entry);
        let key = (class, root, module);
        let selector = self.types.number();
        let plain = functions::intern_full(
            None,
            vec![selector],
            ir::ParameterShape::fixed(1),
            Type::Unknown,
            self.types,
        )?;
        let function = match self.types.type_level.constructors.by_class.get(&key) {
            Some(table) => self.types.type_level.constructors.tables[*table as usize].function,
            None => self.declare_table(id, (entry, class, module))?,
        };
        let value = Expr {
            span: id.span,
            ty: plain,
            kind: E::FunctionRef {
                index: function,
                fresh: false,
                captures: vec![],
            },
        };
        Ok(Expr {
            span: id.span,
            ty: expected,
            kind: E::Reinterpret(Box::new(value)),
        })
    }

    /// Declare the synthesized generic function and the selector function
    /// of a new table.
    fn declare_table(
        &mut self,
        id: &ast::Ident,
        (entry, class, module): (u32, u32, usize),
    ) -> Result<usize> {
        let span = id.span;
        let count = self.types.type_level.constructors.tables.len();
        let declaration = &self.types.type_level.constructors.entries[entry as usize];
        let function = Rc::clone(&declaration.function);
        let ident = |name: String| ast::Ident::new(name.into(), span, Default::default());
        let reference = |name: String| {
            Box::new(ast::TsType::TsTypeRef(ast::TsTypeRef {
                span,
                type_name: ast::TsEntityName::Ident(ident(name)),
                type_params: None,
            }))
        };
        let annotation = |name: String| {
            Some(Box::new(ast::TsTypeAnn {
                span,
                type_ann: reference(name),
            }))
        };
        let mut type_params = Vec::new();
        let mut params = Vec::new();
        let mut args = Vec::new();
        for (position, param) in function.params.iter().enumerate() {
            let parameter = format!("{PREFIX}P{position}");
            let name = format!("{PREFIX}A{position}");
            type_params.push(parameter.clone());
            let pat = match param {
                ast::TsFnParam::Rest(_) => ast::Pat::Rest(ast::RestPat {
                    span,
                    dot3_token: span,
                    arg: Box::new(ast::Pat::Ident(ast::BindingIdent {
                        id: ident(name.clone()),
                        type_ann: None,
                    })),
                    type_ann: annotation(parameter),
                }),
                ast::TsFnParam::Ident(binding) => {
                    let mut id = ident(name.clone());
                    id.optional = binding.id.optional;
                    ast::Pat::Ident(ast::BindingIdent {
                        id,
                        type_ann: annotation(parameter),
                    })
                }
                _ => {
                    return Err(self.source.locate(
                        unsupported("destructuring parameter in a generic construct signature"),
                        span,
                    ));
                }
            };
            params.push(ast::Param {
                span,
                decorators: Vec::new(),
                pat,
            });
            args.push(ast::ExprOrSpread {
                spread: matches!(param, ast::TsFnParam::Rest(_)).then_some(span),
                expr: Box::new(ast::Expr::Ident(ident(name))),
            });
        }
        let result = format!("{PREFIX}R");
        type_params.push(result.clone());
        let construct = ast::Expr::New(ast::NewExpr {
            span,
            ctxt: Default::default(),
            callee: Box::new(ast::Expr::Ident(id.clone())),
            args: Some(args),
            type_args: None,
        });
        let declaration = ast::FnDecl {
            ident: ident(format!("{PREFIX}{count}")),
            declare: false,
            function: Box::new(ast::Function {
                params,
                decorators: Vec::new(),
                span,
                ctxt: Default::default(),
                body: Some(ast::FunctionBody {
                    span,
                    stmts: vec![ast::Stmt::Return(ast::ReturnStmt {
                        span,
                        arg: Some(Box::new(construct)),
                    })],
                }),
                is_generator: false,
                is_async: false,
                type_params: Some(Box::new(ast::TsTypeParamDecl {
                    span,
                    params: type_params
                        .into_iter()
                        .map(|name| ast::TsTypeParam {
                            span,
                            name: ident(name),
                            is_in: false,
                            is_out: false,
                            is_const: false,
                            constraint: None,
                            default: None,
                        })
                        .collect(),
                })),
                return_type: annotation(result),
                this_param: None,
            }),
        };
        let generic = generics::declare_synthesized(&declaration, module, self.source, self.types)
            .map_err(|e| self.source.locate(e, span))?;
        // The selector function: its body is built by `fill`.
        let index = *self.next_function;
        if index >= self.types.function_limit() {
            return Err(self.source.locate(
                invalid("too many functions including generic constructor tables"),
                span,
            ));
        }
        *self.next_function += 1;
        let selector = self.types.number();
        self.generated.insert(
            index,
            selector_function(self.source.clone(), span, selector, Vec::new()),
        );
        let table = self.types.type_level.constructors.tables.len() as u32;
        self.types.type_level.constructors.tables.push(Table {
            entry,
            module,
            generic,
            function: index,
            source: self.source.clone(),
            span,
            instances: BTreeMap::new(),
        });
        let root = self.types.type_level.constructors.root(entry);
        self.types
            .type_level
            .constructors
            .by_class
            .insert((class, root, module), table);
        Ok(index)
    }

    /// `new ctor<T>(args)` of a value of a generic construct type: the
    /// type arguments are explicit or inferred from the arguments as for a
    /// generic function call (`generic_calls::infer`), then defaults, then
    /// `unknown`.
    pub(in crate::lower) fn generic_construct(
        &mut self,
        new: &ast::NewExpr,
        callee: Expr,
        entry: u32,
    ) -> Result<Expr> {
        let span = new.span;
        let args = new.args.clone().unwrap_or_default();
        if args.iter().any(|arg| arg.spread.is_some()) {
            return Err(self.source.locate(
                unsupported("spread argument to a generic constructor value"),
                span,
            ));
        }
        let call = ast::CallExpr {
            span,
            ctxt: Default::default(),
            callee: ast::Callee::Expr(new.callee.clone()),
            args,
            type_args: new.type_args.clone(),
        };
        let function = self.constructor_function(entry)?;
        let (supplied, values) = match &call.type_args {
            Some(explicit) => {
                let explicit = generics::arguments(explicit, self.types)
                    .map_err(|e| self.source.locate(e, explicit.span))?;
                (explicit.into_iter().map(Some).collect(), Vec::new())
            }
            None => self.infer(&function, &call, None)?,
        };
        let arguments = self.complete_arguments(&function.params, supplied, span)?;
        let instance =
            instance(entry, &arguments, self.types).map_err(|e| self.source.locate(e, span))?;
        let Type::Function(instance_id) = instance else {
            return Err(invalid("missing generic construct instance"));
        };
        let signature = self.types.functions[instance_id as usize].clone();
        let registry = &mut self.types.type_level.constructors;
        let selector = match registry.instantiations.iter().position(|a| *a == arguments) {
            Some(selector) => selector,
            None => {
                registry.instantiations.push(arguments);
                registry.instantiations.len() - 1
            }
        } as u32;
        registry.uses.insert((entry, selector));
        // The arguments the inference left unlowered, against the instance.
        let mut values = values;
        arguments::count(call.args.len(), &signature.shape)
            .map_err(|e| self.source.locate(e, span))?;
        for (index, arg) in call.args.iter().enumerate().skip(values.len()) {
            let context =
                arguments::context(index, &signature.params, &signature.shape, self.types);
            values.push(self.expr_expected(&arg.expr, context)?);
        }
        let values = self.bind_arguments(values, &signature.params, &signature.shape, span)?;
        let number = self.types.number();
        let plain = functions::intern_full(
            None,
            vec![number],
            ir::ParameterShape::fixed(1),
            Type::Unknown,
            self.types,
        )?;
        let table = Expr {
            span: callee.span,
            ty: plain,
            kind: E::Reinterpret(Box::new(callee)),
        };
        let selected = self.invoke_value(
            table,
            vec![Expr {
                span,
                ty: number,
                kind: E::Float64(f64::from(selector)),
            }],
            span,
        )?;
        // The table returns the instance's closure, built by `fill` at this
        // exact function type.
        let closure = Expr {
            span,
            ty: instance,
            kind: E::UnboxUnion(Box::new(selected)),
        };
        self.invoke_bound_receiver(closure, None, values, span)
    }

    /// The signature of `entry` as a generic function declaration, for
    /// argument inference.
    fn constructor_function(&self, entry: u32) -> Result<generics::Function> {
        let declaration = &self.types.type_level.constructors.entries[entry as usize];
        let function = &declaration.function;
        let span = function.span;
        let mut params = Vec::new();
        for param in &function.params {
            params.push(ast::Param {
                span,
                decorators: Vec::new(),
                pat: match param {
                    ast::TsFnParam::Ident(id) => ast::Pat::Ident(id.clone()),
                    ast::TsFnParam::Rest(rest) => ast::Pat::Rest(rest.clone()),
                    ast::TsFnParam::Array(array) => ast::Pat::Array(array.clone()),
                    ast::TsFnParam::Object(object) => ast::Pat::Object(object.clone()),
                },
            });
        }
        Ok(generics::Function {
            module: declaration.module,
            kind: "construct signature",
            name: "new".into(),
            declaration: ast::FnDecl {
                ident: ast::Ident::new("new".into(), span, Default::default()),
                declare: true,
                function: Box::new(ast::Function {
                    params,
                    decorators: Vec::new(),
                    span,
                    ctxt: Default::default(),
                    body: None,
                    is_generator: false,
                    is_async: false,
                    type_params: None,
                    return_type: Some(function.type_ann.clone()),
                    this_param: None,
                }),
            },
            params: (*declaration.params).clone(),
            source: declaration
                .source
                .clone()
                .unwrap_or_else(|| self.source.clone()),
        })
    }

    /// The type arguments `supplied` completed: a missing one takes its
    /// default, or `unknown` (TypeScript's inference without a candidate).
    fn complete_arguments(
        &mut self,
        params: &ast::TsTypeParamDecl,
        mut supplied: Vec<Option<Type>>,
        span: swc_common::Span,
    ) -> Result<Vec<Type>> {
        if supplied.len() > params.params.len() {
            return Err(self.source.locate(
                type_error(format!(
                    "expected at most {} type arguments for the generic construct signature",
                    params.params.len()
                )),
                span,
            ));
        }
        supplied.resize(params.params.len(), None);
        for (slot, param) in supplied.iter_mut().zip(&params.params) {
            if slot.is_none() && param.default.is_none() {
                *slot = Some(Type::Unknown);
            }
        }
        let frame = generics::bind(
            params,
            &supplied,
            "generic construct signature",
            generics::Missing::Inference,
            self.source,
            self.types,
        )
        .map_err(|e| self.source.locate(e, span))?;
        Ok(params
            .params
            .iter()
            .map(|param| frame[param.name.sym.as_ref()])
            .collect())
    }
}

/// A value of the generic construct type `given` used as `wanted`: both
/// are physically selector functions, so the value is retyped and the two
/// signatures join one family whose tables serve both (their instances are
/// compared by `fill`).
pub(in crate::lower) fn convert(value: Expr, wanted: Type, types: &ir::ValueTypes) -> Expr {
    let (Some(given), Some(target)) = (entry_of(value.ty, types), entry_of(wanted, types)) else {
        return value;
    };
    if given != target {
        let registry = &types.type_level.constructors;
        if registry.entries[given as usize].params.params.len()
            != registry.entries[target as usize].params.params.len()
        {
            return value;
        }
        types.type_level.constructors.join(given, target);
    }
    Expr {
        span: value.span,
        ty: wanted,
        kind: E::Reinterpret(Box::new(value)),
    }
}

/// The selector function of a table: `selector == s` returns the boxed
/// closure of instance `s`; any other selector traps (no construction of
/// the program asks for it).
fn selector_function(
    source: Source,
    span: swc_common::Span,
    selector: Type,
    cases: Vec<(u32, Expr)>,
) -> ir::Function {
    let mut body = Vec::new();
    for (value, closure) in cases {
        body.push(S::If(
            Expr {
                span,
                ty: Type::Boolean,
                kind: E::Binary(
                    ast::BinaryOp::EqEqEq,
                    Box::new(Expr {
                        span,
                        ty: selector,
                        kind: E::Local(0),
                    }),
                    Box::new(Expr {
                        span,
                        ty: selector,
                        kind: E::Float64(f64::from(value)),
                    }),
                ),
            },
            vec![S::Return(Some(closure))],
            Vec::new(),
        ));
    }
    body.push(S::Trap);
    ir::Function {
        shape: ir::ParameterShape::fixed(1),
        self_slot: None,
        receiver: None,
        source,
        span,
        params: vec![selector],
        result: Type::Unknown,
        locals: vec![selector],
        body,
        cells: BTreeSet::new(),
        captures: Vec::new(),
        environment: false,
        generator: None,
    }
}

/// Build every table entry the program's constructions need (see the
/// module documentation). `Ok(true)` when a new instance was requested,
/// whose body (and any construction it makes) the generic bodies lower
/// before the next round.
pub(in crate::lower) fn fill(
    types: &mut ir::ValueTypes,
    generated: &mut BTreeMap<usize, ir::Function>,
    next: &mut usize,
) -> Result<bool> {
    let mut requested = false;
    for table in 0..types.type_level.constructors.tables.len() {
        let (entry, generic, module) = {
            let table = &types.type_level.constructors.tables[table];
            (table.entry, table.generic, table.module)
        };
        let registry = &types.type_level.constructors;
        let root = registry.root(entry);
        let needed: BTreeSet<u32> = registry
            .uses
            .iter()
            .filter(|(used, _)| registry.root(*used) == root)
            .map(|(_, selector)| *selector)
            .collect();
        let mut changed = false;
        for selector in needed {
            if types.type_level.constructors.tables[table]
                .instances
                .contains_key(&selector)
            {
                continue;
            }
            let arguments = types.type_level.constructors.instantiations[selector as usize].clone();
            let (source, span) = {
                let table = &types.type_level.constructors.tables[table];
                (table.source.clone(), table.span)
            };
            let call = instance(entry, &arguments, types).map_err(|e| source.locate(e, span))?;
            // Every signature of the family that constructs this
            // instantiation must agree on its instance type.
            let users: Vec<u32> = types
                .type_level
                .constructors
                .uses
                .iter()
                .filter(|(used, s)| {
                    *s == selector && types.type_level.constructors.root(*used) == root
                })
                .map(|(used, _)| *used)
                .collect();
            for user in users {
                let other =
                    instance(user, &arguments, types).map_err(|e| source.locate(e, span))?;
                if other != call {
                    return Err(source.locate(
                        type_error(format!(
                            "generic construct signatures {} and {} differ for the type arguments \
                             they are constructed with",
                            void_functions::describe(call, types),
                            void_functions::describe(other, types)
                        )),
                        span,
                    ));
                }
            }
            let Type::Function(call_id) = call else {
                return Err(invalid("missing generic construct instance"));
            };
            let signature = types.functions[call_id as usize].clone();
            let mut supplied: Vec<Option<Type>> =
                signature.params.iter().copied().map(Some).collect();
            supplied.push(Some(signature.result));
            let current = named::current(types);
            named::enter(types, module);
            let instance = generics::instantiate(generic, &supplied, next, types);
            named::enter(types, current);
            let instance = instance.map_err(|e| source.locate(e, span))?;
            types.type_level.constructors.tables[table]
                .instances
                .insert(selector, instance.index);
            requested = true;
            changed = true;
        }
        if changed {
            let table = &types.type_level.constructors.tables[table];
            let (source, span, function) = (table.source.clone(), table.span, table.function);
            let mut cases = Vec::new();
            for (selector, index) in table.instances.clone() {
                let arguments =
                    types.type_level.constructors.instantiations[selector as usize].clone();
                let call = instance(entry, &arguments, types)?;
                let closure = Expr {
                    span,
                    ty: call,
                    kind: E::FunctionRef {
                        index,
                        fresh: false,
                        captures: vec![],
                    },
                };
                cases.push((selector, unknown::widen(closure, types)?));
            }
            let selector = types.number();
            generated.insert(function, selector_function(source, span, selector, cases));
        }
    }
    Ok(requested)
}
