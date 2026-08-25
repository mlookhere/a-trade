#![forbid(unsafe_code)]

pub mod config;
pub mod connections;
pub mod fleet;
pub mod oauth;
pub mod protocol;
pub mod rate;
pub mod rest;
pub mod runtime;
pub mod state;
pub mod stream;

pub use config::{ConfigError, SchwabConfig, SchwabProfileConfig, SecretString};
pub use connections::{
    ProfileConnectOutcome, ProfileStreamConnect, ProfileStreamSession, connect_profiles,
};
pub use fleet::{ProfileAssignment, ProfileClients, ProfilePool};
pub use oauth::{OAuthClient, TokenSet, load_tokens, save_tokens};
pub use protocol::{
    StreamResponseAction, StreamResponseCode, require_response_success, response_action,
};
pub use rate::{BudgetDecision, BudgetError, RestBudget, RestRateLimiter};
pub use rest::{SchwabRestClient, StreamerInfo};
pub use runtime::{
    DispatchReport, ProfileSubscriptionPlan, RuntimeQuality, SchwabFleetRuntime,
};
pub use state::{
    GammaQuality, RebootstrapSchedule, RefreshError, ReliableGexSurface, SchwabGexState,
    StreamApply,
};
pub use stream::{
    SchwabStreamClient, StreamCommand, StreamDataBatch, StreamEvent, StreamRequestFactory,
    StreamResponse, StreamResponseContent, StreamService,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AdapterError {
    InvalidInput(&'static str),
    Transport(String),
    Provider(String),
    ProviderContract(String),
    TokenStore(String),
    UnderlyingMismatch,
    Gex(String),
    RateBudget(BudgetError),
    NoHealthyProfile,
    UnknownProfile,
    DuplicateProfileConnection,
    DuplicateStreamerUser,
    StreamLoginDenied,
    StreamConnectionLimit,
    StreamSymbolLimit,
    StreamUnavailable,
}

impl From<BudgetError> for AdapterError {
    fn from(value: BudgetError) -> Self {
        Self::RateBudget(value)
    }
}
