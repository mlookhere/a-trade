use gex_strategy::{
    AdvisoryDirection, AiNativeFeatureInput, AiNativeFeatureSet, ClaimObservationStatus,
    DataQuality, EvidenceAuditError, EvidenceClaimInput, EvidenceKind, EvidenceReference,
    FeatureReference, FeatureSource, GexScope, MarketSnapshot, MarketSnapshotInput, ObservedValue,
    RawGexLevel, ReferenceStatus, Session, SpecialistBatch, SpecialistOutput,
    SpecialistOutputInput, SpecialistRole, SpecialistStatus, audit_specialist_evidence,
    gex_level_reference, resolve_feature_reference, resolve_reference,
};

fn snapshot(id: &str, quality: DataQuality, levels: &[RawGexLevel]) -> MarketSnapshot {
    MarketSnapshot::build(
        MarketSnapshotInput {
            snapshot_id: id.to_owned(),
            symbol: "SPY".to_owned(),
            source_timestamp_millis: 1_000,
            received_timestamp_millis: 1_012,
            spot_price: 600.0,
            session: Session::Regular,
            data_quality: quality,
        },
        levels,
    )
    .unwrap()
}

fn features(snapshot: &MarketSnapshot) -> AiNativeFeatureSet {
    let mut input = AiNativeFeatureInput {
        snapshot_id: snapshot.snapshot_id().to_owned(),
        ..Default::default()
    };
    input.trend.trend_strength = Some(3.5);
    input.participation.relative_volume = Some(2.0);
    AiNativeFeatureSet::build(snapshot, input).unwrap()
}

fn reference(kind: EvidenceKind, key: &str) -> EvidenceReference {
    EvidenceReference {
        kind,
        reference: key.to_owned(),
    }
}

fn specialist(snapshot: &MarketSnapshot, reference: EvidenceReference) -> SpecialistOutput {
    SpecialistOutput::build(
        snapshot,
        SpecialistOutputInput {
            agent_id: "gex-agent".to_owned(),
            role: SpecialistRole::GexDealerPositioning,
            snapshot_id: snapshot.snapshot_id().to_owned(),
            status: SpecialistStatus::Ok,
            direction: AdvisoryDirection::Neutral,
            confidence: None,
            observations: Vec::new(),
            features: vec![FeatureReference {
                source: FeatureSource::Trend,
                key: "trend.trend_strength".to_owned(),
            }],
            supporting_evidence: vec![EvidenceClaimInput {
                claim: "A level is present".to_owned(),
                evidence: vec![reference],
                confidence: Some(0.8),
            }],
            contradicting_evidence: vec![EvidenceClaimInput {
                claim: "No underlying reason is proven".to_owned(),
                evidence: Vec::new(),
                confidence: Some(0.4),
            }],
            unknowns: Vec::new(),
            risks: Vec::new(),
            proposed_invalidation: Vec::new(),
            hard_veto: false,
            reason_codes: Vec::new(),
        },
    )
    .unwrap()
}

#[test]
fn observed_snapshot_and_feature_values_are_real() {
    let snap = snapshot("s1", DataQuality::Structured, &[]);
    let vals = features(&snap);
    for (key, expected) in [
        ("spot_price", ObservedValue::Price(600.0)),
        (
            "source_timestamp_millis",
            ObservedValue::TimestampMillis(1_000),
        ),
        (
            "received_timestamp_millis",
            ObservedValue::TimestampMillis(1_012),
        ),
        ("data_age_millis", ObservedValue::AgeMillis(12)),
    ] {
        let actual =
            resolve_reference(&snap, &vals, &reference(EvidenceKind::SnapshotField, key)).unwrap();
        assert_eq!(actual.status, ReferenceStatus::Observed);
        assert_eq!(actual.observed_value, Some(expected));
    }
    let observed = resolve_reference(
        &snap,
        &vals,
        &reference(EvidenceKind::DeterministicFeature, "trend.trend_strength"),
    )
    .unwrap();
    assert_eq!(observed.observed_value, Some(ObservedValue::Feature(3.5)));
    let missing = resolve_reference(
        &snap,
        &vals,
        &reference(EvidenceKind::DeterministicFeature, "trend.ema_slope"),
    )
    .unwrap();
    assert_eq!(missing.status, ReferenceStatus::Missing);
    assert_eq!(missing.observed_value, None);
}

