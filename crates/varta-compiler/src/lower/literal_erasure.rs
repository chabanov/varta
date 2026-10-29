//! Erase literal types after lowering (v32 `literal_types`). Every literal set
//! becomes its base primitive, and function types that become identical are
//! merged so signature dispatch and the capture catalog stay unambiguous.
use super::*;
use std::collections::BTreeMap;

/// Replace every literal type in the lowered program by its base primitive.
pub(super) fn erase_program(program: &mut ir::Program) {
    let types = &mut program.types;
    let eraser = Eraser {
        functions: merge_functions(&mut types.functions),
    };
    for members in &mut types.unions {
        for member in members.iter_mut() {
            *member = eraser.ty(*member);
        }
        members.sort();
    }
    for record in &mut types.records {
        for field in &mut record.fields {
            field.ty = eraser.ty(field.ty);
        }
    }
    for element in &mut types.arrays {
        *element = eraser.ty(*element);
    }
    for generator in &mut types.generators {
        for ty in [
            &mut generator.yielded,
            &mut generator.returned,
            &mut generator.next,
            &mut generator.result,
        ] {
            *ty = eraser.ty(*ty);
        }
    }
    for function in &mut program.functions {
        eraser.signature(
            &mut function.receiver,
            &mut function.params,
            &mut function.result,
        );
        for local in &mut function.locals {
            *local = eraser.ty(*local);
        }
        eraser.statements(&mut function.body);
    }
}

type FunctionKey = (Option<Type>, Vec<Type>, Type, Vec<bool>, bool);
fn function_key(function: &ir::FunctionType, map: &[u32]) -> FunctionKey {
    let rewrite = |ty: Type| match ty.erased() {
        Type::Function(id) => Type::Function(map[id as usize]),
        other => other,
    };
    (
        function.receiver.map(rewrite),
        function.params.iter().copied().map(rewrite).collect(),
        rewrite(function.result),
        function.shape.optional.clone(),
        function.shape.rest,
    )
}

/// Merge function types whose erased signatures coincide (to a fixpoint, since
/// signatures nest) and compact the table. Returns the old -> new id map.
fn merge_functions(functions: &mut Vec<ir::FunctionType>) -> Vec<u32> {
    let count = functions.len();
    let mut map: Vec<u32> = (0..count as u32).collect();
    loop {
        let mut first = BTreeMap::new();
        let next: Vec<u32> = functions
            .iter()
            .enumerate()
            .map(|(id, function)| {
                *first
                    .entry(function_key(function, &map))
                    .or_insert(id as u32)
            })
            .collect();
        if next == map {
            break;
        }
        map = next;
    }
    let mut compact = vec![0; count];
    let mut kept = Vec::new();
    for (id, function) in functions.iter().enumerate() {
        if map[id] as usize == id {
            compact[id] = kept.len() as u32;
            kept.push(function.clone());
        }
    }
    let map: Vec<u32> = map.iter().map(|rep| compact[*rep as usize]).collect();
    let eraser = Eraser { functions: map };
    for function in &mut kept {
        eraser.signature(
            &mut function.receiver,
            &mut function.params,
            &mut function.result,
        );
    }
    *functions = kept;
    eraser.functions
}

