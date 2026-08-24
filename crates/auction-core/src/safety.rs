use std::collections::HashSet;

use crate::{Condition, RejectionCode};

/// Canonical §100/§101 violation scope. The caller must classify the origin explicitly;
/// this module never guesses whether a malfunction is local or shared infrastructure.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProcessViolationScope {
    Agent(String),
    SharedInfrastructure,
}

/// Deterministic operational-safety state for §§98-101.
///
/// Current operational health and emergency-drawdown status are tri-state and start UNKNOWN,
/// so a newly created controller cannot authorize new automation until both are validated.
/// Confirmed emergency drawdown and process-violation disables are latched for the lifetime of
/// this controller; no automatic reset behavior is provided because canonical knowledge does not
/// define one.
#[derive(Debug, Default)]
pub struct OperationalSafetyController {
    operational_risk_clear: Condition,
    emergency_drawdown_clear: Condition,
    system_disabled: bool,
    disabled_agents: HashSet<String>,
}

impl OperationalSafetyController {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// §98 receives an already validated aggregate operational-health fact. The canonical
    /// examples are not promoted into an exhaustive hard-coded fault taxonomy here.
    pub fn set_operational_risk_clear(&mut self, condition: Condition) {
        self.operational_risk_clear = condition;
    }

    #[must_use]
    pub const fn operational_risk_clear(&self) -> Condition {
        self.operational_risk_clear
    }

    #[must_use]
    pub const fn emergency_drawdown_clear(&self) -> Condition {
        self.emergency_drawdown_clear
    }

    #[must_use]
    pub const fn system_disabled(&self) -> bool {
        self.system_disabled
    }

    #[must_use]
    pub fn agent_disabled(&self, agent_id: &str) -> bool {
        self.disabled_agents.contains(agent_id)
    }

    /// §99 consumes a deterministic threshold result. TRUE permanently disables automation for
    /// this controller; UNKNOWN blocks authorization but does not invent a permanent breach.
    pub fn observe_emergency_drawdown(&mut self, reached: Condition) {
        match reached {
            Condition::True => {
                self.emergency_drawdown_clear = Condition::False;
                self.system_disabled = true;
            }
            Condition::False => {
                self.emergency_drawdown_clear = Condition::True;
            }
            Condition::Unknown => {
                self.emergency_drawdown_clear = Condition::Unknown;
            }
        }
    }

    /// §100 disables the affected agent. §101 permits whole-system disable when the originating
    /// failure is shared infrastructure. Invalid agent identifiers fail closed by degrading
    /// current operational health to UNKNOWN and returning PROCESS_ERROR.
    pub fn record_process_violation(
        &mut self,
        scope: ProcessViolationScope,
    ) -> Result<(), RejectionCode> {
        match scope {
            ProcessViolationScope::Agent(agent_id) => {
                if agent_id.trim().is_empty() {
                    self.operational_risk_clear = Condition::Unknown;
                    return Err(RejectionCode::ProcessError);
                }
                self.disabled_agents.insert(agent_id);
            }
            ProcessViolationScope::SharedInfrastructure => {
                self.system_disabled = true;
            }
        }
        Ok(())
    }

    /// New automation is allowed only when every operational safety fact is explicitly TRUE and
    /// no system-wide latch has fired.
    #[must_use]
    pub fn system_automation_allowed(&self) -> Condition {
        if self.system_disabled {
            return Condition::False;
        }
        all_clear(&[self.operational_risk_clear, self.emergency_drawdown_clear])
    }

    /// Agent-local isolation is layered on top of shared system safety. Empty identifiers are
    /// UNKNOWN rather than silently mapped to another agent or scope.
    #[must_use]
    pub fn agent_automation_allowed(&self, agent_id: &str) -> Condition {
        if agent_id.trim().is_empty() {
            return Condition::Unknown;
        }
        if self.system_disabled || self.disabled_agents.contains(agent_id) {
            return Condition::False;
        }
        all_clear(&[self.operational_risk_clear, self.emergency_drawdown_clear])
    }
}

/// §99 threshold comparison. Both values are externally supplied; no universal percentage and no
/// session-equity-to-drawdown derivation are invented here.
#[must_use]
pub fn emergency_drawdown_reached(
    session_drawdown_fraction: Option<f64>,
    configured_threshold_fraction: Option<f64>,
) -> Condition {
    let (Some(drawdown), Some(threshold)) =
        (session_drawdown_fraction, configured_threshold_fraction)
    else {
        return Condition::Unknown;
    };
    if !drawdown.is_finite() || !threshold.is_finite() || drawdown < 0.0 || threshold < 0.0 {
        return Condition::Unknown;
    }
    Condition::from(drawdown >= threshold)
}

/// Aggregate caller-validated operational-clear facts without inventing a fault taxonomy.
/// FALSE dominates UNKNOWN; UNKNOWN dominates TRUE.
#[must_use]
pub fn aggregate_operational_risk_clear(signals_clear: &[Condition]) -> Condition {
    all_clear(signals_clear)
}

fn all_clear(conditions: &[Condition]) -> Condition {
    if conditions.contains(&Condition::False) {
        Condition::False
    } else if conditions.contains(&Condition::Unknown) {
        Condition::Unknown
    } else {
        Condition::True
    }
}
