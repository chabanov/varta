//! `switch` over strings, literal unions and `typeof` (v37 `language_core`).
//!
//! The discriminant is evaluated once into a hidden binding; each case test
//! becomes the strict equality `discriminant === test`, evaluated in source
//! order only until one matches, exactly as JavaScript selects a clause
//! (the `default` clause is chosen when every test fails, wherever it
//! appears). The selection itself is the existing `Switch` statement over the
//! Boolean `true`, so fallthrough, `break` and labels keep their lowering.
//!
//! Narrowing: when the discriminant is an identifier, a member path of
//! identifiers or `typeof` of one, and every case test is a literal, each
//! clause is entered under the facts of `discriminant === test` (joined with
//! the fallthrough of the previous clause), and later clauses and the end of
//! the switch under the facts of every earlier test being false.
use super::*;
use std::collections::BTreeSet;

/// Name prefix of the hidden discriminant binding; `#` cannot start a source
/// identifier, so it never shadows or captures user code.
const DISCRIMINANT: &str = "#switch";

/// Whether `expr` is stable between the discriminant and a literal test: an
/// identifier, a member path of identifiers, or `typeof` of one.
fn stable(expr: &ast::Expr) -> bool {
    match stmt::unparen(expr) {
        ast::Expr::Ident(_) | ast::Expr::This(_) => true,
        ast::Expr::Member(member) => {
            matches!(member.prop, ast::MemberProp::Ident(_)) && stable(&member.obj)
        }
        ast::Expr::Unary(unary) if unary.op == ast::UnaryOp::TypeOf => stable(&unary.arg),
        _ => false,
    }
}
fn literal(expr: &ast::Expr) -> bool {
    match stmt::unparen(expr) {
        ast::Expr::Lit(
            ast::Lit::Str(_) | ast::Lit::Num(_) | ast::Lit::Bool(_) | ast::Lit::Null(_),
        ) => true,
        ast::Expr::Ident(id) => id.sym == "undefined",
        ast::Expr::Unary(unary) if unary.op == ast::UnaryOp::Minus => {
            matches!(stmt::unparen(&unary.arg), ast::Expr::Lit(ast::Lit::Num(_)))
        }
        _ => false,
    }
}
fn equality(span: swc_common::Span, left: ast::Expr, right: ast::Expr) -> ast::Expr {
    ast::Expr::Bin(ast::BinExpr {
        span,
        op: ast::BinaryOp::EqEqEq,
        left: Box::new(left),
        right: Box::new(right),
    })
}

