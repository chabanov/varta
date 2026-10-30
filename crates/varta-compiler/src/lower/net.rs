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

/// The v38 (`web_bytes`) byte-body intrinsic of the `fetch` prelude, in the
/// same shape; under `web_bytes`, `varta.net.start` starts with descriptor
/// version 2 (`net_request_start_v2`).
const BYTE_INTRINSICS: [(&str, Host, Type, Type); 1] = [(
    "varta.net.readBytes",
    Host::NetBodyReadBytes,
    Type::Float64,
    Type::Void,
)];
/// The v38 (`web_bytes`) `process.env` snapshot of the global prelude.
const ENV_SNAPSHOT: &str = "varta.env.snapshot";

/// The v38 (`turn_budgets`) intrinsics of the permission and host event
/// preludes, in the same shape.
const INSTANCE_INTRINSICS: [(&str, Host, Type, Type); 6] = [
    (
        "varta.permissions.state",
        Host::PermissionQuery,
        Type::String,
        Type::Float64,
    ),
    (
        "varta.permissions.ask",
        Host::PermissionRequest,
        Type::String,
        Type::Float64,
    ),
    (
        "varta.events.open",
        Host::EventSourceOpen,
        Type::String,
        Type::Float64,
    ),
    (
        "varta.events.take",
        Host::EventTake,
        Type::Float64,
        Type::Float64,
    ),
    (
        "varta.events.text",
        Host::EventText,
        Type::Float64,
        Type::String,
    ),
    (
        "varta.events.release",
        Host::EventSourceClose,
        Type::Float64,
        Type::Void,
    ),
];

impl Lower<'_> {
    /// The intrinsic call `name(argument)` of the `fetch` prelude (v37) or of
    /// the permission and host event preludes (v38), if it is one.
    pub(super) fn prelude_net(&mut self, call: &ast::CallExpr, name: &str) -> Result<Option<Expr>> {
        let net = INTRINSICS
            .iter()
            .find(|(intrinsic, ..)| *intrinsic == name)
            .filter(|_| self.types.profile.net_request());
        let instance = INSTANCE_INTRINSICS
            .iter()
            .find(|(intrinsic, ..)| *intrinsic == name)
            .filter(|_| self.types.profile.turn_budgets());
        let bytes = BYTE_INTRINSICS
            .iter()
            .find(|(intrinsic, ..)| *intrinsic == name)
            .filter(|_| self.types.profile.web_bytes());
        if name == ENV_SNAPSHOT
            && self.types.profile.web_bytes()
            && crate::modules::engine::is_prelude_source(self.source)
        {
            let [argument] = call.args.as_slice() else {
                return Err(invalid(format!("{name} expects one name")));
            };
            let value = self.expr(&argument.expr)?;
            if argument.spread.is_some() || call.type_args.is_some() || value.ty != Type::String {
                return Err(invalid(format!("{name} expects one name")));
            }
            return Ok(Some(Expr {
                span: call.span,
                ty: Type::String,
                kind: E::Host(Host::EnvSnapshot, vec![value]),
            }));
        }
        let Some((_, host, argument_type, result)) = net.or(instance).or(bytes) else {
            return Ok(None);
        };
        if !crate::modules::engine::is_prelude_source(self.source) {
            return Ok(None);
        }
        // V38 (`web_bytes`) requests carry descriptor version 2.
        let host = if *host == Host::NetRequestStart && self.types.profile.web_bytes() {
            &Host::NetRequestStartV2
        } else {
            host
        };
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
    const PERMISSIONS: &str = include_str!("../prelude/permissions.ts");
    const EVENTS: &str = include_str!("../prelude/events.ts");

    /// The v38 preludes' mirrored constants match the broker and event
    /// contracts.
    #[test]
    fn the_permission_and_event_preludes_mirror_their_contracts() {
        use varta_common::events::{INVALID_SOURCE, TakeKind};
        use varta_common::permission::{
            DESCRIPTOR_NO_RESOURCE, DESCRIPTOR_RESOURCE, DESCRIPTOR_SEPARATOR,
            DESCRIPTOR_VERSION as PERMISSION_VERSION, INVALID_DESCRIPTOR, PermissionState,
        };
        let constant = |source: &str, file: &str, name: &str, value: &str| {
            let line = format!("const {name} = {value};");
            assert!(source.contains(&line), "{file} lacks `{line}`");
        };
        let permission =
            |name: &str, value: &str| constant(PERMISSIONS, "permissions.ts", name, value);
        permission("DESCRIPTOR_VERSION", &format!("'{PERMISSION_VERSION}'"));
        assert_eq!(DESCRIPTOR_SEPARATOR, '\n');
        permission("SEPARATOR", "'\\n'");
        permission("NO_RESOURCE", &format!("'{DESCRIPTOR_NO_RESOURCE}'"));
        permission("WITH_RESOURCE", &format!("'{DESCRIPTOR_RESOURCE}'"));
        permission("INVALID_DESCRIPTOR", &INVALID_DESCRIPTOR.to_string());
        for state in PermissionState::ALL {
            let name = state.name().to_uppercase();
            permission(&name, &state.code().to_string());
            assert!(PERMISSIONS.contains(&format!("'{}'", state.name())));
        }
        permission("KIND_EVENT", &(TakeKind::Event as i32).to_string());
        let event = |name: &str, value: &str| constant(EVENTS, "events.ts", name, value);
        event("INVALID_SOURCE", &format!("{}", INVALID_SOURCE as i32));
        event("KIND_EVENT", &(TakeKind::Event as i32).to_string());
        event("KIND_CLOSED", &(TakeKind::Closed as i32).to_string());
    }

    /// The v38 byte-body prelude mirrors descriptor version 2 and the same
    /// completion codes.
    #[test]
    fn the_byte_fetch_prelude_mirrors_the_net_contract() {
        use varta_common::net::{DESCRIPTOR_BYTES, DESCRIPTOR_VERSION_BYTES};
        const BYTES: &str = include_str!("../prelude/fetch_bytes.ts");
        let constant = |name: &str, value: &str| {
            let line = format!("const {name} = {value};");
            assert!(BYTES.contains(&line), "fetch_bytes.ts lacks `{line}`");
        };
        constant(
            "DESCRIPTOR_VERSION",
            &format!("'{DESCRIPTOR_VERSION_BYTES}'"),
        );
        constant("SEPARATOR", "'\\n'");
        constant("NO_BODY", &format!("'{DESCRIPTOR_NO_BODY}'"));
        constant("WITH_BODY", &format!("'{DESCRIPTOR_BODY}'"));
        constant("WITH_BYTES", &format!("'{DESCRIPTOR_BYTES}'"));
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
