//! AI-native research-only GEX role annotations cannot validate ambiguous
//! numeric evidence, stale source contents or UNKNOWN-quality data.
//! Canonical §§5-6,14-16,70,103,111-116: no role/direction/order authority.

use gex_strategy::{
    AiNativeFeatureInput, AiNativeFeatureSet, DataQuality, EvidenceAuditError, GexLevelRole,
    GexScope, MarketSnapshot, MarketSnapshotInput, ObservedValue, RawGexLevel, ReferenceStatus,
    ResearchGexRoleComparison, ResearchGexRoleComparisonError, ResearchGexRoleInput,
    ResearchGexRoleObservationStatus, ResearchGexRoleReport, ResearchGexRoleSubmission, Session,
    gex_level_reference, resolve_reference,
};

fn snapshot(id: &str, spot: f64, quality: DataQuality, raw: &[RawGexLevel]) -> MarketSnapshot {
    MarketSnapshot::build(
        MarketSnapshotInput {
            snapshot_id: id.to_owned(),
            symbol: "SPY".to_owned(),
            source_timestamp_millis: 1_000,
            received_timestamp_millis: 1_010,
            spot_price: spot,
            session: Session::Regular,
            data_quality: quality,
        },
        raw,
    )
    .unwrap()
}

fn level(strike: f64, gex_value: f64, scope: GexScope) -> RawGexLevel {
    RawGexLevel {
        strike,
        gex_value,
        scope,
    }
}

fn features(s: &MarketSnapshot) -> AiNativeFeatureSet {
    AiNativeFeatureSet::build(
        s,
        AiNativeFeatureInput {
            snapshot_id: s.snapshot_id().to_owned(),
            ..Default::default()
        },
    )
    .unwrap()
}

fn submit(
    s: &MarketSnapshot,
    agent: &str,
    index: usize,
    role: GexLevelRole,
) -> ResearchGexRoleSubmission {
    let gex = &s.gex_levels()[index];
    let report = ResearchGexRoleReport::build(
        s,
        vec![ResearchGexRoleInput {
            snapshot_id: s.snapshot_id().to_owned(),
            level_index: index,
            scope: gex.scope.clone(),
            strike: gex.strike,
            role,
            rationale: format!("Unverified {agent} interpretation"),
        }],
    )
    .unwrap();
    ResearchGexRoleSubmission::new(agent, report).unwrap()
}

#[test]
fn agreeing_role_claims_cannot_make_duplicate_gex_reference_observed() {
    let raw = [
        level(610.0, 12.0, GexScope::Aggregate),
        level(610.0, -8.0, GexScope::Aggregate),
    ];
    let s = snapshot("same", 600.0, DataQuality::Structured, &raw);
    let f = features(&s);
    let a = submit(&s, "agent-a", 0, GexLevelRole::Support);
    let b = submit(&s, "agent-b", 0, GexLevelRole::Support);
    let comparison = ResearchGexRoleComparison::build(&s, vec![a, b]).unwrap();

    assert_eq!(comparison.validate_snapshot(&s), Ok(()));
    assert_eq!(comparison.for_level(0).unwrap().claims().len(), 2);
    assert_eq!(
        comparison.for_level(0).unwrap().status(),
        ResearchGexRoleObservationStatus::Unverified
    );
    assert!(comparison.for_level(1).is_none());
    assert_eq!(s.gex_levels()[0].role, GexLevelRole::Unassigned);

    let reference = gex_level_reference(&GexScope::Aggregate, 610.0).unwrap();
    let check = resolve_reference(&s, &f, &reference).unwrap();
    assert_eq!(check.status(), ReferenceStatus::Ambiguous);
    assert_eq!(check.observed_value(), None);
}

#[test]
fn unknown_quality_never_becomes_numeric_evidence_through_an_annotation() {
    let raw = [level(610.0, 12.0, GexScope::Aggregate)];
    let s = snapshot("unknown", 600.0, DataQuality::Unknown, &raw);
    let f = features(&s);
    let a = submit(&s, "agent-a", 0, GexLevelRole::Support);
    let b = submit(&s, "agent-b", 0, GexLevelRole::Resistance);
    let comparison = ResearchGexRoleComparison::build(&s, vec![a, b]).unwrap();

    assert_eq!(
        comparison.for_level(0).unwrap().status(),
        ResearchGexRoleObservationStatus::Conflicting
    );
    let reference = gex_level_reference(&GexScope::Aggregate, 610.0).unwrap();
    let check = resolve_reference(&s, &f, &reference).unwrap();
    assert_eq!(check.status(), ReferenceStatus::DataQualityUnknown);
    assert_eq!(check.observed_value(), None);
    assert_eq!(comparison.validate_snapshot(&s), Ok(()));
}

#[test]
fn observed_gex_numbers_from_distinct_scopes_do_not_certify_role_meaning() {
    let expiry = GexScope::Expiry("2026-12-18".to_owned());
    let raw = [
        level(610.0, 12.0, GexScope::Aggregate),
        level(610.0, -80.0, expiry.clone()),
    ];
    let s = snapshot("scopes", 600.0, DataQuality::Structured, &raw);
    let f = features(&s);
    let a = submit(&s, "aggregate-analyst", 0, GexLevelRole::Support);
    let b = submit(&s, "expiry-analyst", 1, GexLevelRole::Resistance);
    let comparison = ResearchGexRoleComparison::build(&s, vec![a, b]).unwrap();

    let aggregate = gex_level_reference(&GexScope::Aggregate, 610.0).unwrap();
    let specific = gex_level_reference(&expiry, 610.0).unwrap();
    assert_ne!(aggregate.reference, specific.reference);
    assert_eq!(
        resolve_reference(&s, &f, &aggregate)
            .unwrap()
            .observed_value(),
        Some(&ObservedValue::Gex(12.0))
    );
    assert_eq!(
        resolve_reference(&s, &f, &specific)
            .unwrap()
            .observed_value(),
        Some(&ObservedValue::Gex(-80.0))
    );
    for index in 0..2 {
        let observation = comparison.for_level(index).unwrap();
        assert_eq!(
            observation.status(),
            ResearchGexRoleObservationStatus::Unverified
        );
        assert_eq!(observation.claims().len(), 1);
        assert_eq!(s.gex_levels()[index].role, GexLevelRole::Unassigned);
    }
}

#[test]
fn source_revisions_cannot_reuse_numeric_evidence_or_role_review() {
    let raw = [level(610.0, 12.0, GexScope::Aggregate)];
    let original = snapshot("revised", 600.0, DataQuality::Structured, &raw);
    let f = features(&original);
    let report = ResearchGexRoleComparison::build(
        &original,
        vec![submit(&original, "agent-a", 0, GexLevelRole::Support)],
    )
    .unwrap();
    let revised_spot = snapshot("revised", 601.0, DataQuality::Structured, &raw);
    let revised_gex = snapshot(
        "revised",
        600.0,
        DataQuality::Structured,
        &[level(610.0, -12.0, GexScope::Aggregate)],
    );
    let reference = gex_level_reference(&GexScope::Aggregate, 610.0).unwrap();
    for revised in [&revised_spot, &revised_gex] {
        assert_eq!(original.snapshot_id(), revised.snapshot_id());
        assert_ne!(&original, revised);
        assert_eq!(
            report.validate_snapshot(revised),
            Err(ResearchGexRoleComparisonError::SnapshotContentsMismatch)
        );
        assert_eq!(
            resolve_reference(revised, &f, &reference),
            Err(EvidenceAuditError::FeatureSnapshotContentsMismatch)
        );
    }
}
