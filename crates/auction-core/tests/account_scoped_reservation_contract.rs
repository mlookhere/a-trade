mod common;

use std::sync::{Arc, Barrier};
use std::thread;

use auction_core::{
    AccountScopedPortfolioReservations, BrokerEvidenceError, BrokerPortfolioEvidence, Condition,
    Direction, PortfolioRiskLimits, ReservationError, RiskExposure, ScopedReservationError,
};

const ACCOUNT_ID: &str = "BROKER-ACCOUNT-A";
const SNAPSHOT_ID: &str = "ASSERTED-PORTFOLIO-SNAPSHOT-A";

fn proposal(setup_id: &str) -> auction_core::OrderProposal {
    common::valid_long_proposal(setup_id, common::OWNER, common::INSTRUMENT)
}

fn proposed_risk(proposal: &auction_core::OrderProposal) -> f64 {
    proposal.open_portfolio_risk_after() - proposal.open_portfolio_risk_before()
}

fn evidence(max_risk_dollars: f64) -> BrokerPortfolioEvidence {
    BrokerPortfolioEvidence {
        expected_account_id: ACCOUNT_ID.to_owned(),
        observed_account_id: ACCOUNT_ID.to_owned(),
        snapshot_id: SNAPSHOT_ID.to_owned(),
        broker_connected: Condition::True,
        broker_state_known: Condition::True,
        position_state_known: Condition::True,
        open_order_state_known: Condition::True,
        data_fresh: Condition::True,
        timestamps_consistent: Condition::True,
        exposure_complete: Condition::True,
        account_equity: 100_000.0,
        limits: PortfolioRiskLimits {
            max_portfolio_open_risk_percent: max_risk_dollars / 100_000.0,
            max_cluster_open_risk_percent: max_risk_dollars / 100_000.0,
        },
        positions: vec![],
        open_orders: vec![],
    }
}

#[test]
fn sections_9_16_103_account_id_mismatch_does_not_allocate_risk() {
    let ledger = AccountScopedPortfolioReservations::from_evidence(evidence(2_000.0)).unwrap();
    let order = proposal("SETUP-ACCOUNT-MISMATCH");
    assert_eq!(
        ledger.reserve(&order, common::OWNER, "OTHER-ACCOUNT", SNAPSHOT_ID),
        Err(ScopedReservationError::AccountMismatch)
    );
    assert_eq!(
        ledger.reserve(&order, common::OWNER, "", SNAPSHOT_ID),
        Err(ScopedReservationError::AccountMismatch)
    );
    assert_eq!(
        ledger.reserve(&order, common::OWNER, "   ", SNAPSHOT_ID),
        Err(ScopedReservationError::AccountMismatch)
    );
    assert!(ledger.audit_view().unwrap().ledger.exposures.is_empty());
    assert_eq!(ledger.reservation_evidence(order.setup_id()).unwrap(), None);
}

#[test]
fn sections_9_16_103_109_account_scope_is_retained_on_receipt_and_audit() {
    let mut input = evidence(2_000.0);
    input.positions.push(RiskExposure {
        setup_id: "BROKER-EXISTING".to_owned(),
        cluster_id: "OTHER_CLUSTER".to_owned(),
        direction: Direction::Long,
        risk_dollars: 100.0,
    });
    let ledger = AccountScopedPortfolioReservations::from_evidence(input).unwrap();
    let order = proposal("SETUP-ACCOUNT-ACCEPTED");
    assert_eq!(
        ledger.reserve(&order, common::OWNER, ACCOUNT_ID, SNAPSHOT_ID),
        Ok(())
    );
    let receipt = ledger.reservation_evidence(order.setup_id()).unwrap().unwrap();
    assert_eq!(receipt.account_id, ACCOUNT_ID);
    assert_eq!(receipt.reservation.snapshot_id, SNAPSHOT_ID);
    assert_eq!(receipt.reservation.exposure.setup_id, order.setup_id());
    let view = ledger.audit_view().unwrap();
    assert_eq!(view.account_id, ACCOUNT_ID);
    assert_eq!(view.ledger.snapshot_id, SNAPSHOT_ID);
    assert_eq!(view.ledger.exposures.len(), 2);
    assert_eq!(view.ledger.accepted_setup_ids, vec![order.setup_id()]);
    assert!(!view.ledger.invalidated);
    // Initial broker exposure is not a locally accepted reservation.
    assert_eq!(ledger.reservation_evidence("BROKER-EXISTING").unwrap(), None);
}

