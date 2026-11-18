//! The vertical layout of a row of block-tagged inline-blocks
//! (`layout_inline_block_group_inner`), by the rules a line box follows
//! (CSS 2.1 §10.8): each box is aligned by its margin box with its
//! `vertical-align`, the row's strut keeps the line at least as tall as the
//! parent's `line-height`, and a `top` or `bottom` box taller than the rest
//! makes the row taller below or above its baseline, as Chromium does.

use crate::layout::vertical_align::{AlignedBox, AlignmentBasis, LineExtents};

/// One box in the row.
#[derive(Debug, Clone, Copy)]
pub(super) struct AtomicRowBox {
    pub(super) aligned: AlignedBox,
    /// Its own used `line-height`, which a percentage `vertical-align` refers to.
    pub(super) line_height: f32,
}

/// The parent's strut and the font measures `vertical-align` refers to.
#[derive(Debug, Clone, Copy)]
pub(super) struct AtomicRowStrut {
    pub(super) above: f32,
    pub(super) below: f32,
    pub(super) x_height: f32,
    pub(super) font_size: f32,
    pub(super) line_height: f32,
}

impl AtomicRowStrut {
    fn basis(self, item: AtomicRowBox) -> AlignmentBasis {
        AlignmentBasis {
            parent_font_size: self.font_size,
            parent_x_height: self.x_height,
            line_height: item.line_height,
        }
    }
}

/// The row's height, and each box's top margin edge below the row's top.
pub(super) fn align_atomic_row(boxes: &[AtomicRowBox], strut: AtomicRowStrut) -> (f32, Vec<f32>) {
    let mut line = LineExtents {
        above: strut.above,
        below: strut.below,
    };
    for item in boxes {
        if let Some(extents) = item.aligned.baseline_extents(strut.basis(*item)) {
            line.include(extents);
        }
    }
    line.fit_edge_aligned(boxes.iter().map(|item| item.aligned));
    let tops = boxes
        .iter()
        .map(|item| line.top_of(item.aligned, strut.basis(*item)))
        .collect();
    (line.height(), tops)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::style::computed::VerticalAlign;

    const STRUT: AtomicRowStrut = AtomicRowStrut {
        above: 15.0,
        below: 5.0,
        x_height: 7.0,
        font_size: 14.0,
        line_height: 20.0,
    };

    fn row_box(height: f32, ascent: f32, vertical_align: VerticalAlign) -> AtomicRowBox {
        AtomicRowBox {
            aligned: AlignedBox {
                height,
                ascent,
                vertical_align,
            },
            line_height: 20.0,
        }
    }

    fn empty_box(height: f32, vertical_align: VerticalAlign) -> AtomicRowBox {
        row_box(height, height, vertical_align)
    }

    #[test]
    fn a_baseline_box_without_text_rests_on_the_baseline_above_the_struts_descent() {
        let (height, tops) = align_atomic_row(&[empty_box(40.0, VerticalAlign::Baseline)], STRUT);
        assert_eq!((height, tops), (45.0, vec![0.0]));
    }

    #[test]
    fn a_box_with_text_aligns_its_last_baseline() {
        let item = row_box(40.0, 35.0, VerticalAlign::Baseline);
        let (height, tops) = align_atomic_row(&[item], STRUT);
        assert_eq!((height, tops), (40.0, vec![0.0]));
    }

    #[test]
    fn a_top_box_hangs_from_the_row_top_and_adds_no_descent() {
        let (height, tops) = align_atomic_row(&[empty_box(40.0, VerticalAlign::Top)], STRUT);
        assert_eq!((height, tops), (40.0, vec![0.0]));
    }

    #[test]
    fn a_tall_bottom_box_moves_the_baseline_down() {
        let boxes = [
            empty_box(10.0, VerticalAlign::Baseline),
            empty_box(40.0, VerticalAlign::Bottom),
        ];
        let (height, tops) = align_atomic_row(&boxes, STRUT);
        // The 20pt line grows by 20pt above its baseline, which is now 35pt down.
        assert_eq!((height, tops), (40.0, vec![25.0, 0.0]));
    }

    #[test]
    fn a_middle_box_centres_on_the_parents_mid_x_height() {
        let (height, tops) = align_atomic_row(&[empty_box(30.0, VerticalAlign::Middle)], STRUT);
        // 18.5pt above the baseline and 11.5pt below it.
        assert_eq!((height, tops), (30.0, vec![0.0]));
    }

    #[test]
    fn a_percentage_refers_to_the_boxs_own_line_height() {
        // Raised by half its own 60pt line height, not the parent's 20pt
        let mut item = empty_box(10.0, VerticalAlign::Percent(0.5));
        item.line_height = 60.0;
        let (height, tops) = align_atomic_row(&[item], STRUT);
        assert_eq!((height, tops), (45.0, vec![0.0]));
    }
}
