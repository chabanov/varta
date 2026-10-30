//! Property-path narrowing (v37 `language_core`).
//!
//! A guard on a property path `a.b.c` (an identifier or `this`, then data
//! fields of records and class instances) refines later reads of the same
//! path, as TypeScript does: `if (a.b !== undefined) use(a.b)`,
//! `typeof a.b.c === "string"`, truthiness, `a.b?.c` (which proves `a.b` is
//! not nullish), literal comparisons and `switch` clauses. Facts are flow
//! state: branch joins keep a path only when every reachable path has a fact
//! for it, and keep the union of the narrowed types.
//!
//! Soundness: records are mutable and aliased, so facts are dropped, all of
//! them, by every construct that may write a property or run other code after
//! the guard: any call except the checked intrinsics that never reach user
//! code or properties, `new`, `await`, `yield`, getters, tagged templates,
//! assignments and updates to anything but a plain binding, `delete`, and the
//! entry of loops, `catch` and `finally` blocks. A write to the root binding
//! drops the facts rooted at it. A guard whose test itself contains such a
//! construct creates no path fact. Facts never enter closures. Only data
//! fields are path components: a getter runs code, so its result is never
//! narrowed.
use super::references::Guard;
use super::*;
use swc_ecma_visit::{Visit, VisitWith};

/// A property path: the local slot of its root binding and its field names.
pub(super) type Path = (u32, Vec<String>);

/// The declared (stored) type of a path's value and its narrowed type.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) struct Fact {
    pub(super) declared: Type,
    pub(super) narrowed: Type,
}

/// Whether evaluating `expr` may write a property or run user code.
pub(super) fn impure<N: VisitWith<Scan>>(node: &N) -> bool {
    let mut scan = Scan(false);
    node.visit_with(&mut scan);
    scan.0
}
/// The [`impure`] scan.
pub(super) struct Scan(bool);
impl Visit for Scan {
    fn visit_ts_type(&mut self, _: &ast::TsType) {}
    // Function bodies do not run where they are created.
    fn visit_arrow_expr(&mut self, _: &ast::ArrowExpr) {}
    fn visit_function(&mut self, _: &ast::Function) {}
    fn visit_class(&mut self, _: &ast::Class) {}
    fn visit_call_expr(&mut self, _: &ast::CallExpr) {
        self.0 = true;
    }
    fn visit_opt_call(&mut self, _: &ast::OptCall) {
        self.0 = true;
    }
    fn visit_new_expr(&mut self, _: &ast::NewExpr) {
        self.0 = true;
    }
    // A store into a plain binding only drops the facts rooted at it.
    fn visit_assign_expr(&mut self, node: &ast::AssignExpr) {
        if !matches!(
            node.left,
            ast::AssignTarget::Simple(ast::SimpleAssignTarget::Ident(_))
        ) {
            self.0 = true;
        }
        node.visit_children_with(self);
    }
    fn visit_update_expr(&mut self, node: &ast::UpdateExpr) {
        if !matches!(&*node.arg, ast::Expr::Ident(_)) {
            self.0 = true;
        }
        node.visit_children_with(self);
    }
    fn visit_await_expr(&mut self, _: &ast::AwaitExpr) {
        self.0 = true;
    }
    fn visit_yield_expr(&mut self, _: &ast::YieldExpr) {
        self.0 = true;
    }
    fn visit_tagged_tpl(&mut self, _: &ast::TaggedTpl) {
        self.0 = true;
    }
    fn visit_unary_expr(&mut self, node: &ast::UnaryExpr) {
        if node.op == ast::UnaryOp::Delete {
            self.0 = true;
        }
        node.visit_children_with(self);
    }
}

