//! Deterministic contract generation for the official browser host.
//! This is registry data, not effect implementations or a grant of authority.
use crate::{HostOperation, HostScalar, abi, fuel, function, http, language, value};
use language::LanguageProfile;
use serde_json::{Value, json};
mod effects;
mod kv;
mod layout;
mod network;
mod permissions;
mod view;
pub use kv::storage_unavailable as kv_storage_unavailable;

/// CSP origin ceiling derived only from validated package requests. Runtime
/// URL/method checks still enforce the narrower package and host scopes.
pub fn request_origins(manifest: &crate::Manifest) -> crate::Result<Vec<String>> {
    manifest.validate_package()?;
    let mut origins = std::collections::BTreeSet::new();
    for grant in manifest.grants()? {
        if grant.id == crate::net::REQUEST_CAPABILITY
            && let crate::CapabilityValue::Requests(scope) = grant.value
        {
            for pattern in scope.urls {
                let url = url::Url::parse(&pattern)
                    .map_err(|error| crate::VartaError::InvalidManifest(error.to_string()))?;
                origins.insert(url.origin().ascii_serialization());
            }
        }
    }
    Ok(origins.into_iter().collect())
}

pub const SCHEMA_VERSION: u32 = 1;
/// Private execution-image names, rejected on portable guest input.
pub const METER_MODULE: &str = "varta_browser_meter";
pub const METER_COUNTER: &str = "fuel";
pub const METER_SECTION: &str = "varta.browser.meter";
pub const METER_VERSION: u32 = 1;

/// Whether an operation's wire encoding is importable under this profile.
/// Whole-module validation must additionally check metadata, effects, helper
/// purity, signatures and capabilities; this predicate grants no authority.
#[must_use]
pub fn admits(profile: LanguageProfile, operation: HostOperation) -> bool {
    if profile.links(operation)
        || operation.minimum_abi() > profile.host_abi()
        || profile.host_operation(operation) != operation
    {
        return false;
    }
    let json = language::SUPPORTED_PROFILES
        .iter()
        .any(|p| operation == p.json_parse() || operation == p.json_stringify());
    if json {
        return operation == profile.json_parse() || operation == profile.json_stringify();
    }
    operation != HostOperation::HttpService || http::version_for(profile).is_some()
}

fn scalar(ty: &HostScalar) -> &'static str {
    match ty {
        HostScalar::I32 => "i32",
        HostScalar::I64 => "i64",
        HostScalar::F64 => "f64",
    }
}
fn profile(profile: LanguageProfile) -> Value {
    let helpers = if profile.array_mutations() {
        abi::MAX_INTERNAL_FUNCTIONS
    } else if profile.sparse_arrays() {
        abi::SPARSE_INTERNAL_FUNCTIONS
    } else {
        abi::LEGACY_INTERNAL_FUNCTIONS
    };
    let catalogs = if !profile.property_deletion() {
        vec![]
    } else {
        let mut catalogs = vec![json!({"version":function::CATALOG_VERSION,"frames":false})];
        if let Some(version) = function::admission::frame_catalog_version(profile) {
            catalogs.push(json!({"version":version,"frames":true}));
        }
        catalogs
    };
    let exceptions = if profile.errors() {
        let (global, version) = profile.exception_channel();
        json!({"global":global,"value":version})
    } else {
        Value::Null
    };
    json!({
        "name":profile,"hostAbi":profile.host_abi(),"valueAbi":profile.value_abi(),
        "numbers":if profile.binary64(){"binary64"}else{"i32"},
        "strings":if profile.utf16(){"UTF-16LE"}else{"UTF-8"},
        "toolEntries":!profile.binary64(),"httpVersion":http::version_for(profile),
        "exceptionChannel":profile.exceptions(),"exceptionVersion":exceptions,
        "frames":function::admission::frame_catalog_version(profile).map(|catalog|json!({"catalog":catalog})),
        "scheduler":profile.promises(),"fairFuel":profile.fair_fuel(),
        "schedulerLimits":{"held":profile.limits().data.pending_continuations,
            "timers":profile.limits().data.pending_timers,
            "timerError":if profile.limits().data.pending_timers==crate::scheduler::MAX_PENDING_TIMERS {crate::scheduler::SchedulerError::TimerBudget.to_string()}else{crate::scheduler::SchedulerError::TimerLimit{limit:profile.limits().data.pending_timers}.to_string()},
            "queueError":if profile.limits().data.pending_continuations==crate::scheduler::MAX_PENDING_CONTINUATIONS {crate::scheduler::SchedulerError::QueueFull.to_string()}else{crate::scheduler::SchedulerError::ContinuationLimit{limit:profile.limits().data.pending_continuations}.to_string()}},
        "supportedKinds":(0..=value::ValueKind::LAST as u32).filter_map(value::ValueKind::from_tag).filter(|kind|kind.supported_by(profile.value_abi())).map(|k|k as u32).collect::<Vec<_>>(),
        "admittedImports":HostOperation::ALL.iter().filter(|&&op|admits(profile,op)).count(),
        "acceptance":{
            "values":(value::LEGACY_VALUE_VERSION..=value::HASHED_DICTIONARY_VALUE_VERSION)
                .filter(|&version|value::compatible_version(version,profile.value_abi())).collect::<Vec<_>>(),
            "catalogs":catalogs,"helperAllowance":helpers,
            "maxDefinedFunctions":if profile.program_limits(){profile.limits().wasm_functions}else{abi::MAX_USER_FUNCTIONS+helpers+abi::MAX_HTTP_HANDLERS},
            "functionLimitCountsImports":profile.program_limits()
        }
    })
}