impl Lower<'_> {
    /// A `switch` statement: the scalar selection of earlier profiles, or the
    /// v37 equality lowering for other discriminants.
    pub(super) fn switch_statement(&mut self, statement: &ast::SwitchStmt) -> Result<S> {
        let value = self.expr(&statement.discriminant)?;
        if self.equality_switch(&value) {
            return self.switch_by_equality(statement, value);
        }
        scalar(value.ty).map_err(|e| self.source.locate(e, value.span))?;
        let discriminant = self.next;
        let selected = self.next + 1;
        self.local_types.insert(discriminant, value.ty);
        self.next += 2;
        let mut cases = Vec::new();
        let mut default = false;
        let saved = self.flow_snapshot();
        let flow = self.types.profile.refinement_writes();
        let mut tests = Vec::new();
        if flow {
            // Selection tests run before any body and may themselves
            // be skipped. No body fact may influence a later test.
            for case in &statement.cases {
                tests.push(case.test.as_deref().map(|e| self.expr(e)).transpose()?);
                self.flow_restore(&saved);
            }
        }
        let direct = self.flow_snapshot();
        let mut previous = None;
        let target = self.flow_target(flow_control::Kind::Switch);
        self.switches += 1;
        for (index, case) in statement.cases.iter().enumerate() {
            let test = if flow {
                let mut paths = vec![direct.clone()];
                paths.extend(previous.take());
                self.flow_join(&direct, &paths)?;
                tests[index].take()
            } else {
                case.test.as_deref().map(|e| self.expr(e)).transpose()?
            };
            if test.as_ref().is_some_and(|e| e.ty != value.ty) {
                return Err(self
                    .source
                    .locate(type_error("switch case type mismatch"), case.span));
            }
            if test.is_none() {
                if default {
                    return Err(self.source.locate(invalid("duplicate default"), case.span));
                }
                default = true;
            }
            if case.cons.iter().any(|s| matches!(s, ast::Stmt::Decl(_))) {
                return Err(unsupported("case declaration without a block"));
            }
            let body = self.block(&case.cons, true)?;
            if flow && self.reachable {
                previous = Some(self.flow_snapshot());
            }
            cases.push((test, body));
        }
        self.switches -= 1;
        // V38: a literal union covered by the clauses is exhaustive.
        let exhausted = !default
            && self.types.profile.structural_subtyping()
            && self.exhausts(statement, value.ty, &direct);
        if exhausted {
            cases.push((None, vec![S::Trap]));
        }
        if flow {
            let mut paths = self.flow_end(target);
            paths.extend(previous);
            if !default && !exhausted {
                paths.push(direct);
            }
            self.flow_join(&saved, &paths)?;
        }
        Ok(S::Switch {
            value,
            discriminant,
            selected,
            cases,
        })
    }
    /// Whether values of `a` and `b` may be strictly equal.
    fn comparable(&self, a: Type, b: Type) -> bool {
        if a == Type::Unknown || b == Type::Unknown {
            return true;
        }
        let erased = |ty: Type| -> Vec<Type> {
            unions::members(ty, self.types)
                .into_iter()
                .map(|member| match member.erased() {
                    Type::Number => Type::Float64,
                    other => other,
                })
                .collect()
        };
        let left = erased(a);
        erased(b).iter().any(|member| left.contains(member))
    }
    /// Whether the clause literals of `statement` cover every value of its
    /// discriminant of type `ty` (v38): its binding narrowed to `never` once
    /// every test failed, or a literal union whose values are all tested.
    fn exhausts(&mut self, statement: &ast::SwitchStmt, ty: Type, rest: &flow::Snapshot) -> bool {
        if let ast::Expr::Ident(id) = stmt::unparen(&statement.discriminant)
            && self
                .binding(id.sym.as_ref())
                .is_ok_and(|binding| rest.facts.get(&binding.slot) == Some(&Type::Never))
        {
            return true;
        }
        let placeholder = ast::Expr::Ident(ast::Ident::new(
            "#case".into(),
            statement.span,
            Default::default(),
        ));
        let mut singles = Vec::new();
        for case in &statement.cases {
            let Some(test) = case.test.as_deref() else {
                return false;
            };
            match literals::guard_operand(&placeholder, test, self.types) {
                Some((_, single)) => singles.push(single),
                None => return false,
            }
        }
        let declared = self.peek_type(&statement.discriminant, 0).unwrap_or(ty);
        literals::exhausted_by(declared, &singles, self.types)
    }
    /// Whether a `switch` over `value` uses the v37 equality lowering.
    pub(super) fn equality_switch(&self, value: &Expr) -> bool {
        self.types.profile.language_core() && scalar(value.ty).is_err()
    }
    pub(super) fn switch_by_equality(
        &mut self,
        statement: &ast::SwitchStmt,
        value: Expr,
    ) -> Result<S> {
        let span = statement.span;
        self.scopes.push(BTreeMap::new());
        let name = format!("{DISCRIMINANT}{}", self.next);
        let ty = value.ty;
        self.declare(&name, Some(ty), true, false)?;
        let slot = self.binding(&name)?.slot;
        let key = ast::Expr::Ident(ast::Ident::new(
            name.as_str().into(),
            span,
            Default::default(),
        ));
        // Clause narrowing needs every test to be a literal: a test with an
        // effect could change the discriminant's operand before a later
        // clause is selected.
        let narrowing = stable(&statement.discriminant)
            && statement
                .cases
                .iter()
                .all(|case| case.test.as_deref().is_none_or(literal));
        let mut default = false;
        let mut tests = Vec::new();
        let mut guards = Vec::new();
        let saved = self.flow_snapshot();
        for case in &statement.cases {
            let Some(test) = case.test.as_deref() else {
                if default {
                    return Err(self.source.locate(invalid("duplicate default"), case.span));
                }
                default = true;
                tests.push(None);
                guards.push(None);
                continue;
            };
            if case.cons.iter().any(|s| matches!(s, ast::Stmt::Decl(_))) {
                return Err(unsupported("case declaration without a block"));
            }
            // As TypeScript, a clause value must be comparable to the
            // discriminant: their (erased) types share a member.
            if let Some(test_ty) = self
                .peek_type(test, 0)
                .or_else(|| parameters::literal(test, self.types))
                && !self.comparable(ty, test_ty)
            {
                return Err(self
                    .source
                    .locate(type_error("switch case type mismatch"), case.span));
            }
            let comparison = equality(test.span(), key.clone(), test.clone());
            let lowered = self
                .condition(&comparison)
                .map_err(|e| self.source.locate(e, test.span()))?;
            self.flow_restore(&saved);
            tests.push(Some(lowered));
            guards.push(Some(if narrowing && literal(test) {
                equality(test.span(), (*statement.discriminant).clone(), test.clone())
            } else {
                comparison
            }));
        }
        // Clause entry facts: its own test true after every earlier test
        // failed; `rest` holds the facts once every test failed.
        let mut entries = Vec::new();
        let mut rest = self.flow_snapshot();
        for guard in &guards {
            let Some(guard) = guard else {
                entries.push(None);
                continue;
            };
            self.flow_alternate(&rest);
            self.guard(guard, true)?;
            entries.push(Some(self.flow_snapshot()));
            self.flow_alternate(&rest);
            self.guard(guard, false)?;
            rest = self.flow_snapshot();
        }
        // V38: a switch whose clauses narrow its discriminant binding to
        // `never` is exhaustive, as in TypeScript; the missing default is
        // unreachable and traps.
        let exhausted = !default
            && narrowing
            && self.types.profile.structural_subtyping()
            && self.exhausts(statement, ty, &rest);
        let target = self.flow_target(flow_control::Kind::Switch);
        self.switches += 1;
        let mut cases = Vec::new();
        let mut previous = None;
        for ((case, test), entry) in statement.cases.iter().zip(tests).zip(entries) {
            let mut paths = vec![entry.unwrap_or_else(|| rest.clone())];
            paths.extend(previous.take());
            self.flow_join(&saved, &paths)?;
            let body = self.block(&case.cons, true)?;
            if self.reachable {
                previous = Some(self.flow_snapshot());
            }
            cases.push((test, body));
        }
        self.switches -= 1;
        if exhausted {
            cases.push((None, vec![S::Trap]));
        }
        let mut paths = self.flow_end(target);
        paths.extend(previous);
        if !default && !exhausted {
            paths.push(rest);
        }
        self.flow_join(&saved, &paths)?;
        let slots: BTreeSet<u32> = self
            .scopes
            .pop()
            .ok_or_else(|| invalid("missing switch scope"))?
            .values()
            .map(|binding| binding.slot)
            .collect();
        let discriminant = self.hidden(Type::Boolean);
        let selected = self.hidden(Type::Number);
        Ok(S::Scope(
            slots.into_iter().collect(),
            vec![
                S::Init(slot, value),
                S::Switch {
                    value: Expr {
                        span,
                        ty: Type::Boolean,
                        kind: E::Integer(1),
                    },
                    discriminant,
                    selected,
                    cases,
                },
            ],
        ))
    }
}
