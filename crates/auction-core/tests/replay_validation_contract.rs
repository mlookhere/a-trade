use auction_core::{
    AuthorizationInputs, Condition, Direction, EtTime, NewsBlackoutWindow,
    OperationalSafetyController, ProcessViolationScope, ProductionConditions, RejectionCode,
    ReplayEvidenceError, ReplayOutcome, SessionPermissions, SetupState, SetupStateError,
    TerminalState, ValidationDatasetManifest, ValidationManifestError, ValidationPhase,
    defensive_stop_candidate_allowed, evaluate_news_gate, record_replay_case,
    stop_replacement_allowed, summarize_replay, trade_authorized,
};

type ConditionMutation = (&'static str, fn(&mut ProductionConditions));

fn manifest() -> ValidationDatasetManifest {
    ValidationDatasetManifest {
        strategy_version: "strategy-v1".to_owned(),
        backtest_session_ids: vec!["BT-001".to_owned(), "BT-002".to_owned()],
        out_of_sample_session_ids: vec!["OOS-001".to_owned(), "OOS-002".to_owned()],
    }
}

fn outcome(trade_authorized: bool) -> ReplayOutcome {
    ReplayOutcome {
        trade_authorized,
        rejection_code: None,
        setup_state: None,
    }
}

fn all_true_conditions() -> ProductionConditions {
    ProductionConditions {
        time_valid: Condition::True,
        data_valid: Condition::True,
        premarket_plan_complete: Condition::True,
        environment_valid: Condition::True,
        direction_valid: Condition::True,
        qualified_swing_exists: Condition::True,
        fib_zone_valid: Condition::True,
        fib_zone_outside_value: Condition::True,
        location_reached: Condition::True,
        fib_886_valid: Condition::True,
        participation_valid: Condition::True,
        countertrend_aggression: Condition::True,
        aggression_at_extreme: Condition::True,
        effort_failed: Condition::True,
        absorption_present: Condition::True,
        first_dominance_shift: Condition::True,
        second_attempt_present: Condition::True,
        second_attempt_has_countertrend_aggression: Condition::True,
        second_failure_structure_valid: Condition::True,
        second_failure_valid: Condition::True,
        final_reconfirmation: Condition::True,
        structural_target_valid: Condition::True,
        per_trade_risk_valid: Condition::True,
        portfolio_risk_valid: Condition::True,
        correlation_risk_valid: Condition::True,
        setup_not_duplicated: Condition::True,
        no_conflicting_order: Condition::True,
        news_blackout_clear: Condition::True,
        broker_safe: Condition::True,
        execution_engine_safe: Condition::True,
    }
}

fn all_true_authority() -> AuthorizationInputs {
    AuthorizationInputs {
        llm_setup_pass: Condition::True,
        strategy_validator_pass: Condition::True,
        risk_engine_pass: Condition::True,
        portfolio_coordinator_pass: Condition::True,
        execution_gate_pass: Condition::True,
    }
}

fn record_authorization_case(
    case_id: &str,
    setup_id: &str,
    conditions: ProductionConditions,
    authority: AuthorizationInputs,
    expected_authorized: bool,
) -> auction_core::ReplayCaseEvidence {
    let observed = trade_authorized(conditions, authority);
    record_replay_case(
        &manifest(),
        case_id,
        setup_id,
        "BT-001",
        ValidationPhase::Backtest,
        outcome(expected_authorized),
        outcome(observed),
    )
    .unwrap()
}

#[test]
fn section_116_manifest_keeps_backtest_and_out_of_sample_sessions_disjoint() {
    let valid = manifest();
    assert_eq!(valid.validate(), Ok(()));
    assert_eq!(
        valid.phase_for("BT-001").unwrap(),
        Some(ValidationPhase::Backtest)
    );
    assert_eq!(
        valid.phase_for("OOS-001").unwrap(),
        Some(ValidationPhase::OutOfSample)
    );
    assert_eq!(valid.phase_for("UNLISTED").unwrap(), None);

    let overlap = ValidationDatasetManifest {
        strategy_version: "strategy-v1".to_owned(),
        backtest_session_ids: vec!["SAME".to_owned()],
        out_of_sample_session_ids: vec!["SAME".to_owned()],
    };
    assert_eq!(
        overlap.validate(),
        Err(ValidationManifestError::SplitOverlap)
    );
}

#[test]
fn section_116_manifest_rejects_missing_or_duplicate_validation_identity() {
    let missing_version = ValidationDatasetManifest {
        strategy_version: " ".to_owned(),
        ..manifest()
    };
    assert_eq!(
        missing_version.validate(),
        Err(ValidationManifestError::MissingStrategyVersion)
    );

    let missing_backtest = ValidationDatasetManifest {
        backtest_session_ids: vec![],
        ..manifest()
    };
    assert_eq!(
        missing_backtest.validate(),
        Err(ValidationManifestError::MissingBacktestSessions)
    );

    let missing_oos = ValidationDatasetManifest {
        out_of_sample_session_ids: vec![],
        ..manifest()
    };
    assert_eq!(
        missing_oos.validate(),
        Err(ValidationManifestError::MissingOutOfSampleSessions)
    );

    let empty_id = ValidationDatasetManifest {
        backtest_session_ids: vec!["".to_owned()],
        ..manifest()
    };
    assert_eq!(
        empty_id.validate(),
        Err(ValidationManifestError::EmptySessionId)
    );

    let duplicate = ValidationDatasetManifest {
        backtest_session_ids: vec!["BT-001".to_owned(), "BT-001".to_owned()],
        ..manifest()
    };
    assert_eq!(
        duplicate.validate(),
        Err(ValidationManifestError::DuplicateSessionId)
    );
}

#[test]
fn section_116_replay_case_must_match_the_frozen_validation_split() {
    let result = record_replay_case(
        &manifest(),
        "CASE-1",
        "SETUP-1",
        "OOS-001",
        ValidationPhase::Backtest,
        outcome(false),
        outcome(false),
    );
    assert_eq!(result, Err(ReplayEvidenceError::PhaseMismatch));

    let unknown = record_replay_case(
        &manifest(),
        "CASE-2",
        "SETUP-2",
        "UNKNOWN",
        ValidationPhase::Backtest,
        outcome(false),
        outcome(false),
    );
    assert_eq!(unknown, Err(ReplayEvidenceError::UnknownSession));
}

#[test]
fn section_116_out_of_sample_evidence_records_only_in_the_oos_phase() {
    let evidence = record_replay_case(
        &manifest(),
        "OOS-CASE-1",
        "OOS-SETUP-1",
        "OOS-001",
        ValidationPhase::OutOfSample,
        outcome(false),
        outcome(false),
    )
    .unwrap();
    assert_eq!(evidence.phase(), ValidationPhase::OutOfSample);
    assert!(evidence.exact_match());

    let report = summarize_replay(ValidationPhase::OutOfSample, &[evidence]).unwrap();
    assert_eq!(report.total_cases, 1);
    assert_eq!(report.exact_matches, 1);
    assert_eq!(report.mismatches, 0);
}

#[test]
fn section_116_replay_evidence_rejects_contradictory_authorized_rejection() {
    let contradictory = ReplayOutcome {
        trade_authorized: true,
        rejection_code: Some(RejectionCode::ProcessError),
        setup_state: Some(SetupState::EntryAuthorized),
    };
    assert_eq!(
        record_replay_case(
            &manifest(),
            "CASE-1",
            "SETUP-1",
            "BT-001",
            ValidationPhase::Backtest,
            contradictory,
            outcome(true),
        ),
        Err(ReplayEvidenceError::InvalidOutcome)
    );
}

#[test]
fn sections_115_116_replay_report_records_exact_mismatches_without_promotion_threshold() {
    let exact = record_replay_case(
        &manifest(),
        "CASE-EXACT",
        "SETUP-EXACT",
        "BT-001",
        ValidationPhase::Backtest,
        outcome(false),
        outcome(false),
    )
    .unwrap();
    let authorization_mismatch = record_replay_case(
        &manifest(),
        "CASE-AUTH",
        "SETUP-AUTH",
        "BT-001",
        ValidationPhase::Backtest,
        outcome(true),
        outcome(false),
    )
    .unwrap();
    let detail_mismatch = record_replay_case(
        &manifest(),
        "CASE-DETAIL",
        "SETUP-DETAIL",
        "BT-001",
        ValidationPhase::Backtest,
        ReplayOutcome {
            trade_authorized: false,
            rejection_code: None,
            setup_state: Some(SetupState::Terminal(TerminalState::Rejected)),
        },
        ReplayOutcome {
            trade_authorized: false,
            rejection_code: Some(RejectionCode::ProcessError),
            setup_state: Some(SetupState::Terminal(TerminalState::Expired)),
        },
    )
    .unwrap();

    let report = summarize_replay(
        ValidationPhase::Backtest,
        &[exact, authorization_mismatch, detail_mismatch],
    )
    .unwrap();
    assert_eq!(report.total_cases, 3);
    assert_eq!(report.exact_matches, 1);
    assert_eq!(report.mismatches, 2);
    assert_eq!(report.authorization_mismatches, 1);
    assert_eq!(report.rejection_mismatches, 1);
    assert_eq!(report.state_mismatches, 1);
}

#[test]
fn section_116_replay_report_rejects_duplicate_case_or_setup_evidence() {
    let first = record_replay_case(
        &manifest(),
        "CASE-1",
        "SETUP-1",
        "BT-001",
        ValidationPhase::Backtest,
        outcome(false),
        outcome(false),
    )
    .unwrap();
    let duplicate_case = record_replay_case(
        &manifest(),
        "CASE-1",
        "SETUP-2",
        "BT-001",
        ValidationPhase::Backtest,
        outcome(false),
        outcome(false),
    )
    .unwrap();
    assert_eq!(
        summarize_replay(ValidationPhase::Backtest, &[first.clone(), duplicate_case]),
        Err(ReplayEvidenceError::DuplicateCaseId)
    );

    let duplicate_setup = record_replay_case(
        &manifest(),
        "CASE-2",
        "SETUP-1",
        "BT-001",
        ValidationPhase::Backtest,
        outcome(false),
        outcome(false),
    )
    .unwrap();
    assert_eq!(
        summarize_replay(ValidationPhase::Backtest, &[first, duplicate_setup]),
        Err(ReplayEvidenceError::DuplicateSetupEvidence)
    );
}

#[test]
fn sections_10_56_95_108_109_122_replay_integrated_authorization_fail_closed_matrix() {
    let mut evidence = vec![record_authorization_case(
        "AUTH-ALL-TRUE",
        "SETUP-ALL-TRUE",
        all_true_conditions(),
        all_true_authority(),
        true,
    )];

    let mutations: [ConditionMutation; 10] = [
        ("TIME", |value| value.time_valid = Condition::False),
        ("DATA", |value| value.data_valid = Condition::Unknown),
        ("DIRECTION", |value| {
            value.direction_valid = Condition::False
        }),
        ("NEWS", |value| value.news_blackout_clear = Condition::False),
        ("PORTFOLIO", |value| {
            value.portfolio_risk_valid = Condition::False
        }),
        ("CORRELATION", |value| {
            value.correlation_risk_valid = Condition::Unknown
        }),
        ("DUPLICATE", |value| {
            value.setup_not_duplicated = Condition::False
        }),
        ("CONFLICT", |value| {
            value.no_conflicting_order = Condition::Unknown
        }),
        ("BROKER", |value| value.broker_safe = Condition::False),
        ("ENGINE", |value| {
            value.execution_engine_safe = Condition::False
        }),
    ];
    for (index, (name, mutate)) in mutations.into_iter().enumerate() {
        let mut conditions = all_true_conditions();
        mutate(&mut conditions);
        evidence.push(record_authorization_case(
            &format!("AUTH-{name}"),
            &format!("SETUP-{index}"),
            conditions,
            all_true_authority(),
            false,
        ));
    }

    let mut risk_authority = all_true_authority();
    risk_authority.risk_engine_pass = Condition::False;
    evidence.push(record_authorization_case(
        "AUTH-RISK-ENGINE",
        "SETUP-RISK-ENGINE",
        all_true_conditions(),
        risk_authority,
        false,
    ));

    let mut execution_authority = all_true_authority();
    execution_authority.execution_gate_pass = Condition::Unknown;
    evidence.push(record_authorization_case(
        "AUTH-EXECUTION-GATE",
        "SETUP-EXECUTION-GATE",
        all_true_conditions(),
        execution_authority,
        false,
    ));

    let report = summarize_replay(ValidationPhase::Backtest, &evidence).unwrap();
    assert_eq!(report.total_cases, evidence.len());
    assert_eq!(report.exact_matches, evidence.len());
    assert_eq!(report.mismatches, 0);
}

#[test]
fn sections_3_4_95_96_replay_boundaries_preserve_time_news_and_management_behavior() {
    let cutoff = SessionPermissions::at(EtTime::from_hms(11, 0, 0).unwrap());
    assert!(!cutoff.allow_new_entries);
    assert!(cutoff.allow_position_management);
    assert!(!cutoff.allow_position_adds);

    let windows = [NewsBlackoutWindow {
        start_inclusive: EtTime::from_hms(9, 55, 0).unwrap(),
        end_exclusive: EtTime::from_hms(10, 5, 0).unwrap(),
    }];
    let blackout = evaluate_news_gate(
        EtTime::from_hms(10, 0, 0).unwrap(),
        None,
        Condition::True,
        &windows,
    );
    assert_eq!(blackout.news_blackout_clear, Condition::False);
    assert!(!blackout.allow_new_entries);
    assert!(blackout.allow_position_management);
}

#[test]
fn sections_63_76_94_replay_management_checks_never_widen_stops() {
    assert_eq!(
        stop_replacement_allowed(Direction::Long, 99.0, 98.75),
        Condition::False
    );
    assert_eq!(
        stop_replacement_allowed(Direction::Short, 101.0, 101.25),
        Condition::False
    );
    assert_eq!(
        defensive_stop_candidate_allowed(Direction::Long, 99.0, 99.25, Condition::Unknown,),
        Condition::Unknown
    );
}

#[test]
fn sections_98_101_replay_operational_safety_starts_unknown_and_agent_violation_isolated() {
    let mut safety = OperationalSafetyController::new();
    assert_eq!(
        safety.agent_automation_allowed("AGENT-A"),
        Condition::Unknown
    );

    safety.set_operational_risk_clear(Condition::True);
    safety.observe_emergency_drawdown(Condition::False);
    assert_eq!(safety.agent_automation_allowed("AGENT-A"), Condition::True);
    assert_eq!(safety.agent_automation_allowed("AGENT-B"), Condition::True);

    safety
        .record_process_violation(ProcessViolationScope::Agent("AGENT-A".to_owned()))
        .unwrap();
    assert_eq!(safety.agent_automation_allowed("AGENT-A"), Condition::False);
    assert_eq!(safety.agent_automation_allowed("AGENT-B"), Condition::True);
}

#[test]
fn section_102_replay_state_sequence_still_rejects_skips_and_terminal_resume() {
    assert_eq!(
        SetupState::Created.advance(SetupState::LocationReached),
        Err(SetupStateError::StateSkip)
    );
    let terminal = SetupState::LocationReached.terminate(TerminalState::Invalidated);
    assert_eq!(
        terminal.advance(SetupState::AggressionPresent),
        Err(SetupStateError::AlreadyTerminal)
    );
}
