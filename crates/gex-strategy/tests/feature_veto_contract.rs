use gex_strategy::{
    AiNativeFeatureInput, AiNativeFeatureSet, CandleEventFeatures, DataQuality, DecisionVerdict,
    EmaStackError, EmaStackState, FeatureSetError, HardCondition, HardConditionKind, HardVetoError,
    HardVetoInput, HardVetoVerdict, LiquidityExecutionFeatures, MarketSnapshot,
    MarketSnapshotInput, ParticipationFeatures, PortfolioBrokerFeatures, Session, StructureFeatures,
    TrendFeatures, classify_ema_stack, evaluate_hard_vetoes,
};

fn snapshot(id: &str) -> MarketSnapshot {
    MarketSnapshot::build(
        MarketSnapshotInput {
            snapshot_id: id.to_owned(),
            symbol: "SPY".to_owned(),
            source_timestamp_millis: 1_000,
            received_timestamp_millis: 1_010,
            spot_price: 600.0,
            session: Session::Regular,
            data_quality: DataQuality::Structured,
        },
        &[],
    )
    .unwrap()
}

fn feature_input(id: &str) -> AiNativeFeatureInput {
    AiNativeFeatureInput {
        snapshot_id: id.to_owned(),
        trend: TrendFeatures::default(),
        structure: StructureFeatures::default(),
        candle_event: CandleEventFeatures::default(),
        participation: ParticipationFeatures::default(),
        liquidity_execution: LiquidityExecutionFeatures::default(),
        portfolio_broker: PortfolioBrokerFeatures::default(),
    }
}

fn all_pass(id: &str) -> HardVetoInput {
    HardVetoInput {
        snapshot_id: id.to_owned(),
        data_valid: HardCondition::Pass,
        data_fresh: HardCondition::Pass,
        gex_scope_valid: HardCondition::Pass,
        required_regime_data_valid: HardCondition::Pass,
        broker_connected: HardCondition::Pass,
        order_state_valid: HardCondition::Pass,
        instrument_tradable: HardCondition::Pass,
        liquidity_within_limits: HardCondition::Pass,
        max_position_risk_valid: HardCondition::Pass,
        max_portfolio_risk_valid: HardCondition::Pass,
        invalidation_defined: HardCondition::Pass,
        execution_cost_within_limits: HardCondition::Pass,
        kill_switch_clear: HardCondition::Pass,
    }
}

#[test]
fn feature_set_is_bound_to_the_same_snapshot() {
    let snapshot = snapshot("snapshot-1");

    assert_eq!(
        AiNativeFeatureSet::build(&snapshot, feature_input("")),
        Err(FeatureSetError::MissingSnapshotId)
    );
    assert_eq!(
        AiNativeFeatureSet::build(&snapshot, feature_input("snapshot-2")),
        Err(FeatureSetError::SnapshotMismatch)
    );

    let built = AiNativeFeatureSet::build(&snapshot, feature_input("snapshot-1")).unwrap();
    assert_eq!(built.snapshot_id(), "snapshot-1");
}

#[test]
fn missing_features_remain_unknown_instead_of_zero_filled() {
    let snapshot = snapshot("snapshot-1");
    let built = AiNativeFeatureSet::build(&snapshot, feature_input("snapshot-1")).unwrap();

    assert_eq!(built.trend().trend_strength, None);
    assert_eq!(built.structure().impulse_strength, None);
    assert_eq!(built.candle_event().rejection_strength, None);
    assert_eq!(built.participation().relative_volume, None);
    assert_eq!(built.liquidity_execution().expected_slippage, None);
    assert_eq!(built.portfolio_broker().gross_exposure, None);
}

