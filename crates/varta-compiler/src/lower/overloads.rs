//! Function, method and constructor overloads (v38 `declaration_forms`).
//!
//! Overload signatures have no body; a call resolves, as TypeScript does, to
//! the first signature in declaration order whose arity admits the
//! arguments and whose parameters accept every argument type. The call runs
//! the implementation (whose signature is never visible to callers) with
//! the arguments bound to its own parameters, and its result is checked
//! against the selected signature's result type: an upcast is free, a
//! narrower result is tested at run time like `x as T` (a `TypeError` if the
//! implementation returned a value its overload does not admit).
use super::*;
use std::rc::Rc;
use swc_common::Span;

/// A body-less declaration of an overloaded name.
type Declaration = (ast::Ident, Box<ast::Function>);

/// One overload signature.
#[derive(Clone)]
pub(crate) struct Overload {
    pub params: Vec<Type>,
    pub shape: ir::ParameterShape,
    pub result: Type,
    /// A generic overload signature (v38): `params`, `shape` and `result`
    /// are its instantiation with every type parameter `unknown`; a call
    /// instantiates it for the argument types (`generic_calls::
    /// instantiate_overload`).
    pub(super) generic: Option<Rc<generics::Function>>,
    /// The type arguments of the generic class instance that declares a
    /// generic method overload (empty otherwise).
    pub(super) outer: generics::Frame,
}

#[derive(Default)]
pub(crate) struct Overloads {
    /// By the function index of the implementation.
    functions: BTreeMap<usize, Rc<[Overload]>>,
    /// By declaring class, member key and whether the member is static; the
    /// constructor is the member named `constructor`.
    methods: BTreeMap<(u32, String, bool), Rc<[Overload]>>,
}
impl Overloads {
    pub(super) fn function(&self, index: usize) -> Option<Rc<[Overload]>> {
        self.functions.get(&index).cloned()
    }
    pub(super) fn method(&self, class: u32, name: &str, is_static: bool) -> Option<Rc<[Overload]>> {
        self.methods
            .get(&(class, name.to_owned(), is_static))
            .cloned()
    }
    pub(super) fn has_function(&self, index: usize) -> bool {
        self.functions.contains_key(&index)
    }
}

/// The overload signatures of a body-less declaration list: every
/// signature needs a return annotation and must be compatible with the
/// implementation (TS2394).
fn signatures(
    declarations: &[Declaration],
    implementation: &Signature,
    (source, functions, outer): (&Source, bool, &generics::Frame),
    types: &mut ir::ValueTypes,
) -> Result<Rc<[Overload]>> {
    let mut overloads = Vec::new();
    for (name, function) in declarations {
        // V38: a generic overload signature of a module function or a method
        // is instantiated per call.
        let generic = function.type_params.as_ref().map(|params| {
            Rc::new(generics::Function {
                module: named::current(types),
                kind: if functions { "function" } else { "method" },
                name: name.sym.to_string(),
                declaration: ast::FnDecl {
                    ident: name.clone(),
                    declare: false,
                    function: function.clone(),
                },
                params: (**params).clone(),
                source: source.clone(),
            })
        });
        if function.return_type.is_none() {
            return Err(source.locate(
                type_error(format!(
                    "overload signature of {} requires a return type annotation",
                    crate::desugar::display_name(name.sym.as_ref())
                )),
                name.span,
            ));
        }
        let mut function = function.clone();
        function.body = Some(ast::FunctionBody {
            span: function.span,
            stmts: Vec::new(),
        });
        let declaration = ast::FnDecl {
            ident: name.clone(),
            declare: false,
            function,
        };
        let signature = match &generic {
            Some(generic) => generic_calls::unknown_instance(generic, outer, types),
            None => signature::check(&declaration, 0, source, types),
        }
        .map_err(|e| source.locate(e, name.span))?;
        // Parameters compare position by position, a rest parameter by its
        // element type.
        let positions = signature.params.len().max(implementation.params.len()) + 1;
        let parameters = (0..positions).all(|index| {
            let overload = arguments::context(index, &signature.params, &signature.shape, types);
            let actual =
                arguments::context(index, &implementation.params, &implementation.shape, types);
            match (overload, actual) {
                (Some(overload), Some(actual)) => {
                    actual == Type::Unknown
                        || arguments::accepts(actual, overload, types)
                        || arguments::accepts(overload, actual, types)
                }
                (Some(_), None) => false,
                _ => true,
            }
        });
        // A generic overload is checked per call, not against the
        // implementation with `unknown` type arguments.
        let compatible = generic.is_some()
            || parameters
                && (implementation.result == Type::Void && signature.result == Type::Void
                || arguments::accepts(implementation.result, signature.result, types)
                || arguments::accepts(signature.result, implementation.result, types)
                || signature.result == Type::Void
                // Every type is assignable to `unknown` (v38).
                || implementation.result == Type::Unknown);
        if !compatible {
            return Err(source.locate(
                type_error(
                    "this overload signature is not compatible with its implementation signature (TS2394)",
                ),
                name.span,
            ));
        }
        overloads.push(Overload {
            params: signature.params,
            shape: signature.shape,
            result: signature.result,
            generic,
            outer: outer.clone(),
        });
    }
    Ok(overloads.into())
}

