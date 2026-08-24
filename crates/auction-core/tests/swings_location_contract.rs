use auction_core::{
    Bar15, ConfirmedSwing, Direction, EtTime, LocationEvent, LocationSetup, MarketState, SetupState,
    StructureKey, SwingImpulse, SwingKind, TerminalState, bearish_fib, confirmed_swings,
    latest_bearish_impulse, latest_bullish_impulse, long_886_invalidated, qualified_impulse,
    short_886_invalidated,
};

fn closed(high: f64, low: f64) -> Bar15 {
    Bar15 {
        high,
        low,
        closed: true,
    }
}

fn long_impulse() -> SwingImpulse {
    SwingImpulse {
        direction: Direction::Long,
        start_index: 2,
        end_index: 7,
        swing_low: 80.0,
        swing_high: 100.0,
    }
}

fn short_impulse() -> SwingImpulse {
    SwingImpulse {
        direction: Direction::Short,
        start_index: 3,
        end_index: 9,
        swing_low: 100.0,
        swing_high: 120.0,
    }
}

#[test]
fn section_26_confirms_pivot_only_with_two_closed_bars_each_side() {
    let high_bars = [
        closed(10.0, 5.0),
        closed(11.0, 4.0),
        closed(15.0, 6.0),
        closed(12.0, 4.5),
        closed(10.0, 5.5),
    ];
    let high_swings = confirmed_swings(&high_bars);
    assert_eq!(high_swings.len(), 1);
    assert_eq!(high_swings[0].kind, SwingKind::High);
    assert_eq!(high_swings[0].index, 2);
    assert_eq!(high_swings[0].price, 15.0);

    let low_bars = [
        closed(10.0, 5.0),
        closed(11.0, 4.0),
        closed(9.0, 2.0),
        closed(12.0, 4.5),
        closed(10.0, 5.5),
    ];
    let low_swings = confirmed_swings(&low_bars);
    assert_eq!(low_swings.len(), 1);
    assert_eq!(low_swings[0].kind, SwingKind::Low);
    assert_eq!(low_swings[0].index, 2);
    assert_eq!(low_swings[0].price, 2.0);

    let mut open_right = high_bars;
    open_right[4].closed = false;
    assert!(confirmed_swings(&open_right).is_empty());
}

#[test]
fn section_26_uses_strict_pivot_comparisons() {
    let tied_high = [
        closed(10.0, 5.0),
        closed(15.0, 4.0),
        closed(15.0, 6.0),
        closed(12.0, 4.5),
        closed(10.0, 5.5),
    ];
    assert!(confirmed_swings(&tied_high).is_empty());
}

#[test]
fn sections_27_28_require_latest_start_swing_followed_chronologically() {
    let mut bullish = vec![
        ConfirmedSwing {
            index: 2,
            price: 80.0,
            kind: SwingKind::Low,
        },
        ConfirmedSwing {
            index: 5,
            price: 100.0,
            kind: SwingKind::High,
        },
        ConfirmedSwing {
            index: 8,
            price: 90.0,
            kind: SwingKind::Low,
        },
    ];
    assert_eq!(latest_bullish_impulse(&bullish), None);

    bullish.push(ConfirmedSwing {
        index: 11,
        price: 110.0,
        kind: SwingKind::High,
    });
    let impulse = latest_bullish_impulse(&bullish).unwrap();
    assert_eq!(impulse.start_index, 8);
    assert_eq!(impulse.end_index, 11);
    assert_eq!(impulse.swing_low, 90.0);
    assert_eq!(impulse.swing_high, 110.0);

    let bearish = [
        ConfirmedSwing {
            index: 3,
            price: 120.0,
            kind: SwingKind::High,
        },
        ConfirmedSwing {
            index: 6,
            price: 105.0,
            kind: SwingKind::Low,
        },
    ];
    let short = latest_bearish_impulse(&bearish).unwrap();
    assert_eq!(short.start_index, 3);
    assert_eq!(short.end_index, 6);
    assert_eq!(short.direction, Direction::Short);
}

