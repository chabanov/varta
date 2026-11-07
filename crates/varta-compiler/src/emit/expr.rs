use super::*;

/// Host-side bookkeeping only: it must not change emitted fuel or root order.
enum ExpressionScope {
    Direct,
    Quiet { root: bool },
    Marked { marker: u32, root: bool },
}

impl Emitter<'_> {
    pub(super) fn expr(&mut self, expr: &Expr) -> Result<()> {
        self.expr_rooted(expr, true)
    }
    /// V38: evaluate a value whose consumer allocates nothing before using
    /// it, so the result itself needs no temporary root. Earlier profiles
    /// keep rooting it.
    pub(super) fn expr_unrooted(&mut self, expr: &Expr) -> Result<()> {
        self.expr_rooted(expr, !self.codegen_v2())
    }
    fn expr_rooted(&mut self, expr: &Expr, root: bool) -> Result<()> {
        if self.memory.is_some()
            && self.codegen_v2()
            && !self.quiet_mode
            && let E::Call(index, args) = &expr.kind
            && !self.frame_functions.contains_key(index)
            && args.iter().all(|arg| self.quiet(arg))
        {
            // Quiet arguments push nothing; the callee roots parameters.
            self.quiet_mode = true;
            let emitted = args.iter().try_for_each(|arg| self.expr_inner(arg));
            self.quiet_mode = false;
            emitted?;
            self.code
                .push(I::Call(self.functions_begin + *index as u32));
            self.check_exception()?;
            if root {
                self.root_result(expr.ty)?;
            }
            return Ok(());
        }
        let scope = self.begin_expression(expr, root);
        let emitted = self.expr_inner(expr);
        self.finish_expression(scope, expr.ty, emitted)
    }

    fn begin_expression(&mut self, expr: &Expr, root: bool) -> ExpressionScope {
        let Some(memory) = self.memory else {
            return ExpressionScope::Direct;
        };
        if self.codegen_v2() {
            if self.quiet_mode {
                return ExpressionScope::Direct;
            }
            if self.quiet(expr) || self.stable_operands(expr) || self.quiet_function_call(expr) {
                self.quiet_mode = true;
                return ExpressionScope::Quiet { root };
            }
        }
        let marker = self.root_mark_base + self.expr_depth;
        self.expr_depth += 1;
        self.max_expr_depth = self.max_expr_depth.max(self.expr_depth);
        self.code
            .extend([I::GlobalGet(memory.roots), I::LocalSet(marker)]);
        ExpressionScope::Marked { marker, root }
    }

    fn finish_expression(
        &mut self,
        scope: ExpressionScope,
        ty: Type,
        emitted: Result<()>,
    ) -> Result<()> {
        match scope {
            ExpressionScope::Direct => emitted,
            ExpressionScope::Quiet { root } => {
                self.quiet_mode = false;
                emitted?;
                if root {
                    self.root_result(ty)?;
                }
                Ok(())
            }
            ExpressionScope::Marked { marker, root } => {
                emitted?;
                let memory = self.memory.expect("a marked expression has managed memory");
                self.code
                    .extend([I::LocalGet(marker), I::GlobalSet(memory.roots)]);
                if root {
                    self.root_result(ty)?;
                }
                self.expr_depth -= 1;
                Ok(())
            }
        }
    }

    /// Lowering can build conditional spines much deeper than the source AST
    /// (for example a generated checked cast of a large union). Emit both
    /// branches with an explicit work stack, preserving the exact instruction
    /// and root-scope sequence of recursive emission, including old profiles.
    fn conditional(&mut self, test: &Expr, yes: &Expr, no: &Expr, ty: Type) -> Result<()> {
        enum Work<'a> {
            Branch(&'a Expr),
            Finish(ExpressionScope, Type),
            Else,
            End,
        }
        self.test(test)?;
        self.code.push(I::If(BlockType::Result(wasm_types(ty)[0])));
        let mut work = vec![Work::End, Work::Branch(no), Work::Else, Work::Branch(yes)];
        while let Some(next) = work.pop() {
            match next {
                Work::Else => self.code.push(I::Else),
                Work::End => self.code.push(I::End),
                Work::Finish(scope, ty) => self.finish_expression(scope, ty, Ok(()))?,
                Work::Branch(expr) => {
                    if let E::Conditional(test, yes, no) = &expr.kind {
                        let scope = self.begin_expression(expr, true);
                        self.test(test)?;
                        self.code
                            .push(I::If(BlockType::Result(wasm_types(expr.ty)[0])));
                        work.extend([
                            Work::Finish(scope, expr.ty),
                            Work::End,
                            Work::Branch(no),
                            Work::Else,
                            Work::Branch(yes),
                        ]);
                    } else {
                        self.expr(expr)?;
                    }
                }
            }
        }
        Ok(())
    }
    pub(super) fn root_result(&mut self, ty: Type) -> Result<()> {
        if ty.needs_root() {
            if ty == Type::ResultString {
                self.code.push(I::LocalSet(self.temporary));
            }
            self.string_helper(strings::Helper::RootPush)?;
            if ty == Type::ResultString {
                self.code.push(I::LocalGet(self.temporary));
            }
        }
        Ok(())
    }
    fn expr_inner(&mut self, expr: &Expr) -> Result<()> {
        if self.codegen_v2()
            && let Some(read) = dense::dense_read(expr)
        {
            return self.dense_index(&read, expr.ty);
        }
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
            E::Suspend { value, state, .. } => self.suspend_point(value, *state)?,
            E::GeneratorStep {
                generator,
                completion,
                value,
            } => self.generator_step(generator, *completion, value)?,
            E::GeneratorValue(generator) => self.generator_value(generator, expr.ty)?,
            E::Schedule {
                function,
                args,
                mode,
            } => self.schedule(*function, args, *mode)?,
            E::Reinterpret(value) => self.expr(value)?,
            E::Zero => self
                .code
                .extend(wasm_types(expr.ty).into_iter().map(|ty| match ty {
                    ValType::I64 => I::I64Const(0),
                    ValType::F64 => I::F64Const(0.0.into()),
                    _ => I::I32Const(0),
                })),
            E::Await { .. } => {
                return Err(CompilerError::Internal(
                    "await reached emission before normalization".into(),
                ));
            }
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
            E::TextUnits { value, op } => {
                self.expr(value)?;
                self.string_helper(strings::Helper::text_units(*op))?;
            }
            E::Intrinsic(op, args) => {
                for (at, arg) in args.iter().enumerate() {
                    self.expr(arg)?;
                    if op.key_slot() == Some(at) {
                        // A number or boolean key travels as its slot bits.
                        self.pack_slot(arg.ty.erased());
                    }
                }
                match op {
                    ir::Intrinsic::HasCallableThen => {
                        // An unknown cell's record tag is checked before reading
                        // its descriptor; class methods have no own field slot.
                        self.code.extend([
                            I::LocalSet(self.string_temporary),
                            I::LocalGet(self.string_temporary),
                            I::I32WrapI64,
                            I::I32Load(strings::WORD),
                            I::I32Const(varta_common::value::ValueKind::Record as i32),
                            I::I32Eq,
                            I::If(BlockType::Result(ValType::I32)),
                            I::LocalGet(self.string_temporary),
                            I::I32WrapI64,
                            I::I64Load(records::field(0)),
                            I::I32WrapI64,
                            I::I32Load(strings::WORD),
                            I::LocalSet(self.temporary),
                            I::I32Const(0),
                        ]);
                        let offsets: BTreeSet<_> = self
                            .layouts
                            .iter()
                            .filter(|(key, _)| self.thenable_shapes.contains(&key.0))
                            .map(|(_, offset)| *offset)
                            .collect();
                        for offset in offsets {
                            self.code.extend([
                                I::LocalGet(self.temporary),
                                I::I32Const(offset as i32),
                                I::I32Eq,
                                I::I32Or,
                            ]);
                        }
                        self.code.extend([
                            I::Else,
                            I::I32Const(0),
                            I::End,
                            I::If(BlockType::Result(ValType::I32)),
                            I::I32Const(1),
                            I::Else,
                            I::LocalGet(self.string_temporary),
                        ]);
                        self.string_helper(strings::Helper::HasCallableThen)?;
                        self.code.push(I::End);
                    }
                    ir::Intrinsic::CodeUnit => self.string_helper(strings::Helper::CodeUnit)?,
                    ir::Intrinsic::FromUnits => self.string_helper(strings::Helper::FromUnits)?,
                    ir::Intrinsic::CharAt => self.string_helper(strings::Helper::CharAt)?,
                    ir::Intrinsic::CharCodeAt => self.string_helper(strings::Helper::CharCodeAt)?,
                    ir::Intrinsic::DictionaryFind => {
                        self.string_helper(strings::Helper::DictionaryFind)?;
                    }
                    ir::Intrinsic::DictionaryUnlink => {
                        self.string_helper(strings::Helper::DictionaryUnlink)?;
                    }
                    ir::Intrinsic::DictionaryPlace => {
                        self.string_helper(strings::Helper::DictionaryPlace)?;
                    }
                    ir::Intrinsic::DictionaryIndex => {
                        self.string_helper(strings::Helper::DictionaryIndex)?;
                    }
                    ir::Intrinsic::DictionaryInsertAt => {
                        self.string_helper(strings::Helper::DictionaryInsertAt)?;
                    }
                    ir::Intrinsic::Collection(probe, hash) => {
                        self.string_helper(strings::Helper::Collection(*probe, *hash))?;
                    }
                    ir::Intrinsic::Units => {
                        self.string_helper(strings::Helper::Units(args.len() as u8))?;
                    }
                    // The low word of every managed handle is its payload address.
                    ir::Intrinsic::Identity => self.code.extend([I::I32WrapI64, I::F64ConvertI32U]),
                }
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
                if value.ty.is_cell() {
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
            E::ArrayKind(value, kind) => {
                self.expr(value)?;
                self.code.extend([
                    I::I32WrapI64,
                    I::I32Load(strings::WORD),
                    I::I32Const(*kind as i32),
                    I::I32Eq,
                ]);
            }
            E::IsUnionMember(value, member) => self.is_union_member(value, *member)?,
            E::IsUndefined(value) => {
                self.expr(value)?;
                if value.ty.is_cell() {
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
                self.conditional(test, yes, no, expr.ty)?;
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
                let kind = self
                    .tag(needle.ty)
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
                if let Some(appended) = arrays::appended(self.profile, items) {
                    // V38: each slot is rooted (or stable) like an operand,
                    // and the helper appends them without an argument array.
                    for value in &appended {
                        self.expr(value)?;
                        self.pack_slot(value.ty);
                    }
                    self.string_helper(strings::Helper::ArrayAppend(appended.len() as u8))?;
                } else {
                    self.expr(items)?;
                    self.string_helper(strings::Helper::ArrayPush)?;
                }
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
            E::IndexKey(key) => {
                self.expr(key)?;
                self.string_helper(strings::Helper::ArrayIndexKey)?;
            }
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
            // V38 (`json_decoders`): the target's compiled decoder, which
            // falls back to the linked parse itself.
            E::ParseJson(text) if self.decoders.contains_key(&expr.ty) => {
                self.expr(text)?;
                self.code.push(I::Call(self.decoders[&expr.ty]));
                self.check_exception()?;
                self.unpack_slot(expr.ty);
            }
            // A disabled compiler-only attempt misses. Its value block owns
            // the ordinary parse/conversion, including all error behavior.
            E::ParseJson(_) if self.direct_json_targets.contains(&expr.ty) => {
                self.code.push(I::I64Const(ir::JSON_DECODE_MISS));
            }
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
            E::KvStage(mode, value) => self.kv_stage(*mode, value)?,
            E::KvDecode(host, index) => {
                self.expr(index)?;
                let (ptr, len) = self.schemas[&expr.ty];
                self.code.extend([
                    I::I32Const(ptr as i32),
                    I::I32Const(len as i32),
                    I::Call(self.hosts[host]),
                ]);
                self.check_exception()?;
                self.unpack_slot(expr.ty);
            }
            E::Integer(n) => self.code.push(I::I32Const(*n)),
            E::Float64(n) => self.code.push(I::F64Const((*n).into())),
            E::Local(slot) => self.read_binding(*slot, expr.ty),
            E::ModuleConstant(index) => {
                self.code.extend([
                    I::I32Const(self.constant_root(*index)),
                    I::I64Load(strings::ROOT),
                ]);
                // A module binding's initialization flag (v38
                // `module_evaluation`) is a Boolean in its root slot.
                if expr.ty == Type::Boolean {
                    self.code.push(I::I32WrapI64);
                }
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
            E::RegExp(op, args) => {
                for arg in args {
                    self.expr(arg)?;
                }
                self.string_helper(strings::Helper::regexp(*op))?;
            }
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
                ]);
                self.text_result(*host)?;
                if expr.ty == Type::BigInt {
                    self.bigint_leaf();
                }
                self.code.extend([I::Else, I::I64Const(0), I::End]);
            }
            E::Host(host, args)
                if strings::Helper::fast_path(self.profile, *host, args, self.unions).is_some() =>
            {
                // V38: cell operands go to the fast-path helper, which calls
                // the linked operation for the cases it does not answer; a
                // boxed Number goes unboxed.
                let helper = strings::Helper::fast_path(self.profile, *host, args, self.unions);
                for arg in args {
                    match &arg.kind {
                        E::BoxUnion(inner)
                            if matches!(
                                helper,
                                Some(strings::Helper::NumberText | strings::Helper::StringNumber)
                            ) =>
                        {
                            self.expr(inner)?;
                        }
                        _ => self.expr(arg)?,
                    }
                }
                if let Some(helper) = helper {
                    self.string_helper(helper)?;
                    if expr.ty == Type::Float64 && helper.signature().1 == [ValType::I32] {
                        self.code.push(I::F64ConvertI32S);
                    }
                }
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
                    self.text_result(*host)?;
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
                        | Type::Dictionary(_)
                        | Type::Class(_)
                        | Type::Promise(_)
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
            E::Binary(op, left, right)
                if left.ty == Type::String && self.literal_equality(*op, left, right)? => {}
            E::Binary(BinaryOp::Add, left, _) if left.ty == Type::String && self.codegen_v2() => {
                self.concat_chain(expr)?;
            }
            E::Binary(op, left, right) if left.ty == Type::String => {
                self.expr(left)?;
                self.expr(right)?;
                if *op == BinaryOp::Add {
                    self.string_helper(strings::Helper::Concat)?;
                } else if self.codegen_v2()
                    && matches!(
                        op,
                        BinaryOp::EqEq | BinaryOp::EqEqEq | BinaryOp::NotEq | BinaryOp::NotEqEq
                    )
                {
                    self.string_helper(strings::Helper::StringEquals)?;
                    if matches!(op, BinaryOp::NotEq | BinaryOp::NotEqEq) {
                        self.code.push(I::I32Eqz);
                    }
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
        // A quiet (v38) assignment pushes no temporaries to release.
        let prior = if self.quiet_mode {
            self.temporary_floor
        } else {
            self.code
                .extend([I::GlobalGet(memory.roots), I::LocalSet(marker)]);
            self.temporary_floor.replace(marker)
        };
        self.scopes.push(vec![input]);
        self.expr(value)?;
        self.write_binding(input, value.ty, true)?;
        if !self.quiet_mode {
            self.end_temporaries();
        }
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
        // A quiet (v38) value block pushes no temporaries to release.
        let prior = if self.quiet_mode {
            self.temporary_floor
        } else {
            self.code
                .extend([I::GlobalGet(memory.roots), I::LocalSet(marker)]);
            self.temporary_floor.replace(marker)
        };
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
    /// The string a host call of `host` returned (its pointer on the stack,
    /// its byte length in `env_result_len`): an owned string of a linked
    /// v38 operation becomes its handle in place; a result-buffer string is
    /// copied into an owned one.
    pub(super) fn text_result(&mut self, host: varta_common::HostOperation) -> Result<()> {
        if crate::link::owned_text(self.profile, host) {
            self.code.extend(strings::owned_text(self.result_len));
            return Ok(());
        }
        self.code.push(I::GlobalGet(self.result_len));
        self.string_helper(strings::Helper::Snapshot)
    }
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
