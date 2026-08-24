/// Exact source retracement levels from canonical §§29-32.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FibLevels {
    pub level_705: f64,
    pub level_788: f64,
    pub level_886: f64,
}

impl FibLevels {
    pub(crate) fn long_ordered(self) -> bool {
        self.level_705.is_finite()
            && self.level_788.is_finite()
            && self.level_886.is_finite()
            && self.level_705 > self.level_788
            && self.level_788 > self.level_886
    }

    pub(crate) fn short_ordered(self) -> bool {
        self.level_705.is_finite()
            && self.level_788.is_finite()
            && self.level_886.is_finite()
            && self.level_705 < self.level_788
            && self.level_788 < self.level_886
    }
}

#[must_use]
pub fn bullish_fib(swing_low: f64, swing_high: f64) -> Option<FibLevels> {
    if !swing_low.is_finite() || !swing_high.is_finite() || swing_high <= swing_low {
        return None;
    }

    let range = swing_high - swing_low;
    Some(FibLevels {
        level_705: swing_high - range * 0.705,
        level_788: swing_high - range * 0.788,
        level_886: swing_high - range * 0.886,
    })
}

/// Mirrored bearish retracement construction under canonical §§28-32 and §§85-87.
#[must_use]
pub fn bearish_fib(swing_high: f64, swing_low: f64) -> Option<FibLevels> {
    if !swing_low.is_finite() || !swing_high.is_finite() || swing_high <= swing_low {
        return None;
    }

    let range = swing_high - swing_low;
    Some(FibLevels {
        level_705: swing_low + range * 0.705,
        level_788: swing_low + range * 0.788,
        level_886: swing_low + range * 0.886,
    })
}

/// Canonical §31: the long zone must sit completely below value.
#[must_use]
pub fn long_location_valid(levels: FibLevels, reference_val: f64) -> bool {
    reference_val.is_finite() && levels.long_ordered() && levels.level_705 < reference_val
}

/// Canonical §32: the mirrored short zone must sit completely above value.
#[must_use]
pub fn short_location_valid(levels: FibLevels, reference_vah: f64) -> bool {
    reference_vah.is_finite() && levels.short_ordered() && levels.level_705 > reference_vah
}

/// Canonical §35, inclusive at both location boundaries.
#[must_use]
pub fn long_location_reached(levels: FibLevels, price: f64) -> bool {
    price.is_finite()
        && levels.long_ordered()
        && price <= levels.level_705
        && price >= levels.level_886
}

/// Mirrored location check for canonical §§85-87.
#[must_use]
pub fn short_location_reached(levels: FibLevels, price: f64) -> bool {
    price.is_finite()
        && levels.short_ordered()
        && price >= levels.level_705
        && price <= levels.level_886
}
