use super::*;

impl Lower<'_> {
    pub(super) fn condition(&mut self, expr: &ast::Expr) -> Result<Expr> {
        let expr = self.expr(expr)?;
        let expr = self.truth(expr)?;
        if self.types.profile.has_undefined() && matches!(expr.ty, Type::Null | Type::Undefined) {
            return Ok(expr);
        }
        scalar(expr.ty).map_err(|e| self.source.locate(e, expr.span))?;
        Ok(expr)
    }
    pub(super) fn expr(&mut self, expression: &ast::Expr) -> Result<Expr> {
        self.expr_expected(expression, None)
    }
    pub(super) fn expr_expected(
        &mut self,
        expression: &ast::Expr,
        expected: Option<Type>,
    ) -> Result<Expr> {
        self.expr_contextual(expression, expected, None)
    }
    pub(super) fn expr_contextual(
        &mut self,
        expression: &ast::Expr,
        expected: Option<Type>,
        hints: Option<references::AggregateHints>,
    ) -> Result<Expr> {
        let value = self.expr_lowered(expression, expected, hints)?;
        // Without a literal context a literal-typed value is its base primitive.
        Ok(if expected.is_none() {
            literals::erase(value, self.types)
        } else {
            value
        })
    }
    /// An expression whose literal type an inferred binding or literal-aware
    /// assignment target keeps (v32 `literal_types`).
    pub(super) fn expr_preserving(&mut self, expression: &ast::Expr) -> Result<Expr> {
        self.expr_lowered(expression, None, None)
    }
    fn expr_lowered(
        &mut self,
        expression: &ast::Expr,
        expected: Option<Type>,
        hints: Option<references::AggregateHints>,
    ) -> Result<Expr> {
        self.depth += 1;
        if self.depth > 64 {
            return Err(self
                .source
                .locate(invalid("expression nesting exceeds 64"), expression.span()));
        }
        let context = if expected.is_some() {
            self.context(expression, expected)
        } else {
            hints.and_then(|h| h.select(expression))
        };
        if let Some(expected) = expected
            && unions::members(expected, self.types).contains(&Type::Undefined)
        {
            self.optional_index(expression);
        }
        let result = match expression {
            ast::Expr::Arrow(arrow) => self.arrow(arrow, expected),
            ast::Expr::Fn(function) => self.function_expression(function, expected),
            ast::Expr::Call(call) => {
                let value = self.call(call, expected, hints);
                if let Ok(value) = &value {
                    self.flow_call(call, value);
                }
                value
            }
            ast::Expr::Assign(assign) if matches!(assign.left, ast::AssignTarget::Pat(_)) => {
                let ast::AssignTarget::Pat(target) = &assign.left else {
                    return Err(invalid("missing assignment pattern"));
                };
                self.destructuring_assignment_value(assign, target, expected, hints)
            }
            ast::Expr::Assign(assign) => self.simple_assignment_value(assign),
            ast::Expr::Update(update) => self.update_value(update),
            ast::Expr::Cond(branch) if self.types.profile.reference_unions() => {
                self.conditional(branch, expected, hints)
            }
            ast::Expr::Array(array) => match self.tuple_context(context) {
                Some(id) => self.tuple_literal(array, id),
                None => match self.spread_copy(array)? {
                    Some(value) => Ok(value),
                    None => self.array(array, context),
                },
            },
            ast::Expr::New(new)
                if let ast::Expr::Ident(id) = stmt::unparen(&new.callee)
                    && let Some(kind) = collections::builtin(id.sym.as_ref(), self.types)
                    && self.binding(id.sym.as_ref()).is_err() =>
            {
                self.collection_new(new, kind, expected)
            }
            ast::Expr::Object(object) => self.record(object, context),
            ast::Expr::Paren(paren) => self.expr_contextual(&paren.expr, expected, hints),
            ast::Expr::TsAs(assertion) if self.types.profile.checked_assertions() => {
                self.type_assertion(assertion)
            }
            ast::Expr::TsNonNull(assertion) if self.types.profile.checked_assertions() => {
                self.non_null_assertion(assertion)
            }
            ast::Expr::TsTypeAssertion(_) if self.types.profile.checked_assertions() => Err(
                unsupported("angle-bracket type assertion; write a checked `x as T` instead"),
            ),
            ast::Expr::New(new)
                if self.types.profile.promises() && self.promise_builtin(&new.callee) =>
            {
                self.new_promise(new, expected)
            }
            // A generic class (v37) may take its type arguments from context.
            ast::Expr::New(new)
                if self.types.profile.class_generics()
                    && self
                        .class_callee(&new.callee)
                        .is_some_and(|class| self.types.classes.get(class).is_template()) =>
            {
                match self.class_callee(&new.callee) {
                    Some(template) => self.generic_class_new(new, template, expected),
                    None => Err(CompilerError::Internal("missing generic class".into())),
                }
            }
            ast::Expr::Await(expression) if self.types.profile.async_functions() => {
                self.await_expression(expression)
            }
            ast::Expr::OptChain(chain) if self.types.profile.function_fields() => {
                self.optional_chain(chain)
            }
            _ => self.expr_inner(expression),
        };
        if result.is_ok() {
            self.path_effects(expression);
        }
        self.depth -= 1;
        result
            .map(|value| {
                if value.ty == Type::Void && self.types.profile.has_undefined() {
                    Expr {
                        span: value.span,
                        ty: Type::Undefined,
                        kind: E::Undefined(Box::new(value)),
                    }
                } else {
                    value
                }
            })
            .and_then(|value| self.void_function(value, expected))
            .and_then(|value| unions::widen(value, expected, self.types))
            .map_err(|e| self.source.locate(e, expression.span()))
    }
    fn expr_inner(&mut self, expression: &ast::Expr) -> Result<Expr> {
        let (ty, kind) = match expression {
            ast::Expr::This(_) => {
                if self.binding("this").is_err() && !self.outer.contains_key("this") {
                    return Err(unsupported(
                        "this requires an explicit typed function receiver",
                    ));
                }
                return self.expr(&ast::Expr::Ident(ast::Ident {
                    span: expression.span(),
                    ctxt: Default::default(),
                    sym: "this".into(),
                    optional: false,
                }));
            }
            ast::Expr::Lit(ast::Lit::Regex(regex)) if self.types.profile.regexp() => {
                return self.regexp_literal(regex);
            }
            ast::Expr::Lit(ast::Lit::Num(n)) if self.types.profile.binary64() => {
                (Type::Float64, E::Float64(n.value))
            }
            ast::Expr::Lit(ast::Lit::Num(n)) => {
                if n.value.fract() != 0.0 || n.value < i32::MIN as f64 || n.value > i32::MAX as f64
                {
                    return Err(invalid("only integer literals in i32 range are supported"));
                }
                (Type::Number, E::Integer(n.value as i32))
            }
            ast::Expr::Lit(ast::Lit::BigInt(n)) => return self.bigint_literal(n),
            ast::Expr::Lit(ast::Lit::Bool(b)) => (Type::Boolean, E::Integer(i32::from(b.value))),
            ast::Expr::Lit(ast::Lit::Null(_)) => (Type::Null, E::Integer(0)),
            ast::Expr::Lit(ast::Lit::Str(s)) => {
                let bytes = if self.types.profile.utf16() {
                    s.value
                        .as_wtf8()
                        .to_ill_formed_utf16()
                        .flat_map(u16::to_le_bytes)
                        .collect()
                } else {
                    s.value
                        .as_str()
                        .ok_or_else(|| unsupported("non-UTF-8 string"))?
                        .as_bytes()
                        .to_vec()
                };
                (Type::String, E::String(bytes))
            }
            ast::Expr::Ident(name) => {
                if self.outer.contains_key(name.sym.as_ref()) {
                    self.resolve_binding(name.sym.as_ref())?;
                }
                if self.types.profile.has_undefined()
                    && name.sym == "undefined"
                    && !self.functions.contains_key("undefined")
                    && self.binding("undefined").is_err()
                {
                    return Ok(Expr {
                        span: expression.span(),
                        ty: Type::Undefined,
                        kind: E::Integer(0),
                    });
                }
                if self.types.profile.binary64()
                    && !self.functions.contains_key(name.sym.as_ref())
                    && self.binding(name.sym.as_ref()).is_err()
                {
                    let value = match name.sym.as_ref() {
                        "NaN" => Some(f64::NAN),
                        "Infinity" => Some(f64::INFINITY),
                        _ => None,
                    };
                    if let Some(value) = value {
                        return Ok(Expr {
                            span: expression.span(),
                            ty: Type::Float64,
                            kind: E::Float64(value),
                        });
                    }
                }
                if self.binding(name.sym.as_ref()).is_err()
                    && let Some(signature) = self.functions.get(name.sym.as_ref()).cloned()
                {
                    return self.function_reference(&signature, expression.span());
                }
                if self.generic_name(name.sym.as_ref()).is_some() {
                    return Err(unsupported(
                        "generic function as a value (call it so its type arguments are known)",
                    ));
                }
                let binding = self.resolve_binding(name.sym.as_ref())?;
                if binding.global.is_some() {
                    return self.global_read(&binding, expression.span());
                }
                if !binding.initialized
                    && binding.tdz.is_some()
                    && binding.identity.0 != self.index
                    && self.types.profile.language_core()
                {
                    return self.tdz_ident(name.sym.as_ref(), &binding, expression.span());
                }
                if !binding.initialized {
                    return Err(invalid(format!(
                        "binding used before initialization: {}",
                        name.sym
                    )));
                }
                self.require_assigned(name.sym.as_ref(), &binding)?;
                let ty = binding.ty.ok_or_else(|| invalid("missing binding type"))?;
                // Narrowed to `never` (v37): no value is read.
                if self.refinements.get(&binding.slot) == Some(&Type::Never) {
                    return Ok(Expr {
                        span: expression.span(),
                        ty: Type::Never,
                        kind: E::Zero,
                    });
                }
                if let Some(narrow) = self.refinements.get(&binding.slot).copied()
                    // An `object` fact (v35) is read as the `unknown` cell; only
                    // `object_value` consumers use it.
                    && !matches!(narrow, Type::Object(_))
                {
                    let local = Expr {
                        span: expression.span(),
                        ty,
                        kind: E::Local(binding.slot),
                    };
                    // `Array.isArray` on an `unknown` cell (v38).
                    if ty.is_cell()
                        && self.types.profile.structural_subtyping()
                        && narrow == Self::unknown_array_type(self.types)?
                    {
                        return self.unknown_array(local);
                    }
                    // A literal guard may refine a non-union local in place.
                    let retag =
                        narrow.is_cell() || self.types.profile.literal_types() && !ty.is_cell();
                    return Ok(if retag {
                        Expr {
                            ty: narrow,
                            ..local
                        }
                    } else {
                        Expr {
                            span: expression.span(),
                            ty: narrow,
                            kind: E::UnboxUnion(Box::new(local)),
                        }
                    });
                }
                (ty, E::Local(binding.slot))
            }
            ast::Expr::Paren(paren) => return self.expr(&paren.expr),
            ast::Expr::Unary(unary) => {
                if unary.op == ast::UnaryOp::Delete {
                    return self.array_delete(&unary.arg, unary.span);
                }
                let value = self.expr(&unary.arg)?;
                if self.types.profile.bigint_values()
                    && matches!(
                        unary.op,
                        ast::UnaryOp::Minus | ast::UnaryOp::Plus | ast::UnaryOp::Tilde
                    )
                {
                    return self.primitive_unary(unary.op, value, expression.span());
                }
                if value.ty == Type::BigInt
                    && matches!(
                        unary.op,
                        ast::UnaryOp::Minus | ast::UnaryOp::Plus | ast::UnaryOp::Tilde
                    )
                {
                    return self.bigint_unary(unary.op, value, expression.span());
                }
                match unary.op {
                    ast::UnaryOp::TypeOf if value.ty.is_cell() => {
                        return Ok(Expr {
                            span: expression.span(),
                            ty: Type::String,
                            kind: E::Host(
                                self.types.profile.host_operation(Host::UnionTypeOf),
                                vec![value],
                            ),
                        });
                    }
                    ast::UnaryOp::Bang if self.types.profile.primitive_builtins() => {
                        let value = self.truth(value)?;
                        (Type::Boolean, E::Not(Box::new(value)))
                    }
                    ast::UnaryOp::Bang
                        if self.types.profile.unions()
                            && matches!(
                                value.ty,
                                Type::Function(_)
                                    | Type::Error
                                    | Type::NativeError(_)
                                    | Type::String
                                    | Type::Union(_)
                                    | Type::ReferenceUnion(_)
                            ) =>
                    {
                        let value = self.truth(value)?;
                        (Type::Boolean, E::Not(Box::new(value)))
                    }
                    ast::UnaryOp::Void if self.types.profile.has_undefined() => {
                        (Type::Undefined, E::Undefined(Box::new(value)))
                    }
                    ast::UnaryOp::TypeOf if self.types.profile.has_undefined() => {
                        let name = match value.ty {
                            Type::Function(_) => "function",
                            Type::Undefined | Type::Void => "undefined",
                            Type::Error
                            | Type::NativeError(_)
                            | Type::Null
                            | Type::Record(_)
                            | Type::Dictionary(_)
                            | Type::Class(_)
                            | Type::Array(_) => "object",
                            Type::Promise(_) if self.types.profile.promise_values() => "object",
                            Type::Number | Type::Float64 => "number",
                            Type::BigInt => "bigint",
                            Type::Boolean => "boolean",
                            Type::String => "string",
                            _ => return Err(unsupported("typeof an opaque result value")),
                        };
                        return Ok(Expr {
                            span: expression.span(),
                            ty: Type::String,
                            kind: E::Sequence(
                                Box::new(value),
                                Box::new(Expr {
                                    span: expression.span(),
                                    ty: Type::String,
                                    kind: E::String(self.types.profile.string_bytes(name)),
                                }),
                            ),
                        });
                    }
                    ast::UnaryOp::Minus if matches!(value.ty, Type::Number | Type::Float64) => {
                        (value.ty, E::Negative(Box::new(value)))
                    }
                    ast::UnaryOp::Plus if value.ty == Type::Float64 => return Ok(value),
                    ast::UnaryOp::Tilde => {
                        return self.number_bitwise_not(value, expression.span());
                    }
                    ast::UnaryOp::Bang
                        if matches!(value.ty, Type::Number | Type::Float64 | Type::Boolean)
                            || self.types.profile.has_undefined()
                                && matches!(value.ty, Type::Null | Type::Undefined) =>
                    {
                        (Type::Boolean, E::Not(Box::new(value)))
                    }
                    _ => return Err(unsupported("unary operator")),
                }
            }
            ast::Expr::New(new)
                if self.types.profile.bigint()
                    && matches!(&*new.callee,ast::Expr::Ident(id) if id.sym=="BigInt" && self.binding("BigInt").is_err() && !self.outer.contains_key("BigInt") && !self.functions.contains_key("BigInt")) =>
            {
                return self.bigint_new(new);
            }
            ast::Expr::New(new) => {
                if let Some(class) = self.class_callee(&new.callee) {
                    if self.regexp_constructor(class) {
                        return self.regexp_new(new);
                    }
                    return self.class_new(new, class);
                }
                return self.new_error(new);
            }
            ast::Expr::Bin(binary) if binary.op == ast::BinaryOp::InstanceOf => {
                if let Some(class) = self.class_callee(&binary.right) {
                    return self.class_instance_of(binary, class);
                }
                return self.instance_of_error(binary);
            }
            ast::Expr::Yield(expression) if self.types.profile.generators() => {
                return self.yield_expression(expression);
            }
            ast::Expr::Class(_) if self.types.profile.classes() => {
                return Err(unsupported("class expression"));
            }
            ast::Expr::SuperProp(_) if self.types.profile.classes() => {
                return Err(unsupported(
                    "super property read; only super.method(...) calls",
                ));
            }
            ast::Expr::Bin(binary) => {
                // `#x in value` (v37 `class_generics`): a class brand test.
                if binary.op == ast::BinaryOp::In
                    && self.types.profile.class_generics()
                    && let ast::Expr::PrivateName(name) = &*binary.left
                {
                    return self.private_in(name, &binary.right, expression.span());
                }
                if binary.op == ast::BinaryOp::NullishCoalescing {
                    self.optional_index(&binary.left);
                }
                // V38 keeps the literal type of a `??`/`||` operand, as
                // TypeScript does for `mode ?? "json"`.
                let left = if self.types.profile.structural_subtyping()
                    && matches!(
                        binary.op,
                        ast::BinaryOp::NullishCoalescing | ast::BinaryOp::LogicalOr
                    ) {
                    self.expr_preserving(&binary.left)?
                } else {
                    self.expr(&binary.left)?
                };
                if self.types.profile.primitive_builtins()
                    && matches!(
                        binary.op,
                        ast::BinaryOp::LogicalAnd
                            | ast::BinaryOp::LogicalOr
                            | ast::BinaryOp::NullishCoalescing
                    )
                {
                    let saved = self.flow_snapshot();
                    let context = self.fallback_context(binary, left.ty);
                    let right = (|| {
                        if self.types.profile.truth_guards() {
                            match binary.op {
                                ast::BinaryOp::LogicalAnd => self.guard(&binary.left, true)?,
                                ast::BinaryOp::LogicalOr => self.guard(&binary.left, false)?,
                                _ => self.nullish_guard(&binary.left)?,
                            }
                        }
                        match context {
                            Some(context) => self.expr_expected(&binary.right, Some(context)),
                            // A literal fallback of a literal-typed value
                            // (v38) keeps its literal type, so `mode ?? "json"`
                            // stays the literal union.
                            None if self.types.profile.structural_subtyping()
                                && matches!(
                                    stmt::unparen(&binary.right),
                                    ast::Expr::Lit(ast::Lit::Str(_) | ast::Lit::Num(_))
                                ) =>
                            {
                                let literal = unions::members(left.ty, self.types)
                                    .into_iter()
                                    .find(|ty| matches!(ty, Type::Literal(..)));
                                let plain = self.expr(&binary.right)?;
                                match literal {
                                    Some(member) => {
                                        match literals::conform(plain, Some(member), self.types) {
                                            Ok(value) => Ok(value),
                                            Err(_) => self.expr(&binary.right),
                                        }
                                    }
                                    None => Ok(plain),
                                }
                            }
                            None => self.expr(&binary.right),
                        }
                    })();
                    self.flow_restore(&saved);
                    return self.guest_logical(binary.op, left, right?, expression.span());
                }
                let right = if binary.op == ast::BinaryOp::In {
                    // `key in value` for an `object` narrowed from unknown (v35).
                    self.object_value(&binary.right)?
                } else {
                    self.expr(&binary.right)?
                };
                if (left.ty == Type::Unknown || right.ty == Type::Unknown)
                    && !matches!(
                        binary.op,
                        ast::BinaryOp::EqEq
                            | ast::BinaryOp::EqEqEq
                            | ast::BinaryOp::NotEq
                            | ast::BinaryOp::NotEqEq
                    )
                {
                    return Err(self.source.locate(
                        type_error(
                            "operator on an unknown value; narrow it with typeof, instanceof or a checked `as` first",
                        ),
                        expression.span(),
                    ));
                }
                if binary.op == ast::BinaryOp::In && matches!(right.ty, Type::Dictionary(_)) {
                    // `key in d`: own entries plus inherited Object.prototype names.
                    let span = expression.span();
                    let (key, dictionary) = (self.hidden(left.ty), self.hidden(right.ty));
                    let output = self.hidden(Type::Boolean);
                    let marker = self.hidden(Type::Number);
                    let local = |slot, ty| Expr {
                        span,
                        ty,
                        kind: E::Local(slot),
                    };
                    let (key_ty, dictionary_ty) = (left.ty, right.ty);
                    let test = self.dictionary_has(
                        local(dictionary, dictionary_ty),
                        local(key, key_ty),
                        true,
                    )?;
                    return Ok(Expr {
                        span,
                        ty: Type::Boolean,
                        kind: E::ValueBlock {
                            output,
                            marker,
                            roots: vec![key, dictionary],
                            body: vec![
                                S::Init(key, left),
                                S::Init(dictionary, right),
                                S::Init(output, test),
                            ],
                        },
                    });
                }
                if self.types.profile.bigint_values()
                    && matches!(
                        binary.op,
                        ast::BinaryOp::EqEq
                            | ast::BinaryOp::EqEqEq
                            | ast::BinaryOp::NotEq
                            | ast::BinaryOp::NotEqEq
                            | ast::BinaryOp::Lt
                            | ast::BinaryOp::LtEq
                            | ast::BinaryOp::Gt
                            | ast::BinaryOp::GtEq
                    )
                    && [left.ty, right.ty].into_iter().all(|ty| {
                        unions::members(ty, self.types)
                            .into_iter()
                            .all(|member| unions::primitive(member) || member == Type::BigInt)
                    })
                {
                    return self.primitive_comparison(binary.op, left, right, expression.span());
                }
                if self.types.profile.bigint_values()
                    && matches!(
                        binary.op,
                        ast::BinaryOp::Add
                            | ast::BinaryOp::Sub
                            | ast::BinaryOp::Mul
                            | ast::BinaryOp::Div
                            | ast::BinaryOp::Mod
                            | ast::BinaryOp::Exp
                            | ast::BinaryOp::BitAnd
                            | ast::BinaryOp::BitOr
                            | ast::BinaryOp::BitXor
                            | ast::BinaryOp::LShift
                            | ast::BinaryOp::RShift
                            | ast::BinaryOp::ZeroFillRShift
                    )
                {
                    return self.primitive_arithmetic(binary.op, left, right, expression.span());
                }
                if (left.ty == Type::BigInt || right.ty == Type::BigInt)
                    && !left.ty.is_union()
                    && !right.ty.is_union()
                {
                    return self.bigint_binary(binary.op, left, right, expression.span());
                }
                if binary.op == ast::BinaryOp::Exp {
                    return self.number_power(left, right, expression.span());
                }
                if ir::is_number_bitwise(binary.op) {
                    return self.number_bitwise(binary.op, left, right, expression.span());
                }
                if left.ty.is_error() && right.ty.is_error() && left.ty != right.ty {
                    return self.union_binary(binary.op, left, right, expression.span());
                }
                if binary.op == ast::BinaryOp::In
                    && self.shared_records(right.ty)
                    && let ast::Expr::Lit(ast::Lit::Str(text)) = stmt::unparen(&binary.left)
                    && let Some(name) = text.value.as_str()
                {
                    let name = name.to_owned();
                    return self.shared_in(&name, right, expression.span());
                }
                // `"key" in object` as a value (v38): the own-field presence
                // of a record (through its view accessors), an instance member.
                if binary.op == ast::BinaryOp::In
                    && self.types.profile.structural_subtyping()
                    && matches!(right.ty, Type::Record(_) | Type::Class(_))
                    && let ast::Expr::Lit(ast::Lit::Str(text)) = stmt::unparen(&binary.left)
                    && let Some(name) = text.value.as_str()
                {
                    let name = name.to_owned();
                    return self.object_in(&name, right, expression.span());
                }
                if left.ty.is_cell() || right.ty.is_cell() {
                    return self.union_binary(binary.op, left, right, expression.span());
                }
                let (left, right) = classes::common_operands(left, right, self.types);
                let (left, right) =
                    subtyping::identity_operands(binary.op, left, right, self.types);
                let (left, right) = if self.types.profile.has_undefined() {
                    match super::undefined::binary(
                        binary.op,
                        left,
                        right,
                        expression.span(),
                        self.types.profile.language_core(),
                    ) {
                        super::undefined::Binary::Folded(value) => return Ok(value),
                        super::undefined::Binary::Unchanged(left, right) => (left, right),
                    }
                } else {
                    (left, right)
                };
                if left.ty != right.ty {
                    return Err(self.source.locate(
                        type_error(format!(
                            "binary operand type mismatch: expected {}, found {}",
                            left.ty, right.ty
                        )),
                        right.span,
                    ));
                }
                let result = match binary.op {
                    ast::BinaryOp::Add
                        if matches!(left.ty, Type::Number | Type::Float64 | Type::String) =>
                    {
                        left.ty
                    }
                    ast::BinaryOp::Sub
                    | ast::BinaryOp::Mul
                    | ast::BinaryOp::Div
                    | ast::BinaryOp::Mod
                        if matches!(left.ty, Type::Number | Type::Float64) =>
                    {
                        left.ty
                    }
                    ast::BinaryOp::Lt
                    | ast::BinaryOp::LtEq
                    | ast::BinaryOp::Gt
                    | ast::BinaryOp::GtEq
                        if matches!(left.ty, Type::Number | Type::Float64 | Type::String) =>
                    {
                        Type::Boolean
                    }
                    ast::BinaryOp::EqEq
                    | ast::BinaryOp::EqEqEq
                    | ast::BinaryOp::NotEq
                    | ast::BinaryOp::NotEqEq
                        if matches!(
                            left.ty,
                            Type::Number
                                | Type::Float64
                                | Type::Boolean
                                | Type::String
                                | Type::Null
                                | Type::Function(_)
                                | Type::Error
                                | Type::NativeError(_)
                                | Type::Record(_)
                                | Type::Dictionary(_)
                                | Type::Class(_)
                                | Type::Promise(_)
                                | Type::Array(_)
                        ) =>
                    {
                        Type::Boolean
                    }
                    ast::BinaryOp::LogicalAnd | ast::BinaryOp::LogicalOr
                        if matches!(left.ty, Type::Number | Type::Float64 | Type::Boolean) =>
                    {
                        left.ty
                    }
                    _ => return Err(unsupported("binary operator")),
                };
                (
                    result,
                    E::Binary(binary.op, Box::new(left), Box::new(right)),
                )
            }
            ast::Expr::Cond(branch) if self.types.profile.unions() => {
                return self.conditional(branch, None, None);
            }
            ast::Expr::Call(call) => return self.call(call, None, None),
            ast::Expr::Member(member) => return self.member_read(member),
            ast::Expr::Tpl(template) if self.types.profile.template_literals() => {
                return self.template_literal(template);
            }
            ast::Expr::TaggedTpl(_) if self.types.profile.template_literals() => {
                return Err(unsupported(
                    "tagged template literal (including String.raw)",
                ));
            }
            _ => return Err(unsupported("expression")),
        };
        Ok(Expr {
            span: expression.span(),
            ty,
            kind,
        })
    }
    fn call(
        &mut self,
        call: &ast::CallExpr,
        expected: Option<Type>,
        hints: Option<references::AggregateHints>,
    ) -> Result<Expr> {
        #[cfg(any(test, feature = "suspension-test-hook"))]
        if let Some(point) = self.suspension_hook(call)? {
            return Ok(point);
        }
        if let ast::Callee::Expr(callee) = &call.callee
            && callee_name(callee).as_deref() == Some("varta.json.parse")
        {
            if call.args.iter().any(|arg| arg.spread.is_some()) {
                return Err(unsupported("spread to a JSON intrinsic"));
            }
            return self.parse_json(call);
        }
        if let ast::Callee::Expr(callee) = &call.callee
            && self.function_field_callee(callee)
        {
            return self.function_field_call(call, callee);
        }
        if self.types.profile.classes() {
            if matches!(call.callee, ast::Callee::Super(_)) {
                return self.super_call(call);
            }
            if let ast::Callee::Expr(callee) = &call.callee {
                if let ast::Expr::SuperProp(property) = stmt::unparen(callee) {
                    return self.super_method_call(call, property, expected);
                }
                if let ast::Expr::Member(member) = stmt::unparen(callee)
                    && let Some(class) = self.class_callee(&member.obj)
                    && !self.prelude_member(callee)
                {
                    return self.class_static_call(call, member, class, expected);
                }
                if let ast::Expr::Member(member) = stmt::unparen(callee)
                    && self
                        .peek_type(&member.obj, 0)
                        .is_some_and(|ty| self.shared_classes(ty))
                {
                    return self.shared_method_call(call, member, expected);
                }
                if let ast::Expr::Member(member) = stmt::unparen(callee)
                    && self.class_receiver(&member.obj, 0).is_some()
                {
                    return self.class_method_call(call, member, None, expected);
                }
            }
        }
        if self.types.profile.generators()
            && let ast::Callee::Expr(callee) = &call.callee
            && let ast::Expr::Member(member) = stmt::unparen(callee)
            && let ast::MemberProp::Ident(method) = &member.prop
            && matches!(method.sym.as_ref(), "next" | "return" | "throw")
        {
            let receiver = self.expr(&member.obj)?;
            return self.generator_method(call, receiver, method.sym.as_ref());
        }
        if self.types.profile.promises()
            && let ast::Callee::Expr(callee) = &call.callee
            && let ast::Expr::Member(member) = stmt::unparen(callee)
        {
            if self.promise_builtin(&member.obj) {
                return self.promise_static(call, member, expected);
            }
            if let ast::MemberProp::Ident(method) = &member.prop
                && matches!(method.sym.as_ref(), "then" | "catch" | "finally")
            {
                let receiver = self.expr(&member.obj)?;
                return self.promise_method(call, receiver, method.sym.as_ref());
            }
        }
        if self.types.profile.collections()
            && let ast::Callee::Expr(callee) = &call.callee
            && let ast::Expr::Member(member) = stmt::unparen(callee)
            && let Some(value) = self.builtin_method(call, member, expected)?
        {
            return Ok(value);
        }
        if let Some(generic) = self.generic_callee(call) {
            return self.generic_call(call, generic, expected);
        }
        if call.type_args.is_some() {
            return Err(unsupported("generic call"));
        }
        let ast::Callee::Expr(callee) = &call.callee else {
            return Err(unsupported("callee"));
        };
        if let ast::Expr::Member(member) = &**callee
            && matches!(&*member.obj, ast::Expr::Ident(id) if id.sym == "Number")
        {
            return self.number_predicate_call(call, member);
        }
        if let ast::Expr::Member(member) = &**callee
            && matches!(&*member.obj, ast::Expr::Ident(id) if id.sym == "Math")
        {
            return self.math_call(call, member);
        }
        if call.args.iter().any(|arg| arg.spread.is_some()) {
            if let ast::Expr::Ident(id) = &**callee
                && self.timer_builtin(id.sym.as_ref())
            {
                // Timer calls reject spread arguments by name.
                return self.timer_call(call, id.sym.as_ref());
            }
            return self.spread_call(call, callee);
        }
        if let ast::Expr::Ident(id) = &**callee
            && self.outer.contains_key(id.sym.as_ref())
        {
            self.resolve_binding(id.sym.as_ref())?;
        }
        if self.types.profile.function_values()
            && match &**callee {
                ast::Expr::Ident(id) => self.binding(id.sym.as_ref()).is_ok(),
                ast::Expr::Paren(_)
                | ast::Expr::Call(_)
                | ast::Expr::Arrow(_)
                | ast::Expr::Fn(_)
                | ast::Expr::This(_)
                | ast::Expr::Cond(_) => true,
                _ => false,
            }
        {
            return self.function_call(call, callee);
        }
        if self.types.profile.closures()
            && let ast::Expr::Member(member) = &**callee
            && matches!(&member.prop, ast::MemberProp::Ident(id) if id.sym=="call")
        {
            return self.function_call_method(call, member);
        }
        if let ast::Expr::Member(member) = &**callee
            && matches!(&member.prop, ast::MemberProp::Ident(method) if method.sym == "valueOf")
        {
            let value = self.expr(&member.obj)?;
            if value.ty == Type::BigInt {
                return self.bigint_method(call, value, true);
            }
            return self.number_value_of(call, value);
        }
        if let ast::Expr::Member(member) = &**callee
            && let ast::MemberProp::Ident(method) = &member.prop
            && let Some(mode) =
                varta_common::number_format::NumberFormat::from_name(method.sym.as_ref())
        {
            let value = self.expr(&member.obj)?;
            return self.number_format(call, value, mode);
        }
        if self.types.profile.regexp()
            && let ast::Expr::Member(member) = &**callee
            && let ast::MemberProp::Ident(method) = &member.prop
            && regexp::STRING_METHODS.contains(&method.sym.as_ref())
            && !self.prelude_member(callee)
            && self.class_callee(&member.obj).is_none()
        {
            let value = self.expr(&member.obj)?;
            if value.ty == Type::String {
                return self.regexp_string_method(call, value, method.sym.as_ref());
            }
            if matches!(value.ty, Type::Class(_)) {
                return self.class_method_call(call, member, Some(value), expected);
            }
            return Err(self.source.locate(
                type_error(format!(
                    "String.prototype.{} requires a string receiver, found {}",
                    method.sym, value.ty
                )),
                value.span,
            ));
        }
        if let ast::Expr::Member(member) = &**callee
            && let ast::MemberProp::Ident(method) = &member.prop
            && matches!(method.sym.as_ref(), "slice" | "substring")
        {
            // Lower the shared slice receiver exactly once, including lifted
            // functions/captures, before choosing array or string semantics.
            let value = self.expr(&member.obj)?;
            if method.sym == "slice"
                && self.types.profile.resizable_arrays()
                && unions::members(value.ty, self.types)
                    .iter()
                    .any(|ty| matches!(ty, Type::Array(_)))
            {
                return self.array_plain_method(call, value, "slice");
            }
            return self.string_range(call, value, method.sym.as_ref());
        }
        if let ast::Expr::Member(member) = &**callee
            && let ast::MemberProp::Ident(method) = &member.prop
            && matches!(method.sym.as_ref(), "trim" | "trimStart" | "trimEnd")
        {
            let value = self.expr(&member.obj)?;
            return self.string_trim(call, value, method.sym.as_ref());
        }
        if self.types.profile.closures()
            && let ast::Expr::Member(member) = &**callee
            && let ast::MemberProp::Ident(method) = &member.prop
            && matches!(method.sym.as_ref(), "indexOf" | "lastIndexOf")
        {
            let value = self.expr(&member.obj)?;
            if value.ty == Type::String {
                return self.string_search(call, value, method.sym.as_ref());
            }
            if matches!(method.sym.as_ref(), "indexOf" | "lastIndexOf")
                && matches!(value.ty, Type::Array(_))
                && self.types.profile.resizable_arrays()
            {
                return self.array_plain_method(call, value, method.sym.as_ref());
            }
            return Err(type_error(
                "String.indexOf/lastIndexOf requires a narrowed string receiver",
            ));
        }
        if let ast::Expr::Member(member) = &**callee
            && let ast::MemberProp::Ident(method) = &member.prop
            && matches!(method.sym.as_ref(), "includes" | "startsWith" | "endsWith")
        {
            let value = self.expr(&member.obj)?;
            if method.sym == "includes"
                && self.types.profile.resizable_arrays()
                && unions::members(value.ty, self.types)
                    .iter()
                    .any(|ty| matches!(ty, Type::Array(_)))
            {
                return self.array_plain_method(call, value, "includes");
            }
            return self.string_search(call, value, method.sym.as_ref());
        }
        if self.types.profile.resizable_arrays()
            && let ast::Expr::Member(member) = &**callee
            && let ast::MemberProp::Ident(method) = &member.prop
            && matches!(
                method.sym.as_ref(),
                "push"
                    | "pop"
                    | "at"
                    | "indexOf"
                    | "lastIndexOf"
                    | "map"
                    | "filter"
                    | "forEach"
                    | "find"
                    | "findIndex"
                    | "findLast"
                    | "findLastIndex"
                    | "reduce"
                    | "reduceRight"
                    | "some"
                    | "every"
                    | "reverse"
                    | "fill"
            )
        {
            return self.array_method(call, member, method.sym.as_ref(), expected, hints);
        }
        if let ast::Expr::Member(member) = &**callee
            && matches!(&member.prop,ast::MemberProp::Ident(id) if id.sym=="repeat")
        {
            return self.string_repeat(call, member);
        }
        if self.types.profile.utf16()
            && let ast::Expr::Member(member) = &**callee
            && let ast::MemberProp::Ident(method) = &member.prop
            && matches!(method.sym.as_ref(), "charAt" | "charCodeAt")
        {
            let value = self.expr(&member.obj)?;
            if value.ty != Type::String {
                return Err(type_error("string method requires a string receiver"));
            }
            if call.args.len() > 1 {
                return Err(invalid(
                    "string method expects zero or one numeric argument",
                ));
            }
            let index = if let Some(arg) = call.args.first() {
                self.expr(&arg.expr)?
            } else {
                Expr {
                    span: call.span,
                    ty: Type::Float64,
                    kind: E::Float64(0.0),
                }
            };
            if index.ty != Type::Float64 {
                return Err(type_error("string index must be a number"));
            }
            let (host, ty) = if method.sym == "charAt" {
                (Host::StringCharAt16, Type::String)
            } else {
                (Host::StringCharCodeAt16, Type::Float64)
            };
            return Ok(Expr {
                span: call.span,
                ty,
                kind: E::Host(host, vec![value, index]),
            });
        }
        if let Some(class) = self.error_callee(callee) {
            return self.error_constructor(class, &call.args, call.type_args.is_some(), call.span);
        }
        if let ast::Expr::Member(member) = &**callee
            && self.class_late_receiver(member)
            && !self.prelude_member(callee)
        {
            let value = self.expr(&member.obj)?;
            return self.class_method_call(call, member, Some(value), expected);
        }
        if let ast::Expr::Member(member) = &**callee
            && matches!(&member.prop,ast::MemberProp::Ident(id) if id.sym=="toString")
        {
            return self.error_to_string(call, member);
        }
        let name = callee_name(callee).ok_or_else(|| unsupported("callee"))?;
        if let Some(value) = self.web_text(call, &name)? {
            return Ok(value);
        }
        if let Some(value) = self.regexp_intrinsic(call, &name)? {
            return Ok(value);
        }
        if let Some(value) = self.prelude_timer(call, &name)? {
            return Ok(value);
        }
        if name == "Array.from"
            && self.types.profile.collections()
            && self.binding("Array").is_err()
            && let [source] = call.args.as_slice()
            && source.spread.is_none()
        {
            // `Array.from(collection)` and `Array.from(collection.keys())` (v37).
            if self.collection_iterator(&source.expr) {
                let value = self.expr(&source.expr)?;
                return self.drain(value);
            }
            if self.collection_receiver(&source.expr, 0) {
                let value = self.expr(&source.expr)?;
                return self.collection_array(value);
            }
            return Err(self.source.locate(
                unsupported(
                    "Array.from of a value other than a Map, Set, Uint8Array or their iterators",
                ),
                call.span,
            ));
        }
        if let Some(value) = self.prelude_net(call, &name)? {
            return Ok(value);
        }
        if let Some(value) = self.prelude_net_watch(call, &name)? {
            return Ok(value);
        }
        if name == "Array.isArray" && self.types.profile.reference_unions() {
            return self.array_is_array(call);
        }
        if name == "Object.assign" && self.types.profile.property_deletion() {
            return self.object_assign(call);
        }
        if name == "Object.is" && self.types.profile.property_deletion() {
            return self.object_is(call);
        }
        if name == "Object.entries" && self.types.profile.property_deletion() {
            return self.object_entries(call);
        }
        if name == "Object.keys" {
            return self.object_enumeration(call, object_enumeration::Kind::Keys);
        }
        if name == "Object.values" {
            return self.object_enumeration(call, object_enumeration::Kind::Values);
        }
        if name == "Object.hasOwn" && self.types.profile.missing() {
            return self.own_property(call);
        }
        if name == "varta.result.capture" {
            return self.capture(call);
        }
        if name == "JSON.parse" && self.types.profile.unknown_json() {
            return self.global_parse_json(call);
        }
        if name == "JSON.stringify" {
            if call.args.len() != 1 {
                return Err(invalid("JSON.stringify expects one value"));
            }
            let value = self.expr(&call.args[0].expr)?;
            // `JSON.stringify(x)` with `x: never` (v38) is unreachable.
            if value.ty == Type::Never && self.types.profile.structural_subtyping() {
                return Ok(Expr {
                    span: call.span,
                    ty: Type::String,
                    kind: E::Sequence(
                        Box::new(value),
                        Box::new(Expr {
                            span: call.span,
                            ty: Type::String,
                            kind: E::Zero,
                        }),
                    ),
                });
            }
            if collections::reaches(value.ty, self.types) {
                return Err(self
                    .source
                    .locate(collections::opaque("JSON.stringify"), value.span));
            }
            let value = self.physical_array(value)?;
            if value.ty.profile_kind(self.types.profile).is_none()
                || value.ty == Type::BigInt && !self.types.profile.bigint_values()
            {
                return Err(unsupported("JSON.stringify value type"));
            }
            // V37 (`json_stringify_v17`) skips private state, so prelude,
            // Error and #private instances serialize their own properties.
            let hidden = self.types.profile.class_generics();
            if hidden && classes::reaches_json_form(value.ty, self.types) {
                return Err(self.source.locate(
                    unsupported("JSON.stringify of a URL or a RegExp match array; their JSON forms (toJSON, array) are not modelled"),
                    value.span,
                ));
            }
            if !hidden && classes::reaches_prelude(value.ty, self.types) {
                return Err(self.source.locate(
                    unsupported("JSON.stringify of an engine Web type instance (Headers, AbortController, AbortSignal, DOMException); its state is private"),
                    value.span,
                ));
            }
            if !hidden && classes::reaches_instance(value.ty, self.types) {
                return Err(self.source.locate(
                    unsupported("JSON.stringify of an Error subclass instance; own-property enumerability is not modelled"),
                    value.span,
                ));
            }
            if !hidden && classes::reaches_private(value.ty, self.types) {
                return Err(self.source.locate(
                    unsupported("JSON.stringify of a class instance with #private fields; private names are not properties"),
                    value.span,
                ));
            }
            if function_fields::reaches_callable_to_json(value.ty, self.types) {
                return Err(self.source.locate(
                    unsupported("JSON.stringify of a value with a function-valued toJSON field; the host cannot call guest code"),
                    value.span,
                ));
            }
            // A function (v36) serializes like `undefined`.
            let function = |ty: &Type| matches!(ty, Type::Function(_));
            let ty = if value.ty == Type::Unknown
                || value.ty.is_union()
                    && unions::members(value.ty, self.types)
                        .iter()
                        .any(|ty| *ty == Type::Undefined || function(ty))
            {
                unions::intern(vec![Type::String, Type::Undefined], self.types)?
            } else if value.ty == Type::Undefined || function(&value.ty) {
                Type::Undefined
            } else {
                Type::String
            };
            let status = self
                .types
                .profile
                .bigint_values()
                .then(|| self.hidden(Type::Number));
            let result = Expr {
                span: call.span,
                ty,
                kind: E::Json(Box::new(value), status),
            };
            let result = if let Some(status) = status {
                self.bigint_checked_result(result, status)?
            } else {
                result
            };
            // V38 types the result `string`, as TypeScript does; a value
            // without a JSON form (`undefined`, a function) throws instead.
            if self.types.profile.structural_subtyping() && result.ty.is_union() {
                let span = call.span;
                return self.assert_value(
                    result,
                    Type::String,
                    "string",
                    "JSON.stringify produced undefined: the value has no JSON form",
                    span,
                );
            }
            return Ok(result);
        }
        if let Some(signature) = self.functions.get(&name).cloned() {
            if signature.index == self.entry {
                return Err(unsupported("calling main"));
            }
            if !signature.shape.extended() && call.args.len() != signature.params.len() {
                return Err(invalid(format!(
                    "function {name} expects {} arguments",
                    signature.params.len()
                )));
            }
            let values = self.call_arguments(&call.args, &signature.params, &signature.shape)?;
            let mut args = Vec::new();
            if let Some(ty) = signature.receiver {
                let value = unions::widen(
                    Expr {
                        span: call.span,
                        ty: Type::Undefined,
                        kind: E::Integer(0),
                    },
                    Some(ty),
                    self.types,
                )?;
                if value.ty != ty {
                    return Err(self.source.locate(
                        type_error("bare declaration call requires an undefined-capable this type"),
                        call.span,
                    ));
                }
                args.push(value);
            }
            args.extend(self.bind_arguments(
                values,
                &signature.params,
                &signature.shape,
                call.span,
            )?);
            return Ok(Expr {
                span: call.span,
                ty: signature.result,
                kind: E::Call(signature.index, args),
            });
        }
        if self.timer_builtin(&name) {
            return self.timer_call(call, &name);
        }
        if matches!(name.as_str(), "Boolean" | "isFinite" | "isNaN") {
            return self.primitive_builtin(call, &name);
        }
        if name == "parseInt" || name == "parseFloat" {
            return self.number_parse(call, name == "parseInt");
        }
        if matches!(name.as_str(), "BigInt.asIntN" | "BigInt.asUintN")
            && self.binding("BigInt").is_err()
            && !self.outer.contains_key("BigInt")
            && !self.functions.contains_key("BigInt")
        {
            return self.bigint_width(call, name == "BigInt.asIntN");
        }
        if name == "BigInt" {
            return self.bigint_constructor(call);
        }
        if name == "String" || name == "Number" {
            return self.primitive_conversion(call, name == "Number");
        }
        if name == "varta.result.status" {
            if !call.args.is_empty() {
                return Err(invalid("varta.result.status expects no arguments"));
            }
            return Ok(Expr {
                span: call.span,
                ty: self.types.number(),
                kind: E::Status,
            });
        }
        let (host, expected, result) = match name.as_str() {
            "console.log" | "log" => {
                if call.args.len() != 1 {
                    return Err(invalid("log expects one argument"));
                }
                let mut arg = self.expr(&call.args[0].expr)?;
                if collections::reaches(arg.ty, self.types) {
                    return Err(self.source.locate(collections::opaque("logging"), arg.span));
                }
                if arg.ty == Type::BigInt {
                    arg = self.bigint_text(arg)?;
                }
                if arg.ty.is_error() {
                    arg = Expr {
                        span: arg.span,
                        ty: Type::String,
                        kind: E::ErrorText(Box::new(arg)),
                    };
                }
                if matches!(arg.ty, Type::Promise(_)) && self.types.profile.promise_values() {
                    // A value16 promise is logged through its tagged cell.
                    arg = unknown::widen(arg, self.types)?;
                }
                if matches!(arg.ty, Type::Function(_)) && self.types.profile.function_fields() {
                    // So is a value17 function.
                    arg = unknown::widen(arg, self.types)?;
                }
                let host = match arg.ty {
                    Type::String => Host::LogString,
                    Type::Number | Type::Boolean => Host::LogInt,
                    Type::Float64 => Host::LogFloat,
                    Type::Undefined => Host::LogUndefined,
                    Type::Union(_) | Type::ReferenceUnion(_) | Type::Unknown => Host::UnionLog,
                    _ => {
                        return Err(self
                            .source
                            .locate(type_error(format!("cannot log {}", arg.ty)), arg.span));
                    }
                };
                return Ok(Expr {
                    span: call.span,
                    ty: Type::Void,
                    kind: E::Host(self.types.profile.host_operation(host), vec![arg]),
                });
            }
            "varta.http.stop" => (Host::HttpStop, None, Type::Void),
            "varta.time.now" => (Host::Time, None, self.types.number()),
            "varta.env.has" => (Host::EnvHas, Some(Type::String), Type::Boolean),
            "varta.env.get" => (Host::EnvGet, Some(Type::String), Type::String),
            "varta.env.print" => (Host::EnvPrint, Some(Type::String), Type::Void),
            "varta.fs.read" => (Host::FsRead, Some(Type::String), Type::String),
            "varta.fs.print" => (Host::FsPrint, Some(Type::String), Type::Void),
            "varta.net.fetch" => (Host::NetFetch, Some(Type::String), self.types.number()),
            "varta.net.get" => (Host::NetGet, Some(Type::String), Type::String),
            "varta.process.exit" if self.types.profile.binary64() => {
                (Host::ExitFloat, Some(Type::Float64), Type::Void)
            }
            "varta.process.exit" => (Host::Exit, Some(Type::Number), Type::Void),
            _ => return Err(unsupported("call")),
        };
        if call.args.len() != usize::from(expected.is_some()) {
            return Err(invalid(format!("invalid argument count for {name}")));
        }
        let mut args = Vec::new();
        for arg in &call.args {
            let value = self.expr(&arg.expr)?;
            if Some(value.ty) != expected {
                return Err(self.source.locate(
                    type_error(format!(
                        "argument type mismatch for {name}: expected {}, found {}",
                        expected.unwrap_or(Type::Void),
                        value.ty
                    )),
                    value.span,
                ));
            }
            args.push(value);
        }
        Ok(Expr {
            span: call.span,
            ty: result,
            kind: E::Host(self.types.profile.host_operation(host), args),
        })
    }
}

pub(super) fn callee_name(expr: &ast::Expr) -> Option<String> {
    match expr {
        ast::Expr::Ident(name) => Some(name.sym.to_string()),
        ast::Expr::Member(member) => {
            let ast::MemberProp::Ident(name) = &member.prop else {
                return None;
            };
            Some(format!("{}.{}", callee_name(&member.obj)?, name.sym))
        }
        _ => None,
    }
}
