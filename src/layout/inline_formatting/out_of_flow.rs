//! Absolutely positioned elements in a block's inline content.
//!
//! An absolute box is laid out by the block that holds the inline content
//! around it, beside that content, wherever it sits in the inline tree: a
//! child of the paragraph, or inside a `<span>` in it. With `top`/`bottom` or
//! `left`/`right` both `auto`, it sits at its static position, where it falls
//! in its line (CSS 2.1 §10.3.7, §10.6.4). Chrome puts it at the start of the
//! next word after it, at the top of that word's line box.

use std::collections::HashMap;

use crate::parser::css::{AncestorInfo, CssRule, CssValue, SelectorContext, StyleMap};
use crate::parser::dom::{DomNode, ElementNode};
use crate::parser::ttf::TtfFont;
use crate::style::computed::{
    ComputedStyle, TextAlign, compute_style_with_context_with_font_metrics,
};
use crate::style::font_metrics::FontMetrics;
use crate::types::Point;

use super::super::engine::{TextLine, TextRun};
use super::super::text::{
    OutOfFlowBoundary, OutOfFlowPlace, TextWrapOptions, wrap_text_runs_among_floats,
};
use super::blocks_in_inline::collects_contents;
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
    if !may_hold_out_of_flow(children, rules) {
        return Vec::new();
    }
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
        if collects_contents(element, child.role, &child.style) {
            walker.walk(element, &child.style, &selector_context, index, &mut found);
        }
    }
    found
}

/// Whether an element under `children` could be absolutely positioned: a rule or an inline style gives `position`
/// (or `all`) a value that can make it `absolute` or `fixed`. Finding the elements means computing the style of every
/// inline element under the block, so a block that can't hold one skips that.
fn may_hold_out_of_flow(children: &[DomNode], rules: &[CssRule]) -> bool {
    let positions = |declarations: &StyleMap| {
        ["position", "all"].iter().any(|property| {
            declarations
                .properties
                .get(*property)
                .is_some_and(|value| !keeps_in_flow(value))
        })
    };
    if rules.iter().any(|rule| positions(&rule.declarations)) {
        return true;
    }
    let mut pending: Vec<&DomNode> = children.iter().collect();
    while let Some(node) = pending.pop() {
        let DomNode::Element(element) = node else {
            continue;
        };
        let names_position = element.style_attr().is_some_and(|style| {
            let style = style.to_ascii_lowercase();
            style.contains("position") || style.contains("all")
        });
        if names_position {
            return true;
        }
        pending.extend(element.children.iter());
    }
    false
}

/// A `position` value that leaves a box in flow, or an `all` value that resets `position` to one.
fn keeps_in_flow(value: &CssValue) -> bool {
    const IN_FLOW: [&str; 6] = ["static", "relative", "sticky", "initial", "unset", "revert"];
    matches!(value, CssValue::Keyword(keyword) if IN_FLOW.iter().any(|in_flow| keyword.eq_ignore_ascii_case(in_flow)))
}

struct Walker<'a> {
    rules: &'a [CssRule],
    font_metrics: FontMetrics<'a>,
}

