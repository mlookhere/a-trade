//! Single-process reservation proof for canonical §§9, 67-70 and 109.
//! This is not connected to the live Execution Gate, is not durable and cannot
//! authorize orders. No reservation may be released without a future explicit
//! broker-reconciled lifecycle contract.

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

use crate::order_risk::ClusterExposure;
use crate::{
    Condition, Direction, OrderProposal, PortfolioRiskLimits, cluster_direction_clear,
    evaluate_portfolio_risk,
};

#[derive(Debug, Clone, PartialEq)]
pub struct RiskExposure {
    pub setup_id: String,
    pub cluster_id: String,
    pub direction: Direction,
    pub risk_dollars: f64,
}

/// The caller must prove freshness, completeness and broker reconciliation.
/// These supplied flags are NOT independent evidence of broker truth.
#[derive(Debug, Clone)]
pub struct PortfolioRiskSnapshot {
    pub snapshot_id: String,
    pub account_equity: f64,
    pub limits: PortfolioRiskLimits,
    pub broker_reconciled: Condition,
    pub exposure_complete: Condition,
    pub data_fresh: Condition,
    pub existing: Vec<RiskExposure>,
}

/// Read-only in-process evidence; the asserted snapshot ID does not prove
/// external broker freshness, reconciliation or durable persistence.
#[derive(Debug, Clone, PartialEq)]
pub struct ReservationEvidence {
    pub snapshot_id: String,
    pub exposure: RiskExposure,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReservationError {
    InvalidSnapshot,
    InvalidProposal,
    WrongOwner,
    DuplicateSetup,
    PortfolioLimit,
    ClusterLimit,
    ClusterConflict,
    Unavailable,
}

#[derive(Debug)]
struct ReservationState {
    snapshot_id: String,
    account_equity: f64,
    limits: PortfolioRiskLimits,
    exposures: BTreeMap<String, RiskExposure>,
}

/// Clones share a single mutex-protected ledger. It is deliberately not a
/// substitute for a process-shared database or durable order reconciliation.
#[derive(Debug, Clone)]
pub struct SharedPortfolioReservations {
    state: Arc<Mutex<ReservationState>>,
}

impl SharedPortfolioReservations {
    pub fn from_snapshot(snapshot: PortfolioRiskSnapshot) -> Result<Self, ReservationError> {
        if snapshot.snapshot_id.trim().is_empty()
            || snapshot.broker_reconciled != Condition::True
            || snapshot.exposure_complete != Condition::True
            || snapshot.data_fresh != Condition::True
            || !snapshot.account_equity.is_finite()
            || snapshot.account_equity <= 0.0
            || !snapshot.limits.max_portfolio_open_risk_percent.is_finite()
            || snapshot.limits.max_portfolio_open_risk_percent <= 0.0
            || !snapshot.limits.max_cluster_open_risk_percent.is_finite()
            || snapshot.limits.max_cluster_open_risk_percent <= 0.0
        {
            return Err(ReservationError::InvalidSnapshot);
        }

        // Canonical §§67-70: no baseline is usable if any risk ceiling
        // overflows or any existing cluster is over-limit or directionally
        // conflicted. Reject rather than repairing broker-reported exposure.
        let portfolio_ceiling =
            snapshot.account_equity * snapshot.limits.max_portfolio_open_risk_percent;
        let cluster_ceiling =
            snapshot.account_equity * snapshot.limits.max_cluster_open_risk_percent;
        if !portfolio_ceiling.is_finite() || !cluster_ceiling.is_finite() {
            return Err(ReservationError::InvalidSnapshot);
        }

        // Always accumulate in SETUP_ID order. Floating-point addition is not
        // associative; input vector order and randomized hash iteration must
        // not change a deterministic portfolio or cluster risk verdict.
        let mut exposures = BTreeMap::new();
        for exposure in snapshot.existing {
            if exposure.setup_id.trim().is_empty()
                || exposure.cluster_id.trim().is_empty()
                || !exposure.risk_dollars.is_finite()
                || exposure.risk_dollars <= 0.0
                || exposures
                    .insert(exposure.setup_id.clone(), exposure)
                    .is_some()
            {
                return Err(ReservationError::InvalidSnapshot);
            }
        }

        let mut clusters: BTreeMap<String, (f64, Direction)> = BTreeMap::new();
        let mut open_risk = 0.0;
        for exposure in exposures.values() {
            open_risk += exposure.risk_dollars;
            let cluster = clusters
                .entry(exposure.cluster_id.clone())
                .or_insert((0.0, exposure.direction));
            cluster.0 += exposure.risk_dollars;
            if !open_risk.is_finite()
                || open_risk > portfolio_ceiling
                || !cluster.0.is_finite()
                || cluster.0 > cluster_ceiling
                || cluster.1 != exposure.direction
            {
                return Err(ReservationError::InvalidSnapshot);
            }
        }
        Ok(Self {
            state: Arc::new(Mutex::new(ReservationState {
                snapshot_id: snapshot.snapshot_id,
                account_equity: snapshot.account_equity,
                limits: snapshot.limits,
                exposures,
            })),
        })
    }

