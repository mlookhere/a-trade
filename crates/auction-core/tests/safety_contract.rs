use auction_core::{
    Condition, OperationalSafetyController, ProcessViolationScope,
    aggregate_operational_risk_clear, emergency_drawdown_reached,
};

fn validated_clear_controller() -> OperationalSafetyController {
    let mut safety = OperationalSafetyController::new();
    safety.set_operational_risk_clear(Condition::True);
    safety.observe_emergency_drawdown(Condition::False);
    safety
}

#[test]
fn sections_98_108_default_safety_state_is_unknown_and_fails_closed() {
    let safety = OperationalSafetyController::new();
    assert_eq!(safety.operational_risk_clear(), Condition::Unknown);
    assert_eq!(safety.emergency_drawdown_clear(), Condition::Unknown);
    assert_eq!(safety.system_automation_allowed(), Condition::Unknown);
    assert_eq!(
        safety.agent_automation_allowed("MNQ_AGENT"),
        Condition::Unknown
    );
}

#[test]
fn section_98_operational_health_requires_explicit_true() {
    let mut safety = OperationalSafetyController::new();
    safety.observe_emergency_drawdown(Condition::False);

    safety.set_operational_risk_clear(Condition::False);
    assert_eq!(safety.system_automation_allowed(), Condition::False);

    safety.set_operational_risk_clear(Condition::Unknown);
    assert_eq!(safety.system_automation_allowed(), Condition::Unknown);

    safety.set_operational_risk_clear(Condition::True);
    assert_eq!(safety.system_automation_allowed(), Condition::True);
}

#[test]
fn section_98_generic_operational_signal_aggregation_is_tri_state() {
    assert_eq!(
        aggregate_operational_risk_clear(&[Condition::True, Condition::True]),
        Condition::True
    );
    assert_eq!(
        aggregate_operational_risk_clear(&[Condition::True, Condition::Unknown]),
        Condition::Unknown
    );
    assert_eq!(
        aggregate_operational_risk_clear(&[Condition::True, Condition::Unknown, Condition::False,]),
        Condition::False
    );
    assert_eq!(aggregate_operational_risk_clear(&[]), Condition::True);
}

#[test]
fn section_99_drawdown_threshold_has_no_default_and_uses_exact_configured_boundary() {
    assert_eq!(
        emergency_drawdown_reached(None, Some(0.01)),
        Condition::Unknown
    );
    assert_eq!(
        emergency_drawdown_reached(Some(0.01), None),
        Condition::Unknown
    );
    assert_eq!(
        emergency_drawdown_reached(Some(0.0099), Some(0.01)),
        Condition::False
    );
    assert_eq!(
        emergency_drawdown_reached(Some(0.01), Some(0.01)),
        Condition::True
    );
    assert_eq!(
        emergency_drawdown_reached(Some(0.0101), Some(0.01)),
        Condition::True
    );
}

#[test]
fn section_99_invalid_drawdown_inputs_are_unknown() {
    for value in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY, -0.001] {
        assert_eq!(
            emergency_drawdown_reached(Some(value), Some(0.01)),
            Condition::Unknown
        );
        assert_eq!(
            emergency_drawdown_reached(Some(0.01), Some(value)),
            Condition::Unknown
        );
    }
}

#[test]
fn section_99_confirmed_emergency_drawdown_latches_whole_system_disabled() {
    let mut safety = validated_clear_controller();
    assert_eq!(safety.system_automation_allowed(), Condition::True);

    safety.observe_emergency_drawdown(Condition::True);
    assert!(safety.system_disabled());
    assert_eq!(safety.system_automation_allowed(), Condition::False);
    assert_eq!(
        safety.agent_automation_allowed("MNQ_AGENT"),
        Condition::False
    );

    // A later non-breach observation does not invent an automatic session reset.
    safety.observe_emergency_drawdown(Condition::False);
    safety.set_operational_risk_clear(Condition::True);
    assert!(safety.system_disabled());
    assert_eq!(safety.system_automation_allowed(), Condition::False);
}

#[test]
fn section_99_unknown_drawdown_blocks_without_permanent_latch() {
    let mut safety = validated_clear_controller();
    safety.observe_emergency_drawdown(Condition::Unknown);
    assert!(!safety.system_disabled());
    assert_eq!(safety.system_automation_allowed(), Condition::Unknown);

    safety.observe_emergency_drawdown(Condition::False);
    assert_eq!(safety.system_automation_allowed(), Condition::True);
}

#[test]
fn sections_100_101_agent_local_violation_disables_only_that_agent() {
    let mut safety = validated_clear_controller();
    safety
        .record_process_violation(ProcessViolationScope::Agent("MNQ_AGENT".to_owned()))
        .unwrap();

    assert!(safety.agent_disabled("MNQ_AGENT"));
    assert_eq!(
        safety.agent_automation_allowed("MNQ_AGENT"),
        Condition::False
    );
    assert_eq!(safety.agent_automation_allowed("ES_AGENT"), Condition::True);
    assert!(!safety.system_disabled());
    assert_eq!(safety.system_automation_allowed(), Condition::True);
}

#[test]
fn sections_100_101_shared_infrastructure_violation_disables_whole_system() {
    let mut safety = validated_clear_controller();
    safety
        .record_process_violation(ProcessViolationScope::SharedInfrastructure)
        .unwrap();

    assert!(safety.system_disabled());
    assert_eq!(safety.system_automation_allowed(), Condition::False);
    assert_eq!(
        safety.agent_automation_allowed("MNQ_AGENT"),
        Condition::False
    );
    assert_eq!(
        safety.agent_automation_allowed("ES_AGENT"),
        Condition::False
    );
}

#[test]
fn section_100_invalid_agent_identifier_fails_closed_without_reclassifying_scope() {
    let mut safety = validated_clear_controller();
    assert!(
        safety
            .record_process_violation(ProcessViolationScope::Agent("   ".to_owned()))
            .is_err()
    );
    assert!(!safety.system_disabled());
    assert!(!safety.agent_disabled(""));
    assert_eq!(safety.operational_risk_clear(), Condition::Unknown);
    assert_eq!(
        safety.agent_automation_allowed("MNQ_AGENT"),
        Condition::Unknown
    );
    assert_eq!(safety.agent_automation_allowed(""), Condition::Unknown);
}

#[test]
fn sections_100_101_agent_disable_is_latched() {
    let mut safety = validated_clear_controller();
    safety
        .record_process_violation(ProcessViolationScope::Agent("MNQ_AGENT".to_owned()))
        .unwrap();
    safety.set_operational_risk_clear(Condition::True);
    safety.observe_emergency_drawdown(Condition::False);

    assert_eq!(
        safety.agent_automation_allowed("MNQ_AGENT"),
        Condition::False
    );
    assert_eq!(safety.agent_automation_allowed("ES_AGENT"), Condition::True);
}
