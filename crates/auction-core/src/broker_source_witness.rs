//! Offline, source-identifiable broker-risk evidence witness inspection.
//!
//! Canonical §§9,16,65-70,103,109,122; see modules/02_architecture_agents.md,
//! modules/03_data_contracts.md, modules/07_execution_risk.md and
//! modules/11_state_validator_schemas.md.
//!
//! This checks structural consistency of *untrusted, caller-declared* source
//! metadata against the pre-existing non-authorizing BrokerPortfolioEvidence.
//! It cannot authenticate a broker/provider, establish data freshness,
//! reconcile asynchronous broker API responses, or grant an order permit.

use crate::{BrokerEvidenceError, BrokerPortfolioEvidence, Condition};

/// Caller-supplied provenance for one requested collection. None of these
/// fields independently proves that a broker served the response.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UntrustedCollectionWitness {
    pub source_account_id: String,
    /// Application-level aggregation/capture ID shared across this review.
    pub capture_id: String,
    /// Distinct provider response reference; its format is provider-defined.
    pub response_reference: String,
    /// Unverified timestamp. No freshness or clock-skew limit is inferred.
    pub observed_at_unix_ms: Option<u64>,
    /// An explicit caller assertion; even TRUE is not provider attestation.
    pub completeness: Condition,
    /// Explicit count including a genuine, reported zero; None is UNKNOWN.
    pub reported_items: Option<usize>,
}

/// No broker authentication semantics are implied by these text references.
#[derive(Debug, Clone)]
pub struct UntrustedBrokerSourceWitness {
    pub provider_reference: String,
    pub connection_reference: String,
    pub source_account_id: String,
    pub capture_id: String,
    pub account_observed_at_unix_ms: Option<u64>,
    pub positions: UntrustedCollectionWitness,
    pub open_orders: UntrustedCollectionWitness,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SourceWitnessError {
    MissingSource,
    MissingCapture,
    AccountMismatch,
    CaptureMismatch,
    MissingResponseReference,
    MissingTimestamp,
    CollectionUnknown,
    MissingItemCount,
    CountMismatch,
    InvalidAssertedRisk(BrokerEvidenceError),
}

/// Diagnostic-only output. Deliberately contains no trusted broker proof,
/// portfolio reservation, ExecutionPermit, or TRADE_ALLOWED decision.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnverifiedBrokerSourceReview {
    pub provider_reference: String,
    pub source_account_id: String,
    pub capture_id: String,
    pub position_items: usize,
    pub open_order_items: usize,
}

impl UntrustedBrokerSourceWitness {
    /// Checks declared source metadata and legacy asserted risk in one offline
    /// review. A successful review is STILL NOT authenticated broker evidence.
    pub fn review_offline(
        &self,
        evidence: &BrokerPortfolioEvidence,
    ) -> Result<UnverifiedBrokerSourceReview, SourceWitnessError> {
        if self.provider_reference.trim().is_empty()
            || self.connection_reference.trim().is_empty()
            || self.source_account_id.trim().is_empty()
        {
            return Err(SourceWitnessError::MissingSource);
        }
        if self.capture_id.trim().is_empty() {
            return Err(SourceWitnessError::MissingCapture);
        }
        if self.source_account_id != evidence.expected_account_id
            || self.source_account_id != evidence.observed_account_id
        {
            return Err(SourceWitnessError::AccountMismatch);
        }
        if self.capture_id != evidence.snapshot_id {
            return Err(SourceWitnessError::CaptureMismatch);
        }
        if !timestamp_present(self.account_observed_at_unix_ms) {
            return Err(SourceWitnessError::MissingTimestamp);
        }

        self.check_collection(&self.positions, evidence.positions.len())?;
        self.check_collection(&self.open_orders, evidence.open_orders.len())?;

        // Reuse canonical asserted exposure, duplicate-setup, aggregate-risk,
        // and directional-cluster validation; never implement another risk
        // calculator or convert these assertions to an authorization token.
        evidence
            .clone()
            .into_asserted_snapshot()
            .map_err(SourceWitnessError::InvalidAssertedRisk)?;

        Ok(UnverifiedBrokerSourceReview {
            provider_reference: self.provider_reference.clone(),
            source_account_id: self.source_account_id.clone(),
            capture_id: self.capture_id.clone(),
            position_items: evidence.positions.len(),
            open_order_items: evidence.open_orders.len(),
        })
    }

    fn check_collection(
        &self,
        source: &UntrustedCollectionWitness,
        observed_items: usize,
    ) -> Result<(), SourceWitnessError> {
        if source.source_account_id != self.source_account_id {
            return Err(SourceWitnessError::AccountMismatch);
        }
        if source.capture_id != self.capture_id {
            return Err(SourceWitnessError::CaptureMismatch);
        }
        if source.response_reference.trim().is_empty() {
            return Err(SourceWitnessError::MissingResponseReference);
        }
        if !timestamp_present(source.observed_at_unix_ms) {
            return Err(SourceWitnessError::MissingTimestamp);
        }
        if source.completeness != Condition::True {
            return Err(SourceWitnessError::CollectionUnknown);
        }
        let count = source
            .reported_items
            .ok_or(SourceWitnessError::MissingItemCount)?;
        if count != observed_items {
            return Err(SourceWitnessError::CountMismatch);
        }
        Ok(())
    }
}

fn timestamp_present(value: Option<u64>) -> bool {
    matches!(value, Some(t) if t > 0)
}
