//! Engine-internal code-unit intrinsics of the v35 Web type preludes. Header
//! names and values are validated and normalized per UTF-16 code unit; doing
//! that with `charCodeAt` would make one host call per unit and exhaust the
//! production host-call and fuel budgets after a few headers. These calls lower
//! to guest helpers instead (`ir::TextUnits`). They exist only inside engine
//! prelude modules: in program modules the names are ordinary unknown calls.
use super::*;
use ir::{TextUnits, UnitClass};

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
    pub(super) fn web_text(&mut self, call: &ast::CallExpr, name: &str) -> Result<Option<Expr>> {
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
