//! Single-process reservation proof for canonical §§9, 67-70 and 109.
//! This is not connected to the live Execution Gate, is not durable and cannot
//! authorize orders. No reservation may be released without a future explicit
//! broker-reconciled lifecycle contract.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use crate::order_risk::ClusterExposure;
use crate::{
    Condition, Direction, OrderProposal, PortfolioRiskLimits, cluster_direction_clear,
    evaluate_portfolio_risk,
};

#[derive(Debug, Clone)]
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
    account_equity: f64,
    limits: PortfolioRiskLimits,
    exposures: HashMap<String, RiskExposure>,
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

        let mut exposures = HashMap::new();
        let mut open_risk = 0.0;
        for exposure in snapshot.existing {
            if exposure.setup_id.trim().is_empty()
                || exposure.cluster_id.trim().is_empty()
                || !exposure.risk_dollars.is_finite()
                || exposure.risk_dollars <= 0.0
            {
                return Err(ReservationError::InvalidSnapshot);
            }
            open_risk += exposure.risk_dollars;
            if !open_risk.is_finite()
                || exposures
                    .insert(exposure.setup_id.clone(), exposure)
                    .is_some()
            {
                return Err(ReservationError::InvalidSnapshot);
            }
        }
        if open_risk > snapshot.account_equity * snapshot.limits.max_portfolio_open_risk_percent {
            return Err(ReservationError::InvalidSnapshot);
        }
        Ok(Self {
            state: Arc::new(Mutex::new(ReservationState {
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

    #[must_use]
    pub fn reservation_count(&self) -> Result<usize, ReservationError> {
        let state = self
            .state
            .lock()
            .map_err(|_| ReservationError::Unavailable)?;
        Ok(state.exposures.len())
    }
}