#[test]
fn sections_24_27_28_reject_wrong_or_non_directional_environment() {
    let swings = [
        ConfirmedSwing {
            index: 2,
            price: 80.0,
            kind: SwingKind::Low,
        },
        ConfirmedSwing {
            index: 5,
            price: 100.0,
            kind: SwingKind::High,
        },
    ];

    assert!(qualified_impulse(MarketState::ValueUp, &swings).is_some());
    assert!(qualified_impulse(MarketState::ValueDown, &swings).is_none());
    assert!(qualified_impulse(MarketState::Balanced, &swings).is_none());
    assert!(qualified_impulse(MarketState::Unclear, &swings).is_none());
}

#[test]
fn sections_31_34_35_open_only_into_waiting_then_location_reached() {
    let open = EtTime::from_hms(9, 30, 0).unwrap();
    let mut setup = LocationSetup::at_open(open, MarketState::ValueUp, long_impulse(), 90.0, 0.25)
        .unwrap();

    assert_eq!(setup.state(), SetupState::WaitingForLocation);
    assert_eq!(setup.observe_price(90.0), LocationEvent::None);

    let touch = setup.levels().level_705;
    assert_eq!(setup.observe_price(touch), LocationEvent::Reached);
    assert_eq!(setup.state(), SetupState::LocationReached);

    let after_open = EtTime::from_hms(9, 30, 1).unwrap();
    assert!(
        LocationSetup::at_open(
            after_open,
            MarketState::ValueUp,
            long_impulse(),
            90.0,
            0.25
        )
        .is_none()
    );
    assert!(
        LocationSetup::at_open(open, MarketState::ValueDown, long_impulse(), 90.0, 0.25)
            .is_none()
    );
}

#[test]
fn section_36_long_invalidation_is_strictly_beyond_one_tick_before_confirmation() {
    let levels = auction_core::bullish_fib(80.0, 100.0).unwrap();
    let tick = 0.25;
    let boundary = levels.level_886 - tick;

    assert!(!long_886_invalidated(
        levels,
        tick,
        boundary,
        SetupState::LocationReached
    ));
    assert!(long_886_invalidated(
        levels,
        tick,
        boundary - 0.01,
        SetupState::LocationReached
    ));
    assert!(!long_886_invalidated(
        levels,
        tick,
        boundary - 10.0,
        SetupState::FinalReconfirmation
    ));
}

#[test]
fn sections_36_37_invalidation_is_terminal_and_same_structure_cannot_be_rescued() {
    let open = EtTime::from_hms(9, 30, 0).unwrap();
    let mut setup = LocationSetup::at_open(open, MarketState::ValueUp, long_impulse(), 90.0, 0.25)
        .unwrap();
    let threshold = setup.levels().level_886 - 0.25;

    assert_eq!(
        setup.observe_price(threshold - 0.01),
        LocationEvent::Invalidated
    );
    assert_eq!(
        setup.state(),
        SetupState::Terminal(TerminalState::Invalidated)
    );
    assert_eq!(setup.observe_price(setup.levels().level_705), LocationEvent::None);

    let same = setup.structure();
    assert!(!setup.requires_new_setup_id_for(same));
    assert!(setup.requires_new_setup_id_for(StructureKey {
        end_index: same.end_index + 5,
        ..same
    }));
}

#[test]
fn sections_85_and_36_mirror_premium_side_invalidation_explicitly() {
    let levels = bearish_fib(120.0, 100.0).unwrap();
    let tick = 0.25;
    let boundary = levels.level_886 + tick;

    assert!(!short_886_invalidated(
        levels,
        tick,
        boundary,
        SetupState::SecondFailure
    ));
    assert!(short_886_invalidated(
        levels,
        tick,
        boundary + 0.01,
        SetupState::SecondFailure
    ));
    assert!(!short_886_invalidated(
        levels,
        tick,
        boundary + 10.0,
        SetupState::FinalReconfirmation
    ));

    let open = EtTime::from_hms(9, 30, 0).unwrap();
    let setup = LocationSetup::at_open(open, MarketState::ValueDown, short_impulse(), 110.0, tick)
        .unwrap();
    assert_eq!(setup.state(), SetupState::WaitingForLocation);
}
