//! V38 (`codegen_v2`): a chain of string `+` concatenates its operands with
//! one allocation per [`strings::MAX_CONCATENATED`] operands instead of one
//! per `+`. Operands are evaluated in source order exactly as the nested
//! chain evaluates them; intermediate results of a longer chain are rooted
//! before the next operands run.
use super::*;

/// The operands of the string `+` chain `expr`, left to right.
pub(super) fn leaves(expr: &Expr) -> Vec<&Expr> {
    fn walk<'e>(expr: &'e Expr, out: &mut Vec<&'e Expr>) {
        match &expr.kind {
            E::Binary(BinaryOp::Add, left, right) if left.ty == Type::String => {
                walk(left, out);
                walk(right, out);
            }
            _ => out.push(expr),
        }
    }
    let mut out = Vec::new();
    walk(expr, &mut out);
    out
}

/// The operand count of every helper call for `leaves` operands: the first
/// takes leading operands, each later one the previous result and more.
pub(super) fn steps(leaves: usize) -> Vec<usize> {
    let first = leaves.min(strings::MAX_CONCATENATED);
    let mut steps = vec![first];
    let mut remaining = leaves - first;
    while remaining > 0 {
        let more = remaining.min(strings::MAX_CONCATENATED - 1);
        steps.push(more + 1);
        remaining -= more;
    }
    steps
}

/// The helper concatenating `operands` operands.
pub(super) fn helper(operands: usize) -> strings::Helper {
    if operands == 2 {
        strings::Helper::Concat
    } else {
        strings::Helper::ConcatMany(operands as u8)
    }
}

impl Emitter<'_> {
    /// `[] -> [i64]`: the chain `expr` (at least two operands).
    pub(super) fn concat_chain(&mut self, expr: &Expr) -> Result<()> {
        let leaves = leaves(expr);
        let steps = steps(leaves.len());
        let mut next = 0;
        for (index, step) in steps.iter().enumerate() {
            let own = if index == 0 { *step } else { step - 1 };
            for leaf in &leaves[next..next + own] {
                self.expr(leaf)?;
            }
            next += own;
            self.string_helper(helper(*step))?;
            // Later operands may allocate; stable chains have one step.
            if index + 1 < steps.len() && !self.quiet_mode {
                self.root_result(Type::String)?;
            }
        }
        Ok(())
    }
}
