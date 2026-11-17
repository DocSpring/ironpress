//! Float placement while a block formatting context's boxes are laid out.
//!
//! Each formatting context's boxes are emitted, in order, into one vector. The
//! context's frame names that vector; when a block needs to know the floats
//! beside it, the boxes emitted so far are walked with the same block-flow
//! simulation that pagination and painting use, placing each float with
//! [`FloatExclusions`] and recording where it went on the float itself.

use super::{FloatBox, FloatExclusions, FloatOffset, FloatSide, InlineSpan};
use crate::layout::elements::{LayoutElement, LayoutNode};
use crate::layout::engine::LineInsets;
use crate::layout::paginate::{BlockFlowAccumulator, element_float, estimate_element_height};
use crate::style::computed::Clear;

/// The vector a formatting context's boxes are emitted into. Layout that emits
/// into another vector (a table cell, a flex item) lays out a formatting
/// context of its own, which sees none of this one's floats.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct FlowId(usize);

impl FlowId {
    fn of(nodes: &Vec<LayoutNode>) -> Self {
        Self(std::ptr::from_ref(nodes).addr())
    }
}

/// One block's view of its formatting context: the vector it emits into, its
/// content width, and the floats of enclosing blocks that reach into it.
#[derive(Debug)]
struct FloatFrame {
    flow: FlowId,
    width: f32,
    inherited: FloatExclusions,
}

/// The frames of the blocks being laid out, innermost last.
#[derive(Debug, Default)]
pub(crate) struct FloatFrames {
    frames: Vec<FloatFrame>,
}

impl FloatFrames {
    /// Start laying out a block whose boxes go into `nodes`, `width` wide,
    /// with `inherited` floats from enclosing blocks in its own coordinates
    /// (none for a formatting context root).
    pub(crate) fn enter(
        &mut self,
        nodes: &Vec<LayoutNode>,
        width: f32,
        inherited: FloatExclusions,
    ) {
        self.frames.push(FloatFrame {
            flow: FlowId::of(nodes),
            width,
            inherited,
        });
    }

    /// Finish the innermost block: place the floats among its boxes that no
    /// later box asked about.
    pub(crate) fn leave(&mut self, nodes: &mut Vec<LayoutNode>) {
        if let Some(frame) = self.frames.pop()
            && frame.flow == FlowId::of(nodes)
        {
            place_floats(&frame, nodes);
        }
    }

    /// How a formatting context root about to be emitted into `nodes` with
    /// `style` avoids the floats beside it, if any reach it.
    pub(crate) fn avoidance(
        &self,
        nodes: &mut Vec<LayoutNode>,
        style: &crate::style::computed::ComputedStyle,
    ) -> Option<super::FloatAvoidance> {
        self.beside_next_block(nodes, style.margin.top, style.clear)
            .map(|floats| super::FloatAvoidance::new(floats, style))
    }

    /// Whether `nodes` is where the innermost block's boxes go.
    pub(crate) fn is_innermost(&self, nodes: &Vec<LayoutNode>) -> bool {
        self.frames
            .last()
            .is_some_and(|frame| frame.flow == FlowId::of(nodes))
    }

    /// The floats around a block about to be emitted into `nodes`, even
    /// none, when `nodes` is the innermost block's: what a block with floats
    /// in its own text lays its lines out among.
    /// `margin_top` and `clear` are the block's own.
    pub(crate) fn around_next_block(
        &self,
        nodes: &mut Vec<LayoutNode>,
        margin_top: f32,
        clear: Clear,
    ) -> Option<BlockFloats> {
        let frame = self
            .frames
            .last()
            .filter(|frame| frame.flow == FlowId::of(nodes))?;
        let (exclusions, flow) = place_floats(frame, nodes);
        Some(BlockFloats {
            exclusions,
            border_top: flow.next_block_top(margin_top, clear),
            flow_bottom: flow.flow_bottom(),
            width: frame.width,
        })
    }

    /// The floats beside a block about to be emitted into `nodes`, if any
    /// reach it. `margin_top` and `clear` are the block's own.
    pub(crate) fn beside_next_block(
        &self,
        nodes: &mut Vec<LayoutNode>,
        margin_top: f32,
        clear: Clear,
    ) -> Option<BlockFloats> {
        let frame = self
            .frames
            .last()
            .filter(|frame| frame.flow == FlowId::of(nodes))?;
        if frame.inherited.is_empty() && !nodes.iter().any(|node| is_float(node.as_ref())) {
            return None;
        }
        let (exclusions, flow) = place_floats(frame, nodes);
        let border_top = flow.next_block_top(margin_top, clear);
        (exclusions.lowest_bottom() > border_top).then_some(BlockFloats {
            exclusions,
            border_top,
            flow_bottom: flow.flow_bottom(),
            width: frame.width,
        })
    }
}

