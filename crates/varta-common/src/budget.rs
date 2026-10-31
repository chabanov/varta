//! Turn budgets of profile v38 (`turn_budgets`, increment P5).
//!
//! A v38 instance runs as a sequence of **event turns**: the first turn
//! instantiates the module, calls `_start` and drains the microtask queue;
//! every later turn takes exactly one task (a fired timer, a network
//! completion, a host event or a permission answer) and drains the microtasks
//! it queued. Fuel, host calls and microtasks are renewed for every turn; the
//! deadline is renewed for every turn of a long-lived application instance
//! (the bounded entry points keep one wall-clock bound for the whole
//! execution); linear memory and captured output are bounded per instance and
//! never renewed.
//!
//! A package manifest may *request* budgets; the host ceiling caps them. An
//! omitted request keeps the v38 production default ([`Budgets::TURN`]), and
//! without a host ceiling that default is also the cap, so a package can
//! lower its budgets but only the embedder can raise them. Earlier profiles
//! keep their production budgets ([`Budgets::PRODUCTION`]) and ignore both.
//!
//! Collection work between the turns of an application instance (v38
//! `incremental_collection`, [`crate::collector`]) runs under a separate,
//! per-idle-period **maintenance budget** ([`MAINTENANCE_FUEL`],
//! [`MAINTENANCE_DEADLINE_MS`]), requested and capped like the turn budgets
//! and never charged to a turn. Earlier profiles and bounded runs have none.
use crate::abi;
use serde::{Deserialize, Serialize};
use std::time::Duration;

/// Engine maxima no request or ceiling may exceed.
pub const MAX_FUEL_PER_TURN: u64 = 1_000_000_000;
/// The production fuel of one v38 turn (owner decision, from the measured
/// v38 console turns at the fair host-fuel rates: a turn that parses a 20 KB
/// response and updates a 50-row view needs about 5.6 million). Every earlier
/// profile keeps [`abi::FUEL`] per execution.
pub const TURN_FUEL: u64 = 8_000_000;
pub const MAX_DEADLINE_PER_TURN_MS: u64 = 60_000;
pub const MAX_MEMORY_BYTES: u64 = 1024 * 1024 * 1024;
pub const MAX_HOST_CALLS_PER_TURN: u32 = 1 << 20;
pub const MAX_OUTPUT_BYTES: u64 = 64 * 1024 * 1024;
/// The maintenance fuel of one idle period of a v38 application instance:
/// collection work the runtime runs between turns, when no task is runnable
/// (`collector::IDLE_EXPORT`): half the per-turn fuel, enough for several
/// complete cycles of the console's heap (wave 3 (f) measurement).
pub const MAINTENANCE_FUEL: u64 = TURN_FUEL / 2;
pub const MAX_MAINTENANCE_FUEL: u64 = MAX_FUEL_PER_TURN;
/// The wall-clock bound of one idle period's maintenance; a hard bound only
/// (the work done is set by fuel, so scripted sessions stay deterministic).
pub const MAINTENANCE_DEADLINE_MS: u64 = 1000;
pub const MAX_MAINTENANCE_DEADLINE_MS: u64 = MAX_DEADLINE_PER_TURN_MS;
/// The most fuel one `collector::IDLE_EXPORT` call of
/// `collector::IDLE_STEP_UNITS` units may use (measured bound with headroom,
/// `idle_step_fuel_stays_within_its_bound`): the runtime starts another step
/// only while this much maintenance fuel remains, so a step never exhausts
/// the budget of a well-formed module.
pub const MAINTENANCE_STEP_FUEL: u64 = 400_000;
/// Milliseconds per second, for the deadline budget's wire unit.
pub const MILLISECONDS_PER_SECOND: u64 = 1000;
/// Turns whose usage a budget report keeps individually; later turns are
/// counted only in the report's totals.
pub const MAX_REPORTED_TURNS: usize = 256;

/// The wire form of requested (manifest) or allowed (host ceiling) budgets.
/// Every member is optional; a present value must be in `1..=` its engine
/// maximum.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct BudgetRequest {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fuel_per_turn: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub deadline_per_turn_ms: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub memory_bytes: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub host_calls_per_turn: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output_bytes: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub maintenance_fuel: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub maintenance_deadline_ms: Option<u64>,
}

