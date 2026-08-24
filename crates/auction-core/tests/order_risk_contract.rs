use auction_core::{
    ClusterExposure, Condition, Direction, FuturesRiskInputs, InstrumentExecutionConfig,
    OrderProposalInputs, OrderType, PortfolioRiskLimits, SetupState, STRICT_MIN_PLANNED_R,
    build_order_proposal, cluster_direction_clear, entry_limit_valid, entry_trigger,
    evaluate_portfolio_risk, planned_r, size_futures, slippage_guard, stop_replacement_allowed,
    structural_stop, structural_target_valid, trigger_expired,
};

fn config() -> InstrumentExecutionConfig {
    InstrumentExecutionConfig {
        tick_size: 0.25,
        max_entry_slippage_ticks: 4,
        stop_buffer_ticks: 2,
    }
}

fn limits() -> PortfolioRiskLimits {
    PortfolioRiskLimits {
        max_portfolio_open_risk_percent: 0.02,
        max_cluster_open_risk_percent: 0.01,
    }
}

fn assert_close(actual: f64, expected: f64) {
    assert!((actual - expected).abs() < 1e-9, "{actual} != {expected}");
}

#[test]
fn sections_59_60_long_and_mirrored_short_trigger_and_slippage_boundaries() {
    let cfg = config();
    let long_trigger = entry_trigger(Direction::Long, 100.0, cfg.tick_size).unwrap();
    let short_trigger = entry_trigger(Direction::Short, 100.0, cfg.tick_size).unwrap();
    assert_close(long_trigger, 100.25);
    assert_close(short_trigger, 99.75);

    assert_eq!(
        entry_limit_valid(Direction::Long, long_trigger, 101.25, cfg),
        Condition::True
    );
    assert_eq!(
        entry_limit_valid(Direction::Long, long_trigger, 101.250_001, cfg),
        Condition::False
    );
    assert_eq!(
        entry_limit_valid(Direction::Long, long_trigger, 100.0, cfg),
        Condition::False
    );

    assert_eq!(
        entry_limit_valid(Direction::Short, short_trigger, 98.75, cfg),
        Condition::True
    );
    assert_eq!(
        entry_limit_valid(Direction::Short, short_trigger, 98.749_999, cfg),
        Condition::False
    );
    assert_eq!(
        entry_limit_valid(Direction::Short, short_trigger, 100.0, cfg),
        Condition::False
    );

    assert_eq!(
        slippage_guard(Direction::Long, long_trigger, 101.25, cfg),
        Condition::True
    );
    assert_eq!(
        slippage_guard(Direction::Long, long_trigger, 101.250_001, cfg),
        Condition::False
    );
    assert_eq!(
        slippage_guard(Direction::Short, short_trigger, 98.75, cfg),
        Condition::True
    );
    assert_eq!(
        slippage_guard(Direction::Short, short_trigger, 98.749_999, cfg),
        Condition::False
    );
}

#[test]
fn section_60_requires_config_but_does_not_invent_a_positive_slippage_minimum() {
    let zero_slippage = InstrumentExecutionConfig {
        tick_size: 0.25,
        max_entry_slippage_ticks: 0,
        stop_buffer_ticks: 1,
    };
    assert_eq!(
        entry_limit_valid(Direction::Long, 100.0, 100.0, zero_slippage),
        Condition::True
    );
    assert_eq!(
        entry_limit_valid(Direction::Long, 100.0, 100.25, zero_slippage),
        Condition::False
    );

    let invalid = InstrumentExecutionConfig {
        tick_size: 0.0,
        max_entry_slippage_ticks: 4,
        stop_buffer_ticks: 2,
    };
    assert_eq!(
        entry_limit_valid(Direction::Long, 100.0, 100.0, invalid),
        Condition::Unknown
    );
    assert!(structural_stop(Direction::Long, 99.0, invalid).is_none());
}

#[test]
fn section_61_expires_after_two_completed_five_minute_candles() {
    assert!(!trigger_expired(0));
    assert!(!trigger_expired(1));
    assert!(trigger_expired(2));
    assert!(trigger_expired(3));
}