/// A literal argument or a binding, which lowers without effects in any
/// context (a string enum member is `'x' as E`).
fn literal(expr: &ast::Expr) -> bool {
    match expr {
        ast::Expr::Lit(ast::Lit::Num(_) | ast::Lit::Str(_) | ast::Lit::Bool(_))
        | ast::Expr::Ident(_) => true,
        ast::Expr::Unary(unary) if unary.op == ast::UnaryOp::Minus => {
            matches!(&*unary.arg, ast::Expr::Lit(ast::Lit::Num(_)))
        }
        ast::Expr::Paren(paren) => literal(&paren.expr),
        ast::Expr::TsAs(assertion) => literal(&assertion.expr),
        _ => false,
    }
}

/// Record the overloads of the module's functions, after their signatures.
pub(super) fn declare_functions(
    module: &ast::Module,
    symbols: &link::Symbols,
    source: &Source,
    types: &mut ir::ValueTypes,
) -> Result<()> {
    if !types.profile.declaration_forms() {
        return Ok(());
    }
    let mut groups: BTreeMap<String, Vec<Declaration>> = BTreeMap::new();
    for item in &module.body {
        let ast::ModuleItem::Stmt(ast::Stmt::Decl(ast::Decl::Fn(function))) = item else {
            continue;
        };
        if function.declare && function.function.body.is_none() {
            groups
                .entry(function.ident.sym.to_string())
                .or_default()
                .push((function.ident.clone(), function.function.clone()));
        }
    }
    for (name, declarations) in groups {
        let Some(implementation) = symbols.get(&name) else {
            let (ident, _) = &declarations[0];
            return Err(source.locate(
                type_error(format!(
                    "function implementation of {} is missing or generic (TS2391)",
                    crate::desugar::display_name(&name)
                )),
                ident.span,
            ));
        };
        let overloads = signatures(
            &declarations,
            implementation,
            (source, true, &generics::Frame::new()),
            types,
        )?;
        types
            .overloads
            .functions
            .insert(implementation.index, overloads);
    }
    Ok(())
}

/// Record the overloads of a class's methods and constructor.
pub(super) fn declare_methods(
    class: u32,
    declarations: &[classes::OverloadDeclaration],
    source: &Source,
    types: &mut ir::ValueTypes,
) -> Result<()> {
    let mut groups: BTreeMap<(String, bool), Vec<Declaration>> = BTreeMap::new();
    for declaration in declarations {
        groups
            .entry((declaration.key.clone(), declaration.is_static))
            .or_default()
            .push((declaration.ident.clone(), declaration.function.clone()));
    }
    for ((key, is_static), declarations) in groups {
        let entry = types.classes.get(class);
        let implementation = if key == classes::CONSTRUCTOR_KEY {
            entry.init.clone()
        } else if is_static {
            entry.statics.get(&key).map(|m| m.signature.clone())
        } else {
            entry.methods.get(&key).map(|m| m.signature.clone())
        };
        let Some(implementation) = implementation else {
            return Err(source.locate(
                type_error(format!("implementation of {key} is missing (TS2391)")),
                declarations[0].0.span,
            ));
        };
        let outer = types.classes.get(class).frame.clone();
        let overloads = signatures(
            &declarations,
            &implementation,
            (source, false, &outer),
            types,
        )?;
        types
            .overloads
            .methods
            .insert((class, key, is_static), overloads);
    }
    Ok(())
}

