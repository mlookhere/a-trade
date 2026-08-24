use crate::Condition;

pub const MNQ_PARTICIPATION_THRESHOLD: u64 = 20_000;
pub const IMBALANCE_THRESHOLD: f64 = 4.0;
pub const EXTREME_VOLUME_PERCENT: u64 = 35;

/// Provider-neutral footprint level. Buy/sell imbalance ratios are supplied by the configured
/// footprint calculator so this core does not invent a diagonal pairing methodology (§41).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FootprintLevel {
    pub price: f64,
    pub bid_volume: u64,
    pub ask_volume: u64,
    pub delta: i64,
    pub buy_imbalance_ratio: Option<f64>,
    pub sell_imbalance_ratio: Option<f64>,
}

impl FootprintLevel {
    fn valid(self, low: f64, high: f64) -> bool {
        self.price.is_finite()
            && self.price >= low
            && self.price <= high
            && ratio_valid(self.buy_imbalance_ratio)
            && ratio_valid(self.sell_imbalance_ratio)
    }
}

const fn ratio_valid(ratio: Option<f64>) -> bool {
    match ratio {
        Some(value) => value.is_finite() && value >= 0.0,
        None => true,
    }
}

/// Canonical §§12 and 40 execution-candle contract. Only completed 5-minute footprint candles
/// are eligible for setup confirmation.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FootprintCandle5m<'a> {
    pub open: f64,
    pub high: f64,
    pub low: f64,
    pub close: f64,
    pub total_volume: u64,
    pub candle_delta: i64,
    pub volume_poc: f64,
    pub levels: &'a [FootprintLevel],
    pub completed: bool,
}

impl FootprintCandle5m<'_> {
    #[must_use]
    pub fn valid(self) -> bool {
        if !self.completed
            || !self.open.is_finite()
            || !self.high.is_finite()
            || !self.low.is_finite()
            || !self.close.is_finite()
            || !self.volume_poc.is_finite()
            || self.high <= self.low
            || self.open < self.low
            || self.open > self.high
            || self.close < self.low
            || self.close > self.high
            || self.volume_poc < self.low
            || self.volume_poc > self.high
            || self.total_volume == 0
            || self.levels.is_empty()
            || !self
                .levels
                .iter()
                .copied()
                .all(|level| level.valid(self.low, self.high))
        {
            return false;
        }

        let level_volume = self.levels.iter().fold(0_u128, |sum, level| {
            sum + u128::from(level.bid_volume) + u128::from(level.ask_volume)
        });
        if level_volume != u128::from(self.total_volume) {
            return false;
        }

        let level_delta = self
            .levels
            .iter()
            .fold(0_i128, |sum, level| sum + i128::from(level.delta));
        level_delta == i128::from(self.candle_delta)
    }

    #[must_use]
    pub fn midpoint(self) -> Option<f64> {
        self.valid().then_some((self.high + self.low) / 2.0)
    }

    #[must_use]
    pub fn has_sell_imbalance(self) -> Condition {
        if !self.valid() {
            return Condition::Unknown;
        }
        Condition::from(self.levels.iter().any(|level| {
            level
                .sell_imbalance_ratio
                .is_some_and(|ratio| ratio >= IMBALANCE_THRESHOLD)
        }))
    }

    #[must_use]
    pub fn has_sell_imbalance_lower_half(self) -> Condition {
        let Some(midpoint) = self.midpoint() else {
            return Condition::Unknown;
        };
        Condition::from(self.levels.iter().any(|level| {
            level.price <= midpoint
                && level
                    .sell_imbalance_ratio
                    .is_some_and(|ratio| ratio >= IMBALANCE_THRESHOLD)
        }))
    }

    #[must_use]
    pub fn has_buy_imbalance(self) -> Condition {
        if !self.valid() {
            return Condition::Unknown;
        }
        Condition::from(self.levels.iter().any(|level| {
            level
                .buy_imbalance_ratio
                .is_some_and(|ratio| ratio >= IMBALANCE_THRESHOLD)
        }))
    }

    #[must_use]
    pub fn aggression_at_long_extreme(self) -> Condition {
        if !self.valid() {
            return Condition::Unknown;
        }

        let range = self.high - self.low;
        let bottom_quarter_top = self.low + range * 0.25;
        let poc_in_bottom_quarter = self.volume_poc <= bottom_quarter_top;
        let bottom_quarter_volume = self
            .levels
            .iter()
            .filter(|level| level.price <= bottom_quarter_top)
            .fold(0_u128, |sum, level| {
                sum + u128::from(level.bid_volume) + u128::from(level.ask_volume)
            });
        let enough_bottom_quarter_volume = bottom_quarter_volume * 100
            >= u128::from(self.total_volume) * u128::from(EXTREME_VOLUME_PERCENT);

        Condition::from(poc_in_bottom_quarter || enough_bottom_quarter_volume)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ParticipationRule {
    MnqSourceThreshold,
    NonMnqTimeNormalizedRvol,
}

/// Canonical §§38-40. The MNQ source threshold is isolated from the non-MNQ starting model.
/// Non-MNQ requires exactly 20 prior same-time-bucket session volumes and compares current
/// volume to their median without floating-point division.
#[must_use]
pub fn participation_valid(
    candle: FootprintCandle5m<'_>,
    rule: ParticipationRule,
    prior_same_bucket_volumes: Option<&[u64]>,
) -> Condition {
    if !candle.valid() {
        return Condition::Unknown;
    }

    match rule {
        ParticipationRule::MnqSourceThreshold => {
            Condition::from(candle.total_volume >= MNQ_PARTICIPATION_THRESHOLD)
        }
        ParticipationRule::NonMnqTimeNormalizedRvol => {
            let Some(history) = prior_same_bucket_volumes else {
                return Condition::Unknown;
            };
            let Some((middle_low, middle_high)) = median_middle_pair(history) else {
                return Condition::Unknown;
            };
            let median_twice = u128::from(middle_low) + u128::from(middle_high);
            if median_twice == 0 {
                return Condition::Unknown;
            }
            Condition::from(u128::from(candle.total_volume) * 2 >= median_twice)
        }
    }
}

fn median_middle_pair(values: &[u64]) -> Option<(u64, u64)> {
    let mut sorted: [u64; 20] = values.try_into().ok()?;
    sorted.sort_unstable();
    Some((sorted[9], sorted[10]))
}
