use gex_strategy::{
    AdvisoryDirection, ClaimStatus, DataQuality, EvidenceClaimInput, EvidenceKind,
    EvidenceReference, FeatureReference, FeatureSource, GexScope, InternalConfidence, MarketSnapshot,
    MarketSnapshotInput, RawGexLevel, Session, SpecialistBatch, SpecialistContractError, SpecialistOutput,
    SpecialistOutputInput, SpecialistRole, SpecialistStatus,
};

fn snapshot(id: &str) -> MarketSnapshot {
    MarketSnapshot::build(
        MarketSnapshotInput {
            snapshot_id: id.to_owned(),
            symbol: "SPY".to_owned(),
            source_timestamp_millis: 1_000,
            received_timestamp_millis: 1_010,
            spot_price: 600.0,
            session: Session::Regular,
            data_quality: DataQuality::Structured,
        },
        &[],
    )
    .unwrap()
}

fn input(agent_id: &str, snapshot_id: &str) -> SpecialistOutputInput {
    SpecialistOutputInput {
        agent_id: agent_id.to_owned(),
        role: SpecialistRole::GexDealerPositioning,
        snapshot_id: snapshot_id.to_owned(),
        status: SpecialistStatus::Ok,
        direction: AdvisoryDirection::Neutral,
        confidence: Some(0.8),
        observations: vec!["A reported concentration above spot".to_owned()],
        features: vec![FeatureReference {
            source: FeatureSource::Gex,
            key: "aggregate:610".to_owned(),
        }],
        supporting_evidence: vec![EvidenceClaimInput {
            claim: "610 might be relevant".to_owned(),
            evidence: vec![EvidenceReference {
                kind: EvidenceKind::GexLevel,
                reference: "aggregate:610".to_owned(),
            }],
            confidence: Some(0.7),
        }],
        contradicting_evidence: Vec::new(),
        unknowns: Vec::new(),
        risks: Vec::new(),
        proposed_invalidation: Vec::new(),
        hard_veto: false,
        reason_codes: Vec::new(),
    }
}

#[test]
fn every_role_is_supported_by_one_shared_contract() {
    let market = snapshot("s1");
    let roles = [
        SpecialistRole::TrendMarketState,
        SpecialistRole::GexDealerPositioning,
        SpecialistRole::PriceStructureVolatility,
        SpecialistRole::VolumeFlowLiquidity,
        SpecialistRole::CrossMarketContext,
        SpecialistRole::CatalystEvent,
        SpecialistRole::InstrumentExecution,
        SpecialistRole::PortfolioRisk,
    ];
    for (i, role) in roles.into_iter().enumerate() {
        let mut data = input(&format!("agent-{i}"), "s1");
        data.role = role;
        let output = SpecialistOutput::build(&market, data).unwrap();
        assert_eq!(output.role(), role);
        assert_eq!(output.snapshot_id(), "s1");
    }
}

#[test]
fn unknown_or_wrong_snapshot_and_agent_id_are_rejected() {
    let market = snapshot("s1");
    assert_eq!(
        SpecialistOutput::build(&market, input("gex", "")),
        Err(SpecialistContractError::MissingSnapshotId)
    );
    assert_eq!(
        SpecialistOutput::build(&market, input("gex", "s2")),
        Err(SpecialistContractError::SnapshotMismatch)
    );
    assert_eq!(
        SpecialistOutput::build(&market, input(" ", "s1")),
        Err(SpecialistContractError::MissingAgentId)
    );
}

#[test]
fn confidence_is_finite_but_never_interpreted_as_probability() {
    assert_eq!(
        InternalConfidence::new(f64::NAN),
        Err(SpecialistContractError::NonFiniteConfidence)
    );
    assert_eq!(
        InternalConfidence::new(f64::INFINITY),
        Err(SpecialistContractError::NonFiniteConfidence)
    );
    assert_eq!(InternalConfidence::new(-2.0).unwrap().value(), -2.0);
    assert_eq!(InternalConfidence::new(5.0).unwrap().value(), 5.0);
    let market = snapshot("s1");
    let mut data = input("agent", "s1");
    data.confidence = Some(f64::NAN);
    assert_eq!(
        SpecialistOutput::build(&market, data),
        Err(SpecialistContractError::NonFiniteConfidence)
    );
    let mut data = input("agent", "s1");
    data.supporting_evidence[0].confidence = Some(f64::INFINITY);
    assert_eq!(
        SpecialistOutput::build(&market, data),
        Err(SpecialistContractError::NonFiniteConfidence)
    );
}

