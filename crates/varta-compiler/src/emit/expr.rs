use super::*;

impl Emitter<'_> {
    pub(super) fn expr(&mut self, expr: &Expr) -> Result<()> {
        let Some(memory) = self.memory else {
            return self.expr_inner(expr);
        };
        let marker = self.root_mark_base + self.expr_depth;
        self.expr_depth += 1;
        self.max_expr_depth = self.max_expr_depth.max(self.expr_depth);
        self.code
            .extend([I::GlobalGet(memory.roots), I::LocalSet(marker)]);
        self.expr_inner(expr)?;
        self.code
            .extend([I::LocalGet(marker), I::GlobalSet(memory.roots)]);
        if expr.ty.needs_root() {
            if expr.ty == Type::ResultString {
                self.code.push(I::LocalSet(self.temporary));
            }
            self.string_helper(strings::Helper::RootPush)?;
            if expr.ty == Type::ResultString {
                self.code.push(I::LocalGet(self.temporary));
            }
        }
        self.expr_depth -= 1;
        Ok(())
    }
    fn expr_inner(&mut self, expr: &Expr) -> Result<()> {
        match &expr.kind {
            E::ValueBlock {
                output,
                marker,
                roots,
                body,
            } => self.value_block(expr.ty, *output, *marker, roots, body)?,
            E::Assignment {
                value,
                input,
                marker,
                body,
            } => self.assignment_value(value, *input, *marker, body)?,
            E::FunctionCheck { value, slot } => self.check_function_value(value, *slot)?,
            E::FunctionRef {
                index,
                fresh,
                captures,
            } => self.function_reference(*index, *fresh, captures, expr.ty)?,
            E::FunctionCall {
                callee,
                ignored_receiver,
                args,
                slot,
                arguments,
                output,
            } => self.function_call(
                callee,
                ignored_receiver.as_deref(),
                args,
                *slot,
                arguments,
                *output,
            )?,
            E::StringSearch(mode, value, needle, position) => {
                self.expr(value)?;
                self.expr(needle)?;
                self.array_bound(
                    position,
                    match *mode {
                        ir::StringSearch::EndsWith | ir::StringSearch::LastIndexOf => f64::INFINITY,
                        _ => 0.0,
                    },
                )?;
                self.string_helper(strings::Helper::string_search(*mode))?;
                if matches!(
                    *mode,
                    ir::StringSearch::IndexOf | ir::StringSearch::LastIndexOf
                ) {
                    self.code.push(I::F64ConvertI32S);
                }
            }
            E::StringRange {
                value,
                start,
                end,
                substring,
            } => {
                self.expr(value)?;
                self.array_bound(start, 0.0)?;
                self.array_bound(end, f64::INFINITY)?;
                self.string_helper(if *substring {
                    strings::Helper::StringSubstring
                } else {
                    strings::Helper::StringSlice
                })?;
            }
            E::StringTrim { value, mode } => {
                self.expr(value)?;
                self.string_helper(strings::Helper::string_trim(*mode))?;
            }
            E::StringRepeat {
                value,
                count,
                error,
                length,
                filled,
            } => self.string_repeat(value, count, error, *length, *filled)?,
            E::NewError(class, value) => self.new_error(*class, value)?,
            E::ErrorName(value) => self.error_name(value)?,
            E::ErrorMessage(value) => {
                self.error_pointer(value)?;
                self.code.push(I::I64Load(records::field(0)));
            }
            E::ErrorText(value) => self.error_text(value)?,
            E::ErrorOwn(value) => {
                self.error_pointer(value)?;
                self.code.extend([
                    I::I32Load(strings::WORD),
                    I::I32Const(varta_common::value::ERROR_OWN_MESSAGE as i32),
                    I::I32And,
                ]);
            }
            E::BoxUnion(value) => {
                self.expr(value)?;
                self.box_union(value.ty)?;
            }
            E::UnboxUnion(value) => self.unbox_union(value, expr.ty)?,
            E::IsClass {
                value,
                shapes,
                slot,
            } => self.is_class(value, shapes, *slot)?,
            E::IsError(value) => {
                self.expr(value)?;
                if value.ty.is_union() {
                    self.code.extend([
                        I::I32WrapI64,
                        I::I32Load(strings::WORD),
                        I::I32Const(varta_common::value::ValueKind::Error as i32),
                        I::I32Eq,
                    ]);
                } else {
                    for _ in 0..value.ty.width() {
                        self.code.push(I::Drop);
                    }
                    self.code.push(I::I32Const(i32::from(value.ty.is_error())));
                }
            }
            E::IsNullish(value) => self.is_nullish(value)?,
            E::IsUnionMember(value, member) => self.is_union_member(value, *member)?,
            E::IsUndefined(value) => {
                self.expr(value)?;
                if value.ty.is_union() {
                    self.code.extend([
                        I::I32WrapI64,
                        I::I32Load(strings::WORD),
                        I::I32Const(varta_common::value::ValueKind::Undefined as i32),
                        I::I32Eq,
                    ]);
                } else {
                    for _ in 0..value.ty.width() {
                        self.code.push(I::Drop);
                    }
                    self.code.push(I::I32Const(i32::from(matches!(
                        value.ty,
                        Type::Undefined | Type::Void
                    ))));
                }
            }
            E::Conditional(test, yes, no) => {
                self.expr(test)?;
                self.truthy(test.ty);
                self.code
                    .push(I::If(BlockType::Result(wasm_types(expr.ty)[0])));
                self.expr(yes)?;
                self.code.push(I::Else);
                self.expr(no)?;
                self.code.push(I::End);
            }
            E::UnionLogical(op, a, b) => self.union_logical(*op, a, b)?,

            E::Undefined(value) => {
                self.expr(value)?;
                for _ in 0..value.ty.width() {
                    self.code.push(I::Drop);
                }
                self.code.push(I::I32Const(0));
            }
            E::Sequence(left, right) => {
                self.expr(left)?;
                for _ in 0..left.ty.width() {
                    self.code.push(I::Drop);
                }
                self.expr(right)?;
            }
            E::ArraySearch(mode, base, needle, from) => {
                self.expr(base)?;
                let kind = needle
                    .ty
                    .value_kind()
                    .ok_or_else(|| CompilerError::Internal("missing search value kind".into()))?;
                self.code.push(I::I32Const(kind as i32));
                self.expr(needle)?;
                self.pack_slot(needle.ty);
                self.array_bound(from, 0.0)?;
                self.code
                    .push(I::I32Const(i32::from(*mode == ir::ArraySearch::Includes)));
                if *mode == ir::ArraySearch::LastIndexOf {
                    self.string_helper(strings::Helper::ArrayLastIndex)?;
                } else {
                    self.string_helper(strings::Helper::ArraySearch)?;
                }
                if *mode == ir::ArraySearch::Includes {
                    self.code.extend([I::I32Const(0), I::I32GeS]);
                } else {
                    self.code.push(I::F64ConvertI32S);
                }
            }
            E::ArraySlice(base, start, end) => {
                self.expr(base)?;
                self.array_bound(start, 0.0)?;
                self.array_bound(end, f64::INFINITY)?;
                self.string_helper(strings::Helper::ArraySlice)?;
            }
            E::ArrayAt(base, index) => self.maybe_index(base, index, expr.ty, true)?,
            E::ArrayPush(base, items) => {
                self.expr(base)?;
                self.expr(items)?;
                self.string_helper(strings::Helper::ArrayPush)?;
                self.code.push(I::F64ConvertI32U);
            }
            E::ArrayPop(base) => self.array_pop(base, expr.ty)?,
            E::ArrayReverse(base) => {
                self.expr(base)?;
                self.string_helper(strings::Helper::ArrayReverse)?;
            }
            E::ArrayFill(base, value, start, end) => {
                self.expr(base)?;
                self.expr(value)?;
                self.pack_slot(value.ty);
                self.array_bound(start, 0.0)?;
                self.array_bound(end, f64::INFINITY)?;
                self.string_helper(strings::Helper::ArrayFill)?;
            }
            E::Array(elements) => self.array(expr.ty, elements)?,
            E::ArrayEnumerate { value, keys } => {
                self.expr(value)?;
                self.string_helper(if *keys {
                    strings::Helper::ArrayKeys
                } else {
                    strings::Helper::ArrayValues
                })?;
            }
            E::ArrayReserve {
                length,
                count,
                bytes,
            } => self.array_reserve(expr.ty, length, *count, *bytes)?,
            E::ArrayHasOwn(base, index) => self.array_has_own(base, index)?,
            E::ArrayStringHasOwn(base, key) => {
                self.expr(base)?;
                self.expr(key)?;
                self.string_helper(strings::Helper::ArrayStringHasOwn)?;
            }
            E::ArrayDelete {
                array,
                index,
                error,
            } => self.array_delete(array, index.as_deref(), error.as_deref())?,
            E::RecordDelete {
                object,
                absent,
                slot,
                scan,
                write,
                count,
            } => self.record_delete(object, absent, *slot, *scan, *write, *count)?,
            E::Index(array, index) => {
                self.expr(array)?;
                self.index(index)?;
                self.string_helper(strings::Helper::ArrayIndex)?;
                self.code.push(I::I64Load(strings::ROOT));
                self.unpack_slot(expr.ty);
            }
            E::Length(array) => {
                self.expr(array)?;
                if array.ty == Type::String {
                    self.code.extend([
                        I::I64Const(strings::LENGTH_SHIFT as i64),
                        I::I64ShrU,
                        I::I32WrapI64,
                        I::I32Const(1),
                        I::I32ShrU,
                    ]);
                } else {
                    self.code.extend([
                        I::I32WrapI64,
                        I::I32Load(wasm_encoder::MemArg {
                            offset: varta_common::value::ARRAY_LENGTH_OFFSET,
                            ..strings::WORD
                        }),
                    ]);
                }
                if expr.ty == Type::Float64 {
                    self.code.push(I::F64ConvertI32U);
                }
            }
            E::Record(fields) => self.record(expr.ty, fields, None)?,
            E::OptionalRecord(fields, present) => self.record(expr.ty, fields, Some(*present))?,
            E::HasOwn(value, slot) => self.has_own(value, *slot)?,
            E::MaybeIndex(base, index) => self.maybe_index(base, index, expr.ty, false)?,
            E::Field(value, index) => {
                self.expr(value)?;
                self.code
                    .extend([I::I32WrapI64, I::I64Load(records::field(*index))]);
                self.unpack_slot(expr.ty);
            }
            E::Json(value, status) => self.json(value, expr.ty, *status)?,
            E::ParseJson(text) => {
                self.expr(text)?;
                let (ptr, len) = self.schemas[&expr.ty];
                self.code.extend([
                    I::LocalTee(self.string_temporary),
                    I::I32WrapI64,
                    I::LocalGet(self.string_temporary),
                    I::I64Const(strings::LENGTH_SHIFT as i64),
                    I::I64ShrU,
                    I::I32WrapI64,
                    I::I32Const(ptr as i32),
                    I::I32Const(len as i32),
                    I::Call(self.hosts[&self.profile.json_parse()]),
                ]);
                if self.profile.recoverable_json() {
                    self.check_exception()?;
                }
                if self.binary64 {
                    self.unpack_slot(expr.ty);
                } else if expr.ty.is_handle() {
                    self.code.extend([
                        I::I64ExtendI32U,
                        I::GlobalGet(self.result_len),
                        I::I64ExtendI32U,
                        I::I64Const(strings::LENGTH_SHIFT as i64),
                        I::I64Shl,
                        I::I64Or,
                    ]);
                }
            }
            E::Integer(n) => self.code.push(I::I32Const(*n)),
            E::Float64(n) => self.code.push(I::F64Const((*n).into())),
            E::Local(slot) => self.read_binding(*slot, expr.ty),
            E::ModuleConstant(index) => {
                self.code.extend([
                    I::I32Const(self.constant_root(*index)),
                    I::I64Load(strings::ROOT),
                ]);
            }
            E::String(s) => {
                let packed = ((s.len() as u64) << strings::LENGTH_SHIFT) | self.strings[s] as u64;
                if expr.ty == Type::BigInt {
                    self.code.extend([
                        I::I32Const(self.strings[s] as i32),
                        I::I32Const(s.len() as i32),
                    ]);
                    self.string_helper(strings::Helper::Snapshot)?;
                    self.bigint_leaf();
                } else {
                    self.code.push(I::I64Const(packed as i64));
                }
            }
            E::Status => {
                self.code.push(I::GlobalGet(self.result_status));
                if expr.ty == Type::Float64 {
                    self.code.push(I::F64ConvertI32S);
                }
            }
            E::Capture(value) => {
                self.expr(value)?;
                if value.ty == Type::Void {
                    // Keep one payload slot for every result, including void.
                    self.code.push(I::I32Const(0));
                }
                self.code.push(I::GlobalGet(self.result_status));
            }
            E::ResultField(field, value) => {
                self.expr(value)?;
                match field {
                    ir::ResultField::Value => {
                        self.code.push(I::Drop); // status
                        if expr.ty == Type::Void {
                            self.code.push(I::Drop);
                        }
                    }
                    ir::ResultField::Status | ir::ResultField::Ok => {
                        self.code.push(I::LocalSet(self.temporary));
                        self.code.push(I::Drop); // payload
                        self.code.push(I::LocalGet(self.temporary));
                        if matches!(field, ir::ResultField::Ok) {
                            self.code.push(I::I32Eqz);
                        } else if expr.ty == Type::Float64 {
                            self.code.push(I::F64ConvertI32S);
                        }
                    }
                }
            }
            E::Negative(e) => {
                if e.ty == Type::Float64 {
                    self.expr(e)?;
                    self.code.push(I::F64Neg);
                } else {
                    self.code.push(I::I32Const(0));
                    self.expr(e)?;
                    self.code.push(I::I32Sub);
                }
            }
            E::Math(mode, args) => self.math_intrinsic(*mode, args)?,
            E::NumberPredicate(mode, args) => self.number_predicate(*mode, args)?,
            E::ToBoolean(e) => self.boolean_conversion(e)?,
            E::Not(e) => {
                self.expr(e)?;
                self.truthy(e.ty);
                self.code.push(I::I32Eqz);
            }
            E::Call(index, args) => {
                for arg in args {
                    self.expr(arg)?;
                }
                self.code
                    .push(I::Call(self.functions_begin + *index as u32));
                self.check_exception()?;
            }
            E::CheckedBigIntHost { host, args, status } => {
                for arg in args {
                    self.expr(arg)?;
                    if arg.ty == Type::String {
                        self.code.extend([
                            I::LocalTee(self.string_temporary),
                            I::I32WrapI64,
                            I::LocalGet(self.string_temporary),
                            I::I64Const(strings::LENGTH_SHIFT as i64),
                            I::I64ShrU,
                            I::I32WrapI64,
                        ]);
                    }
                }
                self.code.push(I::Call(self.hosts[host]));
                self.code.extend([
                    I::LocalSet(self.local_slots[*status as usize]),
                    I::LocalSet(self.temporary),
                ]);
                self.code.extend([
                    I::LocalGet(self.local_slots[*status as usize]),
                    I::I32Eqz,
                    I::If(BlockType::Result(wasm_encoder::ValType::I64)),
                    I::LocalGet(self.temporary),
                    I::GlobalGet(self.result_len),
                ]);
                self.string_helper(strings::Helper::Snapshot)?;
                if expr.ty == Type::BigInt {
                    self.bigint_leaf();
                }
                self.code.extend([I::Else, I::I64Const(0), I::End]);
            }
            E::Host(host, args) => {
                for arg in args {
                    self.expr(arg)?;
                    if arg.ty == Type::String {
                        // The internal packed value becomes the existing host ptr/len ABI.
                        self.code.push(I::LocalTee(self.string_temporary));
                        self.code.push(I::I32WrapI64);
                        self.code.push(I::LocalGet(self.string_temporary));
                        self.code.push(I::I64Const(strings::LENGTH_SHIFT as i64));
                        self.code.push(I::I64ShrU);
                        self.code.push(I::I32WrapI64);
                    }
                }
                self.code.push(I::Call(self.hosts[host]));
                if expr.ty == Type::Float64
                    && host.scalar_signature().1 == [varta_common::HostScalar::I32]
                {
                    self.code.push(I::F64ConvertI32S);
                }
                if expr.ty == Type::String {
                    self.code.push(I::GlobalGet(self.result_len));
                    self.string_helper(strings::Helper::Snapshot)?;
                }
            }
            E::NumberSameValue(left, right) => {
                // Lowering supplies only cached locals or unboxed cached locals.
                // NaNs compare equal regardless of payload, while raw bits split +/-0.
                self.expr(left)?;
                self.expr(left)?;
                self.code.push(I::F64Ne);
                self.expr(right)?;
                self.expr(right)?;
                self.code.extend([I::F64Ne, I::I32And]);
                self.expr(left)?;
                self.code.push(I::I64ReinterpretF64);
                self.expr(right)?;
                self.code.extend([I::I64ReinterpretF64, I::I64Eq, I::I32Or]);
            }
            E::Binary(op, left, right)
                if matches!(
                    left.ty,
                    Type::Function(_)
                        | Type::Error
                        | Type::NativeError(_)
                        | Type::Record(_)
                        | Type::Class(_)
                        | Type::Array(_)
                ) =>
            {
                self.expr(left)?;
                self.expr(right)?;
                self.code
                    .push(if matches!(op, BinaryOp::EqEq | BinaryOp::EqEqEq) {
                        I::I64Eq
                    } else {
                        I::I64Ne
                    });
            }
            E::Binary(op, left, right) if left.ty == Type::String => {
                self.expr(left)?;
                self.expr(right)?;
                if *op == BinaryOp::Add {
                    self.string_helper(strings::Helper::Concat)?;
                } else {
                    self.string_helper(strings::Helper::Compare)?;
                    self.code.push(I::I32Const(0));
                    self.code.push(match op {
                        BinaryOp::EqEq | BinaryOp::EqEqEq => I::I32Eq,
                        BinaryOp::NotEq | BinaryOp::NotEqEq => I::I32Ne,
                        BinaryOp::Lt => I::I32LtS,
                        BinaryOp::LtEq => I::I32LeS,
                        BinaryOp::Gt => I::I32GtS,
                        BinaryOp::GtEq => I::I32GeS,
                        _ => {
                            return Err(CompilerError::Internal(
                                "unlowered string operator".into(),
                            ));
                        }
                    });
                }
            }
            E::Binary(op, left, right)
                if matches!(op, BinaryOp::LogicalAnd | BinaryOp::LogicalOr) =>
            {
                self.expr(left)?;
                let temporary = if left.ty == Type::Float64 {
                    self.float_temporary
                } else {
                    self.temporary
                };
                self.code.push(I::LocalTee(temporary));
                self.truthy(left.ty);
                self.code
                    .push(I::If(BlockType::Result(if left.ty == Type::Float64 {
                        ValType::F64
                    } else {
                        ValType::I32
                    })));
                if *op == BinaryOp::LogicalAnd {
                    self.expr(right)?;
                } else {
                    self.code.push(I::LocalGet(temporary));
                }
                self.code.push(I::Else);
                if *op == BinaryOp::LogicalOr {
                    self.expr(right)?;
                } else {
                    self.code.push(I::LocalGet(temporary));
                }
                self.code.push(I::End);
            }
            E::Binary(op, a, b) if a.ty == Type::Float64 => {
                self.expr(a)?;
                self.expr(b)?;
                self.float_binary(*op)?;
            }
            E::Binary(op, a, b) => {
                self.expr(a)?;
                self.expr(b)?;
                self.code.push(match op {
                    BinaryOp::Add => I::I32Add,
                    BinaryOp::Sub => I::I32Sub,
                    BinaryOp::Mul => I::I32Mul,
                    BinaryOp::Div => I::I32DivS,
                    BinaryOp::Mod => I::I32RemS,
                    BinaryOp::Lt => I::I32LtS,
                    BinaryOp::LtEq => I::I32LeS,
                    BinaryOp::Gt => I::I32GtS,
                    BinaryOp::GtEq => I::I32GeS,
                    BinaryOp::EqEq | BinaryOp::EqEqEq => I::I32Eq,
                    BinaryOp::NotEq | BinaryOp::NotEqEq => I::I32Ne,
                    _ => return Err(CompilerError::Internal("unlowered binary operator".into())),
                });
            }
        }
        Ok(())
    }

    pub(super) fn string_helper(&mut self, helper: strings::Helper) -> Result<()> {
        let index = self
            .helpers
            .get(&helper)
            .ok_or_else(|| CompilerError::Internal("missing string helper".into()))?;
        self.code.push(I::Call(*index));
        Ok(())
    }
}

