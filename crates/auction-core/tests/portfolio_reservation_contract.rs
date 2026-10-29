mod common;

use std::sync::{Arc, Barrier};
use std::thread;

use auction_core::{
    Condition, Direction, InstrumentExecutionConfig, OrderProposal, OrderProposalInputs,
    PortfolioRiskLimits, PortfolioRiskSnapshot, ReservationError, RiskExposure,
    SharedPortfolioReservations, build_order_proposal,
};

fn proposal(setup_id: &str) -> OrderProposal {
    common::valid_long_proposal(setup_id, common::OWNER, common::INSTRUMENT)
}

fn risk(proposal: &OrderProposal) -> f64 {
    proposal.open_portfolio_risk_after() - proposal.open_portfolio_risk_before()
}

fn snapshot(limits: PortfolioRiskLimits) -> PortfolioRiskSnapshot {
    PortfolioRiskSnapshot {
        snapshot_id: "BROKER-RECONCILED-SNAPSHOT-1".to_owned(),
        account_equity: 100_000.0,
        limits,
        broker_reconciled: Condition::True,
        exposure_complete: Condition::True,
        data_fresh: Condition::True,
        existing: vec![],
    }
}

fn limits(open_percent: f64, cluster_percent: f64) -> PortfolioRiskLimits {
    PortfolioRiskLimits {
        max_portfolio_open_risk_percent: open_percent,
        max_cluster_open_risk_percent: cluster_percent,
    }
}

fn wide_limits() -> PortfolioRiskLimits {
    limits(0.02, 0.01)
}

fn short_proposal(setup_id: &str) -> OrderProposal {
    let proof = common::valid_short_proof(setup_id, common::OWNER, common::INSTRUMENT);
    build_order_proposal(OrderProposalInputs {
        strategy: &proof,
        entry_limit: -0.5,
        target: -15.0,
        execution_config: InstrumentExecutionConfig {
            tick_size: 0.25,
            max_entry_slippage_ticks: 4,
            stop_buffer_ticks: 2,
        },
        account_equity: 100_000.0,
        risk_percent: 0.0025,
        tick_value: 1.25,
        commissions_per_contract: 1.0,
        slippage_reserve_per_contract: 1.0,
        current_open_portfolio_risk: 100.0,
        current_cluster_risk: 50.0,
        portfolio_limits: wide_limits(),
        cluster_id: "NASDAQ_CLUSTER",
        existing_cluster_exposure: &[],
    })
    .unwrap()
}

#[test]
fn sections_67_69_atomic_thread_competition_cannot_double_spend_limit() {
    let one = proposal("SETUP-ATOMIC-ONE");
    let amount = risk(&one);
    let allowance = amount * 1.5 / 100_000.0;
    let ledger =
        SharedPortfolioReservations::from_snapshot(snapshot(limits(allowance, allowance))).unwrap();
    let barrier = Arc::new(Barrier::new(3));
    let mut handles = vec![];

    for setup_id in ["SETUP-ATOMIC-ONE", "SETUP-ATOMIC-TWO"] {
        let ledger = ledger.clone();
        let barrier = barrier.clone();
        handles.push(thread::spawn(move || {
            let order = proposal(setup_id);
            barrier.wait();
            ledger.reserve(&order, common::OWNER)
        }));
    }
    barrier.wait();
    let outcomes: Vec<_> = handles.into_iter().map(|h| h.join().unwrap()).collect();

    assert_eq!(outcomes.iter().filter(|result| result.is_ok()).count(), 1);
    assert_eq!(
        outcomes
            .iter()
            .filter(|result| **result == Err(ReservationError::PortfolioLimit))
            .count(),
        1
    );
    assert_eq!(ledger.reservation_count(), Ok(1));
}

#[test]
fn sections_8_9_103_same_setup_duplicate_and_wrong_owner_fail_closed() {
    let order = proposal("SETUP-OWNER-ONE");
    let ledger = SharedPortfolioReservations::from_snapshot(snapshot(wide_limits())).unwrap();

    assert_eq!(
        ledger.reserve(&order, "OTHER_AGENT"),
        Err(ReservationError::WrongOwner)
    );
    assert_eq!(ledger.reservation_count(), Ok(0));
    assert_eq!(ledger.reserve(&order, common::OWNER), Ok(()));
    assert_eq!(
        ledger.reserve(&order, common::OWNER),
        Err(ReservationError::DuplicateSetup)
    );
    assert_eq!(ledger.reservation_count(), Ok(1));
}

#[test]
fn sections_68_70_opposing_direction_in_same_cluster_fails_closed() {
    let long = proposal("SETUP-CLUSTER-LONG");
    let short = short_proposal("SETUP-CLUSTER-SHORT");
    let ledger = SharedPortfolioReservations::from_snapshot(snapshot(wide_limits())).unwrap();

    assert_eq!(ledger.reserve(&long, common::OWNER), Ok(()));
    assert_eq!(
        ledger.reserve(&short, common::OWNER),
        Err(ReservationError::ClusterConflict)
    );
    assert_eq!(ledger.reservation_count(), Ok(1));
}