#[test]
fn unsupported_fields_and_wrong_feature_groups_cannot_be_laundered() {
    let snap = snapshot("s1", DataQuality::Structured, &[]);
    let vals = features(&snap);
    let unknown = resolve_reference(
        &snap,
        &vals,
        &reference(EvidenceKind::SnapshotField, "future_price"),
    )
    .unwrap();
    assert_eq!(unknown.status, ReferenceStatus::Unsupported);
    let unsupported = resolve_reference(
        &snap,
        &vals,
        &reference(EvidenceKind::Catalyst, "breaking-news"),
    )
    .unwrap();
    assert_eq!(unsupported.status, ReferenceStatus::Unsupported);
    let wrong = resolve_feature_reference(
        &snap,
        &vals,
        &FeatureReference {
            source: FeatureSource::Gex,
            key: "trend.trend_strength".to_owned(),
        },
    )
    .unwrap();
    assert_eq!(wrong.status, ReferenceStatus::Unsupported);
    let correct = resolve_feature_reference(
        &snap,
        &vals,
        &FeatureReference {
            source: FeatureSource::Trend,
            key: "trend.trend_strength".to_owned(),
        },
    )
    .unwrap();
    assert_eq!(correct.observed_value, Some(ObservedValue::Feature(3.5)));
    let wrong_group = resolve_feature_reference(
        &snap,
        &vals,
        &FeatureReference {
            source: FeatureSource::PortfolioBroker,
            key: "trend.trend_strength".to_owned(),
        },
    )
    .unwrap();
    assert_eq!(wrong_group.status, ReferenceStatus::Unsupported);
}

#[test]
fn gex_reference_requires_exact_scope_and_existing_level() {
    let raw = vec![
        RawGexLevel {
            strike: 610.0,
            gex_value: -25.0,
            scope: GexScope::Aggregate,
        },
        RawGexLevel {
            strike: 610.0,
            gex_value: 6.0,
            scope: GexScope::Expiry("2026-10-30".to_owned()),
        },
    ];
    let snap = snapshot("s1", DataQuality::Structured, &raw);
    let vals = features(&snap);
    let aggregate = gex_level_reference(&GexScope::Aggregate, 610.0).unwrap();
    let expiry = gex_level_reference(&GexScope::Expiry("2026-10-30".to_owned()), 610.0).unwrap();
    assert_ne!(aggregate.reference, expiry.reference);
    assert_eq!(
        resolve_reference(&snap, &vals, &aggregate)
            .unwrap()
            .observed_value,
        Some(ObservedValue::Gex(-25.0))
    );
    assert_eq!(
        resolve_reference(&snap, &vals, &expiry)
            .unwrap()
            .observed_value,
        Some(ObservedValue::Gex(6.0))
    );
    assert_eq!(
        resolve_reference(
            &snap,
            &vals,
            &reference(EvidenceKind::GexLevel, "aggregate:610"),
        )
        .unwrap()
        .status,
        ReferenceStatus::Missing
    );
    assert_eq!(
        resolve_reference(
            &snap,
            &vals,
            &gex_level_reference(&GexScope::Expiry("2026-10-31".to_owned()), 610.0).unwrap(),
        )
        .unwrap()
        .status,
        ReferenceStatus::Missing
    );
    assert_eq!(
        gex_level_reference(&GexScope::Aggregate, f64::NAN),
        Err(EvidenceAuditError::InvalidGexIdentity)
    );
}

