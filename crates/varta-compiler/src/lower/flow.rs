//! Declared storage types bound flow facts; writes and branch joins update them.
use super::*;
use std::collections::BTreeSet;
use swc_ecma_visit::{Visit, VisitWith};
#[derive(Clone)]
pub(super) struct Snapshot {
    pub(super) facts: BTreeMap<u32, Type>,
    pub(super) writes: BTreeMap<u32, u64>,
    pub(super) reachable: bool,
    /// V37 deferred `let` bindings that are not definitely assigned.
    pub(super) unassigned: BTreeSet<u32>,
    /// V37 property-path facts.
    pub(super) paths: BTreeMap<paths::Path, paths::Fact>,
}
pub(super) fn native(call: &ast::CallExpr, lower: &Lower<'_>) -> bool {
    let ast::Callee::Expr(callee) = &call.callee else {
        return false;
    };
    // A class method of the same name may run user code (v32 classes).
    if let ast::Expr::Member(member) = &**callee
        && let ast::MemberProp::Ident(method) = &member.prop
        && lower.types.classes.has_method(method.sym.as_ref())
    {
        return false;
    }
    // These admitted member calls dispatch to checked intrinsics before generic
    // function lookup. Array mutation changes elements, not caller binding cells.
    // Revisit this list when methods, prototypes or coercion can invoke user code.
    if let ast::Expr::Member(member) = &**callee
        && let ast::MemberProp::Ident(method) = &member.prop
        && matches!(
            method.sym.as_ref(),
            "valueOf"
                | "toString"
                | "toFixed"
                | "toExponential"
                | "toPrecision"
                | "slice"
                | "substring"
                | "trim"
                | "trimStart"
                | "trimEnd"
                | "indexOf"
                | "lastIndexOf"
                | "includes"
                | "startsWith"
                | "endsWith"
                | "repeat"
                | "charAt"
                | "charCodeAt"
                | "push"
                | "pop"
                | "at"
                | "reverse"
                | "fill"
        )
    {
        return true;
    }
    let Some(name) = expr::callee_name(callee) else {
        return false;
    };
    let root = name.split('.').next().unwrap();
    if lower.binding(root).is_ok()
        || lower.functions.contains_key(root)
        || lower.outer.contains_key(root)
        || lower.generic_name(root).is_some()
    {
        return false;
    }
    matches!(
        name.as_str(),
        "String"
            | "Number"
            | "Boolean"
            | "isFinite"
            | "isNaN"
            | "parseInt"
            | "parseFloat"
            | "Error"
            | "TypeError"
            | "RangeError"
            | "ReferenceError"
            | "SyntaxError"
            | "EvalError"
            | "URIError"
            | "console.log"
            | "log"
    ) || name.starts_with("varta.")
        || name.starts_with("Math.")
        || name.starts_with("Number.")
        || name.starts_with("JSON.")
        || name == "Array.isArray"
        || name.starts_with("Object.")
}

