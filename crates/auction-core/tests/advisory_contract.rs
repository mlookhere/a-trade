use auction_core::{
    AdvisoryConditions, AdvisoryFib, AdvisoryGamma, AdvisorySetupEvaluation, AdvisorySwing,
    AdvisoryValidationError, AdvisoryValue, AgentRole, CANONICAL_AGENT_MASTER_PROMPT, Condition,
    Direction, MarketState, SetupState,
};

fn all_true_conditions() -> AdvisoryConditions {
    AdvisoryConditions {
        data_valid: Condition::True,
        market_state_valid: Condition::True,
        direction_valid: Condition::True,
        location_valid: Condition::True,
        zone_reached: Condition::True,
        not_invalidated: Condition::True,
        participation_valid: Condition::True,
        countertrend_aggression: Condition::True,
        aggression_at_extreme: Condition::True,
        effort_failed: Condition::True,
        absorption: Condition::True,
        first_dominance_shift: Condition::True,
        second_attempt: Condition::True,
        second_failure: Condition::True,
        final_reconfirmation: Condition::True,
        structural_target_valid: Condition::True,
        risk_valid: Condition::True,
        portfolio_risk_valid: Condition::True,
        duplicate_check_pass: Condition::True,
        time_valid: Condition::True,
    }
}

fn evaluation() -> AdvisorySetupEvaluation {
    AdvisorySetupEvaluation {
        setup_id: "MNQ_SETUP_30".to_owned(),
        agent_id: "MNQ_ORDERFLOW".to_owned(),
        instrument: "MNQ".to_owned(),
        timestamp_et: "10:15:00".to_owned(),
        role: AgentRole::OrderFlow,
        market_state: Some(MarketState::ValueUp),
        direction: Some(Direction::Long),
        gamma: AdvisoryGamma::default(),
        value: AdvisoryValue::default(),
        swing: AdvisorySwing::default(),
        fib: AdvisoryFib::default(),
        conditions: all_true_conditions(),
        trade_allowed: false,
        state: Some(SetupState::FinalReconfirmation),
        rejection_code: None,
    }
}

#[test]
fn section_121_master_prompt_is_embedded_without_runtime_rewrite() {
    assert!(CANONICAL_AGENT_MASTER_PROMPT.starts_with(
        "You are one component of a deterministic multi-agent auction-market trading system."
    ));
    assert!(CANONICAL_AGENT_MASTER_PROMPT.contains(
        "The trading strategy must always progress in this order: environment, location, participation, effort versus result, absorption, dominance shift, second attempt, second failure, reconfirmation, risk validation, execution."
    ));
    assert!(CANONICAL_AGENT_MASTER_PROMPT.ends_with("If the system must guess, output NO_TRADE.\n"));
}

#[test]
fn sections_6_109_advisory_roles_never_have_order_submission_authority() {
    let roles = [
        AgentRole::DataSupervisor,
        AgentRole::Environment,
        AgentRole::Gamma,
        AgentRole::ProfileLocation,
        AgentRole::SetupCoordinator,
        AgentRole::OrderFlow,
        AgentRole::StrategyValidator,
        AgentRole::RiskEngine,
        AgentRole::ExecutionEngine,
        AgentRole::PositionManager,
        AgentRole::Audit,
    ];
    for role in roles {
        assert!(!role.llm_may_submit_order());
    }
    assert!(AgentRole::OrderFlow.may_propose_trade());
    assert!(!AgentRole::Environment.may_propose_trade());
    assert!(!AgentRole::RiskEngine.may_propose_trade());
}

#[test]
fn sections_107_108_all_true_advisory_conditions_produce_only_llm_setup_pass() {
    let result = evaluation();
    assert_eq!(result.validate(), Ok(()));
    assert_eq!(result.llm_setup_pass(), Condition::True);
    assert!(!result.trade_allowed);
}

#[test]
fn section_108_false_and_unknown_advisory_conditions_fail_closed() {
    let mut unknown = evaluation();
    unknown.conditions.absorption = Condition::Unknown;
    assert_eq!(unknown.llm_setup_pass(), Condition::Unknown);

    let mut rejected = evaluation();
    rejected.conditions.absorption = Condition::Unknown;
    rejected.conditions.effort_failed = Condition::False;
    assert_eq!(rejected.llm_setup_pass(), Condition::False);
}

#[test]
fn sections_107_109_llm_trade_allowed_true_is_an_authority_violation() {
    let mut result = evaluation();
    result.trade_allowed = true;
    assert_eq!(
        result.validate(),
        Err(AdvisoryValidationError::LlmAuthorityViolation)
    );
    assert_eq!(result.llm_setup_pass(), Condition::Unknown);
}

#[test]
fn sections_10_107_missing_or_invalid_advisory_data_is_unknown_not_invented() {
    let mut missing = evaluation();
    missing.setup_id.clear();
    assert_eq!(
        missing.validate(),
        Err(AdvisoryValidationError::MissingIdentity)
    );
    assert_eq!(missing.llm_setup_pass(), Condition::Unknown);

    let mut invalid = evaluation();
    invalid.value.vah = Some(f64::NAN);
    assert_eq!(
        invalid.validate(),
        Err(AdvisoryValidationError::NonFiniteNumericValue)
    );
    assert_eq!(invalid.llm_setup_pass(), Condition::Unknown);
}

#[test]
fn section_107_default_condition_set_is_unknown_and_cannot_pass() {
    assert_eq!(AdvisoryConditions::default().setup_pass(), Condition::Unknown);
}
