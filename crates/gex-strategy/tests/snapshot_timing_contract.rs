use gex_strategy::{
    DataQuality, ExplicitAgeLimits, GexTimeObservation, MarketSnapshot, MarketSnapshotInput,
    Session, TemporalStatus, assess_snapshot_timing,
};

fn snapshot(source: i64, received: i64, quality: DataQuality) -> MarketSnapshot {
    MarketSnapshot::build(
        MarketSnapshotInput {
            snapshot_id: "snapshot-76".to_owned(),
            symbol: "SPY".to_owned(),
            source_timestamp_millis: source,
            received_timestamp_millis: received,
            spot_price: 600.0,
            session: Session::Regular,
            data_quality: quality,
        },
        &[],
    )
    .unwrap()
}

fn limits() -> ExplicitAgeLimits {
    // Caller-supplied test configuration, never a production default.
    ExplicitAgeLimits {
        market_data_stale_timeout_ms: 20,
        gex_stale_timeout_ms: 18,
        max_decision_age_ms: 10,
    }
}

fn gex(source: i64, received: i64) -> GexTimeObservation {
        snapshot_id: "snapshot-76".to_owned(),
        symbol: "SPY".to_owned(),
    GexTimeObservation {
        snapshot_id: "snapshot-76".to_owned(),
        symbol: "SPY".to_owned(),
        source_timestamp_millis: Some(source),
        received_timestamp_millis: Some(received),
        data_quality: DataQuality::Structured,
    }
}

#[test]
fn exactly_on_each_configured_age_boundary_is_within_limit() {
    let s = snapshot(1_000, 1_010, DataQuality::Structured);
    let a = assess_snapshot_timing(&s, Some(gex(1_002, 1_011)), 1_020, &limits());
    assert_eq!(a.snapshot_id, "snapshot-76");
    assert_eq!(a.market_data, TemporalStatus::WithinConfiguredAge);
    assert_eq!(a.gex_data, TemporalStatus::WithinConfiguredAge);
    assert_eq!(a.decision_age, TemporalStatus::WithinConfiguredAge);
    assert!(a.observed_clocks_within_limits());
}

#[test]
fn stale_market_does_not_become_fresh_because_gex_is_current() {
    let s = snapshot(999, 1_010, DataQuality::Structured);
    let a = assess_snapshot_timing(&s, Some(gex(1_002, 1_011)), 1_020, &limits());
    assert_eq!(a.market_data, TemporalStatus::Stale);
    assert_eq!(a.gex_data, TemporalStatus::WithinConfiguredAge);
    assert!(!a.observed_clocks_within_limits());
}

#[test]
fn stale_gex_does_not_borrow_market_timestamp() {
    let s = snapshot(1_000, 1_010, DataQuality::Structured);
    let a = assess_snapshot_timing(&s, Some(gex(1_001, 1_011)), 1_020, &limits());
    assert_eq!(a.market_data, TemporalStatus::WithinConfiguredAge);
    assert_eq!(a.gex_data, TemporalStatus::Stale);
    assert!(!a.observed_clocks_within_limits());
}

#[test]
fn decision_age_is_checked_independently_of_market_age() {
    let s = snapshot(1_005, 1_010, DataQuality::Structured);
    let config = ExplicitAgeLimits {
        market_data_stale_timeout_ms: 20,
        gex_stale_timeout_ms: 20,
        max_decision_age_ms: 9,
    };
    let a = assess_snapshot_timing(&s, Some(gex(1_003, 1_011)), 1_020, &config);
    assert_eq!(a.market_data, TemporalStatus::WithinConfiguredAge);
    assert_eq!(a.gex_data, TemporalStatus::WithinConfiguredAge);
    assert_eq!(a.decision_age, TemporalStatus::Stale);
}

#[test]
fn absent_gex_feed_metadata_remains_missing_even_with_fresh_spot() {
    let s = snapshot(1_000, 1_010, DataQuality::Structured);
    let a = assess_snapshot_timing(&s, None, 1_020, &limits());
    assert_eq!(a.gex_data, TemporalStatus::Missing);
    assert!(!a.observed_clocks_within_limits());
    let b = assess_snapshot_timing(
        &s,
        Some(GexTimeObservation {
        snapshot_id: "snapshot-76".to_owned(),
        symbol: "SPY".to_owned(),
            source_timestamp_millis: None,
            received_timestamp_millis: Some(1_010),
            data_quality: DataQuality::Structured,
        }),
        1_020,
        &limits(),
    );
    assert_eq!(b.gex_data, TemporalStatus::Missing);
}

