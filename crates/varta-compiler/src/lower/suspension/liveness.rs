//! Liveness of locals across suspension points (v35 `frame_liveness`).
//!
//! A backward dataflow analysis over the normalized body, after states are
//! numbered and before the state machine is built, so control flow is still
//! structured. The saved set of state `k` holds every local that may be read
//! after resumption at `k` before it is written: through the normal
//! continuation, the exception continuation (a `Throw` completion is raised
//! at the point) and the return continuation (a `Return` completion runs the
//! enclosing finalizers). The analysis may only over-approximate:
//!
//! - a statement-level `Init`/`Set` of an unshared local is the only kill; a
//!   write inside an expression (an assignment or value block) counts as a use;
//! - a local shared with closures is never killed and every mention of it is a
//!   use, because its closure cell is the binding and must stay the same cell;
//! - every statement may throw, so the exception continuation is live before it;
//! - a finalizer may continue to any enclosing target, and the completion,
//!   exception and result locals are live throughout it;
//! - a local mentioned only inside one statement operand (scratch of a call,
//!   value block, repeat and similar forms) is never live across a point,
//!   because an operand is evaluated atomically after normalization.
//!
//! The state machine adds the locals its resumption path reads itself (the
//! selected case of a suspending `switch`, the kept completion of a suspending
//! finalizer) for the states they cover.
use super::*;
use std::collections::{BTreeMap, BTreeSet};

type Set = BTreeSet<u32>;

/// Where control continues after an abrupt completion, as live-in sets.
#[derive(Clone, Default)]
struct Targets {
    exception: Set,
    returned: Set,
    broken: Option<Set>,
    continued: Option<Set>,
    /// Named label: its break target and, for an iteration label, the
    /// continue target of its loop once that loop is analysed.
    labels: BTreeMap<u32, (Set, Option<Set>)>,
    /// Iteration labels whose loop is the next loop statement.
    pending: Vec<u32>,
}
impl Targets {
    /// Every set an abrupt completion of a finalizer may continue to.
    fn all(&self) -> Set {
        let mut all = self.exception.clone();
        all.extend(&self.returned);
        for set in self.broken.iter().chain(&self.continued) {
            all.extend(set);
        }
        for (broken, continued) in self.labels.values() {
            all.extend(broken);
            all.extend(continued.iter().flatten());
        }
        all
    }
    /// The targets inside a loop body: `break` leaves the loop, `continue`
    /// reaches `next`, and pending iteration labels name this loop.
    fn looping(&self, out: &Set, next: &Set) -> Self {
        let mut inner = self.clone();
        inner.broken = Some(out.clone());
        inner.continued = Some(next.clone());
        for id in std::mem::take(&mut inner.pending) {
            if let Some(label) = inner.labels.get_mut(&id) {
                label.1 = Some(next.clone());
            }
        }
        inner
    }
}

pub(super) struct Liveness<'a> {
    cells: &'a BTreeSet<u32>,
    /// Every local, the conservative answer for an unknown target.
    universe: Set,
    confined: Set,
    /// Saved set of state `k` at index `k - 1`.
    live: Vec<Set>,
}

/// Saved locals per suspension point (index `state - 1`), sorted by local.
pub(super) fn analyze(
    body: &[S],
    states: usize,
    locals: usize,
    params: usize,
    cells: &BTreeSet<u32>,
) -> Result<Vec<Set>> {
    let mut analysis = Liveness {
        cells,
        universe: (0..locals as u32).collect(),
        confined: confined(body, params, cells),
        live: vec![Set::new(); states],
    };
    analysis.statements(body, Set::new(), &Targets::default())?;
    Ok(analysis.live)
}

