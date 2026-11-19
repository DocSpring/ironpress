//! Floats that would be drawn past the end of their page.
//!
//! Each float's offset comes from its block formatting context, laid out as one long unpaginated flow: below the
//! earlier floats on its side, or for a float in a paragraph's text, from the top of the line it occurs on. Drawn at
//! that offset, a float can land below the page's end, where nothing paints it. A float that doesn't fit goes to the
//! next page instead (CSS 2.1 §9.5.1 rule 8, CSS Fragmentation 3 §5.1, as Chrome does), and what it's positioned
//! against goes with it:
//!
//! - A float in a paragraph's text goes with its line. Where the paragraph breaks is decided first: each of its
//!   floats on a line after the break moves to the next page, as far below the continuation's top as its line is,
//!   and one on a line before the break that doesn't fit beside it starts the next page. When the paragraph doesn't
//!   fit and can't break, it moves to the next page whole, and its floats with it.
//! - Any other float that doesn't fit starts the next page, and the floats stacked below it (the rows of a float
//!   grid) move up with it.
//!
//! A float that can break (text or a box, not an image or a `break-inside: avoid` box) is split at the page end
//! instead, as Chrome fragments floats like any block: its first part is drawn here and the rest carries on at the
//! top of the next page, where the floats that move there stack below it.

use std::collections::VecDeque;

use crate::layout::elements::{
    BlockSize, Container, IntoLayoutNode, LayoutElement, LayoutNode, LayoutVisitor,
    LayoutVisitorMut, PageBreak, Table, TextBlock,
};
use crate::layout::engine::PageBreakSide;
use crate::layout::floats::{FloatLine, FloatOffset};
use crate::layout::roundoff::exceeds_with_roundoff;
use crate::style::computed::{BoxDecorationBreak, Float};

use super::{
    FloatBottoms, FragmentBreakRule, PendingElement, collapse_vertical_margins, element_clear,
    element_float, element_float_offset, element_flow_geometry, split_element,
};

/// Where the paginator is about to draw a float.
#[derive(Debug, Clone, Copy)]
pub(super) struct FloatSpot {
    /// The top of the float's margin box before its offset: where the flow is.
    pub(super) flow_top: f32,
    /// The bottom margin before the float, which the next block's top margin collapses with.
    pub(super) prev_margin_bottom: f32,
    /// The floats drawn on the page so far, which the next block may clear.
    pub(super) floats_above: FloatBottoms,
    /// The top of its border box, with its offset.
    pub(super) top: f32,
    pub(super) height: f32,
    /// The end of the page's content area.
    pub(super) page_bottom: f32,
    /// The page holds content a forced break would end it after.
    pub(super) page_has_content: bool,
    /// The height of the tallest float part already carried over to the top of the next page, which a float moved
    /// there goes below.
    pub(super) carried_height: f32,
}

/// What became of a float the paginator was about to draw.
pub(super) enum FloatOutcome {
    /// It's drawn where it is.
    Here(LayoutNode),
    /// It and what goes with it are back in the queue, rearranged.
    Requeued,
    /// Its first part is drawn here, and the rest carries on at the top of the next page.
    Split { here: LayoutNode, rest: LayoutNode },
}

impl FloatSpot {
    /// Draws `float` here, or moves it and what goes with it to the next page by rearranging `work`. A float in a
    /// paragraph's text that isn't `settled` yet waits for the paragraph's break to be decided.
    pub(super) fn settle(
        self,
        float: LayoutNode,
        settled: bool,
        work: &mut VecDeque<PendingElement>,
    ) -> FloatOutcome {
        if !settled
            && float_line(float.as_ref()).is_some()
            && let Some(index) = paragraph_after_floats(work)
        {
            return self.settle_with_paragraph(float, index, work);
        }
        if exceeds_with_roundoff(self.top + self.height, self.page_bottom) {
            if let Some((here, rest)) = split_float(float.as_ref(), self.page_bottom - self.top) {
                return FloatOutcome::Split { here, rest };
            }
            return self.start_next_page(float, work);
        }
        FloatOutcome::Here(float)
    }

