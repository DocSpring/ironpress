//! Absolutely positioned elements in a block's inline content.
//!
//! An absolute box is laid out by the block that holds the inline content
//! around it, beside that content, wherever it sits in the inline tree: a
//! child of the paragraph, or inside a `<span>` in it. With `top`/`bottom` or
//! `left`/`right` both `auto`, it sits at its static position, where it falls
//! in its line (CSS 2.1 §10.3.7, §10.6.4). Chrome puts it at the start of the
//! next word after it, at the top of that word's line box.

use std::collections::HashMap;

use crate::parser::css::{AncestorInfo, CssRule, SelectorContext};
use crate::parser::dom::{DomNode, ElementNode};
use crate::parser::ttf::TtfFont;
use crate::style::computed::{
    ComputedStyle, Float, TextAlign, compute_style_with_context_with_font_metrics,
};
use crate::style::font_metrics::FontMetrics;
use crate::types::Point;

use super::super::engine::{InlineBox, TextLine, TextRun};
use super::super::text::{OutOfFlowBoundary, TextWrapOptions, wrap_text_runs};
use super::{InlineFormattingChildren, InlineFormattingRole, InlineSiblingCursor};

/// An absolutely positioned element inside one of a block's inline elements.
pub(crate) struct NestedOutOfFlow<'dom> {
    pub(crate) element: &'dom ElementNode,
    /// The element index, among the block's children, of the inline element
    /// that holds it.
    pub(crate) holder: usize,
    /// The style of its inline parent, which it inherits from.
    pub(crate) parent_style: ComputedStyle,
    pub(crate) selector_context: SelectorContext<'dom>,
}

/// Every absolutely positioned element inside the block's inline elements,
/// in source order: the ones the run collector passes over inside them.
pub(crate) fn nested_out_of_flow<'dom>(
    children: &'dom [DomNode],
    roles: &InlineFormattingChildren,
    ancestors: &[AncestorInfo<'dom>],
    rules: &[CssRule],
    font_metrics: FontMetrics<'_>,
) -> Vec<NestedOutOfFlow<'dom>> {
    let walker = Walker {
        rules,
        font_metrics,
    };
    let mut found = Vec::new();
    let mut siblings = InlineSiblingCursor::starting_at(children, 0);
    let elements = children.iter().filter_map(|node| match node {
        DomNode::Element(element) => Some(element),
        DomNode::Text(_) => None,
    });
    for (index, element) in elements.enumerate() {
        let selector_context = siblings.next_context(element, ancestors);
        let Some(child) = roles.get(index) else {
            continue;
        };
        if walker.collects_contents(element, child.role, &child.style) {
            walker.walk(element, &child.style, &selector_context, index, &mut found);
        }
    }
    found
}

struct Walker<'a> {
    rules: &'a [CssRule],
    font_metrics: FontMetrics<'a>,
}

impl Walker<'_> {
    /// Whether the run collector collects `element`'s children into the
    /// block's lines: an inline element, other than a footnote or math, whose
    /// contents it gathers in their own way.
    fn collects_contents(
        &self,
        element: &ElementNode,
        role: InlineFormattingRole,
        style: &ComputedStyle,
    ) -> bool {
        role == InlineFormattingRole::Text
            && style.float != Float::Footnote
            && !element.attributes.contains_key("data-math")
    }

    fn walk<'dom>(
        &self,
        element: &'dom ElementNode,
        style: &ComputedStyle,
        selector_context: &SelectorContext<'dom>,
        holder: usize,
        found: &mut Vec<NestedOutOfFlow<'dom>>,
    ) {
        let mut ancestors = selector_context.ancestors.clone();
        ancestors.push(selector_context.as_ancestor(element));
        let mut siblings = InlineSiblingCursor::starting_at(&element.children, 0);
        for child in &element.children {
            let DomNode::Element(child) = child else {
                continue;
            };
            let child_context = siblings.next_context(child, &ancestors);
            let classes = child.class_list();
            let child_style = compute_style_with_context_with_font_metrics(
                child.tag,
                child.style_attr(),
                style,
                self.rules,
                child.tag_name(),
                &classes,
                child.id(),
                &child.attributes,
                &child_context,
                self.font_metrics,
            );
            let role = InlineFormattingRole::of(child, &child_style);
            if role == InlineFormattingRole::OutOfFlow {
                found.push(NestedOutOfFlow {
                    element: child,
                    holder,
                    parent_style: style.clone(),
                    selector_context: child_context,
                });
            } else if self.collects_contents(child, role, &child_style) {
                self.walk(child, &child_style, &child_context, holder, found);
            }
        }
    }
}