#[test]
fn sections_62_63_93_structural_stops_and_no_widening_are_directional() {
    let cfg = config();
    assert_close(
        structural_stop(Direction::Long, 99.0, cfg).unwrap(),
        98.5,
    );
    assert_close(
        structural_stop(Direction::Short, 101.0, cfg).unwrap(),
        101.5,
    );

    assert_eq!(
        stop_replacement_allowed(Direction::Long, 98.5, 98.5),
        Condition::True
    );
    assert_eq!(
        stop_replacement_allowed(Direction::Long, 98.5, 99.0),
        Condition::True
    );
    assert_eq!(
        stop_replacement_allowed(Direction::Long, 98.5, 98.25),
        Condition::False
    );

    assert_eq!(
        stop_replacement_allowed(Direction::Short, 101.5, 101.5),
        Condition::True
    );
    assert_eq!(
        stop_replacement_allowed(Direction::Short, 101.5, 101.0),
        Condition::True
    );
    assert_eq!(
        stop_replacement_allowed(Direction::Short, 101.5, 101.75),
        Condition::False
    );
}

#[test]
fn sections_65_66_size_futures_uses_configured_risk_and_floors_without_moving_stop() {
    let sized = size_futures(FuturesRiskInputs {
        account_equity: 100_000.0,
        risk_percent: 0.0025,
        entry: 100.0,
        stop: 99.0,
        tick_size: 0.25,
        tick_value: 1.25,
        commissions_per_contract: 2.0,
        slippage_reserve_per_contract: 3.0,
    })
    .unwrap();

    assert_close(sized.risk_budget_dollars, 250.0);
    assert_close(sized.stop_ticks, 4.0);
    assert_close(sized.risk_per_contract, 10.0);
    assert_eq!(sized.size, 25);
    assert_close(sized.proposed_open_risk_dollars, 250.0);

    let floored = size_futures(FuturesRiskInputs {
        account_equity: 99_600.0,
        risk_percent: 0.0025,
        entry: 100.0,
        stop: 99.0,
        tick_size: 0.25,
        tick_value: 1.25,
        commissions_per_contract: 2.0,
        slippage_reserve_per_contract: 3.0,
    })
    .unwrap();
    assert_close(floored.risk_budget_dollars, 249.0);
    assert_eq!(floored.size, 24);
    assert_close(floored.proposed_open_risk_dollars, 240.0);

    assert!(
        size_futures(FuturesRiskInputs {
            account_equity: 1_000.0,
            risk_percent: 0.001,
            entry: 100.0,
            stop: 99.0,
            tick_size: 0.25,
            tick_value: 1.25,
            commissions_per_contract: 2.0,
            slippage_reserve_per_contract: 3.0,
        })
        .is_none()
    );
}

#[test]
fn section_66_invalid_or_unknown_sizing_inputs_fail_closed() {
    let base = FuturesRiskInputs {
        account_equity: 100_000.0,
        risk_percent: 0.0025,
        entry: 100.0,
        stop: 99.0,
        tick_size: 0.25,
        tick_value: 1.25,
        commissions_per_contract: 1.0,
        slippage_reserve_per_contract: 1.0,
    };

    assert!(size_futures(FuturesRiskInputs { risk_percent: 0.0, ..base }).is_none());
    assert!(size_futures(FuturesRiskInputs { entry: f64::NAN, ..base }).is_none());
    assert!(size_futures(FuturesRiskInputs { tick_size: 0.0, ..base }).is_none());
    assert!(
        size_futures(FuturesRiskInputs {
            commissions_per_contract: -0.01,
            ..base
        })
        .is_none()
    );
}

