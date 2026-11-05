//! Wire facts of the browser host's pilot effects (clock, entropy, environment
//! snapshot, exit) and of idle collection between turns. The diagnostic texts
//! repeat the native runtime's private ones, and tests compare each with what
//! the native runtime produces: `varta-project`'s browser tests compare the
//! maintenance templates with the runtime's typed `BudgetExhausted` error and
//! `failures.invalid` with the log of a native `process.exit(1.5)`, and the
//! `fatal` turn and resource templates with the runtime's `BudgetExhausted`
//! and `ResourceExhausted` errors; the CLI
//! smoke suite compares `failures.tooLarge` with the log of a native
//! `process.env` snapshot above the result limit; `varta-runtime`'s
//! maintenance tests compare `collector.invalidStatus` with the native error
//! of an idle export rewritten to return an invalid status (compiled modules
//! cannot); and the Chrome effect acceptance compares the clock and exit
//! denial texts, `failures.invalid` and `failures.tooLarge` with native logs
//! of the same programs, and the fatal texts of guest fuel and memory
//! exhaustion with the native failures.
use crate::{HostOperation as H, budget, collector};
use serde_json::{Value, json};

/// Placeholders of the exhaustion and resource-failure templates.
const TURN_PLACEHOLDER: &str = "{turn}";
const KIND_PLACEHOLDER: &str = "{kind}";
const LIMIT_PLACEHOLDER: &str = "{limit}";
const REQUEST_PLACEHOLDER: &str = "{requested}";

/// The native text of an exhausted maintenance budget, with placeholders for
/// the turn the idle period follows and the budget's limit.
fn maintenance_exhausted(kind: budget::BudgetKind) -> String {
    let unit = match kind {
        budget::BudgetKind::MaintenanceDeadline => " ms per idle period",
        _ => " instructions per idle period",
    };
    format!(
        "turn {TURN_PLACEHOLDER} (maintenance) exhausted its {} budget ({LIMIT_PLACEHOLDER}{unit}); the instance was terminated",
        kind.label()
    )
}

/// The native text of a turn that exhausted its fuel, with placeholders for
/// the turn, the kind of task it ran and the per-turn limit.
fn turn_fuel_exhausted() -> String {
    format!(
        "turn {TURN_PLACEHOLDER} ({KIND_PLACEHOLDER}) exhausted its {} budget ({LIMIT_PLACEHOLDER} instructions per turn); the instance was terminated",
        budget::BudgetKind::Fuel.label()
    )
}

/// The native text of each v38 resource failure by its guest code, with a
/// placeholder for the requested amount.
fn resource_texts() -> Value {
    use crate::limits::{Resource, ResourceExhausted};
    let texts = [
        Resource::Memory,
        Resource::ValueSize,
        Resource::RootStack,
        Resource::LiveFrames,
    ]
    .into_iter()
    .map(|resource| {
        // Rendered with a sentinel request that no description contains, so
        // the template is exactly the native `Display` text.
        let sentinel = u32::MAX;
        let text = ResourceExhausted {
            resource,
            requested: sentinel,
        }
        .to_string()
        .replace(&sentinel.to_string(), REQUEST_PLACEHOLDER);
        ((resource as i32).to_string(), Value::String(text))
    })
    .collect::<serde_json::Map<_, _>>();
    Value::Object(texts)
}

fn capability(operation: H) -> &'static str {
    operation.capability().unwrap_or_default()
}

pub(super) fn registry() -> Value {
    json!({
        "clock":{"operation":H::DateNow.name(),"capability":capability(H::DateNow),
            "denialText":format!("<capability denied: {}> (Date.now)",capability(H::DateNow))},
        // One 64-bit entropy word per sample; its top `sampleBits` bits scaled
        // by 2^-sampleBits, exactly as the native sampler converts it.
        "random":{"operation":H::MathRandom.name(),"wordBits":u64::BITS,"sampleBits":f64::MANTISSA_DIGITS},
        "environment":{"operation":H::EnvSnapshot.name(),"capability":capability(H::EnvSnapshot)},
        // The native host-failure texts of an invalid input and of a result
        // above `abi::MAX_RESULT_BYTES`.
        "failures":{"invalid":"<invalid host input>","tooLarge":"<result too large>"},
        "exit":{"operation":H::ExitFloat.name(),"capability":capability(H::ExitFloat),
            "denialText":format!("<capability denied: {}> (process)",capability(H::ExitFloat)),
            "minimum":i32::MIN,"maximum":i32::MAX},
        "collector":{
            "idleExport":collector::IDLE_EXPORT,"stepUnits":collector::IDLE_STEP_UNITS,
            "none":collector::IDLE_NONE,"more":collector::IDLE_MORE,"done":collector::IDLE_DONE,
            "stepFuel":budget::MAINTENANCE_STEP_FUEL,"reportedPeriods":budget::MAX_REPORTED_TURNS,
            "fuelBudget":budget::BudgetKind::MaintenanceFuel.wire_name(),
            "deadlineBudget":budget::BudgetKind::MaintenanceDeadline.wire_name(),
            "invalidStatus":"invalid idle collection status",
            "fuelExhausted":maintenance_exhausted(budget::BudgetKind::MaintenanceFuel),
            "deadlineExhausted":maintenance_exhausted(budget::BudgetKind::MaintenanceDeadline),
            "turnPlaceholder":TURN_PLACEHOLDER,"limitPlaceholder":LIMIT_PLACEHOLDER
        },
        // Fatal failures the Worker reports with the native runtime's texts:
        // a turn whose fuel ran out (guest metering or a host charge) and a
        // v38 resource failure a guest reported before its `unreachable`.
        "fatal":{
            "fuelExhausted":turn_fuel_exhausted(),
            "turnPlaceholder":TURN_PLACEHOLDER,"kindPlaceholder":KIND_PLACEHOLDER,
            "limitPlaceholder":LIMIT_PLACEHOLDER,
            "kinds":{"main":budget::turn_kind::MAIN,"timer":budget::turn_kind::TIMER,
                "network":budget::turn_kind::NETWORK,"event":budget::turn_kind::HOST_EVENT,
                "view":budget::turn_kind::VIEW},
            "resourceExport":crate::limits::RESOURCE_FAILURE,
            "requestExport":crate::limits::RESOURCE_REQUEST,
            "resources":resource_texts(),
            "requestPlaceholder":REQUEST_PLACEHOLDER
        }
    })
}
