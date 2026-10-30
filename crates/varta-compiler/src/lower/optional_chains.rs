//! Optional chains (v36 `function_fields`): `x?.p`, `x?.m(args)`, `f?.(args)`
//! and longer chains such as `x?.a.b` or `x?.a?.b`. The object of the first
//! optional link is evaluated once into a hidden binding; when it is
//! `undefined` or `null` the rest of the chain (including its arguments) is not
//! evaluated and the result is `undefined`. Otherwise the rest of the chain is
//! lowered as an ordinary expression over the binding narrowed to its
//! non-nullish members, so member reads, method and field calls keep their
//! existing rules. Parentheses end a chain, as in JavaScript.
use super::*;

/// Prefix of the hidden binding; `#` cannot start a source identifier.
const CHAIN_BINDING: &str = "#chain";

/// The object of a chain link: the member base or the called value.
fn link_object(chain: &ast::OptChainExpr) -> &ast::Expr {
    match &*chain.base {
        ast::OptChainBase::Member(member) => &member.obj,
        ast::OptChainBase::Call(call) => &call.callee,
    }
}

/// The object of the first optional link evaluated in `expression`.
fn first_optional(expression: &ast::Expr) -> Option<&ast::Expr> {
    match expression {
        ast::Expr::OptChain(chain) => {
            let object = link_object(chain);
            first_optional(object).or(chain.optional.then_some(object))
        }
        ast::Expr::Member(member) => first_optional(&member.obj),
        _ => None,
    }
}

/// `expression` with the first optional link's object `target` replaced by
/// `binding`; that link becomes an ordinary member or call, links after it
/// keep their own optionality.
fn rewrite(expression: &ast::Expr, target: &ast::Expr, binding: &ast::Ident) -> ast::Expr {
    match expression {
        ast::Expr::OptChain(chain) => {
            let object = link_object(chain);
            let at_target = std::ptr::eq(object, target);
            let object = Box::new(if at_target {
                ast::Expr::Ident(binding.clone())
            } else {
                rewrite(object, target, binding)
            });
            if chain.optional && !at_target {
                let base = match &*chain.base {
                    ast::OptChainBase::Member(member) => {
                        ast::OptChainBase::Member(ast::MemberExpr {
                            obj: object,
                            ..member.clone()
                        })
                    }
                    ast::OptChainBase::Call(call) => ast::OptChainBase::Call(ast::OptCall {
                        callee: object,
                        ..call.clone()
                    }),
                };
                return ast::Expr::OptChain(ast::OptChainExpr {
                    base: Box::new(base),
                    ..chain.clone()
                });
            }
            match &*chain.base {
                ast::OptChainBase::Member(member) => ast::Expr::Member(ast::MemberExpr {
                    obj: object,
                    ..member.clone()
                }),
                ast::OptChainBase::Call(call) => ast::Expr::Call(ast::CallExpr {
                    span: call.span,
                    ctxt: call.ctxt,
                    callee: ast::Callee::Expr(object),
                    args: call.args.clone(),
                    type_args: call.type_args.clone(),
                }),
            }
        }
        ast::Expr::Member(member) => ast::Expr::Member(ast::MemberExpr {
            obj: Box::new(rewrite(&member.obj, target, binding)),
            ..member.clone()
        }),
        other => other.clone(),
    }
}

/// The identifier whose optional link starts the chain `test` (`x?.p`,
/// `x?.m()`): a truthy chain proves it is neither `undefined` nor `null`.
pub(super) fn chained_binding(test: &ast::Expr) -> Option<String> {
    let ast::Expr::OptChain(_) = test else {
        return None;
    };
    match first_optional(test)? {
        ast::Expr::Ident(id) => Some(id.sym.to_string()),
        _ => None,
    }
}

