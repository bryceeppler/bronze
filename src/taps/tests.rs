use std::time::Duration;

use super::{Detector, Gesture, Input, Side};

const LEFT_DOWN: Input = Input::Shift { side: Side::Left, pressed: true };
const LEFT_UP: Input = Input::Shift { side: Side::Left, pressed: false };
const RIGHT_DOWN: Input = Input::Shift { side: Side::Right, pressed: true };
const RIGHT_UP: Input = Input::Shift { side: Side::Right, pressed: false };
const OTHER: Input = Input::Other;

fn run(timeline: &[(u64, Input)]) -> Vec<(u64, Gesture)> {
    let mut detector = Detector::default();
    timeline
        .iter()
        .filter_map(|&(ms, input)| detector.feed(input, Duration::from_millis(ms)).map(|gesture| (ms, gesture)))
        .collect()
}

#[test]
fn a_clean_double_tap_fires_on_the_second_release() {
    let fired = run(&[(1000, LEFT_DOWN), (1080, LEFT_UP), (1200, LEFT_DOWN), (1270, LEFT_UP)]);
    assert_eq!(fired, [(1270, Gesture::DoubleLeftShift)]);
}

#[test]
fn typing_two_capitals_quickly_fires_nothing() {
    let fired = run(&[(0, LEFT_DOWN), (40, OTHER), (90, LEFT_UP), (150, LEFT_DOWN), (190, OTHER), (230, LEFT_UP)]);
    assert_eq!(fired, []);
}

#[test]
fn a_held_shift_fires_nothing() {
    assert_eq!(run(&[(0, LEFT_DOWN), (100, LEFT_UP), (200, LEFT_DOWN), (900, LEFT_UP)]), []);
    assert_eq!(run(&[(0, LEFT_DOWN), (600, LEFT_UP), (700, LEFT_DOWN), (760, LEFT_UP)]), []);
}

#[test]
fn shift_clicking_twice_fires_nothing() {
    let fired = run(&[(0, LEFT_DOWN), (50, OTHER), (90, LEFT_UP), (160, LEFT_DOWN), (200, OTHER), (240, LEFT_UP)]);
    assert_eq!(fired, []);
}

#[test]
fn taps_too_far_apart_fire_nothing() {
    assert_eq!(run(&[(0, LEFT_DOWN), (80, LEFT_UP), (500, LEFT_DOWN), (560, LEFT_UP)]), []);
}

#[test]
fn a_third_quick_tap_does_not_fire_again_off_the_same_pair() {
    let fired =
        run(&[(0, LEFT_DOWN), (60, LEFT_UP), (150, LEFT_DOWN), (210, LEFT_UP), (300, LEFT_DOWN), (360, LEFT_UP)]);
    assert_eq!(fired, [(210, Gesture::DoubleLeftShift)]);
}

#[test]
fn both_shifts_fire_once_in_either_order_and_never_count_as_a_tap() {
    for (first, second) in [(LEFT_DOWN, RIGHT_DOWN), (RIGHT_DOWN, LEFT_DOWN)] {
        let fired = run(&[
            (0, first),
            (60, second),
            (400, OTHER),
            (500, LEFT_UP),
            (520, RIGHT_UP),
            (600, LEFT_DOWN),
            (660, LEFT_UP),
        ]);
        assert_eq!(fired, [(60, Gesture::BothShifts)]);
    }
}

#[test]
fn a_right_shift_event_cancels_a_tap_in_progress() {
    let fired =
        run(&[(0, LEFT_DOWN), (60, LEFT_UP), (100, RIGHT_DOWN), (130, RIGHT_UP), (160, LEFT_DOWN), (220, LEFT_UP)]);
    assert_eq!(fired, []);
}

#[test]
fn a_double_tap_of_right_shift_fires_nothing() {
    assert_eq!(run(&[(0, RIGHT_DOWN), (60, RIGHT_UP), (150, RIGHT_DOWN), (210, RIGHT_UP)]), []);
}
