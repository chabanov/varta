//! Data-only layouts for bounded scheduler diagnostics. Numeric layout facts
//! come from the same owners as the native/linked readers.
use crate::{abi, bigint, exceptions, function::frame, scheduler as s, value as v};
use serde_json::{Value, json};

pub(super) fn scheduler() -> Value {
    use s::SchedulerError as E;
    json!({
        "maxJobs":s::MAX_EXECUTION_JOBS,"maxSerials":s::MAX_PROMISE_SERIALS,
        "retainedReasons":s::MAX_RETAINED_REJECTION_REASONS,
        "reasonUnits":s::MAX_REJECTION_REASON_UNITS,"reasonBytes":s::MAX_REJECTION_REASON_BYTES,
        "truncated":s::REJECTION_TRUNCATION_MARKER,"notRetained":s::REJECTION_NOT_RETAINED,
        "resumeExport":frame::RESUME_EXPORT,"normal":frame::Completion::Normal as i32,
        "outcomes":[frame::Outcome::Suspended as i32,frame::Outcome::Returned as i32],
        "errors":{"jobs":E::JobBudget.to_string(),"serials":E::SerialBudget.to_string(),
            "duplicate":E::DuplicateToken.to_string(),"unknown":E::UnknownSerial.to_string(),
            "outcome":E::Outcome.to_string()},
        "timer":{"capability":s::TIMER_CAPABILITY,"interval":s::TIMER_INTERVAL,"unref":s::TIMER_UNREF,
            "minimumDelay":s::TIMER_MIN_DELAY_MS,"maximumDelay":s::TIMER_MAX_DELAY_MS,
            "stateError":E::TimerState.to_string(),"deniedError":E::TimerDenied.to_string(),
            "denialText":format!("<capability denied: {}> (timer)",s::TIMER_CAPABILITY),
            "deadlineTemplate":E::TimerDeadline{due_ms:123456789,deadline_seconds:987654321}.to_string().replace("123456789","{due}").replace("987654321","{seconds}")},
    })
}

pub(super) fn values() -> Value {
    let tags: std::collections::BTreeMap<_, _> = (0..=v::ValueKind::Promise as u32)
        .filter_map(v::ValueKind::from_tag)
        .map(|kind| (format!("{kind:?}"), kind as u32))
        .collect();
    json!({
        "tags":tags,"references":(0..=v::ValueKind::Promise as u32).filter_map(v::ValueKind::from_tag).filter(|k|k.is_reference()).map(|k|k as u32).collect::<Vec<_>>(),
        "ownedBase":abi::OWNED_STRING_BASE,"headerBytes":abi::STRING_HEADER_BYTES,
        "alignment":v::HEAP_ALIGNMENT,"flagsOffset":v::HEAP_FLAGS_OFFSET,"linkOffset":v::HEAP_LINK_OFFSET,
        "allocated":v::HEAP_ALLOCATED_FLAG,"unionFlag":v::HEAP_UNION_FLAG,"cellBytes":v::UNION_BYTES,"cellPayload":v::UNION_META_BYTES,
        "errorFlag":v::HEAP_ERROR_FLAG,"errorBytes":v::ERROR_BYTES,"errorShift":v::ERROR_CLASS_SHIFT,"errorOwn":v::ERROR_OWN_MESSAGE,
        "errorClasses":v::ErrorClass::ALL.iter().map(|c|c.name()).collect::<Vec<_>>(),
        "recordFlag":v::HEAP_RECORD_FLAG,"recordMeta":v::RECORD_META_BYTES,"slotBytes":v::RECORD_SLOT_BYTES,
        "descriptorHeader":v::RECORD_DESCRIPTOR_HEADER,"descriptorEntry":v::RECORD_DESCRIPTOR_ENTRY,
        "optionalFlag":v::OPTIONAL_FIELD_FLAG,"privatePrefix":v::PRIVATE_KEY_PREFIX.to_string(),"maxKeyBytes":v::MAX_RECORD_KEY_BYTES,
        "legacyStringBytes":abi::MAX_STRING_BYTES,"diagnosticUnits":exceptions::MAX_DIAGNOSTIC_UNITS,
        "exceptionValue":exceptions::VALUE,"exceptionRoot":exceptions::ROOT,"exceptionRootBytes":exceptions::RESERVED_ROOT_BYTES,
        "staticSection":abi::STATIC_SECTION,"firstPageEnd":abi::RESULT_BASE,"pageBytes":abi::WASM_PAGE_BYTES,
        "recordShapes":(0..=v::MAX_WIDE_RECORD_FIELDS).map(|count|json!({
            "bytes":v::record_bytes(v::HASHED_DICTIONARY_VALUE_VERSION,count),
            "descriptorBytes":v::descriptor_bytes(count),"presence":v::presence_offset(count),
            "order":v::order_offset(count),"words":v::presence_words(count),"wide":v::is_wide_record(count)
        })).collect::<Vec<_>>(),
        "bigint":{"flag":bigint::HEAP_FLAG,"meta":bigint::META_BYTES,"digitBytes":bigint::DIGIT_BYTES,"maxBytes":bigint::MAX_PAYLOAD_BYTES,
            "zero":bigint::Sign::Zero as u32,"positive":bigint::Sign::Positive as u32,"negative":bigint::Sign::Negative as u32}
    })
}
