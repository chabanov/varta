//! Wire facts of the browser host's pilot effects (clock, entropy, environment
//! snapshot, exit) and of idle collection between turns. The diagnostic texts
//! repeat the native runtime's private ones, and tests compare each with what
//! the native runtime produces: `varta-project`'s browser tests compare the
//! maintenance templates with the runtime's typed `BudgetExhausted` error and
//! `failures.invalid` with the log of a native `process.exit(1.5)`; the CLI
//! smoke suite compares `failures.tooLarge` with the log of a native
//! `process.env` snapshot above the result limit; `varta-runtime`'s
//! maintenance tests compare `collector.invalidStatus` with the native error
//! of an idle export rewritten to return an invalid status (compiled modules
//! cannot); and the Chrome effect acceptance compares the clock and exit
//! denial texts, `failures.invalid` and `failures.tooLarge` with native logs
//! of the same programs.
use crate::{HostOperation as H, budget, collector};
use serde_json::{Value, json};

/// Placeholders of the maintenance-exhaustion templates.
const TURN_PLACEHOLDER: &str = "{turn}";
const LIMIT_PLACEHOLDER: &str = "{limit}";

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
        }
    })
}
