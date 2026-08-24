#![forbid(unsafe_code)]

pub mod audit;
pub mod authority;
pub mod authorization;
pub mod broker_execution;
pub mod data;
pub mod environment;
pub mod fibonacci;
pub mod footprint;
pub mod location;
pub mod long_orderflow;
pub mod market;
pub mod news;
pub mod order_risk;
pub mod position_management;
pub mod rejection;
pub mod safety;
pub mod session;
pub mod setup_state;
pub mod short_orderflow;
pub mod swings;
pub mod truth;

pub use audit::{
    AuditError, AuditLedger, CompletedTradeRecord, DailyMetrics, EffortResultChange,
    EffortResultSnapshot, RejectedSetupRecord, StoredRejectedSetupRecord,
};
pub use authorization::{AuthorizationInputs, ProductionConditions, trade_authorized};
pub use broker_execution::{
    BrokerAdapter, BrokerCapabilities, BrokerReconciliation, BrokerSubmission,
    ExecutionCoordinator, ExecutionGateContext, ExecutionPermit, ExecutionStatus,
    PreExecutionAuthority, SetupRegistry, SetupRegistryStatus,
};
pub use data::{DataHealth, GammaContext, GammaRegime, VolatilityExpectation};
pub use environment::{
    EnvironmentEvaluation, EnvironmentInput, HourlyStructure, PremarketReferences, SessionValue,
    evaluate_environment,
};
pub use fibonacci::{
    FibLevels, bearish_fib, bullish_fib, long_location_reached, long_location_valid,
    short_location_reached, short_location_valid,
};
pub use footprint::{
    EXTREME_VOLUME_PERCENT, FootprintCandle5m, FootprintLevel, IMBALANCE_THRESHOLD,
    MNQ_PARTICIPATION_THRESHOLD, ParticipationRule, participation_valid,
};
pub use location::{
    LocationEvent, LocationSetup, StructureKey, long_886_invalidated, short_886_invalidated,
};
pub use long_orderflow::{
    LongOrderflowSequence, buyer_reconfirmation, candle_participation, first_buyer_dominance_shift,
    genuine_second_seller_attempt, potential_absorption, second_failure_higher,
    second_test_has_real_selling, seller_aggression,
};
pub use market::{Direction, MarketState, direction_allowed};
pub use news::{NewsBlackoutWindow, NewsGateEvaluation, evaluate_news_gate};
pub use order_risk::{
    ClusterExposure, FuturesRiskInputs, InstrumentExecutionConfig, OrderProposal,
    OrderProposalInputs, OrderType, PortfolioRiskEvaluation, PortfolioRiskLimits, PositionSizing,
    STRICT_MIN_PLANNED_R, build_order_proposal, cluster_direction_clear, entry_limit_valid,
    entry_trigger, evaluate_portfolio_risk, planned_r, size_futures, slippage_guard,
    stop_replacement_allowed, structural_stop, structural_target_valid, trigger_expired,
};
pub use position_management::{
    ManagementDirective, StructuralTrailEvaluation, defensive_stop_candidate_allowed,
    effort_without_value_reclaim, favorable_effort_failure, favorable_effort_successful,
    final_target_directive, one_r_reached, reference_management_directive, structural_trail,
    value_reclaimed,
};
pub use rejection::RejectionCode;
pub use safety::{
    OperationalSafetyController, ProcessViolationScope, aggregate_operational_risk_clear,
    emergency_drawdown_reached,
};
pub use session::{EtTime, SessionPermissions};
pub use setup_state::{SetupState, SetupStateError, TerminalState};
pub use short_orderflow::{
    ShortOrderflowSequence, buyer_aggression, first_seller_dominance_shift,
    genuine_second_buyer_attempt, potential_buyer_absorption, second_failure_lower,
    second_test_has_real_buying, seller_reconfirmation,
};
pub use swings::{
    Bar15, ConfirmedSwing, SwingImpulse, SwingKind, confirmed_swings, latest_bearish_impulse,
    latest_bullish_impulse, qualified_impulse,
};
pub use truth::Condition;
