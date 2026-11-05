//! Wire facts of v38 KV storage (`crate::kv`) for the official browser host.
//!
//! The browser host offers no KV storage yet (docs/embedding.md, KV storage
//! (v38)): it links the nine `kv_*` imports and denies every `kv_open` the
//! way a host without a grant does, and a browser build refuses a manifest
//! that grants `kv.read` or `kv.write` ([`storage_refusal`]). The denial text repeats the native
//! runtime's private one; `varta-project`'s browser tests compare it with the
//! log of a native open that no grant covers, and the Chrome KV acceptance
//! compares whole runs with native.
use crate::{HostOperation as H, kv};
use serde_json::{Value, json};

/// The placeholder of the denied path in [`registry`]'s denial text.
const PATH_PLACEHOLDER: &str = "{path}";
/// The start of the browser build refusal of a manifest that grants KV
/// storage this host cannot honour; [`storage_refusal`] appends the grants.
const STORAGE_REFUSAL: &str = "browser KV storage is not implemented; the manifest grants ";
/// The separator of the refused grant ids in [`storage_refusal`].
const REFUSAL_SEPARATOR: &str = ", ";

/// The browser build refusal of a manifest granting the KV capabilities
/// `grants` (in manifest order): the host offers no KV storage, so such a
/// grant could never be honoured and is refused before any bundle exists.
#[must_use]
pub fn storage_refusal<'a>(grants: impl IntoIterator<Item = &'a str>) -> String {
    let grants = grants.into_iter().collect::<Vec<_>>();
    format!("{STORAGE_REFUSAL}{}", grants.join(REFUSAL_SEPARATOR))
}

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
        "storageRefusal":STORAGE_REFUSAL,
        "refusalSeparator":REFUSAL_SEPARATOR,
    })
}