#[test]
fn sections_16_103_wrong_snapshot_and_invalidation_remain_authoritative() {
    let ledger = AccountScopedPortfolioReservations::from_evidence(evidence(2_000.0)).unwrap();
    let order = proposal("SETUP-ACCOUNT-SNAPSHOT");
    assert_eq!(
        ledger.reserve(&order, common::OWNER, ACCOUNT_ID, "OLD-SNAPSHOT"),
        Err(ScopedReservationError::Reservation(
            ReservationError::SnapshotMismatch
        ))
    );
    let peer = ledger.clone();
    assert_eq!(peer.invalidate(), Ok(()));
    assert_eq!(
        ledger.reserve(&order, common::OWNER, ACCOUNT_ID, SNAPSHOT_ID),
        Err(ScopedReservationError::Reservation(
            ReservationError::Unavailable
        ))
    );
    assert!(peer.audit_view().unwrap().ledger.invalidated);
    assert!(ledger.audit_view().unwrap().ledger.exposures.is_empty());
}

#[test]
fn sections_9_67_69_103_shared_account_clones_cannot_double_spend() {
    let sample = proposal("SETUP-SCOPED-COMPETE-A");
    let ledger = AccountScopedPortfolioReservations::from_evidence(evidence(
        proposed_risk(&sample) * 1.5,
    ))
    .unwrap();
    let barrier = Arc::new(Barrier::new(3));
    let mut handles = Vec::new();
    for setup_id in ["SETUP-SCOPED-COMPETE-A", "SETUP-SCOPED-COMPETE-B"] {
        let ledger = ledger.clone();
        let barrier = barrier.clone();
        handles.push(thread::spawn(move || {
            let order = proposal(setup_id);
            barrier.wait();
            ledger.reserve(&order, common::OWNER, ACCOUNT_ID, SNAPSHOT_ID)
        }));
    }
    barrier.wait();
    let outcomes: Vec<_> = handles.into_iter().map(|h| h.join().unwrap()).collect();
    assert_eq!(outcomes.iter().filter(|result| result.is_ok()).count(), 1);
    assert_eq!(
        outcomes
            .iter()
            .filter(|result| {
                **result
                    == Err(ScopedReservationError::Reservation(
                        ReservationError::PortfolioLimit,
                    ))
            })
            .count(),
        1
    );
    let view = ledger.audit_view().unwrap();
    assert_eq!(view.account_id, ACCOUNT_ID);
    assert_eq!(view.ledger.exposures.len(), 1);
    assert_eq!(view.ledger.accepted_setup_ids.len(), 1);
}

#[test]
fn sections_9_16_103_duplicate_and_owner_rejection_remain_fail_closed() {
    let ledger = AccountScopedPortfolioReservations::from_evidence(evidence(2_000.0)).unwrap();
    let order = proposal("SETUP-SCOPED-DUPLICATE");
    assert_eq!(
        ledger.reserve(&order, "WRONG-OWNER", ACCOUNT_ID, SNAPSHOT_ID),
        Err(ScopedReservationError::Reservation(
            ReservationError::WrongOwner
        ))
    );
    assert_eq!(
        ledger.reserve(&order, common::OWNER, ACCOUNT_ID, SNAPSHOT_ID),
        Ok(())
    );
    assert_eq!(
        ledger.reserve(&order, common::OWNER, ACCOUNT_ID, SNAPSHOT_ID),
        Err(ScopedReservationError::Reservation(
            ReservationError::DuplicateSetup
        ))
    );
    assert_eq!(ledger.audit_view().unwrap().ledger.exposures.len(), 1);
}

#[test]
fn sections_16_67_incomplete_broker_evidence_never_constructs_account_book() {
    let mut input = evidence(2_000.0);
    input.open_orders.clear();
    input.open_order_state_known = Condition::Unknown;
    assert!(matches!(
        AccountScopedPortfolioReservations::from_evidence(input),
        Err(ScopedReservationError::Evidence(
            BrokerEvidenceError::OpenOrdersUnknown
        ))
    ));
    let mut input = evidence(2_000.0);
    input.observed_account_id = "BROKER-ACCOUNT-B".to_owned();
    assert!(matches!(
        AccountScopedPortfolioReservations::from_evidence(input),
        Err(ScopedReservationError::Evidence(
            BrokerEvidenceError::AccountMismatch
        ))
    ));
}