impl Walker<'_> {
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
            } else if collects_contents(child, role, &child_style) {
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
/// `runs` are wrapped once as the whole paragraph is, with an anchor where each
/// element is (`RunAnchor::OutOfFlow`): the anchor's line is the element's
/// line, and its place there is where a zero-width box would go, after the
/// space before it, which would hang at a line's end without the box. A box
/// that was block-level starts on a line of its own: below that line when it
/// has content, at its start edge. `lines` are the whole paragraph's lines,
/// which align the element's line.
pub(crate) fn out_of_flow_static_positions(
    runs: Vec<TextRun>,
    boundaries: &[OutOfFlowBoundary],
    lines: &[TextLine],
    geometry: LineGeometry,
    fonts: &HashMap<String, TtfFont>,
) -> Vec<Point> {
    let mut places: Vec<Option<OutOfFlowPlace>> = vec![None; boundaries.len()];
    let anchored = anchored_runs(runs, boundaries);
    let wrapped = wrap_text_runs_among_floats(anchored, geometry.wrap, fonts, None, &[]);
    for (index, place) in wrapped.out_of_flow_places {
        if let Some(slot) = places.get_mut(index) {
            *slot = Some(place);
        }
    }
    boundaries
        .iter()
        .zip(places)
        .map(|(boundary, place)| {
            static_position(*boundary, place, &wrapped.lines, lines, geometry, fonts)
        })
        .collect()
}

/// `runs` with an anchor for each element after the runs collected before it, which takes its font from the run
/// before it. An element before every run is at the start of the first line, with no anchor.
fn anchored_runs(runs: Vec<TextRun>, boundaries: &[OutOfFlowBoundary]) -> Vec<TextRun> {
    let mut anchored = Vec::with_capacity(runs.len() + boundaries.len());
    let mut pending = boundaries.iter().enumerate().peekable();
    for (index, run) in runs.into_iter().enumerate() {
        while let Some((anchor, _)) = pending.next_if(|(_, boundary)| boundary.runs_before <= index)
        {
            if let Some(before) = anchored.last() {
                let anchor_run = TextRun::out_of_flow_anchor_after(before, anchor);
                anchored.push(anchor_run);
            }
        }
        anchored.push(run);
    }
    if let Some(before) = anchored.last().cloned() {
        for (anchor, _) in pending {
            anchored.push(before.out_of_flow_anchor_after(anchor));
        }
    }
    anchored
}

fn static_position(
    boundary: OutOfFlowBoundary,
    place: Option<OutOfFlowPlace>,
    wrapped: &[TextLine],
    lines: &[TextLine],
    geometry: LineGeometry,
    fonts: &HashMap<String, TtfFont>,
) -> Point {
    // No place: before every run, at the start of the first line
    let place = place.unwrap_or(OutOfFlowPlace {
        line: 0,
        advance: 0.0,
        wraps: false,
        line_has_content: false,
    });
    let top = |line: usize| -> f32 { wrapped.iter().take(line).map(|line| line.height).sum() };
    if boundary.block_level {
        let line = if place.line_has_content {
            place.line + 1
        } else {
            place.line
        };
        return Point::new(0.0, top(line));
    }
    let (line, advance) = if place.wraps {
        (place.line + 1, 0.0)
    } else {
        (place.line, place.advance)
    };
    let indent = if line == 0 { geometry.text_indent } else { 0.0 };
    let x_offset = wrapped.get(line).map_or(0.0, |line| line.x_offset);
    let align = lines.get(line).map_or(0.0, |line| {
        line_alignment_offset(line, indent, geometry, fonts)
    });
    Point::new(indent + x_offset + align + advance, top(line))
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

#[cfg(test)]
mod tests {
    use super::may_hold_out_of_flow;
    use crate::parser::css::parse_stylesheet;
    use crate::parser::html::parse_html;

    fn may_hold(css: &str, body: &str) -> bool {
        let nodes = parse_html(&format!("<html><body><p>{body}</p></body></html>")).unwrap();
        may_hold_out_of_flow(&nodes, &parse_stylesheet(css))
    }

    #[test]
    fn content_without_a_positioning_rule_or_style_holds_no_absolute_box() {
        let body = "Text <b>bold <i>italic</i></b> <span style=\"color:red\">red</span>";
        assert!(!may_hold("", body));
        assert!(!may_hold(
            "b { position: relative } i { position: STATIC } span { position: sticky; all: unset }",
            body
        ));
    }

    #[test]
    fn a_rule_or_an_inline_style_that_can_position_a_box_counts() {
        let body = "Text <b>bold <i>italic</i></b>";
        assert!(may_hold(".marker { position: absolute }", body));
        assert!(may_hold("i::after { position: fixed }", body));
        assert!(may_hold("b { position: var(--where) }", body));
        assert!(may_hold("b { all: inherit }", body));
        assert!(may_hold(
            "",
            "Text <b>bold <i style=\"Position: absolute\">x</i></b>"
        ));
    }
}
