use crate::{Condition, EtTime};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NewsBlackoutWindow {
    pub start_inclusive: EtTime,
    pub end_exclusive: EtTime,
}

impl NewsBlackoutWindow {
    #[must_use]
    pub const fn valid(self) -> bool {
        self.start_inclusive < self.end_exclusive
    }

    #[must_use]
    pub const fn contains(self, time: EtTime) -> bool {
        self.start_inclusive <= time && time < self.end_exclusive
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NewsGateEvaluation {
    pub news_blackout_clear: Condition,
    pub allow_new_entries: bool,
    pub allow_position_management: bool,
    pub latest_blackout_end: Option<EtTime>,
}

/// Canonical §§95-96 scheduled-news gate.
///
/// The blackout intervals are externally validated/configured. No ordinary-event T±5-minute
/// default is embedded here because §95 classifies that duration as an example implementation.
/// `setup_sequence_started_at` is required only after at least one blackout has ended; §96 then
/// requires a new setup sequence rather than revival of a pre-event or in-blackout setup.
#[must_use]
pub fn evaluate_news_gate(
    now_et: EtTime,
    setup_sequence_started_at: Option<EtTime>,
    schedule_valid: Condition,
    windows: &[NewsBlackoutWindow],
) -> NewsGateEvaluation {
    if schedule_valid != Condition::True || !windows_valid(windows) {
        return NewsGateEvaluation {
            news_blackout_clear: Condition::Unknown,
            allow_new_entries: false,
            allow_position_management: true,
            latest_blackout_end: None,
        };
    }

    let active = windows.iter().copied().any(|window| window.contains(now_et));
    let latest_blackout_end = windows
        .iter()
        .copied()
        .filter(|window| window.end_exclusive <= now_et)
        .map(|window| window.end_exclusive)
        .max();

    if active {
        return NewsGateEvaluation {
            news_blackout_clear: Condition::False,
            allow_new_entries: false,
            allow_position_management: true,
            latest_blackout_end,
        };
    }

    let news_blackout_clear = match latest_blackout_end {
        None => Condition::True,
        Some(end) => match setup_sequence_started_at {
            Some(start) => Condition::from(start >= end),
            None => Condition::Unknown,
        },
    };

    NewsGateEvaluation {
        news_blackout_clear,
        allow_new_entries: news_blackout_clear == Condition::True,
        allow_position_management: true,
        latest_blackout_end,
    }
}

fn windows_valid(windows: &[NewsBlackoutWindow]) -> bool {
    if windows.iter().copied().any(|window| !window.valid()) {
        return false;
    }

    windows.windows(2).all(|pair| {
        let previous = pair[0];
        let current = pair[1];
        let ordered = (previous.start_inclusive, previous.end_exclusive)
            <= (current.start_inclusive, current.end_exclusive);
        let duplicate = previous == current;
        ordered && !duplicate
    })
}
