//! Static Math/Number properties lower to exact binary64 values, never host calls.
use super::stmt::unparen;
use super::*;

impl Lower<'_> {
    pub(super) fn write_member_base(&mut self, member: &ast::MemberExpr) -> Result<Expr> {
        if self.numeric_constant(member)?.is_some() {
            return Err(self.source.locate(
                type_error("cannot modify a read-only Math/Number constant"),
                member.span,
            ));
        }
        self.reject_readonly_write(&member.obj)
            .map_err(|e| self.source.locate(e, member.span))?;
        self.expr(&member.obj)
    }
    pub(super) fn numeric_constant(&self, member: &ast::MemberExpr) -> Result<Option<Expr>> {
        let ast::Expr::Ident(namespace) = unparen(&member.obj) else {
            return Ok(None);
        };
        let name = namespace.sym.as_ref();
        if !matches!(name, "Math" | "Number")
            || self.binding(name).is_ok()
            || self.functions.contains_key(name)
        {
            // Real lexical values keep their ordinary property and TDZ rules.
            return Ok(None);
        }
        if !self.types.profile.numeric_constants() {
            return Err(unsupported(
                "Math/Number constants require checked-f64-utf16-v15",
            ));
        }
        let property = match &member.prop {
            ast::MemberProp::Ident(id) => id.sym.to_string(),
            ast::MemberProp::Computed(key) => {
                let ast::Expr::Lit(ast::Lit::Str(text)) = unparen(&key.expr) else {
                    return Err(unsupported("dynamic Math/Number constant key"));
                };
                text.value
                    .as_str()
                    .ok_or_else(|| unsupported("non-UTF-8 constant name"))?
                    .to_owned()
            }
            _ => return Err(unsupported("private Math/Number constant")),
        };
        let value = match (name, property.as_str()) {
            ("Math", "E") => std::f64::consts::E,
            ("Math", "LN10") => std::f64::consts::LN_10,
            ("Math", "LN2") => std::f64::consts::LN_2,
            ("Math", "LOG10E") => std::f64::consts::LOG10_E,
            ("Math", "LOG2E") => std::f64::consts::LOG2_E,
            ("Math", "PI") => std::f64::consts::PI,
            ("Math", "SQRT1_2") => std::f64::consts::FRAC_1_SQRT_2,
            ("Math", "SQRT2") => std::f64::consts::SQRT_2,
            ("Number", "EPSILON") => f64::EPSILON,
            ("Number", "MAX_SAFE_INTEGER") => 9007199254740991.0,
            ("Number", "MIN_SAFE_INTEGER") => -9007199254740991.0,
            ("Number", "MAX_VALUE") => f64::MAX,
            ("Number", "MIN_VALUE") => f64::from_bits(1),
            ("Number", "NaN") => f64::NAN,
            ("Number", "NEGATIVE_INFINITY") => f64::NEG_INFINITY,
            ("Number", "POSITIVE_INFINITY") => f64::INFINITY,
            _ => {
                return Err(unsupported(&format!(
                    "{name}.{property} constant or method value"
                )));
            }
        };
        Ok(Some(Expr {
            span: member.span,
            ty: Type::Float64,
            kind: E::Float64(value),
        }))
    }
}