#[test]
fn ambiguous_duplicate_level_is_never_claimed_as_observed() {
    let raw = vec![
        RawGexLevel {
            strike: 610.0,
            gex_value: 7.0,
            scope: GexScope::Aggregate,
        },
        RawGexLevel {
            strike: 610.0,
            gex_value: 8.0,
            scope: GexScope::Aggregate,
        },
    ];
    let snap = snapshot("s1", DataQuality::Structured, &raw);
    let vals = features(&snap);
    let r = gex_level_reference(&GexScope::Aggregate, 610.0).unwrap();
    let result = resolve_reference(&snap, &vals, &r).unwrap();
    assert_eq!(result.status, ReferenceStatus::Ambiguous);
    assert_eq!(result.observed_value, None);
}

#[test]
fn unknown_data_quality_prevents_certifying_any_reference() {
    let snap = snapshot("s1", DataQuality::Unknown, &[]);
    let vals = features(&snap);
    let r = reference(EvidenceKind::SnapshotField, "spot_price");
    let result = resolve_reference(&snap, &vals, &r).unwrap();
    assert_eq!(result.status, ReferenceStatus::DataQualityUnknown);
    assert_eq!(result.observed_value, None);
}

#[test]
fn mismatched_snapshot_feature_ids_fail_closed() {
    let one = snapshot("s1", DataQuality::Structured, &[]);
    let other = snapshot("s2", DataQuality::Structured, &[]);
    let vals = features(&other);
    let result = resolve_reference(
        &one,
        &vals,
        &reference(EvidenceKind::SnapshotField, "spot_price"),
    );
    assert_eq!(result, Err(EvidenceAuditError::FeatureSnapshotMismatch));
}

#[test]
fn batch_audits_claims_without_promoting_text_or_unverified_freshness() {
    let raw = vec![RawGexLevel {
        strike: 610.0,
        gex_value: 40.0,
        scope: GexScope::Aggregate,
    }];
    let snap = snapshot("s1", DataQuality::Structured, &raw);
    let vals = features(&snap);
    let r = gex_level_reference(&GexScope::Aggregate, 610.0).unwrap();
    let output = specialist(&snap, r);
    let batch = SpecialistBatch::build(&snap, vec![output]).unwrap();
    let report = audit_specialist_evidence(&snap, &vals, &batch).unwrap();
    assert_eq!(report.snapshot_id, "s1");
    assert_eq!(report.specialists.len(), 1);
    let item = &report.specialists[0];
    assert_eq!(
        item.feature_references[0].observed_value,
        Some(ObservedValue::Feature(3.5))
    );
    assert_eq!(
        item.supporting[0].status,
        ClaimObservationStatus::ObservedReferencesOnly
    );
    assert_eq!(
        item.contradicting[0].status,
        ClaimObservationStatus::Unknown
    );
    assert_eq!(
        item.supporting[0].references[0].observed_value,
        Some(ObservedValue::Gex(40.0))
    );
    // No ALLOW, broker execution, calibrated probability or claim-semantic proof exists.
}

#[test]
fn mixed_snapshot_batch_fails_closed_even_when_references_are_valid() {
    let snap = snapshot("s1", DataQuality::Structured, &[]);
    let other = snapshot("s2", DataQuality::Structured, &[]);
    let vals = features(&snap);
    let batch = SpecialistBatch::build(
        &other,
        vec![specialist(
            &other,
            reference(EvidenceKind::SnapshotField, "spot_price"),
        )],
    )
    .unwrap();
    assert_eq!(
        audit_specialist_evidence(&snap, &vals, &batch),
        Err(EvidenceAuditError::SnapshotMismatch)
    );
}

