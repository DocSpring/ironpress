//! Floats that occur in a paragraph's text (`<p>Total <span
//! style="float: right">…</span> due</p>`). Chromium keeps the text one
//! paragraph and floats each from the line it occurs on (CSS 2.1 §9.5.1 rule
//! 8: as high as possible), where a float that doesn't fit beside the line's
//! content so far goes below it.

use super::{BlockFloats, FloatBox, FloatFrames, TextOrigin, float_box};
use crate::layout::elements::LayoutNode;
use crate::layout::engine::TextRun;
use crate::layout::text::has_non_collapsible_text;
use crate::style::computed::{Clear, ComputedStyle};

/// The floats met so far in the text being collected, in order.
#[derive(Default)]
pub(crate) struct FloatsInText {
    nodes: Vec<LayoutNode>,
}

impl FloatsInText {
    /// Whether a child with `style` that floats occurs inside the text
    /// collected so far, rather than before it: it does when some text
    /// precedes it and `output` is where the innermost block's boxes go.
    pub(crate) fn occurs_in_text(
        style: &ComputedStyle,
        runs: &[TextRun],
        frames: &FloatFrames,
        output: &Vec<LayoutNode>,
    ) -> bool {
        super::FloatSide::of(style.float).is_some()
            && runs.iter().any(|run| has_non_collapsible_text(&run.text))
            && frames.is_innermost(output)
    }

    /// Take the boxes a floated child laid out: its float occurs where the
    /// text has got to, and anything else goes to `output`.
    pub(crate) fn take(
        &mut self,
        laid_out: Vec<LayoutNode>,
        runs: &mut Vec<TextRun>,
        output: &mut Vec<LayoutNode>,
    ) {
        for node in laid_out {
            if float_box(node.as_ref()).is_some() {
                runs.push(TextRun::float_anchor(self.nodes.len()));
                self.nodes.push(node);
            } else {
                output.push(node);
            }
        }
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }

    pub(crate) fn boxes(&self) -> Vec<FloatBox> {
        self.nodes
            .iter()
            .filter_map(|node| float_box(node.as_ref()))
            .collect()
    }

    /// The floats around the block about to be emitted into `output` that
    /// holds this text, with its own `margin_top` and `clear`: those it goes
    /// among and, when floats occur in the text, the formatting context they
    /// float in.
    pub(crate) fn around_block(
        &self,
        frames: &FloatFrames,
        output: &mut Vec<LayoutNode>,
        margin_top: f32,
        clear: Clear,
    ) -> Option<BlockFloats> {
        if self.nodes.is_empty() {
            frames.beside_next_block(output, margin_top, clear)
        } else {
            frames.around_next_block(output, margin_top, clear)
        }
    }

    /// Emit the floats into `output` ahead of the text's block, each kept
    /// from floating above the line `tops[i]` below the block's content-box
    /// top that it occurs on (the first line when `tops` has no entry for
    /// it). Without an `origin` the block isn't in the floats' formatting
    /// context, and they float as if they came before it.
    pub(crate) fn emit(
        &mut self,
        tops: &[f32],
        origin: Option<TextOrigin>,
        output: &mut Vec<LayoutNode>,
    ) {
        for (index, mut node) in self.nodes.drain(..).enumerate() {
            if let Some(owner) = node.block_flow_owner_mut() {
                owner.block_flow_mut().float_line =
                    origin.map(|origin| origin.line(tops.get(index).copied().unwrap_or_default()));
            }
            output.push(node);
        }
    }
}
