use crate::footprint::delta_magnitude_at_least_prior20_median;
use crate::{
    Condition, Direction, FootprintCandle5m, LocationSetup, ParticipationContext,
    ParticipationRule, SetupState, StructureKey, participation_valid,
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

/// Canonical §43 deterministic seller-aggression test.
#[must_use]
pub fn seller_aggression(
    candle: FootprintCandle5m<'_>,
    previous_20_completed_deltas: &[i64],
    participation: Condition,
) -> Condition {
    if !candle.valid() {
        return Condition::Unknown;
    }

    all_conditions(&[
        Condition::from(candle.candle_delta < 0),
        candle.has_sell_imbalance_lower_half(),
        participation,
        delta_magnitude_at_least_prior20_median(candle.candle_delta, previous_20_completed_deltas),
    ])
}

/// Canonical §47 rejection structure. `seller_aggression_present` is supplied separately so
/// absorption cannot be inferred from candle shape without the preceding aggression gate.
#[must_use]
pub fn potential_absorption(
    candle: FootprintCandle5m<'_>,
    seller_aggression_present: Condition,
) -> Condition {
    if !candle.valid() {
        return Condition::Unknown;
    }

    let range = candle.high - candle.low;
    let lower_wick = candle.open.min(candle.close) - candle.low;
    let wick_rejection = lower_wick >= range * 0.25;
    let bullish_or_upper_half =
        candle.close > candle.open || candle.close >= (candle.high + candle.low) / 2.0;

    all_conditions(&[
        seller_aggression_present,
        Condition::from(wick_rejection),
        Condition::from(bullish_or_upper_half),
    ])
}

/// Canonical §49 completed-5M first buyer dominance shift.
#[must_use]
pub fn first_buyer_dominance_shift(
    candle: FootprintCandle5m<'_>,
    aggression_candle_midpoint: f64,
    participation: Condition,
) -> Condition {
    if !candle.valid() || !aggression_candle_midpoint.is_finite() {
        return Condition::Unknown;
    }

    all_conditions(&[
        Condition::from(candle.close > candle.open),
        Condition::from(candle.close > aggression_candle_midpoint),
        Condition::from(candle.candle_delta > 0),
        candle.has_buy_imbalance(),
        participation,
    ])
}

/// Canonical §51 genuine second seller attempt.
#[must_use]
pub fn genuine_second_seller_attempt(
    test_candle: FootprintCandle5m<'_>,
    dominance_candle_midpoint: f64,
) -> Condition {
    if !test_candle.valid() || !dominance_candle_midpoint.is_finite() {
        return Condition::Unknown;
    }

    all_conditions(&[
        Condition::from(test_candle.low < dominance_candle_midpoint),
        Condition::from(test_candle.candle_delta < 0),
    ])
}

/// Canonical §54 real selling on the second test. "Sell imbalance" uses the same configured
/// §41 >=4.0 imbalance definition used throughout this implementation.
#[must_use]
pub fn second_test_has_real_selling(
    test_candle: FootprintCandle5m<'_>,
    participation: Condition,
    first_failure_low: f64,
) -> Condition {
    if !test_candle.valid() || !first_failure_low.is_finite() {
        return Condition::Unknown;
    }

    all_conditions(&[
        Condition::from(test_candle.candle_delta < 0),
        test_candle.has_sell_imbalance(),
        participation,
        Condition::from(test_candle.close >= first_failure_low),
    ])
}

/// Canonical §53. The tick relationship is the deterministic minimum required by the spec.
#[must_use]
pub fn second_failure_higher(
    second_test_low: f64,
    first_failure_low: f64,
    tick_size: f64,
) -> Condition {
    if !second_test_low.is_finite()
        || !first_failure_low.is_finite()
        || !tick_size.is_finite()
        || tick_size <= 0.0
    {
        return Condition::Unknown;
    }

    Condition::from(second_test_low >= first_failure_low + tick_size)
}

/// Canonical §55 completed-5M buyer reconfirmation.
#[must_use]
pub fn buyer_reconfirmation(
    candle: FootprintCandle5m<'_>,
    second_test_midpoint: f64,
    participation: Condition,
) -> Condition {
    if !candle.valid() || !second_test_midpoint.is_finite() {
        return Condition::Unknown;
    }

    all_conditions(&[
        Condition::from(candle.close > candle.open),
        Condition::from(candle.candle_delta > 0),
        candle.has_buy_imbalance(),
        Condition::from(candle.close > second_test_midpoint),
        participation,
    ])
}

/// Long-side state coordinator for canonical §§42-55 and §102. It can only begin from an actual
/// long LOCATION_REACHED lifecycle object, binds itself to that structure, and derives every
/// deterministic participation condition from raw volume context. It has no execution API.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LongOrderflowSequence {
    structure: StructureKey,
    state: SetupState,
    aggression: Option<CandleSnapshot>,
    dominance: Option<CandleSnapshot>,
    second_test: Option<CandleSnapshot>,
    reconfirmation: Option<CandleSnapshot>,
    first_failure_low: Option<f64>,
}

