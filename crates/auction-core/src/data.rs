use crate::Condition;

/// Required evaluation-cycle health facts from canonical §16.
///
/// Provider-specific freshness durations are intentionally not defined here because the
/// canonical strategy does not specify them. Feed/broker adapters must supply these facts;
/// FALSE or UNKNOWN fails closed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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
        let facts = [
            self.quote_fresh,
            self.footprint_fresh,
            self.volume_profile_available,
            self.broker_state_known,
            self.position_state_known,
            self.open_order_state_known,
            self.timestamps_synchronized,
        ];

        if facts.contains(&Condition::False) {
            Condition::False
        } else if facts.contains(&Condition::Unknown) {
            Condition::Unknown
        } else {
            Condition::True
        }
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
