use auction_core::{
    BrokerEvidenceError, BrokerPortfolioEvidence, Condition, Direction, PortfolioRiskLimits,
    ReservationError, RiskExposure, SharedPortfolioReservations,
};

fn exposure(setup_id: &str, cluster: &str, direction: Direction, risk: f64) -> RiskExposure {
    RiskExposure {
        setup_id: setup_id.to_owned(),
        cluster_id: cluster.to_owned(),
        direction,
        risk_dollars: risk,
    }
}

fn evidence() -> BrokerPortfolioEvidence {
    BrokerPortfolioEvidence {
        expected_account_id: "ACCOUNT-ONE".to_owned(),
        observed_account_id: "ACCOUNT-ONE".to_owned(),
        snapshot_id: "ASSERTED-SNAPSHOT-ONE".to_owned(),
        broker_connected: Condition::True,
        broker_state_known: Condition::True,
        position_state_known: Condition::True,
        open_order_state_known: Condition::True,
        data_fresh: Condition::True,
        timestamps_consistent: Condition::True,
        exposure_complete: Condition::True,
        account_equity: 100_000.0,
        limits: PortfolioRiskLimits {
            max_portfolio_open_risk_percent: 0.02,
            max_cluster_open_risk_percent: 0.01,
        },
        positions: vec![exposure(
            "POSITION-ONE",
            "NASDAQ_CLUSTER",
            Direction::Long,
            100.0,
        )],
        open_orders: vec![exposure(
            "ORDER-ONE",
            "OTHER_CLUSTER",
            Direction::Long,
            200.0,
        )],
    }
}

#[test]
fn sections_9_16_67_109_valid_asserted_positions_and_open_orders_are_both_counted() {
    let snapshot = evidence().into_asserted_snapshot().unwrap();
    assert_eq!(snapshot.snapshot_id, "ASSERTED-SNAPSHOT-ONE");
    assert_eq!(snapshot.existing.len(), 2);
    let ledger = SharedPortfolioReservations::from_snapshot(snapshot).unwrap();
    let view = ledger.audit_view().unwrap();
    assert_eq!(view.exposures.len(), 2);
    assert_eq!(view.exposures[0].setup_id, "ORDER-ONE");
    assert_eq!(view.exposures[1].setup_id, "POSITION-ONE");
    assert!(view.accepted_setup_ids.is_empty());
}

#[test]
fn section_16_wrong_or_absent_account_never_produces_a_snapshot() {
    let mut input = evidence();
    input.observed_account_id = "ACCOUNT-TWO".to_owned();
    assert!(matches!(
        input.into_asserted_snapshot(),
        Err(BrokerEvidenceError::AccountMismatch)
    ));
    let mut input = evidence();
    input.expected_account_id.clear();
    assert!(matches!(
        input.into_asserted_snapshot(),
        Err(BrokerEvidenceError::MissingAccount)
    ));
}

#[test]
fn sections_9_16_empty_order_list_with_unknown_order_state_is_not_zero_risk() {
    let mut input = evidence();
    input.open_orders.clear();
    input.open_order_state_known = Condition::Unknown;
    assert!(matches!(
        input.into_asserted_snapshot(),
        Err(BrokerEvidenceError::OpenOrdersUnknown)
    ));
    let mut input = evidence();
    input.open_order_state_known = Condition::False;
    assert!(matches!(
        input.into_asserted_snapshot(),
        Err(BrokerEvidenceError::OpenOrdersUnknown)
    ));
}

#[test]
fn section_16_unknown_position_broker_or_timestamp_state_fails_closed() {
    let mut input = evidence();
    input.position_state_known = Condition::Unknown;
    assert!(matches!(
        input.into_asserted_snapshot(),
        Err(BrokerEvidenceError::PositionsUnknown)
    ));
    let mut input = evidence();
    input.broker_connected = Condition::False;
    assert!(matches!(
        input.into_asserted_snapshot(),
        Err(BrokerEvidenceError::BrokerUnknown)
    ));
    let mut input = evidence();
    input.timestamps_consistent = Condition::Unknown;
    assert!(matches!(
        input.into_asserted_snapshot(),
        Err(BrokerEvidenceError::DataUnknown)
    ));
    let mut input = evidence();
    input.exposure_complete = Condition::False;
    assert!(matches!(
        input.into_asserted_snapshot(),
        Err(BrokerEvidenceError::DataUnknown)
    ));
}

#[test]
fn sections_9_103_position_order_setup_overlap_is_ambiguous_and_rejected() {
    let mut input = evidence();
    input.open_orders[0].setup_id = "POSITION-ONE".to_owned();
    assert!(matches!(
        input.into_asserted_snapshot(),
        Err(BrokerEvidenceError::DuplicateSetup)
    ));
    let mut input = evidence();
    input.positions.push(input.positions[0].clone());
    assert!(matches!(
        input.into_asserted_snapshot(),
        Err(BrokerEvidenceError::DuplicateSetup)
    ));
}

#[test]
fn sections_67_69_malformed_risk_and_cluster_ceiling_fail_closed() {
    let mut input = evidence();
    input.open_orders[0].risk_dollars = f64::NAN;
    assert!(matches!(
        input.into_asserted_snapshot(),
        Err(BrokerEvidenceError::InvalidExposure)
    ));
    let mut input = evidence();
    input.positions[0].risk_dollars = 0.0;
    assert!(matches!(
        input.into_asserted_snapshot(),
        Err(BrokerEvidenceError::InvalidExposure)
    ));
    let mut input = evidence();
    input.positions[0].risk_dollars = 1_100.0;
    assert!(matches!(
        input.into_asserted_snapshot(),
        Err(BrokerEvidenceError::InvalidRiskBook(
            ReservationError::InvalidSnapshot
        ))
    ));
}

#[test]
fn section_70_conflicting_existing_cluster_directions_are_not_netted() {
    let mut input = evidence();
    input.open_orders[0].cluster_id = "NASDAQ_CLUSTER".to_owned();
    input.open_orders[0].direction = Direction::Short;
    assert!(matches!(
        input.into_asserted_snapshot(),
        Err(BrokerEvidenceError::InvalidRiskBook(
            ReservationError::InvalidSnapshot
        ))
    ));
}