/// Registry-only schema. Arrays follow profile order or sorted wire names, and
/// object keys are deterministically ordered by serde_json's default map.
#[must_use]
pub fn registry() -> Value {
    let mut operations = HostOperation::ALL.to_vec();
    operations.sort_by_key(|op| op.name());
    json!({
        "schemaVersion":SCHEMA_VERSION,
        "maxTurnFuel":crate::budget::MAX_FUEL_PER_TURN,
        "scheduler":layout::scheduler(),"values":layout::values(),"network":network::registry(),"view":view::registry(),
        "permissions":permissions::registry(),"effects":effects::registry(),"kv":kv::registry(),
        "environment":{
            "maxNameBytes":crate::MAX_ENV_NAME_BYTES,
            "maxInputBytes":abi::MAX_INPUT_BYTES,
            "maxResultBytes":abi::MAX_RESULT_BYTES,
            "maxPatterns":crate::MAX_GRANT_PATTERNS,
            "maxPatternBytes":crate::MAX_PATTERN_BYTES,
            "maxMatchWork":crate::MAX_PATTERN_MATCH_WORK,
            "resultLength":abi::RESULT_LEN,
            "resultStatus":abi::RESULT_STATUS,
            "statuses":{"ok":abi::HostStatus::Ok as i32,"denied":abi::HostStatus::Denied as i32,"invalid":abi::HostStatus::InvalidInput as i32,"io":abi::HostStatus::Io as i32,"tooLarge":abi::HostStatus::TooLarge as i32}
        },
        "booleanCapabilities":HostOperation::ALL.iter().filter_map(|op|op.capability()).filter(|id|crate::capability_kind(id)==Some(crate::GrantKind::Boolean)).chain([crate::view::CLIPBOARD_WRITE_CAPABILITY]).collect::<std::collections::BTreeSet<_>>(),
        "meter":{"module":METER_MODULE,"counter":METER_COUNTER,"section":METER_SECTION,"version":METER_VERSION},
        "profiles":language::SUPPORTED_PROFILES.iter().copied().map(profile).collect::<Vec<_>>(),
        "operations":operations.into_iter().map(|op|{
            let (params,results)=op.scalar_signature();
            json!({"name":op.name(),"params":params.iter().map(scalar).collect::<Vec<_>>(),
                "results":results.iter().map(scalar).collect::<Vec<_>>(),
                "minHostAbi":op.minimum_abi(),"capability":op.capability(),
                "scheduler":op.scheduler(),"resultGlobals":op.needs_result_globals(),
                "networkBookkeeping":op.net() && !matches!(op,HostOperation::NetRequestStart|HostOperation::NetRequestStartV2),
                "timerBookkeeping":matches!(op,HostOperation::TimerClear|HostOperation::TimerTake|HostOperation::TimerUncaught),
                "instanceBookkeeping":op.instance_events(),
                "kvBookkeeping":kv::bookkeeping(op),
                "stringResult":op.returns_string(),
                "admittedUnder":language::SUPPORTED_PROFILES.iter().copied()
                    .filter(|&p|admits(p,op)).collect::<Vec<_>>()})
        }).collect::<Vec<_>>(),
        "fuel":{
            "call":fuel::CALL,"byte":fuel::BYTE,
            "jsonParseUnit":fuel::JSON_PARSE_UNIT,"jsonParseValue":fuel::JSON_PARSE_VALUE,
            "jsonSchemaType":fuel::JSON_SCHEMA_TYPE,"jsonSchemaField":fuel::JSON_SCHEMA_FIELD,
            "jsonStringifyValue":fuel::JSON_STRINGIFY_VALUE,"jsonStringifyUnit":fuel::JSON_STRINGIFY_UNIT,
            "kvOperation":fuel::KV_OPERATION,"kvByte":fuel::KV_BYTE,"kvEntry":fuel::KV_ENTRY
        }
    })
}

/// Generate a dependency-free ES module. The object and its nested values are
/// frozen to avoid accidental policy-table mutation by trusted host code.
pub fn javascript() -> Result<String, serde_json::Error> {
    let data = serde_json::to_string_pretty(&registry())?;
    Ok(format!(
        "// Generated from varta-common registries; do not edit.\n\
         // Regenerate: cargo run -p varta-common --example generate_browser_registry --locked\n\
         function freeze(value) {{\n  if (value !== null && typeof value === \"object\") {{\n    for (const child of Object.values(value)) freeze(child);\n    Object.freeze(value);\n  }}\n  return value;\n}}\n\
         export const registry = freeze({data});\n"
    ))
}

#[cfg(test)]
mod tests;
