//! Type predicates `x is T`, assertion signatures `asserts x is T` and
//! `asserts x` (TypeScript Handbook, Using Type Predicates; Assertion
//! Functions). A predicate function returns `boolean`, an assertion
//! function `void`; a call guarding a branch (or, for an assertion, the
//! call statement) narrows the argument binding.
//!
//! TypeScript trusts a predicate's body. Varta's narrowed reads must stay
//! memory-safe even when a predicate lies, so a binding narrowed by a
//! predicate is read through a run-time member test (`TypeError` when it
//! holds another member) whenever the narrowed type is not already checked
//! by its value tag. Narrowing a non-union class instance to a subclass or
//! an `unknown` value to an object type has no checked read and is not
//! applied (the binding keeps its type).
use super::*;

/// The `TypeError` message of a read whose predicate lied (an intentional
/// deviation from Node, which continues with the mistyped value).
const PREDICATE_MISMATCH: &str = "value does not have the type its type predicate asserted";

#[derive(Clone)]
pub(super) struct Predicate {
    /// The guarded parameter position; `None` for `this`.
    parameter: Option<usize>,
    /// The asserted type; `None` for `asserts x` (truthiness).
    ty: Option<Type>,
    asserts: bool,
}

/// The result type of a predicate signature.
pub(super) fn result(predicate: &ast::TsTypePredicate, types: &mut ir::ValueTypes) -> Result<Type> {
    if let Some(annotation) = &predicate.type_ann {
        keyword_type(&annotation.type_ann, types)?;
    }
    Ok(if predicate.asserts {
        Type::Void
    } else {
        Type::Boolean
    })
}

/// Record the predicate of `declaration` at its function index.
pub(super) fn note(
    declaration: &ast::FnDecl,
    signature: &Signature,
    types: &mut ir::ValueTypes,
) -> Result<()> {
    if !types.profile.type_level() {
        return Ok(());
    }
    let Some(annotation) = &declaration.function.return_type else {
        return Ok(());
    };
    let ast::TsType::TsTypePredicate(predicate) = &*annotation.type_ann else {
        return Ok(());
    };
    let parameter = match &predicate.param_name {
        ast::TsThisTypeOrIdent::TsThisType(_) => None,
        ast::TsThisTypeOrIdent::Ident(id) => Some(
            signature
                .names
                .iter()
                .position(|name| name.as_deref() == Some(id.sym.as_ref()))
                .ok_or_else(|| {
                    type_error(format!(
                        "cannot find parameter '{}' of the type predicate",
                        id.sym
                    ))
                })?,
        ),
    };
    let ty = match &predicate.type_ann {
        Some(annotation) => Some(keyword_type(&annotation.type_ann, types)?),
        None => None,
    };
    // Class hierarchies are linked after signatures: a class predicate's
    // relation to its parameter is checked by the narrowing instead.
    if let (Some(at), Some(ty)) = (parameter, ty)
        && !involves_class(ty, types)
        && !involves_class(signature.params[at], types)
        && !extends(ty, signature.params[at], types)
        && !extends(signature.params[at], ty, types)
        && !unions::members(signature.params[at], types)
            .iter()
            .any(|member| extends(*member, ty, types) || extends(ty, *member, types))
    {
        return Err(type_error(format!(
            "a type predicate's type must be assignable to its parameter's type: {} is not assignable to {}",
            subtyping::describe(ty, types),
            subtyping::describe(signature.params[at], types)
        )));
    }
    types.type_level.predicates.insert(
        signature.index,
        Predicate {
            parameter,
            ty,
            asserts: predicate.asserts,
        },
    );
    Ok(())
}

fn involves_class(ty: Type, types: &ir::ValueTypes) -> bool {
    unions::members(ty, types)
        .iter()
        .any(|member| matches!(member, Type::Class(_)))
}

/// Record a `this is T` method predicate (`isDog(): this is Dog`) or an
/// `asserts this [is T]` assertion method.
pub(super) fn note_method(
    (class, name): (u32, &str),
    function: &ast::Function,
    types: &mut ir::ValueTypes,
) -> Result<()> {
    let Some(annotation) = &function.return_type else {
        return Ok(());
    };
    let ast::TsType::TsTypePredicate(predicate) = &*annotation.type_ann else {
        return Ok(());
    };
    if !matches!(predicate.param_name, ast::TsThisTypeOrIdent::TsThisType(_)) {
        return Ok(());
    }
    let ty = match &predicate.type_ann {
        Some(target) => Some(keyword_type(&target.type_ann, types)?),
        // `this is` always has a type; `asserts this` asserts truthiness,
        // which a receiver always has.
        None => None,
    };
    types.type_level.this_predicates.insert(
        (class, name.to_owned()),
        Predicate {
            parameter: None,
            ty,
            asserts: predicate.asserts,
        },
    );
    Ok(())
}

