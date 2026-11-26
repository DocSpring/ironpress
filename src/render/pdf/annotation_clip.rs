//! The part of the page that the `overflow` clips being painted inside leave visible, where link annotations go.
//!
//! Chromium cuts each link annotation to the clip: Skia intersects the link's transformed rectangle with the clip
//! stack's path and writes the bounding box of what's left, or nothing when nothing is (`SkPDFDevice::drawAnnotation`).
//! An overflow clip is a rectangle, or a rounded one, through the transforms around it, so the visible part is a convex
//! polygon on the page, and so is its intersection with the clips around it.

use super::geometry::{PdfPoint, PdfRect, RoundedRect};
use crate::types::CornerRadius;

/// Points along each rounded corner: the outline stays within 0.1% of a corner's radius inside the curve
const CORNER_SEGMENTS: usize = 16;

/// The outline of a clip, counterclockwise
pub(super) fn outline(clip: RoundedRect) -> Vec<PdfPoint> {
    let rect = clip.rect;
    let radii = clip.radii.fit_to(rect.width, rect.height);
    let (left, right, bottom, top) = (rect.left, rect.right(), rect.bottom, rect.top());
    let mut points = Vec::new();
    // Each corner's arc, from the start of its quarter turn (counterclockwise from the bottom left)
    let corners = [
        (
            radii.bottom_left,
            PdfPoint::new(left, bottom),
            (1.0, 1.0),
            180.0_f32,
        ),
        (
            radii.bottom_right,
            PdfPoint::new(right, bottom),
            (-1.0, 1.0),
            270.0,
        ),
        (
            radii.top_right,
            PdfPoint::new(right, top),
            (-1.0, -1.0),
            0.0,
        ),
        (radii.top_left, PdfPoint::new(left, top), (1.0, -1.0), 90.0),
    ];
    for (radius, corner, (towards_x, towards_y), start) in corners {
        arc(&mut points, radius, corner, (towards_x, towards_y), start);
    }
    // Where one corner's arc ends and the next begins, or a corner has no radius, the points meet: an edge between
    // two points rounding left apart would have no direction to clip by
    points.dedup_by(|point, previous| same_point(*point, *previous));
    while points.len() > 1 && same_point(points[0], points[points.len() - 1]) {
        points.pop();
    }
    points
}

/// Whether two outline points are the same but for rounding
fn same_point(a: PdfPoint, b: PdfPoint) -> bool {
    (a.x - b.x).abs() < 1e-3 && (a.y - b.y).abs() < 1e-3
}

fn arc(
    points: &mut Vec<PdfPoint>,
    radius: CornerRadius,
    corner: PdfPoint,
    (towards_x, towards_y): (f32, f32),
    start: f32,
) {
    if radius.is_zero() {
        points.push(corner);
        return;
    }
    let center = PdfPoint::new(
        corner.x + towards_x * radius.x,
        corner.y + towards_y * radius.y,
    );
    for step in 0..=CORNER_SEGMENTS {
        let angle = (start + 90.0 * step as f32 / CORNER_SEGMENTS as f32).to_radians();
        points.push(PdfPoint::new(
            center.x + radius.x * angle.cos(),
            center.y + radius.y * angle.sin(),
        ));
    }
}

/// The part of the polygon `subject` inside the convex polygon `clip` (Sutherland–Hodgman), empty when they don't
/// overlap
pub(super) fn intersect(subject: &[PdfPoint], clip: &[PdfPoint]) -> Vec<PdfPoint> {
    if clip.len() < 3 {
        return Vec::new();
    }
    let orientation = signed_area(clip).signum();
    if orientation == 0.0 {
        return Vec::new();
    }
    let mut output = subject.to_vec();
    for (index, &start) in clip.iter().enumerate() {
        let end = clip[(index + 1) % clip.len()];
        if same_point(start, end) {
            continue;
        }
        let inside = |point: PdfPoint| side(start, end, point) * orientation >= 0.0;
        let input = std::mem::take(&mut output);
        for (point_index, &current) in input.iter().enumerate() {
            let previous = input[(point_index + input.len() - 1) % input.len()];
            match (inside(previous), inside(current)) {
                (true, true) => output.push(current),
                (true, false) => output.push(crossing(previous, current, start, end)),
                (false, true) => {
                    output.push(crossing(previous, current, start, end));
                    output.push(current);
                }
                (false, false) => {}
            }
        }
        if output.is_empty() {
            break;
        }
    }
    output
}