impl Lower<'_> {
    /// Lower the arguments of a call to an overloaded implementation, select
    /// the overload and bind the arguments to the implementation. Returns
    /// the bound arguments and the selected overload's result type.
    pub(super) fn overloaded_arguments(
        &mut self,
        call: &ast::CallExpr,
        overloads: &[Overload],
        implementation: &Signature,
    ) -> Result<(Vec<Expr>, Type)> {
        self.overloaded_values(&call.args, call.span, overloads, implementation)
    }
    pub(super) fn overloaded_values(
        &mut self,
        args: &[ast::ExprOrSpread],
        span: Span,
        overloads: &[Overload],
        implementation: &Signature,
    ) -> Result<(Vec<Expr>, Type)> {
        if args.iter().any(|arg| arg.spread.is_some()) {
            return Err(unsupported("spread argument to an overloaded function"));
        }
        // An argument whose parameter type all overloads agree on is typed
        // in that context; a function argument takes the implementation's.
        let mut values = Vec::new();
        for (index, arg) in args.iter().enumerate() {
            // A generic overload (v38) gives no context: it is instantiated
            // from the argument types.
            let contexts: Vec<Option<Type>> = overloads
                .iter()
                .map(|o| {
                    o.generic
                        .is_none()
                        .then(|| arguments::context(index, &o.params, &o.shape, self.types))
                        .flatten()
                })
                .collect();
            let expected = match contexts.first().copied().flatten() {
                Some(first) if contexts.iter().all(|c| *c == Some(first)) => Some(first),
                _ => arguments::context(
                    index,
                    &implementation.params,
                    &implementation.shape,
                    self.types,
                )
                .filter(|ty| matches!(ty, Type::Function(_))),
            };
            // A generic overload infers literal types, as tsc does for a
            // type parameter constrained to a primitive.
            values.push(match expected {
                None if overloads.iter().any(|o| o.generic.is_some()) => {
                    self.expr_preserving(&arg.expr)?
                }
                _ => self.expr_expected(&arg.expr, expected)?,
            });
        }
        // A literal argument (an enum member lowers to one) has its literal
        // type in the context of a literal parameter type, as in TypeScript.
        let mut selected = None;
        for overload in overloads {
            // A generic overload (v38) is instantiated for the argument types.
            let instance;
            let overload = match &overload.generic {
                Some(generic) => {
                    let found: Vec<Type> = values.iter().map(|value| value.ty).collect();
                    match generic_calls::instantiate_overload(
                        generic,
                        &overload.outer,
                        &found,
                        self.types,
                    )? {
                        Some(instantiated) => {
                            instance = instantiated;
                            &instance
                        }
                        None => continue,
                    }
                }
                None => overload,
            };
            if arguments::count(values.len(), &overload.shape).is_err() {
                continue;
            }
            let mut typed = Vec::new();
            let mut matches = true;
            for (index, (value, arg)) in values.iter().zip(args).enumerate() {
                let Some(param) =
                    arguments::context(index, &overload.params, &overload.shape, self.types)
                else {
                    matches = false;
                    break;
                };
                if arguments::accepts(param, value.ty, self.types) {
                    continue;
                }
                let literal = literal(&arg.expr)
                    .then(|| self.expr_expected(&arg.expr, Some(param)).ok())
                    .flatten()
                    .filter(|value| arguments::accepts(param, value.ty, self.types));
                match literal {
                    Some(literal) => typed.push((index, literal)),
                    None => {
                        matches = false;
                        break;
                    }
                }
            }
            if matches {
                for (index, value) in typed {
                    values[index] = value;
                }
                selected = Some(overload.result);
                break;
            }
        }
        let Some(result) = selected else {
            let found: Vec<String> = values.iter().map(|v| v.ty.to_string()).collect();
            return Err(self.source.locate(
                type_error(format!(
                    "no overload matches this call with arguments ({}) (TS2769)",
                    found.join(", ")
                )),
                span,
            ));
        };
        let args =
            self.bind_arguments(values, &implementation.params, &implementation.shape, span)?;
        Ok((args, result))
    }
    /// `f(...)` of an overloaded module function.
    pub(super) fn overloaded_function_call(
        &mut self,
        call: &ast::CallExpr,
        signature: &Signature,
        overloads: &[Overload],
    ) -> Result<Expr> {
        if signature.receiver.is_some() {
            return Err(unsupported("overloaded function with a this parameter"));
        }
        let (args, result) = self.overloaded_arguments(call, overloads, signature)?;
        let value = Expr {
            span: call.span,
            ty: signature.result,
            kind: E::Call(signature.index, args),
        };
        self.overloaded_result(value, result)
    }
    /// Check the implementation's result against the selected overload.
    pub(super) fn overloaded_result(&mut self, value: Expr, result: Type) -> Result<Expr> {
        if value.ty == result || result == Type::Void {
            return Ok(value);
        }
        let span = value.span;
        let name = self.mismatch(result, value.ty).0;
        let message = format!("overload implementation returned a value that is not {name}");
        self.assert_value(value, result, &name, &message, span)
    }
}
