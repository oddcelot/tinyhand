//! Ink strokes: smoothing, arrowheads, fading.

use std::f32::consts::PI;
use tinyhand_core::ink::*;

fn close(a: f32, b: f32) -> bool {
    (a - b).abs() < 1e-3
}

#[test]
fn colours_and_width() {
    assert_eq!(PURPLE, 0x8754F2);
    assert_eq!(RED, 0xFF291F);
    let (r, g, b) = rgb(0xFF8000);
    assert!(close(r, 1.0) && close(g, 128.0 / 255.0) && close(b, 0.0));
    assert!(close(ink_width(1000.0), 5.5));
    assert!(close(ink_width(400.0), 3.0));
}

#[test]
fn points_closer_than_1_5_are_dropped() {
    let mut s = Stroke::new((0.0, 0.0), PURPLE, 4.0);
    s.add_point((1.0, 1.0)); // 1.41 away
    assert_eq!(s.points.len(), 1);
    s.add_point((1.5, 0.0));
    assert_eq!(s.points.len(), 2);
    s.add_point((2.0, 0.0));
    assert_eq!(s.points.len(), 2);
}

#[test]
fn path_one_two_three_points() {
    let mut s = Stroke::new((0.0, 0.0), PURPLE, 4.0);
    assert_eq!(s.path(), vec![Segment::MoveTo((0.0, 0.0)), Segment::LineTo((0.0, 0.0))]);
    s.add_point((10.0, 0.0));
    assert_eq!(
        s.path(),
        vec![
            Segment::MoveTo((0.0, 0.0)),
            Segment::QuadTo { ctrl: (0.0, 0.0), to: (5.0, 0.0) },
            Segment::LineTo((10.0, 0.0)),
        ]
    );
    s.add_point((10.0, 10.0));
    assert_eq!(
        s.path(),
        vec![
            Segment::MoveTo((0.0, 0.0)),
            Segment::QuadTo { ctrl: (0.0, 0.0), to: (5.0, 0.0) },
            Segment::QuadTo { ctrl: (10.0, 0.0), to: (10.0, 5.0) },
            Segment::LineTo((10.0, 10.0)),
        ]
    );
}

#[test]
fn arrowhead_on_a_long_line() {
    let mut s = Stroke::new((0.0, 0.0), PURPLE, 3.0);
    s.add_point((100.0, 0.0));
    assert!(close(s.arrow_size(), 14.0));
    let [a, b] = s.arrowhead().unwrap();
    assert_eq!(a.0, (100.0, 0.0));
    assert_eq!(b.0, (100.0, 0.0));
    let (c, sn) = ((PI / 7.0).cos() * 14.0, (PI / 7.0).sin() * 14.0);
    assert!(close(a.1.0, 100.0 - c) && close(a.1.1, sn), "{:?}", a.1);
    assert!(close(b.1.0, 100.0 - c) && close(b.1.1, -sn), "{:?}", b.1);
}

#[test]
fn arrowhead_size_grows_with_width_and_shrinks_for_short_strokes() {
    let s = Stroke::new((0.0, 0.0), PURPLE, 10.0);
    assert!(close(s.arrow_size(), 32.0));
    assert!(s.arrowhead().is_none(), "a single point has no direction");

    let mut s = Stroke::new((0.0, 0.0), PURPLE, 3.0);
    s.add_point((5.0, 0.0)); // nothing is 16.8 back, so aim from the first point
    let [a, _] = s.arrowhead().unwrap();
    let head = (a.1.0 - 5.0).hypot(a.1.1);
    assert!(close(head, 4.0), "head {head}"); // min(14, 5*0.8)

    let mut s = Stroke::new((0.0, 0.0), PURPLE, 3.0);
    s.add_point((1.5, 0.0));
    assert!(s.arrowhead().is_some(), "1.5 long is above the 1 pt minimum");
}

#[test]
fn ease_in_curve() {
    assert!(close(ease_in(0.0), 0.0) && close(ease_in(1.0), 1.0));
    let mut prev = 0.0;
    for i in 1..=20 {
        let v = ease_in(i as f32 / 20.0);
        assert!(v >= prev);
        prev = v;
    }
    // Ease-in is slow at the start.
    assert!(ease_in(0.5) < 0.4);
    assert!(ease_in(0.25) < 0.1);
}

#[test]
fn drawing_flow_and_arrow() {
    let mut ink = Ink::new(2.0);
    ink.begin((0.0, 0.0), RED, 4.0);
    assert!(ink.is_drawing());
    ink.extend((50.0, 0.0));
    ink.set_arrow(true);
    ink.end(1.0);
    assert!(!ink.is_drawing());
    ink.extend((99.0, 0.0)); // ignored: pen is up
    assert_eq!(ink.strokes[0].points.len(), 2);
    assert!(ink.strokes[0].arrow, "the finished stroke keeps its arrow");
    ink.begin((0.0, 0.0), PURPLE, 4.0);
    assert!(!ink.strokes[1].arrow);
    ink.clear();
    assert!(ink.strokes.is_empty());
}

#[test]
fn fade_numbers() {
    let mut ink = Ink::new(2.0);
    ink.begin((0.0, 0.0), PURPLE, 4.0);
    assert_eq!(ink.opacity(&ink.strokes[0], 100.0), 1.0, "still drawing");
    ink.end(10.0);
    let s = ink.strokes[0].clone();
    assert_eq!(ink.opacity(&s, 11.9), 1.0);
    assert_eq!(ink.opacity(&s, 12.0), 1.0);
    assert!(close(ink.opacity(&s, 12.3), 1.0 - ease_in(0.5)));
    assert!(close(ink.opacity(&s, 12.6), 0.0));
    assert_eq!(ink.opacity(&s, 50.0), 0.0);

    ink.prune(12.5);
    assert_eq!(ink.strokes.len(), 1);
    ink.prune(12.6);
    assert!(ink.strokes.is_empty());
}

#[test]
fn short_fade_waits_half_a_second_and_zero_keeps_forever() {
    let mut ink = Ink::new(0.1);
    ink.begin((0.0, 0.0), PURPLE, 4.0);
    ink.end(0.0);
    let s = ink.strokes[0].clone();
    assert_eq!(ink.opacity(&s, 0.5), 1.0);
    assert!(ink.opacity(&s, 0.8) < 1.0);

    let mut keep = Ink::new(0.0);
    keep.begin((0.0, 0.0), PURPLE, 4.0);
    keep.end(0.0);
    let s = keep.strokes[0].clone();
    assert_eq!(keep.opacity(&s, 1e6), 1.0);
    keep.prune(1e6);
    assert_eq!(keep.strokes.len(), 1);
}