/// What a predicate keeps of `current` (`positive`) or excludes from it.
fn narrowed(
    current: Type,
    target: Type,
    positive: bool,
    types: &mut ir::ValueTypes,
) -> Option<Vec<Type>> {
    if current == Type::Unknown {
        let primitive = matches!(
            target,
            Type::String
                | Type::Float64
                | Type::Number
                | Type::Boolean
                | Type::BigInt
                | Type::Null
                | Type::Undefined
        );
        if primitive {
            return unknown::refine(references::Guard::Exact(target), positive, types);
        }
        // An object type: reads assert it (`predicate_read`).
        return positive.then(|| vec![target]);
    }
    // A class instance narrowed to a subclass: reads test the descriptor.
    if let (Type::Class(own), Type::Class(sub)) = (current, target)
        && own != sub
        && types.classes.descends(sub, own)
    {
        return positive.then(|| vec![target]);
    }
    let members = unions::members(current, types);
    if members.len() <= 1 {
        // A non-union keeps its type, or is excluded entirely.
        return match (extends(current, target, types), positive) {
            (true, false) => Some(Vec::new()),
            _ => None,
        };
    }
    let mut kept = Vec::new();
    for member in members {
        if extends(member, target, types) {
            if positive {
                kept.push(member);
            }
        } else if !positive {
            kept.push(member);
        } else if extends(target, member, types) && unions::members(target, types).len() == 1 {
            // `x: string | number` narrowed by `x is 'a'`.
            kept.push(target);
        }
    }
    Some(kept)
}

