mod common;

use std::sync::{Arc, Barrier};
use std::thread;

use auction_core::{
    BrokerEvidenceError, BrokerPortfolioEvidence, Condition, Direction, PortfolioRiskLimits,
    ProcessAccountRiskRegistry, ProcessRiskRegistryError, ReservationError, RiskExposure,
    ScopedReservationError,
};

const ACCOUNT: &str = "ASSERTED-ACCOUNT-A";
const SNAPSHOT: &str = "ASSERTED-SNAPSHOT-A";

fn proposal(setup_id: &str) -> auction_core::OrderProposal {
    common::valid_long_proposal(setup_id, common::OWNER, common::INSTRUMENT)
}

fn risk(p: &auction_core::OrderProposal) -> f64 {
    p.open_portfolio_risk_after() - p.open_portfolio_risk_before()
}

fn evidence(account: &str, allowance: f64) -> BrokerPortfolioEvidence {
    BrokerPortfolioEvidence {
        expected_account_id: account.to_owned(),
        observed_account_id: account.to_owned(),
        snapshot_id: SNAPSHOT.to_owned(),
        broker_connected: Condition::True,
        broker_state_known: Condition::True,
        position_state_known: Condition::True,
        open_order_state_known: Condition::True,
        data_fresh: Condition::True,
        timestamps_consistent: Condition::True,
        exposure_complete: Condition::True,
        account_equity: 100_000.0,
        limits: PortfolioRiskLimits {
            max_portfolio_open_risk_percent: allowance / 100_000.0,
            max_cluster_open_risk_percent: allowance / 100_000.0,
        },
        positions: vec![],
        open_orders: vec![],
    }
}

#[test]
fn sections_9_67_103_same_initial_evidence_reuses_the_same_account_book() {
    let sample = proposal("SETUP-REGISTRY-ONE");
    let input = evidence(ACCOUNT, risk(&sample) * 1.5);
    let registry = ProcessAccountRiskRegistry::new();
    let first = registry.get_or_initialize(input.clone()).unwrap();
    let second = registry.get_or_initialize(input).unwrap();
    assert_eq!(
        first.reserve(&sample, common::OWNER, ACCOUNT, SNAPSHOT),
        Ok(())
    );
    let other = proposal("SETUP-REGISTRY-TWO");
    assert_eq!(
        second.reserve(&other, common::OWNER, ACCOUNT, SNAPSHOT),
        Err(ScopedReservationError::Reservation(
            ReservationError::PortfolioLimit
        ))
    );
    assert_eq!(
        second.audit_view().unwrap().ledger.accepted_setup_ids.len(),
        1
    );
}

#[test]
fn sections_9_67_69_103_competing_registry_initializers_do_not_double_spend() {
    let sample = proposal("SETUP-RACE-ONE");
    let registry = ProcessAccountRiskRegistry::new();
    let barrier = Arc::new(Barrier::new(3));
    let mut threads = Vec::new();
    for setup_id in ["SETUP-RACE-ONE", "SETUP-RACE-TWO"] {
        let registry = registry.clone();
        let barrier = barrier.clone();
        let input = evidence(ACCOUNT, risk(&sample) * 1.5);
        threads.push(thread::spawn(move || {
            barrier.wait();
            let ledger = registry.get_or_initialize(input).unwrap();
            ledger.reserve(&proposal(setup_id), common::OWNER, ACCOUNT, SNAPSHOT)
        }));
    }
    barrier.wait();
    let results: Vec<_> = threads.into_iter().map(|t| t.join().unwrap()).collect();
    assert_eq!(results.iter().filter(|value| value.is_ok()).count(), 1);
    assert_eq!(
        results
            .iter()
            .filter(|value| {
                **value
                    == Err(ScopedReservationError::Reservation(
                        ReservationError::PortfolioLimit,
                    ))
            })
            .count(),
        1
    );
    let view = registry
        .get_or_initialize(evidence(ACCOUNT, risk(&sample) * 1.5))
        .unwrap()
        .audit_view()
        .unwrap();
    assert_eq!(view.ledger.exposures.len(), 1);
}

