//! Weak references (v38 `weak_references`, K9 D5): the `varta.weak.*`
//! ephemeron-table intrinsics of the weak prelude (`prelude/weak.ts`) and
//! the test-only `gc()`.
//!
//! The prelude implements `WeakMap`, `WeakSet`, `WeakRef` and
//! `FinalizationRegistry` as typed guest code over tables of weak pairs
//! (`varta_common::weak`): a table is a `WeakTable` value that only these
//! intrinsics create and read, a key is any value of an object or symbol
//! type (a cell is read through to its member), stored as the identity
//! address of its heap block, and a value keeps the type the prelude names.
//! The collector treats the pairs as ephemerons (`emit::strings::weak`).
//! The intrinsics exist only inside engine preludes; in program modules the
//! names are ordinary unknown calls.
use super::*;
use ir::WeakOp;
use varta_common::weak as w;

/// The table class of the weak prelude.
const TABLE_CLASS: &str = "WeakTable";

/// The intrinsic names, their operation and operand count.
const INTRINSICS: [(&str, WeakOp, usize); 16] = [
    (TABLE, WeakOp::Table(0), 1),
    ("varta.weak.kept", WeakOp::Table(w::KIND_STRONG), 1),
    ("varta.weak.targets", WeakOp::Table(w::KIND_NOTIFY), 1),
    ("varta.weak.like", WeakOp::Like, 2),
    ("varta.weak.capacity", WeakOp::Word(w::CAPACITY_WORD), 1),
    ("varta.weak.used", WeakOp::Word(w::USED_WORD), 1),
    ("varta.weak.state", WeakOp::State, 2),
    ("varta.weak.hash", WeakOp::Hash, 1),
    ("varta.weak.slotHash", WeakOp::SlotHash, 2),
    ("varta.weak.holds", WeakOp::Holds, 3),
    ("varta.weak.value", WeakOp::Value, 2),
    ("varta.weak.put", WeakOp::Put, 4),
    ("varta.weak.remove", WeakOp::Remove, 2),
    ("varta.weak.move", WeakOp::Move, 4),
    ("varta.weak.clear", WeakOp::Clear, 1),
    ("varta.weak.pending", WeakOp::Pending, 0),
];
/// `table<V>(capacity)`: an ephemeron table whose values are `V`s.
const TABLE: &str = "varta.weak.table";
/// `(first: WeakKey, second) -> boolean` ([`WeakOp::Same`]).
const SAME: &str = "varta.weak.same";
/// `(error) -> void`: report an exception a cleanup callback threw as
/// uncaught, like a timer callback's (the instance fails).
const UNCAUGHT: &str = "varta.weak.uncaught";
/// The test-only global of [`with_exposed_gc`].
const GC: &str = "gc";

/// Whether a value of `ty` can be a weak key: an object or symbol type,
/// or a cell whose members are (checked again when it is read).
pub(super) fn key_type(ty: Type) -> bool {
    ty.is_handle() && !matches!(ty, Type::String | Type::BigInt)
}

thread_local! {
    static EXPOSED_GC: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}
/// Test-only: compile inside `body` with a global `gc()` that runs a
/// complete collection, as Node's `--expose-gc` does, so a weak-reference
/// program states where its collections happen.
pub(crate) fn with_exposed_gc<T>(body: impl FnOnce() -> T) -> T {
    let previous = EXPOSED_GC.with(|cell| cell.replace(true));
    let result = body();
    EXPOSED_GC.with(|cell| cell.set(previous));
    result
}

/// The weak prelude's checkpoint function, when the program loaded it.
pub(super) fn checkpoint_function(types: &ir::ValueTypes) -> Option<usize> {
    if !types.profile.weak_references() {
        return None;
    }
    let prelude = &crate::modules::engine::WEAK;
    engine::module(prelude, types)?;
    types
        .engine
        .function(prelude, crate::modules::engine::WEAK_CHECKPOINT)
        .map(|signature| signature.index)
}

