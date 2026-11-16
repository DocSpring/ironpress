//! The vertical layout of a row of block-tagged inline-blocks
//! (`layout_inline_block_group_inner`), by the rules a line box follows
//! (CSS 2.1 §10.8): each box is aligned by its margin box with its
//! `vertical-align`, the row's strut keeps the line at least as tall as the
//! parent's `line-height`, and a `top` or `bottom` box taller than the rest
//! makes the row taller below or above its baseline, as Chromium does.

use crate::style::computed::VerticalAlign;

/// One box in the row, by its margin box.
#[derive(Debug, Clone, Copy)]
pub(super) struct AtomicRowBox {
    pub(super) height: f32,
    /// From the top margin edge to the box's baseline: its last line's
    /// baseline, or with none its bottom margin edge (CSS 2.1 §10.8.1).
    pub(super) ascent: f32,
    pub(super) vertical_align: VerticalAlign,
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

/// The row's height, and each box's top margin edge below the row's top.
pub(super) fn align_atomic_row(boxes: &[AtomicRowBox], strut: AtomicRowStrut) -> (f32, Vec<f32>) {
    let mut above = strut.above;
    let mut below = strut.below;
    let extents: Vec<Option<(f32, f32)>> = boxes
        .iter()
        .map(|item| baseline_extents(*item, strut))
        .collect();
    for (box_above, box_below) in extents.iter().flatten() {
        above = above.max(box_above.max(0.0));
        below = below.max(box_below.max(0.0));
    }
    for item in boxes {
        let extra = item.height - (above + below);
        if extra <= 0.0 {
            continue;
        }
        match item.vertical_align {
            VerticalAlign::Top | VerticalAlign::TextTop => below += extra,
            VerticalAlign::Bottom | VerticalAlign::TextBottom => above += extra,
            _ => {}
        }
    }
    let height = above + below;
    let tops = boxes
        .iter()
        .zip(&extents)
        .map(|(item, extent)| match (item.vertical_align, extent) {
            (_, Some((box_above, _))) => above - box_above,
            (VerticalAlign::Bottom | VerticalAlign::TextBottom, None) => height - item.height,
            _ => 0.0,
        })
        .collect();
    (height, tops)
}

/// How far a box aligned to the baseline reaches above and below it; `None`
/// for a box aligned to the row's top or bottom.
fn baseline_extents(item: AtomicRowBox, strut: AtomicRowStrut) -> Option<(f32, f32)> {
    let descent = item.height - item.ascent;
    let shifted = |shift: f32| Some((item.ascent + shift, descent - shift));
    match item.vertical_align {
        VerticalAlign::Baseline => shifted(0.0),
        VerticalAlign::Sub => shifted(-strut.font_size * crate::render::pdf::SUB_SHIFT_RATIO),
        VerticalAlign::Super => shifted(strut.font_size * crate::render::pdf::SUPER_SHIFT_RATIO),
        VerticalAlign::Length(value) => shifted(value),
        VerticalAlign::Percent(percent) => shifted(strut.line_height * percent),
        VerticalAlign::Middle => Some((
            item.height / 2.0 + strut.x_height / 2.0,
            item.height / 2.0 - strut.x_height / 2.0,
        )),
        // `text-top` and `text-bottom` are laid out as `top` and `bottom`, as
        // in a line of text (`resolve_line_box_metrics`).
        VerticalAlign::Top
        | VerticalAlign::Bottom
        | VerticalAlign::TextTop
        | VerticalAlign::TextBottom => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const STRUT: AtomicRowStrut = AtomicRowStrut {
        above: 15.0,
        below: 5.0,
        x_height: 7.0,
        font_size: 14.0,
        line_height: 20.0,
    };

    fn empty_box(height: f32, vertical_align: VerticalAlign) -> AtomicRowBox {
        AtomicRowBox {
            height,
            ascent: height,
            vertical_align,
        }
    }

    #[test]
    fn a_baseline_box_without_text_rests_on_the_baseline_above_the_struts_descent() {
        let (height, tops) = align_atomic_row(&[empty_box(40.0, VerticalAlign::Baseline)], STRUT);
        assert_eq!((height, tops), (45.0, vec![0.0]));
    }

    #[test]
    fn a_box_with_text_aligns_its_last_baseline() {
        let item = AtomicRowBox {
            height: 40.0,
            ascent: 35.0,
            vertical_align: VerticalAlign::Baseline,
        };
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
}
