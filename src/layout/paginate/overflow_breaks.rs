//! Content that runs past the end of a box with a fixed height, and past the end of the page.
//!
//! A box with a definite `height` and visible overflow draws the content that doesn't fit past its own end. When
//! that content also runs past the end of the page, Chrome carries on drawing it at the top of the next page: CSS
//! Fragmentation 3 §2.1 breaks overflow independently of the box and of the flow after it. The box itself ends
//! where its height says, and what follows it goes below that end, as if the overflow weren't there.
//!
//! So the paginator draws the box with the content that fits on its page, and carries the rest on to the top of
//! the next page as a piece of the box without its decoration (no background, border or margins, but the same
//! left and right edges), which takes no room in the flow there and can overlap what's drawn on that page, as
//! Chrome's overflow does. A box whose overflow is clipped keeps hiding it, so nothing carries on from one.

use crate::layout::elements::{
    BlockSize, BoxModel, BoxPaint, Container, FragmentBreakRule, LayoutElement, LayoutNode,
    LayoutVisitor, TextBlock,
};
use crate::layout::roundoff::{exceeds_with_roundoff, is_positive_with_roundoff};
use crate::style::computed::{BoxDecorationBreak, Float, Position};

use super::{
    element_collapses_margins, element_float, element_flow_geometry, element_is_out_of_flow,
    estimate_element_height, sibling_margin_overlap, split_element,
};

/// Splits `element`, whose border box starts `available` above the end of the page, into what's drawn on this
/// page and the pieces of its overflow that carry on at the top of the next one. `None` when none of its content
/// runs past the end of the page.
pub(super) fn split_overflow(
    element: &dyn LayoutElement,
    available: f32,
) -> Option<(LayoutNode, Vec<LayoutNode>)> {
    struct Split {
        available: f32,
        result: Option<(LayoutNode, Vec<LayoutNode>)>,
    }

    impl LayoutVisitor for Split {
        fn visit_container(&mut self, element: &Container) {
            self.result = split_container(element, self.available)
                .map(|(here, pieces)| (Box::new(here) as LayoutNode, pieces));
        }

        fn visit_text_block(&mut self, element: &TextBlock) {
            self.result = split_text_block(element, self.available)
                .map(|(here, piece)| (Box::new(here) as LayoutNode, vec![piece]));
        }
    }

    let mut split = Split {
        available,
        result: None,
    };
    element.accept(&mut split);
    split.result
}

/// Whether a box in the flow can carry its overflow on: positioned and floated boxes are placed elsewhere, a clip
/// hides the overflow, and a transform, opacity or filter would have to apply to each piece as one group.
fn carries_overflow(position: Position, float: Float, clips: bool, paint: &BoxPaint) -> bool {
    position == Position::Static && float == Float::None && !clips && paint.group.is_identity()
}

fn split_container(element: &Container, available: f32) -> Option<(Container, Vec<LayoutNode>)> {
    if !carries_overflow(
        element.positioning.scheme,
        element.flow.float,
        element.overflow.combined.clips(),
        &element.paint,
    ) {
        return None;
    }
    // Only a fixed height leaves children below the box's end. An auto height grows to hold them, so its children
    // already fit on the page, and only their own overflow can run past its end.
    let definite = element.box_model.size.height.is_definite();
    let content_top = element.box_model.border.top.width + element.box_model.padding.top;
    let mut here = Vec::with_capacity(element.children.len());
    let mut moved = Vec::new();
    let mut pieces = Vec::new();
    let mut flow_end = 0.0f32;
    let mut prev_margin_bottom = None;
    let mut placed_in_flow = false;
    for child in &element.children {
        // A float or an absolutely positioned child stays with the box it's placed against
        if element_is_out_of_flow(child.as_ref()) || element_float(child.as_ref()) != Float::None {
            here.push(child.clone());
            continue;
        }
        let geometry = element_flow_geometry(child.as_ref());
        let overlap = sibling_margin_overlap(prev_margin_bottom, child.as_ref());
        let top = flow_end - overlap + geometry.margins.start;
        let room = available - content_top - top;
        flow_end += estimate_element_height(child.as_ref()) - overlap;
        prev_margin_bottom = element_collapses_margins(child.as_ref()).then(|| {
            child
                .margin_holder()
                .map(|holder| holder.margins().end)
                .unwrap_or_default()
        });
        if !moved.is_empty() {
            moved.push(child.clone());
            continue;
        }
        // The first child stays, so that each page draws some of the overflow
        let first = !std::mem::replace(&mut placed_in_flow, true);
        if definite && !first && !is_positive_with_roundoff(room) {
            moved.push(child.clone());
            continue;
        }
        if definite && exceeds_with_roundoff(geometry.content_height, room) {
            if let Some((head, tail)) = is_positive_with_roundoff(room)
                .then(|| split_element(child.as_ref(), room, FragmentBreakRule::Emergency))
                .flatten()
            {
                here.push(head);
                moved.push(tail);
                continue;
            }
            if !first {
                moved.push(child.clone());
                continue;
            }
        }
        match split_overflow(child.as_ref(), room) {
            Some((head, nested)) => {
                here.push(head);
                pieces.extend(
                    nested
                        .into_iter()
                        .map(|piece| Box::new(overflow_piece(element, vec![piece])) as LayoutNode),
                );
            }
            None => here.push(child.clone()),
        }
    }
    if let Some(first) = moved.first_mut() {
        first.suppress_first_fragment_spacing();
        pieces.insert(0, Box::new(overflow_piece(element, moved)));
    }
    if pieces.is_empty() {
        return None;
    }
    let mut drawn = element.clone();
    drawn.children = here;
    Some((drawn, pieces))
}

