use auction_core::{
    AuthorizationInputs, Condition, Direction, EtTime, FibLevels, MarketState,
    ProductionConditions, RejectionCode, SessionPermissions, SetupState, SetupStateError,
    TerminalState, bullish_fib, direction_allowed, long_location_reached, long_location_valid,
    short_location_reached, short_location_valid, trade_authorized,
};

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

#[test]
fn section_108_unknown_fails_closed() {
    assert!(!Condition::Unknown.permits());
    assert!(!Condition::False.permits());
    assert!(Condition::True.permits());
}

#[test]
fn sections_3_4_enforce_exact_entry_window() {
    let before = SessionPermissions::at(EtTime::from_hms(9, 29, 59).unwrap());
    let open = SessionPermissions::at(EtTime::from_hms(9, 30, 0).unwrap());
    let last = SessionPermissions::at(EtTime::from_hms(10, 59, 59).unwrap());
    let cutoff = SessionPermissions::at(EtTime::from_hms(11, 0, 0).unwrap());

    assert!(!before.allow_new_entries);
    assert!(open.allow_new_entries);
    assert!(last.allow_new_entries);
    assert!(!cutoff.allow_new_entries);
    assert!(cutoff.allow_position_management);
    assert!(!cutoff.allow_position_adds);
}

#[test]
fn section_24_direction_permission_is_fail_closed() {
    assert!(direction_allowed(MarketState::ValueUp, Direction::Long));
    assert!(direction_allowed(MarketState::ValueDown, Direction::Short));
    assert!(!direction_allowed(MarketState::ValueUp, Direction::Short));
    assert!(!direction_allowed(MarketState::ValueDown, Direction::Long));
    assert!(!direction_allowed(MarketState::Balanced, Direction::Long));
    assert!(!direction_allowed(MarketState::Unclear, Direction::Short));
}

#[test]
fn sections_29_30_use_exact_source_fibonacci_levels() {
    let fib = bullish_fib(80.0, 100.0).unwrap();
    assert!((fib.level_705 - 85.9).abs() < 1e-12);
    assert!((fib.level_788 - 84.24).abs() < 1e-12);
    assert!((fib.level_886 - 82.28).abs() < 1e-12);
}

#[test]
fn sections_31_32_require_entire_zone_outside_value() {
    let long = FibLevels {
        level_705: 90.0,
        level_788: 88.0,
        level_886: 86.0,
    };
    assert!(long_location_valid(long, 90.01));
    assert!(!long_location_valid(long, 90.0));

    let short = FibLevels {
        level_705: 110.0,
        level_788: 112.0,
        level_886: 114.0,
    };
    assert!(short_location_valid(short, 109.99));
    assert!(!short_location_valid(short, 110.0));
}

#[test]
fn section_35_location_boundaries_are_inclusive() {
    let long = FibLevels {
        level_705: 90.0,
        level_788: 88.0,
        level_886: 86.0,
    };
    assert!(long_location_reached(long, 90.0));
    assert!(long_location_reached(long, 86.0));
    assert!(!long_location_reached(long, 90.01));
    assert!(!long_location_reached(long, 85.99));

    let short = FibLevels {
        level_705: 110.0,
        level_788: 112.0,
        level_886: 114.0,
    };
    assert!(short_location_reached(short, 110.0));
    assert!(short_location_reached(short, 114.0));
    assert!(!short_location_reached(short, 109.99));
    assert!(!short_location_reached(short, 114.01));
}

#[test]
fn section_102_disallows_state_skipping() {
    assert_eq!(
        SetupState::Created.advance(SetupState::LocationReached),
        Err(SetupStateError::StateSkip)
    );
    assert_eq!(
        SetupState::Created.advance(SetupState::WaitingForLocation),
        Ok(SetupState::WaitingForLocation)
    );
}

#[test]
fn section_102_terminal_state_cannot_resume() {
    let terminal = SetupState::LocationReached.terminate(TerminalState::Invalidated);
    assert_eq!(
        terminal.advance(SetupState::AggressionPresent),
        Err(SetupStateError::AlreadyTerminal)
    );
}

#[test]
fn sections_10_56_95_108_109_122_require_every_gate() {
    assert!(trade_authorized(
        all_true_conditions(),
        all_true_authority()
    ));

    let mut conditions = all_true_conditions();
    conditions.broker_safe = Condition::Unknown;
    assert!(!trade_authorized(conditions, all_true_authority()));

    let mut news = all_true_conditions();
    news.news_blackout_clear = Condition::False;
    assert!(!trade_authorized(news, all_true_authority()));

    let mut conflict = all_true_conditions();
    conflict.no_conflicting_order = Condition::Unknown;
    assert!(!trade_authorized(conflict, all_true_authority()));

    let mut authority = all_true_authority();
    authority.risk_engine_pass = Condition::False;
    assert!(!trade_authorized(all_true_conditions(), authority));
}

#[test]
fn section_113_rejection_codes_are_stable() {
    assert_eq!(RejectionCode::DataInvalid.code(), "R01");
    assert_eq!(RejectionCode::Fib886Invalidation.code(), "R06");
    assert_eq!(RejectionCode::DuplicateSetup.code(), "R20");
    assert_eq!(RejectionCode::BrokerUnsafe.code(), "R25");
}