/// One budget, by its wire name.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum BudgetKind {
    Fuel,
    Deadline,
    Memory,
    HostCalls,
    Output,
    /// Fuel of one idle period's maintenance (v38 `incremental_collection`).
    MaintenanceFuel,
    /// Wall-clock bound of one idle period's maintenance.
    MaintenanceDeadline,
}
impl BudgetKind {
    pub const ALL: [Self; 7] = [
        Self::Fuel,
        Self::Deadline,
        Self::Memory,
        Self::HostCalls,
        Self::Output,
        Self::MaintenanceFuel,
        Self::MaintenanceDeadline,
    ];
    /// The manifest member name.
    #[must_use]
    pub fn wire_name(self) -> &'static str {
        match self {
            Self::Fuel => "fuelPerTurn",
            Self::Deadline => "deadlinePerTurnMs",
            Self::Memory => "memoryBytes",
            Self::HostCalls => "hostCallsPerTurn",
            Self::Output => "outputBytes",
            Self::MaintenanceFuel => "maintenanceFuel",
            Self::MaintenanceDeadline => "maintenanceDeadlineMs",
        }
    }
    /// The human name used in diagnostics and budget reports.
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::Fuel => "fuel",
            Self::Deadline => "deadline",
            Self::Memory => "memory",
            Self::HostCalls => "host-call",
            Self::Output => "output",
            Self::MaintenanceFuel => "maintenance fuel",
            Self::MaintenanceDeadline => "maintenance deadline",
        }
    }
    /// The engine maximum of this budget.
    #[must_use]
    pub fn maximum(self) -> u64 {
        match self {
            Self::Fuel => MAX_FUEL_PER_TURN,
            Self::Deadline => MAX_DEADLINE_PER_TURN_MS,
            Self::Memory => MAX_MEMORY_BYTES,
            Self::HostCalls => u64::from(MAX_HOST_CALLS_PER_TURN),
            Self::Output => MAX_OUTPUT_BYTES,
            Self::MaintenanceFuel => MAX_MAINTENANCE_FUEL,
            Self::MaintenanceDeadline => MAX_MAINTENANCE_DEADLINE_MS,
        }
    }
    /// The v38 production default ([`Budgets::TURN`]) in the wire unit: an
    /// omitted request's value and, without a ceiling, the cap.
    #[must_use]
    pub fn production(self) -> u64 {
        Budgets::TURN.value(self)
    }
}

impl BudgetRequest {
    #[must_use]
    pub fn value(&self, kind: BudgetKind) -> Option<u64> {
        match kind {
            BudgetKind::Fuel => self.fuel_per_turn,
            BudgetKind::Deadline => self.deadline_per_turn_ms,
            BudgetKind::Memory => self.memory_bytes,
            BudgetKind::HostCalls => self.host_calls_per_turn.map(u64::from),
            BudgetKind::Output => self.output_bytes,
            BudgetKind::MaintenanceFuel => self.maintenance_fuel,
            BudgetKind::MaintenanceDeadline => self.maintenance_deadline_ms,
        }
    }
    /// Every present value must be positive and within its engine maximum.
    pub fn validate(&self) -> crate::Result<()> {
        for kind in BudgetKind::ALL {
            if let Some(value) = self.value(kind)
                && !(1..=kind.maximum()).contains(&value)
            {
                return Err(crate::VartaError::InvalidManifest(format!(
                    "budget {} must be between 1 and {}",
                    kind.wire_name(),
                    kind.maximum()
                )));
            }
        }
        Ok(())
    }
    #[must_use]
    pub fn is_empty(&self) -> bool {
        BudgetKind::ALL
            .iter()
            .all(|kind| self.value(*kind).is_none())
    }
}

