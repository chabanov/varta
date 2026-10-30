//! Completion-targeted facts are separate from physical Wasm branch labels.
use super::*;
use flow::Snapshot;
#[derive(Default)]
pub(super) struct Control {
    next: u32,
    targets: Vec<Target>,
    exits: Vec<Exit>,
}
struct Target {
    id: u32,
    kind: Kind,
}
pub(super) enum Kind {
    Loop(Vec<u32>),
    Switch,
    Label(u32, bool),
}
struct Exit {
    target: u32,
    continuing: bool,
    state: Snapshot,
}
/// A constant result can still have effects (void); its operand is lowered as
/// usual. This only decides whether a successful test can select an edge.
pub(super) fn truth(expr: &ast::Expr) -> Option<bool> {
    truth_inner(expr, 0)
}
fn truth_inner(expr: &ast::Expr, depth: usize) -> Option<bool> {
    if depth >= 64 {
        return None;
    }
    match stmt::unparen(expr) {
        ast::Expr::Lit(ast::Lit::Bool(value)) => Some(value.value),
        ast::Expr::Lit(ast::Lit::Num(value)) => Some(value.value != 0.0 && !value.value.is_nan()),
        ast::Expr::Lit(ast::Lit::Str(value)) => Some(!value.value.is_empty()),
        ast::Expr::Lit(ast::Lit::Null(_)) => Some(false),
        ast::Expr::Array(_) | ast::Expr::Object(_) | ast::Expr::Arrow(_) | ast::Expr::Fn(_) => {
            Some(true)
        }
        ast::Expr::Unary(value) if value.op == ast::UnaryOp::Void => Some(false),
        ast::Expr::Unary(value) if value.op == ast::UnaryOp::Bang => {
            truth_inner(&value.arg, depth + 1).map(|v| !v)
        }
        ast::Expr::Unary(value) if value.op == ast::UnaryOp::Minus => {
            if let ast::Expr::Lit(ast::Lit::Num(number)) = stmt::unparen(&value.arg) {
                Some(number.value != 0.0 && !number.value.is_nan())
            } else {
                None
            }
        }
        _ => None,
    }
}
impl Lower<'_> {
    pub(super) fn flow_edge(&mut self, selected: bool) {
        if self.types.profile.refinement_writes() {
            self.reachable &= selected;
        }
    }
    pub(super) fn flow_target(&mut self, kind: Kind) -> Option<u32> {
        if !self.types.profile.refinement_writes() {
            return None;
        }
        let id = self.control.next;
        self.control.next += 1;
        self.control.targets.push(Target { id, kind });
        Some(id)
    }
    pub(super) fn flow_iteration(&mut self) -> Option<u32> {
        // Only directly wrapping iteration labels bind to this loop. A nested
        // loop cannot consume an already bound outer continue target.
        let labels = self
            .control
            .targets
            .iter()
            .rev()
            .take_while(|target| matches!(target.kind, Kind::Label(_, true)))
            .filter_map(|target| {
                if let Kind::Label(id, _) = target.kind {
                    Some(id)
                } else {
                    None
                }
            })
            .collect();
        self.flow_target(Kind::Loop(labels))
    }
    pub(super) fn flow_take(&mut self, target: Option<u32>, continuing: bool) -> Vec<Snapshot> {
        let Some(target) = target else { return vec![] };
        let mut paths = vec![];
        self.control.exits.retain(|exit| {
            if exit.target == target && exit.continuing == continuing {
                paths.push(exit.state.clone());
                false
            } else {
                true
            }
        });
        paths
    }
    pub(super) fn flow_end(&mut self, target: Option<u32>) -> Vec<Snapshot> {
        let paths = self.flow_take(target, false);
        if let Some(id) = target {
            debug_assert!(self.control.targets.last().is_some_and(|t| t.id == id));
            self.control.targets.pop();
            debug_assert!(!self.control.exits.iter().any(|e| e.target == id));
        }
        paths
    }
    pub(super) fn flow_branch(&mut self, label: Option<u32>, continuing: bool) -> Result<()> {
        if !self.types.profile.refinement_writes() {
            return Ok(());
        }
        let target = self
            .control
            .targets
            .iter()
            .rev()
            .find(|target| match (&target.kind, label, continuing) {
                (Kind::Loop(names), Some(id), true) => names.contains(&id),
                (Kind::Label(id, _), Some(label), false) => *id == label,
                (Kind::Loop(_), None, _) | (Kind::Switch, None, false) => true,
                _ => false,
            })
            .map(|t| t.id);
        let target = target.ok_or_else(|| invalid("missing checked flow target"))?;
        if self.reachable {
            self.control.exits.push(Exit {
                target,
                continuing,
                state: self.flow_snapshot(),
            });
        }
        self.reachable = false;
        Ok(())
    }
    pub(super) fn flow_exit_mark(&self) -> usize {
        self.control.exits.len()
    }
    pub(super) fn flow_finally(
        &mut self,
        start: usize,
        end: usize,
        before: &Snapshot,
        paths: Vec<Snapshot>,
    ) -> Vec<Snapshot> {
        if !self.types.profile.refinement_writes() {
            return paths;
        }
        let after = self.flow_snapshot();
        if !after.reachable {
            self.control.exits.drain(start..end);
            return vec![];
        }
        let transform = |mut state: Snapshot| {
            state
                .facts
                .retain(|slot, _| before.writes.get(slot) == after.writes.get(slot));
            state.facts.extend(after.facts.clone());
            state.writes = after.writes.clone();
            state
                .unassigned
                .retain(|slot| after.unassigned.contains(slot));
            // The finalizer started without path facts; its own still hold.
            state.paths = after.paths.clone();
            state
        };
        for exit in &mut self.control.exits[start..end] {
            exit.state = transform(exit.state.clone());
        }
        paths
            .into_iter()
            .filter(|p| p.reachable)
            .map(transform)
            .collect()
    }
}
