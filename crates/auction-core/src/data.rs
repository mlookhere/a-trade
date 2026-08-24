use crate::Condition;

/// Required evaluation-cycle health facts from canonical §16.
///
/// Provider-specific freshness durations are intentionally not defined here because the
/// canonical strategy does not specify them. Feed/broker adapters must supply these facts;
/// FALSE or UNKNOWN fails closed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct DataHealth {
    pub quote_fresh: Condition,
    pub footprint_fresh: Condition,
    pub volume_profile_available: Condition,
    pub broker_state_known: Condition,
    pub position_state_known: Condition,
    pub open_order_state_known: Condition,
    pub timestamps_synchronized: Condition,
}

impl DataHealth {
    /// Canonical §16 and §108 tri-state conjunction.
    #[must_use]
    pub fn validity(self) -> Condition {
        all_conditions(&[
            self.quote_fresh,
            self.footprint_fresh,
            self.volume_profile_available,
            self.broker_state_known,
            self.position_state_known,
            self.open_order_state_known,
            self.timestamps_synchronized,
        ])
    }
}

/// Exact required market-data families from canonical §11. These are availability/validity
/// facts only; provider-specific bar construction and freshness thresholds remain outside the
/// strategy core.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct RequiredMarketDataStatus {
    pub current_bid: Condition,
    pub current_ask: Condition,
    pub last_trade: Condition,
    pub ohlcv_1m: Condition,
    pub ohlcv_5m: Condition,
    pub ohlcv_15m: Condition,
    pub ohlcv_1h: Condition,
    pub ohlcv_4h: Condition,
}

impl RequiredMarketDataStatus {
    #[must_use]
    pub fn readiness(self) -> Condition {
        all_conditions(&[
            self.current_bid,
            self.current_ask,
            self.last_trade,
            self.ohlcv_1m,
            self.ohlcv_5m,
            self.ohlcv_15m,
            self.ohlcv_1h,
            self.ohlcv_4h,
        ])
    }
}

/// Exact required volume-profile families and methodology controls from canonical §13.
/// The profile calculation formula itself is intentionally not defined because canonical
/// knowledge does not specify one.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct RequiredVolumeProfileStatus {
    pub vah: Condition,
    pub val: Condition,
    pub poc: Condition,
    pub volume_at_price: Condition,
    pub low_volume_nodes: Condition,
    pub high_volume_nodes: Condition,
    pub same_methodology_every_session: Condition,
    pub methodology_unchanged_intraday: Condition,
}

impl RequiredVolumeProfileStatus {
    #[must_use]
    pub fn readiness(self) -> Condition {
        all_conditions(&[
            self.vah,
            self.val,
            self.poc,
            self.volume_at_price,
            self.low_volume_nodes,
            self.high_volume_nodes,
            self.same_methodology_every_session,
            self.methodology_unchanged_intraday,
        ])
    }
}

/// Deterministic §§11,13,16 data-validity composition. This adds no provider assumptions: each
/// source adapter supplies tri-state facts, and the core only enforces that every required fact
/// is TRUE before data can be considered valid.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct DataCycleReadiness {
    pub health: DataHealth,
    pub market_data: RequiredMarketDataStatus,
    pub volume_profile: RequiredVolumeProfileStatus,
}

impl DataCycleReadiness {
    #[must_use]
    pub fn validity(self) -> Condition {
        all_conditions(&[
            self.health.validity(),
            self.market_data.readiness(),
            self.volume_profile.readiness(),
        ])
    }
}

/// Canonical §§14-15. Gamma is volatility context only and exposes no directional permission.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GammaRegime {
    Positive,
    Negative,
    Unavailable,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VolatilityExpectation {
    Dampened,
    Amplified,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GammaContext {
    pub regime: GammaRegime,
    pub flip: Option<f64>,
    pub call_wall: Option<f64>,
    pub put_wall: Option<f64>,
}

impl GammaContext {
    #[must_use]
    pub const fn volatility_expectation(self) -> VolatilityExpectation {
        match self.regime {
            GammaRegime::Positive => VolatilityExpectation::Dampened,
            GammaRegime::Negative => VolatilityExpectation::Amplified,
            GammaRegime::Unavailable => VolatilityExpectation::Unknown,
        }
    }
}

fn all_conditions(conditions: &[Condition]) -> Condition {
    if conditions.contains(&Condition::False) {
        Condition::False
    } else if conditions.contains(&Condition::Unknown) {
        Condition::Unknown
    } else {
        Condition::True
    }
}