/// Effective budgets of one instance.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Budgets {
    pub fuel_per_turn: u64,
    pub deadline_per_turn: Duration,
    pub memory_bytes: usize,
    pub host_calls_per_turn: u32,
    pub output_bytes: usize,
    /// Fuel of one idle period's maintenance; zero: no maintenance.
    pub maintenance_fuel: u64,
    pub maintenance_deadline: Duration,
}
impl Budgets {
    /// The frozen production budgets (`abi`) of every profile before v38 and
    /// of the tool and HTTP entry points, which run no maintenance.
    pub const PRODUCTION: Self = Self {
        fuel_per_turn: abi::FUEL,
        deadline_per_turn: Duration::from_secs(abi::DEADLINE_SECONDS),
        memory_bytes: abi::MAX_MEMORY_BYTES,
        host_calls_per_turn: abi::MAX_HOST_CALLS,
        output_bytes: abi::MAX_OUTPUT_BYTES,
        maintenance_fuel: 0,
        maintenance_deadline: Duration::ZERO,
    };
    /// The production budgets of a v38 (`turn_budgets`) instance, per turn:
    /// [`TURN_FUEL`] fuel and, per idle period, [`MAINTENANCE_FUEL`] and
    /// [`MAINTENANCE_DEADLINE_MS`]; the other budgets as
    /// [`Self::PRODUCTION`].
    pub const TURN: Self = Self {
        fuel_per_turn: TURN_FUEL,
        maintenance_fuel: MAINTENANCE_FUEL,
        maintenance_deadline: Duration::from_millis(MAINTENANCE_DEADLINE_MS),
        ..Self::PRODUCTION
    };
    /// One budget in its wire unit (milliseconds for the deadline).
    #[must_use]
    pub fn value(&self, kind: BudgetKind) -> u64 {
        match kind {
            BudgetKind::Fuel => self.fuel_per_turn,
            BudgetKind::Deadline => {
                u64::try_from(self.deadline_per_turn.as_millis()).unwrap_or(u64::MAX)
            }
            BudgetKind::Memory => self.memory_bytes as u64,
            BudgetKind::HostCalls => u64::from(self.host_calls_per_turn),
            BudgetKind::Output => self.output_bytes as u64,
            BudgetKind::MaintenanceFuel => self.maintenance_fuel,
            BudgetKind::MaintenanceDeadline => {
                u64::try_from(self.maintenance_deadline.as_millis()).unwrap_or(u64::MAX)
            }
        }
    }
    fn set(&mut self, kind: BudgetKind, value: u64) {
        match kind {
            BudgetKind::Fuel => self.fuel_per_turn = value,
            BudgetKind::Deadline => self.deadline_per_turn = Duration::from_millis(value),
            // Values are bounded by the engine maxima, which fit usize/u32.
            BudgetKind::Memory => self.memory_bytes = usize::try_from(value).unwrap_or(usize::MAX),
            BudgetKind::HostCalls => {
                self.host_calls_per_turn = u32::try_from(value).unwrap_or(u32::MAX);
            }
            BudgetKind::Output => self.output_bytes = usize::try_from(value).unwrap_or(usize::MAX),
            BudgetKind::MaintenanceFuel => self.maintenance_fuel = value,
            BudgetKind::MaintenanceDeadline => {
                self.maintenance_deadline = Duration::from_millis(value);
            }
        }
    }
    /// Resolve a v38 package request against the host ceiling: each budget
    /// is the request (or the v38 production default, [`Self::TURN`]) capped
    /// by the ceiling (or, without a ceiling value, by that default). Both
    /// inputs must already be valid.
    #[must_use]
    pub fn resolve(requested: &BudgetRequest, ceiling: Option<&BudgetRequest>) -> ResolvedBudgets {
        let mut budgets = Self::TURN;
        let mut capped = Vec::new();
        for kind in BudgetKind::ALL {
            let wanted = requested.value(kind).unwrap_or(kind.production());
            let cap = ceiling
                .and_then(|ceiling| ceiling.value(kind))
                .unwrap_or(kind.production());
            if wanted > cap {
                capped.push(CappedBudget {
                    kind,
                    requested: wanted,
                    allowed: cap,
                });
            }
            budgets.set(kind, wanted.min(cap));
        }
        ResolvedBudgets { budgets, capped }
    }
}