#[test]
fn conflicting_or_future_provider_timestamps_are_invalid() {
    let s = snapshot(1_000, 1_010, DataQuality::Structured);
    let future_decision = assess_snapshot_timing(&s, Some(gex(1_002, 1_011)), 1_009, &limits());
    assert_eq!(future_decision.market_data, TemporalStatus::InvalidTimeline);
    assert_eq!(
        future_decision.decision_age,
        TemporalStatus::InvalidTimeline
    );
    assert_eq!(future_decision.gex_data, TemporalStatus::InvalidTimeline);
    let reversed = assess_snapshot_timing(&s, Some(gex(1_015, 1_014)), 1_020, &limits());
    assert_eq!(reversed.gex_data, TemporalStatus::InvalidTimeline);
    let future_gex = assess_snapshot_timing(&s, Some(gex(1_020, 1_021)), 1_020, &limits());
    assert_eq!(future_gex.gex_data, TemporalStatus::InvalidTimeline);
    let negative = assess_snapshot_timing(&s, Some(gex(-1, 1_010)), 1_020, &limits());
    assert_eq!(negative.gex_data, TemporalStatus::InvalidTimeline);
}

#[test]
fn data_quality_unknown_is_not_treated_as_freshness_proof() {
    let s = snapshot(1_000, 1_010, DataQuality::Unknown);
    let a = assess_snapshot_timing(&s, Some(gex(1_002, 1_011)), 1_020, &limits());
    assert_eq!(a.market_data, TemporalStatus::UnknownQuality);
    assert_eq!(a.decision_age, TemporalStatus::UnknownQuality);
    assert_eq!(a.gex_data, TemporalStatus::WithinConfiguredAge);
    assert!(!a.observed_clocks_within_limits());

    let b = assess_snapshot_timing(
        &snapshot(1_000, 1_010, DataQuality::Structured),
        Some(GexTimeObservation {
        snapshot_id: "snapshot-76".to_owned(),
        symbol: "SPY".to_owned(),
            source_timestamp_millis: Some(1_002),
            received_timestamp_millis: Some(1_011),
            data_quality: DataQuality::Unknown,
        }),
        1_020,
        &limits(),
    );
    assert_eq!(b.gex_data, TemporalStatus::UnknownQuality);
}

#[test]
fn caller_supplies_all_limits_with_no_defaults() {
    let s = snapshot(1_000, 1_010, DataQuality::Structured);
    let zero_limits = ExplicitAgeLimits {
        market_data_stale_timeout_ms: 0,
        gex_stale_timeout_ms: 0,
        max_decision_age_ms: 0,
    };
    let a = assess_snapshot_timing(&s, Some(gex(1_010, 1_010)), 1_010, &zero_limits);
    assert_eq!(a.market_data, TemporalStatus::Stale);
    assert_eq!(a.gex_data, TemporalStatus::WithinConfiguredAge);
    assert_eq!(a.decision_age, TemporalStatus::WithinConfiguredAge);
    // Only timing is measured. This test does not provide DATA_VALID,
    // risk permissions, plan eligibility, or any broker order authorization.
}

#[test]
fn negative_decision_time_fails_without_overflow() {
    let s = snapshot(1_000, 1_010, DataQuality::Structured);
    let a = assess_snapshot_timing(&s, Some(gex(1_002, 1_011)), -1, &limits());
    assert_eq!(a.market_data, TemporalStatus::InvalidTimeline);
    assert_eq!(a.gex_data, TemporalStatus::InvalidTimeline);
    assert_eq!(a.decision_age, TemporalStatus::InvalidTimeline);
}

#[test]
fn gex_timestamp_identity_must_match_snapshot_and_symbol() {
    let s = snapshot(1_000, 1_010, DataQuality::Structured);
    let mut wrong_snapshot = gex(1_002, 1_011);
    wrong_snapshot.snapshot_id = "other-decision".to_owned();
    let a = assess_snapshot_timing(&s, Some(wrong_snapshot), 1_020, &limits());
    assert_eq!(a.gex_data, TemporalStatus::IdentityMismatch);
    assert!(!a.observed_clocks_within_limits());

    let mut wrong_symbol = gex(1_002, 1_011);
    wrong_symbol.symbol = "QQQ".to_owned();
    let b = assess_snapshot_timing(&s, Some(wrong_symbol), 1_020, &limits());
    assert_eq!(b.gex_data, TemporalStatus::IdentityMismatch);
    assert!(!b.observed_clocks_within_limits());
}