fn split_text_block(element: &TextBlock, available: f32) -> Option<(TextBlock, LayoutNode)> {
    if !element.box_model.size.height.is_definite()
        || !carries_overflow(
            element.positioning.scheme,
            element.flow.float,
            element.clipping.rect.is_some(),
            &element.paint,
        )
    {
        return None;
    }
    let room = available - element.box_model.border.top.width - element.box_model.padding.top;
    // The first line stays, so that each page draws some of the overflow
    let mut lines_end = 0.0f32;
    let mut fit = 0;
    for (index, line) in element.lines.iter().enumerate() {
        if index > 0 && exceeds_with_roundoff(lines_end + line.height, room) {
            break;
        }
        lines_end += line.height;
        fit = index + 1;
    }
    if fit == element.lines.len() {
        return None;
    }
    let mut drawn = element.clone();
    drawn.lines.truncate(fit);
    let mut piece = element.clone();
    piece.lines = element.lines[fit..].to_vec();
    strip_decoration(&mut piece.box_model, &mut piece.paint);
    piece.fragmentation.box_fragmentation.decoration = BoxDecorationBreak::Slice;
    piece.fragmentation.box_fragmentation.reference_slice = None;
    piece.suppress_first_fragment_spacing();
    Some((drawn, Box::new(piece)))
}

/// How far below its border box's top `element` draws when its visible overflow runs past its end, or `None` when
/// it draws nothing past its end. Content in the flow after the box overlaps that overflow, but a footnote area
/// mustn't: the paginator moves a footnote's reference to the next page instead.
pub(super) fn overflow_extent(element: &dyn LayoutElement) -> Option<f32> {
    let excess = overflow_excess(element);
    is_positive_with_roundoff(excess)
        .then(|| element_flow_geometry(element).content_height + excess)
}

/// How far past the end of its border box `element` draws its in-flow content, or 0
fn overflow_excess(element: &dyn LayoutElement) -> f32 {
    struct Excess {
        excess: f32,
    }

    impl LayoutVisitor for Excess {
        fn visit_container(&mut self, element: &Container) {
            if !carries_overflow(
                element.positioning.scheme,
                element.flow.float,
                element.overflow.combined.clips(),
                &element.paint,
            ) {
                return;
            }
            let content_top = element.box_model.border.top.width + element.box_model.padding.top;
            let mut flow_end = 0.0f32;
            let mut prev_margin_bottom = None;
            let mut drawn_end = 0.0f32;
            for child in &element.children {
                if element_is_out_of_flow(child.as_ref())
                    || element_float(child.as_ref()) != Float::None
                {
                    continue;
                }
                let geometry = element_flow_geometry(child.as_ref());
                let overlap = sibling_margin_overlap(prev_margin_bottom, child.as_ref());
                let top = flow_end - overlap + geometry.margins.start;
                drawn_end = drawn_end.max(
                    content_top + top + geometry.content_height + overflow_excess(child.as_ref()),
                );
                flow_end += estimate_element_height(child.as_ref()) - overlap;
                prev_margin_bottom = element_collapses_margins(child.as_ref()).then(|| {
                    child
                        .margin_holder()
                        .map(|holder| holder.margins().end)
                        .unwrap_or_default()
                });
            }
            // Measured against the same walk of the children, so that only overflow counts and not the small
            // differences from how the flow collapses margins
            let end = if element.box_model.size.height.is_definite() {
                element_flow_geometry(element).content_height
            } else {
                content_top
                    + flow_end
                    + element.box_model.padding.bottom
                    + element.box_model.border.bottom.width
            };
            self.excess = (drawn_end - end).max(0.0);
        }

        fn visit_text_block(&mut self, element: &TextBlock) {
            if !element.box_model.size.height.is_definite()
                || !carries_overflow(
                    element.positioning.scheme,
                    element.flow.float,
                    element.clipping.rect.is_some(),
                    &element.paint,
                )
            {
                return;
            }
            let lines: f32 = element.lines.iter().map(|line| line.height).sum();
            let drawn_end =
                element.box_model.border.top.width + element.box_model.padding.top + lines;
            self.excess = (drawn_end - element_flow_geometry(element).content_height).max(0.0);
        }
    }

    let mut visitor = Excess { excess: 0.0 };
    element.accept(&mut visitor);
    visitor.excess
}

/// A piece of `element` that holds `children` without the box's own decoration
fn overflow_piece(element: &Container, children: Vec<LayoutNode>) -> Container {
    let mut piece = element.clone();
    piece.children = children;
    strip_decoration(&mut piece.box_model, &mut piece.paint);
    piece.fragmentation.decoration = BoxDecorationBreak::Slice;
    piece.fragmentation.reference_slice = None;
    piece
}

/// Removes a box's height, vertical margins, padding and borders, and everything it paints, keeping its content's
/// left and right edges where they were: the side borders' widths become padding.
fn strip_decoration(box_model: &mut BoxModel, paint: &mut BoxPaint) {
    box_model.size.height = BlockSize::AUTO;
    box_model.margins.start = 0.0;
    box_model.margins.end = 0.0;
    box_model.padding.top = 0.0;
    box_model.padding.bottom = 0.0;
    box_model.padding.left += box_model.border.left.width;
    box_model.padding.right += box_model.border.right.width;
    box_model.border.top.width = 0.0;
    box_model.border.right.width = 0.0;
    box_model.border.bottom.width = 0.0;
    box_model.border.left.width = 0.0;
    *paint = BoxPaint {
        group: paint.group.clone(),
        visible: paint.visible,
        ..BoxPaint::default()
    };
}
