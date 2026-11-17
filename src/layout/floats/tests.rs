//! Expectations from CSS 2.1 §9.5.1 and Chrome's placement of the same boxes
//! (a 296px-wide formatting context, 24px line boxes).

use super::*;

const CONTAINER: InlineSpan = InlineSpan::new(0.0, 296.0);

fn request(side: FloatSide, width: f32, height: f32, min_top: f32) -> FloatRequest {
    FloatRequest {
        side,
        width,
        height,
        min_top,
        container: CONTAINER,
    }
}

#[test]
fn a_left_float_sits_beside_an_earlier_left_float_that_leaves_room() {
    let mut floats = FloatExclusions::default();
    floats.place(request(FloatSide::Left, 68.0, 50.0, 0.0));
    let second = floats.place(request(FloatSide::Left, 40.0, 30.0, 0.0));

    assert_eq!(second.inline, InlineSpan::new(68.0, 108.0));
    assert_eq!(second.top, 0.0);
}

#[test]
fn a_float_too_wide_for_the_room_beside_earlier_floats_moves_below_them() {
    let mut floats = FloatExclusions::default();
    floats.place(request(FloatSide::Left, 120.0, 20.0, 0.0));
    let second = floats.place(request(FloatSide::Left, 200.0, 20.0, 0.0));

    assert_eq!(second.inline, InlineSpan::new(0.0, 200.0));
    assert_eq!(second.top, 20.0);
}

#[test]
fn a_line_box_avoids_every_float_it_overlaps() {
    let mut floats = FloatExclusions::default();
    floats.place(request(FloatSide::Left, 120.0, 20.0, 0.0));
    floats.place(request(FloatSide::Left, 200.0, 20.0, 0.0));

    // The first line (0 to 24) meets both floats; the second only the lower.
    assert_eq!(
        floats.line_space(0.0, 24.0, CONTAINER),
        InlineSpan::new(200.0, 296.0)
    );
    assert_eq!(
        floats.line_space(24.0, 24.0, CONTAINER),
        InlineSpan::new(200.0, 296.0)
    );
    assert_eq!(floats.line_space(40.0, 24.0, CONTAINER), CONTAINER);
}

#[test]
fn left_and_right_floats_shorten_lines_from_both_sides() {
    let mut floats = FloatExclusions::default();
    floats.place(request(FloatSide::Left, 50.0, 50.0, 0.0));
    let right = floats.place(request(FloatSide::Right, 50.0, 50.0, 0.0));

    assert_eq!(right.inline, InlineSpan::new(246.0, 296.0));
    assert_eq!(
        floats.line_space(24.0, 24.0, CONTAINER),
        InlineSpan::new(50.0, 246.0)
    );
    assert_eq!(
        floats.line_space(48.0, 24.0, CONTAINER),
        InlineSpan::new(50.0, 246.0)
    );
    assert_eq!(floats.line_space(50.0, 24.0, CONTAINER), CONTAINER);
}

#[test]
fn right_floats_stack_leftwards_from_the_right_edge() {
    let mut floats = FloatExclusions::default();
    floats.place(request(FloatSide::Right, 80.0, 40.0, 0.0));
    let second = floats.place(request(FloatSide::Right, 60.0, 40.0, 0.0));

    assert_eq!(second.inline, InlineSpan::new(156.0, 216.0));
}

#[test]
fn a_float_is_never_higher_than_an_earlier_float() {
    let mut floats = FloatExclusions::default();
    floats.place(request(FloatSide::Left, 50.0, 50.0, 40.0));
    let later = floats.place(request(FloatSide::Right, 50.0, 50.0, 0.0));

    assert_eq!(later.top, 40.0);
}

#[test]
fn a_float_is_never_higher_than_its_place_in_the_flow() {
    let mut floats = FloatExclusions::default();
    floats.place(request(FloatSide::Left, 50.0, 10.0, 0.0));
    let later = floats.place(request(FloatSide::Left, 50.0, 10.0, 72.0));

    assert_eq!(later.top, 72.0);
    assert_eq!(later.inline, InlineSpan::new(0.0, 50.0));
}

#[test]
fn a_float_wider_than_its_container_goes_below_every_float_and_overflows() {
    let mut floats = FloatExclusions::default();
    floats.place(request(FloatSide::Left, 100.0, 30.0, 0.0));
    floats.place(request(FloatSide::Right, 100.0, 60.0, 0.0));
    let wide = floats.place(request(FloatSide::Left, 400.0, 10.0, 0.0));

    assert_eq!(wide.top, 60.0);
    assert_eq!(wide.inline, InlineSpan::new(0.0, 400.0));
}

#[test]
fn a_float_keeps_inside_its_own_containing_block() {
    let mut floats = FloatExclusions::default();
    let container = InlineSpan::new(10.0, 286.0);
    let right = floats.place(FloatRequest {
        container,
        ..request(FloatSide::Right, 60.0, 40.0, 10.0)
    });

    assert_eq!(right.inline, InlineSpan::new(226.0, 286.0));
    assert_eq!(
        floats.line_space(10.0, 24.0, container),
        InlineSpan::new(10.0, 226.0)
    );
}

#[test]
fn a_line_that_does_not_fit_tries_again_below_the_nearest_float_bottom() {
    let mut floats = FloatExclusions::default();
    floats.place(request(FloatSide::Left, 100.0, 30.0, 0.0));
    floats.place(request(FloatSide::Right, 100.0, 60.0, 0.0));

    assert_eq!(floats.next_line_top(0.0, 24.0), Some(30.0));
    assert_eq!(floats.next_line_top(30.0, 24.0), Some(60.0));
    assert_eq!(floats.next_line_top(60.0, 24.0), None);
}