impl LongOrderflowSequence {
    #[must_use]
    pub fn from_location_reached(location: &LocationSetup) -> Option<Self> {
        let structure = location.structure();
        (location.state() == SetupState::LocationReached && structure.direction == Direction::Long)
            .then_some(Self {
                structure,
                state: SetupState::LocationReached,
                aggression: None,
                dominance: None,
                second_test: None,
                reconfirmation: None,
                first_failure_low: None,
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
        self.first_failure_low
    }

    #[must_use]
    pub(crate) fn reconfirmation_extreme(self) -> Option<f64> {
        self.reconfirmation.map(|candle| candle.high)
    }

    /// §§43-46. `seller_effort_failed` is deliberately tri-state because canonical §§45-46 do
    /// not define a numeric algorithm for "meaningful" price progression. Participation itself
    /// is recomputed here from raw volume history and cannot be asserted by the caller.
    pub fn record_aggression(
        &mut self,
        candle: FootprintCandle5m<'_>,
        previous_20_completed_deltas: &[i64],
        participation: ParticipationContext<'_>,
        seller_effort_failed: Condition,
    ) -> Condition {
        if self.state != SetupState::LocationReached {
            return Condition::False;
        }

        let participation = participation.evaluate(candle);
        let aggression = seller_aggression(candle, previous_20_completed_deltas, participation);
        let at_extreme = candle.aggression_at_long_extreme();
        let result = all_conditions(&[aggression, at_extreme, seller_effort_failed]);
        if result.permits()
            && let Ok(next) = self.state.advance(SetupState::AggressionPresent)
        {
            self.state = next;
            self.aggression = Some(candle.into());
        }
        result
    }

    /// §§47-48 and §52. The source says FIRST_FAILURE_LOW comes from the original absorption
    /// structure but does not define an extraction algorithm, so it is an explicit finite input
    /// rather than silently assumed to equal one particular candle low. Once accepted, the
    /// value is retained inside the sequence and later order construction cannot replace it.
    pub fn record_absorption(&mut self, first_failure_low: f64) -> Condition {
        if self.state != SetupState::AggressionPresent {
            return Condition::False;
        }
        if !first_failure_low.is_finite() {
            return Condition::Unknown;
        }
        let Some(aggression) = self.aggression else {
            return Condition::Unknown;
        };

        let rejection = snapshot_absorption(aggression);
        if rejection.permits()
            && let Ok(next) = self.state.advance(SetupState::PotentialAbsorption)
        {
            self.state = next;
            self.first_failure_low = Some(first_failure_low);
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

        let result = first_buyer_dominance_shift(
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

    /// Canonical §50 makes the waiting state mandatory and explicit.
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

    /// §§51 and 54. Higher-low failure is deliberately checked in a separate state transition
    /// so SECOND_TEST cannot skip directly to SECOND_FAILURE.
    pub fn record_second_test(
        &mut self,
        candle: FootprintCandle5m<'_>,
        participation: ParticipationContext<'_>,
    ) -> Condition {
        if self.state != SetupState::WaitingSecondTest {
            return Condition::False;
        }
        let (Some(dominance), Some(first_failure_low)) = (self.dominance, self.first_failure_low)
        else {
            return Condition::Unknown;
        };

        let result = all_conditions(&[
            genuine_second_seller_attempt(candle, dominance.midpoint()),
            second_test_has_real_selling(candle, participation.evaluate(candle), first_failure_low),
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
        let (Some(test), Some(first_failure_low)) = (self.second_test, self.first_failure_low)
        else {
            return Condition::Unknown;
        };

        let result = second_failure_higher(test.low, first_failure_low, tick_size);
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

        let result = buyer_reconfirmation(candle, test.midpoint(), participation.evaluate(candle));
        if result.permits()
            && let Ok(next) = self.state.advance(SetupState::FinalReconfirmation)
        {
            self.state = next;
            self.reconfirmation = Some(candle.into());
        }
        result
    }
}

/// Same deterministic §47 candle-shape test as `potential_absorption`, applied to the stored
/// aggression snapshot after §43 has already passed.
fn snapshot_absorption(candle: CandleSnapshot) -> Condition {
    if !candle.open.is_finite()
        || !candle.high.is_finite()
        || !candle.low.is_finite()
        || !candle.close.is_finite()
        || candle.high <= candle.low
    {
        return Condition::Unknown;
    }

    let range = candle.high - candle.low;
    let lower_wick = candle.open.min(candle.close) - candle.low;
    let wick_rejection = lower_wick >= range * 0.25;
    let bullish_or_upper_half = candle.close > candle.open || candle.close >= candle.midpoint();
    all_conditions(&[
        Condition::from(wick_rejection),
        Condition::from(bullish_or_upper_half),
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

/// Convenience wrapper that binds §38/§39 participation selection to an execution candle.
#[must_use]
pub fn candle_participation(
    candle: FootprintCandle5m<'_>,
    rule: ParticipationRule,
    prior_same_bucket_volumes: Option<&[u64]>,
) -> Condition {
    participation_valid(candle, rule, prior_same_bucket_volumes)
}