impl Lower<'_> {
    pub(super) fn optional_chain(&mut self, chain: &ast::OptChainExpr) -> Result<Expr> {
        // `obj.m?.(args)` on a class instance (v38): a method always exists,
        // so the optional call is the ordinary call.
        if self.types.profile.structural_subtyping()
            && chain.optional
            && let ast::OptChainBase::Call(call) = &*chain.base
            && let ast::Expr::Member(member) = stmt::unparen(&call.callee)
            && let ast::MemberProp::Ident(name) = &member.prop
            && first_optional(&member.obj).is_none()
            && let Some(class) = self.class_receiver(&member.obj, 0)
            && self.types.classes.field(class, name.sym.as_ref()).is_none()
            && self
                .types
                .classes
                .get(class)
                .methods
                .get(name.sym.as_ref())
                .is_some_and(|method| !method.getter)
        {
            let call = ast::Expr::Call(ast::CallExpr {
                span: call.span,
                ctxt: call.ctxt,
                callee: ast::Callee::Expr(call.callee.clone()),
                args: call.args.clone(),
                type_args: call.type_args.clone(),
            });
            return self.expr(&call);
        }
        let expression = ast::Expr::OptChain(chain.clone());
        let Some(target) = first_optional(&expression) else {
            return Err(invalid("optional chain without an optional link"));
        };
        let span = chain.span;
        let value = self.expr(target)?;
        if value.ty.result_value().is_some() || value.ty == Type::Void {
            return Err(self.source.locate(
                type_error("optional chain on an opaque result pair or void value"),
                value.span,
            ));
        }
        let present_members: Vec<_> = unions::members(value.ty, self.types)
            .into_iter()
            .filter(|ty| !matches!(ty, Type::Null | Type::Undefined))
            .collect();
        if present_members.is_empty() {
            return Err(self.source.locate(
                type_error("optional chain on a value that is always undefined or null"),
                value.span,
            ));
        }
        let present_ty = if present_members.len() == 1 {
            present_members[0]
        } else {
            unions::intern(present_members, self.types)?
        };
        let ty = value.ty;
        let name = format!("{CHAIN_BINDING}{}", self.next);
        self.scopes.push(BTreeMap::new());
        let declared = self.declare(&name, Some(ty), true, false);
        let lowered = declared.and_then(|()| {
            let slot = self.binding(&name)?.slot;
            let binding = ast::Ident {
                span: value.span,
                ctxt: Default::default(),
                sym: name.as_str().into(),
                optional: false,
            };
            let rest = rewrite(&expression, target, &binding);
            let previous = self.refinements.insert(slot, present_ty);
            let present = self.expr(&rest);
            match previous {
                Some(previous) => self.refinements.insert(slot, previous),
                None => self.refinements.remove(&slot),
            };
            Ok((slot, present?))
        });
        self.scopes.pop();
        let (slot, present) = lowered?;
        let present = if present.ty == Type::Void {
            Expr {
                span,
                ty: Type::Undefined,
                kind: E::Undefined(Box::new(present)),
            }
        } else {
            present
        };
        let local = Expr {
            span,
            ty,
            kind: E::Local(slot),
        };
        let test = Expr {
            span,
            ty: Type::Boolean,
            kind: E::IsNullish(Box::new(local)),
        };
        let undefined = Expr {
            span,
            ty: Type::Undefined,
            kind: E::Integer(0),
        };
        // A value without nullish members always continues the chain.
        let (result, selected) = if present_ty == ty {
            (present.ty, present)
        } else {
            let (result, absent, present) = if present.ty == Type::Undefined {
                (Type::Undefined, undefined, present)
            } else {
                self.join(undefined, present)?
            };
            (
                result,
                Expr {
                    span,
                    ty: result,
                    kind: E::Conditional(Box::new(test), Box::new(absent), Box::new(present)),
                },
            )
        };
        let output = self.hidden(result);
        let marker = self.hidden(Type::Number);
        Ok(Expr {
            span,
            ty: result,
            kind: E::ValueBlock {
                output,
                marker,
                roots: vec![slot, output],
                body: vec![S::Init(slot, value), S::Init(output, selected)],
            },
        })
    }
}