#[test]
fn nonfinite_feature_values_fail_structural_validation() {
    let snapshot = snapshot("snapshot-1");

    let mut trend = feature_input("snapshot-1");
    trend.trend.trend_strength = Some(f64::NAN);
    assert_eq!(
        AiNativeFeatureSet::build(&snapshot, trend),
        Err(FeatureSetError::NonFiniteFeature)
    );

    let mut structure = feature_input("snapshot-1");
    structure.structure.breakout_pressure = Some(f64::INFINITY);
    assert_eq!(
        AiNativeFeatureSet::build(&snapshot, structure),
        Err(FeatureSetError::NonFiniteFeature)
    );

    let mut candle = feature_input("snapshot-1");
    candle.candle_event.trade_velocity = Some(f64::NEG_INFINITY);
    assert_eq!(
        AiNativeFeatureSet::build(&snapshot, candle),
        Err(FeatureSetError::NonFiniteFeature)
    );

    let mut participation = feature_input("snapshot-1");
    participation.participation.relative_volume = Some(f64::NAN);
    assert_eq!(
        AiNativeFeatureSet::build(&snapshot, participation),
        Err(FeatureSetError::NonFiniteFeature)
    );

    let mut liquidity = feature_input("snapshot-1");
    liquidity.liquidity_execution.expected_slippage = Some(f64::INFINITY);
    assert_eq!(
        AiNativeFeatureSet::build(&snapshot, liquidity),
        Err(FeatureSetError::NonFiniteFeature)
    );

    let mut portfolio = feature_input("snapshot-1");
    portfolio.portfolio_broker.open_order_risk = Some(f64::NAN);
    assert_eq!(
        AiNativeFeatureSet::build(&snapshot, portfolio),
        Err(FeatureSetError::NonFiniteFeature)
    );
}

#[test]
fn finite_feature_values_are_preserved_without_hidden_normalization() {
    let snapshot = snapshot("snapshot-1");
    let mut input = feature_input("snapshot-1");
    input.trend.trend_direction = Some(-2.25);
    input.structure.range_contraction = Some(12.5);
    input.participation.relative_volume = Some(3.75);

    let built = AiNativeFeatureSet::build(&snapshot, input).unwrap();

    assert_eq!(built.trend().trend_direction, Some(-2.25));
    assert_eq!(built.structure().range_contraction, Some(12.5));
    assert_eq!(built.participation().relative_volume, Some(3.75));
}

#[test]
fn ema_stack_keeps_source_baseline_as_metadata_not_a_veto() {
    assert_eq!(
        classify_ema_stack(Some(9.0), Some(8.0), Some(7.0)),
        Ok(EmaStackState::Bullish)
    );
    assert_eq!(
        classify_ema_stack(Some(7.0), Some(8.0), Some(9.0)),
        Ok(EmaStackState::Bearish)
    );
    assert_eq!(
        classify_ema_stack(Some(9.0), Some(7.0), Some(8.0)),
        Ok(EmaStackState::Mixed)
    );
    assert_eq!(
        classify_ema_stack(None, Some(8.0), Some(7.0)),
        Ok(EmaStackState::Unknown)
    );
    assert_eq!(
        classify_ema_stack(Some(f64::NAN), Some(8.0), Some(7.0)),
        Err(EmaStackError::NonFiniteValue)
    );
}

