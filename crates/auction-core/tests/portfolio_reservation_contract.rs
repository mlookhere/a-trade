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
    // Evidence is visible through the shared ledger after concurrent claims,
    // and no losing proposal can manufacture a reservation receipt.
    let receipts = ["SETUP-ATOMIC-ONE", "SETUP-ATOMIC-TWO"]
        .iter()
        .filter(|id| ledger.reservation_evidence(id).unwrap().is_some())
        .count();
    assert_eq!(receipts, 1);
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

fn existing_exposure(
    setup_id: &str,
    cluster_id: &str,
    direction: Direction,
    risk: f64,
) -> RiskExposure {
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
    for limits in [limits(f64::MAX, 0.01), limits(0.02, f64::MAX)] {
        let input = snapshot(limits);
        assert!(matches!(
            SharedPortfolioReservations::from_snapshot(input),
            Err(ReservationError::InvalidSnapshot)
        ));
    }
}

#[test]
fn sections_67_69_baseline_and_reservation_independent_of_vector_order() {
    let first = proposal("SETUP-STABLE-ONE");
    let second = proposal("SETUP-STABLE-TWO");
    let existing = vec![
        existing_exposure("EXISTING-Z", "NASDAQ_CLUSTER", Direction::Long, 110.125),
        existing_exposure("EXISTING-A", "NASDAQ_CLUSTER", Direction::Long, 210.375),
        existing_exposure("EXISTING-M", "NASDAQ_CLUSTER", Direction::Long, 0.001),
    ];
    let current_risk = existing.iter().map(|item| item.risk_dollars).sum::<f64>();
    let cluster_ceiling = (current_risk + risk(&first) + 0.01) / 100_000.0;
    let mut outcomes = vec![];

    for reversed in [false, true] {
        let mut input = snapshot(limits(0.02, cluster_ceiling));
        input.existing = existing.clone();
        if reversed {
            input.existing.reverse();
        }
        let ledger = SharedPortfolioReservations::from_snapshot(input).unwrap();
        let first_result = ledger.reserve(&first, common::OWNER);
        let second_result = ledger.reserve(&second, common::OWNER);
        outcomes.push((first_result, second_result, ledger.reservation_count()));
    }

    assert_eq!(outcomes[0], outcomes[1]);
    assert_eq!(outcomes[0].0, Ok(()));
    assert_eq!(outcomes[0].1, Err(ReservationError::ClusterLimit));
    assert_eq!(outcomes[0].2, Ok(existing.len() + 1));
}

#[test]
fn sections_67_69_overlimit_baseline_rejection_is_order_independent() {
    let exposures = vec![
        existing_exposure("EXISTING-Z", "NASDAQ_CLUSTER", Direction::Long, 650.0),
        existing_exposure("EXISTING-A", "NASDAQ_CLUSTER", Direction::Long, 400.0),
        existing_exposure("EXISTING-M", "OTHER_CLUSTER", Direction::Long, 100.0),
    ];
    for reversed in [false, true] {
        let mut input = snapshot(wide_limits());
        input.existing = exposures.clone();
        if reversed {
            input.existing.reverse();
        }
        assert!(matches!(
            SharedPortfolioReservations::from_snapshot(input),
            Err(ReservationError::InvalidSnapshot)
        ));
    }
}

#[test]
fn sections_103_109_111_reservation_evidence_is_bound_to_asserted_snapshot_id() {
    let order = proposal("SETUP-EVIDENCE-ONE");
    let input = snapshot(wide_limits());
    let expected_snapshot_id = input.snapshot_id.clone();
    let ledger = SharedPortfolioReservations::from_snapshot(input).unwrap();

    assert_eq!(ledger.reservation_evidence(order.setup_id()), Ok(None));
    assert_eq!(ledger.reservation_evidence(""), Ok(None));
    assert_eq!(ledger.reserve(&order, common::OWNER), Ok(()));

    let evidence = ledger
        .reservation_evidence(order.setup_id())
        .unwrap()
        .unwrap();
    assert_eq!(evidence.snapshot_id, expected_snapshot_id);
    assert_eq!(evidence.exposure.setup_id, order.setup_id());
    assert_eq!(evidence.exposure.cluster_id, order.cluster_id());
    assert_eq!(evidence.exposure.direction, order.side());
    assert_eq!(evidence.exposure.risk_dollars, risk(&order));
    assert_eq!(ledger.reservation_count(), Ok(1));

    // Duplicate rejection cannot fabricate a second exposure or change
    // the original reservation's read-only provenance.
    assert_eq!(
        ledger.reserve(&order, common::OWNER),
        Err(ReservationError::DuplicateSetup)
    );
    assert_eq!(
        ledger.reservation_evidence(order.setup_id()),
        Ok(Some(evidence))
    );
    assert_eq!(ledger.reservation_evidence("UNKNOWN-SETUP"), Ok(None));
}