/// The floats beside one block, in its formatting context's coordinates.
#[derive(Debug, Clone)]
pub(crate) struct BlockFloats {
    pub(super) exclusions: FloatExclusions,
    pub(super) border_top: f32,
    /// The bottom of the flow before the block, below the previous block's
    /// bottom margin's collapsing.
    pub(super) flow_bottom: f32,
    pub(super) width: f32,
}

impl BlockFloats {
    /// What the block's lines must avoid. Its content box starts `top` below
    /// its border-box top and `left` from its containing block's content edge,
    /// and is `width` wide.
    pub(crate) fn lines(&self, top: f32, left: f32, width: f32) -> LineFloats {
        LineFloats {
            exclusions: self.exclusions.clone(),
            top: self.border_top + top,
            container: InlineSpan::new(left, left + width),
        }
    }

    /// The floats as a child block whose content box starts `top` below this
    /// block's border-box top and `left` from its containing block's edge sees
    /// them.
    pub(crate) fn inherited_by_child(&self, top: f32, left: f32) -> FloatExclusions {
        self.exclusions.translated(-left, -(self.border_top + top))
    }
}

/// The floats one block's line boxes avoid, relative to its content box.
#[derive(Debug, Clone)]
pub(crate) struct LineFloats {
    exclusions: FloatExclusions,
    top: f32,
    container: InlineSpan,
}

impl LineFloats {
    /// How much of the content box the floats take beside a line box `top`
    /// below the content-box top and `height` tall.
    pub(crate) fn insets(&self, top: f32, height: f32) -> LineInsets {
        let space = self
            .exclusions
            .line_space(self.top + top, height, self.container);
        LineInsets {
            left: (space.start - self.container.start).max(0.0),
            right: (self.container.end - space.end).max(0.0),
        }
    }

    /// Place a float that occurs in the block's text on the line `top` below
    /// the content-box top.
    pub(crate) fn place(&mut self, float: FloatBox, top: f32) {
        self.exclusions
            .place(float.request(self.top + top, self.container));
    }

    /// Where a line box that doesn't fit beside the floats at `top` tries
    /// next, relative to the content-box top; `None` when no float shortens it.
    pub(crate) fn next_line_top(&self, top: f32, height: f32) -> Option<f32> {
        self.exclusions
            .next_line_top(self.top + top, height)
            .map(|next| next - self.top)
    }
}

fn is_float(node: &dyn LayoutElement) -> bool {
    float_box(node).is_some()
}

/// Walk `nodes` as block flow, placing every float and recording its offset,
/// and return the floats and the flow position after the last box.
fn place_floats(
    frame: &FloatFrame,
    nodes: &mut [LayoutNode],
) -> (FloatExclusions, BlockFlowAccumulator) {
    let mut exclusions = frame.inherited.clone();
    let mut flow = BlockFlowAccumulator::default();
    for (index, node) in nodes.iter_mut().enumerate() {
        if let Some(float) = float_box(node.as_ref()) {
            let offset = place_float(
                &mut exclusions,
                frame.width,
                float,
                node,
                flow.next_float_top(),
            );
            if let Some(owner) = node.block_flow_owner_mut() {
                owner.block_flow_mut().float_offset = Some(offset);
            }
        }
        flow.include(index, node.as_ref(), estimate_element_height(node.as_ref()));
    }
    (exclusions, flow)
}

/// A float's side and margin box, or `None` for a box that doesn't float.
pub(crate) fn float_box(node: &dyn LayoutElement) -> Option<FloatBox> {
    if !node.contributes_to_normal_flow() {
        return None;
    }
    let side = FloatSide::of(element_float(node))?;
    let margins = node
        .block_flow_owner()
        .map(|owner| owner.block_flow().inline_margins)
        .unwrap_or_default();
    let (_, border_width) = crate::layout::block::child_x_and_width(node);
    Some(FloatBox {
        side,
        width: margins.left + border_width + margins.right,
        height: estimate_element_height(node),
    })
}

fn place_float(
    exclusions: &mut FloatExclusions,
    width: f32,
    float: FloatBox,
    node: &LayoutNode,
    flow_top: f32,
) -> FloatOffset {
    let (margins, line_offset) = node
        .block_flow_owner()
        .map(|owner| {
            (
                owner.block_flow().inline_margins,
                owner.block_flow().float_line_offset,
            )
        })
        .unwrap_or_default();
    let (border_left, _) = crate::layout::block::child_x_and_width(node.as_ref());
    let placed =
        exclusions.place(float.request(flow_top + line_offset, InlineSpan::new(0.0, width)));
    // Painting puts a left float's border box at `border_left` and a right
    // float's against the right edge; the offset moves it to its margin box's
    // place.
    let inline = match float.side {
        FloatSide::Left => placed.inline.start + margins.left - border_left,
        FloatSide::Right => placed.inline.end - margins.right - width,
    };
    FloatOffset {
        inline,
        block: placed.top - flow_top,
    }
}
