use auction_core::{Condition, EtTime, NewsBlackoutWindow, evaluate_news_gate};

fn t(hour: u8, minute: u8, second: u8) -> EtTime {
    EtTime::from_hms(hour, minute, second).unwrap()
}

fn window(
    start_hour: u8,
    start_minute: u8,
    end_hour: u8,
    end_minute: u8,
) -> NewsBlackoutWindow {
    NewsBlackoutWindow {
        start_inclusive: t(start_hour, start_minute, 0),
        end_exclusive: t(end_hour, end_minute, 0),
    }
}

#[test]
fn section_95_has_no_hard_coded_five_minute_window() {
    let short = [window(9, 58, 10, 2)];
    let long = [window(9, 43, 10, 17)];

    assert_eq!(
        evaluate_news_gate(t(10, 1, 59), None, Condition::True, &short).news_blackout_clear,
        Condition::False
    );
    assert_eq!(
        evaluate_news_gate(t(10, 10, 0), None, Condition::True, &long).news_blackout_clear,
        Condition::False
    );
}

#[test]
fn section_96_blackout_start_is_inclusive_and_end_is_resumable_with_new_sequence() {
    let windows = [window(9, 55, 10, 5)];

    let before = evaluate_news_gate(t(9, 54, 59), None, Condition::True, &windows);
    assert_eq!(before.news_blackout_clear, Condition::True);
    assert!(before.allow_new_entries);

    let at_start = evaluate_news_gate(t(9, 55, 0), None, Condition::True, &windows);
    assert_eq!(at_start.news_blackout_clear, Condition::False);
    assert!(!at_start.allow_new_entries);

    let inside = evaluate_news_gate(t(10, 4, 59), None, Condition::True, &windows);
    assert_eq!(inside.news_blackout_clear, Condition::False);
    assert!(!inside.allow_new_entries);

    let at_end_with_new_setup = evaluate_news_gate(
        t(10, 5, 0),
        Some(t(10, 5, 0)),
        Condition::True,
        &windows,
    );
    assert_eq!(at_end_with_new_setup.news_blackout_clear, Condition::True);
    assert!(at_end_with_new_setup.allow_new_entries);
}

#[test]
fn section_96_pre_event_or_in_blackout_setup_cannot_be_revived_after_event() {
    let windows = [window(9, 55, 10, 5)];

    for setup_start in [t(9, 40, 0), t(9, 55, 0), t(10, 4, 59)] {
        let result = evaluate_news_gate(
            t(10, 5, 0),
            Some(setup_start),
            Condition::True,
            &windows,
        );
        assert_eq!(result.news_blackout_clear, Condition::False);
        assert!(!result.allow_new_entries);
    }

    let after = evaluate_news_gate(
        t(10, 5, 1),
        Some(t(10, 5, 1)),
        Condition::True,
        &windows,
    );
    assert_eq!(after.news_blackout_clear, Condition::True);
    assert!(after.allow_new_entries);
}

#[test]
fn section_96_missing_setup_timestamp_after_blackout_is_unknown_and_fails_closed() {
    let windows = [window(9, 55, 10, 5)];
    let result = evaluate_news_gate(t(10, 6, 0), None, Condition::True, &windows);

    assert_eq!(result.news_blackout_clear, Condition::Unknown);
    assert!(!result.allow_new_entries);
    assert_eq!(result.latest_blackout_end, Some(t(10, 5, 0)));
}

#[test]
fn section_96_existing_position_management_remains_allowed_for_all_news_gate_states() {
    let windows = [window(9, 55, 10, 5)];

    let clear = evaluate_news_gate(t(9, 54, 0), None, Condition::True, &windows);
    let blackout = evaluate_news_gate(t(10, 0, 0), None, Condition::True, &windows);
    let unknown = evaluate_news_gate(t(10, 0, 0), None, Condition::Unknown, &windows);

    assert!(clear.allow_position_management);
    assert!(blackout.allow_position_management);
    assert!(unknown.allow_position_management);
}

