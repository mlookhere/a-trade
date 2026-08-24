#![forbid(unsafe_code)]

pub mod authority;
pub mod authorization;
pub mod data;
pub mod environment;
pub mod fibonacci;
pub mod footprint;
pub mod location;
pub mod long_orderflow;
pub mod market;
pub mod rejection;
pub mod session;
pub mod setup_state;
pub mod short_orderflow;
pub mod swings;
pub mod truth;

pub use authorization::{AuthorizationInputs, ProductionConditions, trade_authorized};
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
pub use rejection::RejectionCode;
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
