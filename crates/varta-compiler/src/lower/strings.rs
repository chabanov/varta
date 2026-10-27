use super::*;
impl Lower<'_> {
    pub(super) fn string_repeat(
        &mut self,
        call: &ast::CallExpr,
        member: &ast::MemberExpr,
    ) -> Result<Expr> {
        if !self.types.profile.error_families() {
            return Err(unsupported("String.repeat requires checked-f64-utf16-v9"));
        }
        let value = self.expr(&member.obj)?;
        if value.ty != Type::String {
            return Err(type_error("repeat requires a string receiver"));
        }
        if call.args.len() > 1 {
            return Err(invalid(
                "String.repeat expects zero or one number/undefined argument",
            ));
        }
        let count = if let Some(arg) = call.args.first() {
            self.expr(&arg.expr)?
        } else {
            Expr {
                span: call.span,
                ty: Type::Undefined,
                kind: E::Integer(0),
            }
        };
        if unions::members(count.ty, self.types)
            .iter()
            .any(|ty| !matches!(ty, Type::Float64 | Type::Undefined))
        {
            return Err(type_error(
                "String.repeat count requires number or undefined; coercion is unsupported",
            ));
        }
        let message_ty = unions::intern(vec![Type::String, Type::Undefined], self.types)?;
        let message = unions::widen(
            Expr {
                span: call.span,
                ty: Type::String,
                kind: E::String(
                    self.types
                        .profile
                        .string_bytes("Invalid string repeat count"),
                ),
            },
            Some(message_ty),
            self.types,
        )?;
        let class = varta_common::value::ErrorClass::RangeError;
        let error = Expr {
            span: call.span,
            ty: Type::from_error_class(class),
            kind: E::NewError(class, Box::new(message)),
        };
        let thrown = self.exception_type()?;
        let error = unions::widen(error, Some(thrown), self.types)?;
        // Shared only after all operand evaluation; nested expressions/calls
        // finish using these activation-local slots before the outer copy starts.
        let (length, filled) = if let Some(slots) = self.repeat_slots {
            slots
        } else {
            let slots = (self.hidden(Type::Number), self.hidden(Type::Number));
            self.repeat_slots = Some(slots);
            slots
        };
        Ok(Expr {
            span: call.span,
            ty: Type::String,
            kind: E::StringRepeat {
                value: Box::new(value),
                count: Box::new(count),
                error: Box::new(error),
                length,
                filled,
            },
        })
    }
}
