//! Possible completions retain named targets until their own label consumes them.
use super::S;
use std::collections::BTreeSet;
const NORMAL: u8 = 1;
const RETURN: u8 = 2;
const THROW: u8 = 4;
const BREAK: u8 = 8;
const CONTINUE: u8 = 16;
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Branch {
    Break(u32),
    Continue(u32),
}
struct Flow {
    kinds: u8,
    branches: BTreeSet<Branch>,
}
impl Flow {
    fn new(kinds: u8) -> Self {
        Self {
            kinds,
            branches: BTreeSet::new(),
        }
    }
    fn branch(target: Branch) -> Self {
        Self {
            kinds: 0,
            branches: BTreeSet::from([target]),
        }
    }
    fn union(mut self, other: Self) -> Self {
        self.kinds |= other.kinds;
        self.branches.extend(other.branches);
        self
    }
}
/// Which completion refinements apply.
#[derive(Clone, Copy, Default)]
struct Mode {
    /// V36: exhaustive switches (see [`exits_function_through_switches`]).
    switches: bool,
    /// V37 (`language_core`): a loop without a test (or with the literal
    /// `true`) completes normally only through a `break` that targets it.
    loops: bool,
}
pub(super) fn normal(body: &[S]) -> bool {
    completions(body, Mode::default()).kinds & NORMAL != 0
}
pub(super) fn exits_function(body: &[S]) -> bool {
    exits(body, Mode::default())
}
/// [`exits_function`] with exhaustive switches (v36): a `switch` with a
/// `default` clause completes normally only through a `break` or by falling
/// off its last clause, as TypeScript's return analysis decides.
pub(super) fn exits_function_through_switches(body: &[S]) -> bool {
    exits(
        body,
        Mode {
            switches: true,
            loops: false,
        },
    )
}
/// [`exits_function_through_switches`] that also knows infinite loops (v37),
/// as TypeScript's return analysis: `for (;;) { ... return x; }`.
pub(super) fn exits_function_through_loops(body: &[S]) -> bool {
    exits(
        body,
        Mode {
            switches: true,
            loops: true,
        },
    )
}
/// Whether a loop test is absent or the literal `true`.
fn infinite(test: Option<&super::Expr>) -> bool {
    test.is_none_or(|test| {
        test.ty == super::Type::Boolean && matches!(test.kind, super::E::Integer(1))
    })
}
fn exits(body: &[S], switches: Mode) -> bool {
    let flow = completions(body, switches);
    flow.kinds & (NORMAL | BREAK | CONTINUE) == 0 && flow.branches.is_empty()
}
fn completions(body: &[S], switches: Mode) -> Flow {
    let mut flow = Flow::new(NORMAL);
    for statement in body {
        if flow.kinds & NORMAL == 0 {
            break;
        }
        let next = match statement {
            S::Return(_) => Flow::new(RETURN | THROW),
            S::Trap | S::Throw(_) => Flow::new(THROW),
            S::Break => Flow::new(BREAK),
            S::Continue => Flow::new(CONTINUE),
            S::BreakLabel(id) => Flow::branch(Branch::Break(*id)),
            S::ContinueLabel(id) => Flow::branch(Branch::Continue(*id)),
            S::Block(body) | S::Scope(_, body) => completions(body, switches),
            S::Label {
                id,
                iteration,
                body,
            } => {
                let mut inner = completions(body, switches);
                if inner.branches.remove(&Branch::Break(*id)) {
                    inner.kinds |= NORMAL;
                }
                if *iteration && inner.branches.remove(&Branch::Continue(*id)) {
                    inner.kinds |= NORMAL;
                }
                inner
            }
            S::If(_, yes, no) => completions(yes, switches)
                .union(completions(no, switches))
                .union(Flow::new(THROW)),
            S::TryCatch { body, handler, .. } => {
                let mut inner = completions(body, switches);
                let thrown = inner.kinds & THROW != 0;
                inner.kinds &= !THROW;
                if thrown {
                    inner.union(completions(handler, switches))
                } else {
                    inner
                }
            }
            S::TryFinally {
                body, finalizer, ..
            } => {
                let mut finalizer = completions(finalizer, switches);
                if finalizer.kinds & NORMAL != 0 {
                    finalizer.kinds &= !NORMAL;
                    finalizer.union(completions(body, switches))
                } else {
                    finalizer
                }
            }
            S::DoWhile(_, body) => {
                let mut inner = completions(body, switches);
                inner.kinds = (inner.kinds & (RETURN | THROW))
                    | if inner.kinds & (NORMAL | BREAK | CONTINUE) != 0 {
                        NORMAL | THROW
                    } else {
                        0
                    };
                inner
            }
            S::While(test, body) if switches.loops && infinite(Some(test)) => {
                endless(completions(body, switches))
            }
            S::For(_, test, _, body) if switches.loops && infinite(test.as_ref()) => {
                endless(completions(body, switches))
            }
            S::While(_, body) | S::For(_, _, _, body) => {
                let mut inner = completions(body, switches);
                inner.kinds = NORMAL | THROW | (inner.kinds & RETURN);
                inner
            }
            S::Switch { cases, .. } if switches.switches => {
                let exhaustive = cases.iter().any(|(test, _)| test.is_none());
                let mut flow = Flow::new(THROW);
                let mut ends = !exhaustive || cases.is_empty();
                for (index, (_, body)) in cases.iter().enumerate() {
                    let mut inner = completions(body, switches);
                    let last = index + 1 == cases.len();
                    ends |= inner.kinds & BREAK != 0 || last && inner.kinds & NORMAL != 0;
                    inner.kinds &= RETURN | THROW | CONTINUE;
                    flow = flow.union(inner);
                }
                if ends {
                    flow.kinds |= NORMAL;
                }
                flow
            }
            S::Switch { cases, .. } => {
                cases
                    .iter()
                    .fold(Flow::new(NORMAL | THROW), |flow, (_, body)| {
                        let mut inner = completions(body, switches);
                        inner.kinds &= RETURN | THROW | CONTINUE;
                        flow.union(inner)
                    })
            }
            _ => Flow::new(NORMAL | THROW),
        };
        flow.kinds &= !NORMAL;
        flow = flow.union(next);
    }
    flow
}
/// The completions of an endless loop with `body`: only a `break` of the
/// loop itself ends it normally; labelled branches still propagate.
fn endless(mut inner: Flow) -> Flow {
    let broken = inner.kinds & BREAK != 0;
    inner.kinds = THROW | (inner.kinds & RETURN) | if broken { NORMAL } else { 0 };
    inner
}
