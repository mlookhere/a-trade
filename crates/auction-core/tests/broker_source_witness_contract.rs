//! Canonical §§9,16,67-70,103,109,122: offline provenance review tests.
//! Fixtures are deliberate caller claims, NEVER authenticated broker records.

use auction_core::{
    BrokerEvidenceError, BrokerPortfolioEvidence, Condition, Direction, PortfolioRiskLimits,
    RiskExposure, SourceWitnessError, UntrustedBrokerSourceWitness, UntrustedCollectionWitness,
};

fn risk(setup: &str, cluster: &str, dollars: f64) -> RiskExposure {
    RiskExposure {
        setup_id: setup.to_owned(),
        cluster_id: cluster.to_owned(),
        direction: Direction::Long,
        risk_dollars: dollars,
    }
}

fn evidence() -> BrokerPortfolioEvidence {
    BrokerPortfolioEvidence {
        expected_account_id: "ACCOUNT-A".to_owned(),
        observed_account_id: "ACCOUNT-A".to_owned(),
        snapshot_id: "CAPTURE-1".to_owned(),
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
        positions: vec![risk("POSITION-1", "NASDAQ", 100.0)],
        open_orders: vec![risk("ORDER-1", "OTHER", 200.0)],
    }
}

fn collection(reference: &str, count: usize) -> UntrustedCollectionWitness {
    UntrustedCollectionWitness {
        source_account_id: "ACCOUNT-A".to_owned(),
        capture_id: "CAPTURE-1".to_owned(),
        response_reference: reference.to_owned(),
        observed_at_unix_ms: Some(1_000),
        completeness: Condition::True,
        reported_items: Some(count),
    }
}

fn witness() -> UntrustedBrokerSourceWitness {
    UntrustedBrokerSourceWitness {
        provider_reference: "MOCK-NOT-AUTHENTICATED".to_owned(),
        connection_reference: "MOCK-SESSION".to_owned(),
        source_account_id: "ACCOUNT-A".to_owned(),
        capture_id: "CAPTURE-1".to_owned(),
        account_observed_at_unix_ms: Some(1_000),
        positions: collection("POSITION-RESPONSE-1", 1),
        open_orders: collection("ORDER-RESPONSE-1", 1),
    }
}

#[test]
fn sections_9_16_offline_review_preserves_both_exposure_counts_without_broker_authority() {
    let reviewed = witness().review_offline(&evidence()).unwrap();
    assert_eq!(reviewed.source_account_id, "ACCOUNT-A");
    assert_eq!(reviewed.capture_id, "CAPTURE-1");
    assert_eq!(reviewed.position_items, 1);
    assert_eq!(reviewed.open_order_items, 1);
    // Type only exposes non-authorizing provenance/counts, no risk permit.
}

#[test]
fn section_16_declared_zero_requires_complete_state_and_explicit_zero_count() {
    let mut input = evidence();
    input.open_orders.clear();
    let mut source = witness();
    source.open_orders.reported_items = Some(0);
    assert_eq!(source.review_offline(&input).unwrap().open_order_items, 0);
    input.open_order_state_known = Condition::Unknown;
    assert_eq!(
        source.review_offline(&input),
        Err(SourceWitnessError::InvalidAssertedRisk(
            BrokerEvidenceError::OpenOrdersUnknown
        ))
    );
    input.open_order_state_known = Condition::True;
    source.open_orders.completeness = Condition::Unknown;
    assert_eq!(
        source.review_offline(&input),
        Err(SourceWitnessError::CollectionUnknown)
    );
}

#[test]
fn section_16_rejects_missing_source_session_and_capture() {
    let mut source = witness();
    source.provider_reference.clear();
    assert_eq!(
        source.review_offline(&evidence()),
        Err(SourceWitnessError::MissingSource)
    );
    source = witness();
    source.connection_reference.clear();
    assert_eq!(
        source.review_offline(&evidence()),
        Err(SourceWitnessError::MissingSource)
    );
    source = witness();
    source.capture_id.clear();
    assert_eq!(
        source.review_offline(&evidence()),
        Err(SourceWitnessError::MissingCapture)
    );
    source = witness();
    source.positions.response_reference.clear();
    assert_eq!(
        source.review_offline(&evidence()),
        Err(SourceWitnessError::MissingResponseReference)
    );
}

