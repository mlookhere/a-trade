//! Cross-component research contract validation, not a strategy execution path.
//! Canonical §§5–6,11–16,111–116: fail closed and keep audit separate from trade ALLOW.

use gex_strategy::{
    AdvisoryDirection, AiNativeFeatureInput, AiNativeFeatureSet, DataQuality, DecisionVerdict,
    EvidenceAuditProvenanceError, EvidenceClaimInput, EvidenceKind, EvidenceReference,
    ExplicitAgeLimits, GexTimeObservation, HardCondition, HardVetoInput, HardVetoProvenanceError,
    HardVetoVerdict, MarketSnapshot, MarketSnapshotInput, ObservedValue, ReferenceStatus, Session,
    SnapshotTimingProvenanceError, SpecialistBatch, SpecialistContractError, SpecialistOutput,
    SpecialistOutputInput, SpecialistRole, SpecialistStatus, TemporalStatus,
    assess_snapshot_timing, audit_specialist_evidence, evaluate_hard_vetoes, resolve_reference,
};

fn snapshot(id: &str, spot: f64) -> MarketSnapshot {
    MarketSnapshot::build(
        MarketSnapshotInput {
            snapshot_id: id.to_owned(),
            symbol: "SPY".to_owned(),
            source_timestamp_millis: 1_000,
            received_timestamp_millis: 1_010,
            spot_price: spot,
            session: Session::Regular,
            data_quality: DataQuality::Structured,
        },
        &[],
    )
    .unwrap()
}

fn features(s: &MarketSnapshot) -> AiNativeFeatureSet {
    let mut input = AiNativeFeatureInput {
        snapshot_id: s.snapshot_id().to_owned(),
        ..Default::default()
    };
    input.trend.trend_strength = Some(3.5);
    AiNativeFeatureSet::build(s, input).unwrap()
}

fn specialist(s: &MarketSnapshot) -> SpecialistOutput {
    SpecialistOutput::build(
        s,
        SpecialistOutputInput {
            agent_id: "research-trend".to_owned(),
            role: SpecialistRole::TrendMarketState,
            snapshot_id: s.snapshot_id().to_owned(),
            status: SpecialistStatus::Ok,
            direction: AdvisoryDirection::Neutral,
            confidence: None,
            observations: vec!["Source field observed".to_owned()],
            features: vec![],
            supporting_evidence: vec![EvidenceClaimInput {
                claim: "Spot price is present".to_owned(),
                evidence: vec![EvidenceReference {
                    kind: EvidenceKind::SnapshotField,
                    reference: "spot_price".to_owned(),
                }],
                confidence: None,
            }],
            contradicting_evidence: vec![],
            unknowns: vec![],
            risks: vec![],
            proposed_invalidation: vec![],
            hard_veto: false,
            reason_codes: vec![],
        },
    )
    .unwrap()
}

fn gex_time(s: &MarketSnapshot) -> GexTimeObservation {
    GexTimeObservation {
        snapshot_id: s.snapshot_id().to_owned(),
        symbol: s.symbol().to_owned(),
        source_timestamp_millis: Some(1_002),
        received_timestamp_millis: Some(1_011),
        data_quality: DataQuality::Structured,
    }
}

fn limits() -> ExplicitAgeLimits {
    // Test-only caller configuration, NOT a production fallback or threshold.
    ExplicitAgeLimits {
        market_data_stale_timeout_ms: 20,
        gex_stale_timeout_ms: 18,
        max_decision_age_ms: 10,
    }
}

fn hard_conditions(id: &str, state: HardCondition) -> HardVetoInput {
    HardVetoInput {
        snapshot_id: id.to_owned(),
        data_valid: state,
        data_fresh: state,
        gex_scope_valid: state,
        required_regime_data_valid: state,
        broker_connected: state,
        order_state_valid: state,
        instrument_tradable: state,
        liquidity_within_limits: state,
        max_position_risk_valid: state,
        max_portfolio_risk_valid: state,
        invalidation_defined: state,
        execution_cost_within_limits: state,
        kill_switch_clear: state,
    }
}

