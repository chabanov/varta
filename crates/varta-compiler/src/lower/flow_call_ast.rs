//! Borrowed AST effects specialize stable callback inputs before repeated code.
use super::*;
use flow_calls::{Impact, callbacks};
use std::collections::BTreeSet;
impl Lower<'_> {
    fn ast_callback(&self, expression: &ast::Expr, depth: usize, expand_literals: bool) -> Impact {
        if depth >= 64 {
            return Impact::unknown();
        }
        match stmt::unparen(expression) {
            ast::Expr::Ident(id) => {
                if let Ok(binding) = self.binding(id.sym.as_ref()) {
                    if let Some(&origin) = self.parameter_values.get(&binding.identity) {
                        return Impact {
                            parameters: BTreeSet::from([origin]),
                            ..Impact::default()
                        };
                    }
                    if !binding.mutable
                        && let Some(proof) = self.known_calls.get(&binding.identity)
                    {
                        return self.targets(proof, &[], true, depth + 1);
                    }
                    if binding.ty.is_some_and(|ty| !callbacks(ty, self.types, 0)) {
                        return Impact::default();
                    }
                } else if let Some(signature) = self.functions.get(id.sym.as_ref()) {
                    return self.targets(&BTreeSet::from([signature.index]), &[], true, depth + 1);
                }
                Impact::unknown()
            }
            // The Effects visitor already visits child bodies with its full
            // lexical shadow stack. Expand only for a normal-call fallback.
            ast::Expr::Arrow(_) | ast::Expr::Fn(_) => {
                if expand_literals {
                    self.flow_literal_impact(expression, depth + 1)
                } else {
                    Impact::default()
                }
            }
            ast::Expr::Cond(branch) => {
                let mut impact = self.ast_callback(&branch.cons, depth + 1, expand_literals);
                impact.extend(self.ast_callback(&branch.alt, depth + 1, expand_literals));
                impact
            }
            _ => Impact::unknown(),
        }
    }
    fn ast_argument(
        &self,
        call: &ast::CallExpr,
        position: u32,
        receiver: bool,
        method: bool,
        depth: usize,
        expand_literals: bool,
    ) -> Impact {
        if call.args.iter().any(|arg| arg.spread.is_some()) {
            return Impact::unknown();
        }
        let index = if receiver {
            if position == 0 {
                if !method {
                    return Impact::unknown();
                }
                0
            } else {
                position as usize - 1 + usize::from(method)
            }
        } else {
            position as usize + usize::from(method)
        };
        call.args.get(index).map_or_else(Impact::unknown, |arg| {
            self.ast_callback(&arg.expr, depth + 1, expand_literals)
        })
    }
    fn ast_targets(
        &self,
        proof: &BTreeSet<usize>,
        call: &ast::CallExpr,
        receiver: bool,
        method: bool,
        depth: usize,
        expand_literals: bool,
    ) -> Impact {
        let mut impact = Impact::default();
        for &index in proof {
            if self.module_function(index) {
                if let Some(signature) = self.functions.values().find(|s| s.index == index) {
                    for (position, ty) in signature
                        .receiver
                        .into_iter()
                        .chain(signature.params.iter().copied())
                        .enumerate()
                    {
                        if callbacks(ty, self.types, 0) {
                            impact.extend(self.ast_argument(
                                call,
                                position as u32,
                                receiver,
                                method,
                                depth + 1,
                                expand_literals,
                            ));
                        }
                    }
                }
            } else if let Some(summary) = self.call_effects.get(&index) {
                impact.writes.extend(summary.writes.iter().copied());
                impact.unknown |= summary.unknown;
                for &(owner, position) in &summary.parameters {
                    if owner == index {
                        impact.extend(self.ast_argument(
                            call,
                            position,
                            receiver,
                            method,
                            depth + 1,
                            expand_literals,
                        ));
                    } else {
                        impact.parameters.insert((owner, position));
                    }
                }
            } else {
                impact.unknown = true;
            }
        }
        impact
    }
    pub(super) fn flow_ast_call(
        &self,
        call: &ast::CallExpr,
        depth: usize,
        expand_literals: bool,
    ) -> Impact {
        if depth >= 64 {
            return Impact::unknown();
        }
        if super::flow::native(call, self) {
            return Impact::default();
        }
        let ast::Callee::Expr(callee) = &call.callee else {
            return Impact::unknown();
        };
        let (callee, method) = match stmt::unparen(callee) {
            ast::Expr::Member(member) if matches!(&member.prop, ast::MemberProp::Ident(id) if id.sym == "call") => {
                (stmt::unparen(&member.obj), true)
            }
            callee => (callee, false),
        };
        if matches!(callee, ast::Expr::Arrow(_) | ast::Expr::Fn(_)) {
            return if expand_literals {
                self.flow_literal_impact(callee, depth + 1)
            } else {
                Impact::default()
            };
        }
        let ast::Expr::Ident(id) = callee else {
            return Impact::unknown();
        };
        if let Ok(binding) = self.binding(id.sym.as_ref()) {
            if let Some(&origin) = self.parameter_values.get(&binding.identity) {
                return Impact {
                    parameters: BTreeSet::from([origin]),
                    ..Impact::default()
                };
            }
            if !binding.mutable
                && let Some(proof) = self.known_calls.get(&binding.identity)
            {
                let receiver = match binding.ty {
                    Some(Type::Function(id)) => {
                        self.types.functions[id as usize].receiver.is_some()
                    }
                    _ => false,
                };
                return self.ast_targets(proof, call, receiver, method, depth + 1, expand_literals);
            }
            return Impact::unknown();
        }
        if let Some(signature) = self.functions.get(id.sym.as_ref()) {
            return self.ast_targets(
                &BTreeSet::from([signature.index]),
                call,
                signature.receiver.is_some(),
                method,
                depth + 1,
                expand_literals,
            );
        }
        Impact::unknown()
    }
}
