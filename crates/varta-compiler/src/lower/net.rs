//! Engine-internal network request intrinsics of the v37 `fetch` prelude
//! (`net_request`). Each lowers to one `net_*` host import; the wait for a
//! completion (`varta.net.watch`) is a scheduled frame like a timer's (see
//! `promises/timers.rs`). They exist only inside engine prelude modules: in
//! program modules the names are ordinary unknown calls, so application code
//! can reach the network only through `fetch`.
use super::*;

/// `varta.net.*` intrinsics with one argument: (name, operation, argument
/// type, result type). The result types are the guest-visible types.
const INTRINSICS: [(&str, Host, Type, Type); 5] = [
    (
        "varta.net.start",
        Host::NetRequestStart,
        Type::String,
        Type::Float64,
    ),
    (
        "varta.net.take",
        Host::NetRequestTake,
        Type::Float64,
        Type::Float64,
    ),
    (
        "varta.net.text",
        Host::NetRequestText,
        Type::Float64,
        Type::String,
    ),
    (
        "varta.net.read",
        Host::NetBodyRead,
        Type::Float64,
        Type::Void,
    ),
    (
        "varta.net.abort",
        Host::NetRequestAbort,
        Type::Float64,
        Type::Void,
    ),
];

impl Lower<'_> {
    /// The intrinsic call `name(argument)` of the `fetch` prelude, if it is one.
    pub(super) fn prelude_net(&mut self, call: &ast::CallExpr, name: &str) -> Result<Option<Expr>> {
        let Some((_, host, argument_type, result)) =
            INTRINSICS.iter().find(|(intrinsic, ..)| *intrinsic == name)
        else {
            return Ok(None);
        };
        if !self.types.profile.net_request()
            || !crate::modules::engine::is_prelude_source(self.source)
        {
            return Ok(None);
        }
        let [argument] = call.args.as_slice() else {
            return Err(invalid(format!("{name} expects one argument")));
        };
        if argument.spread.is_some() || call.type_args.is_some() {
            return Err(invalid(format!("{name} expects one argument")));
        }
        let value = self.expr(&argument.expr)?;
        if value.ty != *argument_type {
            return Err(type_error(format!(
                "{name} requires a {argument_type}, found {}",
                value.ty
            )));
        }
        Ok(Some(Expr {
            span: call.span,
            ty: *result,
            kind: E::Host(*host, vec![value]),
        }))
    }
}

#[cfg(test)]
mod tests {
    use varta_common::net::{
        CompletionKind, DESCRIPTOR_BODY, DESCRIPTOR_NO_BODY, DESCRIPTOR_SEPARATOR,
        DESCRIPTOR_VERSION,
    };

    const FETCH: &str = include_str!("../prelude/fetch.ts");

    /// The prelude's mirrored constants match the shared contract.
    #[test]
    fn the_fetch_prelude_mirrors_the_net_contract() {
        let constant = |name: &str, value: &str| {
            let line = format!("const {name} = {value};");
            assert!(FETCH.contains(&line), "fetch.ts lacks `{line}`");
        };
        constant("DESCRIPTOR_VERSION", &format!("'{DESCRIPTOR_VERSION}'"));
        assert_eq!(DESCRIPTOR_SEPARATOR, '\n');
        constant("SEPARATOR", "'\\n'");
        constant("NO_BODY", &format!("'{DESCRIPTOR_NO_BODY}'"));
        constant("WITH_BODY", &format!("'{DESCRIPTOR_BODY}'"));
        for kind in CompletionKind::ALL {
            let name = match kind {
                CompletionKind::Head => "KIND_HEAD",
                CompletionKind::Chunk => "KIND_CHUNK",
                CompletionKind::End => "KIND_END",
                CompletionKind::Failed => "KIND_FAILED",
                CompletionKind::Aborted => "KIND_ABORTED",
                CompletionKind::Denied => "KIND_DENIED",
            };
            constant(name, &(kind as i32).to_string());
        }
    }
}