impl Liveness<'_> {
    fn kill(&self, set: &mut Set, slot: u32) {
        if !self.cells.contains(&slot) {
            set.remove(&slot);
        }
    }
    fn uses(&self, expr: &Expr, set: &mut Set) -> Result<()> {
        let mut found = Ok(());
        expression(expr, &mut |mention| match mention {
            Mention::Local(slot) => {
                if !self.confined.contains(&slot) {
                    set.insert(slot);
                }
            }
            Mention::Point => {
                found = Err(CompilerError::Internal(
                    "noncanonical suspension point in liveness".into(),
                ));
            }
        });
        found
    }
    fn statements(&mut self, body: &[S], mut live: Set, targets: &Targets) -> Result<Set> {
        for statement in body.iter().rev() {
            live = self.statement(statement, live, targets)?;
        }
        Ok(live)
    }
    fn target(&self, set: Option<&Set>) -> Set {
        set.cloned().unwrap_or_else(|| self.universe.clone())
    }
    fn statement(&mut self, statement: &S, out: Set, targets: &Targets) -> Result<Set> {
        let mut live = match statement {
            S::Init(slot, value) => {
                if let E::Suspend { value, state } = &value.kind {
                    return self.point(*slot, value, *state, out, targets);
                }
                let mut live = out;
                self.kill(&mut live, *slot);
                self.uses(value, &mut live)?;
                live
            }
            S::Set(slot, value) => {
                let mut live = out;
                self.kill(&mut live, *slot);
                self.uses(value, &mut live)?;
                live
            }
            S::Expr(value) | S::InitModuleConstant(_, value) => {
                let mut live = out;
                self.uses(value, &mut live)?;
                live
            }
            S::Throw(value) => {
                let mut live = Set::new();
                self.uses(value, &mut live)?;
                live
            }
            S::Return(value) => {
                let mut live = targets.returned.clone();
                if let Some(value) = value {
                    self.uses(value, &mut live)?;
                }
                live
            }
            S::Trap => return Ok(Set::new()),
            S::Break => return Ok(self.target(targets.broken.as_ref())),
            S::Continue => return Ok(self.target(targets.continued.as_ref())),
            S::BreakLabel(id) => return Ok(self.target(targets.labels.get(id).map(|l| &l.0))),
            S::ContinueLabel(id) => {
                return Ok(self.target(targets.labels.get(id).and_then(|l| l.1.as_ref())));
            }
            S::Rebind(slots) => {
                let mut live = out;
                live.extend(slots);
                live
            }
            S::RecordEnumerate {
                source,
                target,
                slots,
                values,
                ..
            } => {
                let mut live = out;
                live.extend([*source, *target]);
                live.extend(slots);
                for value in values {
                    self.uses(value, &mut live)?;
                }
                live
            }
            S::RecordRest {
                source,
                target,
                index,
                cursor,
                ..
            } => {
                let mut live = out;
                live.extend([*source, *target, *index, *cursor]);
                live
            }
            S::SetField(object, _, value) => {
                let mut live = out;
                self.uses(object, &mut live)?;
                self.uses(value, &mut live)?;
                live
            }
            S::ArrayLength {
                array,
                length,
                error,
            } => {
                let mut live = out;
                for value in [array, length, error] {
                    self.uses(value, &mut live)?;
                }
                live
            }
            S::SetIndex {
                array,
                index,
                value,
                slots,
            } => {
                let mut live = out;
                for value in [array, index, value] {
                    self.uses(value, &mut live)?;
                }
                live.extend(slots.iter().flat_map(|(base, index)| [*base, *index]));
                live
            }
            S::Block(body) | S::Scope(_, body) => self.statements(body, out, targets)?,
            S::Label {
                id,
                iteration,
                body,
            } => {
                let mut inner = targets.clone();
                inner.labels.insert(*id, (out.clone(), None));
                if *iteration {
                    inner.pending.push(*id);
                }
                self.statements(body, out, &inner)?
            }
            S::If(test, yes, no) => {
                let mut live = self.statements(yes, out.clone(), targets)?;
                live.extend(self.statements(no, out, targets)?);
                self.uses(test, &mut live)?;
                live
            }
            S::While(test, body) => {
                let mut head = Set::new();
                loop {
                    let inner = targets.looping(&out, &head);
                    let mut next = self.statements(body, head.clone(), &inner)?;
                    next.extend(&out);
                    self.uses(test, &mut next)?;
                    next.extend(&targets.exception);
                    if next == head {
                        break head;
                    }
                    head = next;
                }
            }
            S::DoWhile(test, body) => {
                let mut condition = Set::new();
                loop {
                    let inner = targets.looping(&out, &condition);
                    let entry = self.statements(body, condition.clone(), &inner)?;
                    let mut next = entry.clone();
                    next.extend(&out);
                    self.uses(test, &mut next)?;
                    next.extend(&targets.exception);
                    if next == condition {
                        break entry;
                    }
                    condition = next;
                }
            }
            S::For(init, test, update, body) => {
                let mut head = Set::new();
                let head = loop {
                    let after = self.statements(update, head.clone(), targets)?;
                    let inner = targets.looping(&out, &after);
                    let mut next = self.statements(body, after, &inner)?;
                    if let Some(test) = test {
                        next.extend(&out);
                        self.uses(test, &mut next)?;
                        next.extend(&targets.exception);
                    }
                    if next == head {
                        break head;
                    }
                    head = next;
                };
                let mut outer = targets.clone();
                outer.pending.clear();
                self.statements(init, head, &outer)?
            }
            S::Switch { value, cases, .. } => {
                let mut inner = targets.clone();
                inner.broken = Some(out.clone());
                inner.pending.clear();
                let mut live = out.clone();
                let mut next = out;
                for (test, body) in cases.iter().rev() {
                    next = self.statements(body, next, &inner)?;
                    live.extend(&next);
                    if let Some(test) = test {
                        self.uses(test, &mut live)?;
                    }
                }
                self.uses(value, &mut live)?;
                live
            }
            S::TryCatch {
                body,
                binding,
                handler,
                ..
            } => {
                let mut caught = self.statements(handler, out.clone(), targets)?;
                if let Some(binding) = binding {
                    self.kill(&mut caught, *binding);
                }
                let mut inner = targets.clone();
                inner.exception = caught;
                self.statements(body, out, &inner)?
            }
            S::TryFinally {
                body,
                finalizer,
                completion,
                exception,
                result,
                ..
            } => {
                let pending: Set = [*completion, *exception]
                    .into_iter()
                    .chain(*result)
                    .collect();
                let mut after = out;
                after.extend(targets.all());
                after.extend(&pending);
                let mut entry = self.statements(finalizer, after, targets)?;
                // Entering the finalizer writes the pending completion.
                for slot in &pending {
                    self.kill(&mut entry, *slot);
                }
                let mut inner = targets.clone();
                inner.exception = entry.clone();
                inner.returned = entry.clone();
                inner.broken = inner.broken.map(|_| entry.clone());
                inner.continued = inner.continued.map(|_| entry.clone());
                for label in inner.labels.values_mut() {
                    label.0 = entry.clone();
                    label.1 = label.1.as_ref().map(|_| entry.clone());
                }
                self.statements(body, entry, &inner)?
            }
        };
        // Any statement may throw before it completes.
        live.extend(&targets.exception);
        Ok(live)
    }
    /// `Init(t, Suspend { value })`: the saved set is everything live after the
    /// resumption (except `t`, which receives the input) and on the throw and
    /// return completions delivered at the point; saving reads all of it.
    fn point(
        &mut self,
        slot: u32,
        value: &Expr,
        state: u32,
        out: Set,
        targets: &Targets,
    ) -> Result<Set> {
        let mut saved = out;
        self.kill(&mut saved, slot);
        saved.extend(&targets.exception);
        saved.extend(&targets.returned);
        let index = (state as usize)
            .checked_sub(1)
            .filter(|index| *index < self.live.len())
            .ok_or_else(|| CompilerError::Internal("unnumbered suspension point".into()))?;
        self.live[index].extend(&saved);
        let mut live = saved;
        self.uses(value, &mut live)?;
        live.extend(&targets.exception);
        Ok(live)
    }
}