    /// A float in the text of the paragraph at `index` in `work`, after the run of floats at its front. Each float
    /// goes back in the queue settled, beside its paragraph or its fragment, or before it on the next page.
    fn settle_with_paragraph(
        self,
        float: LayoutNode,
        index: usize,
        work: &mut VecDeque<PendingElement>,
    ) -> FloatOutcome {
        let Some(text) = work.remove(index) else {
            return FloatOutcome::Here(float);
        };
        let geometry = element_flow_geometry(text.element.as_ref());
        // The paragraph clears earlier floats and its top margin collapses here as they will when it's drawn,
        // which can differ from the unpaginated flow the floats were placed in (at a page's top, in a
        // continuation).
        let flow = self.flow_top - self.prev_margin_bottom;
        let paragraph_top = self
            .floats_above
            .cleared(flow, element_clear(text.element.as_ref()))
            + collapse_vertical_margins(self.prev_margin_bottom, geometry.margins.start)
            - self.flow_top;
        let floats: Vec<LayoutNode> = std::iter::once(float)
            .chain(work.drain(..index).map(|pending| pending.element))
            .map(|mut float| {
                align_to_paragraph(&mut float, paragraph_top);
                float
            })
            .collect();
        let available = self.page_bottom - self.flow_top - paragraph_top;
        let fits = !exceeds_with_roundoff(geometry.content_height, available);
        let split = if fits {
            None
        } else {
            split_element(text.element.as_ref(), available, FragmentBreakRule::Normal)
        };
        match split {
            Some((first, rest)) => {
                let first_height = element_flow_geometry(first.as_ref()).content_height;
                let (here, next): (Vec<LayoutNode>, Vec<LayoutNode>) =
                    floats.into_iter().partition(|float| {
                        line_top(float.as_ref()) < first_height && self.stays(float.as_ref())
                    });
                work.push_front(PendingElement::continuation(rest));
                for mut float in next.into_iter().rev() {
                    let line = float_line(float.as_ref()).unwrap_or_default();
                    if line.line_top < first_height {
                        // It doesn't fit beside its line before the break: it starts the next page
                        let offset = offset_block(float.as_ref());
                        move_up(&mut float, offset, FloatLine::default());
                    } else {
                        // Its line is in the continuation, which starts the next page without the top margin or
                        // the first fragment
                        let in_rest = FloatLine {
                            paragraph_top: 0.0,
                            line_top: line.line_top - first_height,
                        };
                        move_up(&mut float, line.paragraph_top + first_height, in_rest);
                    }
                    work.push_front(PendingElement::fresh(float));
                }
                work.push_front(PendingElement::fresh(page_break()));
                work.push_front(PendingElement {
                    element: first,
                    is_fragment_continuation: text.is_fragment_continuation,
                    avoid_group: None,
                    float_settled: false,
                });
                push_settled(here, work);
            }
            // It stays on this page, and so do its floats. One that doesn't fit beside its line starts the next
            // page once the paragraph is drawn, as a float that isn't in the text.
            None if fits => {
                let (here, after): (Vec<LayoutNode>, Vec<LayoutNode>) = floats
                    .into_iter()
                    .partition(|float| self.stays(float.as_ref()));
                for mut float in after.into_iter().rev() {
                    let offset = offset_block(float.as_ref());
                    move_up(&mut float, offset, FloatLine::default());
                    set_line(&mut float, None);
                    work.push_front(PendingElement::settled_float(float));
                }
                work.push_front(text);
                push_settled(here, work);
            }
            // It moves to the next page whole, and its floats with it, at the same distance from its top
            None if self.page_has_content => {
                work.push_front(text);
                for float in floats.into_iter().rev() {
                    work.push_front(PendingElement::fresh(float));
                }
                work.push_front(PendingElement::fresh(page_break()));
            }
            // At the top of an empty page and it can't break: there's nothing to move it past
            None => {
                work.push_front(text);
                push_settled(floats, work);
            }
        }
        FloatOutcome::Requeued
    }