#[test]
fn sections_16_67_109_unknown_incomplete_and_invalid_snapshots_fail_closed() {
    let base = snapshot(wide_limits());
    let mut unknown = base.clone();
    unknown.data_fresh = Condition::Unknown;
    assert!(matches!(
        SharedPortfolioReservations::from_snapshot(unknown),
        Err(ReservationError::InvalidSnapshot)
    ));

    let mut invalid = base.clone();
    invalid.exposure_complete = Condition::False;
    assert!(matches!(
        SharedPortfolioReservations::from_snapshot(invalid),
        Err(ReservationError::InvalidSnapshot)
    ));

    let mut invalid = base.clone();
    invalid.account_equity = f64::NAN;
    assert!(matches!(
        SharedPortfolioReservations::from_snapshot(invalid),
        Err(ReservationError::InvalidSnapshot)
    ));

    let mut duplicates = base;
    let exposure = RiskExposure {
        setup_id: "EXISTING".to_owned(),
        cluster_id: "NASDAQ_CLUSTER".to_owned(),
        direction: Direction::Long,
        risk_dollars: 100.0,
    };
    duplicates.existing = vec![exposure.clone(), exposure];
    assert!(matches!(
        SharedPortfolioReservations::from_snapshot(duplicates),
        Err(ReservationError::InvalidSnapshot)
    ));
}

#[test]
fn sections_67_69_rechecks_exposure_not_proposal_stale_before_values() {
    let order = proposal("SETUP-RECHECK");
    let amount = risk(&order);
    let mut state = snapshot(limits(amount * 1.5 / 100_000.0, 0.01));
    state.existing.push(RiskExposure {
        setup_id: "EXISTING-ORDER".to_owned(),
        cluster_id: "OTHER_CLUSTER".to_owned(),
        direction: Direction::Long,
        risk_dollars: amount,
    });
    let ledger = SharedPortfolioReservations::from_snapshot(state).unwrap();
    // The proposal was evaluated with a prior risk snapshot. Its portfolio flag
    // cannot compensate for the verified exposure already in this ledger.
    assert_eq!(
        ledger.reserve(&order, common::OWNER),
        Err(ReservationError::PortfolioLimit)
    );
    assert_eq!(ledger.reservation_count(), Ok(1));
}

fn existing_exposure(setup_id: &str, cluster_id: &str, direction: Direction, risk: f64) -> RiskExposure {
    RiskExposure {
        setup_id: setup_id.to_owned(),
        cluster_id: cluster_id.to_owned(),
        direction,
        risk_dollars: risk,
    }
}

#[test]
fn sections_67_69_snapshot_rejects_over_ceiling_cluster_even_below_portfolio_ceiling() {
    let mut input = snapshot(wide_limits());
    input.existing = vec![
        existing_exposure("EXISTING-MNQ", "NASDAQ_CLUSTER", Direction::Long, 600.0),
        existing_exposure("EXISTING-NQ", "NASDAQ_CLUSTER", Direction::Long, 500.0),
    ];
    assert!(matches!(
        SharedPortfolioReservations::from_snapshot(input),
        Err(ReservationError::InvalidSnapshot)
    ));
}

#[test]
fn sections_67_69_snapshot_accepts_exact_cluster_ceiling_but_no_more() {
    let mut input = snapshot(wide_limits());
    input.existing = vec![
        existing_exposure("EXISTING-MNQ", "NASDAQ_CLUSTER", Direction::Long, 600.0),
        existing_exposure("EXISTING-NQ", "NASDAQ_CLUSTER", Direction::Long, 400.0),
    ];
    let ledger = SharedPortfolioReservations::from_snapshot(input).unwrap();
    assert_eq!(ledger.reservation_count(), Ok(2));
    let order = proposal("SETUP-AT-CLUSTER-CEILING");
    assert_eq!(
        ledger.reserve(&order, common::OWNER),
        Err(ReservationError::ClusterLimit)
    );
    assert_eq!(ledger.reservation_count(), Ok(2));
}

#[test]
fn section_70_snapshot_rejects_existing_opposing_cluster_directions() {
    let mut input = snapshot(wide_limits());
    input.existing = vec![
        existing_exposure("EXISTING-LONG", "NASDAQ_CLUSTER", Direction::Long, 100.0),
        existing_exposure("EXISTING-SHORT", "NASDAQ_CLUSTER", Direction::Short, 100.0),
    ];
    assert!(matches!(
        SharedPortfolioReservations::from_snapshot(input),
        Err(ReservationError::InvalidSnapshot)
    ));
}

#[test]
fn sections_16_67_69_snapshot_rejects_nonfinite_computed_ceilings() {
    for limits in [
        limits(f64::MAX, 0.01),
        limits(0.02, f64::MAX),
    ] {
        let input = snapshot(limits);
        assert!(matches!(
            SharedPortfolioReservations::from_snapshot(input),
            Err(ReservationError::InvalidSnapshot)
        ));
    }
}
