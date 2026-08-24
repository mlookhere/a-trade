use crate::{Condition, Direction, GammaRegime, MarketState, RejectionCode, SetupState};

/// Verbatim canonical §121 master prompt. Runtime code does not rewrite or tune it.
pub const CANONICAL_AGENT_MASTER_PROMPT: &str =
    include_str!("../../../prompts/agent_master_prompt_canonical.txt");

/// Canonical architecture roles from §§5-6.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AgentRole {
    DataSupervisor,
    Environment,
    Gamma,
    ProfileLocation,
    SetupCoordinator,
    OrderFlow,
    StrategyValidator,
    RiskEngine,
    ExecutionEngine,
    PositionManager,
    Audit,
}

impl AgentRole {
    /// Canonical §6: only the Order-Flow Agent may propose a trade.
    #[must_use]
    pub const fn may_propose_trade(self) -> bool {
        matches!(self, Self::OrderFlow)
    }

    /// No LLM advisory output, including one labeled with the Execution Engine role, receives
    /// broker-transmission authority. Real deterministic execution remains in ExecutionCoordinator.
    #[must_use]
    pub const fn llm_may_submit_order(self) -> bool {
        false
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct AdvisoryGamma {
    pub regime: Option<GammaRegime>,
    pub flip: Option<f64>,
    pub call_wall: Option<f64>,
    pub put_wall: Option<f64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct AdvisoryValue {
    pub vah: Option<f64>,
    pub val: Option<f64>,
    pub poc: Option<f64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct AdvisorySwing {
    pub low: Option<f64>,
    pub high: Option<f64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct AdvisoryFib {
    pub level_705: Option<f64>,
    pub level_788: Option<f64>,
    pub level_886: Option<f64>,
}

/// Runtime form of canonical §107 mandatory conditions using §108 tri-state semantics.
/// The source JSON example uses booleans, but §108 explicitly requires TRUE/FALSE/UNKNOWN.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct AdvisoryConditions {
    pub data_valid: Condition,
    pub market_state_valid: Condition,
    pub direction_valid: Condition,
    pub location_valid: Condition,
    pub zone_reached: Condition,
    pub not_invalidated: Condition,
    pub participation_valid: Condition,
    pub countertrend_aggression: Condition,
    pub aggression_at_extreme: Condition,
    pub effort_failed: Condition,
    pub absorption: Condition,
    pub first_dominance_shift: Condition,
    pub second_attempt: Condition,
    pub second_failure: Condition,
    pub final_reconfirmation: Condition,
    pub structural_target_valid: Condition,
    pub risk_valid: Condition,
    pub portfolio_risk_valid: Condition,
    pub duplicate_check_pass: Condition,
    pub time_valid: Condition,
}

impl AdvisoryConditions {
    #[must_use]
    pub fn setup_pass(self) -> Condition {
        all_conditions(&[
            self.data_valid,
            self.market_state_valid,
            self.direction_valid,
            self.location_valid,
            self.zone_reached,
            self.not_invalidated,
            self.participation_valid,
            self.countertrend_aggression,
            self.aggression_at_extreme,
            self.effort_failed,
            self.absorption,
            self.first_dominance_shift,
            self.second_attempt,
            self.second_failure,
            self.final_reconfirmation,
            self.structural_target_valid,
            self.risk_valid,
            self.portfolio_risk_valid,
            self.duplicate_check_pass,
            self.time_valid,
        ])
    }
}

/// LLM-facing §107-shaped evaluation. It is deliberately advisory: `trade_allowed` must remain
/// false and this type exposes no broker, risk mutation, order-construction, or permit API.
#[derive(Debug, Clone, PartialEq)]
pub struct AdvisorySetupEvaluation {
    pub setup_id: String,
    pub agent_id: String,
    pub instrument: String,
    pub timestamp_et: String,
    pub role: AgentRole,
    pub market_state: Option<MarketState>,
    pub direction: Option<Direction>,
    pub gamma: AdvisoryGamma,
    pub value: AdvisoryValue,
    pub swing: AdvisorySwing,
    pub fib: AdvisoryFib,
    pub conditions: AdvisoryConditions,
    pub trade_allowed: bool,
    pub state: Option<SetupState>,
    pub rejection_code: Option<RejectionCode>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AdvisoryValidationError {
    MissingIdentity,
    NonFiniteNumericValue,
    LlmAuthorityViolation,
}

impl AdvisorySetupEvaluation {
    /// Structural validation only. Deterministic strategy/risk/execution modules independently
    /// recalculate their own facts under §§109 and 122.
    pub fn validate(&self) -> Result<(), AdvisoryValidationError> {
        if self.setup_id.trim().is_empty()
            || self.agent_id.trim().is_empty()
            || self.instrument.trim().is_empty()
            || self.timestamp_et.trim().is_empty()
        {
            return Err(AdvisoryValidationError::MissingIdentity);
        }
        if self.trade_allowed {
            return Err(AdvisoryValidationError::LlmAuthorityViolation);
        }
        if !all_optional_finite(&[
            self.gamma.flip,
            self.gamma.call_wall,
            self.gamma.put_wall,
            self.value.vah,
            self.value.val,
            self.value.poc,
            self.swing.low,
            self.swing.high,
            self.fib.level_705,
            self.fib.level_788,
            self.fib.level_886,
        ]) {
            return Err(AdvisoryValidationError::NonFiniteNumericValue);
        }
        Ok(())
    }

    /// The only authorization-related output exposed by the advisory boundary. TRUE means the
    /// LLM-side setup assessment passed; it is still only one input to canonical §109.
    #[must_use]
    pub fn llm_setup_pass(&self) -> Condition {
        if self.validate().is_err() {
            return Condition::Unknown;
        }
        self.conditions.setup_pass()
    }
}

fn all_optional_finite(values: &[Option<f64>]) -> bool {
    values.iter().all(|value| value.is_none_or(f64::is_finite))
}

fn all_conditions(conditions: &[Condition]) -> Condition {
    if conditions.contains(&Condition::False) {
        Condition::False
    } else if conditions.contains(&Condition::Unknown) {
        Condition::Unknown
    } else {
        Condition::True
    }
}
