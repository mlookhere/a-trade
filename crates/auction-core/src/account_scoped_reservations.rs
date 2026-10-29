//! Account-bound, in-process reservation proof for canonical §§9,16,67-70,103,109.
//! All broker/account health remains caller asserted. This is not a
//! broker-authenticated, durable, cross-process, or live execution gate.

use crate::{
    AssertedAccountRiskSnapshot, BrokerEvidenceError, BrokerPortfolioEvidence, OrderProposal,
    PortfolioLedgerAuditView, ReservationError, ReservationEvidence, SharedPortfolioReservations,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScopedReservationError {
    Evidence(BrokerEvidenceError),
    AccountMismatch,
    Reservation(ReservationError),
}

/// An accepted in-process reservation with its original asserted account.
#[derive(Debug, Clone, PartialEq)]
pub struct ScopedReservationEvidence {
    pub account_id: String,
    pub reservation: ReservationEvidence,
}

/// A coherent read-only view; neither an authenticated broker snapshot nor
/// a durable/replayable recovery checkpoint.
#[derive(Debug, Clone)]
pub struct ScopedPortfolioAuditView {
    pub account_id: String,
    pub ledger: PortfolioLedgerAuditView,
}

/// Keeps the caller-asserted account identity attached to the isolated risk
/// ledger and every reservation/read-only observation. No unscoped ledger
/// handle is exported from this wrapper.
#[derive(Debug, Clone)]
pub struct AccountScopedPortfolioReservations {
    account_id: String,
    ledger: SharedPortfolioReservations,
}

impl AccountScopedPortfolioReservations {
    /// All input status flags are still untrusted caller assertions. The
    /// underlying broker identity and freshness must be verified elsewhere.
    pub fn from_evidence(
        evidence: BrokerPortfolioEvidence,
    ) -> Result<Self, ScopedReservationError> {
        let AssertedAccountRiskSnapshot {
            account_id,
            risk_snapshot,
        } = evidence
            .into_asserted_snapshot()
            .map_err(ScopedReservationError::Evidence)?;
        let ledger = SharedPortfolioReservations::from_snapshot(risk_snapshot)
            .map_err(ScopedReservationError::Reservation)?;
        Ok(Self { account_id, ledger })
    }

    /// Matching text is provenance consistency only, NOT account
    /// authentication. Shared portfolio risk checks remain atomic in the
    /// existing in-process ledger. No order execution is authorized.
    pub fn reserve(
        &self,
        proposal: &OrderProposal,
        owner: &str,
        expected_account_id: &str,
        expected_snapshot_id: &str,
    ) -> Result<(), ScopedReservationError> {
        if expected_account_id.trim().is_empty() || expected_account_id != self.account_id {
            return Err(ScopedReservationError::AccountMismatch);
        }
        self.ledger
            .reserve(proposal, owner, expected_snapshot_id)
            .map_err(ScopedReservationError::Reservation)
    }

    /// An external supervisor may permanently block later reservations.
    /// Existing exposure and evidence are deliberately retained.
    pub fn invalidate(&self) -> Result<(), ScopedReservationError> {
        self.ledger
            .invalidate()
            .map_err(ScopedReservationError::Reservation)
    }

    pub fn reservation_evidence(
        &self,
        setup_id: &str,
    ) -> Result<Option<ScopedReservationEvidence>, ScopedReservationError> {
        self.ledger
            .reservation_evidence(setup_id)
            .map(|receipt| {
                receipt.map(|reservation| ScopedReservationEvidence {
                    account_id: self.account_id.clone(),
                    reservation,
                })
            })
            .map_err(ScopedReservationError::Reservation)
    }

    pub fn audit_view(&self) -> Result<ScopedPortfolioAuditView, ScopedReservationError> {
        self.ledger
            .audit_view()
            .map(|ledger| ScopedPortfolioAuditView {
                account_id: self.account_id.clone(),
                ledger,
            })
            .map_err(ScopedReservationError::Reservation)
    }
}