#[test]
fn evidence_free_claim_is_unknown_and_loses_unsupported_confidence() {
    let market = snapshot("s1");
    let mut data = input("agent", "s1");
    data.supporting_evidence = vec![EvidenceClaimInput {
        claim: "Unsupported conclusion".to_owned(),
        evidence: Vec::new(),
        confidence: Some(0.99),
    }];
    let result = SpecialistOutput::build(&market, data).unwrap();
    let claim = &result.supporting_evidence()[0];
    assert_eq!(claim.status(), ClaimStatus::Unknown);
    assert_eq!(claim.confidence(), None);
}

#[test]
fn nonempty_evidence_reference_is_not_certified_as_supported() {
    let market = snapshot("s1");
    let result = SpecialistOutput::build(&market, input("agent", "s1")).unwrap();
    let claim = &result.supporting_evidence()[0];
    assert_eq!(claim.status(), ClaimStatus::ReferencedUnverified);
    assert_eq!(claim.evidence()[0].reference, "aggregate:610");
    // This type cannot prove a referenced GEX level exists in the authoritative snapshot.
    assert!(market.gex_levels().is_empty());
}

#[test]
fn blank_evidence_and_feature_references_fail_closed() {
    let market = snapshot("s1");
    let mut data = input("agent", "s1");
    data.features[0].key = " ".to_owned();
    assert_eq!(
        SpecialistOutput::build(&market, data),
        Err(SpecialistContractError::BlankFeatureReference)
    );
    let mut data = input("agent", "s1");
    data.supporting_evidence[0].evidence[0].reference = " ".to_owned();
    assert_eq!(
        SpecialistOutput::build(&market, data),
        Err(SpecialistContractError::BlankEvidenceReference)
    );
    let mut data = input("agent", "s1");
    data.supporting_evidence[0].claim = "".to_owned();
    assert_eq!(
        SpecialistOutput::build(&market, data),
        Err(SpecialistContractError::BlankClaim)
    );
}

#[test]
fn no_blank_structural_text_is_accepted() {
    let market = snapshot("s1");
    let mut cases: Vec<(SpecialistOutputInput, SpecialistContractError)> = Vec::new();
    let mut input_observation = input("a", "s1");
    input_observation.observations.push(" ".to_owned());
    cases.push((input_observation, SpecialistContractError::BlankObservation));
    let mut input_unknown = input("b", "s1");
    input_unknown.unknowns.push(" ".to_owned());
    cases.push((input_unknown, SpecialistContractError::BlankUnknown));
    let mut input_risk = input("c", "s1");
    input_risk.risks.push(" ".to_owned());
    cases.push((input_risk, SpecialistContractError::BlankRisk));
    let mut input_invalidation = input("d", "s1");
    input_invalidation
        .proposed_invalidation
        .push(" ".to_owned());
    cases.push((
        input_invalidation,
        SpecialistContractError::BlankInvalidation,
    ));
    let mut input_reason = input("e", "s1");
    input_reason.reason_codes.push(" ".to_owned());
    cases.push((input_reason, SpecialistContractError::BlankReasonCode));
    for (data, error) in cases {
        assert_eq!(SpecialistOutput::build(&market, data), Err(error));
    }
}

