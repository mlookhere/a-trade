use crate::footprint::delta_magnitude_at_least_prior20_median;
use crate::{
    Condition, Direction, FootprintCandle5m, LocationSetup, ParticipationContext, SetupState,
    StructureKey,
};

#[derive(Debug, Clone, Copy, PartialEq)]
struct CandleSnapshot {
    open: f64,
    high: f64,
    low: f64,
    close: f64,
}

impl From<FootprintCandle5m<'_>> for CandleSnapshot {
    fn from(candle: FootprintCandle5m<'_>) -> Self {
        Self {
            open: candle.open,
            high: candle.high,
            low: candle.low,
            close: candle.close,
        }
    }
}

impl CandleSnapshot {
    fn midpoint(self) -> f64 {
        (self.high + self.low) / 2.0
    }
}

/// §85 mirrored application of §43 for canonical §88 aggressive buyers at premium.
#[must_use]
pub fn buyer_aggression(
    candle: FootprintCandle5m<'_>,
    previous_20_completed_deltas: &[i64],
    participation: Condition,
) -> Condition {
    if !candle.valid() {
        return Condition::Unknown;
    }

    all_conditions(&[
        Condition::from(candle.candle_delta > 0),
        candle.has_buy_imbalance_upper_half(),
        participation,
        delta_magnitude_at_least_prior20_median(candle.candle_delta, previous_20_completed_deltas),
    ])
}

/// §§85,88-89 mirrored buyer-absorption rejection structure. Buyer effort failure itself is a
/// separate tri-state input in the sequence because no numeric upward-progression algorithm is
/// specified by the canonical source.
#[must_use]
pub fn potential_buyer_absorption(
    candle: FootprintCandle5m<'_>,
    buyer_aggression_present: Condition,
) -> Condition {
    if !candle.valid() {
        return Condition::Unknown;
    }

    let range = candle.high - candle.low;
    let upper_wick = candle.high - candle.open.max(candle.close);
    let wick_rejection = upper_wick >= range * 0.25;
    let bearish_or_lower_half =
        candle.close < candle.open || candle.close <= (candle.high + candle.low) / 2.0;

    all_conditions(&[
        buyer_aggression_present,
        Condition::from(wick_rejection),
        Condition::from(bearish_or_lower_half),
    ])
}

/// §90 implemented as the explicit §85 mirror of the deterministic §49 dominance formalization.
#[must_use]
pub fn first_seller_dominance_shift(
    candle: FootprintCandle5m<'_>,
    aggression_candle_midpoint: f64,
    participation: Condition,
) -> Condition {
    if !candle.valid() || !aggression_candle_midpoint.is_finite() {
        return Condition::Unknown;
    }

    all_conditions(&[
        Condition::from(candle.close < candle.open),
        Condition::from(candle.close < aggression_candle_midpoint),
        Condition::from(candle.candle_delta < 0),
        candle.has_sell_imbalance(),
        participation,
    ])
}

/// §91 mirrored second-attempt requirement: buyers must trade above the prior dominance
/// candle midpoint with positive delta.
#[must_use]
pub fn genuine_second_buyer_attempt(
    test_candle: FootprintCandle5m<'_>,
    dominance_candle_midpoint: f64,
) -> Condition {
    if !test_candle.valid() || !dominance_candle_midpoint.is_finite() {
        return Condition::Unknown;
    }

    all_conditions(&[
        Condition::from(test_candle.high > dominance_candle_midpoint),
        Condition::from(test_candle.candle_delta > 0),
    ])
}

/// §91 mirrored real-buying requirement from the long §54 formalization.
#[must_use]
pub fn second_test_has_real_buying(
    test_candle: FootprintCandle5m<'_>,
    participation: Condition,
    first_failure_high: f64,
) -> Condition {
    if !test_candle.valid() || !first_failure_high.is_finite() {
        return Condition::Unknown;
    }

    all_conditions(&[
        Condition::from(test_candle.candle_delta > 0),
        test_candle.has_buy_imbalance(),
        participation,
        Condition::from(test_candle.close <= first_failure_high),
    ])
}

