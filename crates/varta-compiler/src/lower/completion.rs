//! Conservative possible completions, including finalizer overrides.
use super::S;
const NORMAL: u8 = 1;
const RETURN: u8 = 2;
const THROW: u8 = 4;
const BREAK: u8 = 8;
const CONTINUE: u8 = 16;

pub(super) fn exits_function(body: &[S]) -> bool {
    completions(body) & (NORMAL | BREAK | CONTINUE) == 0
}
fn completions(body: &[S]) -> u8 {
    let mut flow = NORMAL;
    for statement in body {
        if flow & NORMAL == 0 {
            break;
        }
        let next = match statement {
            S::Return(_) => RETURN | THROW,
            S::Trap | S::Throw(_) => THROW,
            S::Break => BREAK,
            S::Continue => CONTINUE,
            S::Block(b) | S::Scope(_, b) => completions(b),
            S::If(_, a, b) => completions(a) | completions(b) | THROW,
            S::TryCatch { body, handler, .. } => {
                let body = completions(body);
                (body & !THROW)
                    | if body & THROW != 0 {
                        completions(handler)
                    } else {
                        0
                    }
            }
            S::TryFinally {
                body, finalizer, ..
            } => {
                let finalizer = completions(finalizer);
                (finalizer & !NORMAL)
                    | if finalizer & NORMAL != 0 {
                        completions(body)
                    } else {
                        0
                    }
            }
            // Loops and switches may finish normally; preserve function exits and
            // continue through switches to the enclosing loop.
            S::While(_, b) | S::For(_, _, _, b) => NORMAL | THROW | (completions(b) & RETURN),
            S::Switch { cases, .. } => cases.iter().fold(NORMAL | THROW, |flow, (_, body)| {
                flow | (completions(body) & (RETURN | THROW | CONTINUE))
            }),
            _ => NORMAL | THROW,
        };
        flow = (flow & !NORMAL) | next;
    }
    flow
}
