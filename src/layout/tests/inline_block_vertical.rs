//! An inline-block is aligned in its line by its margin box (CSS 2.1 §10.8):
//! its vertical margins make the line taller, and `vertical-align` places the
//! margin box. A `top` box taller than the line makes it taller below the
//! baseline and a `bottom` box above it, so text beside a tall `bottom` box
//! moves down, as Chrome lays it out. Rows of block-tagged inline-blocks
//! (`<div style="display:inline-block">`) follow the same rules. The parity
//! fixture `inline-text-inline-block-vertical-margins` holds Chrome's output.

use super::support::layout_pages_at;
use crate::types::PageSize;

const PARENT: &str = "margin:0;font:12pt Helvetica;line-height:15pt";

/// The top of the block after `content`, which is in a paragraph of its own.
fn next_block_top(content: &str) -> f32 {
    let pages = layout_pages_at(
        &format!(r#"<div style="{PARENT}">{content}</div><div style="height:10pt"></div>"#),
        PageSize::new(300.0, 300.0),
    );
    pages[0].elements[1].0
}

/// The first line's baseline, below the line's top.
fn first_baseline(content: &str) -> f32 {
    use crate::layout::elements::{LayoutVisitor, TextBlock};
    struct First(Option<f32>);
    impl LayoutVisitor for First {
        fn visit_text_block(&mut self, block: &TextBlock) {
            if self.0.is_none() {
                self.0 = block.lines.first().and_then(|line| line.baseline_ascent);
            }
        }
    }
    let mut first = First(None);
    let pages = layout_pages_at(
        &format!(r#"<p style="{PARENT}">{content}</p>"#),
        PageSize::new(300.0, 300.0),
    );
    for (_, element) in &pages[0].elements {
        element.accept(&mut first);
    }
    first.0.expect("the line has a resolved baseline")
}

fn span(css: &str) -> String {
    format!(r#"Tx <span style="display:inline-block;width:20pt;height:30pt;{css}"></span>"#)
}

fn div(css: &str) -> String {
    format!(r#"<div style="display:inline-block;width:20pt;height:30pt;{css}"></div>"#)
}

#[test]
fn a_bottom_margin_makes_the_line_taller() {
    let without = next_block_top(&span(""));
    let with = next_block_top(&span("margin-bottom:10pt"));
    assert!(
        (with - without - 10.0).abs() < 0.01,
        "the 10pt margin adds 10pt: {without} then {with}"
    );
}

#[test]
fn a_top_margin_makes_the_line_taller() {
    let without = next_block_top(&span(""));
    let with = next_block_top(&span("margin-top:10pt"));
    assert!(
        (with - without - 10.0).abs() < 0.01,
        "{without} then {with}"
    );
}

#[test]
fn a_tall_bottom_box_moves_the_text_down_and_a_top_box_does_not() {
    let top = first_baseline(&span("vertical-align:top"));
    let bottom = first_baseline(&span("vertical-align:bottom"));
    // Both lines are 30pt tall. The `top` box leaves the baseline where the
    // 15pt strut puts it; the `bottom` box adds its extra 15pt above it.
    assert!(
        (bottom - top - 15.0).abs() < 0.01,
        "baselines {top} and {bottom}"
    );
}

#[test]
fn a_top_aligned_inline_block_div_adds_no_descent() {
    let top = next_block_top(&div("vertical-align:top"));
    assert!(
        (top - 30.0).abs() < 0.01,
        "the 30pt box hangs from the line's top: {top}"
    );
}

#[test]
fn an_inline_block_divs_margins_are_in_its_line() {
    let top = next_block_top(&div("vertical-align:top;margin-bottom:10pt"));
    assert!((top - 40.0).abs() < 0.01, "30pt and a 10pt margin: {top}");
}

#[test]
fn an_inline_block_div_with_text_aligns_its_baseline() {
    // Two boxes with one line of text each share a baseline, so the row is one
    // 15pt line plus the second box's 10pt top padding.
    let top = next_block_top(
        r#"<div style="display:inline-block">One</div><div style="display:inline-block;padding-top:10pt">Two</div>"#,
    );
    assert!((top - 25.0).abs() < 0.01, "{top}");
}