/// §91 one-tick lower-failure mirror of canonical §53.
#[must_use]
pub fn second_failure_lower(
    second_test_high: f64,
    first_failure_high: f64,
    tick_size: f64,
) -> Condition {
    if !second_test_high.is_finite()
        || !first_failure_high.is_finite()
        || !tick_size.is_finite()
        || tick_size <= 0.0
    {
        return Condition::Unknown;
    }

    Condition::from(second_test_high <= first_failure_high - tick_size)
}

/// §92 seller reconfirmation as the explicit mirror of §55.
#[must_use]
pub fn seller_reconfirmation(
    candle: FootprintCandle5m<'_>,
    second_test_midpoint: f64,
    participation: Condition,
) -> Condition {
    if !candle.valid() || !second_test_midpoint.is_finite() {
        return Condition::Unknown;
    }

    all_conditions(&[
        Condition::from(candle.close < candle.open),
        Condition::from(candle.candle_delta < 0),
        candle.has_sell_imbalance(),
        Condition::from(candle.close < second_test_midpoint),
        participation,
    ])
}

/// §85 mirrored bearish state coordinator. It can only begin from an actual short
/// LOCATION_REACHED lifecycle object, binds itself to that structure, and derives deterministic
/// participation from raw volume context rather than caller-asserted pass flags.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ShortOrderflowSequence {
    structure: StructureKey,
    state: SetupState,
    aggression: Option<CandleSnapshot>,
    dominance: Option<CandleSnapshot>,
    second_test: Option<CandleSnapshot>,
    reconfirmation: Option<CandleSnapshot>,
    first_failure_high: Option<f64>,
}

impl ShortOrderflowSequence {
    #[must_use]
    pub fn from_location_reached(location: &LocationSetup) -> Option<Self> {
        let structure = location.structure();
        (location.state() == SetupState::LocationReached && structure.direction == Direction::Short)
            .then_some(Self {
                structure,
                state: SetupState::LocationReached,
                aggression: None,
                dominance: None,
                second_test: None,
                reconfirmation: None,
                first_failure_high: None,
            })
    }

    #[must_use]
    pub const fn state(self) -> SetupState {
        self.state
    }

    #[must_use]
    pub(crate) const fn structure(self) -> StructureKey {
        self.structure
    }

    #[must_use]
    pub(crate) fn first_failure_extreme(self) -> Option<f64> {
        self.first_failure_high
    }

    #[must_use]
    pub(crate) fn reconfirmation_extreme(self) -> Option<f64> {
        self.reconfirmation.map(|candle| candle.low)
    }

    pub fn record_aggression(
        &mut self,
        candle: FootprintCandle5m<'_>,
        previous_20_completed_deltas: &[i64],
        participation: ParticipationContext<'_>,
        buyer_effort_failed: Condition,
    ) -> Condition {
        if self.state != SetupState::LocationReached {
            return Condition::False;
        }

        let participation = participation.evaluate(candle);
        let aggression = buyer_aggression(candle, previous_20_completed_deltas, participation);
        let at_extreme = candle.aggression_at_short_extreme();
        let result = all_conditions(&[aggression, at_extreme, buyer_effort_failed]);
        if result.permits()
            && let Ok(next) = self.state.advance(SetupState::AggressionPresent)
        {
            self.state = next;
            self.aggression = Some(candle.into());
        }
        result
    }

    /// §91 says FIRST_FAILURE_HIGH is stored but does not specify a machine extraction
    /// algorithm, so it remains an explicit finite input rather than an invented candle rule.
    /// Once accepted, the stored value is the only one later order construction may use.
    pub fn record_absorption(&mut self, first_failure_high: f64) -> Condition {
        if self.state != SetupState::AggressionPresent {
            return Condition::False;
        }
        if !first_failure_high.is_finite() {
            return Condition::Unknown;
        }
        let Some(aggression) = self.aggression else {
            return Condition::Unknown;
        };

        let rejection = snapshot_buyer_absorption(aggression);
        if rejection.permits()
            && let Ok(next) = self.state.advance(SetupState::PotentialAbsorption)
        {
            self.state = next;
            self.first_failure_high = Some(first_failure_high);
        }
        rejection
    }

