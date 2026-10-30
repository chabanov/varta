//! Engine-internal code-unit intrinsics of the v35 Web type preludes. Header
//! names and values are validated and normalized per UTF-16 code unit; doing
//! that with `charCodeAt` would make one host call per unit and exhaust the
//! production host-call and fuel budgets after a few headers. These calls lower
//! to guest helpers instead (`ir::TextUnits`). They exist only inside engine
//! prelude modules: in program modules the names are ordinary unknown calls.
use super::*;
use ir::{TextUnits, UnitClass};

/// The v37 code-unit read of the `URL` prelude.
const UNIT_AT: &str = "varta.web.unitAt";

const INTRINSICS: [(&str, TextUnits); 5] = [
    (
        "varta.web.indexAboveByte",
        TextUnits::Index(UnitClass::AboveByte),
    ),
    (
        "varta.web.indexNonToken",
        TextUnits::Index(UnitClass::NonToken),
    ),
    (
        "varta.web.indexForbiddenValue",
        TextUnits::Index(UnitClass::ForbiddenValue),
    ),
    ("varta.web.asciiLowercase", TextUnits::AsciiLowercase),
    (
        "varta.web.trimHttpWhitespace",
        TextUnits::TrimHttpWhitespace,
    ),
];

impl Lower<'_> {
    /// The intrinsic call `name(text)` of an engine prelude, if it is one.
    /// `varta.web.unitAt(text, index)` (v37 `net_request` preludes): the UTF-16
    /// code unit at `index`, or NaN, read in place without a host call (the
    /// `URL` parser classifies every unit of its input). Both arguments must
    /// be identifiers or literals, so no allocation runs between them.
    fn unit_at(&mut self, call: &ast::CallExpr) -> Result<Expr> {
        let [text, index] = call.args.as_slice() else {
            return Err(invalid(format!("{UNIT_AT} expects a string and an index")));
        };
        let simple = |arg: &ast::ExprOrSpread| {
            arg.spread.is_none() && matches!(&*arg.expr, ast::Expr::Ident(_) | ast::Expr::Lit(_))
        };
        if call.type_args.is_some() || !simple(text) || !simple(index) {
            return Err(invalid(format!(
                "{UNIT_AT} expects a local string and a local or literal index"
            )));
        }
        let text = self.expr(&text.expr)?;
        let index = self.expr(&index.expr)?;
        if text.ty != Type::String || index.ty != Type::Float64 {
            return Err(type_error(format!(
                "{UNIT_AT} requires a string and a number"
            )));
        }
        Ok(Expr {
            span: call.span,
            ty: Type::Float64,
            kind: E::TextUnitAt(Box::new(text), Box::new(index)),
        })
    }
    pub(super) fn web_text(&mut self, call: &ast::CallExpr, name: &str) -> Result<Option<Expr>> {
        if name == UNIT_AT
            && self.types.profile.net_request()
            && crate::modules::engine::is_prelude_source(self.source)
        {
            return self.unit_at(call).map(Some);
        }
        let Some((_, op)) = INTRINSICS.iter().find(|(intrinsic, _)| *intrinsic == name) else {
            return Ok(None);
        };
        if !crate::modules::engine::is_prelude_source(self.source) || !self.types.profile.utf16() {
            return Ok(None);
        }
        let [argument] = call.args.as_slice() else {
            return Err(invalid(format!("{name} expects one string argument")));
        };
        if argument.spread.is_some() || call.type_args.is_some() {
            return Err(invalid(format!("{name} expects one string argument")));
        }
        let value = self.expr(&argument.expr)?;
        if value.ty != Type::String {
            return Err(type_error(format!("{name} requires a string")));
        }
        let ty = match op {
            TextUnits::Index(_) => self.types.number(),
            TextUnits::AsciiLowercase | TextUnits::TrimHttpWhitespace => Type::String,
        };
        Ok(Some(Expr {
            span: call.span,
            ty,
            kind: E::TextUnits {
                value: Box::new(value),
                op: *op,
            },
        }))
    }
}