impl Lower<'_> {
    /// The `WeakTable` class type of the loaded weak prelude.
    fn weak_table(&self) -> Result<Type> {
        self.types
            .classes
            .prelude_class(TABLE_CLASS)
            .map(Type::Class)
            .ok_or_else(|| CompilerError::Internal("the weak prelude is not loaded".into()))
    }
    /// A `varta.weak.*` intrinsic call of the weak prelude, if it is one.
    pub(super) fn weak_intrinsic(
        &mut self,
        call: &ast::CallExpr,
        name: &str,
    ) -> Result<Option<Expr>> {
        if !self.types.profile.weak_references() || !name.starts_with("varta.weak.") {
            return Ok(None);
        }
        if name == UNCAUGHT {
            let [error] = call.args.as_slice() else {
                return Err(invalid(format!("{name} expects the caught value")));
            };
            let error = self.expr(&error.expr)?;
            return Ok(Some(Expr {
                span: call.span,
                ty: Type::Void,
                kind: E::Host(Host::TimerUncaught, vec![error]),
            }));
        }
        let op = if name == SAME {
            Some((WeakOp::Same, 2))
        } else {
            INTRINSICS
                .iter()
                .find(|(intrinsic, ..)| *intrinsic == name)
                .map(|(_, op, count)| (*op, *count))
        };
        let Some((op, count)) = op else {
            return Err(invalid(format!("unknown weak intrinsic {name}")));
        };
        if call.args.len() != count || call.args.iter().any(|arg| arg.spread.is_some()) {
            return Err(invalid(format!("{name} expects {count} plain arguments")));
        }
        // `table<V>` and `value<V>` name the value type of their table.
        let named = match &call.type_args {
            Some(params) => match params.params.as_slice() {
                [ty] if name == TABLE || op == WeakOp::Value => Some(keyword_type(ty, self.types)?),
                _ => return Err(invalid(format!("{name} takes no such type arguments"))),
            },
            None => None,
        };
        let args = call
            .args
            .iter()
            .map(|arg| self.expr(&arg.expr))
            .collect::<Result<Vec<_>>>()?;
        let table = self.weak_table()?;
        let number = self.types.number();
        let is = |at: usize, ty: Type| args.get(at).is_some_and(|arg| arg.ty == ty);
        let (op, ty, valid) = match op {
            WeakOp::Table(_) if name == TABLE => {
                let Some(value) = named else {
                    return Err(invalid(format!("{name} names its value type")));
                };
                let kind = if value.is_handle() { w::KIND_VALUES } else { 0 };
                (WeakOp::Table(kind), table, is(0, number))
            }
            WeakOp::Table(_) => (op, table, is(0, number)),
            WeakOp::Like => (op, table, is(0, table) && is(1, number)),
            WeakOp::Word(_) => (op, number, is(0, table)),
            WeakOp::State | WeakOp::SlotHash => (op, number, is(0, table) && is(1, number)),
            WeakOp::Hash => (op, number, key_type(args[0].ty)),
            WeakOp::Holds => (
                op,
                Type::Boolean,
                is(0, table) && is(1, number) && key_type(args[2].ty),
            ),
            WeakOp::Value => {
                let Some(value) = named else {
                    return Err(invalid(format!("{name} names its value type")));
                };
                if value.width() != 1 {
                    return Err(unsupported(&format!("weak table value type {value}")));
                }
                (op, value, is(0, table) && is(1, number))
            }
            WeakOp::Put => (
                op,
                Type::Void,
                is(0, table) && is(1, number) && key_type(args[2].ty) && args[3].ty.width() == 1,
            ),
            WeakOp::Remove => (op, Type::Void, is(0, table) && is(1, number)),
            WeakOp::Move => (
                op,
                Type::Void,
                is(0, table) && is(1, number) && is(2, table) && is(3, number),
            ),
            WeakOp::Clear => (op, Type::Void, is(0, table)),
            WeakOp::Pending => (op, Type::Boolean, true),
            WeakOp::Same => (op, Type::Boolean, key_type(args[0].ty)),
            WeakOp::Collect => (op, Type::Void, false),
        };
        if !valid {
            return Err(type_error(format!("invalid operands for {name}")));
        }
        Ok(Some(Expr {
            span: call.span,
            ty,
            kind: E::Intrinsic(ir::Intrinsic::Weak(op), args),
        }))
    }

    /// The calls this module lowers before the general call lowering: the
    /// typed `varta.weak.table<V>` and `value<V>` of the weak prelude, and
    /// the test-only `gc()`. Kept out of line, so the recursive call
    /// lowering's frame does not grow.
    #[inline(never)]
    pub(super) fn weak_call(&mut self, call: &ast::CallExpr) -> Result<Option<Expr>> {
        let ast::Callee::Expr(callee) = &call.callee else {
            return Ok(None);
        };
        if !self.types.profile.weak_references() {
            return Ok(None);
        }
        let Some(name) = expr::callee_name(callee) else {
            return Ok(None);
        };
        if call.type_args.is_some() && crate::modules::engine::is_prelude_source(self.source) {
            return self.weak_intrinsic(call, &name);
        }
        self.exposed_gc(call, &name)
    }

    /// The test-only `gc()` of [`with_exposed_gc`] (v38): a complete
    /// collection where no binding shadows the name.
    fn exposed_gc(&mut self, call: &ast::CallExpr, name: &str) -> Result<Option<Expr>> {
        if name != GC
            || !self.types.profile.weak_references()
            || !EXPOSED_GC.with(std::cell::Cell::get)
            || self.binding(GC).is_ok()
        {
            return Ok(None);
        }
        if !call.args.is_empty() || call.type_args.is_some() {
            return Err(invalid("gc() takes no arguments"));
        }
        Ok(Some(Expr {
            span: call.span,
            ty: Type::Void,
            kind: E::Intrinsic(ir::Intrinsic::Weak(WeakOp::Collect), vec![]),
        }))
    }
}

#[cfg(test)]
mod tests {
    const WEAK: &str = include_str!("../prelude/weak.ts");

    /// The prelude's mirrored table constants match the shared registry.
    #[test]
    fn the_weak_prelude_mirrors_the_registry() {
        for line in [
            format!("const MIN_PAIRS = {};", varta_common::weak::MIN_PAIRS),
            format!(
                "const MAX_PAIRS = {};",
                varta_common::limits::V38_MAX_ARRAY_ELEMENTS
            ),
        ] {
            assert!(WEAK.contains(&line), "weak.ts lacks `{line}`");
        }
    }
}