    /// A float that doesn't fit starts the next page, and the floats stacked below it move up with it.
    fn start_next_page(
        self,
        mut float: LayoutNode,
        work: &mut VecDeque<PendingElement>,
    ) -> FloatOutcome {
        let offset = offset_block(float.as_ref());
        if !self.page_has_content && offset <= 0.0 {
            // Already at the top of an empty page: a float taller than the page is drawn here
            return FloatOutcome::Here(float);
        }
        // It goes to the top of the next page, below the parts of floats carried over there
        let by = offset - self.carried_height;
        move_up(&mut float, by, FloatLine::default());
        for pending in work.iter_mut() {
            if element_float(pending.element.as_ref()) == Float::None {
                break;
            }
            if float_line(pending.element.as_ref()).is_none() {
                move_up(&mut pending.element, by, FloatLine::default());
            }
        }
        work.push_front(PendingElement::settled_float(float));
        if self.page_has_content {
            work.push_front(PendingElement::fresh(page_break()));
        }
        FloatOutcome::Requeued
    }

    /// Whether `float` fits on the page drawn at its offset from where the flow is.
    fn fits(self, float: &dyn LayoutElement) -> bool {
        let geometry = element_flow_geometry(float);
        let bottom =
            self.flow_top + geometry.margins.start + offset_block(float) + geometry.content_height;
        !exceeds_with_roundoff(bottom, self.page_bottom)
    }

    /// Whether `float` is drawn on this page: it fits, or it can break and some of it fits (the rest carries on
    /// on the next page).
    fn stays(self, float: &dyn LayoutElement) -> bool {
        if self.fits(float) {
            return true;
        }
        let geometry = element_flow_geometry(float);
        let top = self.flow_top + geometry.margins.start + offset_block(float);
        split_float(float, self.page_bottom - top).is_some()
    }
}

/// The parts of split floats that carry on at the top of a later page.
#[derive(Default)]
pub(super) struct Carried {
    parts: Vec<(usize, LayoutNode)>,
}

impl Carried {
    /// Carries `rest` on at the top of the page with index `page`.
    pub(super) fn push(&mut self, page: usize, rest: LayoutNode) {
        self.parts.push((page, rest));
    }

    /// The parts to draw at the top of the page with index `page` (or an earlier one, past a blank page).
    pub(super) fn take_for(&mut self, page: usize) -> Vec<LayoutNode> {
        let (now, later): (Vec<_>, Vec<_>) = std::mem::take(&mut self.parts)
            .into_iter()
            .partition(|(at, _)| *at <= page);
        self.parts = later;
        now.into_iter().map(|(_, part)| part).collect()
    }

    /// The height of the tallest part carried on to the page with index `page`, which a float moved there goes
    /// below.
    pub(super) fn height_on(&self, page: usize) -> f32 {
        self.parts
            .iter()
            .filter(|(at, _)| *at == page)
            .map(|(_, part)| {
                let geometry = element_flow_geometry(part.as_ref());
                geometry.margins.start + geometry.content_height + geometry.margins.end
            })
            .fold(0.0, f32::max)
    }

    pub(super) fn is_empty(&self) -> bool {
        self.parts.is_empty()
    }
}

/// Splits a float that can break at `available` below its border box's top: its first part, and the rest, which
/// is drawn at the top of the next page. `None` for a float that can't break there (an image, a
/// `break-inside: avoid` box, no room for any of it).
pub(super) fn split_float(
    float: &dyn LayoutElement,
    available: f32,
) -> Option<(LayoutNode, LayoutNode)> {
    if !can_break(float) || available <= 0.0 {
        return None;
    }
    // The block splitters leave floats whole, as an in-flow box's floated descendants move with their line; a float
    // that the paginator draws itself breaks like a block
    let mut block = float.clone_box();
    let flow = block.block_flow_owner_mut()?.block_flow_mut();
    let floated = *flow;
    flow.float = Float::None;
    let (mut here, mut rest) = split_element(block.as_ref(), available, FragmentBreakRule::Normal)?;
    if let Some(owner) = here.block_flow_owner_mut() {
        *owner.block_flow_mut() = floated;
    }
    fill_to_page_end(&mut here, available);
    if let Some(owner) = rest.block_flow_owner_mut() {
        let flow = owner.block_flow_mut();
        *flow = floated;
        flow.float_offset = floated.float_offset.map(|offset| FloatOffset {
            block: 0.0,
            ..offset
        });
        flow.float_line = None;
    }
    rest.suppress_first_fragment_spacing();
    Some((here, rest))
}