    pub fn record_first_dominance_shift(
        &mut self,
        candle: FootprintCandle5m<'_>,
        participation: ParticipationContext<'_>,
    ) -> Condition {
        if self.state != SetupState::PotentialAbsorption {
            return Condition::False;
        }
        let Some(aggression) = self.aggression else {
            return Condition::Unknown;
        };

        let result = first_seller_dominance_shift(
            candle,
            aggression.midpoint(),
            participation.evaluate(candle),
        );
        if result.permits()
            && let Ok(next) = self.state.advance(SetupState::FirstDominanceShift)
        {
            self.state = next;
            self.dominance = Some(candle.into());
        }
        result
    }

    pub fn begin_second_test(&mut self) -> Condition {
        if self.state != SetupState::FirstDominanceShift {
            return Condition::False;
        }
        match self.state.advance(SetupState::WaitingSecondTest) {
            Ok(next) => {
                self.state = next;
                Condition::True
            }
            Err(_) => Condition::False,
        }
    }

    pub fn record_second_test(
        &mut self,
        candle: FootprintCandle5m<'_>,
        participation: ParticipationContext<'_>,
    ) -> Condition {
        if self.state != SetupState::WaitingSecondTest {
            return Condition::False;
        }
        let (Some(dominance), Some(first_failure_high)) = (self.dominance, self.first_failure_high)
        else {
            return Condition::Unknown;
        };

        let result = all_conditions(&[
            genuine_second_buyer_attempt(candle, dominance.midpoint()),
            second_test_has_real_buying(
                candle,
                participation.evaluate(candle),
                first_failure_high,
            ),
        ]);
        if result.permits()
            && let Ok(next) = self.state.advance(SetupState::SecondTest)
        {
            self.state = next;
            self.second_test = Some(candle.into());
        }
        result
    }

    pub fn confirm_second_failure(&mut self, tick_size: f64) -> Condition {
        if self.state != SetupState::SecondTest {
            return Condition::False;
        }
        let (Some(test), Some(first_failure_high)) = (self.second_test, self.first_failure_high)
        else {
            return Condition::Unknown;
        };

        let result = second_failure_lower(test.high, first_failure_high, tick_size);
        if result.permits()
            && let Ok(next) = self.state.advance(SetupState::SecondFailure)
        {
            self.state = next;
        }
        result
    }

    pub fn record_reconfirmation(
        &mut self,
        candle: FootprintCandle5m<'_>,
        participation: ParticipationContext<'_>,
    ) -> Condition {
        if self.state != SetupState::SecondFailure {
            return Condition::False;
        }
        let Some(test) = self.second_test else {
            return Condition::Unknown;
        };

        let result = seller_reconfirmation(
            candle,
            test.midpoint(),
            participation.evaluate(candle),
        );
        if result.permits()
            && let Ok(next) = self.state.advance(SetupState::FinalReconfirmation)
        {
            self.state = next;
            self.reconfirmation = Some(candle.into());
        }
        result
    }
}

fn snapshot_buyer_absorption(candle: CandleSnapshot) -> Condition {
    if !candle.open.is_finite()
        || !candle.high.is_finite()
        || !candle.low.is_finite()
        || !candle.close.is_finite()
        || candle.high <= candle.low
    {
        return Condition::Unknown;
    }

    let range = candle.high - candle.low;
    let upper_wick = candle.high - candle.open.max(candle.close);
    let wick_rejection = upper_wick >= range * 0.25;
    let bearish_or_lower_half = candle.close < candle.open || candle.close <= candle.midpoint();
    all_conditions(&[
        Condition::from(wick_rejection),
        Condition::from(bearish_or_lower_half),
    ])
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
