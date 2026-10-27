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

impl Lower<'_> {
    pub(super) fn string_range(
        &mut self,
        call: &ast::CallExpr,
        value: Expr,
        method: &str,
    ) -> Result<Expr> {
        if !self.types.profile.closures() {
            return Err(unsupported(&format!(
                "String.{method} requires checked-f64-utf16-v12"
            )));
        }
        if value.ty != Type::String {
            return Err(self.source.locate(
                type_error(format!(
                    "String.{method} requires a narrowed string receiver"
                )),
                value.span,
            ));
        }
        if call.args.len() > 2 {
            return Err(invalid(format!(
                "String.{method} expects zero to two number/undefined arguments"
            )));
        }
        let start =
            self.string_numeric_bound(call.args.first(), call.span, "string range bound")?;
        let end = self.string_numeric_bound(call.args.get(1), call.span, "string range bound")?;
        Ok(Expr {
            span: call.span,
            ty: Type::String,
            kind: E::StringRange {
                value: Box::new(value),
                start: Box::new(start),
                end: Box::new(end),
                substring: method == "substring",
            },
        })
    }
    fn string_numeric_bound(
        &mut self,
        argument: Option<&ast::ExprOrSpread>,
        span: swc_common::Span,
        description: &str,
    ) -> Result<Expr> {
        let value = if let Some(argument) = argument {
            self.expr(&argument.expr)?
        } else {
            Expr {
                span,
                ty: Type::Undefined,
                kind: E::Integer(0),
            }
        };
        if unions::members(value.ty, self.types)
            .iter()
            .any(|ty| !matches!(ty, Type::Float64 | Type::Undefined))
        {
            return Err(self.source.locate(
                type_error(format!(
                    "{description} requires number or undefined; coercion is unsupported"
                )),
                value.span,
            ));
        }
        Ok(value)
    }
}

impl Lower<'_> {
    pub(super) fn string_search(
        &mut self,
        call: &ast::CallExpr,
        value: Expr,
        method: &str,
    ) -> Result<Expr> {
        if !self.types.profile.closures() {
            return Err(unsupported(&format!(
                "String.{method} requires checked-f64-utf16-v12"
            )));
        }
        if value.ty != Type::String {
            return Err(self.source.locate(
                type_error(format!(
                    "String.{method} requires a narrowed string receiver"
                )),
                value.span,
            ));
        }
        if !(1..=2).contains(&call.args.len()) {
            return Err(invalid(format!(
                "String.{method} expects one string and an optional number/undefined position"
            )));
        }
        let needle = self.expr(&call.args[0].expr)?;
        if needle.ty != Type::String {
            return Err(self.source.locate(
                type_error("string search requires a string needle; coercion is unsupported"),
                needle.span,
            ));
        }
        let position =
            self.string_numeric_bound(call.args.get(1), call.span, "string search position")?;
        let mode = match method {
            "includes" => ir::StringSearch::Includes,
            "startsWith" => ir::StringSearch::StartsWith,
            "endsWith" => ir::StringSearch::EndsWith,
            "indexOf" => ir::StringSearch::IndexOf,
            "lastIndexOf" => ir::StringSearch::LastIndexOf,
            _ => return Err(invalid("unknown string search method")),
        };
        let ty = match method {
            "includes" | "startsWith" | "endsWith" => Type::Boolean,
            "indexOf" | "lastIndexOf" => self.types.number(),
            _ => unreachable!(),
        };
        Ok(Expr {
            span: call.span,
            ty,
            kind: E::StringSearch(mode, Box::new(value), Box::new(needle), Box::new(position)),
        })
    }
}

impl Lower<'_> {
    pub(super) fn string_trim(
        &mut self,
        call: &ast::CallExpr,
        value: Expr,
        method: &str,
    ) -> Result<Expr> {
        if !self.types.profile.closures() {
            return Err(unsupported(&format!(
                "String.{method} requires checked-f64-utf16-v12"
            )));
        }
        if value.ty != Type::String {
            return Err(self.source.locate(
                type_error(format!(
                    "String.{method} requires a narrowed string receiver"
                )),
                value.span,
            ));
        }
        if !call.args.is_empty() {
            return Err(invalid(format!("String.{method} expects no arguments")));
        }
        let mode = match method {
            "trim" => ir::StringTrimMode::Trim,
            "trimStart" => ir::StringTrimMode::TrimStart,
            "trimEnd" => ir::StringTrimMode::TrimEnd,
            _ => return Err(invalid("unknown string trim method")),
        };
        Ok(Expr {
            span: call.span,
            ty: Type::String,
            kind: E::StringTrim {
                value: Box::new(value),
                mode,
            },
        })
    }
}
