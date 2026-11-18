//! Floats across pages (CSS 2.1 §9.5.1 rule 8, CSS Fragmentation 3 §5.1): a float that doesn't fit on its page
//! goes to the next one, and a float in a paragraph's text stays beside the line it occurs on, wherever the
//! paragraph breaks. Each expectation is what Chrome prints for the same markup. The floats are images, which
//! Chrome doesn't split across pages.

use crate::layout::elements::{LayoutVisitor, TextBlock};
use crate::layout::engine::{Page, visit_layout_tree};
use crate::layout::paginate::element_float;
use crate::style::computed::Float;
use crate::types::PageSize;

use super::support::layout_pages_at;

/// Pages 240pt by 400pt with no margins: 22 lines of 18pt fit on each.
fn pages(markup: &str) -> Vec<Page> {
    layout_pages_at(
        &format!(
            "<style>body {{ margin: 0; font: 12pt/18pt sans-serif }} p {{ margin: 0 }}</style>\
             {markup}"
        ),
        PageSize::new(240.0, 400.0),
    )
}

/// The tops of a page's floats, in order.
fn float_tops(page: &Page) -> Vec<f32> {
    page.elements
        .iter()
        .filter(|(_, element)| element_float(element.as_ref()) != Float::None)
        .map(|(top, _)| *top)
        .collect()
}

/// The top of the line on `page` whose text starts with `word`.
fn line_top(page: &Page, word: &str) -> Option<f32> {
    struct Find<'a> {
        word: &'a str,
        found: Option<f32>,
    }
    impl LayoutVisitor for Find<'_> {
        fn visit_text_block(&mut self, block: &TextBlock) {
            let mut top = 0.0;
            for line in &block.lines {
                let text: String = line.runs.iter().map(|run| run.text.as_str()).collect();
                if self.found.is_none() && text.split_whitespace().next() == Some(self.word) {
                    self.found = Some(top);
                }
                top += line.height;
            }
        }
    }
    page.elements.iter().find_map(|(top, element)| {
        let mut find = Find { word, found: None };
        visit_layout_tree(element.as_ref(), &mut find);
        // The paragraphs here have no padding or border: their lines start at their top
        find.found.map(|line| top + line)
    })
}

/// `count` lines of text, `line0` to `line{count - 1}`, with `float` after the word that starts line `at`.
fn lines_with_float(count: usize, at: usize, float: &str) -> String {
    (0..count)
        .map(|index| {
            if index == at {
                format!("line{index} {float} tail")
            } else {
                format!("line{index}")
            }
        })
        .collect::<Vec<_>>()
        .join("<br>")
}

fn assert_near(actual: f32, expected: f32, what: &str) {
    assert!(
        (actual - expected).abs() < 0.01,
        "{what}: expected {expected}, got {actual}"
    );
}

const PIXEL: &str = "data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mP8/5+hHgAHggJ/PchI7wAAAABJRU5ErkJggg==";

/// A right float `height` tall.
fn image(height: f32) -> String {
    format!(r#"<img style="float:right;width:50pt;height:{height}pt" src="{PIXEL}">"#)
}

#[test]
fn a_float_grid_carries_on_over_the_next_page() {
    let cells =
        format!(r#"<img style="float:left;width:100pt;height:100pt" src="{PIXEL}">"#).repeat(12);
    let pages = pages(&cells);
    assert_eq!(pages.len(), 2, "six rows of two, four rows to a page");
    assert_eq!(
        float_tops(&pages[0]),
        [0.0, 0.0, 100.0, 100.0, 200.0, 200.0, 300.0, 300.0]
    );
    assert_eq!(float_tops(&pages[1]), [0.0, 0.0, 100.0, 100.0]);
}

#[test]
fn a_float_in_text_after_the_break_goes_beside_its_line_on_the_next_page() {
    let pages = pages(&format!(
        "<p>{}</p>",
        lines_with_float(30, 24, &image(12.0))
    ));
    assert_eq!(pages.len(), 2, "the paragraph breaks once");
    assert!(float_tops(&pages[0]).is_empty());
    assert_eq!(line_top(&pages[0], "line21"), Some(378.0), "page 1 is full");
    let line = line_top(&pages[1], "line24").expect("line 24 is on page 2");
    assert_near(line, 36.0, "the third line of the continuation");
    assert_eq!(
        float_tops(&pages[1]),
        [line],
        "the float is beside its line"
    );
}

#[test]
fn a_float_in_text_on_a_paragraph_with_a_top_margin_starts_at_its_line() {
    let pages = pages(&format!(
        r#"<div style="height:10pt;margin-bottom:20pt"></div>
        <p style="margin-top:30pt">{}</p>"#,
        lines_with_float(3, 1, &image(12.0))
    ));
    let line = line_top(&pages[0], "line1").expect("line 1");
    assert_near(
        line,
        58.0,
        "the margins collapse to 30pt below the 10pt box",
    );
    assert_eq!(float_tops(&pages[0]), [line]);
}

#[test]
fn a_paragraph_that_cannot_break_takes_its_floats_to_the_next_page() {
    let pages = pages(&format!(
        r#"<div style="height:300pt"></div><p style="break-inside:avoid">{}</p>"#,
        lines_with_float(7, 3, &image(12.0))
    ));
    assert_eq!(pages.len(), 2);
    assert!(float_tops(&pages[0]).is_empty());
    let line = line_top(&pages[1], "line3").expect("the paragraph moved to page 2");
    assert_near(line, 54.0, "the fourth line");
    assert_eq!(float_tops(&pages[1]), [line]);
}

#[test]
fn a_float_that_does_not_fit_beside_its_line_starts_the_next_page() {
    // 42pt of room is left below the top of the line the float occurs on; the float is 50pt tall
    let pages = pages(&format!(
        r#"<div style="height:340pt"></div><p>{}</p>"#,
        lines_with_float(2, 1, &image(50.0))
    ));
    assert_eq!(pages.len(), 2);
    assert_eq!(
        line_top(&pages[0], "line1"),
        Some(358.0),
        "the text stays on page 1"
    );
    assert!(float_tops(&pages[0]).is_empty());
    assert_eq!(float_tops(&pages[1]), [0.0], "the float tops page 2");
}
