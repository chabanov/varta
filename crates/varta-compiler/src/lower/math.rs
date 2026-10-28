//! Direct Math calls use narrowed Numbers; random ignores evaluated arguments.
use super::*;
impl Lower<'_> {
    pub(super) fn math_call(
        &mut self,
        call: &ast::CallExpr,
        member: &ast::MemberExpr,
    ) -> Result<Expr> {
        if !self.types.profile.math_intrinsics() {
            return Err(unsupported("Math intrinsics require checked-f64-utf16-v15"));
        }
        if self.binding("Math").is_ok() || self.functions.contains_key("Math") {
            return Err(unsupported("shadowed Math intrinsic"));
        }
        let ast::MemberProp::Ident(method) = &member.prop else {
            return Err(unsupported("computed Math intrinsic"));
        };
        if method.sym == "hypot" {
            return self.math_hypot(call);
        }
        let mode = match method.sym.as_ref() {
            "random" => {
                if !self.types.profile.random() {
                    return Err(unsupported("Math.random requires checked-f64-utf16-v17"));
                }
                ir::MathIntrinsic::Random
            }
            "abs" => ir::MathIntrinsic::Abs,
            "floor" => ir::MathIntrinsic::Floor,
            "ceil" => ir::MathIntrinsic::Ceil,
            "trunc" => ir::MathIntrinsic::Trunc,
            "round" => ir::MathIntrinsic::Round,
            "sqrt" => ir::MathIntrinsic::Sqrt,
            "sign" => ir::MathIntrinsic::Sign,
            "fround" => ir::MathIntrinsic::Fround,
            "f16round" => ir::MathIntrinsic::F16round,
            "clz32" => ir::MathIntrinsic::Clz32,
            "imul" => ir::MathIntrinsic::Imul,
            "min" => ir::MathIntrinsic::Min,
            "max" => ir::MathIntrinsic::Max,
            "pow" => ir::MathIntrinsic::Pow,
            _ => {
                let Some(operation) =
                    varta_common::native_math::NativeMath::from_name(method.sym.as_ref())
                else {
                    return Err(unsupported(&format!("Math.{} intrinsic", method.sym)));
                };
                if !self.types.profile.native_math() {
                    return Err(unsupported(
                        "native Math operations require checked-f64-utf16-v16",
                    ));
                }
                ir::MathIntrinsic::Native(operation)
            }
        };
        let arity = match mode {
            ir::MathIntrinsic::Min | ir::MathIntrinsic::Max | ir::MathIntrinsic::Random => None,
            ir::MathIntrinsic::Native(operation) => Some(operation.arity()),
            ir::MathIntrinsic::Imul | ir::MathIntrinsic::Pow => Some(2),
            _ => Some(1),
        };
        if call.args.iter().any(|arg| arg.spread.is_some()) {
            return Err(unsupported("spread to a Math intrinsic"));
        }
        if let Some(arity) = arity
            && call.args.len() != arity
        {
            return Err(unsupported(&format!(
                "Math.{} requires exactly {arity} Number arguments",
                method.sym
            )));
        }
        let mut args = Vec::new();
        for arg in &call.args {
            let value = self.expr(&arg.expr)?;
            if mode != ir::MathIntrinsic::Random && value.ty != Type::Float64 {
                return Err(self.source.locate(
                    type_error(
                        "Math intrinsic argument must be a narrowed Number; coercion is unsupported",
                    ),
                    value.span,
                ));
            }
            args.push(value);
        }
        Ok(Expr {
            span: call.span,
            ty: Type::Float64,
            kind: E::Math(mode, args),
        })
    }
}
