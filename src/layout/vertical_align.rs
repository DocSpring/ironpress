//! Where an inline-level box sits against its line's baseline (CSS 2.1 §10.8.1), for every path that builds or
//! paints a line box: the line wrapper, the line box's final metrics, the painter and rows of inline-blocks.

use crate::layout::inline_box::InlineBox;
use crate::render::pdf::{SUB_SHIFT_RATIO, SUPER_SHIFT_RATIO};
use crate::style::computed::VerticalAlign;

/// What a box's `vertical-align` refers to: its parent's font for `sub`, `super` and `middle`, and its own
/// `line-height` for a percentage.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct AlignmentBasis {
    pub(crate) parent_font_size: f32,
    pub(crate) parent_x_height: f32,
    pub(crate) line_height: f32,
}

/// An inline-level box, by its margin box.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct AlignedBox {
    pub(crate) height: f32,
    /// From the top margin edge to the box's baseline: its last line's baseline, or with none its bottom margin edge.
    pub(crate) ascent: f32,
    pub(crate) vertical_align: VerticalAlign,
}

impl AlignedBox {
    pub(crate) fn of(inline: &InlineBox) -> Self {
        Self {
            height: inline.outer_height(),
            ascent: inline.outer_baseline_ascent(),
            vertical_align: inline.vertical_align,
        }
    }

    /// How far the box reaches above and below the line's baseline, or `None` for a box aligned to the line box's
    /// top or bottom (`top`, `bottom`, and `text-top` and `text-bottom`, which are laid out as those), which is
    /// placed once the baseline-aligned boxes have set the line's height ([`LineExtents::fit_edge_aligned`]).
    pub(crate) fn baseline_extents(self, basis: AlignmentBasis) -> Option<(f32, f32)> {
        let descent = (self.height - self.ascent).max(0.0);
        let raised = |shift: f32| Some((self.ascent + shift, descent - shift));
        match self.vertical_align {
            VerticalAlign::Baseline => raised(0.0),
            VerticalAlign::Sub => raised(-basis.parent_font_size * SUB_SHIFT_RATIO),
            VerticalAlign::Super => raised(basis.parent_font_size * SUPER_SHIFT_RATIO),
            VerticalAlign::Length(length) => raised(length),
            VerticalAlign::Percent(percent) => raised(basis.line_height * percent),
            VerticalAlign::Middle => Some((
                self.height / 2.0 + basis.parent_x_height / 2.0,
                self.height / 2.0 - basis.parent_x_height / 2.0,
            )),
            VerticalAlign::Top
            | VerticalAlign::Bottom
            | VerticalAlign::TextTop
            | VerticalAlign::TextBottom => None,
        }
    }
}

/// How far a line box reaches above and below its baseline.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct LineExtents {
    pub(crate) above: f32,
    pub(crate) below: f32,
}

impl LineExtents {
    /// Reaches at least as far as `extents` on each side (a box doesn't make the line reach past the baseline on the
    /// other side).
    pub(crate) fn include(&mut self, (above, below): (f32, f32)) {
        self.above = self.above.max(above.max(0.0));
        self.below = self.below.max(below.max(0.0));
    }

    pub(crate) fn height(self) -> f32 {
        self.above + self.below
    }

    /// A `top`- or `bottom`-aligned box taller than the line makes the line taller, below the baseline for `top` and
    /// above it for `bottom`, so the line's text moves down beside a tall `bottom` box (CSS 2.1 §10.8, as Chromium
    /// applies them in source order).
    pub(crate) fn fit_edge_aligned(&mut self, boxes: impl IntoIterator<Item = AlignedBox>) {
        for aligned in boxes {
            let extra = aligned.height - self.height();
            if extra <= 0.0 {
                continue;
            }
            match aligned.vertical_align {
                VerticalAlign::Top | VerticalAlign::TextTop => self.below += extra,
                VerticalAlign::Bottom | VerticalAlign::TextBottom => self.above += extra,
                _ => {}
            }
        }
    }

    /// The top margin edge of `aligned`, below the line box's top.
    pub(crate) fn top_of(self, aligned: AlignedBox, basis: AlignmentBasis) -> f32 {
        match (aligned.vertical_align, aligned.baseline_extents(basis)) {
            (_, Some((above, _))) => self.above - above,
            (VerticalAlign::Bottom | VerticalAlign::TextBottom, None) => {
                self.height() - aligned.height
            }
            _ => 0.0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const BASIS: AlignmentBasis = AlignmentBasis {
        parent_font_size: 14.0,
        parent_x_height: 7.0,
        line_height: 20.0,
    };

    fn aligned(height: f32, ascent: f32, vertical_align: VerticalAlign) -> AlignedBox {
        AlignedBox {
            height,
            ascent,
            vertical_align,
        }
    }

    #[test]
    fn each_value_moves_the_box_by_what_it_refers_to() {
        let extents = |vertical_align| aligned(30.0, 24.0, vertical_align).baseline_extents(BASIS);
        assert_eq!(extents(VerticalAlign::Baseline), Some((24.0, 6.0)));
        assert_eq!(extents(VerticalAlign::Length(4.0)), Some((28.0, 2.0)));
        assert_eq!(extents(VerticalAlign::Percent(0.5)), Some((34.0, -4.0)));
        assert_eq!(extents(VerticalAlign::Middle), Some((18.5, 11.5)));
        let (above, below) = extents(VerticalAlign::Super).unwrap();
        assert!((above - (24.0 + 14.0 * SUPER_SHIFT_RATIO)).abs() < 1e-4);
        assert!((below - (6.0 - 14.0 * SUPER_SHIFT_RATIO)).abs() < 1e-4);
        for edge in [
            VerticalAlign::Top,
            VerticalAlign::Bottom,
            VerticalAlign::TextTop,
            VerticalAlign::TextBottom,
        ] {
            assert_eq!(extents(edge), None);
        }
    }

    #[test]
    fn a_box_never_makes_the_line_reach_past_the_baseline_on_its_other_side() {
        let mut line = LineExtents {
            above: 10.0,
            below: 3.0,
        };
        line.include((30.0, -4.0));
        assert_eq!(
            line,
            LineExtents {
                above: 30.0,
                below: 3.0
            }
        );
    }

    #[test]
    fn a_tall_top_box_grows_the_line_below_and_a_bottom_box_above() {
        let mut line = LineExtents {
            above: 15.0,
            below: 5.0,
        };
        line.fit_edge_aligned([aligned(30.0, 30.0, VerticalAlign::Top)]);
        assert_eq!(
            line,
            LineExtents {
                above: 15.0,
                below: 15.0
            }
        );
        line.fit_edge_aligned([aligned(40.0, 40.0, VerticalAlign::TextBottom)]);
        assert_eq!(
            line,
            LineExtents {
                above: 25.0,
                below: 15.0
            }
        );
        let bottom = aligned(40.0, 40.0, VerticalAlign::Bottom);
        assert_eq!(line.top_of(bottom, BASIS), 0.0);
        assert_eq!(
            line.top_of(aligned(10.0, 10.0, VerticalAlign::Bottom), BASIS),
            30.0
        );
        assert_eq!(
            line.top_of(aligned(10.0, 8.0, VerticalAlign::Baseline), BASIS),
            17.0
        );
    }
}