#[test]
fn sections_67_69_portfolio_and_cluster_limits_are_inclusive_at_exact_ceiling() {
    let exact = evaluate_portfolio_risk(
        100_000.0,
        1_500.0,
        500.0,
        500.0,
        PortfolioRiskLimits {
            max_portfolio_open_risk_percent: 0.02,
            max_cluster_open_risk_percent: 0.01,
        },
    );
    assert_close(exact.open_portfolio_risk_after, 2_000.0);
    assert_close(exact.cluster_risk_after, 1_000.0);
    assert_eq!(exact.portfolio_pass, Condition::True);
    assert_eq!(exact.cluster_pass, Condition::True);

    let over = evaluate_portfolio_risk(
        100_000.0,
        1_500.0,
        500.0,
        500.01,
        PortfolioRiskLimits {
            max_portfolio_open_risk_percent: 0.02,
            max_cluster_open_risk_percent: 0.01,
        },
    );
    assert_eq!(over.portfolio_pass, Condition::False);
    assert_eq!(over.cluster_pass, Condition::False);

    let unknown = evaluate_portfolio_risk(
        100_000.0,
        f64::NAN,
        0.0,
        100.0,
        limits(),
    );
    assert_eq!(unknown.portfolio_pass, Condition::Unknown);
    assert_eq!(unknown.cluster_pass, Condition::Unknown);
}

#[test]
fn sections_68_70_same_cluster_opposing_direction_fails_closed() {
    let existing = [ClusterExposure {
        cluster_id: "NASDAQ_CLUSTER",
        direction: Direction::Long,
    }];

    assert_eq!(
        cluster_direction_clear("NASDAQ_CLUSTER", Direction::Short, &existing),
        Condition::False
    );
    assert_eq!(
        cluster_direction_clear("NASDAQ_CLUSTER", Direction::Long, &existing),
        Condition::True
    );
    assert_eq!(
        cluster_direction_clear("SP500_CLUSTER", Direction::Short, &existing),
        Condition::True
    );
    assert_eq!(
        cluster_direction_clear("", Direction::Long, &existing),
        Condition::Unknown
    );

    let invalid_existing = [ClusterExposure {
        cluster_id: " ",
        direction: Direction::Long,
    }];
    assert_eq!(
        cluster_direction_clear("NASDAQ_CLUSTER", Direction::Long, &invalid_existing),
        Condition::Unknown
    );
}

#[test]
fn sections_71_72_94_enforce_strict_one_point_five_r_for_both_directions() {
    assert_close(STRICT_MIN_PLANNED_R, 1.5);
    assert_close(
        planned_r(Direction::Long, 100.0, 99.0, 101.5).unwrap(),
        1.5,
    );
    assert_eq!(
        structural_target_valid(Direction::Long, 100.0, 99.0, 101.5),
        Condition::True
    );
    assert_eq!(
        structural_target_valid(Direction::Long, 100.0, 99.0, 101.499),
        Condition::False
    );

    assert_close(
        planned_r(Direction::Short, 100.0, 101.0, 98.5).unwrap(),
        1.5,
    );
    assert_eq!(
        structural_target_valid(Direction::Short, 100.0, 101.0, 98.5),
        Condition::True
    );
    assert_eq!(
        structural_target_valid(Direction::Short, 100.0, 101.0, 98.501),
        Condition::False
    );

    assert!(planned_r(Direction::Long, 100.0, 101.0, 102.0).is_none());
    assert_eq!(
        structural_target_valid(Direction::Short, 100.0, 99.0, 98.0),
        Condition::Unknown
    );
}

