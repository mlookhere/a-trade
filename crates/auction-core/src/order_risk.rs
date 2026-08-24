use crate::{Condition, Direction, StrategyValidationProof};

/// Canonical §72 strict autonomous implementation threshold. This is not represented as a
/// universal source-trader risk/reward rule in provenance.
pub const STRICT_MIN_PLANNED_R: f64 = 1.5;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OrderType {
    StopLimit,
}

/// Deployment-owned execution values. Canonical §§60 and 62 contain MNQ examples only, so this
/// type deliberately has no defaults.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct InstrumentExecutionConfig {
    pub tick_size: f64,
    pub max_entry_slippage_ticks: u32,
    pub stop_buffer_ticks: u32,
}

impl InstrumentExecutionConfig {
    #[must_use]
    pub fn valid(self) -> bool {
        self.tick_size.is_finite() && self.tick_size > 0.0 && self.stop_buffer_ticks > 0
    }

    fn max_slippage(self) -> f64 {
        self.tick_size * f64::from(self.max_entry_slippage_ticks)
    }

    fn stop_buffer(self) -> f64 {
        self.tick_size * f64::from(self.stop_buffer_ticks)
    }
}

/// §59 long trigger and explicit §85 mirrored short trigger.
#[must_use]
pub fn entry_trigger(
    direction: Direction,
    reconfirmation_extreme: f64,
    tick_size: f64,
) -> Option<f64> {
    if !reconfirmation_extreme.is_finite() || !tick_size.is_finite() || tick_size <= 0.0 {
        return None;
    }

    Some(match direction {
        Direction::Long => reconfirmation_extreme + tick_size,
        Direction::Short => reconfirmation_extreme - tick_size,
    })
}

/// §60 slippage cap applied to a supplied STOP_LIMIT limit price.
#[must_use]
pub fn entry_limit_valid(
    direction: Direction,
    trigger: f64,
    entry_limit: f64,
    config: InstrumentExecutionConfig,
) -> Condition {
    if !trigger.is_finite() || !entry_limit.is_finite() || !config.valid() {
        return Condition::Unknown;
    }

    let max_slippage = config.max_slippage();
    Condition::from(match direction {
        Direction::Long => entry_limit >= trigger && entry_limit <= trigger + max_slippage,
        Direction::Short => entry_limit <= trigger && entry_limit >= trigger - max_slippage,
    })
}

/// §60 missed-trade guard. Equality at the configured maximum remains inside the allowance.
#[must_use]
pub fn slippage_guard(
    direction: Direction,
    trigger: f64,
    current_price: f64,
    config: InstrumentExecutionConfig,
) -> Condition {
    if !trigger.is_finite() || !current_price.is_finite() || !config.valid() {
        return Condition::Unknown;
    }

    let max_slippage = config.max_slippage();
    Condition::from(match direction {
        Direction::Long => current_price <= trigger + max_slippage,
        Direction::Short => current_price >= trigger - max_slippage,
    })
}

/// §61: an unfilled trigger expires after two completed 5-minute candles.
#[must_use]
pub const fn trigger_expired(completed_5m_candles_since_reconfirmation: u32) -> bool {
    completed_5m_candles_since_reconfirmation >= 2
}

/// §§62 and 93 structural stop placement using deployment-owned stop buffer.
#[must_use]
pub fn structural_stop(
    direction: Direction,
    first_failure_extreme: f64,
    config: InstrumentExecutionConfig,
) -> Option<f64> {
    if !first_failure_extreme.is_finite() || !config.valid() {
        return None;
    }

    Some(match direction {
        Direction::Long => first_failure_extreme - config.stop_buffer(),
        Direction::Short => first_failure_extreme + config.stop_buffer(),
    })
}