impl Emitter<'_> {
    /// Statements inside an expression may release only their own temporaries;
    /// older operands still live on the caller's Wasm stack and shadow roots.
    fn assignment_value(
        &mut self,
        value: &Expr,
        input: u32,
        marker: u32,
        body: &[S],
    ) -> Result<()> {
        let memory = self
            .memory
            .ok_or_else(|| CompilerError::Internal("missing assignment expression roots".into()))?;
        let marker = self.local_slots[marker as usize];
        self.code
            .extend([I::GlobalGet(memory.roots), I::LocalSet(marker)]);
        let prior = self.temporary_floor.replace(marker);
        self.scopes.push(vec![input]);
        self.expr(value)?;
        self.write_binding(input, value.ty, true)?;
        self.end_temporaries();
        self.statements(body)?;
        self.scratch_get(input, value.ty);
        // No allocation occurs between loading the result and its outer expr
        // wrapper retaining it. Exceptional exits clear the scope through the
        // ordinary handler/frame path and never publish this success result.
        self.clear_scopes(self.scopes.len() - 1);
        self.scopes.pop();
        self.temporary_floor = prior;
        Ok(())
    }
}

impl Emitter<'_> {
    fn value_block(
        &mut self,
        ty: Type,
        output: u32,
        marker: u32,
        roots: &[u32],
        body: &[S],
    ) -> Result<()> {
        let memory = self
            .memory
            .ok_or_else(|| CompilerError::Internal("missing value-block roots".into()))?;
        let marker = self.local_slots[marker as usize];
        self.code
            .extend([I::GlobalGet(memory.roots), I::LocalSet(marker)]);
        let prior = self.temporary_floor.replace(marker);
        self.scopes.push(roots.to_vec());
        self.statements(body)?;
        self.scratch_get(output, ty);
        // Loading precedes root cleanup without allocation; the enclosing expr
        // retains the success result. Handlers own all roots before RHS effects.
        self.clear_scopes(self.scopes.len() - 1);
        self.scopes.pop();
        self.temporary_floor = prior;
        Ok(())
    }
}

impl Emitter<'_> {
    fn bigint_leaf(&mut self) {
        self.code.extend([
            I::LocalTee(self.string_temporary),
            I::I32WrapI64,
            I::I32Const(varta_common::abi::STRING_HEADER_BYTES as i32),
            I::I32Sub,
            I::I32Const(
                (varta_common::value::HEAP_ALLOCATED_FLAG | varta_common::bigint::HEAP_FLAG) as i32,
            ),
            I::I32Store(strings::FLAG),
            I::LocalGet(self.string_temporary),
            I::I32WrapI64,
            I::I32Const(size_of::<u32>() as i32),
            I::I32Sub,
            I::I32Const(0),
            I::I32Store(strings::WORD),
            I::LocalGet(self.string_temporary),
        ]);
    }
}
