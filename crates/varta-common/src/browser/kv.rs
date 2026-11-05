//! Wire facts of v38 KV storage (`crate::kv`) for the official browser host.
//!
//! The browser host stores KV databases in one Varta KV container per
//! artifact in the origin private file system, through a
//! `FileSystemSyncAccessHandle` its dedicated Worker acquires before
//! instantiation, and runs the shared value layer (`varta-kv-codec`) compiled
//! to wasm32 (docs/embedding.md, KV storage (v38)). A Worker that cannot
//! hold sync access handles at all refuses a program whose granted KV
//! storage it could never honour, by name, before instantiation
//! ([`storage_unavailable`]). The denial and failure texts repeat the native
//! runtime's; `varta-project`'s browser tests compare them with the native
//! texts, and the Chrome KV acceptance compares whole runs with native.
use crate::{HostOperation as H, kv};
use serde_json::{Value, json};

/// The placeholders of [`registry`]'s denial text.
const PATH_PLACEHOLDER: &str = "{path}";
const CAPABILITY_PLACEHOLDER: &str = "{capability}";
/// The Worker's refusal of a granted KV program when the host has no OPFS
/// sync access handles; [`storage_unavailable`] appends the grant.
const STORAGE_UNAVAILABLE: &str = "browser KV storage needs OPFS sync access handles in a dedicated Worker, which this host does not provide; the manifest grants ";
/// The failure an open reports in a Worker that acquired no storage for the
/// artifact although a grant covers the path.
const NO_STORAGE: &str = "no KV storage was acquired for this application";
/// The errors a `FileSystemSyncAccessHandle` is refused with while another
/// handle holds the file (another tab or Worker of the same artifact): the
/// standard's, and WebKit's.
const LOCKED_ERRORS: [&str; 2] = ["NoModificationAllowedError", "InvalidStateError"];
/// The error a storage write is refused with past the origin's quota.
const QUOTA_ERROR: &str = "QuotaExceededError";
/// Host work stopped at the turn deadline (the loader's watchdog text).
const DEADLINE_ERROR: &str = "browser execution deadline exceeded";

/// The Worker's refusal of a program granted `grant` (`kv.read`) on a host
/// without OPFS sync access handles.
#[must_use]
pub fn storage_unavailable(grant: &str) -> String {
    format!("{STORAGE_UNAVAILABLE}{grant}")
}

/// The KV operations that are bookkeeping: charged the call fuel only, never
/// to the host-call budget (the native runtime's entry, text and close
/// handlers check the deadline and do not tick).
#[must_use]
pub fn bookkeeping(operation: H) -> bool {
    matches!(operation, H::KvEntry | H::KvText | H::KvClose)
}

fn codec() -> Value {
    use kv::codec as C;
    json!({
        "importModule":C::IMPORT_MODULE,
        "imports":{"memorySize":C::IMPORT_MEMORY_SIZE,"memoryRead":C::IMPORT_MEMORY_READ,
            "memoryWrite":C::IMPORT_MEMORY_WRITE,"memoryZero":C::IMPORT_MEMORY_ZERO,
            "allocate":C::IMPORT_ALLOCATE,"retain":C::IMPORT_RETAIN,"deadline":C::IMPORT_DEADLINE,
            "charge":C::IMPORT_CHARGE,"channelReady":C::IMPORT_CHANNEL_READY,
            "channelThrow":C::IMPORT_CHANNEL_THROW,"resultLength":C::IMPORT_RESULT_LENGTH},
        "exports":{"input":C::EXPORT_INPUT,"output":C::EXPORT_OUTPUT,"outputLength":C::EXPORT_OUTPUT_LENGTH,
            "configure":C::EXPORT_CONFIGURE,"stage":C::EXPORT_STAGE,"parseRead":C::EXPORT_PARSE_READ,
            "readEntries":C::EXPORT_READ_ENTRIES,"parseCommit":C::EXPORT_PARSE_COMMIT,
            "decode":C::EXPORT_DECODE,"memory":C::EXPORT_MEMORY},
        "statuses":{"ok":C::OK,"failed":C::FAILED,"fatal":C::FATAL,"deadline":C::DEADLINE},
        "entries":{"none":C::ENTRY_NONE,"key":C::ENTRY_KEY,"value":C::ENTRY_VALUE},
        "absent":C::ABSENT,"present":C::PRESENT,
        "sections":{"static":crate::abi::STATIC_SECTION,"intrinsics":crate::abi::INTRINSIC_SECTION},
    })
}

