//! Non-authorizing account-scoped broker exposure evidence proof.
//! Canonical §§9, 16, 65-70, 103 and 109. This module cannot authenticate
//! provider state, derive broker risk, authorize an order, or recover after
//! restart. All health fields are caller assertions, not broker proof.

use std::collections::BTreeSet;

use crate::{
    Condition, PortfolioRiskLimits, PortfolioRiskSnapshot, ReservationError, RiskExposure,
    SharedPortfolioReservations,
};

#[derive(Debug, Clone)]
pub struct BrokerPortfolioEvidence {
    /// Deployment-selected account identifier, not inferred from market data.
    pub expected_account_id: String,
    /// Account identifier reported by the upstream broker/data adapter.
    pub observed_account_id: String,
    pub snapshot_id: String,
    pub broker_connected: Condition,
    pub broker_state_known: Condition,
    pub position_state_known: Condition,
    pub open_order_state_known: Condition,
    pub data_fresh: Condition,
    pub timestamps_consistent: Condition,
    pub exposure_complete: Condition,
    pub account_equity: f64,
    pub limits: PortfolioRiskLimits,
    /// Broker-reported positions with externally determined risk amounts.
    pub positions: Vec<RiskExposure>,
    /// Broker-reported open/pending orders with externally determined risk.
    pub open_orders: Vec<RiskExposure>,
}

/// Account identity and caller-asserted risk data remain associated.
/// This is not an authenticated broker record or execution authorization.
#[derive(Debug, Clone)]
pub struct AssertedAccountRiskSnapshot {
    pub account_id: String,
    pub risk_snapshot: PortfolioRiskSnapshot,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BrokerEvidenceError {
    MissingAccount,
    AccountMismatch,
    MissingSnapshot,
    BrokerUnknown,
    PositionsUnknown,
    OpenOrdersUnknown,
    DataUnknown,
    DuplicateSetup,
    InvalidExposure,
    InvalidRiskBook(ReservationError),
}

impl BrokerPortfolioEvidence {
    /// Validates the declared evidence, then constructs an *asserted* risk
    /// snapshot for the existing isolated in-process reservation proof.
    /// This does not independently verify a broker API or grant live authority.
    pub fn into_asserted_snapshot(self) -> Result<AssertedAccountRiskSnapshot, BrokerEvidenceError> {
        if self.expected_account_id.trim().is_empty()
            || self.observed_account_id.trim().is_empty()
        {
            return Err(BrokerEvidenceError::MissingAccount);
        }
        if self.expected_account_id != self.observed_account_id {
            return Err(BrokerEvidenceError::AccountMismatch);
        }
        if self.snapshot_id.trim().is_empty() {
            return Err(BrokerEvidenceError::MissingSnapshot);
        }
        if self.broker_connected != Condition::True
            || self.broker_state_known != Condition::True
        {
            return Err(BrokerEvidenceError::BrokerUnknown);
        }
        if self.position_state_known != Condition::True {
            return Err(BrokerEvidenceError::PositionsUnknown);
        }
        // An empty list is not proof that there are no open orders.
        if self.open_order_state_known != Condition::True {
            return Err(BrokerEvidenceError::OpenOrdersUnknown);
        }
        if self.data_fresh != Condition::True
            || self.timestamps_consistent != Condition::True
            || self.exposure_complete != Condition::True
        {
            return Err(BrokerEvidenceError::DataUnknown);
        }

        let mut seen = BTreeSet::new();
        for exposure in self.positions.iter().chain(self.open_orders.iter()) {
            if exposure.setup_id.trim().is_empty()
                || exposure.cluster_id.trim().is_empty()
                || !exposure.risk_dollars.is_finite()
                || exposure.risk_dollars <= 0.0
            {
                return Err(BrokerEvidenceError::InvalidExposure);
            }
            // A setup reported simultaneously as an open order and a
            // position is ambiguous without a provider-specific fill/
            // order-linkage contract. Fail closed rather than net or double count.
            if !seen.insert(exposure.setup_id.as_str()) {
                return Err(BrokerEvidenceError::DuplicateSetup);
            }
        }

        let snapshot = PortfolioRiskSnapshot {
            snapshot_id: self.snapshot_id,
            account_equity: self.account_equity,
            limits: self.limits,
            broker_reconciled: Condition::True,
            exposure_complete: Condition::True,
            data_fresh: Condition::True,
            existing: self
                .positions
                .into_iter()
                .chain(self.open_orders)
                .collect(),
        };
        // Reuse the exact aggregate, cluster and directional fail-close
        // checks instead of duplicating or loosening canonical risk logic.
        SharedPortfolioReservations::from_snapshot(snapshot.clone())
            .map_err(BrokerEvidenceError::InvalidRiskBook)?;
        Ok(AssertedAccountRiskSnapshot {
            account_id: self.expected_account_id,
            risk_snapshot: snapshot,
        })
    }
}
