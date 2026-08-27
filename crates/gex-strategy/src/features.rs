use crate::{DecisionVerdict, MarketSnapshot};

/// AI-native GEX spec §§12-15, 33-34.
///
/// Missing feature values remain `None`/UNKNOWN. Values supplied by an upstream deterministic
/// calculator must be finite, but this layer intentionally does not invent normalization ranges,
/// score formulas, or strategy thresholds that the specification leaves configurable.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct TrendFeatures {
    pub trend_direction: Option<f64>,
    pub trend_strength: Option<f64>,
    pub ema_alignment: Option<f64>,
    pub ema_separation: Option<f64>,
    pub ema_slope: Option<f64>,
    pub trend_acceleration: Option<f64>,
    pub multi_timeframe_agreement: Option<f64>,
    pub regime_transition_probability: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct StructureFeatures {
    pub impulse_strength: Option<f64>,
    pub retracement_quality: Option<f64>,
    pub range_contraction: Option<f64>,
    pub volatility_compression: Option<f64>,
    pub volume_contraction: Option<f64>,
    pub directional_structure_preserved: Option<f64>,
    pub breakout_pressure: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct CandleEventFeatures {
    pub rejection_strength: Option<f64>,
    pub wick_asymmetry: Option<f64>,
    pub close_location_value: Option<f64>,
    pub reclaim_strength: Option<f64>,
    pub absorption_score: Option<f64>,
    pub follow_through_score: Option<f64>,
    pub volume_delta: Option<f64>,
    pub trade_velocity: Option<f64>,
    pub bid_ask_imbalance: Option<f64>,
    pub liquidity_response: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct ParticipationFeatures {
    pub relative_volume: Option<f64>,
    pub volume_acceleration: Option<f64>,
    pub trade_count: Option<f64>,
    pub average_trade_size: Option<f64>,
    pub buy_sell_imbalance: Option<f64>,
    pub volume_delta: Option<f64>,
    pub volume_at_price: Option<f64>,
    pub liquidity_depletion: Option<f64>,
    pub spread_quality: Option<f64>,
    pub quote_replenishment: Option<f64>,
    pub price_movement_per_unit_volume: Option<f64>,
    pub consolidation_volume_contraction: Option<f64>,
    pub breakout_volume_expansion: Option<f64>,
    pub participation_score: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct LiquidityExecutionFeatures {
    pub liquidity_depletion: Option<f64>,
    pub spread_quality: Option<f64>,
    pub quote_replenishment: Option<f64>,
    pub expected_spread_cost: Option<f64>,
    pub expected_slippage: Option<f64>,
    pub fees: Option<f64>,
    pub execution_risk_penalty: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct PortfolioBrokerFeatures {
    pub gross_exposure: Option<f64>,
    pub net_directional_exposure: Option<f64>,
    pub same_underlying_exposure: Option<f64>,
    pub correlated_underlying_exposure: Option<f64>,
    pub concentration_exposure: Option<f64>,
    pub aggregate_gamma_exposure: Option<f64>,
    pub aggregate_vega_exposure: Option<f64>,
    pub aggregate_theta_exposure: Option<f64>,
    pub daily_loss: Option<f64>,
    pub open_order_risk: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct AiNativeFeatureInput {
    pub snapshot_id: String,
    pub trend: TrendFeatures,
    pub structure: StructureFeatures,
    pub candle_event: CandleEventFeatures,
    pub participation: ParticipationFeatures,
    pub liquidity_execution: LiquidityExecutionFeatures,
    pub portfolio_broker: PortfolioBrokerFeatures,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FeatureSetError {
    MissingSnapshotId,
    SnapshotMismatch,
    NonFiniteFeature,
}

/// Immutable, snapshot-bound deterministic feature state for AI-native GEX spec §§6,12-16,39,42.
///
/// This object validates feature transport integrity only. It does not calculate missing features,
/// apply playbook thresholds, estimate probability/EV, or authorize a trade.
#[derive(Debug, Clone, PartialEq)]
pub struct AiNativeFeatureSet {
    snapshot_id: String,
    trend: TrendFeatures,
    structure: StructureFeatures,
    candle_event: CandleEventFeatures,
    participation: ParticipationFeatures,
    liquidity_execution: LiquidityExecutionFeatures,
    portfolio_broker: PortfolioBrokerFeatures,
}

impl AiNativeFeatureSet {
    pub fn build(
        snapshot: &MarketSnapshot,
        input: AiNativeFeatureInput,
    ) -> Result<Self, FeatureSetError> {
        if input.snapshot_id.trim().is_empty() {
            return Err(FeatureSetError::MissingSnapshotId);
        }
        if input.snapshot_id != snapshot.snapshot_id() {
            return Err(FeatureSetError::SnapshotMismatch);
        }
        if !all_feature_values_finite(&input) {
            return Err(FeatureSetError::NonFiniteFeature);
        }

        Ok(Self {
            snapshot_id: input.snapshot_id,
            trend: input.trend,
            structure: input.structure,
            candle_event: input.candle_event,
            participation: input.participation,
            liquidity_execution: input.liquidity_execution,
            portfolio_broker: input.portfolio_broker,
        })
    }

    #[must_use]
    pub fn snapshot_id(&self) -> &str {
        &self.snapshot_id
    }

    #[must_use]
    pub const fn trend(&self) -> &TrendFeatures {
        &self.trend
    }

    #[must_use]
    pub const fn structure(&self) -> &StructureFeatures {
        &self.structure
    }

    #[must_use]
    pub const fn candle_event(&self) -> &CandleEventFeatures {
        &self.candle_event
    }

    #[must_use]
    pub const fn participation(&self) -> &ParticipationFeatures {
        &self.participation
    }

    #[must_use]
    pub const fn liquidity_execution(&self) -> &LiquidityExecutionFeatures {
        &self.liquidity_execution
    }

    #[must_use]
    pub const fn portfolio_broker(&self) -> &PortfolioBrokerFeatures {
        &self.portfolio_broker
    }
}

fn finite_or_unknown(value: Option<f64>) -> bool {
    value.is_none_or(f64::is_finite)
}

fn all_feature_values_finite(input: &AiNativeFeatureInput) -> bool {
    let trend = &input.trend;
    let structure = &input.structure;
    let candle = &input.candle_event;
    let participation = &input.participation;
    let liquidity = &input.liquidity_execution;
    let portfolio = &input.portfolio_broker;

    [
        trend.trend_direction,
        trend.trend_strength,
        trend.ema_alignment,
        trend.ema_separation,
        trend.ema_slope,
        trend.trend_acceleration,
        trend.multi_timeframe_agreement,
        trend.regime_transition_probability,
        structure.impulse_strength,
        structure.retracement_quality,
        structure.range_contraction,
        structure.volatility_compression,
        structure.volume_contraction,
        structure.directional_structure_preserved,
        structure.breakout_pressure,
        candle.rejection_strength,
        candle.wick_asymmetry,
        candle.close_location_value,
        candle.reclaim_strength,
        candle.absorption_score,
        candle.follow_through_score,
        candle.volume_delta,
        candle.trade_velocity,
        candle.bid_ask_imbalance,
        candle.liquidity_response,
        participation.relative_volume,
        participation.volume_acceleration,
        participation.trade_count,
        participation.average_trade_size,
        participation.buy_sell_imbalance,
        participation.volume_delta,
        participation.volume_at_price,
        participation.liquidity_depletion,
        participation.spread_quality,
        participation.quote_replenishment,
        participation.price_movement_per_unit_volume,
        participation.consolidation_volume_contraction,
        participation.breakout_volume_expansion,
        participation.participation_score,
        liquidity.liquidity_depletion,
        liquidity.spread_quality,
        liquidity.quote_replenishment,
        liquidity.expected_spread_cost,
        liquidity.expected_slippage,
        liquidity.fees,
        liquidity.execution_risk_penalty,
        portfolio.gross_exposure,
        portfolio.net_directional_exposure,
        portfolio.same_underlying_exposure,
        portfolio.correlated_underlying_exposure,
        portfolio.concentration_exposure,
        portfolio.aggregate_gamma_exposure,
        portfolio.aggregate_vega_exposure,
        portfolio.aggregate_theta_exposure,
        portfolio.daily_loss,
        portfolio.open_order_risk,
    ]
    .into_iter()
    .all(finite_or_unknown)
}

/// Source-baseline EMA-stack state retained as an explanatory AI-native feature per spec §§12,35.
/// It is not a universal hard veto in AI-native mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EmaStackState {
    Bullish,
    Bearish,
    Mixed,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EmaStackError {
    NonFiniteValue,
}

pub fn classify_ema_stack(
    ema_9: Option<f64>,
    ema_21: Option<f64>,
    ema_50: Option<f64>,
) -> Result<EmaStackState, EmaStackError> {
    for value in [ema_9, ema_21, ema_50].into_iter().flatten() {
        if !value.is_finite() {
            return Err(EmaStackError::NonFiniteValue);
        }
    }

    let (Some(ema_9), Some(ema_21), Some(ema_50)) = (ema_9, ema_21, ema_50) else {
        return Ok(EmaStackState::Unknown);
    };

    if ema_9 > ema_21 && ema_21 > ema_50 {
        Ok(EmaStackState::Bullish)
    } else if ema_9 < ema_21 && ema_21 < ema_50 {
        Ok(EmaStackState::Bearish)
    } else {
        Ok(EmaStackState::Mixed)
    }
}

/// Exact mandatory hard-condition set from AI-native GEX spec §16.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HardConditionKind {
    DataValid,
    DataFresh,
    GexScopeValid,
    RequiredRegimeDataValid,
    BrokerConnected,
    OrderStateValid,
    InstrumentTradable,
    LiquidityWithinLimits,
    MaxPositionRiskValid,
    MaxPortfolioRiskValid,
    InvalidationDefined,
    ExecutionCostWithinLimits,
    KillSwitchClear,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HardCondition {
    Pass,
    Fail,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HardVetoInput {
    pub snapshot_id: String,
    pub data_valid: HardCondition,
    pub data_fresh: HardCondition,
    pub gex_scope_valid: HardCondition,
    pub required_regime_data_valid: HardCondition,
    pub broker_connected: HardCondition,
    pub order_state_valid: HardCondition,
    pub instrument_tradable: HardCondition,
    pub liquidity_within_limits: HardCondition,
    pub max_position_risk_valid: HardCondition,
    pub max_portfolio_risk_valid: HardCondition,
    pub invalidation_defined: HardCondition,
    pub execution_cost_within_limits: HardCondition,
    pub kill_switch_clear: HardCondition,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HardVetoError {
    MissingSnapshotId,
    SnapshotMismatch,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HardVetoVerdict {
    Cleared,
    Wait,
    Reject,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HardVetoResult {
    snapshot_id: String,
    verdict: HardVetoVerdict,
    failed_conditions: Vec<HardConditionKind>,
    unknown_conditions: Vec<HardConditionKind>,
}

impl HardVetoResult {
    #[must_use]
    pub fn snapshot_id(&self) -> &str {
        &self.snapshot_id
    }

    #[must_use]
    pub const fn verdict(&self) -> HardVetoVerdict {
        self.verdict
    }

    #[must_use]
    pub fn failed_conditions(&self) -> &[HardConditionKind] {
        &self.failed_conditions
    }

    #[must_use]
    pub fn unknown_conditions(&self) -> &[HardConditionKind] {
        &self.unknown_conditions
    }

    /// A cleared hard-veto layer is intentionally not `DecisionVerdict::Allow`.
    /// Downstream regime/playbook/evidence/EV/risk/execution authority is still required.
    #[must_use]
    pub const fn blocking_decision(&self) -> Option<DecisionVerdict> {
        match self.verdict {
            HardVetoVerdict::Cleared => None,
            HardVetoVerdict::Wait => Some(DecisionVerdict::Wait),
            HardVetoVerdict::Reject => Some(DecisionVerdict::Reject),
        }
    }
}

/// AI-native GEX spec §§16,26,30,33,39-42.
///
/// FAIL has precedence over UNKNOWN because a known hard failure is a deterministic rejection.
/// UNKNOWN becomes WAIT only when no mandatory hard condition is known to have failed.
pub fn evaluate_hard_vetoes(
    snapshot: &MarketSnapshot,
    input: &HardVetoInput,
) -> Result<HardVetoResult, HardVetoError> {
    if input.snapshot_id.trim().is_empty() {
        return Err(HardVetoError::MissingSnapshotId);
    }
    if input.snapshot_id != snapshot.snapshot_id() {
        return Err(HardVetoError::SnapshotMismatch);
    }

    let conditions = [
        (HardConditionKind::DataValid, input.data_valid),
        (HardConditionKind::DataFresh, input.data_fresh),
        (HardConditionKind::GexScopeValid, input.gex_scope_valid),
        (
            HardConditionKind::RequiredRegimeDataValid,
            input.required_regime_data_valid,
        ),
        (HardConditionKind::BrokerConnected, input.broker_connected),
        (HardConditionKind::OrderStateValid, input.order_state_valid),
        (
            HardConditionKind::InstrumentTradable,
            input.instrument_tradable,
        ),
        (
            HardConditionKind::LiquidityWithinLimits,
            input.liquidity_within_limits,
        ),
        (
            HardConditionKind::MaxPositionRiskValid,
            input.max_position_risk_valid,
        ),
        (
            HardConditionKind::MaxPortfolioRiskValid,
            input.max_portfolio_risk_valid,
        ),
        (
            HardConditionKind::InvalidationDefined,
            input.invalidation_defined,
        ),
        (
            HardConditionKind::ExecutionCostWithinLimits,
            input.execution_cost_within_limits,
        ),
        (HardConditionKind::KillSwitchClear, input.kill_switch_clear),
    ];

    let mut failed_conditions = Vec::new();
    let mut unknown_conditions = Vec::new();
    for (kind, condition) in conditions {
        match condition {
            HardCondition::Pass => {}
            HardCondition::Fail => failed_conditions.push(kind),
            HardCondition::Unknown => unknown_conditions.push(kind),
        }
    }

    let verdict = if !failed_conditions.is_empty() {
        HardVetoVerdict::Reject
    } else if !unknown_conditions.is_empty() {
        HardVetoVerdict::Wait
    } else {
        HardVetoVerdict::Cleared
    };

    Ok(HardVetoResult {
        snapshot_id: input.snapshot_id.clone(),
        verdict,
        failed_conditions,
        unknown_conditions,
    })
}