#[test]
fn sections_10_108_invalid_or_unknown_schedule_state_fails_closed() {
    let windows = [window(9, 55, 10, 5)];

    for state in [Condition::False, Condition::Unknown] {
        let result = evaluate_news_gate(t(9, 40, 0), None, state, &windows);
        assert_eq!(result.news_blackout_clear, Condition::Unknown);
        assert!(!result.allow_new_entries);
        assert!(result.allow_position_management);
    }
}

#[test]
fn invalid_reversed_or_zero_length_windows_fail_closed() {
    let reversed = [NewsBlackoutWindow {
        start_inclusive: t(10, 5, 0),
        end_exclusive: t(9, 55, 0),
    }];
    let zero = [NewsBlackoutWindow {
        start_inclusive: t(10, 0, 0),
        end_exclusive: t(10, 0, 0),
    }];

    for windows in [&reversed[..], &zero[..]] {
        let result = evaluate_news_gate(t(10, 0, 0), None, Condition::True, windows);
        assert_eq!(result.news_blackout_clear, Condition::Unknown);
        assert!(!result.allow_new_entries);
    }
}

#[test]
fn duplicate_or_unsorted_windows_fail_closed_without_silent_normalization() {
    let duplicate = [window(9, 55, 10, 5), window(9, 55, 10, 5)];
    let unsorted = [window(10, 10, 10, 15), window(9, 55, 10, 5)];

    for windows in [&duplicate[..], &unsorted[..]] {
        let result = evaluate_news_gate(t(9, 40, 0), None, Condition::True, windows);
        assert_eq!(result.news_blackout_clear, Condition::Unknown);
        assert!(!result.allow_new_entries);
    }
}

#[test]
fn overlapping_windows_are_supported_and_latest_end_controls_setup_freshness() {
    let windows = [window(9, 55, 10, 5), window(10, 0, 10, 10)];

    let overlap = evaluate_news_gate(t(10, 4, 0), None, Condition::True, &windows);
    assert_eq!(overlap.news_blackout_clear, Condition::False);

    let first_ended_second_active =
        evaluate_news_gate(t(10, 5, 0), None, Condition::True, &windows);
    assert_eq!(first_ended_second_active.news_blackout_clear, Condition::False);

    let stale_after_all = evaluate_news_gate(
        t(10, 10, 0),
        Some(t(10, 5, 0)),
        Condition::True,
        &windows,
    );
    assert_eq!(stale_after_all.latest_blackout_end, Some(t(10, 10, 0)));
    assert_eq!(stale_after_all.news_blackout_clear, Condition::False);

    let fresh_after_all = evaluate_news_gate(
        t(10, 10, 0),
        Some(t(10, 10, 0)),
        Condition::True,
        &windows,
    );
    assert_eq!(fresh_after_all.news_blackout_clear, Condition::True);
}

#[test]
fn same_start_different_end_windows_are_deterministic_when_sorted() {
    let windows = [window(9, 55, 10, 5), window(9, 55, 10, 10)];
    let result = evaluate_news_gate(t(10, 6, 0), None, Condition::True, &windows);
    assert_eq!(result.news_blackout_clear, Condition::False);
}

#[test]
fn empty_valid_schedule_means_no_scheduled_blackout() {
    let result = evaluate_news_gate(t(10, 0, 0), None, Condition::True, &[]);
    assert_eq!(result.news_blackout_clear, Condition::True);
    assert!(result.allow_new_entries);
    assert_eq!(result.latest_blackout_end, None);
}

#[test]
fn invalid_clock_values_cannot_create_news_timestamps() {
    assert!(EtTime::from_hms(24, 0, 0).is_none());
    assert!(EtTime::from_hms(10, 60, 0).is_none());
    assert!(EtTime::from_hms(10, 0, 60).is_none());
}
