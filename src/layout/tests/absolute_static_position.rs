//! An absolutely positioned box whose insets are both `auto` on an axis sits
//! at its static position on that axis: where it would have been in normal
//! flow (CSS 2.1 §10.3.7, §10.6.4). In a line of text that's where it falls
//! among the words, whether it's a child of the paragraph or nested in one of
//! its inline elements. A box that was block-level before it was positioned
//! starts on a line of its own. The parity fixture
//! `positioning-absolute-static-position` holds Chrome's output.

use crate::layout::elements::{LayoutElement, Positioning};
use crate::style::computed::FontFamily;
use crate::types::Point;

use super::support::layout_pages;

/// Every absolutely positioned box's positioning, in paint order.
fn absolute_boxes(markup: &str) -> Vec<Positioning> {
    fn walk(element: &dyn LayoutElement, found: &mut Vec<Positioning>) {
        if let Some(owner) = element.positioning_owner()
            && owner.positioning().scheme.is_absolute()
        {
            found.push(owner.positioning().clone());
        }
        element.visit_children(&mut |child| walk(child, found));
    }
    let mut found = Vec::new();
    for page in layout_pages(markup) {
        for (_, element) in &page.elements {
            walk(element.as_ref(), &mut found);
        }
    }
    found
}

fn width(text: &str) -> f32 {
    crate::fonts::str_width(text, 12.0, &FontFamily::Helvetica, false)
}

fn static_position(markup: &str) -> Point {
    let boxes = absolute_boxes(markup);
    assert_eq!(boxes.len(), 1, "one absolute box is laid out");
    boxes[0]
        .static_position
        .expect("the box records its static position in the line")
}

const PARAGRAPH: &str = "position:relative;margin:0;font:12pt Helvetica;line-height:15pt";

#[test]
fn a_box_in_a_line_sits_after_the_words_before_it() {
    let position = static_position(&format!(
        r#"<p style="{PARAGRAPH}">Alpha beta <i style="position:absolute;width:10pt;height:6pt"></i> gamma</p>"#
    ));
    assert!(
        (position.x - width("Alpha beta ")).abs() < 0.01,
        "x {} is the advance of the words and the space before the box ({})",
        position.x,
        width("Alpha beta ")
    );
    assert_eq!(position.y, 0.0, "on the first line");
}

#[test]
fn a_box_in_a_span_is_laid_out_where_it_falls_in_the_line() {
    let position = static_position(&format!(
        r#"<p style="{PARAGRAPH}">Alpha <span>beta <i style="position:absolute;width:10pt;height:6pt"></i> gamma</span></p>"#
    ));
    assert!(
        (position.x - width("Alpha beta ")).abs() < 0.01,
        "x {} is after the words before it ({})",
        position.x,
        width("Alpha beta ")
    );
}

#[test]
fn a_box_on_a_later_line_sits_below_the_lines_before_it() {
    let position = static_position(&format!(
        r#"<p style="{PARAGRAPH};width:60pt">Alpha beta gamma <i style="position:absolute;width:10pt;height:6pt"></i> delta</p>"#
    ));
    assert_eq!(position.y, 15.0, "on the second 15pt line");
}

#[test]
fn a_block_level_box_starts_below_its_line() {
    let position = static_position(&format!(
        r#"<p style="{PARAGRAPH}">Alpha beta <i style="position:absolute;display:block;width:10pt;height:6pt"></i> gamma</p>"#
    ));
    assert_eq!(
        position,
        Point::new(0.0, 15.0),
        "a block-level box starts a line of its own at the start edge"
    );
}

#[test]
fn a_block_level_box_before_any_words_starts_on_the_first_line() {
    let position = static_position(&format!(
        r#"<p style="{PARAGRAPH}"><i style="position:absolute;display:block;width:10pt;height:6pt"></i>Alpha beta</p>"#
    ));
    assert_eq!(position, Point::new(0.0, 0.0));
}

#[test]
fn a_flex_items_static_position_is_applied_once() {
    let boxes = absolute_boxes(
        r#"<div style="position:relative;display:flex;justify-content:center;width:200pt;height:100pt"><div style="position:absolute;align-self:flex-end;width:40pt;height:30pt"></div></div>"#,
    );
    assert_eq!(boxes.len(), 1);
    let positioning = &boxes[0];
    assert_eq!(
        (positioning.insets.left, positioning.insets.top),
        (80.0, 70.0),
        "centred, and at the end of the cross axis"
    );
    assert!(
        !positioning.has_static_inline_position() && !positioning.has_static_block_position(),
        "the flex container's static position is final"
    );
}