#[test]
fn sections_9_103_existing_broker_exposures_do_not_fabricate_reservation_receipts() {
    let exposure = existing_exposure("EXISTING-ONE", "OTHER_CLUSTER", Direction::Long, 100.0);
    let mut input = snapshot(wide_limits());
    input.existing = vec![exposure];
    let ledger = SharedPortfolioReservations::from_snapshot(input).unwrap();
    assert_eq!(ledger.reservation_evidence("EXISTING-ONE"), Ok(None));
    assert_eq!(ledger.reservation_count(), Ok(1));
}

#[test]
fn sections_16_98_100_invalidation_blocks_new_reservations_across_clones() {
    let ledger = SharedPortfolioReservations::from_snapshot(snapshot(wide_limits())).unwrap();
    let clone = ledger.clone();
    let order = proposal("SETUP-INVALIDATED-ONE");

    assert_eq!(clone.invalidate(), Ok(()));
    assert_eq!(ledger.invalidate(), Ok(()));
    assert_eq!(
        ledger.reserve(&order, common::OWNER),
        Err(ReservationError::Unavailable)
    );
    assert_eq!(
        clone.reserve(&order, common::OWNER),
        Err(ReservationError::Unavailable)
    );
    assert_eq!(ledger.reservation_count(), Ok(0));
    assert_eq!(clone.reservation_evidence(order.setup_id()), Ok(None));
}

#[test]
fn sections_9_16_103_invalidation_preserves_existing_risk_and_receipts() {
    let mut input = snapshot(wide_limits());
    input.existing = vec![existing_exposure(
        "BROKER-EXPOSURE",
        "OTHER_CLUSTER",
        Direction::Long,
        100.0,
    )];
    let ledger = SharedPortfolioReservations::from_snapshot(input).unwrap();
    let accepted = proposal("SETUP-ACCEPTED-BEFORE-INVALIDATION");
    let rejected = proposal("SETUP-REJECTED-AFTER-INVALIDATION");
    assert_eq!(ledger.reserve(&accepted, common::OWNER), Ok(()));
    let original_receipt = ledger
        .reservation_evidence(accepted.setup_id())
        .unwrap()
        .unwrap();

    assert_eq!(ledger.invalidate(), Ok(()));
    assert_eq!(
        ledger.reserve(&rejected, common::OWNER),
        Err(ReservationError::Unavailable)
    );
    assert_eq!(ledger.reservation_count(), Ok(2));
    assert_eq!(
        ledger.reservation_evidence(accepted.setup_id()),
        Ok(Some(original_receipt))
    );
    assert_eq!(ledger.reservation_evidence("BROKER-EXPOSURE"), Ok(None));
    assert_eq!(ledger.reservation_evidence(rejected.setup_id()), Ok(None));
}

#[test]
fn sections_9_16_67_109_invalidation_and_reservation_are_serialized() {
    let ledger = SharedPortfolioReservations::from_snapshot(snapshot(wide_limits())).unwrap();
    let barrier = Arc::new(Barrier::new(3));
    let submitter = ledger.clone();
    let invalidator = ledger.clone();

    let submit_barrier = barrier.clone();
    let submission = thread::spawn(move || {
        let order = proposal("SETUP-RACING-INVALIDATION");
        submit_barrier.wait();
        submitter.reserve(&order, common::OWNER)
    });
    let invalidate_barrier = barrier.clone();
    let invalidation = thread::spawn(move || {
        invalidate_barrier.wait();
        invalidator.invalidate()
    });
    barrier.wait();
    let submission_result = submission.join().unwrap();
    assert_eq!(invalidation.join().unwrap(), Ok(()));
    assert!(matches!(
        submission_result,
        Ok(()) | Err(ReservationError::Unavailable)
    ));
    assert_eq!(
        ledger.reserve(&proposal("SETUP-AFTER-RACING-INVALIDATION"), common::OWNER),
        Err(ReservationError::Unavailable)
    );
    let expected = if submission_result.is_ok() { 1 } else { 0 };
    assert_eq!(ledger.reservation_count(), Ok(expected));
    assert_eq!(
        ledger
            .reservation_evidence("SETUP-RACING-INVALIDATION")
            .unwrap()
            .is_some(),
        submission_result.is_ok()
    );
}