#[test]
fn every_mandatory_hard_condition_can_reject() {
    let snapshot = snapshot("snapshot-1");
    let cases: &[(HardConditionKind, fn(&mut HardVetoInput))] = &[
        (HardConditionKind::DataValid, |input| input.data_valid = HardCondition::Fail),
        (HardConditionKind::DataFresh, |input| input.data_fresh = HardCondition::Fail),
        (HardConditionKind::GexScopeValid, |input| input.gex_scope_valid = HardCondition::Fail),
        (HardConditionKind::RequiredRegimeDataValid, |input| {
            input.required_regime_data_valid = HardCondition::Fail;
        }),
        (HardConditionKind::BrokerConnected, |input| {
            input.broker_connected = HardCondition::Fail;
        }),
        (HardConditionKind::OrderStateValid, |input| {
            input.order_state_valid = HardCondition::Fail;
        }),
        (HardConditionKind::InstrumentTradable, |input| {
            input.instrument_tradable = HardCondition::Fail;
        }),
        (HardConditionKind::LiquidityWithinLimits, |input| {
            input.liquidity_within_limits = HardCondition::Fail;
        }),
        (HardConditionKind::MaxPositionRiskValid, |input| {
            input.max_position_risk_valid = HardCondition::Fail;
        }),
        (HardConditionKind::MaxPortfolioRiskValid, |input| {
            input.max_portfolio_risk_valid = HardCondition::Fail;
        }),
        (HardConditionKind::InvalidationDefined, |input| {
            input.invalidation_defined = HardCondition::Fail;
        }),
        (HardConditionKind::ExecutionCostWithinLimits, |input| {
            input.execution_cost_within_limits = HardCondition::Fail;
        }),
        (HardConditionKind::KillSwitchClear, |input| {
            input.kill_switch_clear = HardCondition::Fail;
        }),
    ];

    for (expected_kind, fail) in cases {
        let mut input = all_pass("snapshot-1");
        fail(&mut input);
        let result = evaluate_hard_vetoes(&snapshot, &input).unwrap();
        assert_eq!(result.verdict(), HardVetoVerdict::Reject);
        assert_eq!(result.failed_conditions(), &[*expected_kind]);
        assert_eq!(result.blocking_decision(), Some(DecisionVerdict::Reject));
    }
}

#[test]
fn unknown_hard_condition_waits_when_no_failure_exists() {
    let snapshot = snapshot("snapshot-1");
    let mut input = all_pass("snapshot-1");
    input.liquidity_within_limits = HardCondition::Unknown;

    let result = evaluate_hard_vetoes(&snapshot, &input).unwrap();

    assert_eq!(result.verdict(), HardVetoVerdict::Wait);
    assert!(result.failed_conditions().is_empty());
    assert_eq!(
        result.unknown_conditions(),
        &[HardConditionKind::LiquidityWithinLimits]
    );
    assert_eq!(result.blocking_decision(), Some(DecisionVerdict::Wait));
}

#[test]
fn known_hard_failure_precedes_unknown() {
    let snapshot = snapshot("snapshot-1");
    let mut input = all_pass("snapshot-1");
    input.data_fresh = HardCondition::Unknown;
    input.max_portfolio_risk_valid = HardCondition::Fail;

    let result = evaluate_hard_vetoes(&snapshot, &input).unwrap();

    assert_eq!(result.verdict(), HardVetoVerdict::Reject);
    assert_eq!(
        result.failed_conditions(),
        &[HardConditionKind::MaxPortfolioRiskValid]
    );
    assert_eq!(
        result.unknown_conditions(),
        &[HardConditionKind::DataFresh]
    );
    assert_eq!(result.blocking_decision(), Some(DecisionVerdict::Reject));
}

#[test]
fn clearing_hard_vetoes_is_not_trade_authorization() {
    let snapshot = snapshot("snapshot-1");
    let input = all_pass("snapshot-1");

    let result = evaluate_hard_vetoes(&snapshot, &input).unwrap();

    assert_eq!(result.verdict(), HardVetoVerdict::Cleared);
    assert_eq!(result.blocking_decision(), None);
    assert!(result.failed_conditions().is_empty());
    assert!(result.unknown_conditions().is_empty());
}

#[test]
fn hard_veto_evaluation_fails_closed_on_snapshot_identity_errors() {
    let snapshot = snapshot("snapshot-1");

    assert_eq!(
        evaluate_hard_vetoes(&snapshot, &all_pass("")),
        Err(HardVetoError::MissingSnapshotId)
    );
    assert_eq!(
        evaluate_hard_vetoes(&snapshot, &all_pass("snapshot-2")),
        Err(HardVetoError::SnapshotMismatch)
    );
}