/// A mention found while visiting an expression.
enum Mention {
    Local(u32),
    /// A suspension point or `await` left inside an expression.
    Point,
}

/// Visit every local an expression mentions: reads, hidden scratch slots,
/// closure captures and the targets and hidden slots of nested statements.
fn expression(expr: &Expr, visit: &mut dyn FnMut(Mention)) {
    walk::expression(expr, &mut |node| {
        node_mentions(node, visit);
        if let E::Assignment { body, .. } | E::ValueBlock { body, .. } = &node.kind {
            for statement in body {
                statement_tree(statement, &mut |slot| visit(Mention::Local(slot)));
            }
        }
    });
}

/// Locals named by the fields of one expression node.
fn node_mentions(expr: &Expr, visit: &mut dyn FnMut(Mention)) {
    let mut local = |slot: u32| visit(Mention::Local(slot));
    match &expr.kind {
        E::Local(slot)
        | E::FunctionCheck { slot, .. }
        | E::IsClass { slot, .. }
        | E::CheckedBigIntHost { status: slot, .. }
        | E::Json(_, Some(slot)) => local(*slot),
        E::Assignment { input, marker, .. } => [*input, *marker].into_iter().for_each(local),
        E::FunctionRef { captures, .. } => captures.iter().copied().for_each(local),
        E::FunctionCall {
            slot,
            arguments,
            output,
            ..
        } => std::iter::once(slot)
            .chain(arguments)
            .chain(output)
            .copied()
            .for_each(local),
        E::StringRepeat { length, filled, .. } => [*length, *filled].into_iter().for_each(local),
        E::ValueBlock {
            output,
            marker,
            roots,
            ..
        } => [output, marker]
            .into_iter()
            .chain(roots)
            .copied()
            .for_each(local),
        E::RecordDelete {
            slot,
            scan,
            write,
            count,
            ..
        } => [*slot, *scan, *write, *count].into_iter().for_each(local),
        E::ArrayReserve { count, bytes, .. } => [*count, *bytes].into_iter().for_each(local),
        E::Suspend { .. } | E::Await { .. } => visit(Mention::Point),
        _ => {}
    }
}

