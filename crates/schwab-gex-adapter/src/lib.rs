#![forbid(unsafe_code)]

pub mod config;
pub mod fleet;
pub mod oauth;
pub mod rate;
pub mod rest;
pub mod state;
pub mod stream;

pub use config::{ConfigError, SchwabConfig, SchwabProfileConfig, SecretString};
pub use fleet::{ProfileAssignment, ProfileClients, ProfilePool};
pub use oauth::{OAuthClient, TokenSet, load_tokens, save_tokens};
pub use rate::{BudgetDecision, BudgetError, RestBudget, RestRateLimiter};
pub use rest::{SchwabRestClient, StreamerInfo};
pub use state::{
    GammaQuality, RebootstrapSchedule, RefreshError, ReliableGexSurface, SchwabGexState,
    StreamApply,
};
pub use stream::{
    StreamCommand, StreamDataBatch, StreamEvent, StreamRequestFactory, StreamResponse,
    StreamResponseContent, StreamService, SchwabStreamClient,
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
}

impl From<BudgetError> for AdapterError {
    fn from(value: BudgetError) -> Self {
        Self::RateBudget(value)
    }
}
