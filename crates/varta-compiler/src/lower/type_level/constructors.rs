//! Call and construct signatures (TypeScript Handbook, Call Signatures and
//! Construct Signatures) and constructor types `new (...) => T`,
//! `abstract new (...) => T`.
//!
//! An object type whose only member is one call signature is that function
//! type. A construct signature or constructor type is a function type marked
//! `construct`: its value is the closure that constructs (a class named
//! where a constructor type is expected becomes `(...args) => new C(...args)`),
//! and only `new` invokes it. Overloaded signatures and callable objects with
//! properties have no single static layout and are refused.
use super::*;

/// Whether an object type declares a call or construct signature.
pub(super) fn callable(literal: &ast::TsTypeLit) -> bool {
    literal.members.iter().any(|member| {
        matches!(
            member,
            ast::TsTypeElement::TsCallSignatureDecl(_)
                | ast::TsTypeElement::TsConstructSignatureDecl(_)
        )
    })
}

/// `{ (x: number): number }` and `{ new (x: number): C }`.
pub(super) fn type_literal(literal: &ast::TsTypeLit, types: &mut ir::ValueTypes) -> Result<Type> {
    let [member] = &literal.members[..] else {
        let signatures = literal
            .members
            .iter()
            .filter(|member| {
                matches!(
                    member,
                    ast::TsTypeElement::TsCallSignatureDecl(_)
                        | ast::TsTypeElement::TsConstructSignatureDecl(_)
                )
            })
            .count();
        return Err(unsupported(if signatures > 1 {
            "overloaded call or construct signatures (an overloaded function value has no single static signature)"
        } else {
            "object type with a call or construct signature and other members (a callable object has no closed layout)"
        }));
    };
    match member {
        ast::TsTypeElement::TsCallSignatureDecl(call) => {
            if call.type_params.is_some() {
                return Err(unsupported(
                    "generic call signature (a generic function value has no single function type)",
                ));
            }
            signature(&call.params, call.type_ann.as_deref(), false, types)
        }
        ast::TsTypeElement::TsConstructSignatureDecl(construct) => {
            if construct.type_params.is_some() {
                return Err(unsupported("generic construct signature"));
            }
            signature(
                &construct.params,
                construct.type_ann.as_deref(),
                true,
                types,
            )
        }
        _ => Err(invalid("missing signature member")),
    }
}

/// `new (params) => T` and `abstract new (params) => T`.
pub(super) fn constructor_type(
    constructor: &ast::TsConstructorType,
    types: &mut ir::ValueTypes,
) -> Result<Type> {
    if constructor.type_params.is_some() {
        return Err(unsupported("generic constructor type"));
    }
    signature(
        &constructor.params,
        Some(&constructor.type_ann),
        true,
        types,
    )
}

fn signature(
    params: &[ast::TsFnParam],
    result: Option<&ast::TsTypeAnn>,
    construct: bool,
    types: &mut ir::ValueTypes,
) -> Result<Type> {
    let mut patterns = Vec::new();
    for parameter in params {
        patterns.push(match parameter {
            ast::TsFnParam::Ident(id) if id.id.sym == "this" => {
                return Err(unsupported(
                    "this parameter in a call or construct signature",
                ));
            }
            ast::TsFnParam::Ident(id) => ast::Pat::Ident(id.clone()),
            ast::TsFnParam::Rest(rest) => ast::Pat::Rest(rest.clone()),
            ast::TsFnParam::Array(array) => ast::Pat::Array(array.clone()),
            ast::TsFnParam::Object(object) => ast::Pat::Object(object.clone()),
        });
    }
    let plans = parameters::analyze(&patterns, None, None, types, None)?;
    let shape = parameters::shape(&plans);
    let params = plans.iter().map(|p| p.incoming).collect();
    let result = annotation(result, types)?
        .ok_or_else(|| type_error("a call or construct signature requires a result type"))?;
    if construct && !matches!(result, Type::Class(_) | Type::Record(_)) && result != Type::Never {
        return Err(unsupported(&format!(
            "construct signature producing {} (only class instances are constructed)",
            subtyping::describe(result, types)
        )));
    }
    let call = functions::intern_full(None, params, shape, result, types)?;
    if construct {
        variant(call, true, types)
    } else {
        Ok(call)
    }
}

/// The function type identical to `ty` but for its `construct` mark.
pub(super) fn variant(ty: Type, construct: bool, types: &mut ir::ValueTypes) -> Result<Type> {
    let Type::Function(id) = ty else {
        return Err(invalid("missing function type"));
    };
    let mut signature = types.functions[id as usize].clone();
    if signature.construct == construct {
        return Ok(ty);
    }
    signature.construct = construct;
    if let Some(index) = types.functions.iter().position(|f| *f == signature) {
        return Ok(Type::Function(index as u32));
    }
    if types.functions.len() >= types.profile.limits().function_types {
        return Err(invalid("function type count exceeds limit"));
    }
    types.functions.push(signature);
    Ok(Type::Function(types.functions.len() as u32 - 1))
}