/// The failure texts of an open, read or commit, with [`PATH_PLACEHOLDER`]
/// for the database path. The native texts (`varta_kv_codec::protocol::
/// KvError`) are repeated here; `varta-project`'s browser tests compare
/// them. The browser-only texts name the OPFS container's own failures.
fn texts() -> Value {
    let path = PATH_PLACEHOLDER;
    json!({
        // Native texts.
        "notFound":format!("no KV database at {path}"),
        "locked":format!("the KV database {path} is locked by another writer"),
        "readOnly":format!("the KV database {path} is open read-only"),
        "tooLate":format!("too little time is left in this turn to commit to the KV database {path}; nothing was applied"),
        "invalidName":format!("{path} is not a valid database file name"),
        "noFile":format!("{path} names no database file"),
        "unavailable":format!("cannot open the KV database {path}: {FAILURE_PLACEHOLDER}"),
        "tooManyDatabases":format!("at most {} KV databases are open at once",kv::MAX_OPEN_DATABASES),
        "nonU64":format!("Failed to perform '{OPERATION_PLACEHOLDER}' mutation on a non-U64 value in the database"),
        // Browser texts: a container that is not format version 1, a full
        // origin quota, and a write whose outcome the host cannot know.
        "corrupt":format!("{path} is not in a Varta KV container of format version {CONTAINER_FORMAT}: {FAILURE_PLACEHOLDER}"),
        "full":format!("the KV storage of {path} is full ({FAILURE_PLACEHOLDER}); nothing was applied"),
        "uncertain":format!("the KV storage of {path} is unavailable after an unfinished write; reload the application"),
    })
}
/// The placeholders of [`texts`] beside the path.
const FAILURE_PLACEHOLDER: &str = "{failure}";
const OPERATION_PLACEHOLDER: &str = "{operation}";
/// The format version of the browser host's KV container
/// (`kv-store.mjs`).
pub const CONTAINER_FORMAT: u32 = 1;

pub(super) fn registry() -> Value {
    use kv::{EntryField as F, MutationCode as M};
    json!({
        "capabilities":{"read":kv::READ_CAPABILITY,"write":kv::WRITE_CAPABILITY},
        "operations":{"open":H::KvOpen.name(),"read":H::KvRead.name(),"commit":H::KvCommit.name(),
            "stage":H::KvStage.name(),"entry":H::KvEntry.name(),"key":H::KvKey.name(),
            "value":H::KvValue.name(),"text":H::KvText.name(),"close":H::KvClose.name()},
        "failed":kv::FAILED,"checkFailed":kv::CHECK_FAILED,"committed":kv::COMMITTED,
        "separator":kv::DESCRIPTOR_SEPARATOR.to_string(),
        "maxDescriptorBytes":kv::MAX_DESCRIPTOR_BYTES,
        "maxOpenDatabases":kv::MAX_OPEN_DATABASES,
        "maxStaged":kv::MAX_STAGED,"maxStagedBytes":kv::MAX_STAGED_BYTES,
        "stagedTooLarge":kv::STAGED_TOO_LARGE,
        "commitReserveMs":kv::COMMIT_RESERVE_MS,
        "reservedNamePrefix":kv::RESERVED_NAME_PREFIX.to_string(),
        "sidecarSuffixes":kv::SIDECAR_SUFFIXES,
        "notAllowed":kv::error::NOT_ALLOWED,
        "errors":{"notAllowed":kv::error::NOT_ALLOWED,"notFound":kv::error::NOT_FOUND,
            "typeError":kv::error::TYPE_ERROR,"rangeError":kv::error::RANGE_ERROR},
        "encodings":{"v8":kv::ENCODING_V8,"u64":kv::ENCODING_U64,"bytes":kv::ENCODING_BYTES},
        "mutations":{"set":M::Set as u8,"remove":M::Delete as u8,"sum":M::Sum as u8,"min":M::Min as u8,"max":M::Max as u8},
        // The operation names `denokv_sqlite` reports, by mutation code.
        "mutationNames":{(M::Sum as u8).to_string():"sum",(M::Min as u8).to_string():"min",(M::Max as u8).to_string():"max"},
        "entryFields":{"versionstamp":F::Versionstamp as u8,"encoding":F::Encoding as u8,"u64":F::U64 as u8,
            "bytes":F::Bytes as u8,"cursor":F::Cursor as u8,"range":F::Range as u8},
        "fuel":{"operation":crate::fuel::KV_OPERATION},
        "paths":{"separator":crate::files::SEPARATOR.to_string(),"maxPathBytes":crate::files::MAX_PATH_BYTES,
            "maxRootNameBytes":crate::files::MAX_ROOT_NAME_BYTES,"maxMatchWork":crate::MAX_PATTERN_MATCH_WORK,
            "maxPatterns":crate::MAX_GRANT_PATTERNS,"maxPatternBytes":crate::MAX_PATTERN_BYTES},
        "denialText":format!("<capability denied: {CAPABILITY_PLACEHOLDER}> ({PATH_PLACEHOLDER})"),
        "pathPlaceholder":PATH_PLACEHOLDER,"capabilityPlaceholder":CAPABILITY_PLACEHOLDER,
        "storageUnavailable":STORAGE_UNAVAILABLE,
        "noStorage":NO_STORAGE,"lockedErrors":LOCKED_ERRORS,"quotaError":QUOTA_ERROR,"deadlineError":DEADLINE_ERROR,
        "guest":{"allocate":crate::value::VALUE_ALLOCATE,"retain":crate::value::VALUE_RETAIN,
            "resultLength":crate::abi::RESULT_LEN,"pending":crate::exceptions::PENDING},
        "codec":codec(),
        "texts":texts(),"failurePlaceholder":FAILURE_PLACEHOLDER,"operationPlaceholder":OPERATION_PLACEHOLDER,
        "containerFormat":CONTAINER_FORMAT,
    })
}