#[test]
fn same_id_revised_snapshot_cannot_launder_research_feature_evidence() {
    // Canonical §§11,16,116: provenance consistency only, never trade authorization.
    let original = snapshot("same-id", DataQuality::Structured, &[]);
    let original_features = features(&original);
    let reference = reference(EvidenceKind::SnapshotField, "spot_price");
    let invalid_group = FeatureReference {
        source: FeatureSource::Trend,
        key: "portfolio_broker.daily_loss".to_owned(),
    };
    let original_batch =
        SpecialistBatch::build(&original, vec![specialist(&original, reference.clone())]).unwrap();
    let original_empty = SpecialistBatch::build(&original, vec![]).unwrap();

    assert_eq!(original_features.source_snapshot(), &original);
    assert_eq!(original_batch.source_snapshot(), &original);
    assert_eq!(original_empty.source_snapshot(), &original);
    assert!(resolve_reference(&original, &original_features, &reference).is_ok());
    assert!(audit_specialist_evidence(&original, &original_features, &original_batch).is_ok());
    assert!(audit_specialist_evidence(&original, &original_features, &original_empty).is_ok());

    let base = MarketSnapshotInput {
        snapshot_id: "same-id".to_owned(),
        symbol: "SPY".to_owned(),
        source_timestamp_millis: 1_000,
        received_timestamp_millis: 1_012,
        spot_price: 600.0,
        session: Session::Regular,
        data_quality: DataQuality::Structured,
    };
    let revised_inputs = [
        (
            MarketSnapshotInput {
                source_timestamp_millis: 999,
                ..base.clone()
            },
            false,
        ),
        (
            MarketSnapshotInput {
                received_timestamp_millis: 1_013,
                ..base.clone()
            },
            false,
        ),
        (
            MarketSnapshotInput {
                spot_price: 601.0,
                ..base.clone()
            },
            false,
        ),
        (
            MarketSnapshotInput {
                session: Session::Premarket,
                ..base.clone()
            },
            false,
        ),
        (
            MarketSnapshotInput {
                data_quality: DataQuality::Parsed,
                ..base.clone()
            },
            false,
        ),
        (
            MarketSnapshotInput {
                data_quality: DataQuality::Unknown,
                ..base.clone()
            },
            false,
        ),
        (base, true),
    ];
    let gex = [RawGexLevel {
        strike: 610.0,
        gex_value: 20.0,
        scope: GexScope::Aggregate,
    }];
    for (input, with_gex) in revised_inputs {
        let levels: &[RawGexLevel] = if with_gex { &gex } else { &[] };
        let revised = MarketSnapshot::build(input, levels).unwrap();
        assert_eq!(revised.snapshot_id(), original.snapshot_id());
        assert_ne!(revised, original);
        let revised_features = features(&revised);
        let revised_empty = SpecialistBatch::build(&revised, vec![]).unwrap();
        let revised_batch =
            SpecialistBatch::build(&revised, vec![specialist(&revised, reference.clone())])
                .unwrap();

        assert_eq!(
            resolve_reference(&original, &revised_features, &reference),
            Err(EvidenceAuditError::FeatureSnapshotContentsMismatch)
        );
        assert_eq!(
            resolve_feature_reference(&original, &revised_features, &invalid_group),
            Err(EvidenceAuditError::FeatureSnapshotContentsMismatch)
        );
        assert_eq!(
            audit_specialist_evidence(&original, &revised_features, &original_empty),
            Err(EvidenceAuditError::FeatureSnapshotContentsMismatch)
        );
        assert_eq!(
            audit_specialist_evidence(&original, &original_features, &revised_empty),
            Err(EvidenceAuditError::SpecialistSnapshotContentsMismatch)
        );
        assert_eq!(
            audit_specialist_evidence(&original, &original_features, &revised_batch),
            Err(EvidenceAuditError::SpecialistSnapshotContentsMismatch)
        );
    }

    let foreign = snapshot("other-id", DataQuality::Structured, &[]);
    assert_eq!(
        resolve_reference(&original, &features(&foreign), &reference),
        Err(EvidenceAuditError::FeatureSnapshotMismatch)
    );
    let foreign_batch = SpecialistBatch::build(&foreign, vec![]).unwrap();
    assert_eq!(
        audit_specialist_evidence(&original, &original_features, &foreign_batch),
        Err(EvidenceAuditError::SnapshotMismatch)
    );
}