/// Locals named by statement fields (targets and hidden slots) in a
/// statement and its nested statement lists; operands are not visited.
fn statement_tree(statement: &S, visit: &mut dyn FnMut(u32)) {
    statement_fields(statement, visit);
    for body in substatements(statement) {
        for statement in body {
            statement_tree(statement, visit);
        }
    }
}

fn statement_fields(statement: &S, visit: &mut dyn FnMut(u32)) {
    match statement {
        S::Init(slot, _) | S::Set(slot, _) => visit(*slot),
        S::Rebind(slots) | S::Scope(slots, _) => slots.iter().copied().for_each(visit),
        S::RecordEnumerate {
            source,
            target,
            slots,
            ..
        } => [source, target]
            .into_iter()
            .chain(slots)
            .copied()
            .for_each(visit),
        S::RecordRest {
            source,
            target,
            index,
            cursor,
            ..
        } => [*source, *target, *index, *cursor]
            .into_iter()
            .for_each(visit),
        S::TryCatch {
            binding, marker, ..
        } => std::iter::once(marker)
            .chain(binding)
            .copied()
            .for_each(visit),
        S::TryFinally {
            marker,
            completion,
            exception,
            result,
            ..
        } => [marker, completion, exception]
            .into_iter()
            .chain(result)
            .copied()
            .for_each(visit),
        S::SetIndex {
            slots: Some((base, index)),
            ..
        } => [*base, *index].into_iter().for_each(visit),
        S::Switch {
            discriminant,
            selected,
            ..
        } => [*discriminant, *selected].into_iter().for_each(visit),
        _ => {}
    }
}

/// The nested statement lists of a statement.
fn substatements(statement: &S) -> Vec<&[S]> {
    match statement {
        S::Block(body)
        | S::Scope(_, body)
        | S::Label { body, .. }
        | S::While(_, body)
        | S::DoWhile(_, body) => vec![body],
        S::If(_, yes, no) => vec![yes, no],
        S::For(init, _, update, body) => vec![init, update, body],
        S::Switch { cases, .. } => cases.iter().map(|(_, body)| body.as_slice()).collect(),
        S::TryCatch { body, handler, .. } => vec![body, handler],
        S::TryFinally {
            body, finalizer, ..
        } => vec![body, finalizer],
        _ => vec![],
    }
}

/// The direct expression operands of a statement.
fn operands(statement: &S) -> Vec<&Expr> {
    match statement {
        S::Throw(value)
        | S::Init(_, value)
        | S::Set(_, value)
        | S::InitModuleConstant(_, value)
        | S::Expr(value)
        | S::Return(Some(value)) => vec![value],
        S::RecordEnumerate { values, .. } => values.iter().collect(),
        S::SetField(object, _, value) => vec![object, value],
        S::ArrayLength {
            array,
            length,
            error,
        } => vec![array, length, error],
        S::SetIndex {
            array,
            index,
            value,
            ..
        } => vec![array, index, value],
        S::If(test, ..) | S::While(test, _) | S::DoWhile(test, _) => vec![test],
        S::For(_, test, ..) => test.iter().collect(),
        S::Switch { value, cases, .. } => std::iter::once(value)
            .chain(cases.iter().filter_map(|(test, _)| test.as_ref()))
            .collect(),
        _ => vec![],
    }
}

/// Locals whose every mention lies inside one statement operand. An operand
/// holds no suspension point after normalization and is evaluated as a
/// whole, so such a local never carries a value across a point. Parameters
/// and closure-shared locals are never confined.
fn confined(body: &[S], params: usize, cells: &BTreeSet<u32>) -> Set {
    /// Unit of a statement-level mention, which is never confined.
    const STATEMENT: usize = usize::MAX;
    let mut units: BTreeMap<u32, BTreeSet<usize>> = BTreeMap::new();
    let mut next = 0usize;
    fn statements(body: &[S], units: &mut BTreeMap<u32, BTreeSet<usize>>, next: &mut usize) {
        for statement in body {
            statement_fields(statement, &mut |slot| {
                units.entry(slot).or_default().insert(STATEMENT);
            });
            for operand in operands(statement) {
                let unit = *next;
                *next += 1;
                expression(operand, &mut |mention| {
                    if let Mention::Local(slot) = mention {
                        units.entry(slot).or_default().insert(unit);
                    }
                });
            }
            for body in substatements(statement) {
                statements(body, units, next);
            }
        }
    }
    statements(body, &mut units, &mut next);
    units
        .into_iter()
        .filter(|(slot, units)| {
            units.len() == 1
                && !units.contains(&STATEMENT)
                && *slot as usize >= params
                && !cells.contains(slot)
        })
        .map(|(slot, _)| slot)
        .collect()
}
