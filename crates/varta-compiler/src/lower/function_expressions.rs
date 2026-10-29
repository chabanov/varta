//! Anonymous strict function expressions share typed lifting and managed captures.
use super::functions::LiftContext;
use super::*;

pub(super) fn receiver_type(
    ann: Option<&ast::TsTypeAnn>,
    types: &mut ir::ValueTypes,
) -> Result<Type> {
    let ty = annotation(ann, types)?
        .ok_or_else(|| invalid("this parameter requires a type annotation"))?;
    let ty = if ty == Type::Void {
        Type::Undefined
    } else {
        ty
    };
    if ty == Type::BigInt && !types.profile.bigint_values()
        || ty.value_kind().is_none() && !matches!(ty, Type::Function(_))
    {
        return Err(unsupported("this type other than a checked JS value"));
    }
    Ok(ty)
}
impl Lower<'_> {
    pub(super) fn function_expression(
        &mut self,
        expression: &ast::FnExpr,
        expected: Option<Type>,
    ) -> Result<Expr> {
        let context = match self.function_context(expected).or(expected) {
            Some(Type::Function(id)) => Some(self.types.functions[id as usize].clone()),
            Some(Type::Unknown) if self.types.profile.function_fields() => None,
            Some(_) => {
                return Err(type_error(
                    "function expression requires a function context",
                ));
            }
            None => None,
        };
        self.function_expression_context(
            expression,
            context.as_ref().map(|t| t.params.clone()),
            context.as_ref().map(|t| t.result),
            context.as_ref().map(|t| t.shape.clone()),
            context.and_then(|t| t.receiver),
        )
    }
    pub(super) fn function_expression_context(
        &mut self,
        expression: &ast::FnExpr,
        params: Option<Vec<Type>>,
        result: Option<Type>,
        shape: Option<ir::ParameterShape>,
        receiver: Option<Type>,
    ) -> Result<Expr> {
        if !self.types.profile.closures() {
            return Err(unsupported(
                "function expressions require checked-f64-utf16-v12",
            ));
        }
        let function = &expression.function;
        let asynchronous = function.is_async && self.types.profile.async_methods();
        if function.is_async && !asynchronous && self.types.profile.async_functions() {
            return Err(promises::rejected("async function expression"));
        }
        if function.is_async && function.is_generator && self.types.profile.async_generators() {
            return Err(promises::async_generator_rejected(
                "async generator function expression",
            ));
        }
        if function.is_async && !asynchronous
            || function.is_generator
            || function.type_params.is_some()
            || !function.decorators.is_empty()
        {
            return Err(unsupported(
                "async/generator/generic/decorated function expression",
            ));
        }
        let mut positional = Vec::new();
        let annotated_receiver = function
            .this_param
            .as_ref()
            .map(|p| {
                receiver_type(p.type_ann.as_deref(), self.types)
                    .map_err(|e| self.source.locate(e, p.span))
            })
            .transpose()?;
        for parameter in &function.params {
            if !parameter.decorators.is_empty() {
                return Err(unsupported("parameter decorator"));
            }
            positional.push(parameter.pat.clone());
        }
        if let (Some(actual), Some(expected)) = (annotated_receiver, receiver)
            && actual != expected
        {
            return Err(type_error(
                "function this annotation does not match context",
            ));
        }
        let body = function
            .body
            .as_ref()
            .ok_or_else(|| invalid("function expression body is required"))?;
        let arrow = ast::ArrowExpr {
            span: function.span,
            params: positional,
            body: Box::new(ast::ArrowFunctionBody::FunctionBody(body.clone())),
            return_type: function.return_type.clone(),
            is_async: asynchronous,
            ..Default::default()
        };
        self.arrow_context(
            &arrow,
            LiftContext {
                params,
                shape,
                result,
                infer: true,
                receiver: annotated_receiver.or(receiver),
                lexical: false,
                self_name: expression.ident.clone(),
            },
        )
    }
    pub(super) fn function_call_method(
        &mut self,
        call: &ast::CallExpr,
        member: &ast::MemberExpr,
    ) -> Result<Expr> {
        let callee = self.expr(&member.obj)?;
        let Type::Function(id) = callee.ty else {
            return Err(type_error("call requires a typed function receiver"));
        };
        let signature = self.types.functions[id as usize].clone();
        let receiver = if let Some(arg) = call.args.first() {
            self.expr_expected(&arg.expr, signature.receiver)?
        } else {
            Expr {
                span: call.span,
                ty: Type::Undefined,
                kind: E::Integer(0),
            }
        };
        let args = self.call_arguments(
            call.args.get(1..).unwrap_or_default(),
            &signature.params,
            &signature.shape,
        )?;
        self.invoke_receiver(callee, Some(receiver), args, call.span)
    }
}
