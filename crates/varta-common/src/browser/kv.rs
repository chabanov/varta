//! Wire facts of v38 KV storage (`crate::kv`) for the official browser host.
//!
//! The browser host offers no KV storage yet (docs/embedding.md, KV storage
//! (v38)): it links the nine `kv_*` imports and denies every `kv_open` the
//! way a host without a grant does. The denial text repeats the native
//! runtime's private one; `varta-project`'s browser tests compare it with the
//! log of a native open that no grant covers, and the Chrome KV acceptance
//! compares whole runs with native.
use crate::{HostOperation as H, kv};
use serde_json::{Value, json};

/// The placeholder of the denied path in [`registry`]'s denial text.
const PATH_PLACEHOLDER: &str = "{path}";

/// The KV operations that are bookkeeping: charged the call fuel only, never
/// to the host-call budget (the native runtime's entry, text and close
/// handlers check the deadline and do not tick).
#[must_use]
pub fn bookkeeping(operation: H) -> bool {
    matches!(operation, H::KvEntry | H::KvText | H::KvClose)
}

pub(super) fn registry() -> Value {
    json!({
        "capabilities":{"read":kv::READ_CAPABILITY,"write":kv::WRITE_CAPABILITY},
        "operations":{"open":H::KvOpen.name(),"read":H::KvRead.name(),"commit":H::KvCommit.name(),
            "stage":H::KvStage.name(),"entry":H::KvEntry.name(),"key":H::KvKey.name(),
            "value":H::KvValue.name(),"text":H::KvText.name(),"close":H::KvClose.name()},
        "failed":kv::FAILED,
        "separator":kv::DESCRIPTOR_SEPARATOR.to_string(),
        "maxDescriptorBytes":kv::MAX_DESCRIPTOR_BYTES,
        "notAllowed":kv::error::NOT_ALLOWED,
        "denialText":format!("<capability denied: {}> ({PATH_PLACEHOLDER})",kv::READ_CAPABILITY),
        "pathPlaceholder":PATH_PLACEHOLDER,
    })
}
