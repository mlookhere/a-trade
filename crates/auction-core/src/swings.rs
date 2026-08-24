use crate::{Direction, MarketState};

/// Minimal 15-minute bar input for canonical §26 pivot discovery.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Bar15 {
    pub high: f64,
    pub low: f64,
    pub closed: bool,
}

impl Bar15 {
    fn valid(self) -> bool {
        self.closed && self.high.is_finite() && self.low.is_finite() && self.high >= self.low
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SwingKind {
    High,
    Low,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ConfirmedSwing {
    pub index: usize,
    pub price: f64,
    pub kind: SwingKind,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SwingImpulse {
    pub direction: Direction,
    pub start_index: usize,
    pub end_index: usize,
    pub swing_low: f64,
    pub swing_high: f64,
}

/// Canonical §26 deterministic formalization: a pivot requires exactly two closed bars on
/// each side and strict greater/less-than comparisons. Invalid or open bars cannot confirm a
/// pivot.
#[must_use]
pub fn confirmed_swings(bars: &[Bar15]) -> Vec<ConfirmedSwing> {
    if bars.len() < 5 {
        return Vec::new();
    }

    let mut swings = Vec::new();
    for index in 2..bars.len() - 2 {
        let window = &bars[index - 2..=index + 2];
        if !window.iter().copied().all(Bar15::valid) {
            continue;
        }

        let center = bars[index];
        let swing_high = center.high > bars[index - 1].high
            && center.high > bars[index - 2].high
            && center.high > bars[index + 1].high
            && center.high > bars[index + 2].high;
        let swing_low = center.low < bars[index - 1].low
            && center.low < bars[index - 2].low
            && center.low < bars[index + 1].low
            && center.low < bars[index + 2].low;

        if swing_high {
            swings.push(ConfirmedSwing {
                index,
                price: center.high,
                kind: SwingKind::High,
            });
        }
        if swing_low {
            swings.push(ConfirmedSwing {
                index,
                price: center.low,
                kind: SwingKind::Low,
            });
        }
    }

    swings
}

/// Canonical §27: use the most recent confirmed swing low only when a confirmed swing high
/// exists chronologically after it. A newer low without a later confirmed high is not yet a
/// complete bullish impulse.
#[must_use]
pub fn latest_bullish_impulse(swings: &[ConfirmedSwing]) -> Option<SwingImpulse> {
    let low = swings.iter().rev().find(|swing| swing.kind == SwingKind::Low)?;
    let high = swings
        .iter()
        .rev()
        .find(|swing| swing.kind == SwingKind::High && swing.index > low.index)?;

    if high.price <= low.price {
        return None;
    }

    Some(SwingImpulse {
        direction: Direction::Long,
        start_index: low.index,
        end_index: high.index,
        swing_low: low.price,
        swing_high: high.price,
    })
}

/// Canonical §28 mirrored bearish impulse selection.
#[must_use]
pub fn latest_bearish_impulse(swings: &[ConfirmedSwing]) -> Option<SwingImpulse> {
    let high = swings
        .iter()
        .rev()
        .find(|swing| swing.kind == SwingKind::High)?;
    let low = swings
        .iter()
        .rev()
        .find(|swing| swing.kind == SwingKind::Low && swing.index > high.index)?;

    if high.price <= low.price {
        return None;
    }

    Some(SwingImpulse {
        direction: Direction::Short,
        start_index: high.index,
        end_index: low.index,
        swing_low: low.price,
        swing_high: high.price,
    })
}

/// §§24, 27-28: only the impulse matching deterministic directional permission is eligible.
#[must_use]
pub fn qualified_impulse(
    market_state: MarketState,
    swings: &[ConfirmedSwing],
) -> Option<SwingImpulse> {
    match market_state {
        MarketState::ValueUp => latest_bullish_impulse(swings),
        MarketState::ValueDown => latest_bearish_impulse(swings),
        MarketState::Balanced | MarketState::Unclear => None,
    }
}