/// Makes a float's first part reach the page end, as a box that breaks fills the rest of its page (Chrome draws
/// its background and side borders down to the end). A container's splitter already does.
fn fill_to_page_end(part: &mut LayoutNode, available: f32) {
    struct Fill(f32);
    impl LayoutVisitorMut for Fill {
        fn visit_text_block(&mut self, element: &mut TextBlock) {
            if element.fragmentation.box_fragmentation.decoration == BoxDecorationBreak::Clone {
                return;
            }
            // A text block's height is its padding box's
            let text_height = element.lines.iter().map(|line| line.height).sum::<f32>();
            let natural = element.box_model.padding.vertical() + text_height;
            let filled = self.0 - element.box_model.border.vertical_width();
            if filled > element.box_model.size.height.resolve(natural) {
                element.box_model.size.height = BlockSize::fragment(filled);
            }
        }
    }
    part.accept_mut(&mut Fill(available));
}

/// Whether a float can break across pages: text or a box. Chrome doesn't split an image.
pub(super) fn can_break(element: &dyn LayoutElement) -> bool {
    struct Breakable(bool);
    impl LayoutVisitor for Breakable {
        fn visit_text_block(&mut self, _element: &TextBlock) {
            self.0 = true;
        }
        fn visit_container(&mut self, _element: &Container) {
            self.0 = true;
        }
        fn visit_table(&mut self, _element: &Table) {
            self.0 = true;
        }
    }
    let mut visitor = Breakable(false);
    element.accept(&mut visitor);
    visitor.0
}

/// The index of the paragraph after the run of floats at the front of `work`, when it's text the floats can be in.
fn paragraph_after_floats(work: &VecDeque<PendingElement>) -> Option<usize> {
    let index = work
        .iter()
        .position(|pending| element_float(pending.element.as_ref()) == Float::None)?;
    is_text_block(work[index].element.as_ref()).then_some(index)
}

fn push_settled(floats: Vec<LayoutNode>, work: &mut VecDeque<PendingElement>) {
    for float in floats.into_iter().rev() {
        work.push_front(PendingElement::settled_float(float));
    }
}

/// The line a float in a paragraph's text occurs on; `None` for any other float.
fn float_line(element: &dyn LayoutElement) -> Option<FloatLine> {
    element
        .block_flow_owner()
        .and_then(|owner| owner.block_flow().float_line)
}

/// How far below its paragraph's top a float's line is, or 0 for a float that isn't in text.
fn line_top(element: &dyn LayoutElement) -> f32 {
    float_line(element).map_or(0.0, |line| line.line_top)
}

fn offset_block(element: &dyn LayoutElement) -> f32 {
    element_float_offset(element).map_or(0.0, |offset| offset.block)
}

/// Moves a float's line to where the paginator puts its paragraph, `paragraph_top` below the float's place in the
/// flow, and the float with it.
fn align_to_paragraph(float: &mut LayoutNode, paragraph_top: f32) {
    let Some(line) = float_line(float.as_ref()) else {
        return;
    };
    let by = line.paragraph_top - paragraph_top;
    move_up(
        float,
        by,
        FloatLine {
            paragraph_top,
            ..line
        },
    );
}

/// Moves a float up `by` toward the top of the page it'll be drawn on (down for a negative `by`), and a float in
/// text's line to `line`.
fn move_up(float: &mut LayoutNode, by: f32, line: FloatLine) {
    let Some(owner) = float.block_flow_owner_mut() else {
        return;
    };
    let flow = owner.block_flow_mut();
    if let Some(offset) = flow.float_offset.as_mut() {
        offset.block = (offset.block - by).max(0.0);
    }
    if flow.float_line.is_some() {
        flow.float_line = Some(line);
    }
}

fn set_line(float: &mut LayoutNode, line: Option<FloatLine>) {
    if let Some(owner) = float.block_flow_owner_mut() {
        owner.block_flow_mut().float_line = line;
    }
}

fn is_text_block(element: &dyn LayoutElement) -> bool {
    struct IsText(bool);
    impl LayoutVisitor for IsText {
        fn visit_text_block(&mut self, _element: &TextBlock) {
            self.0 = true;
        }
    }
    let mut visitor = IsText(false);
    element.accept(&mut visitor);
    visitor.0
}

fn page_break() -> LayoutNode {
    PageBreak {
        side: PageBreakSide::Any,
        page_name: None,
    }
    .boxed()
}
