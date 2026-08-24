use std::time::Duration;

use crate::{Condition, Direction, GammaRegime, MarketState, RejectionCode, SetupState};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EffortResultSnapshot {
    pub favorable_effort_successful: Condition,
    pub favorable_effort_failure: Condition,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EffortResultChange {
    pub from: EffortResultSnapshot,
    pub to: EffortResultSnapshot,
}

/// Minimal append-only §118 state-change journal record. Audit deliberately does not validate or
/// repair the transition itself; an illegal observed transition must remain visible to audit.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StateChangeRecord {
    pub setup_id: String,
    pub from: SetupState,
    pub to: SetupState,
}

/// Canonical §111 completed-trade payload. Audit validation is structural only; a
/// strategy-noncompliant trade still needs to be recordable so violations are not hidden.
#[derive(Debug, Clone, PartialEq)]
pub struct CompletedTradeRecord {
    pub setup_id: String,
    pub instrument: String,
    pub direction: Direction,
    pub environment: MarketState,
    pub gamma_regime: GammaRegime,
    pub entry: f64,
    pub stop: f64,
    pub target: f64,
    pub size: u64,
    pub realized_r: f64,
    pub mfe: f64,
    pub mae: f64,
    pub slippage: f64,
    pub time_in_trade: Duration,
    pub value_reclaim_outcome: Condition,
    pub effort_vs_result_changes: Vec<EffortResultChange>,
    pub exit_reason: String,
}