/// The field names of a member path and its root expression.
fn components(mut expr: &ast::Expr) -> Option<(&ast::Expr, Vec<String>)> {
    let mut fields = Vec::new();
    loop {
        match expr {
            ast::Expr::Paren(paren) => expr = &paren.expr,
            ast::Expr::Member(member) => {
                let name = match &member.prop {
                    ast::MemberProp::Ident(name) => name.sym.to_string(),
                    ast::MemberProp::Computed(key) => match &*key.expr {
                        ast::Expr::Lit(ast::Lit::Str(text)) => text.value.as_str()?.to_owned(),
                        _ => return None,
                    },
                    ast::MemberProp::PrivateName(_) => return None,
                };
                fields.push(name);
                expr = &member.obj;
            }
            ast::Expr::Ident(_) | ast::Expr::This(_) => break,
            _ => return None,
        }
    }
    if fields.is_empty() {
        return None;
    }
    fields.reverse();
    Some((expr, fields))
}

/// An optional member chain (`a?.b.c`, no calls) as the plain member path.
fn plain(expr: &ast::Expr) -> Option<ast::Expr> {
    match expr {
        ast::Expr::Paren(paren) => plain(&paren.expr),
        ast::Expr::OptChain(chain) => match &*chain.base {
            ast::OptChainBase::Member(member) => Some(ast::Expr::Member(ast::MemberExpr {
                span: member.span,
                obj: Box::new(plain(&member.obj)?),
                prop: member.prop.clone(),
            })),
            ast::OptChainBase::Call(_) => None,
        },
        ast::Expr::Member(member) => Some(ast::Expr::Member(ast::MemberExpr {
            span: member.span,
            obj: Box::new(plain(&member.obj)?),
            prop: member.prop.clone(),
        })),
        ast::Expr::Ident(_) | ast::Expr::This(_) => Some(expr.clone()),
        _ => None,
    }
}