    /// Atomically checks current in-ledger exposure and records a reservation.
    /// A successful result is NOT an ExecutionPermit and cannot authorize entry.
    pub fn reserve(&self, proposal: &OrderProposal, owner: &str) -> Result<(), ReservationError> {
        if owner.trim().is_empty() || owner != proposal.owner_agent_id() {
            return Err(ReservationError::WrongOwner);
        }
        if proposal.strategy_pass() != Condition::True
            || proposal.risk_pass() != Condition::True
            || proposal.portfolio_pass() != Condition::True
            || proposal.execution_pass() != Condition::Unknown
            || proposal.setup_id().trim().is_empty()
            || proposal.cluster_id().trim().is_empty()
            || proposal.size() == 0
            || !proposal.open_portfolio_risk_before().is_finite()
            || !proposal.open_portfolio_risk_after().is_finite()
            || proposal.open_portfolio_risk_before() < 0.0
        {
            return Err(ReservationError::InvalidProposal);
        }
        let risk = proposal.open_portfolio_risk_after() - proposal.open_portfolio_risk_before();
        if !risk.is_finite() || risk <= 0.0 {
            return Err(ReservationError::InvalidProposal);
        }

        let mut state = self
            .state
            .lock()
            .map_err(|_| ReservationError::Unavailable)?;
        if state.exposures.contains_key(proposal.setup_id()) {
            return Err(ReservationError::DuplicateSetup);
        }

        let mut portfolio_risk = 0.0;
        let mut cluster_risk = 0.0;
        let mut cluster_directions = Vec::new();
        for exposure in state.exposures.values() {
            portfolio_risk += exposure.risk_dollars;
            if exposure.cluster_id == proposal.cluster_id() {
                cluster_risk += exposure.risk_dollars;
            }
            cluster_directions.push(ClusterExposure {
                cluster_id: &exposure.cluster_id,
                direction: exposure.direction,
            });
        }
        if !portfolio_risk.is_finite() || !cluster_risk.is_finite() {
            return Err(ReservationError::Unavailable);
        }
        if cluster_direction_clear(proposal.cluster_id(), proposal.side(), &cluster_directions)
            != Condition::True
        {
            return Err(ReservationError::ClusterConflict);
        }
        let evaluation = evaluate_portfolio_risk(
            state.account_equity,
            portfolio_risk,
            cluster_risk,
            risk,
            state.limits,
        );
        if evaluation.portfolio_pass != Condition::True {
            return Err(ReservationError::PortfolioLimit);
        }
        if evaluation.cluster_pass != Condition::True {
            return Err(ReservationError::ClusterLimit);
        }

        state.exposures.insert(
            proposal.setup_id().to_owned(),
            RiskExposure {
                setup_id: proposal.setup_id().to_owned(),
                cluster_id: proposal.cluster_id().to_owned(),
                direction: proposal.side(),
                risk_dollars: risk,
            },
        );
        Ok(())
    }

    /// Read-only evidence for an existing exposure. A missing SETUP_ID never
    /// creates a reservation or implies that broker reconciliation succeeded.
    pub fn reservation_evidence(
        &self,
        setup_id: &str,
    ) -> Result<Option<ReservationEvidence>, ReservationError> {
        if setup_id.trim().is_empty() {
            return Ok(None);
        }
        let state = self
            .state
            .lock()
            .map_err(|_| ReservationError::Unavailable)?;
        Ok(state.exposures.get(setup_id).map(|exposure| ReservationEvidence {
            snapshot_id: state.snapshot_id.clone(),
            exposure: exposure.clone(),
        }))
    }

    pub fn reservation_count(&self) -> Result<usize, ReservationError> {
        let state = self
            .state
            .lock()
            .map_err(|_| ReservationError::Unavailable)?;
        Ok(state.exposures.len())
    }
}