fn bound(pattern: &ast::Pat) -> BTreeSet<String> {
    patterns::names(std::slice::from_ref(pattern))
        .unwrap_or_default()
        .into_iter()
        .map(|(name, _)| name)
        .collect()
}
struct Effects<'a, 'b> {
    lower: &'a Lower<'b>,
    names: BTreeSet<String>,
    calls: bool,
    identities: BTreeSet<(usize, u32)>,
    shadows: Vec<BTreeSet<String>>,
    depth: usize,
    exhausted: bool,
}
impl Effects<'_, '_> {
    fn add(&mut self, name: String) {
        if !self.shadows.iter().any(|s| s.contains(&name)) {
            self.names.insert(name);
        }
    }
    fn pattern(&mut self, pat: &ast::Pat) {
        for (name, _) in patterns::targets(pat).unwrap_or_default() {
            self.add(name);
        }
    }
    fn head(&mut self, head: &ast::ForHead) -> BTreeSet<String> {
        match head {
            ast::ForHead::VarDecl(var) => var.decls.iter().flat_map(|d| bound(&d.name)).collect(),
            ast::ForHead::Pat(pat) => {
                self.pattern(pat);
                Default::default()
            }
            _ => Default::default(),
        }
    }
    fn enter(&mut self) -> bool {
        self.depth += 1;
        if self.depth > 64 {
            self.exhausted = true;
            self.depth -= 1;
            false
        } else {
            true
        }
    }
}
impl Visit for Effects<'_, '_> {
    fn visit_expr(&mut self, node: &ast::Expr) {
        if self.enter() {
            node.visit_children_with(self);
            self.depth -= 1;
        }
    }
    fn visit_stmt(&mut self, node: &ast::Stmt) {
        if self.enter() {
            node.visit_children_with(self);
            self.depth -= 1;
        }
    }
    fn visit_pat(&mut self, node: &ast::Pat) {
        if self.enter() {
            node.visit_children_with(self);
            self.depth -= 1;
        }
    }
    fn visit_ts_type(&mut self, _node: &ast::TsType) {}
    fn visit_assign_expr(&mut self, node: &ast::AssignExpr) {
        match &node.left {
            ast::AssignTarget::Simple(ast::SimpleAssignTarget::Ident(id)) => {
                self.add(id.id.sym.to_string())
            }
            ast::AssignTarget::Pat(pat) => self.pattern(&pat.clone().into()),
            _ => {}
        }
        node.visit_children_with(self);
    }
    fn visit_update_expr(&mut self, node: &ast::UpdateExpr) {
        if let ast::Expr::Ident(id) = stmt::unparen(&node.arg) {
            self.add(id.sym.to_string());
        }
        node.visit_children_with(self);
    }
    fn visit_new_expr(&mut self, node: &ast::NewExpr) {
        // A class constructor runs user code; treat it as an unknown call.
        if self.lower.class_callee(&node.callee).is_some() {
            self.calls = true;
        }
        node.visit_children_with(self);
    }
    fn visit_call_expr(&mut self, node: &ast::CallExpr) {
        let shadowed = if let ast::Callee::Expr(callee) = &node.callee {
            expr::callee_name(callee).is_some_and(|name| {
                self.shadows
                    .iter()
                    .any(|s| s.contains(name.split('.').next().unwrap()))
            })
        } else {
            false
        };
        if shadowed {
            self.calls = true;
        } else {
            let impact = self.lower.flow_ast_call(node, self.depth + 1, false);
            self.calls |= impact.unknown || !impact.parameters.is_empty();
            self.identities.extend(impact.writes);
        }
        node.visit_children_with(self);
    }
    fn visit_block_stmt(&mut self, node: &ast::BlockStmt) {
        let mut names = BTreeSet::new();
        for statement in &node.stmts {
            match statement {
                ast::Stmt::Decl(ast::Decl::Var(var)) => {
                    for d in &var.decls {
                        names.extend(bound(&d.name));
                    }
                }
                ast::Stmt::Decl(ast::Decl::Fn(f)) => {
                    names.insert(f.ident.sym.to_string());
                }
                _ => {}
            }
        }
        self.shadows.push(names);
        node.visit_children_with(self);
        self.shadows.pop();
    }
    fn visit_function(&mut self, node: &ast::Function) {
        let names = node.params.iter().flat_map(|p| bound(&p.pat)).collect();
        self.shadows.push(names);
        node.visit_children_with(self);
        self.shadows.pop();
    }
    fn visit_fn_expr(&mut self, node: &ast::FnExpr) {
        self.shadows
            .push(node.ident.iter().map(|id| id.sym.to_string()).collect());
        node.visit_children_with(self);
        self.shadows.pop();
    }
    fn visit_arrow_expr(&mut self, node: &ast::ArrowExpr) {
        self.shadows
            .push(node.params.iter().flat_map(bound).collect());
        node.visit_children_with(self);
        self.shadows.pop();
    }
    fn visit_for_stmt(&mut self, node: &ast::ForStmt) {
        let names = if let Some(ast::VarDeclOrExpr::VarDecl(var)) = &node.init {
            var.decls.iter().flat_map(|d| bound(&d.name)).collect()
        } else {
            Default::default()
        };
        self.shadows.push(names);
        node.visit_children_with(self);
        self.shadows.pop();
    }
    fn visit_for_of_stmt(&mut self, node: &ast::ForOfStmt) {
        let names = self.head(&node.left);
        self.shadows.push(names);
        node.visit_children_with(self);
        self.shadows.pop();
    }
    fn visit_for_in_stmt(&mut self, node: &ast::ForInStmt) {
        let names = self.head(&node.left);
        self.shadows.push(names);
        node.visit_children_with(self);
        self.shadows.pop();
    }
    fn visit_catch_clause(&mut self, node: &ast::CatchClause) {
        self.shadows
            .push(node.param.iter().flat_map(bound).collect());
        node.visit_children_with(self);
        self.shadows.pop();
    }
}
impl Lower<'_> {
    pub(super) fn flow_literal_impact(
        &self,
        expression: &ast::Expr,
        depth: usize,
    ) -> flow_calls::Impact {
        if depth >= 64 {
            return flow_calls::Impact {
                unknown: true,
                ..Default::default()
            };
        }
        let mut effects = Effects {
            lower: self,
            names: Default::default(),
            identities: Default::default(),
            calls: false,
            shadows: vec![],
            depth,
            exhausted: false,
        };
        expression.visit_with(&mut effects);
        let mut writes = effects.identities;
        for name in effects.names {
            if let Ok(binding) = self.binding(&name)
                && binding.mutable
            {
                writes.insert(binding.identity);
            }
        }
        flow_calls::Impact {
            writes,
            unknown: effects.calls || effects.exhausted,
            ..Default::default()
        }
    }
    pub(super) fn flow_snapshot(&self) -> Snapshot {
        Snapshot {
            facts: self.refinements.clone(),
            writes: self.writes.clone(),
            reachable: self.reachable,
            unassigned: self.core.unassigned.clone(),
            paths: self.core.paths.clone(),
        }
    }
    pub(super) fn flow_alternate(&mut self, saved: &Snapshot) {
        self.refinements = saved.facts.clone();
        self.reachable = saved.reachable;
        // Bindings declared since `saved` stay unassigned (and out of scope).
        self.core
            .unassigned
            .extend(saved.unassigned.iter().copied());
        self.core.paths = saved.paths.clone();
    }
    pub(super) fn flow_restore(&mut self, saved: &Snapshot) {
        self.reachable = saved.reachable;
        self.core
            .unassigned
            .extend(saved.unassigned.iter().copied());
        // Both the skipped and the taken region reach here: keep the facts
        // that hold after either.
        self.core
            .paths
            .retain(|path, fact| saved.paths.get(path) == Some(fact));
        self.refinements = saved
            .facts
            .iter()
            .filter(|(slot, _)| {
                !self.types.profile.refinement_writes()
                    || self.writes.get(slot) == saved.writes.get(slot)
            })
            .map(|(&slot, &ty)| (slot, ty))
            .collect();
    }
    pub(super) fn flow_write(&mut self, slot: u32) {
        self.drop_root_paths(slot);
        if self.types.profile.refinement_writes() {
            *self.writes.entry(slot).or_default() += 1;
            self.refinements.remove(&slot);
        }
    }
    pub(super) fn flow_assigned(&mut self, slot: u32, observed: Type) -> Result<()> {
        self.core.unassigned.remove(&slot);
        if self.types.profile.refinement_writes() {
            self.direct_writes.insert(slot);
        }
        self.flow_write(slot);
        self.flow_fact(slot, observed)
    }
    pub(super) fn flow_fact(&mut self, slot: u32, observed: Type) -> Result<()> {
        if !self.types.profile.refinement_writes() {
            return Ok(());
        }
        let Some(&declared) = self.local_types.get(&slot) else {
            return Ok(());
        };
        if declared == Type::Unknown {
            // `unknown` has no member list: any narrower admitted type that a
            // path proved (a guard or a join of guarded paths) refines it.
            self.refinements.remove(&slot);
            if observed != Type::Unknown && unknown::admissible(observed) {
                self.refinements.insert(slot, observed);
            }
            return Ok(());
        }
        if self.types.profile.literal_types() {
            return self.literal_flow_fact(slot, declared, observed);
        }
        if !declared.is_union() {
            return Ok(());
        }
        let upper = unions::members(declared, self.types);
        let members = unions::members(observed, self.types);
        self.refinements.remove(&slot);
        if members.len() < upper.len() && members.iter().all(|ty| upper.contains(ty)) {
            let ty = unions::intern(members, self.types)?;
            self.refinements.insert(slot, ty);
        }
        Ok(())
    }
    pub(super) fn flow_join(&mut self, entry: &Snapshot, paths: &[Snapshot]) -> Result<()> {
        if !self.types.profile.refinement_writes() {
            self.flow_restore(entry);
            return Ok(());
        }
        let paths: Vec<_> = paths.iter().filter(|p| p.reachable).collect();
        self.reachable = !paths.is_empty();
        self.core.unassigned = paths
            .iter()
            .flat_map(|p| p.unassigned.iter().copied())
            .collect();
        self.flow_join_paths(&paths)?;
        let slots: BTreeSet<_> = paths.iter().flat_map(|p| p.facts.keys().copied()).collect();
        self.refinements.clear();
        for slot in slots {
            let Some(&declared) = self.local_types.get(&slot) else {
                continue;
            };
            let mut members: Vec<_> = paths
                .iter()
                .flat_map(|path| {
                    unions::members(
                        path.facts.get(&slot).copied().unwrap_or(declared),
                        self.types,
                    )
                })
                .collect();
            members.sort();
            members.dedup();
            if !members.is_empty() {
                let ty = unions::intern(members, self.types)?;
                self.flow_fact(slot, ty)?;
            }
        }
        Ok(())
    }
    pub(super) fn mutable_cells(&self) -> BTreeSet<u32> {
        self.scopes
            .iter()
            .flat_map(|s| s.values())
            .chain(self.captures.values())
            .filter(|b| b.mutable && self.cells.contains(&b.slot))
            .map(|b| b.slot)
            .collect()
    }
    pub(super) fn flow_guard(&mut self, test: &ast::Expr, yes: bool) -> Result<()> {
        if !self.types.profile.refinement_writes() {
            return self.guard_inner(test, yes);
        }
        let (names, identities, calls, exhausted) = {
            let mut effects = Effects {
                lower: self,
                names: Default::default(),
                calls: false,
                identities: Default::default(),
                shadows: vec![],
                depth: 0,
                exhausted: false,
            };
            test.visit_with(&mut effects);
            (
                effects.names,
                effects.identities,
                effects.calls,
                effects.exhausted,
            )
        };
        let saved = self.guard_blocked.clone();
        let impure = self.core.impure_guard;
        self.core.impure_guard |= paths::impure(test);
        if exhausted {
            self.guard_blocked.extend(self.local_types.keys().copied());
        }
        for name in names {
            if let Ok(binding) = self.resolve_binding(&name) {
                self.guard_blocked.insert(binding.slot);
            }
        }
        self.guard_blocked
            .extend(self.flow_identity_slots(&identities));
        if calls {
            self.guard_blocked.extend(self.mutable_cells());
        }
        let result = self.guard_inner(test, yes);
        self.guard_blocked = saved;
        self.core.impure_guard = impure;
        result
    }
    // A loop entry keeps the path facts (v37) only when no iteration can
    // write a property or run code; `for...of`/`for...in` steps, handlers and
    // finalizers drop them.
    pub(super) fn flow_while_effects(&mut self, statement: &ast::WhileStmt) {
        if paths::impure(statement) {
            self.drop_paths();
        }
        self.flow_effects(|effects| statement.visit_with(effects));
    }
    pub(super) fn flow_do_effects(&mut self, statement: &ast::DoWhileStmt) {
        if paths::impure(statement) {
            self.drop_paths();
        }
        self.flow_effects(|effects| statement.visit_with(effects));
    }
    pub(super) fn flow_for_effects(&mut self, statement: &ast::ForStmt) {
        if paths::impure(&statement.test)
            || paths::impure(&statement.update)
            || paths::impure(&*statement.body)
        {
            self.drop_paths();
        }
        self.flow_effects(|effects| {
            statement.test.visit_with(effects);
            statement.update.visit_with(effects);
            statement.body.visit_with(effects);
        });
    }
    pub(super) fn flow_for_of_effects(&mut self, statement: &ast::ForOfStmt) {
        self.drop_paths();
        self.flow_effects(|effects| {
            let names = effects.head(&statement.left);
            effects.shadows.push(names);
            statement.left.visit_with(effects);
            statement.body.visit_with(effects);
            effects.shadows.pop();
        });
    }
    pub(super) fn flow_for_in_effects(&mut self, statement: &ast::ForInStmt) {
        self.drop_paths();
        self.flow_effects(|effects| {
            let names = effects.head(&statement.left);
            effects.shadows.push(names);
            statement.left.visit_with(effects);
            statement.body.visit_with(effects);
            effects.shadows.pop();
        });
    }
    pub(super) fn flow_block_effects(&mut self, block: &ast::BlockStmt) {
        self.drop_paths();
        self.flow_effects(|effects| block.visit_with(effects));
    }
    pub(super) fn flow_try_prefix(&mut self, statement: &ast::TryStmt) {
        self.drop_paths();
        self.flow_effects(|effects| {
            statement.block.visit_with(effects);
            statement.handler.visit_with(effects);
        });
    }
    fn flow_effects(&mut self, visit: impl FnOnce(&mut Effects<'_, '_>)) {
        if !self.types.profile.refinement_writes() {
            return;
        }
        let (names, identities, calls, exhausted) = {
            let mut effects = Effects {
                lower: self,
                names: Default::default(),
                calls: false,
                identities: Default::default(),
                shadows: vec![],
                depth: 0,
                exhausted: false,
            };
            visit(&mut effects);
            (
                effects.names,
                effects.identities,
                effects.calls,
                effects.exhausted,
            )
        };
        let mut slots = self.flow_identity_slots(&identities);
        if exhausted {
            slots.extend(self.refinements.keys().copied());
        }
        for name in names {
            if let Ok(binding) = self.resolve_binding(&name) {
                slots.insert(binding.slot);
            }
        }
        if calls {
            slots.extend(self.mutable_cells());
        }
        for slot in slots {
            self.flow_write(slot);
        }
    }
    pub(super) fn capture_narrows(&self) -> BTreeMap<(usize, u32), Type> {
        let mut result = self.outer_narrows.clone();
        for binding in self
            .scopes
            .iter()
            .flat_map(|s| s.values())
            .chain(self.captures.values())
        {
            if !binding.mutable
                && let Some(&ty) = self.refinements.get(&binding.slot)
            {
                result.insert(binding.identity, ty);
            }
        }
        result
    }
}