impl Lower<'_> {
    /// The predicate of a call expression, with its argument expressions.
    fn call_predicate<'a>(&self, test: &'a ast::Expr) -> Option<(Predicate, &'a ast::CallExpr)> {
        let ast::Expr::Call(call) = stmt::unparen(test) else {
            return None;
        };
        let ast::Callee::Expr(callee) = &call.callee else {
            return None;
        };
        // `x.isDog()` of a `this is Dog` method narrows `x`.
        if let ast::Expr::Member(member) = stmt::unparen(callee)
            && let ast::MemberProp::Ident(method) = &member.prop
            && let Some(class) = self.class_receiver(&member.obj, 0)
            && let Some(found) = self
                .types
                .classes
                .get(class)
                .methods
                .get(method.sym.as_ref())
            && let Some(predicate) = self
                .types
                .type_level
                .this_predicates
                .get(&(found.owner, method.sym.to_string()))
        {
            return Some((predicate.clone(), call));
        }
        let index = match stmt::unparen(callee) {
            ast::Expr::Ident(id) if self.binding(id.sym.as_ref()).is_err() => {
                match self.functions.get(id.sym.as_ref()) {
                    Some(signature) => Some(signature.index),
                    None => self.generic_call_index(call),
                }
            }
            _ => None,
        }?;
        let predicate = self.types.type_level.predicates.get(&index)?.clone();
        Some((predicate, call))
    }

    fn generic_call_index(&self, call: &ast::CallExpr) -> Option<usize> {
        let module = named::current(self.types);
        self.types
            .type_level
            .calls
            .get(&(module, call.span.lo.0, call.span.hi.0))
            .copied()
    }

    /// Narrow the guarded argument of a predicate call used as a condition.
    pub(in crate::lower) fn predicate_guard(
        &mut self,
        test: &ast::Expr,
        yes: bool,
    ) -> Result<bool> {
        if !self.types.profile.type_level() {
            return Ok(false);
        }
        let Some((predicate, call)) = self.call_predicate(test) else {
            return Ok(false);
        };
        if predicate.asserts {
            return Ok(false);
        }
        self.apply_predicate(&predicate, call, yes)?;
        Ok(true)
    }

    /// After an assertion call statement, narrow its asserted argument.
    pub(in crate::lower) fn assertion_effect(&mut self, statement: &ast::Expr) -> Result<()> {
        if !self.types.profile.type_level() {
            return Ok(());
        }
        let Some((predicate, call)) = self.call_predicate(statement) else {
            return Ok(());
        };
        if predicate.asserts {
            self.apply_predicate(&predicate, call, true)?;
        }
        Ok(())
    }

    fn apply_predicate(
        &mut self,
        predicate: &Predicate,
        call: &ast::CallExpr,
        yes: bool,
    ) -> Result<()> {
        let guarded = match predicate.parameter {
            Some(at) => match call.args.get(at) {
                Some(argument) if argument.spread.is_none() => &*argument.expr,
                _ => return Ok(()),
            },
            // `this is T`: the receiver of the method call.
            None => match &call.callee {
                ast::Callee::Expr(callee) => match stmt::unparen(callee) {
                    ast::Expr::Member(member) => &*member.obj,
                    _ => return Ok(()),
                },
                _ => return Ok(()),
            },
        };
        let ast::Expr::Ident(id) = stmt::unparen(guarded) else {
            return Ok(());
        };
        let name = id.sym.to_string();
        let Some(target) = predicate.ty else {
            // `asserts x`: x is truthy afterwards.
            return self.refine(&name, references::Guard::Truth(yes), true);
        };
        let identity = self
            .binding(&name)
            .or_else(|_| self.resolve_binding(&name))
            .ok()
            .map(|binding| binding.identity);
        self.refine_with(&name, |current, types| {
            narrowed(current, target, yes, types)
        })?;
        if let Some(identity) = identity {
            self.types.type_level.checked.insert(identity);
        }
        Ok(())
    }

    /// Whether reads of the binding `identity` narrowed to `narrow` must be
    /// tested at run time (a predicate narrowed it to a reference member).
    pub(in crate::lower) fn predicate_checked(&self, identity: (usize, u32), narrow: Type) -> bool {
        self.types.profile.type_level()
            && self.types.type_level.checked.contains(&identity)
            && unions::members(narrow, self.types).iter().any(|member| {
                !matches!(
                    member,
                    Type::String
                        | Type::Float64
                        | Type::Number
                        | Type::Boolean
                        | Type::BigInt
                        | Type::Null
                        | Type::Undefined
                        | Type::Literal(..)
                        | Type::Never
                )
            })
    }

    /// A read of a predicate-narrowed binding: its member tested at run time.
    pub(in crate::lower) fn predicate_read(&mut self, local: Expr, narrow: Type) -> Result<Expr> {
        let span = local.span;
        if local.ty == Type::Unknown {
            // An `unknown` value narrowed to an object type: the checked
            // assertion of `x as T`.
            let name = subtyping::describe(narrow, self.types);
            return self.assert_value(local, narrow, &name, PREDICATE_MISMATCH, span);
        }
        if let (Type::Class(_), Type::Class(class)) = (local.ty, narrow) {
            return self.checked_downcast(local, class);
        }
        if !local.ty.is_cell() {
            return Ok(Expr {
                ty: narrow,
                ..local
            });
        }
        self.checked_narrow(local, narrow, PREDICATE_MISMATCH)
    }

    /// A class instance read as its subclass `class`: its descriptor is
    /// tested, and a `TypeError` is thrown when it is another class.
    fn checked_downcast(&mut self, local: Expr, class: u32) -> Result<Expr> {
        let span = local.span;
        let saved = self.hidden(local.ty);
        let held = |ty: Type| Expr {
            span,
            ty,
            kind: E::Local(saved),
        };
        let test = self.class_test(held(local.ty), class, span)?;
        let error = self.array_error(
            varta_common::value::ErrorClass::TypeError,
            PREDICATE_MISMATCH,
            span,
        )?;
        let narrow = Type::Class(class);
        let output = self.hidden(narrow);
        let marker = self.hidden(Type::Number);
        let source = local.ty;
        Ok(Expr {
            span,
            ty: narrow,
            kind: E::ValueBlock {
                output,
                marker,
                roots: vec![saved],
                body: vec![
                    S::Init(saved, local),
                    S::If(
                        Expr {
                            span,
                            ty: Type::Boolean,
                            kind: E::Not(Box::new(test)),
                        },
                        vec![S::Throw(error)],
                        Vec::new(),
                    ),
                    S::Init(
                        output,
                        Expr {
                            span,
                            ty: narrow,
                            kind: E::Reinterpret(Box::new(held(source))),
                        },
                    ),
                ],
            },
        })
    }
}