#[test]
fn exact_source_research_reports_remain_observational_and_reject_cross_snapshot_reuse() {
    let s = snapshot("research-1", 600.0);
    let feat = features(&s);
    let output = specialist(&s);
    let batch = SpecialistBatch::build(&s, vec![output.clone()]).unwrap();
    let audit = audit_specialist_evidence(&s, &feat, &batch).unwrap();
    let time_input = gex_time(&s);
    let timing = assess_snapshot_timing(&s, Some(time_input.clone()), 1_020, &limits());
    let conditions = hard_conditions(s.snapshot_id(), HardCondition::Unknown);
    let veto = evaluate_hard_vetoes(&s, &conditions).unwrap();

    assert_eq!(audit.validate_inputs(&s, &feat, &batch), Ok(()));
    assert_eq!(
        timing.validate_inputs(&s, Some(&time_input), 1_020, &limits()),
        Ok(())
    );
    assert_eq!(veto.validate_inputs(&s, &conditions), Ok(()));
    assert!(timing.observed_clocks_within_limits());
    assert_eq!(veto.verdict(), HardVetoVerdict::Wait);
    assert_eq!(veto.blocking_decision(), Some(DecisionVerdict::Wait));
    assert_eq!(audit.specialists().len(), 1);
    assert_eq!(
        audit.specialists()[0].supporting()[0].references()[0]
            .observed_value()
            .cloned(),
        Some(ObservedValue::Price(600.0))
    );
    assert_eq!(
        resolve_reference(
            &s,
            &feat,
            &EvidenceReference {
                kind: EvidenceKind::DeterministicFeature,
                reference: "trend.ema_slope".to_owned(),
            },
        )
        .unwrap()
        .status(),
        ReferenceStatus::Missing
    );

    // Matching labels cannot substitute another complete source snapshot.
    let changed = snapshot("research-1", 601.0);
    assert_eq!(
        audit.validate_inputs(&changed, &feat, &batch),
        Err(EvidenceAuditProvenanceError::SnapshotContentsMismatch)
    );
    assert_eq!(
        timing.validate_inputs(&changed, Some(&time_input), 1_020, &limits()),
        Err(SnapshotTimingProvenanceError::SnapshotContentsMismatch)
    );
    assert_eq!(
        veto.validate_inputs(&changed, &conditions),
        Err(HardVetoProvenanceError::SnapshotContentsMismatch)
    );
    assert_eq!(
        SpecialistBatch::build(&changed, vec![output]),
        Err(SpecialistContractError::SnapshotContentsMismatch)
    );

    let foreign = snapshot("different-id", 600.0);
    assert_eq!(
        audit.validate_inputs(&foreign, &feat, &batch),
        Err(EvidenceAuditProvenanceError::SnapshotMismatch)
    );
    assert_eq!(
        timing.validate_inputs(&foreign, Some(&time_input), 1_020, &limits()),
        Err(SnapshotTimingProvenanceError::SnapshotMismatch)
    );
    assert_eq!(
        veto.validate_inputs(&foreign, &conditions),
        Err(HardVetoProvenanceError::SnapshotMismatch)
    );
    assert_eq!(
        timing.validate_inputs(&s, Some(&time_input), 1_021, &limits()),
        Err(SnapshotTimingProvenanceError::DecisionClockMismatch)
    );
    let revised_input = hard_conditions(s.snapshot_id(), HardCondition::Pass);
    assert_eq!(
        veto.validate_inputs(&s, &revised_input),
        Err(HardVetoProvenanceError::InputContentsMismatch)
    );
}

#[test]
fn missing_gex_and_empty_batch_are_not_implicit_freshness_or_trade_permission() {
    let s = snapshot("research-2", 600.0);
    let feat = features(&s);
    let empty = SpecialistBatch::build(&s, vec![]).unwrap();
    let audit = audit_specialist_evidence(&s, &feat, &empty).unwrap();
    assert!(audit.specialists().is_empty());
    assert_eq!(audit.validate_inputs(&s, &feat, &empty), Ok(()));

    let timing = assess_snapshot_timing(&s, None, 1_020, &limits());
    assert_eq!(timing.gex_data(), TemporalStatus::Missing);
    assert!(!timing.observed_clocks_within_limits());
    assert_eq!(timing.validate_inputs(&s, None, 1_020, &limits()), Ok(()));

    let all_pass = hard_conditions(s.snapshot_id(), HardCondition::Pass);
    let cleared = evaluate_hard_vetoes(&s, &all_pass).unwrap();
    assert_eq!(cleared.verdict(), HardVetoVerdict::Cleared);
    assert_eq!(cleared.blocking_decision(), None);
    // None is not Some(Allow), and cannot substitute for unavailable source
    // data, strategy sequence, centralized risk, or execution authority.
    assert_ne!(cleared.blocking_decision(), Some(DecisionVerdict::Allow));

    let failed = hard_conditions(s.snapshot_id(), HardCondition::Fail);
    let rejected = evaluate_hard_vetoes(&s, &failed).unwrap();
    assert_eq!(rejected.verdict(), HardVetoVerdict::Reject);
    assert_eq!(rejected.blocking_decision(), Some(DecisionVerdict::Reject));
    assert_eq!(
        cleared.validate_inputs(&s, &failed),
        Err(HardVetoProvenanceError::InputContentsMismatch)
    );
}
