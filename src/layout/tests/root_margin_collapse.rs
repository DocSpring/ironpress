//! The first block's top margin collapses with the body's top margin only
//! when no top padding or border separates them (CSS 2.1 §8.3.1). Chrome
//! puts a paragraph with a 12pt margin 12pt lower in a padded body, and at
//! the larger of the two margins in an unpadded one.

use super::support::layout_pages_at;
use crate::types::PageSize;

fn first_text_top(body: &str, paragraph_margin: &str) -> f32 {
    let pages = layout_pages_at(
        &format!(
            "<style>body {{ {body} }} p {{ margin: {paragraph_margin} 0 0 }}</style><p>Text</p>"
        ),
        PageSize::new(200.0, 200.0),
    );
    pages[0]
        .elements
        .iter()
        .filter(|(_, element)| element.contributes_to_normal_flow())
        .map(|(top, _)| *top)
        .reduce(f32::min)
        .expect("the paragraph is laid out")
}

fn margin_shift(body: &str) -> f32 {
    first_text_top(body, "12pt") - first_text_top(body, "0")
}

#[test]
fn body_padding_keeps_the_first_margin_whole() {
    let shift = margin_shift("margin:0; padding:10pt 0 0");
    assert!(
        (shift - 12.0).abs() < 0.01,
        "the paragraph moves its whole 12pt margin, got {shift}"
    );
}

#[test]
fn a_body_border_keeps_the_first_margin_whole() {
    let shift = margin_shift("margin:0; border-top:3pt solid black");
    assert!(
        (shift - 12.0).abs() < 0.01,
        "the paragraph moves its whole 12pt margin, got {shift}"
    );
}

#[test]
fn an_unpadded_body_collapses_its_margin_with_the_first() {
    let shift = margin_shift("margin:8pt; padding:0");
    assert!(
        (shift - 4.0).abs() < 0.01,
        "the margins collapse to 12pt, 4pt past body's 8pt, got {shift}"
    );
}
