use super::*;
use varta_common::value::{self as values, ErrorClass};
impl Emitter<'_> {
    pub(super) fn new_error(&mut self, class: ErrorClass, value: &Expr) -> Result<()> {
        self.expr(value)?;
        self.code.extend([
            I::LocalSet(self.string_temporary),
            I::I32Const(values::ERROR_BYTES as i32),
        ]);
        self.string_helper(strings::Helper::Allocate)?;
        self.code.push(I::LocalSet(self.temporary));
        self.initialize_value(strings::ALLOCATED | values::HEAP_ERROR_FLAG, 1, 0, 1);
        self.code.extend([
            I::LocalGet(self.temporary),
            I::LocalGet(self.string_temporary),
            I::I32WrapI64,
            I::I32Load(strings::WORD),
            I::I32Const(values::ValueKind::String as i32),
            I::I32Eq,
            I::I32Const((class as i32) << values::ERROR_CLASS_SHIFT),
            I::I32Or,
            I::I32Store(strings::WORD),
            I::LocalGet(self.temporary),
            I::LocalGet(self.string_temporary),
            I::I32WrapI64,
            I::I64Load(records::field(0)),
            I::I64Store(records::field(0)),
        ]);
        self.pack_value(values::ERROR_BYTES);
        Ok(())
    }
    pub(super) fn error_pointer(&mut self, value: &Expr) -> Result<()> {
        self.expr(value)?;
        self.code.push(I::I32WrapI64);
        if value.ty.is_union() {
            self.code
                .extend([I::I64Load(records::field(0)), I::I32WrapI64]);
        }
        Ok(())
    }
    pub(super) fn error_name(&mut self, value: &Expr) -> Result<()> {
        self.error_pointer(value)?;
        self.code.push(I::LocalSet(self.temporary));
        self.error_class_text(false);
        Ok(())
    }
    // The object pointer is in temporary; names/prefixes are immutable static slices.
    fn error_class_text(&mut self, prefix: bool) {
        let dynamic = self.profile.error_families();
        if dynamic {
            for class in &ErrorClass::ALL[1..] {
                self.code.extend([
                    I::LocalGet(self.temporary),
                    I::I32Load(strings::WORD),
                    I::I32Const(values::ERROR_CLASS_SHIFT as i32),
                    I::I32ShrU,
                    I::I32Const(*class as i32),
                    I::I32Eq,
                    I::If(BlockType::Result(ValType::I64)),
                ]);
                self.error_static_text(*class, prefix);
                self.code.push(I::Else);
            }
        }
        self.error_static_text(ErrorClass::Error, prefix);
        if dynamic {
            for _ in &ErrorClass::ALL[1..] {
                self.code.push(I::End);
            }
        }
    }
    fn error_static_text(&mut self, class: ErrorClass, prefix: bool) {
        let text = if prefix {
            format!("{}: ", class.name())
        } else {
            class.name().to_owned()
        };
        let bytes = self.profile.string_bytes(&text);
        self.code.push(I::I64Const(
            ((bytes.len() as i64) << strings::LENGTH_SHIFT) | self.strings[&bytes] as i64,
        ));
    }
    pub(super) fn error_text(&mut self, value: &Expr) -> Result<()> {
        self.error_pointer(value)?;
        self.code.extend([
            I::LocalTee(self.temporary),
            I::I64Load(records::field(0)),
            I::LocalTee(self.string_temporary),
            I::I64Const(strings::LENGTH_SHIFT as i64),
            I::I64ShrU,
            I::I64Eqz,
            I::If(BlockType::Result(ValType::I64)),
        ]);
        self.error_class_text(false);
        self.code.push(I::Else);
        self.error_class_text(true);
        self.code.push(I::LocalGet(self.string_temporary));
        self.string_helper(strings::Helper::Concat)?;
        self.code.push(I::End);
        Ok(())
    }
}