#[test]
fn sections_16_67_conflicting_reinit_latches_old_ledger_without_recovery() {
    let initial = evidence(ACCOUNT, 2000.0);
    let registry = ProcessAccountRiskRegistry::new();
    let old_book = registry.get_or_initialize(initial.clone()).unwrap();

    let mut changed = initial.clone();
    changed.snapshot_id = "ASSERTED-SNAPSHOT-CHANGED".to_owned();
    assert!(matches!(
        registry.get_or_initialize(changed),
        Err(ProcessRiskRegistryError::ConflictingEvidence)
    ));
    assert_eq!(
        old_book.reserve(
            &proposal("SETUP-AFTER-CONFLICT"),
            common::OWNER,
            ACCOUNT,
            SNAPSHOT
        ),
        Err(ScopedReservationError::Reservation(
            ReservationError::Unavailable
        ))
    );
    assert!(matches!(
        registry.get_or_initialize(initial),
        Err(ProcessRiskRegistryError::Unavailable)
    ));
    assert!(old_book.audit_view().unwrap().ledger.invalidated);
}

#[test]
fn sections_16_67_same_snapshot_changed_baseline_or_limits_fails_closed() {
    for case in 0..4 {
        let original = evidence(ACCOUNT, 2000.0);
        let registry = ProcessAccountRiskRegistry::new();
        let old = registry.get_or_initialize(original.clone()).unwrap();
        let mut changed = original;
        match case {
            0 => changed.account_equity = 100_001.0,
            1 => changed.limits.max_cluster_open_risk_percent = 0.019,
            2 => changed.positions.push(RiskExposure {
                setup_id: "POSITION-ONE".to_owned(),
                cluster_id: "NASDAQ_CLUSTER".to_owned(),
                direction: Direction::Long,
                risk_dollars: 100.0,
            }),
            _ => changed.open_orders.push(RiskExposure {
                setup_id: "ORDER-ONE".to_owned(),
                cluster_id: "NASDAQ_CLUSTER".to_owned(),
                direction: Direction::Long,
                risk_dollars: 100.0,
            }),
        }
        assert!(matches!(
            registry.get_or_initialize(changed),
            Err(ProcessRiskRegistryError::ConflictingEvidence)
        ));
        assert!(old.audit_view().unwrap().ledger.invalidated);
    }
}

#[test]
fn section_16_unknown_later_broker_order_state_invalidates_existing_book() {
    let registry = ProcessAccountRiskRegistry::new();
    let book = registry
        .get_or_initialize(evidence(ACCOUNT, 2000.0))
        .unwrap();
    let mut unknown = evidence(ACCOUNT, 2000.0);
    unknown.open_orders.clear();
    unknown.open_order_state_known = Condition::Unknown;
    assert!(matches!(
        registry.get_or_initialize(unknown),
        Err(ProcessRiskRegistryError::InvalidEvidence(
            BrokerEvidenceError::OpenOrdersUnknown
        ))
    ));
    assert!(book.audit_view().unwrap().ledger.invalidated);
    assert!(matches!(
        registry.get_or_initialize(evidence(ACCOUNT, 2000.0)),
        Err(ProcessRiskRegistryError::Unavailable)
    ));
}

#[test]
fn sections_9_16_distinct_declared_accounts_are_separate_offline_books() {
    let registry = ProcessAccountRiskRegistry::new();
    let one = registry
        .get_or_initialize(evidence("ACCOUNT-ONE", 2000.0))
        .unwrap();
    let two = registry
        .get_or_initialize(evidence("ACCOUNT-TWO", 2000.0))
        .unwrap();
    assert_eq!(
        one.reserve(
            &proposal("SETUP-ACCOUNT-ONE"),
            common::OWNER,
            "ACCOUNT-ONE",
            SNAPSHOT,
        ),
        Ok(())
    );
    assert!(two.audit_view().unwrap().ledger.exposures.is_empty());
    assert!(one.audit_view().unwrap().ledger.exposures.len() == 1);
    // Separate book instances here do not assert account-wide portfolio
    // aggregation or correlated cross-account risk safety.
}