/// §63 long no-widening rule and explicit §85 mirrored short rule.
#[must_use]
pub fn stop_replacement_allowed(
    direction: Direction,
    current_stop: f64,
    proposed_stop: f64,
) -> Condition {
    if !current_stop.is_finite() || !proposed_stop.is_finite() {
        return Condition::Unknown;
    }

    Condition::from(match direction {
        Direction::Long => proposed_stop >= current_stop,
        Direction::Short => proposed_stop <= current_stop,
    })
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FuturesRiskInputs {
    pub account_equity: f64,
    pub risk_percent: f64,
    pub entry: f64,
    pub stop: f64,
    pub tick_size: f64,
    pub tick_value: f64,
    pub commissions_per_contract: f64,
    pub slippage_reserve_per_contract: f64,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PositionSizing {
    pub risk_budget_dollars: f64,
    pub stop_ticks: f64,
    pub risk_per_contract: f64,
    pub size: u64,
    pub proposed_open_risk_dollars: f64,
}

/// §§65-66 exact futures sizing. `risk_percent` is required input; no 0.25% default exists.
#[must_use]
pub fn size_futures(inputs: FuturesRiskInputs) -> Option<PositionSizing> {
    if !inputs.account_equity.is_finite()
        || inputs.account_equity <= 0.0
        || !inputs.risk_percent.is_finite()
        || inputs.risk_percent <= 0.0
        || !inputs.entry.is_finite()
        || !inputs.stop.is_finite()
        || inputs.entry == inputs.stop
        || !inputs.tick_size.is_finite()
        || inputs.tick_size <= 0.0
        || !inputs.tick_value.is_finite()
        || inputs.tick_value <= 0.0
        || !inputs.commissions_per_contract.is_finite()
        || inputs.commissions_per_contract < 0.0
        || !inputs.slippage_reserve_per_contract.is_finite()
        || inputs.slippage_reserve_per_contract < 0.0
    {
        return None;
    }

    let risk_budget_dollars = inputs.account_equity * inputs.risk_percent;
    let stop_ticks = (inputs.entry - inputs.stop).abs() / inputs.tick_size;
    let risk_per_contract = stop_ticks * inputs.tick_value
        + inputs.commissions_per_contract
        + inputs.slippage_reserve_per_contract;
    if !risk_budget_dollars.is_finite()
        || risk_budget_dollars <= 0.0
        || !stop_ticks.is_finite()
        || stop_ticks <= 0.0
        || !risk_per_contract.is_finite()
        || risk_per_contract <= 0.0
    {
        return None;
    }

    let raw_size = (risk_budget_dollars / risk_per_contract).floor();
    if !raw_size.is_finite() || raw_size < 1.0 || raw_size > u64::MAX as f64 {
        return None;
    }
    let size = raw_size as u64;
    let proposed_open_risk_dollars = risk_per_contract * size as f64;

    Some(PositionSizing {
        risk_budget_dollars,
        stop_ticks,
        risk_per_contract,
        size,
        proposed_open_risk_dollars,
    })
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PortfolioRiskLimits {
    pub max_portfolio_open_risk_percent: f64,
    pub max_cluster_open_risk_percent: f64,
}

impl PortfolioRiskLimits {
    fn valid(self) -> bool {
        self.max_portfolio_open_risk_percent.is_finite()
            && self.max_portfolio_open_risk_percent > 0.0
            && self.max_cluster_open_risk_percent.is_finite()
            && self.max_cluster_open_risk_percent > 0.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PortfolioRiskEvaluation {
    pub open_portfolio_risk_after: f64,
    pub cluster_risk_after: f64,
    pub portfolio_pass: Condition,
    pub cluster_pass: Condition,
}

/// §§67 and 69. Portfolio and cluster ceilings are deployment percentages and have no defaults.
#[must_use]
pub fn evaluate_portfolio_risk(
    account_equity: f64,
    current_open_risk: f64,
    current_cluster_risk: f64,
    proposed_trade_risk: f64,
    limits: PortfolioRiskLimits,
) -> PortfolioRiskEvaluation {
    if !account_equity.is_finite()
        || account_equity <= 0.0
        || !current_open_risk.is_finite()
        || current_open_risk < 0.0
        || !current_cluster_risk.is_finite()
        || current_cluster_risk < 0.0
        || !proposed_trade_risk.is_finite()
        || proposed_trade_risk < 0.0
        || !limits.valid()
    {
        return PortfolioRiskEvaluation {
            open_portfolio_risk_after: f64::NAN,
            cluster_risk_after: f64::NAN,
            portfolio_pass: Condition::Unknown,
            cluster_pass: Condition::Unknown,
        };
    }

    let open_portfolio_risk_after = current_open_risk + proposed_trade_risk;
    let cluster_risk_after = current_cluster_risk + proposed_trade_risk;
    let max_portfolio_risk = account_equity * limits.max_portfolio_open_risk_percent;
    let max_cluster_risk = account_equity * limits.max_cluster_open_risk_percent;

    PortfolioRiskEvaluation {
        open_portfolio_risk_after,
        cluster_risk_after,
        portfolio_pass: Condition::from(open_portfolio_risk_after <= max_portfolio_risk),
        cluster_pass: Condition::from(cluster_risk_after <= max_cluster_risk),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ClusterExposure<'a> {
    pub cluster_id: &'a str,
    pub direction: Direction,
}

/// §§68-70: a candidate must have explicit cluster identity; same-cluster opposing direction is
/// a conflict and fails closed. Same-direction exposure is governed separately by cluster risk.
#[must_use]
pub fn cluster_direction_clear(
    candidate_cluster_id: &str,
    candidate_direction: Direction,
    existing: &[ClusterExposure<'_>],
) -> Condition {
    if candidate_cluster_id.trim().is_empty()
        || existing
            .iter()
            .any(|exposure| exposure.cluster_id.trim().is_empty())
    {
        return Condition::Unknown;
    }

    Condition::from(!existing.iter().any(|exposure| {
        exposure.cluster_id == candidate_cluster_id && exposure.direction != candidate_direction
    }))
}

/// §§71-72 and §§85/94 mirror.
#[must_use]
pub fn planned_r(direction: Direction, entry: f64, stop: f64, target: f64) -> Option<f64> {
    if !entry.is_finite() || !stop.is_finite() || !target.is_finite() {
        return None;
    }

    let risk = match direction {
        Direction::Long if stop < entry && target > entry => entry - stop,
        Direction::Short if stop > entry && target < entry => stop - entry,
        Direction::Long | Direction::Short => return None,
    };
    let reward = match direction {
        Direction::Long => target - entry,
        Direction::Short => entry - target,
    };
    Some(reward / risk)
}

#[must_use]
pub fn structural_target_valid(
    direction: Direction,
    entry: f64,
    stop: f64,
    target: f64,
) -> Condition {
    match planned_r(direction, entry, stop, target) {
        Some(value) if value.is_finite() => Condition::from(value >= STRICT_MIN_PLANNED_R),
        _ => Condition::Unknown,
    }
}

/// Canonical §110 proposal whose pre-execution authority is non-forgeable outside this crate.
#[derive(Debug, Clone, PartialEq)]
pub struct OrderProposal {
    setup_id: String,
    owner_agent_id: String,
    instrument: String,
    side: Direction,
    order_type: OrderType,
    entry_trigger: f64,
    entry_limit: f64,
    stop: f64,
    target: f64,
    size: u64,
    risk_dollars: f64,
    risk_percent: f64,
    open_portfolio_risk_before: f64,
    open_portfolio_risk_after: f64,
    cluster_id: String,
    cluster_risk_after: f64,
    strategy_pass: Condition,
    risk_pass: Condition,
    portfolio_pass: Condition,
    execution_pass: Condition,
}

impl OrderProposal {
    #[must_use]
    pub fn setup_id(&self) -> &str {
        &self.setup_id
    }

    #[must_use]
    pub(crate) fn owner_agent_id(&self) -> &str {
        &self.owner_agent_id
    }

    #[must_use]
    pub fn instrument(&self) -> &str {
        &self.instrument
    }

    #[must_use]
    pub const fn side(&self) -> Direction {
        self.side
    }

    #[must_use]
    pub const fn order_type(&self) -> OrderType {
        self.order_type
    }

    #[must_use]
    pub const fn entry_trigger(&self) -> f64 {
        self.entry_trigger
    }

    #[must_use]
    pub const fn entry_limit(&self) -> f64 {
        self.entry_limit
    }

    #[must_use]
    pub const fn stop(&self) -> f64 {
        self.stop
    }

    #[must_use]
    pub const fn target(&self) -> f64 {
        self.target
    }

    #[must_use]
    pub const fn size(&self) -> u64 {
        self.size
    }

    #[must_use]
    pub const fn risk_dollars(&self) -> f64 {
        self.risk_dollars
    }

    #[must_use]
    pub const fn risk_percent(&self) -> f64 {
        self.risk_percent
    }

    #[must_use]
    pub const fn open_portfolio_risk_before(&self) -> f64 {
        self.open_portfolio_risk_before
    }

    #[must_use]
    pub const fn open_portfolio_risk_after(&self) -> f64 {
        self.open_portfolio_risk_after
    }

    #[must_use]
    pub fn cluster_id(&self) -> &str {
        &self.cluster_id
    }

    #[must_use]
    pub const fn cluster_risk_after(&self) -> f64 {
        self.cluster_risk_after
    }

    #[must_use]
    pub const fn strategy_pass(&self) -> Condition {
        self.strategy_pass
    }

    #[must_use]
    pub const fn risk_pass(&self) -> Condition {
        self.risk_pass
    }

    #[must_use]
    pub const fn portfolio_pass(&self) -> Condition {
        self.portfolio_pass
    }

    #[must_use]
    pub const fn execution_pass(&self) -> Condition {
        self.execution_pass
    }

    pub(crate) fn mark_execution_pass(&mut self) {
        self.execution_pass = Condition::True;
    }
}

#[derive(Debug, Clone, Copy)]
pub struct OrderProposalInputs<'a> {
    /// Opaque deterministic Strategy Validator proof. Identity, direction, accepted first-failure
    /// extreme, and final reconfirmation extreme are inherited from this proof rather than
    /// repeated as caller-supplied fields.
    pub strategy: &'a StrategyValidationProof,
    pub entry_limit: f64,
    pub target: f64,
    pub execution_config: InstrumentExecutionConfig,
    pub account_equity: f64,
    pub risk_percent: f64,
    pub tick_value: f64,
    pub commissions_per_contract: f64,
    pub slippage_reserve_per_contract: f64,
    pub current_open_portfolio_risk: f64,
    pub current_cluster_risk: f64,
    pub portfolio_limits: PortfolioRiskLimits,
    pub cluster_id: &'a str,
    pub existing_cluster_exposure: &'a [ClusterExposure<'a>],
}

/// Canonical §§6, 59-72, 93-94, 109-110 deterministic pre-broker proposal. Strategy identity
/// and structure-derived extrema come from the opaque proof; this function independently
/// recalculates entry, stop, target validity, position size, portfolio/cluster exposure, and
/// correlation conflict. `execution_pass` remains UNKNOWN until the broker Execution Gate.
#[must_use]
pub fn build_order_proposal(inputs: OrderProposalInputs<'_>) -> Option<OrderProposal> {
    if inputs.strategy.setup_id().trim().is_empty()
        || inputs.strategy.owner_agent_id().trim().is_empty()
        || inputs.strategy.instrument().trim().is_empty()
        || inputs.cluster_id.trim().is_empty()
        || !inputs.execution_config.valid()
    {
        return None;
    }

    let direction = inputs.strategy.direction();
    let trigger = entry_trigger(
        direction,
        inputs.strategy.reconfirmation_extreme(),
        inputs.execution_config.tick_size,
    )?;
    if !entry_limit_valid(
        direction,
        trigger,
        inputs.entry_limit,
        inputs.execution_config,
    )
    .permits()
    {
        return None;
    }

    let stop = structural_stop(
        direction,
        inputs.strategy.first_failure_extreme(),
        inputs.execution_config,
    )?;
    if !structural_target_valid(direction, trigger, stop, inputs.target).permits() {
        return None;
    }

    let sizing = size_futures(FuturesRiskInputs {
        account_equity: inputs.account_equity,
        risk_percent: inputs.risk_percent,
        entry: trigger,
        stop,
        tick_size: inputs.execution_config.tick_size,
        tick_value: inputs.tick_value,
        commissions_per_contract: inputs.commissions_per_contract,
        slippage_reserve_per_contract: inputs.slippage_reserve_per_contract,
    })?;

    let portfolio = evaluate_portfolio_risk(
        inputs.account_equity,
        inputs.current_open_portfolio_risk,
        inputs.current_cluster_risk,
        sizing.proposed_open_risk_dollars,
        inputs.portfolio_limits,
    );
    if !portfolio.portfolio_pass.permits()
        || !portfolio.cluster_pass.permits()
        || !cluster_direction_clear(
            inputs.cluster_id,
            direction,
            inputs.existing_cluster_exposure,
        )
        .permits()
    {
        return None;
    }

    Some(OrderProposal {
        setup_id: inputs.strategy.setup_id().to_owned(),
        owner_agent_id: inputs.strategy.owner_agent_id().to_owned(),
        instrument: inputs.strategy.instrument().to_owned(),
        side: direction,
        order_type: OrderType::StopLimit,
        entry_trigger: trigger,
        entry_limit: inputs.entry_limit,
        stop,
        target: inputs.target,
        size: sizing.size,
        risk_dollars: sizing.risk_budget_dollars,
        risk_percent: inputs.risk_percent,
        open_portfolio_risk_before: inputs.current_open_portfolio_risk,
        open_portfolio_risk_after: portfolio.open_portfolio_risk_after,
        cluster_id: inputs.cluster_id.to_owned(),
        cluster_risk_after: portfolio.cluster_risk_after,
        strategy_pass: Condition::True,
        risk_pass: Condition::True,
        portfolio_pass: Condition::True,
        execution_pass: Condition::Unknown,
    })
}
