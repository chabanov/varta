//! V38 local time (`local_time`, K9 D4): the engine intrinsics of the date
//! prelude that read the recorded host inputs and the linked zone
//! operations. They exist only inside engine prelude modules.
//!
//! - `varta.date.timeZone()` and `varta.date.locale()`: the host's zone
//!   identifier and default locale (`intl_time_zone`, `intl_locale`);
//! - `varta.date.zoneOffset(zone, time, local)`: the offset LocalTime
//!   (`local` 0) or UTC(t) (`local` 1) applies (`date_zone_offset`);
//! - `varta.date.zoneName(zone, locale, time)`: the localized zone name of
//!   `Date.prototype.toString` (`date_zone_name`).
use super::*;

/// Each intrinsic, its operation and its parameter kinds (`true` for a
/// string, `false` for a number), in order.
const INTRINSICS: [(&str, Host, &[bool]); 4] = [
    ("varta.date.timeZone", Host::IntlTimeZone, &[]),
    ("varta.date.locale", Host::IntlLocale, &[]),
    (
        "varta.date.zoneOffset",
        Host::DateZoneOffset,
        &[true, false, false],
    ),
    (
        "varta.date.zoneName",
        Host::DateZoneName,
        &[true, true, false],
    ),
];

impl Lower<'_> {
    /// The local-time intrinsic call `name(...)` of the date prelude, if it
    /// is one.
    pub(super) fn local_time_intrinsic(
        &mut self,
        call: &ast::CallExpr,
        name: &str,
    ) -> Result<Option<Expr>> {
        let Some(&(_, host, parameters)) = INTRINSICS.iter().find(|(n, ..)| *n == name) else {
            return Ok(None);
        };
        if !self.types.profile.local_time() {
            return Ok(None);
        }
        if call.args.len() != parameters.len()
            || call.type_args.is_some()
            || call.args.iter().any(|argument| argument.spread.is_some())
        {
            return Err(invalid(format!(
                "{name} expects {} plain arguments",
                parameters.len()
            )));
        }
        let mut args = Vec::with_capacity(parameters.len());
        for (argument, &string) in call.args.iter().zip(parameters) {
            let value = self.expr(&argument.expr)?;
            let expected = if string {
                Type::String
            } else {
                self.types.number()
            };
            if value.ty != expected {
                return Err(type_error(format!(
                    "{name} requires {} arguments",
                    if string { "string" } else { "number" }
                )));
            }
            args.push(value);
        }
        let ty = if host.returns_string() {
            Type::String
        } else {
            self.types.number()
        };
        Ok(Some(Expr {
            span: call.span,
            ty,
            kind: E::Host(host, args),
        }))
    }
}
