use std::time::Duration;

use auction_core::{
    AuditError, AuditLedger, CompletedTradeRecord, Condition, Direction, EffortResultChange,
    EffortResultSnapshot, GammaRegime, MarketState, RejectedSetupRecord, RejectionCode,
};

fn trade(setup_id: &str, realized_r: f64) -> CompletedTradeRecord {
    CompletedTradeRecord {
        setup_id: setup_id.to_owned(),
        instrument: "MNQ".to_owned(),
        direction: Direction::Long,
        environment: MarketState::ValueUp,
        gamma_regime: GammaRegime::Positive,
        entry: 100.0,
        stop: 98.0,
        target: 103.0,
        size: 2,
        realized_r,
        mfe: 1.5,
        mae: 0.5,
        slippage: 0.25,
        time_in_trade: Duration::from_secs(900),
        value_reclaim_outcome: Condition::True,
        effort_vs_result_changes: vec![EffortResultChange {
            from: EffortResultSnapshot {
                favorable_effort_successful: Condition::False,
                favorable_effort_failure: Condition::False,
            },
            to: EffortResultSnapshot {
                favorable_effort_successful: Condition::True,
                favorable_effort_failure: Condition::False,
            },
        }],
        exit_reason: "FINAL_SWING_TARGET".to_owned(),
    }
}

fn approx_eq(left: f64, right: f64) {
    assert!((left - right).abs() < 1e-12, "left={left}, right={right}");
}

#[test]
fn section_111_completed_trade_preserves_every_required_audit_field() {
    let mut ledger = AuditLedger::new();
    let record = trade("SETUP-111", 1.25);
    ledger.record_completed_trade(record.clone()).unwrap();

    assert_eq!(ledger.completed_trades(), &[record]);
    let stored = &ledger.completed_trades()[0];
    assert_eq!(stored.setup_id, "SETUP-111");
    assert_eq!(stored.instrument, "MNQ");
    assert_eq!(stored.direction, Direction::Long);
    assert_eq!(stored.environment, MarketState::ValueUp);
    assert_eq!(stored.gamma_regime, GammaRegime::Positive);
    assert_eq!(stored.entry, 100.0);
    assert_eq!(stored.stop, 98.0);
    assert_eq!(stored.target, 103.0);
    assert_eq!(stored.size, 2);
    assert_eq!(stored.realized_r, 1.25);
    assert_eq!(stored.mfe, 1.5);
    assert_eq!(stored.mae, 0.5);
    assert_eq!(stored.slippage, 0.25);
    assert_eq!(stored.time_in_trade, Duration::from_secs(900));
    assert_eq!(stored.value_reclaim_outcome, Condition::True);
    assert_eq!(stored.effort_vs_result_changes.len(), 1);
    assert_eq!(stored.exit_reason, "FINAL_SWING_TARGET");
}

#[test]
fn audit_validation_does_not_erase_structurally_valid_strategy_violations() {
    let mut ledger = AuditLedger::new();
    let mut violating = trade("SETUP-BAD", -1.0);
    violating.entry = 100.0;
    violating.stop = 100.0;
    violating.target = 99.0;
    violating.value_reclaim_outcome = Condition::False;
    violating.exit_reason = "PROCESS_VIOLATION".to_owned();

    assert!(violating.structurally_valid());
    ledger.record_completed_trade(violating).unwrap();
    assert_eq!(ledger.daily_metrics().trades_executed, 1);
}

#[test]
fn invalid_or_nonfinite_completed_trade_is_rejected_without_mutating_ledger() {
    for mutation in 0..5 {
        let mut ledger = AuditLedger::new();
        let mut invalid = trade("SETUP-INVALID", 1.0);
        match mutation {
            0 => invalid.setup_id = "   ".to_owned(),
            1 => invalid.instrument.clear(),
            2 => invalid.realized_r = f64::NAN,
            3 => invalid.slippage = f64::INFINITY,
            4 => invalid.size = 0,
            _ => unreachable!(),
        }
        assert_eq!(
            ledger.record_completed_trade(invalid),
            Err(AuditError::InvalidRecord)
        );
        assert!(ledger.completed_trades().is_empty());
        assert_eq!(ledger.daily_metrics().trades_executed, 0);
    }
}