impl CompletedTradeRecord {
    #[must_use]
    pub fn structurally_valid(&self) -> bool {
        !self.setup_id.trim().is_empty()
            && !self.instrument.trim().is_empty()
            && self.entry.is_finite()
            && self.stop.is_finite()
            && self.target.is_finite()
            && self.size > 0
            && self.realized_r.is_finite()
            && self.mfe.is_finite()
            && self.mae.is_finite()
            && self.slippage.is_finite()
            && !self.exit_reason.trim().is_empty()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RejectedSetupRecord<'a> {
    pub setup_id: &'a str,
    pub rejection_code: RejectionCode,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredRejectedSetupRecord {
    pub setup_id: String,
    pub rejection_code: RejectionCode,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuditError {
    InvalidRecord,
    DuplicateRecord,
}

/// Reporting-only deterministic formalization of canonical §§97 and 114 metrics.
#[derive(Debug, Clone, PartialEq)]
pub struct DailyMetrics {
    pub qualified_setups: u64,
    pub rejected_setups: u64,
    pub trades_executed: u64,
    pub winning_trades: u64,
    pub losing_trades: u64,
    pub breakevens: u64,
    pub realized_r: f64,
    pub expectancy_r: Option<f64>,
    pub profit_factor: Option<f64>,
    pub win_rate: Option<f64>,
    pub average_win_r: Option<f64>,
    pub average_loss_r: Option<f64>,
    pub maximum_drawdown_r: f64,
    pub maximum_portfolio_heat: Option<f64>,
    pub mean_mfe: Option<f64>,
    pub mean_mae: Option<f64>,
    pub mean_slippage: Option<f64>,
    pub rule_compliance_percentage: Option<f64>,
    pub duplicate_signals_prevented: u64,
    pub risk_conflicts_prevented: u64,
    pub execution_failures: u64,
    pub current_consecutive_losses: u64,
}

/// §6 Audit Agent boundary: this type only appends observations and derives reports. It owns no
/// authorization, risk, broker, order, or live-parameter handle.
#[derive(Debug, Default)]
pub struct AuditLedger {
    qualified_setup_ids: Vec<String>,
    state_changes: Vec<StateChangeRecord>,
    completed_trades: Vec<CompletedTradeRecord>,
    rejected_setups: Vec<StoredRejectedSetupRecord>,
    portfolio_heat_observations: Vec<f64>,
    rule_compliance_observations: Vec<bool>,
    duplicate_signals_prevented: u64,
    risk_conflicts_prevented: u64,
    execution_failures: u64,
}

impl AuditLedger {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    pub fn record_qualified_setup(&mut self, setup_id: &str) -> Result<(), AuditError> {
        if setup_id.trim().is_empty() {
            return Err(AuditError::InvalidRecord);
        }
        if self
            .qualified_setup_ids
            .iter()
            .any(|existing| existing == setup_id)
        {
            return Err(AuditError::DuplicateRecord);
        }
        self.qualified_setup_ids.push(setup_id.to_owned());
        Ok(())
    }

    pub fn record_state_change(
        &mut self,
        setup_id: &str,
        from: SetupState,
        to: SetupState,
    ) -> Result<(), AuditError> {
        if setup_id.trim().is_empty() {
            return Err(AuditError::InvalidRecord);
        }
        self.state_changes.push(StateChangeRecord {
            setup_id: setup_id.to_owned(),
            from,
            to,
        });
        Ok(())
    }

    pub fn record_completed_trade(
        &mut self,
        record: CompletedTradeRecord,
    ) -> Result<(), AuditError> {
        if !record.structurally_valid() {
            return Err(AuditError::InvalidRecord);
        }
        if self.terminal_record_exists(&record.setup_id) {
            return Err(AuditError::DuplicateRecord);
        }
        self.completed_trades.push(record);
        Ok(())
    }

    pub fn record_rejected_setup(
        &mut self,
        record: RejectedSetupRecord<'_>,
    ) -> Result<(), AuditError> {
        if record.setup_id.trim().is_empty() {
            return Err(AuditError::InvalidRecord);
        }
        if self.terminal_record_exists(record.setup_id) {
            return Err(AuditError::DuplicateRecord);
        }
        self.rejected_setups.push(StoredRejectedSetupRecord {
            setup_id: record.setup_id.to_owned(),
            rejection_code: record.rejection_code,
        });
        Ok(())
    }

    pub fn record_portfolio_heat(&mut self, heat: f64) -> Result<(), AuditError> {
        if !heat.is_finite() || heat < 0.0 {
            return Err(AuditError::InvalidRecord);
        }
        self.portfolio_heat_observations.push(heat);
        Ok(())
    }

    pub fn record_rule_compliance(&mut self, compliant: bool) {
        self.rule_compliance_observations.push(compliant);
    }

    pub fn record_duplicate_signal_prevented(&mut self) {
        self.duplicate_signals_prevented = self.duplicate_signals_prevented.saturating_add(1);
    }

    pub fn record_risk_conflict_prevented(&mut self) {
        self.risk_conflicts_prevented = self.risk_conflicts_prevented.saturating_add(1);
    }

    pub fn record_execution_failure(&mut self) {
        self.execution_failures = self.execution_failures.saturating_add(1);
    }

    #[must_use]
    pub fn state_changes(&self) -> &[StateChangeRecord] {
        &self.state_changes
    }

    #[must_use]
    pub fn completed_trades(&self) -> &[CompletedTradeRecord] {
        &self.completed_trades
    }

    #[must_use]
    pub fn rejected_setups(&self) -> &[StoredRejectedSetupRecord] {
        &self.rejected_setups
    }

    /// §97 rolling expectancy requires an explicit reporting window; no default is embedded.
    #[must_use]
    pub fn rolling_expectancy_r(&self, window_trades: usize) -> Option<f64> {
        if window_trades == 0 || self.completed_trades.len() < window_trades {
            return None;
        }
        let start = self.completed_trades.len() - window_trades;
        let sum: f64 = self.completed_trades[start..]
            .iter()
            .map(|trade| trade.realized_r)
            .sum();
        Some(sum / window_trades as f64)
    }

    #[must_use]
    pub fn daily_metrics(&self) -> DailyMetrics {
        let trades_executed = self.completed_trades.len() as u64;
        let winning_trades = self
            .completed_trades
            .iter()
            .filter(|trade| trade.realized_r > 0.0)
            .count() as u64;
        let losing_trades = self
            .completed_trades
            .iter()
            .filter(|trade| trade.realized_r < 0.0)
            .count() as u64;
        let breakevens = trades_executed - winning_trades - losing_trades;
        let realized_r: f64 = self.completed_trades.iter().map(|trade| trade.realized_r).sum();
        let gross_win_r: f64 = self
            .completed_trades
            .iter()
            .filter(|trade| trade.realized_r > 0.0)
            .map(|trade| trade.realized_r)
            .sum();
        let gross_loss_r: f64 = self
            .completed_trades
            .iter()
            .filter(|trade| trade.realized_r < 0.0)
            .map(|trade| trade.realized_r)
            .sum();

        DailyMetrics {
            qualified_setups: self.qualified_setup_ids.len() as u64,
            rejected_setups: self.rejected_setups.len() as u64,
            trades_executed,
            winning_trades,
            losing_trades,
            breakevens,
            realized_r,
            expectancy_r: mean_or_none(realized_r, trades_executed),
            profit_factor: if gross_loss_r < 0.0 {
                Some(gross_win_r / gross_loss_r.abs())
            } else {
                None
            },
            win_rate: mean_or_none(winning_trades as f64, trades_executed),
            average_win_r: mean_or_none(gross_win_r, winning_trades),
            average_loss_r: mean_or_none(gross_loss_r, losing_trades),
            maximum_drawdown_r: maximum_drawdown_r(&self.completed_trades),
            maximum_portfolio_heat: finite_max(&self.portfolio_heat_observations),
            mean_mfe: mean_trade_field(&self.completed_trades, |trade| trade.mfe),
            mean_mae: mean_trade_field(&self.completed_trades, |trade| trade.mae),
            mean_slippage: mean_trade_field(&self.completed_trades, |trade| trade.slippage),
            rule_compliance_percentage: compliance_percentage(&self.rule_compliance_observations),
            duplicate_signals_prevented: self.duplicate_signals_prevented,
            risk_conflicts_prevented: self.risk_conflicts_prevented,
            execution_failures: self.execution_failures,
            current_consecutive_losses: current_consecutive_losses(&self.completed_trades),
        }
    }

    fn terminal_record_exists(&self, setup_id: &str) -> bool {
        self.completed_trades
            .iter()
            .any(|trade| trade.setup_id == setup_id)
            || self
                .rejected_setups
                .iter()
                .any(|record| record.setup_id == setup_id)
    }
}

fn mean_or_none(sum: f64, count: u64) -> Option<f64> {
    if count == 0 {
        None
    } else {
        Some(sum / count as f64)
    }
}

fn mean_trade_field(
    trades: &[CompletedTradeRecord],
    field: impl Fn(&CompletedTradeRecord) -> f64,
) -> Option<f64> {
    if trades.is_empty() {
        return None;
    }
    Some(trades.iter().map(field).sum::<f64>() / trades.len() as f64)
}

fn finite_max(values: &[f64]) -> Option<f64> {
    values.iter().copied().reduce(f64::max)
}

fn compliance_percentage(observations: &[bool]) -> Option<f64> {
    if observations.is_empty() {
        return None;
    }
    let compliant = observations.iter().filter(|value| **value).count();
    Some(compliant as f64 / observations.len() as f64 * 100.0)
}

fn maximum_drawdown_r(trades: &[CompletedTradeRecord]) -> f64 {
    let mut cumulative = 0.0;
    let mut peak = 0.0;
    let mut maximum_drawdown: f64 = 0.0;
    for trade in trades {
        cumulative += trade.realized_r;
        peak = peak.max(cumulative);
        maximum_drawdown = maximum_drawdown.max(peak - cumulative);
    }
    maximum_drawdown
}

fn current_consecutive_losses(trades: &[CompletedTradeRecord]) -> u64 {
    let mut losses = 0_u64;
    for trade in trades.iter().rev() {
        if trade.realized_r < 0.0 {
            losses = losses.saturating_add(1);
        } else {
            break;
        }
    }
    losses
}
