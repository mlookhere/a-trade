//! Offline process registry for one asserted account risk book per source baseline.
//! Canonical §§9,16,67-70,103,109. No broker authentication, persistence,
//! process-global singleton, live order permission, or cross-process safety.

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

use crate::{
    AccountScopedPortfolioReservations, BrokerEvidenceError, BrokerPortfolioEvidence, Direction,
    RiskExposure, ScopedReservationError,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProcessRiskRegistryError {
    InvalidEvidence(BrokerEvidenceError),
    InvalidBook(ScopedReservationError),
    ConflictingEvidence,
    Unavailable,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ExposureSignature {
    setup_id: String,
    cluster_id: String,
    direction: Direction,
    risk_bits: u64,
}

fn exposure_signature(exposures: &[RiskExposure]) -> Vec<ExposureSignature> {
    let mut sorted: Vec<_> = exposures
        .iter()
        .map(|item| ExposureSignature {
            setup_id: item.setup_id.clone(),
            cluster_id: item.cluster_id.clone(),
            direction: item.direction,
            risk_bits: item.risk_dollars.to_bits(),
        })
        .collect();
    sorted.sort_by(|a, b| a.setup_id.cmp(&b.setup_id));
    sorted
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct InitialEvidenceSignature {
    snapshot_id: String,
    equity_bits: u64,
    portfolio_limit_bits: u64,
    cluster_limit_bits: u64,
    positions: Vec<ExposureSignature>,
    open_orders: Vec<ExposureSignature>,
}

impl InitialEvidenceSignature {
    fn from_validated(evidence: &BrokerPortfolioEvidence) -> Self {
        Self {
            snapshot_id: evidence.snapshot_id.clone(),
            equity_bits: evidence.account_equity.to_bits(),
            portfolio_limit_bits: evidence.limits.max_portfolio_open_risk_percent.to_bits(),
            cluster_limit_bits: evidence.limits.max_cluster_open_risk_percent.to_bits(),
            positions: exposure_signature(&evidence.positions),
            open_orders: exposure_signature(&evidence.open_orders),
        }
    }
}

#[derive(Debug)]
struct RegistryEntry {
    initial: InitialEvidenceSignature,
    ledger: AccountScopedPortfolioReservations,
    invalidated: bool,
}

/// Explicitly shared in-process registry. Every participant must be passed
/// the same registry instance; separate instances/processes do NOT coordinate.
/// No call in this module authenticates the broker or authorizes a trade.
#[derive(Debug, Clone, Default)]
pub struct ProcessAccountRiskRegistry {
    books: Arc<Mutex<BTreeMap<String, RegistryEntry>>>,
}

impl ProcessAccountRiskRegistry {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Validates asserted account/position/order evidence before acquiring
    /// a registry lock. Identical replay returns a clone of the SAME ledger.
    /// Conflicting evidence permanently invalidates the old book and never
    /// installs a replacement or releases its existing reservations.
    pub fn get_or_initialize(
        &self,
        evidence: BrokerPortfolioEvidence,
    ) -> Result<AccountScopedPortfolioReservations, ProcessRiskRegistryError> {
        let validated = evidence
            .clone()
            .into_asserted_snapshot()
            .map_err(ProcessRiskRegistryError::InvalidEvidence)?;
        let account_id = validated.account_id;
        let signature = InitialEvidenceSignature::from_validated(&evidence);
        let mut books = self
            .books
            .lock()
            .map_err(|_| ProcessRiskRegistryError::Unavailable)?;

        if let Some(existing) = books.get_mut(&account_id) {
            if existing.invalidated {
                return Err(ProcessRiskRegistryError::Unavailable);
            }
            if existing.initial != signature {
                // Canonical §16: contradictory evidence cannot let a stale
                // coordinator keep allocating risk while another is rejected.
                existing.invalidated = true;
                existing
                    .ledger
                    .invalidate()
                    .map_err(|_| ProcessRiskRegistryError::Unavailable)?;
                return Err(ProcessRiskRegistryError::ConflictingEvidence);
            }
            return Ok(existing.ledger.clone());
        }

        let ledger = AccountScopedPortfolioReservations::from_evidence(evidence)
            .map_err(ProcessRiskRegistryError::InvalidBook)?;
        books.insert(
            account_id,
            RegistryEntry {
                initial: signature,
                ledger: ledger.clone(),
                invalidated: false,
            },
        );
        Ok(ledger)
    }
}