impl Lower<'_> {
    /// The path of `expr` with the current type of its root, when the root is
    /// a binding of this function (or one it already captured).
    fn path(&self, expr: &ast::Expr) -> Option<(Path, Type)> {
        if !self.types.profile.language_core() {
            return None;
        }
        let (root, fields) = components(expr)?;
        let name = match root {
            ast::Expr::Ident(id) => id.sym.to_string(),
            _ => "this".to_owned(),
        };
        let binding = self.binding(&name).ok()?;
        if !binding.initialized
            || binding.identity.0 != self.index && !self.captures.contains_key(&binding.identity)
        {
            return None;
        }
        let ty = self
            .refinements
            .get(&binding.slot)
            .copied()
            .or(binding.ty)?;
        Some(((binding.slot, fields), ty))
    }
    /// The type a data field `name` of `ty` stores.
    fn data_field(&self, ty: Type, name: &str) -> Option<Type> {
        match ty {
            Type::Record(id) => self.types.records[id as usize]
                .fields
                .iter()
                // An optional field's type already admits `undefined`.
                .find(|field| field.name == name)
                .map(|field| field.ty),
            Type::Class(id) => self
                .types
                .classes
                .field(id, name)
                .map(|(_, field)| field.ty),
            _ => None,
        }
    }
    /// The declared and current types of `path`, walking data fields through
    /// the current types (facts) of its prefixes.
    fn path_types(&self, path: &Path, root: Type) -> Option<Fact> {
        let mut current = root;
        let mut declared = root;
        for length in 1..=path.1.len() {
            declared = self.data_field(current, &path.1[length - 1])?;
            let prefix = (path.0, path.1[..length].to_vec());
            current = match self.core.paths.get(&prefix) {
                Some(fact) if fact.declared == declared => fact.narrowed,
                _ => declared,
            };
        }
        Some(Fact {
            declared,
            narrowed: current,
        })
    }
    /// Refine `expr` by `matched` when it is a narrowable path. Returns
    /// whether `expr` was a path.
    pub(super) fn refine_path(
        &mut self,
        expr: &ast::Expr,
        matched: Guard,
        positive: bool,
    ) -> Result<bool> {
        let Some((path, root)) = self.path(expr) else {
            return Ok(false);
        };
        if self.core.impure_guard || self.guard_blocked.contains(&path.0) {
            return Ok(true);
        }
        let Some(fact) = self.path_types(&path, root) else {
            return Ok(true);
        };
        let ty = fact.narrowed;
        let remaining = if let Guard::Literal(single) = matched {
            match literals::refine(ty, single, positive, self.types) {
                Some(refined) => vec![refined],
                None => return Ok(true),
            }
        } else if ty.is_union() {
            unions::members(ty, self.types)
                .into_iter()
                .filter(|member| matched.matches(*member) == positive)
                .collect()
        } else {
            return Ok(true);
        };
        if remaining.is_empty() || remaining == unions::members(ty, self.types) {
            return Ok(true);
        }
        let narrowed = unions::intern(remaining, self.types)?;
        if narrowed != fact.declared {
            self.core.paths.insert(
                path,
                Fact {
                    declared: fact.declared,
                    narrowed,
                },
            );
        }
        Ok(true)
    }
    /// Path guards of one test leaf (`a.b`, `typeof a.b === "t"`,
    /// `a.b === null|undefined|literal`, `a.b?.c`). Returns whether handled.
    pub(super) fn path_guard(&mut self, test: &ast::Expr, yes: bool) -> Result<bool> {
        if !self.types.profile.language_core() {
            return Ok(false);
        }
        let test = stmt::unparen(test);
        if let ast::Expr::Member(_) = test {
            return self.refine_path(test, Guard::Truth(yes), true);
        }
        // `a.b?.c` truthy: every object of the chain is not nullish and
        // `a.b.c` is truthy. The root binding is refined by the caller.
        if let ast::Expr::OptChain(_) = test
            && let Some(plain) = plain(test)
        {
            if yes {
                let mut objects = Vec::new();
                let mut current = &plain;
                while let ast::Expr::Member(member) = current {
                    objects.push(&*member.obj);
                    current = &member.obj;
                }
                for object in objects.into_iter().rev() {
                    self.refine_path(object, Guard::Nullish, false)?;
                }
                self.refine_path(&plain, Guard::Truth(true), true)?;
            }
            return Ok(false);
        }
        let ast::Expr::Bin(binary) = test else {
            return Ok(false);
        };
        let positive = match binary.op {
            ast::BinaryOp::EqEqEq => yes,
            ast::BinaryOp::NotEqEq => !yes,
            _ => return Ok(false),
        };
        for (operand, other) in [
            (&*binary.left, &*binary.right),
            (&*binary.right, &*binary.left),
        ] {
            if let Some((guarded, matched)) = self.path_comparison(operand, other) {
                return self.refine_path(guarded, matched, positive);
            }
        }
        Ok(false)
    }
    fn path_comparison<'a>(
        &mut self,
        operand: &'a ast::Expr,
        other: &ast::Expr,
    ) -> Option<(&'a ast::Expr, Guard)> {
        let operand = stmt::unparen(operand);
        let other = stmt::unparen(other);
        if let ast::Expr::Unary(unary) = operand
            && unary.op == ast::UnaryOp::TypeOf
            && components(&unary.arg).is_some()
            && let ast::Expr::Lit(ast::Lit::Str(text)) = other
        {
            let guard = match text.value.as_str()? {
                "bigint" if self.types.profile.bigint_values() => Guard::Exact(Type::BigInt),
                "number" => Guard::Exact(Type::Float64),
                "boolean" => Guard::Exact(Type::Boolean),
                "string" => Guard::Exact(Type::String),
                "undefined" => Guard::Exact(Type::Undefined),
                "object" => Guard::Object,
                "function" if self.types.profile.function_fields() => Guard::Function,
                _ => return None,
            };
            return Some((&unary.arg, guard));
        }
        components(operand)?;
        match other {
            ast::Expr::Lit(ast::Lit::Null(_)) => Some((operand, Guard::Exact(Type::Null))),
            ast::Expr::Ident(id)
                if id.sym == "undefined"
                    && self.binding("undefined").is_err()
                    && !self.functions.contains_key("undefined") =>
            {
                Some((operand, Guard::Exact(Type::Undefined)))
            }
            _ => {
                let placeholder = ast::Expr::Ident(ast::Ident::new(
                    "#path".into(),
                    operand.span(),
                    Default::default(),
                ));
                literals::guard_operand(&placeholder, other, self.types)
                    .map(|(_, single)| (operand, Guard::Literal(single)))
            }
        }
    }
    /// A member read, narrowed by the current path facts (v37).
    pub(super) fn member_read(&mut self, member: &ast::MemberExpr) -> Result<Expr> {
        let value = self.result_field(member)?;
        self.path_read(member, value)
    }
    /// A path read under the current facts.
    pub(super) fn path_read(&mut self, member: &ast::MemberExpr, value: Expr) -> Result<Expr> {
        let expr = ast::Expr::Member(member.clone());
        let Some((path, _)) = self.path(&expr) else {
            return Ok(value);
        };
        let Some(fact) = self.core.paths.get(&path).copied() else {
            return Ok(value);
        };
        if fact.declared != value.ty || fact.narrowed == value.ty {
            return Ok(value);
        }
        let narrow = fact.narrowed;
        let retag = narrow.is_cell() || self.types.profile.literal_types() && !value.ty.is_cell();
        Ok(if retag {
            Expr {
                ty: narrow,
                ..value
            }
        } else {
            Expr {
                span: value.span,
                ty: narrow,
                kind: E::UnboxUnion(Box::new(value)),
            }
        })
    }
    /// Join the path facts of the reachable `paths`: a path keeps a fact only
    /// when every one has a fact for it, with the union of their types.
    pub(super) fn flow_join_paths(&mut self, paths: &[&flow::Snapshot]) -> Result<()> {
        let Some((first, rest)) = paths.split_first() else {
            self.core.paths.clear();
            return Ok(());
        };
        let mut joined = BTreeMap::new();
        'facts: for (path, fact) in &first.paths {
            let mut members = unions::members(fact.narrowed, self.types);
            for other in rest {
                match other.paths.get(path) {
                    Some(next) if next.declared == fact.declared => {
                        members.extend(unions::members(next.narrowed, self.types));
                    }
                    _ => continue 'facts,
                }
            }
            members.sort();
            members.dedup();
            let narrowed = unions::intern(members, self.types)?;
            if narrowed != fact.declared {
                joined.insert(
                    path.clone(),
                    Fact {
                        declared: fact.declared,
                        narrowed,
                    },
                );
            }
        }
        self.core.paths = joined;
        Ok(())
    }
    /// Drop every path fact (an effect that may write a property).
    pub(super) fn drop_paths(&mut self) {
        self.core.paths.clear();
    }
    /// Drop the facts rooted at `slot` (a write to the root binding).
    pub(super) fn drop_root_paths(&mut self, slot: u32) {
        self.core.paths.retain(|(root, _), _| *root != slot);
    }
    /// Drop the path facts an evaluated `expression` may invalidate.
    pub(super) fn path_effects(&mut self, expression: &ast::Expr) {
        if !self.types.profile.language_core() || self.core.paths.is_empty() {
            return;
        }
        let effect = match expression {
            ast::Expr::Call(call) => {
                !flow::native(call, self)
                    || matches!(&call.callee, ast::Callee::Expr(callee)
                    if expr::callee_name(callee).is_some_and(|name| {
                        name.starts_with("Object.") || name == "JSON.stringify"
                    }))
            }
            ast::Expr::Assign(assign) => !matches!(
                assign.left,
                ast::AssignTarget::Simple(ast::SimpleAssignTarget::Ident(_))
            ),
            ast::Expr::Update(update) => !matches!(&*update.arg, ast::Expr::Ident(_)),
            ast::Expr::Unary(unary) => unary.op == ast::UnaryOp::Delete,
            ast::Expr::OptChain(_) => impure(expression),
            ast::Expr::New(_)
            | ast::Expr::Await(_)
            | ast::Expr::Yield(_)
            | ast::Expr::TaggedTpl(_) => true,
            _ => false,
        };
        if effect {
            self.drop_paths();
        }
    }
}
