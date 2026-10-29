//! ToBoolean reads immutable checked slots without allocating or calling a host.
use super::*;
use varta_common::value::ValueKind;
impl Emitter<'_> {
    fn boolean_string(&mut self) {
        self.code
            .extend([I::I64Const(32), I::I64ShrU, I::I64Eqz, I::I32Eqz]);
    }
    pub(super) fn boolean_conversion(&mut self, value: &Expr) -> Result<()> {
        self.expr(value)?;
        if value.ty.is_cell() {
            self.code
                .extend([I::I32WrapI64, I::LocalSet(self.temporary)]);
            let mut tags = vec![ValueKind::Float64, ValueKind::String, ValueKind::Boolean];
            if self.profile.bigint_values() {
                tags.push(ValueKind::BigInt);
            }
            for &tag in &tags {
                self.code.extend([
                    I::LocalGet(self.temporary),
                    I::I32Load(strings::WORD),
                    I::I32Const(tag as i32),
                    I::I32Eq,
                    I::If(BlockType::Result(ValType::I32)),
                    I::LocalGet(self.temporary),
                    I::I64Load(records::field(0)),
                ]);
                match tag {
                    ValueKind::Float64 => {
                        self.code.push(I::F64ReinterpretI64);
                        self.truthy(Type::Float64);
                    }
                    ValueKind::BigInt => self.code.extend([
                        I::I32WrapI64,
                        I::I32Load(strings::WORD),
                        I::I32Eqz,
                        I::I32Eqz,
                    ]),
                    ValueKind::String => self.boolean_string(),
                    ValueKind::Boolean => self.code.extend([I::I32WrapI64, I::I32Eqz, I::I32Eqz]),
                    _ => unreachable!(),
                }
                self.code.push(I::Else);
            }
            for tag in [ValueKind::Null, ValueKind::Undefined] {
                self.code.extend([
                    I::LocalGet(self.temporary),
                    I::I32Load(strings::WORD),
                    I::I32Const(tag as i32),
                    I::I32Ne,
                ]);
            }
            self.code.push(I::I32And);
            self.code.extend(std::iter::repeat_n(I::End, tags.len()));
        } else {
            match value.ty {
                Type::Float64 => self.truthy(value.ty),
                Type::BigInt => self.code.extend([
                    I::I32WrapI64,
                    I::I32Load(strings::WORD),
                    I::I32Eqz,
                    I::I32Eqz,
                ]),
                Type::Boolean | Type::Number => self.code.extend([I::I32Eqz, I::I32Eqz]),
                Type::String => self.boolean_string(),
                Type::Null | Type::Undefined => self.code.extend([I::Drop, I::I32Const(0)]),
                Type::Record(_)
                | Type::Dictionary(_)
                | Type::Class(_)
                | Type::Array(_)
                | Type::Function(_)
                | Type::Error
                | Type::NativeError(_) => self.code.extend([I::Drop, I::I32Const(1)]),
                _ => return Err(CompilerError::Internal("opaque value in ToBoolean".into())),
            }
        }
        Ok(())
    }
}
