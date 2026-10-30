//! Read-only traversal of lowered IR, exhaustive so new forms decide their edges.
use super::*;

/// Visit `expr` and every nested expression, including statement bodies.
pub(super) fn expression(expr: &Expr, visit: &mut dyn FnMut(&Expr)) {
    visit(expr);
    match &expr.kind {
        E::Integer(_)
        | E::Float64(_)
        | E::String(_)
        | E::Local(_)
        | E::Status
        | E::ModuleConstant(_)
        | E::Zero
        | E::FunctionRef { .. } => {}
        E::Await {
            promise, result, ..
        } => [promise, result]
            .into_iter()
            .for_each(|v| expression(v, visit)),
        E::Schedule { args, .. } => args.iter().for_each(|value| expression(value, visit)),
        E::Assignment { value, body, .. } => {
            expression(value, visit);
            statements(body, visit);
        }
        E::ValueBlock { body, .. } => statements(body, visit),
        E::GeneratorStep {
            generator, value, ..
        } => [generator, value]
            .into_iter()
            .for_each(|v| expression(v, visit)),
        E::FunctionCheck { value, .. }
        | E::GeneratorValue(value)
        | E::Reinterpret(value)
        | E::Suspend { value, .. }
        | E::StringTrim { value, .. }
        | E::TextUnits { value, .. }
        | E::ArrayEnumerate { value, .. }
        | E::ArrayReserve { length: value, .. }
        | E::NewError(_, value)
        | E::ErrorName(value)
        | E::ErrorMessage(value)
        | E::ErrorText(value)
        | E::ErrorOwn(value)
        | E::BoxUnion(value)
        | E::UnboxUnion(value)
        | E::IsUndefined(value)
        | E::IsError(value)
        | E::IsClass { value, .. }
        | E::ArrayKind(value, _)
        | E::IsNullish(value)
        | E::IsUnionMember(value, _)
        | E::Undefined(value)
        | E::Negative(value)
        | E::ToBoolean(value)
        | E::Not(value)
        | E::Capture(value)
        | E::ResultField(_, value)
        | E::HasOwn(value, _)
        | E::Field(value, _)
        | E::Json(value, _)
        | E::ParseJson(value)
        | E::KvStage(_, value)
        | E::KvDecode(_, value)
        | E::ArrayPop(value)
        | E::ArrayReverse(value)
        | E::IndexKey(value)
        | E::Length(value) => expression(value, visit),
        E::FunctionCall {
            callee,
            ignored_receiver,
            args,
            ..
        } => {
            expression(callee, visit);
            if let Some(receiver) = ignored_receiver {
                expression(receiver, visit);
            }
            args.iter().for_each(|value| expression(value, visit));
        }
        E::StringSearch(_, a, b, c)
        | E::Conditional(a, b, c)
        | E::ArraySearch(_, a, b, c)
        | E::ArraySlice(a, b, c)
        | E::StringRange {
            value: a,
            start: b,
            end: c,
            ..
        }
        | E::StringRepeat {
            value: a,
            count: b,
            error: c,
            ..
        } => [a, b, c].into_iter().for_each(|v| expression(v, visit)),
        E::UnionLogical(_, a, b)
        | E::Sequence(a, b)
        | E::Binary(_, a, b)
        | E::NumberSameValue(a, b)
        | E::MaybeIndex(a, b)
        | E::ArrayHasOwn(a, b)
        | E::ArrayStringHasOwn(a, b)
        | E::ArrayPush(a, b)
        | E::ArrayAt(a, b)
        | E::Index(a, b)
        | E::RecordDelete {
            object: a,
            absent: b,
            ..
        } => [a, b].into_iter().for_each(|v| expression(v, visit)),
        E::ArrayFill(a, b, c, d) => [a, b, c, d].into_iter().for_each(|v| expression(v, visit)),
        E::Math(_, args)
        | E::NumberPredicate(_, args)
        | E::Intrinsic(_, args)
        | E::RegExp(_, args)
        | E::Call(_, args)
        | E::Host(_, args)
        | E::CheckedBigIntHost { args, .. } => {
            args.iter().for_each(|value| expression(value, visit))
        }
        E::Record(fields) | E::OptionalRecord(fields, _) => fields
            .iter()
            .for_each(|(_, value)| expression(value, visit)),
        E::Array(elements) => elements
            .iter()
            .flatten()
            .for_each(|value| expression(value, visit)),
        E::ArrayDelete {
            array,
            index,
            error,
        } => {
            expression(array, visit);
            index.iter().chain(error).for_each(|v| expression(v, visit));
        }
    }
}

pub(super) fn statements(body: &[S], visit: &mut dyn FnMut(&Expr)) {
    for statement in body {
        self::statement(statement, visit);
    }
}

fn statement(statement: &S, visit: &mut dyn FnMut(&Expr)) {
    match statement {
        S::Trap
        | S::Rebind(_)
        | S::RecordRest { .. }
        | S::BreakLabel(_)
        | S::ContinueLabel(_)
        | S::Break
        | S::Continue
        | S::Return(None) => {}
        S::Throw(value)
        | S::Init(_, value)
        | S::Set(_, value)
        | S::InitModuleConstant(_, value)
        | S::Expr(value)
        | S::Return(Some(value)) => expression(value, visit),
        S::TryCatch { body, handler, .. } => {
            statements(body, visit);
            statements(handler, visit);
        }
        S::TryFinally {
            body, finalizer, ..
        } => {
            statements(body, visit);
            statements(finalizer, visit);
        }
        S::RecordEnumerate { values, .. } => values.iter().for_each(|v| expression(v, visit)),
        S::SetField(object, _, value) => {
            expression(object, visit);
            expression(value, visit);
        }
        S::ArrayLength {
            array,
            length,
            error,
        } => [array, length, error]
            .into_iter()
            .for_each(|v| expression(v, visit)),
        S::SetIndex {
            array,
            index,
            value,
            ..
        } => [array, index, value]
            .into_iter()
            .for_each(|v| expression(v, visit)),
        S::Block(body) | S::Scope(_, body) | S::Label { body, .. } => statements(body, visit),
        S::If(test, yes, no) => {
            expression(test, visit);
            statements(yes, visit);
            statements(no, visit);
        }
        S::While(test, body) | S::DoWhile(test, body) => {
            expression(test, visit);
            statements(body, visit);
        }
        S::For(init, test, update, body) => {
            statements(init, visit);
            if let Some(test) = test {
                expression(test, visit);
            }
            statements(update, visit);
            statements(body, visit);
        }
        S::Switch { value, cases, .. } => {
            expression(value, visit);
            for (test, body) in cases {
                if let Some(test) = test {
                    expression(test, visit);
                }
                statements(body, visit);
            }
        }
    }
}

/// A suspension point, or an `await` (v34) that normalization turns into one.
pub(super) fn point(expr: &Expr) -> bool {
    matches!(expr.kind, E::Suspend { .. } | E::Await { .. })
}

pub(super) fn suspends(expr: &Expr) -> bool {
    let mut found = false;
    expression(expr, &mut |e| found |= point(e));
    found
}

pub(super) fn body_suspends(body: &[S]) -> bool {
    let mut found = false;
    statements(body, &mut |e| found |= point(e));
    found
}
