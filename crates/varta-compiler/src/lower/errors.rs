use super::*;
use varta_common::{language::LanguageProfile, value::ErrorClass};
pub(super) fn classes(profile: LanguageProfile) -> impl Iterator<Item = ErrorClass> {
    ErrorClass::ALL
        .iter()
        .copied()
        .filter(move |class| profile.error_families() || *class == ErrorClass::Error)
}
pub(super) fn annotation(class: ErrorClass, types: &mut ir::ValueTypes) -> Result<Type> {
    if !types.profile.errors() {
        return Err(unsupported("Error requires checked-f64-utf16-v8"));
    }
    if class != ErrorClass::Error && !types.profile.error_families() {
        return Err(unsupported(
            "native error families require checked-f64-utf16-v9",
        ));
    }
    if class == ErrorClass::Error && types.profile.error_families() {
        unions::intern(
            classes(types.profile).map(Type::from_error_class).collect(),
            types,
        )
    } else {
        Ok(Type::from_error_class(class))
    }
}
impl Lower<'_> {
    pub(super) fn error_value(&self, ty: Type) -> bool {
        unions::members(ty, self.types)
            .iter()
            .all(|ty| ty.is_error())
    }
    pub(super) fn error_callee(&self, expr: &ast::Expr) -> Option<ErrorClass> {
        let ast::Expr::Ident(id) = expr else {
            return None;
        };
        let class = ErrorClass::from_name(id.sym.as_ref())?;
        (self.binding(id.sym.as_ref()).is_err() && !self.functions.contains_key(id.sym.as_ref()))
            .then_some(class)
    }
    pub(super) fn error_constructor(
        &mut self,
        class: ErrorClass,
        args: &[ast::ExprOrSpread],
        generic: bool,
        span: swc_common::Span,
    ) -> Result<Expr> {
        if !self.types.profile.errors() {
            return Err(unsupported("Error requires checked-f64-utf16-v8"));
        }
        if class != ErrorClass::Error && !self.types.profile.error_families() {
            return Err(unsupported(
                "native error families require checked-f64-utf16-v9",
            ));
        }
        if generic || args.len() > 1 || args.iter().any(|a| a.spread.is_some()) {
            return Err(unsupported(
                "error constructor accepts zero or one string/undefined argument",
            ));
        }
        let value = if let Some(arg) = args.first() {
            self.expr(&arg.expr)?
        } else {
            Expr {
                span,
                ty: Type::Undefined,
                kind: E::Integer(0),
            }
        };
        if unions::members(value.ty, self.types)
            .iter()
            .any(|ty| !matches!(ty, Type::String | Type::Undefined))
        {
            return Err(type_error(
                "Error message must be string or undefined; coercion is unsupported",
            ));
        }
        let target = unions::intern(vec![Type::String, Type::Undefined], self.types)?;
        Ok(Expr {
            span,
            ty: Type::from_error_class(class),
            kind: E::NewError(
                class,
                Box::new(unions::widen(value, Some(target), self.types)?),
            ),
        })
    }
    pub(super) fn new_error(&mut self, new: &ast::NewExpr) -> Result<Expr> {
        let class = self.error_callee(&new.callee).ok_or_else(|| {
            unsupported("constructor other than an unshadowed supported error class")
        })?;
        self.error_constructor(
            class,
            new.args.as_deref().unwrap_or_default(),
            new.type_args.is_some(),
            new.span,
        )
    }
    pub(super) fn instance_of_error(&mut self, binary: &ast::BinExpr) -> Result<Expr> {
        let class = self
            .error_callee(&binary.right)
            .filter(|class| {
                self.types.profile.errors()
                    && (*class == ErrorClass::Error || self.types.profile.error_families())
            })
            .ok_or_else(|| {
                unsupported("instanceof requires an unshadowed supported error class")
            })?;
        let value = self.expr(&binary.left)?;
        if value.ty.value_kind().is_none() {
            return Err(unsupported("instanceof operand type"));
        }
        let kind = if value.ty.is_union() {
            if self.types.profile.error_families() {
                E::Host(
                    self.types.profile.host_operation(Host::ValueIsErrorClass),
                    vec![
                        value,
                        Expr {
                            span: binary.span,
                            ty: Type::Number,
                            kind: E::Integer(class as i32),
                        },
                    ],
                )
            } else {
                E::Host(
                    self.types.profile.host_operation(Host::ValueIsError),
                    vec![value],
                )
            }
        } else {
            let matched = i32::from(
                value.ty.is_error()
                    && (class == ErrorClass::Error || value.ty.error_class() == Some(class)),
            );
            E::Sequence(
                Box::new(value),
                Box::new(Expr {
                    span: binary.span,
                    ty: Type::Boolean,
                    kind: E::Integer(matched),
                }),
            )
        };
        Ok(Expr {
            span: binary.span,
            ty: Type::Boolean,
            kind,
        })
    }
    pub(super) fn error_field(&mut self, member: &ast::MemberExpr, value: Expr) -> Result<Expr> {
        let ast::MemberProp::Ident(name) = &member.prop else {
            return Err(unsupported("computed Error property"));
        };
        let kind = match name.sym.as_ref() {
            "message" => E::ErrorMessage(Box::new(value)),
            "name" => E::ErrorName(Box::new(value)),
            _ => return Err(unsupported("Error property other than name/message")),
        };
        Ok(Expr {
            span: member.span,
            ty: Type::String,
            kind,
        })
    }
    pub(super) fn error_to_string(
        &mut self,
        call: &ast::CallExpr,
        member: &ast::MemberExpr,
    ) -> Result<Expr> {
        let value = self.expr(&member.obj)?;
        if !self.error_value(value.ty) || !call.args.is_empty() {
            return Err(unsupported(
                "toString requires Error receiver and no arguments",
            ));
        }
        Ok(Expr {
            span: call.span,
            ty: Type::String,
            kind: E::ErrorText(Box::new(value)),
        })
    }
}