#[test]
fn sections_112_113_rejections_preserve_fixed_code_and_terminal_setup_uniqueness() {
    let mut ledger = AuditLedger::new();
    ledger
        .record_rejected_setup(RejectedSetupRecord {
            setup_id: "SETUP-R20",
            rejection_code: RejectionCode::DuplicateSetup,
        })
        .unwrap();

    assert_eq!(ledger.rejected_setups().len(), 1);
    assert_eq!(ledger.rejected_setups()[0].setup_id, "SETUP-R20");
    assert_eq!(
        ledger.rejected_setups()[0].rejection_code,
        RejectionCode::DuplicateSetup
    );
    assert_eq!(ledger.rejected_setups()[0].rejection_code.code(), "R20");

    assert_eq!(
        ledger.record_rejected_setup(RejectedSetupRecord {
            setup_id: "SETUP-R20",
            rejection_code: RejectionCode::BrokerUnsafe,
        }),
        Err(AuditError::DuplicateRecord)
    );
    assert_eq!(
        ledger.record_completed_trade(trade("SETUP-R20", 1.0)),
        Err(AuditError::DuplicateRecord)
    );
}

#[test]
fn qualified_setup_count_is_explicit_and_duplicate_qualification_is_not_double_counted() {
    let mut ledger = AuditLedger::new();
    ledger.record_qualified_setup("SETUP-Q1").unwrap();
    ledger.record_qualified_setup("SETUP-Q2").unwrap();
    assert_eq!(
        ledger.record_qualified_setup("SETUP-Q1"),
        Err(AuditError::DuplicateRecord)
    );
    assert_eq!(ledger.daily_metrics().qualified_setups, 2);

    ledger
        .record_completed_trade(trade("SETUP-Q1", 1.0))
        .unwrap();
    assert_eq!(ledger.daily_metrics().trades_executed, 1);
}

#[test]
fn section_114_empty_day_has_zero_counts_and_no_fabricated_ratio_or_heat_metrics() {
    let ledger = AuditLedger::new();
    let metrics = ledger.daily_metrics();

    assert_eq!(metrics.qualified_setups, 0);
    assert_eq!(metrics.rejected_setups, 0);
    assert_eq!(metrics.trades_executed, 0);
    assert_eq!(metrics.winning_trades, 0);
    assert_eq!(metrics.losing_trades, 0);
    assert_eq!(metrics.breakevens, 0);
    assert_eq!(metrics.realized_r, 0.0);
    assert_eq!(metrics.expectancy_r, None);
    assert_eq!(metrics.profit_factor, None);
    assert_eq!(metrics.win_rate, None);
    assert_eq!(metrics.average_win_r, None);
    assert_eq!(metrics.average_loss_r, None);
    assert_eq!(metrics.maximum_drawdown_r, 0.0);
    assert_eq!(metrics.maximum_portfolio_heat, None);
    assert_eq!(metrics.mean_mfe, None);
    assert_eq!(metrics.mean_mae, None);
    assert_eq!(metrics.mean_slippage, None);
    assert_eq!(metrics.rule_compliance_percentage, None);
    assert_eq!(metrics.current_consecutive_losses, 0);
}

#[test]
fn section_114_reporting_metrics_follow_explicit_r_space_formalizations() {
    let mut ledger = AuditLedger::new();
    for (index, realized_r) in [1.0, -0.5, 0.0, 2.0, -1.0, -1.0]
        .into_iter()
        .enumerate()
    {
        let mut record = trade(&format!("SETUP-M{index}"), realized_r);
        record.mfe = index as f64 + 1.0;
        record.mae = index as f64 * 0.5;
        record.slippage = index as f64 * 0.1;
        ledger.record_completed_trade(record).unwrap();
    }

    let metrics = ledger.daily_metrics();
    assert_eq!(metrics.trades_executed, 6);
    assert_eq!(metrics.winning_trades, 2);
    assert_eq!(metrics.losing_trades, 3);
    assert_eq!(metrics.breakevens, 1);
    approx_eq(metrics.realized_r, 0.5);
    approx_eq(metrics.expectancy_r.unwrap(), 0.5 / 6.0);
    approx_eq(metrics.profit_factor.unwrap(), 1.2);
    approx_eq(metrics.win_rate.unwrap(), 2.0 / 6.0);
    approx_eq(metrics.average_win_r.unwrap(), 1.5);
    approx_eq(metrics.average_loss_r.unwrap(), -2.5 / 3.0);
    approx_eq(metrics.maximum_drawdown_r, 2.0);
    approx_eq(metrics.mean_mfe.unwrap(), 3.5);
    approx_eq(metrics.mean_mae.unwrap(), 1.25);
    approx_eq(metrics.mean_slippage.unwrap(), 0.25);
    assert_eq!(metrics.current_consecutive_losses, 2);
}