#[test]
fn sections_9_16_reject_cross_account_and_mixed_capture_claims() {
    let mut source = witness();
    source.source_account_id = "ACCOUNT-B".to_owned();
    assert_eq!(
        source.review_offline(&evidence()),
        Err(SourceWitnessError::AccountMismatch)
    );
    source = witness();
    source.open_orders.source_account_id = "ACCOUNT-B".to_owned();
    assert_eq!(
        source.review_offline(&evidence()),
        Err(SourceWitnessError::AccountMismatch)
    );
    source = witness();
    source.capture_id = "CAPTURE-OTHER".to_owned();
    assert_eq!(
        source.review_offline(&evidence()),
        Err(SourceWitnessError::CaptureMismatch)
    );
    source = witness();
    source.positions.capture_id = "CAPTURE-OTHER".to_owned();
    assert_eq!(
        source.review_offline(&evidence()),
        Err(SourceWitnessError::CaptureMismatch)
    );
    source = witness();
    let mut input = evidence();
    input.observed_account_id = "ACCOUNT-B".to_owned();
    assert_eq!(
        source.review_offline(&input),
        Err(SourceWitnessError::AccountMismatch)
    );
}

#[test]
fn section_16_missing_timestamps_are_unknown_not_fresh() {
    let mut source = witness();
    source.account_observed_at_unix_ms = None;
    assert_eq!(
        source.review_offline(&evidence()),
        Err(SourceWitnessError::MissingTimestamp)
    );
    source = witness();
    source.positions.observed_at_unix_ms = Some(0);
    assert_eq!(
        source.review_offline(&evidence()),
        Err(SourceWitnessError::MissingTimestamp)
    );
    source = witness();
    source.open_orders.observed_at_unix_ms = None;
    assert_eq!(
        source.review_offline(&evidence()),
        Err(SourceWitnessError::MissingTimestamp)
    );
    // Well-formed timestamps are still unverified caller strings/integers,
    // not a proven clock/freshness or provider identity.
}

#[test]
fn section_16_no_implicit_zero_or_ignored_unreported_open_orders() {
    let mut source = witness();
    source.open_orders.reported_items = None;
    assert_eq!(
        source.review_offline(&evidence()),
        Err(SourceWitnessError::MissingItemCount)
    );
    source = witness();
    source.open_orders.reported_items = Some(0);
    assert_eq!(
        source.review_offline(&evidence()),
        Err(SourceWitnessError::CountMismatch)
    );
    source = witness();
    source.positions.reported_items = Some(2);
    assert_eq!(
        source.review_offline(&evidence()),
        Err(SourceWitnessError::CountMismatch)
    );
    for state in [Condition::False, Condition::Unknown] {
        source = witness();
        source.open_orders.completeness = state;
        assert_eq!(
            source.review_offline(&evidence()),
            Err(SourceWitnessError::CollectionUnknown)
        );
    }
}

#[test]
fn sections_9_67_70_103_duplicate_and_risk_invalidity_reuse_existing_validator() {
    let mut input = evidence();
    input.open_orders[0].setup_id = "POSITION-1".to_owned();
    assert_eq!(
        witness().review_offline(&input),
        Err(SourceWitnessError::InvalidAssertedRisk(
            BrokerEvidenceError::DuplicateSetup
        ))
    );
    input = evidence();
    input.positions[0].risk_dollars = f64::NAN;
    assert_eq!(
        witness().review_offline(&input),
        Err(SourceWitnessError::InvalidAssertedRisk(
            BrokerEvidenceError::InvalidExposure
        ))
    );
    input = evidence();
    input.timestamps_consistent = Condition::Unknown;
    assert_eq!(
        witness().review_offline(&input),
        Err(SourceWitnessError::InvalidAssertedRisk(
            BrokerEvidenceError::DataUnknown
        ))
    );
}

#[test]
fn section_16_none_or_false_position_state_not_inferred_from_reported_count() {
    for status in [Condition::Unknown, Condition::False] {
        let mut input = evidence();
        input.position_state_known = status;
        assert_eq!(
            witness().review_offline(&input),
            Err(SourceWitnessError::InvalidAssertedRisk(
                BrokerEvidenceError::PositionsUnknown
            ))
        );
    }
}
