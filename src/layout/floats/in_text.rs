//! Floats that occur in a paragraph's text (`<p>Total <span
//! style="float: right">…</span> due</p>`). Chromium keeps the text one
//! paragraph and floats each from the line it occurs on (CSS 2.1 §9.5.1 rule
//! 8: as high as possible), where a float that doesn't fit beside the line's
//! content so far goes below it.

use super::{FloatBox, FloatFrames, LineFloats, float_box};
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

    /// The floats the text's lines go among, for an anonymous block about to
    /// be emitted into `output` whose content box is `left` from the
    /// formatting context's edge and `width` wide.
    pub(crate) fn line_floats(
        &self,
        frames: &FloatFrames,
        output: &mut Vec<LayoutNode>,
        left: f32,
        width: f32,
    ) -> Option<LineFloats> {
        let floats = if self.nodes.is_empty() {
            frames.beside_next_block(output, 0.0, Clear::None)
        } else {
            frames.around_next_block(output, 0.0, Clear::None)
        };
        floats.map(|floats| floats.lines(0.0, left, width))
    }

    /// Emit the floats into `output` ahead of the text's block, each kept
    /// from floating above the line `tops[i]` below the block's top that it
    /// occurs on.
    pub(crate) fn emit(&mut self, tops: &[f32], output: &mut Vec<LayoutNode>) {
        for (index, mut node) in self.nodes.drain(..).enumerate() {
            if let Some(owner) = node.block_flow_owner_mut() {
                owner.block_flow_mut().float_line_offset =
                    tops.get(index).copied().unwrap_or_default();
            }
            output.push(node);
        }
    }
}