#[test]
fn reported_veto_and_conflicting_directions_remain_advisory_only() {
    let market = snapshot("s1");
    let mut long = input("trend", "s1");
    long.direction = AdvisoryDirection::Long;
    long.hard_veto = true;
    let mut short = input("flow", "s1");
    short.direction = AdvisoryDirection::Short;
    let batch = SpecialistBatch::build(
        &market,
        vec![
            SpecialistOutput::build(&market, long).unwrap(),
            SpecialistOutput::build(&market, short).unwrap(),
        ],
    )
    .unwrap();
    assert_eq!(batch.outputs().len(), 2);
    assert_eq!(batch.outputs()[0].direction(), AdvisoryDirection::Long);
    assert_eq!(batch.outputs()[1].direction(), AdvisoryDirection::Short);
    assert!(batch.outputs()[0].hard_veto_reported());
    // Deliberately no batch direction, voting, P(win), ALLOW or broker submission API.
}

#[test]
fn mixing_snapshots_or_reusing_agent_id_fails_closed() {
    let market = snapshot("s1");
    let other = snapshot("s2");
    let first = SpecialistOutput::build(&market, input("gex", "s1")).unwrap();
    let duplicate = SpecialistOutput::build(&market, input("gex", "s1")).unwrap();
    assert_eq!(
        SpecialistBatch::build(&market, vec![first.clone(), duplicate]),
        Err(SpecialistContractError::DuplicateAgentId)
    );
    let mismatched = SpecialistOutput::build(&other, input("other", "s2")).unwrap();
    assert_eq!(
        SpecialistBatch::build(&market, vec![first, mismatched]),
        Err(SpecialistContractError::SnapshotMismatch)
    );
    assert_eq!(
        SpecialistBatch::build(&market, vec![])
            .unwrap()
            .outputs()
            .len(),
        0
    );
}

#[test]
fn specialist_batches_reject_distinct_complete_snapshots_with_the_same_id() {
    // Canonical §§11,16: missing/conflicting market evidence must never be inferred as valid.
    // Research specialist observations are advisory, not trade authorizations.
    let original = snapshot("s1");
    let original_output =
        SpecialistOutput::build(&original, input("original", "s1")).unwrap();
    assert_eq!(original_output.source_snapshot(), &original);
    assert!(SpecialistBatch::build(&original, vec![original_output.clone()]).is_ok());

    let base = MarketSnapshotInput {
        snapshot_id: "s1".to_owned(),
        symbol: "SPY".to_owned(),
        source_timestamp_millis: 1_000,
        received_timestamp_millis: 1_010,
        spot_price: 600.0,
        session: Session::Regular,
        data_quality: DataQuality::Structured,
    };
    let changes = [
        (
            MarketSnapshotInput {
                source_timestamp_millis: 999,
                ..base.clone()
            },
            false,
        ),
        (
            MarketSnapshotInput {
                received_timestamp_millis: 1_011,
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
        (base, true),
    ];

    for (changed, has_gex) in changes {
        let levels = [RawGexLevel {
            strike: 610.0,
            gex_value: 100.0,
            scope: GexScope::Aggregate,
        }];
        let gex: &[RawGexLevel] = if has_gex { &levels } else { &[] };
        let revised = MarketSnapshot::build(changed, gex).unwrap();
        assert_eq!(revised.snapshot_id(), original.snapshot_id());
        assert_eq!(revised.symbol(), original.symbol());
        assert_ne!(revised, original);

        let revised_output = SpecialistOutput::build(&revised, input("revised", "s1")).unwrap();
        assert_eq!(revised_output.source_snapshot(), &revised);

        assert_eq!(
            SpecialistBatch::build(&original, vec![original_output.clone(), revised_output]),
            Err(SpecialistContractError::SnapshotContentsMismatch)
        );
        assert_eq!(
            SpecialistBatch::build(&revised, vec![original_output.clone()]),
            Err(SpecialistContractError::SnapshotContentsMismatch)
        );
    }

    // Distinct IDs and duplicate-agent errors retain their existing precedence.
    let foreign = snapshot("other-id");
    assert_eq!(
        SpecialistBatch::build(
            &original,
            vec![SpecialistOutput::build(&foreign, input("foreign", "other-id")).unwrap()]
        ),
        Err(SpecialistContractError::SnapshotMismatch)
    );
    assert_eq!(
        SpecialistBatch::build(&original, vec![original_output.clone(), original_output]),
        Err(SpecialistContractError::DuplicateAgentId)
    );
}