/// How the block lays out its lines, for finding a place in them.
#[derive(Clone, Copy)]
pub(crate) struct LineGeometry {
    pub(crate) wrap: TextWrapOptions,
    pub(crate) text_indent: f32,
    pub(crate) text_align: TextAlign,
    pub(crate) content_width: f32,
}

/// The static position of each absolute element, from the top-left of the
/// lines' content box.
///
/// The runs before an element are wrapped as the whole paragraph is: their
/// last line is the element's line, and its end is the element's place. A
/// zero-width box after them keeps a trailing space, which would hang at a
/// line's end, as the advance it is before the next word. A box that was
/// block-level starts on a line of its own: below that line when it has
/// content, at its start edge. `lines` are the whole paragraph's lines,
/// which align the element's line.
pub(crate) fn out_of_flow_static_positions(
    runs: &[TextRun],
    boundaries: &[OutOfFlowBoundary],
    lines: &[TextLine],
    geometry: LineGeometry,
    fonts: &HashMap<String, TtfFont>,
) -> Vec<Point> {
    boundaries
        .iter()
        .map(|boundary| static_position(runs, *boundary, lines, geometry, fonts))
        .collect()
}

fn static_position(
    runs: &[TextRun],
    boundary: OutOfFlowBoundary,
    lines: &[TextLine],
    geometry: LineGeometry,
    fonts: &HashMap<String, TtfFont>,
) -> Point {
    let line_start = |top: f32, first: bool| {
        let indent = if first { geometry.text_indent } else { 0.0 };
        Point::new(if boundary.block_level { 0.0 } else { indent }, top)
    };
    let Some(prefix) = runs
        .get(..boundary.runs_before)
        .filter(|prefix| !prefix.is_empty())
    else {
        return line_start(0.0, true);
    };
    let mut prefix = prefix.to_vec();
    prefix.push(TextRun {
        inline_box: Some(Box::new(InlineBox::advance_only(0.0))),
        ..prefix[prefix.len() - 1].text_fragment(String::new(), true)
    });
    let prefix_lines = wrap_text_runs(prefix, geometry.wrap, fonts);
    let Some((last, before)) = prefix_lines.split_last() else {
        return line_start(0.0, true);
    };
    let top: f32 = before.iter().map(|line| line.height).sum();
    let advance = crate::layout::helpers::measure_runs_width(&last.runs, fonts);
    if boundary.block_level {
        // The zero-width box closing the prefix adds no advance, so any
        // advance is content before the element.
        let has_content = advance > 0.0;
        return if has_content {
            line_start(top + last.height, false)
        } else {
            line_start(top, before.is_empty())
        };
    }
    let indent = if before.is_empty() {
        geometry.text_indent
    } else {
        0.0
    };
    let align = lines.get(before.len()).map_or(0.0, |line| {
        line_alignment_offset(line, indent, geometry, fonts)
    });
    Point::new(indent + last.x_offset + align + advance, top)
}

/// How far `text-align` moves a left-to-right line from its start edge.
fn line_alignment_offset(
    line: &TextLine,
    indent: f32,
    geometry: LineGeometry,
    fonts: &HashMap<String, TtfFont>,
) -> f32 {
    let share = match geometry.text_align {
        TextAlign::Left | TextAlign::Justify => return 0.0,
        TextAlign::Center => 0.5,
        TextAlign::Right => 1.0,
    };
    let width = crate::layout::helpers::measure_runs_width(&line.runs, fonts);
    ((geometry.content_width - indent - line.x_offset - width) * share).max(0.0)
}