/// Thinner than this (in points), what's left of a link after clipping is only an edge it shares with the clip: two
/// boxes that meet along a line, such as a line of text below a clipped one, through a rotation that leaves rounding
const SLIVER: f32 = 0.01;

/// The smallest rectangle around what's left of a link after clipping, or `None` when that covers no area
pub(super) fn bounds(points: &[PdfPoint]) -> Option<PdfRect> {
    if points.len() < 3 {
        return None;
    }
    let extent = extent(points);
    let diagonal = extent.width.hypot(extent.height);
    (signed_area(points).abs() > SLIVER * diagonal).then_some(extent)
}

/// The smallest rectangle around some points
pub(super) fn extent(points: &[PdfPoint]) -> PdfRect {
    let left = points
        .iter()
        .map(|point| point.x)
        .fold(f32::INFINITY, f32::min);
    let right = points
        .iter()
        .map(|point| point.x)
        .fold(f32::NEG_INFINITY, f32::max);
    let bottom = points
        .iter()
        .map(|point| point.y)
        .fold(f32::INFINITY, f32::min);
    let top = points
        .iter()
        .map(|point| point.y)
        .fold(f32::NEG_INFINITY, f32::max);
    PdfRect::new(left, bottom, right - left, top - bottom)
}

fn signed_area(points: &[PdfPoint]) -> f32 {
    let doubled: f32 = points
        .iter()
        .zip(points.iter().cycle().skip(1))
        .map(|(a, b)| a.x * b.y - b.x * a.y)
        .sum();
    doubled / 2.0
}

/// Which side of the line from `start` to `end` a point is on: positive to its left
fn side(start: PdfPoint, end: PdfPoint, point: PdfPoint) -> f32 {
    (end.x - start.x) * (point.y - start.y) - (end.y - start.y) * (point.x - start.x)
}

/// Where the segment from `from` to `to` crosses the line through `start` and `end`
fn crossing(from: PdfPoint, to: PdfPoint, start: PdfPoint, end: PdfPoint) -> PdfPoint {
    let from_side = side(start, end, from);
    let to_side = side(start, end, to);
    let t = from_side / (from_side - to_side);
    PdfPoint::new(from.x + (to.x - from.x) * t, from.y + (to.y - from.y) * t)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::CornerRadii;

    fn rect(left: f32, bottom: f32, width: f32, height: f32) -> Vec<PdfPoint> {
        outline(RoundedRect::new(
            PdfRect::new(left, bottom, width, height),
            CornerRadii::default(),
        ))
    }

    #[test]
    fn overlapping_rectangles_meet_in_their_overlap() {
        let overlap = bounds(&intersect(
            &rect(0.0, 0.0, 10.0, 10.0),
            &rect(5.0, 5.0, 10.0, 10.0),
        ));
        assert_eq!(overlap, Some(PdfRect::new(5.0, 5.0, 5.0, 5.0)));
    }

    #[test]
    fn separate_rectangles_have_nothing_in_common() {
        let apart = intersect(&rect(0.0, 0.0, 10.0, 10.0), &rect(20.0, 0.0, 10.0, 10.0));
        assert_eq!(bounds(&apart), None);
        // A clipped-out clip leaves nothing visible inside it either
        assert_eq!(
            bounds(&intersect(&rect(0.0, 0.0, 10.0, 10.0), &apart)),
            None
        );
    }

    #[test]
    fn a_rounded_corner_leaves_out_what_lies_outside_its_curve() {
        let circle = outline(RoundedRect::new(
            PdfRect::new(0.0, 0.0, 200.0, 200.0),
            CornerRadii::circular(100.0),
        ));
        // The top left corner of the circle's square, outside the circle
        assert_eq!(
            bounds(&intersect(&rect(0.0, 185.0, 40.0, 15.0), &circle)),
            None
        );
        // Across the curve: cut to where the circle reaches
        let cut = bounds(&intersect(&rect(0.0, 95.0, 30.0, 10.0), &circle))
            .expect("part of it is inside");
        assert!(
            cut.left >= 0.0 && cut.left < 1.0 && (cut.right() - 30.0).abs() < 1e-3,
            "{cut:?}"
        );
        let inside = bounds(&intersect(&rect(60.0, 60.0, 80.0, 80.0), &circle));
        assert_eq!(inside, Some(PdfRect::new(60.0, 60.0, 80.0, 80.0)));
    }
}