#[test]
fn profit_factor_is_none_without_realized_losses_instead_of_inventing_infinity() {
    let mut ledger = AuditLedger::new();
    ledger
        .record_completed_trade(trade("SETUP-W1", 1.0))
        .unwrap();
    ledger
        .record_completed_trade(trade("SETUP-BE", 0.0))
        .unwrap();

    let metrics = ledger.daily_metrics();
    assert_eq!(metrics.profit_factor, None);
    assert_eq!(metrics.average_loss_r, None);
    approx_eq(metrics.average_win_r.unwrap(), 1.0);
}

#[test]
fn section_97_rolling_expectancy_requires_explicit_nonzero_window_and_enough_trades() {
    let mut ledger = AuditLedger::new();
    for (index, realized_r) in [1.0, -0.5, 0.0, 2.0, -1.0, -1.0]
        .into_iter()
        .enumerate()
    {
        ledger
            .record_completed_trade(trade(&format!("SETUP-R{index}"), realized_r))
            .unwrap();
    }

    assert_eq!(ledger.rolling_expectancy_r(0), None);
    assert_eq!(ledger.rolling_expectancy_r(7), None);
    approx_eq(ledger.rolling_expectancy_r(3).unwrap(), 0.0);
    approx_eq(ledger.rolling_expectancy_r(6).unwrap(), 0.5 / 6.0);
}

#[test]
fn section_97_consecutive_loss_streak_is_analytical_and_resets_on_nonloss() {
    let mut ledger = AuditLedger::new();
    ledger
        .record_completed_trade(trade("SETUP-L1", -1.0))
        .unwrap();
    ledger
        .record_completed_trade(trade("SETUP-L2", -0.5))
        .unwrap();
    assert_eq!(ledger.daily_metrics().current_consecutive_losses, 2);

    ledger
        .record_completed_trade(trade("SETUP-B0", 0.0))
        .unwrap();
    assert_eq!(ledger.daily_metrics().current_consecutive_losses, 0);

    ledger
        .record_completed_trade(trade("SETUP-L3", -0.25))
        .unwrap();
    assert_eq!(ledger.daily_metrics().current_consecutive_losses, 1);

    ledger
        .record_completed_trade(trade("SETUP-W1", 0.5))
        .unwrap();
    assert_eq!(ledger.daily_metrics().current_consecutive_losses, 0);
}

#[test]
fn section_97_trade_count_and_loss_streak_do_not_create_a_shutdown_condition() {
    let mut ledger = AuditLedger::new();
    for index in 0..8 {
        ledger
            .record_completed_trade(trade(&format!("SETUP-LOSS-{index}"), -1.0))
            .unwrap();
    }

    let metrics = ledger.daily_metrics();
    assert_eq!(metrics.trades_executed, 8);
    assert_eq!(metrics.current_consecutive_losses, 8);

    // Audit remains append-only reporting: another structurally valid record is still accepted.
    ledger
        .record_completed_trade(trade("SETUP-AFTER-STREAK", 1.0))
        .unwrap();
    assert_eq!(ledger.daily_metrics().trades_executed, 9);
}

#[test]
fn section_114_compliance_heat_and_prevention_counters_use_explicit_observations() {
    let mut ledger = AuditLedger::new();
    for compliant in [true, true, false, true] {
        ledger.record_rule_compliance(compliant);
    }
    ledger.record_portfolio_heat(0.01).unwrap();
    ledger.record_portfolio_heat(0.025).unwrap();
    ledger.record_portfolio_heat(0.02).unwrap();
    ledger.record_duplicate_signal_prevented();
    ledger.record_duplicate_signal_prevented();
    ledger.record_risk_conflict_prevented();
    ledger.record_execution_failure();
    ledger.record_execution_failure();
    ledger.record_execution_failure();

    let metrics = ledger.daily_metrics();
    approx_eq(metrics.rule_compliance_percentage.unwrap(), 75.0);
    approx_eq(metrics.maximum_portfolio_heat.unwrap(), 0.025);
    assert_eq!(metrics.duplicate_signals_prevented, 2);
    assert_eq!(metrics.risk_conflicts_prevented, 1);
    assert_eq!(metrics.execution_failures, 3);
}

#[test]
fn invalid_portfolio_heat_is_rejected_without_fabricating_an_observation() {
    let mut ledger = AuditLedger::new();
    for invalid in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY, -0.001] {
        assert_eq!(
            ledger.record_portfolio_heat(invalid),
            Err(AuditError::InvalidRecord)
        );
    }
    assert_eq!(ledger.daily_metrics().maximum_portfolio_heat, None);
}
