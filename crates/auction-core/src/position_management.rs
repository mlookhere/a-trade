use crate::{Condition, Direction, FootprintCandle5m, stop_replacement_allowed};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ManagementDirective {
    NoDirective,
    MaintainOrTrail,
    Tighten,
    ExitRemaining,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct StructuralTrailEvaluation {
    pub candidate_stop: Option<f64>,
    pub apply: Condition,
}

/// §§74/77 and explicit §94 mirror. Longs reclaim value from below at VAL; shorts reclaim from
/// above at VAH. Only completed, structurally valid 5M footprint candles are accepted.
#[must_use]
pub fn value_reclaimed(
    direction: Direction,
    candle: FootprintCandle5m<'_>,
    reference_value_boundary: f64,
) -> Condition {
    if !candle.valid() || !reference_value_boundary.is_finite() {
        return Condition::Unknown;
    }

    Condition::from(match direction {
        Direction::Long => candle.close >= reference_value_boundary,
        Direction::Short => candle.close <= reference_value_boundary,
    })
}

/// §75 exact two-candle long condition and explicit §94 mirrored short condition.
///
/// The canonical long wording says neither candle closes *above* REFERENCE_VAL, while §74 uses
/// close >= REFERENCE_VAL for reclaim. Those facts are intentionally evaluated independently so
/// equality is not silently reconciled into a new rule.
#[must_use]
pub fn effort_without_value_reclaim(
    direction: Direction,
    first: FootprintCandle5m<'_>,
    second: FootprintCandle5m<'_>,
    reference_value_boundary: f64,
) -> Condition {
    if !first.valid() || !second.valid() || !reference_value_boundary.is_finite() {
        return Condition::Unknown;
    }

    match direction {
        Direction::Long => {
            let imbalance = any_true(first.has_buy_imbalance(), second.has_buy_imbalance());
            if imbalance == Condition::Unknown {
                return Condition::Unknown;
            }
            Condition::from(
                first.candle_delta > 0
                    && second.candle_delta > 0
                    && imbalance == Condition::True
                    && first.close <= reference_value_boundary
                    && second.close <= reference_value_boundary,
            )
        }
        Direction::Short => {
            let imbalance = any_true(first.has_sell_imbalance(), second.has_sell_imbalance());
            if imbalance == Condition::Unknown {
                return Condition::Unknown;
            }
            Condition::from(
                first.candle_delta < 0
                    && second.candle_delta < 0
                    && imbalance == Condition::True
                    && first.close >= reference_value_boundary
                    && second.close >= reference_value_boundary,
            )
        }
    }
}

/// §79 healthy buyer candle and §94 mirrored healthy seller candle.
#[must_use]
pub fn favorable_effort_successful(
    direction: Direction,
    current: FootprintCandle5m<'_>,
    previous: FootprintCandle5m<'_>,
) -> Condition {
    if !current.valid() || !previous.valid() {
        return Condition::Unknown;
    }

    Condition::from(match direction {
        Direction::Long => {
            current.candle_delta > 0
                && (current.close > previous.close || current.high > previous.high)
        }
        Direction::Short => {
            current.candle_delta < 0
                && (current.close < previous.close || current.low < previous.low)
        }
    })
}

/// §80 buyer failure and §94 mirrored seller failure. The imbalance test reuses the canonical
/// centralized 4.0 footprint threshold rather than duplicating it here.
#[must_use]
pub fn favorable_effort_failure(
    direction: Direction,
    current: FootprintCandle5m<'_>,
    previous: FootprintCandle5m<'_>,
) -> Condition {
    if !current.valid() || !previous.valid() {
        return Condition::Unknown;
    }

    match direction {
        Direction::Long => match current.has_buy_imbalance() {
            Condition::Unknown => Condition::Unknown,
            imbalance => Condition::from(
                current.candle_delta > 0
                    && imbalance == Condition::True
                    && current.high <= previous.high
                    && current.close <= previous.close,
            ),
        },
        Direction::Short => match current.has_sell_imbalance() {
            Condition::Unknown => Condition::Unknown,
            imbalance => Condition::from(
                current.candle_delta < 0
                    && imbalance == Condition::True
                    && current.low >= previous.low
                    && current.close >= previous.close,
            ),
        },
    }
}

/// §81 deterministic +1R test using the original entry-to-initial-stop risk distance. This is the
/// same R arithmetic used by the structural opportunity calculation; invalid geometry is UNKNOWN.
#[must_use]
pub fn one_r_reached(
    direction: Direction,
    entry: f64,
    initial_stop: f64,
    favorable_extreme_since_entry: f64,
) -> Condition {
    if !entry.is_finite()
        || !initial_stop.is_finite()
        || !favorable_extreme_since_entry.is_finite()
    {
        return Condition::Unknown;
    }

    match direction {
        Direction::Long if initial_stop < entry => Condition::from(
            favorable_extreme_since_entry >= entry + (entry - initial_stop),
        ),
        Direction::Short if initial_stop > entry => Condition::from(
            favorable_extreme_since_entry <= entry - (initial_stop - entry),
        ),
        Direction::Long | Direction::Short => Condition::Unknown,
    }
}

/// §§76/81 and §94 mirror. The caller supplies an explicitly validated higher-low/lower-high fact
/// because canonical knowledge does not define a post-entry structural-pivot extraction algorithm.
/// The buffer is external configuration and has no default.
#[must_use]
pub fn structural_trail(
    direction: Direction,
    reached_one_r: Condition,
    favorable_effort_was_successful: Condition,
    structural_pivot_valid: Condition,
    structural_pivot: f64,
    buffer: f64,
    current_stop: f64,
) -> StructuralTrailEvaluation {
    if !structural_pivot.is_finite()
        || !buffer.is_finite()
        || buffer < 0.0
        || !current_stop.is_finite()
    {
        return StructuralTrailEvaluation {
            candidate_stop: None,
            apply: Condition::Unknown,
        };
    }

    let candidate_stop = match direction {
        Direction::Long => structural_pivot - buffer,
        Direction::Short => structural_pivot + buffer,
    };
    if !candidate_stop.is_finite() {
        return StructuralTrailEvaluation {
            candidate_stop: None,
            apply: Condition::Unknown,
        };
    }

    let prerequisites = all_true(&[
        reached_one_r,
        favorable_effort_was_successful,
        structural_pivot_valid,
    ]);
    if prerequisites != Condition::True {
        return StructuralTrailEvaluation {
            candidate_stop: Some(candidate_stop),
            apply: prerequisites,
        };
    }

    let no_widen = stop_replacement_allowed(direction, current_stop, candidate_stop);
    if no_widen != Condition::True {
        return StructuralTrailEvaluation {
            candidate_stop: Some(candidate_stop),
            apply: no_widen,
        };
    }

    let strictly_tighter = match direction {
        Direction::Long => candidate_stop > current_stop,
        Direction::Short => candidate_stop < current_stop,
    };
    StructuralTrailEvaluation {
        candidate_stop: Some(candidate_stop),
        apply: Condition::from(strictly_tighter),
    }
}

/// §76 defensive candidates must use predefined structure/breakeven permission supplied by the
/// caller and may never widen risk. This validator does not choose which defensive structure to use.
#[must_use]
pub fn defensive_stop_candidate_allowed(
    direction: Direction,
    current_stop: f64,
    candidate_stop: f64,
    structure_permits: Condition,
) -> Condition {
    if structure_permits != Condition::True {
        return structure_permits;
    }
    match stop_replacement_allowed(direction, current_stop, candidate_stop) {
        Condition::Unknown => Condition::Unknown,
        Condition::False => Condition::False,
        Condition::True => Condition::from(match direction {
            Direction::Long => candidate_stop > current_stop,
            Direction::Short => candidate_stop < current_stop,
        }),
    }
}

/// §§82-83 and §94 mirror. POC/gamma references do not auto-exit or reverse. The source leaves
/// HOLD versus TRAIL as a management choice, so successful effort returns the combined directive.
#[must_use]
pub const fn reference_management_directive(
    favorable_effort_successful: Condition,
    favorable_effort_failure: Condition,
) -> ManagementDirective {
    match (favorable_effort_successful, favorable_effort_failure) {
        (Condition::True, Condition::False) => ManagementDirective::MaintainOrTrail,
        (Condition::False, Condition::True) => ManagementDirective::Tighten,
        (Condition::False, Condition::False) => ManagementDirective::NoDirective,
        (Condition::Unknown, _) | (_, Condition::Unknown) | (Condition::True, Condition::True) => {
            ManagementDirective::Unknown
        }
    }
}

/// §84 strict final long swing-high target and §94 mirrored short swing-low target. A touch or
/// overshoot exits the remaining position; no runner branch exists in v1.
#[must_use]
pub fn final_target_directive(
    direction: Direction,
    candle: FootprintCandle5m<'_>,
    preidentified_final_target: f64,
) -> ManagementDirective {
    if !candle.valid() || !preidentified_final_target.is_finite() {
        return ManagementDirective::Unknown;
    }

    let touched = match direction {
        Direction::Long => candle.high >= preidentified_final_target,
        Direction::Short => candle.low <= preidentified_final_target,
    };
    if touched {
        ManagementDirective::ExitRemaining
    } else {
        ManagementDirective::NoDirective
    }
}

const fn any_true(first: Condition, second: Condition) -> Condition {
    match (first, second) {
        (Condition::True, _) | (_, Condition::True) => Condition::True,
        (Condition::False, Condition::False) => Condition::False,
        (Condition::Unknown, _) | (_, Condition::Unknown) => Condition::Unknown,
    }
}

const fn all_true(conditions: &[Condition]) -> Condition {
    let mut index = 0;
    let mut unknown = false;
    while index < conditions.len() {
        match conditions[index] {
            Condition::False => return Condition::False,
            Condition::Unknown => unknown = true,
            Condition::True => {}
        }
        index += 1;
    }
    if unknown {
        Condition::Unknown
    } else {
        Condition::True
    }
}
