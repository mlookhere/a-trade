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

/// Canonical §§14-15. Gamma is contextual information only and exposes no directional
/// permission.
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RelativeExpectation {
    Higher,
    Lower,
}

/// Exact §15 interpretation outputs. Fields omitted by the canonical rule for a regime remain
/// `None`; the implementation does not infer them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GammaInterpretation {
    pub volatility: VolatilityExpectation,
    pub mean_reversion: Option<RelativeExpectation>,
    pub breakout_persistence: Option<RelativeExpectation>,
    pub move_acceleration: Option<RelativeExpectation>,
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
        self.interpretation().volatility
    }

    /// Canonical §15 only. No direction is produced from gamma regime.
    #[must_use]
    pub const fn interpretation(self) -> GammaInterpretation {
        match self.regime {
            GammaRegime::Positive => GammaInterpretation {
                volatility: VolatilityExpectation::Dampened,
                mean_reversion: Some(RelativeExpectation::Higher),
                breakout_persistence: Some(RelativeExpectation::Lower),
                move_acceleration: None,
            },
            GammaRegime::Negative => GammaInterpretation {
                volatility: VolatilityExpectation::Amplified,
                mean_reversion: None,
                breakout_persistence: None,
                move_acceleration: Some(RelativeExpectation::Higher),
            },
            GammaRegime::Unavailable => GammaInterpretation {
                volatility: VolatilityExpectation::Unknown,
                mean_reversion: None,
                breakout_persistence: None,
                move_acceleration: None,
            },
        }
    }

    #[must_use]
    pub fn levels_valid(self) -> bool {
        self.flip.is_none_or(f64::is_finite)
            && self.call_wall.is_none_or(f64::is_finite)
            && self.put_wall.is_none_or(f64::is_finite)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GammaSnapshotError {
    MissingUnderlying,
    MissingTimestamp,
    UnavailableRegime,
    InvalidLevel,
}

/// Provider-neutral §14 metadata for a reliable GEX observation. Timestamp text is intentionally
/// opaque: canonical knowledge requires that it be obtained but does not specify a timezone,
/// serialization format, or freshness duration.
#[derive(Debug, Clone, PartialEq)]
pub struct ReliableGammaSnapshot {
    underlying: String,
    timestamp: String,
    context: GammaContext,
}

impl ReliableGammaSnapshot {
    pub fn new(
        underlying: &str,
        timestamp: &str,
        context: GammaContext,
    ) -> Result<Self, GammaSnapshotError> {
        if underlying.trim().is_empty() {
            return Err(GammaSnapshotError::MissingUnderlying);
        }
        if timestamp.trim().is_empty() {
            return Err(GammaSnapshotError::MissingTimestamp);
        }
        if context.regime == GammaRegime::Unavailable {
            return Err(GammaSnapshotError::UnavailableRegime);
        }
        if !context.levels_valid() {
            return Err(GammaSnapshotError::InvalidLevel);
        }

        Ok(Self {
            underlying: underlying.to_owned(),
            timestamp: timestamp.to_owned(),
            context,
        })
    }

    #[must_use]
    pub fn underlying(&self) -> &str {
        &self.underlying
    }

    #[must_use]
    pub fn timestamp(&self) -> &str {
        &self.timestamp
    }

    #[must_use]
    pub const fn context(&self) -> GammaContext {
        self.context
    }

    #[must_use]
    pub const fn interpretation(&self) -> GammaInterpretation {
        self.context.interpretation()
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