fn valid_long_inputs<'a>(existing: &'a [ClusterExposure<'a>]) -> OrderProposalInputs<'a> {
    OrderProposalInputs {
        setup_id: "SETUP-12-LONG",
        instrument: "TEST_FUTURE",
        setup_state: SetupState::FinalReconfirmation,
        direction: Direction::Long,
        reconfirmation_extreme: 100.0,
        entry_limit: 100.5,
        first_failure_extreme: 99.0,
        target: 103.0,
        execution_config: config(),
        account_equity: 100_000.0,
        risk_percent: 0.0025,
        tick_value: 1.25,
        commissions_per_contract: 1.0,
        slippage_reserve_per_contract: 1.0,
        current_open_portfolio_risk: 100.0,
        current_cluster_risk: 50.0,
        portfolio_limits: limits(),
        cluster_id: "NASDAQ_CLUSTER",
        existing_cluster_exposure: existing,
        strategy_pass: Condition::True,
    }
}

#[test]
fn sections_109_110_valid_prebroker_proposal_matches_schema_and_stays_non_executable() {
    let proposal = build_order_proposal(valid_long_inputs(&[])).unwrap();

    assert_eq!(proposal.setup_id, "SETUP-12-LONG");
    assert_eq!(proposal.instrument, "TEST_FUTURE");
    assert_eq!(proposal.side, Direction::Long);
    assert_eq!(proposal.order_type, OrderType::StopLimit);
    assert_close(proposal.entry_trigger, 100.25);
    assert_close(proposal.entry_limit, 100.5);
    assert_close(proposal.stop, 98.5);
    assert_close(proposal.target, 103.0);
    assert!(proposal.size >= 1);
    assert_close(proposal.risk_dollars, 250.0);
    assert_close(proposal.risk_percent, 0.0025);
    assert_close(proposal.open_portfolio_risk_before, 100.0);
    assert!(proposal.open_portfolio_risk_after > proposal.open_portfolio_risk_before);
    assert_eq!(proposal.cluster_id, "NASDAQ_CLUSTER");
    assert!(proposal.cluster_risk_after > 50.0);
    assert_eq!(proposal.strategy_pass, Condition::True);
    assert_eq!(proposal.risk_pass, Condition::True);
    assert_eq!(proposal.portfolio_pass, Condition::True);
    assert_eq!(proposal.execution_pass, Condition::Unknown);
}

#[test]
fn section_110_proposal_fails_closed_on_state_strategy_target_risk_portfolio_or_correlation() {
    let no_exposure: [ClusterExposure<'_>; 0] = [];

    let mut inputs = valid_long_inputs(&no_exposure);
    inputs.setup_state = SetupState::SecondFailure;
    assert!(build_order_proposal(inputs).is_none());

    let mut inputs = valid_long_inputs(&no_exposure);
    inputs.strategy_pass = Condition::Unknown;
    assert!(build_order_proposal(inputs).is_none());

    let mut inputs = valid_long_inputs(&no_exposure);
    inputs.target = 102.0;
    assert!(build_order_proposal(inputs).is_none());

    let mut inputs = valid_long_inputs(&no_exposure);
    inputs.risk_percent = 0.0;
    assert!(build_order_proposal(inputs).is_none());

    let mut inputs = valid_long_inputs(&no_exposure);
    inputs.current_open_portfolio_risk = 1_950.0;
    assert!(build_order_proposal(inputs).is_none());

    let opposing = [ClusterExposure {
        cluster_id: "NASDAQ_CLUSTER",
        direction: Direction::Short,
    }];
    assert!(build_order_proposal(valid_long_inputs(&opposing)).is_none());
}

#[test]
fn section_110_proposal_rejects_invalid_execution_configuration_or_limit_price() {
    let no_exposure: [ClusterExposure<'_>; 0] = [];

    let mut inputs = valid_long_inputs(&no_exposure);
    inputs.execution_config.tick_size = 0.0;
    assert!(build_order_proposal(inputs).is_none());

    let mut inputs = valid_long_inputs(&no_exposure);
    inputs.entry_limit = 101.500_001;
    assert!(build_order_proposal(inputs).is_none());
}

#[test]
fn sections_85_93_94_build_mirrored_short_proposal_without_execution_authority() {
    let inputs = OrderProposalInputs {
        setup_id: "SETUP-12-SHORT",
        instrument: "TEST_FUTURE",
        setup_state: SetupState::FinalReconfirmation,
        direction: Direction::Short,
        reconfirmation_extreme: 100.0,
        entry_limit: 99.5,
        first_failure_extreme: 101.0,
        target: 97.0,
        execution_config: config(),
        account_equity: 100_000.0,
        risk_percent: 0.0025,
        tick_value: 1.25,
        commissions_per_contract: 1.0,
        slippage_reserve_per_contract: 1.0,
        current_open_portfolio_risk: 100.0,
        current_cluster_risk: 50.0,
        portfolio_limits: limits(),
        cluster_id: "NASDAQ_CLUSTER",
        existing_cluster_exposure: &[],
        strategy_pass: Condition::True,
    };

    let proposal = build_order_proposal(inputs).unwrap();
    assert_eq!(proposal.side, Direction::Short);
    assert_close(proposal.entry_trigger, 99.75);
    assert_close(proposal.entry_limit, 99.5);
    assert_close(proposal.stop, 101.5);
    assert_eq!(proposal.execution_pass, Condition::Unknown);
}
