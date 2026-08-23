#![forbid(unsafe_code)]

pub mod authority;
pub mod authorization;
pub mod fibonacci;
pub mod market;
pub mod rejection;
pub mod session;
pub mod setup_state;
pub mod truth;

pub use authorization::{AuthorizationInputs, ProductionConditions, trade_authorized};
pub use fibonacci::{
    FibLevels, bearish_fib, bullish_fib, long_location_reached, long_location_valid,
    short_location_reached, short_location_valid,
};
pub use market::{Direction, MarketState, direction_allowed};
pub use rejection::RejectionCode;
pub use session::{EtTime, SessionPermissions};
pub use setup_state::{SetupState, SetupStateError, TerminalState};
pub use truth::Condition;
