use crate::Condition;

/// Union of mandatory production conditions from canonical §§56, 95-96, and 122.
/// Direction-specific structural comparisons are supplied as tri-state facts by the
/// deterministic strategy validator; FALSE or UNKNOWN remains fail-closed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProductionConditions {
    pub time_valid: Condition,
    pub data_valid: Condition,
    pub premarket_plan_complete: Condition,
    pub environment_valid: Condition,
    pub direction_valid: Condition,
    pub qualified_swing_exists: Condition,
    pub fib_zone_valid: Condition,
    pub fib_zone_outside_value: Condition,
    pub location_reached: Condition,
    pub fib_886_valid: Condition,
    pub participation_valid: Condition,
    pub countertrend_aggression: Condition,
    pub aggression_at_extreme: Condition,
    pub effort_failed: Condition,
    pub absorption_present: Condition,
    pub first_dominance_shift: Condition,
    pub second_attempt_present: Condition,
    pub second_attempt_has_countertrend_aggression: Condition,
    pub second_failure_structure_valid: Condition,
    pub second_failure_valid: Condition,
    pub final_reconfirmation: Condition,
    pub structural_target_valid: Condition,
    pub per_trade_risk_valid: Condition,
    pub portfolio_risk_valid: Condition,
    pub correlation_risk_valid: Condition,
    pub setup_not_duplicated: Condition,
    pub no_conflicting_order: Condition,
    pub news_blackout_clear: Condition,
    pub broker_safe: Condition,
    pub execution_engine_safe: Condition,
}

impl ProductionConditions {
    #[must_use]
    pub fn all_true(self) -> bool {
        [
            self.time_valid,
            self.data_valid,
            self.premarket_plan_complete,
            self.environment_valid,
            self.direction_valid,
            self.qualified_swing_exists,
            self.fib_zone_valid,
            self.fib_zone_outside_value,
            self.location_reached,
            self.fib_886_valid,
            self.participation_valid,
            self.countertrend_aggression,
            self.aggression_at_extreme,
            self.effort_failed,
            self.absorption_present,
            self.first_dominance_shift,
            self.second_attempt_present,
            self.second_attempt_has_countertrend_aggression,
            self.second_failure_structure_valid,
            self.second_failure_valid,
            self.final_reconfirmation,
            self.structural_target_valid,
            self.per_trade_risk_valid,
            self.portfolio_risk_valid,
            self.correlation_risk_valid,
            self.setup_not_duplicated,
            self.no_conflicting_order,
            self.news_blackout_clear,
            self.broker_safe,
            self.execution_engine_safe,
        ]
        .into_iter()
        .all(Condition::permits)
    }
}

/// Authority chain from canonical §109. LLM interpretation remains advisory until every
/// deterministic subsystem passes independently.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AuthorizationInputs {
    pub llm_setup_pass: Condition,
    pub strategy_validator_pass: Condition,
    pub risk_engine_pass: Condition,
    pub portfolio_coordinator_pass: Condition,
    pub execution_gate_pass: Condition,
}

impl AuthorizationInputs {
    #[must_use]
    pub fn all_true(self) -> bool {
        [
            self.llm_setup_pass,
            self.strategy_validator_pass,
            self.risk_engine_pass,
            self.portfolio_coordinator_pass,
            self.execution_gate_pass,
        ]
        .into_iter()
        .all(Condition::permits)
    }
}

/// Canonical §§10, 56, 95-96, 108-109, and 122: every required strategy and authority gate
/// must be TRUE. There is intentionally no trade-count or consecutive-loss input.
#[must_use]
pub fn trade_authorized(
    conditions: ProductionConditions,
    authority: AuthorizationInputs,
) -> bool {
    conditions.all_true() && authority.all_true()
}
