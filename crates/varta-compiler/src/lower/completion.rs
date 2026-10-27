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
pub(super) fn exits_function(body: &[S]) -> bool {
    let flow = completions(body);
    flow.kinds & (NORMAL | BREAK | CONTINUE) == 0 && flow.branches.is_empty()
}
fn completions(body: &[S]) -> Flow {
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
            S::Block(body) | S::Scope(_, body) => completions(body),
            S::Label {
                id,
                iteration,
                body,
            } => {
                let mut inner = completions(body);
                if inner.branches.remove(&Branch::Break(*id)) {
                    inner.kinds |= NORMAL;
                }
                if *iteration && inner.branches.remove(&Branch::Continue(*id)) {
                    inner.kinds |= NORMAL;
                }
                inner
            }
            S::If(_, yes, no) => completions(yes)
                .union(completions(no))
                .union(Flow::new(THROW)),
            S::TryCatch { body, handler, .. } => {
                let mut inner = completions(body);
                let thrown = inner.kinds & THROW != 0;
                inner.kinds &= !THROW;
                if thrown {
                    inner.union(completions(handler))
                } else {
                    inner
                }
            }
            S::TryFinally {
                body, finalizer, ..
            } => {
                let mut finalizer = completions(finalizer);
                if finalizer.kinds & NORMAL != 0 {
                    finalizer.kinds &= !NORMAL;
                    finalizer.union(completions(body))
                } else {
                    finalizer
                }
            }
            S::DoWhile(_, body) => {
                let mut inner = completions(body);
                inner.kinds = (inner.kinds & (RETURN | THROW))
                    | if inner.kinds & (NORMAL | BREAK | CONTINUE) != 0 {
                        NORMAL | THROW
                    } else {
                        0
                    };
                inner
            }
            S::While(_, body) | S::For(_, _, _, body) => {
                let mut inner = completions(body);
                inner.kinds = NORMAL | THROW | (inner.kinds & RETURN);
                inner
            }
            S::Switch { cases, .. } => {
                cases
                    .iter()
                    .fold(Flow::new(NORMAL | THROW), |flow, (_, body)| {
                        let mut inner = completions(body);
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
