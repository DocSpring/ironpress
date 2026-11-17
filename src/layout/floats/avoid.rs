//! Boxes that start a formatting context of their own beside floats: a
//! table's, a flex or grid container's border box must not overlap the
//! floats in its parent's formatting context (CSS 2.1 §9.5). Chromium puts it
//! at the first place, from where it would start, that is wide enough for it:
//! beside the floats when it fits, below them when it doesn't. One without a
//! width of its own takes the space beside the floats there.

use super::flow::BlockFloats;
use super::{FloatExclusions, InlineSpan};
use crate::layout::elements::LayoutNode;
use crate::layout::paginate::estimate_element_height;
use crate::style::computed::ComputedStyle;

/// A formatting context root about to be laid out among floats.
#[derive(Debug)]
pub(crate) struct FloatAvoidance {
    exclusions: FloatExclusions,
    container: InlineSpan,
    /// Where its border box would start, and the bottom of the flow before it.
    border_top: f32,
    flow_bottom: f32,
    margin_left: f32,
    margin_right: f32,
    /// The width of the space it lays out in, margins included, when it has
    /// no width of its own.
    auto_width: Option<f32>,
}

impl FloatAvoidance {
    pub(super) fn new(floats: BlockFloats, style: &ComputedStyle) -> Self {
        let container = InlineSpan::new(0.0, floats.width);
        let mut avoidance = Self {
            exclusions: floats.exclusions,
            container,
            border_top: floats.border_top,
            flow_bottom: floats.flow_bottom,
            margin_left: style.margin.left,
            margin_right: style.margin.right,
            auto_width: None,
        };
        let has_width = style.width.is_some() || style.percentage_sizing.width.is_some();
        if !has_width {
            let top = avoidance.first_top(|space| space.width() > avoidance.margins());
            let space = avoidance.exclusions.space_at(top, container);
            avoidance.auto_width = Some(space.width());
        }
        avoidance
    }

    /// The containing width a box without a width of its own lays out in.
    pub(crate) const fn layout_width(&self) -> Option<f32> {
        self.auto_width
    }

    /// Move the box, the first in-flow box in `laid_out`, beside or below the
    /// floats.
    pub(crate) fn apply(&self, laid_out: &mut [LayoutNode]) {
        let Some(node) = laid_out
            .iter_mut()
            .find(|node| node.contributes_to_normal_flow() && node.margin_holder().is_some())
        else {
            return;
        };
        let (_, border_width) = crate::layout::block::child_x_and_width(node.as_ref());
        let margin_box_width = border_width + self.margins();
        let height = estimate_element_height(node.as_ref());
        let top = match self.auto_width {
            Some(_) => self.first_top(|space| space.width() > self.margins()),
            None => self.first_top_for(margin_box_width, height),
        };
        let space =
            self.exclusions
                .space(top, top + height.max(super::FIT_TOLERANCE), self.container);
        let shift = space.start - self.container.start;
        if let Some(owner) = node.positioning_owner_mut() {
            owner.positioning_mut().insets.left += shift;
        }
        // A larger top margin moves it down, and still collapses with the
        // previous block's bottom margin as its own would have.
        if top > self.border_top
            && let Some(holder) = node.margin_holder_mut()
        {
            holder.margins_mut().start = top - self.flow_bottom;
        }
    }

    const fn margins(&self) -> f32 {
        self.margin_left + self.margin_right
    }

    /// The first top, from where the box would start, where `fits` holds for
    /// the space beside the floats there.
    fn first_top(&self, fits: impl Fn(InlineSpan) -> bool) -> f32 {
        self.exclusions
            .candidate_tops(self.border_top)
            .find(|&top| fits(self.exclusions.space_at(top, self.container)))
            .unwrap_or_else(|| self.exclusions.lowest_bottom().max(self.border_top))
    }

    /// The first top where a margin box `width` wide and `height` tall fits
    /// beside the floats over its whole height.
    fn first_top_for(&self, width: f32, height: f32) -> f32 {
        self.exclusions
            .candidate_tops(self.border_top)
            .find(|&top| {
                self.exclusions
                    .space(top, top + height, self.container)
                    .fits(width)
            })
            .unwrap_or_else(|| self.exclusions.lowest_bottom().max(self.border_top))
    }
}
