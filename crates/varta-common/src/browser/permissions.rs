//! Permission/event-source wire facts shared with the native broker.
use crate::{GrantKind, HostOperation, capability_kind, events as e, permission as p};
use serde_json::{Value, json};
pub(super) fn registry() -> Value {
    let capabilities = HostOperation::ALL
        .iter()
        .filter_map(|op| op.capability())
        .chain([
            crate::view::OPEN_URL_CAPABILITY,
            crate::view::CLIPBOARD_WRITE_CAPABILITY,
        ])
        .collect::<std::collections::BTreeSet<_>>()
        .into_iter()
        .map(|id| {
            let kind = match capability_kind(id).expect("registered capability") {
                GrantKind::Boolean => "boolean",
                GrantKind::Patterns => "patterns",
                GrantKind::Urls => "urls",
                GrantKind::Requests => "requests",
                GrantKind::Listeners => "listeners",
                GrantKind::OpenUrls => "openUrls",
            };
            json!({"id":id,"kind":kind})
        })
        .collect::<Vec<_>>();
    json!({
        "capabilities":capabilities,"schemaVersion":p::DECISION_SCHEMA_VERSION,
        "artifactPrefix":p::ARTIFACT_PREFIX,"artifactHex":p::ARTIFACT_DIGEST_HEX,
        "states":p::PermissionState::ALL.iter().map(|state|(state.name(),state.code())).collect::<std::collections::BTreeMap<_,_>>(),
        "decisions":[p::StoredDecision::Allow,p::StoredDecision::Deny],
        "descriptor":{"version":p::DESCRIPTOR_VERSION,"separator":p::DESCRIPTOR_SEPARATOR.to_string(),"fields":p::DESCRIPTOR_FIELDS,
            "none":p::DESCRIPTOR_NO_RESOURCE,"resource":p::DESCRIPTOR_RESOURCE,"invalid":p::INVALID_DESCRIPTOR,"maxBytes":p::MAX_DESCRIPTOR_BYTES},
        "limits":{"applications":p::MAX_DECISION_APPLICATIONS,"decisions":p::MAX_DECISIONS_PER_APPLICATION,
            "audit":p::MAX_AUDIT_ENTRIES,"auditResourceBytes":p::MAX_AUDIT_RESOURCE_BYTES,"prompts":p::MAX_OPEN_PROMPTS,"dismissals":p::MAX_DISMISSALS},
        "requiredDenied":p::PermissionError::RequiredDenied{id:"{id}".into(),source_label:"stored decision".into()}.to_string(),
        "requiredRefused":p::PermissionError::RequiredDenied{id:"{id}".into(),source_label:"refused at first run".into()}.to_string(),
        "events":{"sources":e::MAX_EVENT_SOURCES,"maxId":e::MAX_EVENT_SOURCE_ID,"tasks":e::MAX_PENDING_HOST_TASKS,"recordBytes":e::MAX_EVENT_RECORD_BYTES,"nameBytes":e::MAX_EVENT_NAME_BYTES,
            "noListener":e::EventError::NoListener("{name}".into()).to_string(),
            "invalid":e::INVALID_SOURCE,"event":e::TakeKind::Event as i32,"closed":e::TakeKind::Closed as i32,
            "stateError":e::EventError::State.to_string(),"queueError":e::EventError::QueueFull.to_string(),"recordError":e::EventError::RecordTooLarge.to_string()}
    })
}
