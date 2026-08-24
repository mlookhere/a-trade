#![forbid(unsafe_code)]

pub mod authority;
pub mod authorization;
pub mod data;
pub mod environment;
pub mod fibonacci;
pub mod location;
pub mod market;
pub mod rejection;
pub mod session;
pub mod setup_state;
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
pub use location::{
    LocationEvent, LocationSetup, StructureKey, long_886_invalidated, short_886_invalidated,
};
pub use market::{Direction, MarketState, direction_allowed};
pub use rejection::RejectionCode;
pub use session::{EtTime, SessionPermissions};
pub use setup_state::{SetupState, SetupStateError, TerminalState};
pub use swings::{
    Bar15, ConfirmedSwing, SwingImpulse, SwingKind, confirmed_swings, latest_bearish_impulse,
    latest_bullish_impulse, qualified_impulse,
};
pub use truth::Condition;