/// The construct type of class `class` (`typeof C`): its constructor's
/// parameters and its instance type.
pub(super) fn class_constructor(class: u32, types: &mut ir::ValueTypes) -> Result<Type> {
    let info = types.classes.get(class);
    if info.is_template() {
        return Err(unsupported(
            "typeof a generic class (a generic class has no single constructor type)",
        ));
    }
    let (params, shape) = match &info.init {
        Some(init) => (init.params.clone(), init.shape.clone()),
        None => {
            let source = info.source.clone();
            let patterns = classes::constructor_patterns(types, class);
            let plans = parameters::analyze(&patterns, None, None, types, Some(&source))?;
            (
                plans.iter().map(|p| p.incoming).collect(),
                parameters::shape(&plans),
            )
        }
    };
    let call = functions::intern_full(None, params, shape, Type::Class(class), types)?;
    variant(call, true, types)
}

impl Lower<'_> {
    /// The construct type `expected` admits for a class named by `id`.
    pub(in crate::lower) fn constructor_context(
        &self,
        id: &ast::Ident,
        expected: Option<Type>,
    ) -> Option<(u32, Type)> {
        if !self.types.profile.type_level() {
            return None;
        }
        let class = self.class_callee(&ast::Expr::Ident(id.clone()))?;
        let construct = unions::members(expected?, self.types).into_iter().find(|member| {
            matches!(member, Type::Function(f) if self.types.functions[*f as usize].construct)
        })?;
        Some((class, construct))
    }

    /// A class `C` as a value of construct type `expected`: the closure
    /// `(a0, a1, ...) => new C(a0, a1, ...)`, typed by the construct type.
    pub(in crate::lower) fn class_constructor_value(
        &mut self,
        id: &ast::Ident,
        (class, expected): (u32, Type),
    ) -> Result<Expr> {
        let Type::Function(function) = expected else {
            return Err(invalid("missing constructor type"));
        };
        let signature = self.types.functions[function as usize].clone();
        if signature.shape.rest {
            return Err(self.source.locate(
                unsupported("class value for a constructor type with a rest parameter"),
                id.span,
            ));
        }
        let span = id.span;
        let name = |index: usize| ast::Ident {
            span,
            ctxt: Default::default(),
            sym: format!("$new{index}").into(),
            optional: false,
        };
        let params = (0..signature.params.len())
            .map(|index| {
                let mut ident = name(index);
                ident.optional = signature.shape.optional[index];
                ast::Pat::Ident(ast::BindingIdent {
                    id: ident,
                    type_ann: None,
                })
            })
            .collect();
        let args = (0..signature.params.len())
            .map(|index| ast::ExprOrSpread {
                spread: None,
                expr: Box::new(ast::Expr::Ident(name(index))),
            })
            .collect();
        let construct = ast::Expr::New(ast::NewExpr {
            span,
            ctxt: Default::default(),
            callee: Box::new(ast::Expr::Ident(id.clone())),
            args: Some(args),
            type_args: None,
        });
        let arrow = ast::ArrowExpr {
            span,
            ctxt: Default::default(),
            params,
            body: Box::new(ast::ArrowFunctionBody::Expr(Box::new(construct))),
            is_async: false,
            is_generator: false,
            type_params: None,
            return_type: None,
        };
        let call = variant(expected, false, self.types)?;
        let value = self.arrow(&arrow, Some(call))?;
        let value = unions::widen(value, Some(call), self.types)?;
        if value.ty != call {
            return Err(self.source.locate(
                type_error(format!(
                    "class {} is not assignable to the constructor type {}",
                    self.types.classes.get(class).name,
                    void_functions::describe(expected, self.types)
                )),
                span,
            ));
        }
        Ok(Expr {
            span,
            ty: expected,
            kind: E::Reinterpret(Box::new(value)),
        })
    }

    /// Whether `new callee(...)` invokes a value of a constructor type.
    pub(in crate::lower) fn constructs_value(&self, callee: &ast::Expr) -> bool {
        self.types.profile.type_level()
            && self.class_callee(callee).is_none()
            && matches!(self.peek_type(callee, 0), Some(Type::Function(f)) if self.types.functions[f as usize].construct)
    }

    /// `new c(args)` for a value `c` of a constructor type.
    pub(in crate::lower) fn construct_value(&mut self, new: &ast::NewExpr) -> Result<Expr> {
        let callee = self.expr(&new.callee)?;
        let Type::Function(index) = callee.ty else {
            return Err(type_error("constructed value is not of a constructor type"));
        };
        let signature = self.types.functions[index as usize].clone();
        let args = new.args.as_deref().unwrap_or(&[]);
        if args.iter().any(|arg| arg.spread.is_some()) {
            return Err(unsupported("spread argument to a constructor value"));
        }
        let args = self.call_arguments(args, &signature.params, &signature.shape)?;
        let call = variant(callee.ty, false, self.types)?;
        let callee = Expr {
            span: callee.span,
            ty: call,
            kind: E::Reinterpret(Box::new(callee)),
        };
        self.invoke_value(callee, args, new.span)
    }
}

impl Lower<'_> {
    /// The constructor type of a class named as a generic call argument,
    /// so inference sees `typeof C` (v38 `type_level`).
    pub(in crate::lower) fn class_value_hint(
        &mut self,
        argument: &ast::Expr,
    ) -> Result<Option<Type>> {
        if !self.types.profile.type_level() {
            return Ok(None);
        }
        let Some(class) = self.class_callee(argument) else {
            return Ok(None);
        };
        if self.types.classes.get(class).is_template() {
            return Ok(None);
        }
        class_constructor(class, self.types)
            .map(Some)
            .map_err(|e| self.source.locate(e, argument.span()))
    }
}