/// A requested budget the host ceiling (or the production default) lowered.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CappedBudget {
    pub kind: BudgetKind,
    pub requested: u64,
    pub allowed: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedBudgets {
    pub budgets: Budgets,
    pub capped: Vec<CappedBudget>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn omitted_requests_keep_the_production_budgets() {
        let resolved = Budgets::resolve(&BudgetRequest::default(), None);
        assert_eq!(resolved.budgets, Budgets::TURN);
        assert!(resolved.capped.is_empty());
        assert_eq!(BudgetKind::Fuel.production(), TURN_FUEL);
        // Only the per-turn fuel and the maintenance budget differ from the
        // frozen production budgets, which run no maintenance.
        assert_eq!(
            Budgets {
                fuel_per_turn: abi::FUEL,
                maintenance_fuel: 0,
                maintenance_deadline: Duration::ZERO,
                ..Budgets::TURN
            },
            Budgets::PRODUCTION
        );
        assert_eq!(BudgetKind::MaintenanceFuel.production(), MAINTENANCE_FUEL);
        assert!(MAINTENANCE_STEP_FUEL < MAINTENANCE_FUEL);
        assert_eq!(
            BudgetKind::Deadline.production(),
            abi::DEADLINE_SECONDS * MILLISECONDS_PER_SECOND
        );
    }

    #[test]
    fn packages_lower_budgets_and_only_the_ceiling_raises_them() {
        let lower = BudgetRequest {
            fuel_per_turn: Some(1000),
            deadline_per_turn_ms: Some(250),
            ..BudgetRequest::default()
        };
        let resolved = Budgets::resolve(&lower, None);
        assert_eq!(resolved.budgets.fuel_per_turn, 1000);
        assert_eq!(
            resolved.budgets.deadline_per_turn,
            Duration::from_millis(250)
        );
        assert!(resolved.capped.is_empty());

        let raise = BudgetRequest {
            fuel_per_turn: Some(40_000_000),
            ..BudgetRequest::default()
        };
        // Without a ceiling value the production default caps the request.
        let capped = Budgets::resolve(&raise, None);
        assert_eq!(capped.budgets.fuel_per_turn, TURN_FUEL);
        assert_eq!(
            capped.capped,
            [CappedBudget {
                kind: BudgetKind::Fuel,
                requested: 40_000_000,
                allowed: TURN_FUEL
            }]
        );
        let ceiling = BudgetRequest {
            fuel_per_turn: Some(30_000_000),
            memory_bytes: Some(16 * 1024 * 1024),
            ..BudgetRequest::default()
        };
        let resolved = Budgets::resolve(&raise, Some(&ceiling));
        assert_eq!(resolved.budgets.fuel_per_turn, 30_000_000);
        // A raised ceiling grants nothing the package did not request.
        assert_eq!(resolved.budgets.memory_bytes, abi::MAX_MEMORY_BYTES);
        assert_eq!(resolved.capped[0].allowed, 30_000_000);
        // A ceiling below the default also lowers an omitted request.
        let tight = BudgetRequest {
            host_calls_per_turn: Some(8),
            ..BudgetRequest::default()
        };
        let resolved = Budgets::resolve(&BudgetRequest::default(), Some(&tight));
        assert_eq!(resolved.budgets.host_calls_per_turn, 8);
    }

    #[test]
    fn budget_values_are_positive_and_bounded() {
        for kind in BudgetKind::ALL {
            for bad in [0, kind.maximum() + 1] {
                let mut request = BudgetRequest::default();
                match kind {
                    BudgetKind::Fuel => request.fuel_per_turn = Some(bad),
                    BudgetKind::Deadline => request.deadline_per_turn_ms = Some(bad),
                    BudgetKind::Memory => request.memory_bytes = Some(bad),
                    BudgetKind::HostCalls => {
                        request.host_calls_per_turn = u32::try_from(bad).ok().or(Some(0));
                    }
                    BudgetKind::Output => request.output_bytes = Some(bad),
                    BudgetKind::MaintenanceFuel => request.maintenance_fuel = Some(bad),
                    BudgetKind::MaintenanceDeadline => {
                        request.maintenance_deadline_ms = Some(bad);
                    }
                }
                assert!(request.validate().is_err(), "{kind:?} {bad}");
            }
        }
        assert!(
            serde_json::from_str::<BudgetRequest>(r#"{"fuel":1}"#).is_err(),
            "unknown members are rejected"
        );
        let request: BudgetRequest =
            serde_json::from_str(r#"{"fuelPerTurn":5,"outputBytes":10}"#).unwrap();
        request.validate().unwrap();
        assert_eq!(request.value(BudgetKind::Output), Some(10));
    }
}