struct Eraser {
    functions: Vec<u32>,
}
impl Eraser {
    fn ty(&self, ty: Type) -> Type {
        match ty.erased() {
            Type::Function(id) => Type::Function(self.functions[id as usize]),
            other => other,
        }
    }
    fn signature(&self, receiver: &mut Option<Type>, params: &mut [Type], result: &mut Type) {
        if let Some(receiver) = receiver {
            *receiver = self.ty(*receiver);
        }
        for param in params {
            *param = self.ty(*param);
        }
        *result = self.ty(*result);
    }
    fn statements(&self, body: &mut [S]) {
        for statement in body {
            self.statement(statement);
        }
    }
    fn statement(&self, statement: &mut S) {
        match statement {
            S::Trap
            | S::Rebind(_)
            | S::RecordRest { .. }
            | S::BreakLabel(_)
            | S::ContinueLabel(_)
            | S::Break
            | S::Continue => {}
            S::Throw(value)
            | S::Init(_, value)
            | S::Set(_, value)
            | S::InitModuleConstant(_, value)
            | S::Expr(value) => {
                self.expression(value);
            }
            S::Return(value) => {
                if let Some(value) = value {
                    self.expression(value);
                }
            }
            S::TryCatch { body, handler, .. } => {
                self.statements(body);
                self.statements(handler);
            }
            S::TryFinally {
                body, finalizer, ..
            } => {
                self.statements(body);
                self.statements(finalizer);
            }
            S::RecordEnumerate { values, .. } => {
                values.iter_mut().for_each(|value| self.expression(value))
            }
            S::SetField(object, _, value) => {
                self.expression(object);
                self.expression(value);
            }
            S::ArrayLength {
                array,
                length,
                error,
            } => {
                self.expression(array);
                self.expression(length);
                self.expression(error);
            }
            S::SetIndex {
                array,
                index,
                value,
                ..
            } => {
                self.expression(array);
                self.expression(index);
                self.expression(value);
            }
            S::Block(body) | S::Scope(_, body) | S::Label { body, .. } => self.statements(body),
            S::If(test, yes, no) => {
                self.expression(test);
                self.statements(yes);
                self.statements(no);
            }
            S::While(test, body) | S::DoWhile(test, body) => {
                self.expression(test);
                self.statements(body);
            }
            S::For(init, test, update, body) => {
                self.statements(init);
                if let Some(test) = test {
                    self.expression(test);
                }
                self.statements(update);
                self.statements(body);
            }
            S::Switch { value, cases, .. } => {
                self.expression(value);
                for (test, body) in cases {
                    if let Some(test) = test {
                        self.expression(test);
                    }
                    self.statements(body);
                }
            }
        }
    }
    fn boxed(&self, values: &mut [&mut Box<Expr>]) {
        for value in values {
            self.expression(value);
        }
    }
    /// Exhaustive on purpose: a new IR form must decide how its types erase.
    fn expression(&self, value: &mut Expr) {
        value.ty = self.ty(value.ty);
        match &mut value.kind {
            E::Integer(_)
            | E::Float64(_)
            | E::String(_)
            | E::Local(_)
            | E::Status
            | E::ModuleConstant(_)
            | E::FunctionRef { .. } => {}
            E::Assignment { value, body, .. } => {
                self.expression(value);
                self.statements(body);
            }
            E::ValueBlock { body, .. } => self.statements(body),
            E::GeneratorStep {
                generator, value, ..
            } => self.boxed(&mut [generator, value]),
            E::FunctionCheck { value, .. }
            | E::GeneratorValue(value)
            | E::Suspend { value, .. }
            | E::StringTrim { value, .. }
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
            | E::IsNullish(value)
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
            | E::ArrayPop(value)
            | E::ArrayReverse(value)
            | E::IndexKey(value)
            | E::Length(value) => self.expression(value),
            E::IsUnionMember(value, member) => {
                *member = self.ty(*member);
                self.expression(value);
            }
            E::FunctionCall {
                callee,
                ignored_receiver,
                args,
                ..
            } => {
                self.expression(callee);
                if let Some(receiver) = ignored_receiver {
                    self.expression(receiver);
                }
                args.iter_mut().for_each(|value| self.expression(value));
            }
            E::StringSearch(_, a, b, c)
            | E::Conditional(a, b, c)
            | E::ArraySearch(_, a, b, c)
            | E::ArraySlice(a, b, c) => self.boxed(&mut [a, b, c]),
            E::StringRange {
                value, start, end, ..
            } => self.boxed(&mut [value, start, end]),
            E::StringRepeat {
                value,
                count,
                error,
                ..
            } => self.boxed(&mut [value, count, error]),
            E::UnionLogical(_, a, b)
            | E::Sequence(a, b)
            | E::Binary(_, a, b)
            | E::NumberSameValue(a, b)
            | E::MaybeIndex(a, b)
            | E::ArrayHasOwn(a, b)
            | E::ArrayStringHasOwn(a, b)
            | E::ArrayPush(a, b)
            | E::ArrayAt(a, b)
            | E::Index(a, b) => self.boxed(&mut [a, b]),
            E::RecordDelete { object, absent, .. } => self.boxed(&mut [object, absent]),
            E::ArrayFill(a, b, c, d) => self.boxed(&mut [a, b, c, d]),
            E::Math(_, args)
            | E::NumberPredicate(_, args)
            | E::Call(_, args)
            | E::Host(_, args)
            | E::CheckedBigIntHost { args, .. } => {
                args.iter_mut().for_each(|value| self.expression(value))
            }
            E::Record(fields) | E::OptionalRecord(fields, _) => {
                fields
                    .iter_mut()
                    .for_each(|(_, value)| self.expression(value));
            }
            E::Array(elements) => elements
                .iter_mut()
                .flatten()
                .for_each(|value| self.expression(value)),
            E::ArrayDelete {
                array,
                index,
                error,
            } => {
                self.expression(array);
                if let Some(index) = index {
                    self.expression(index);
                }
                if let Some(error) = error {
                    self.expression(error);
                }
            }
        }
    }
}
