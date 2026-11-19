//! Floats across pages (CSS 2.1 §9.5.1 rule 8, CSS Fragmentation 3 §5.1): a float that doesn't fit on its page
//! goes to the next one, and a float in a paragraph's text stays beside the line it occurs on, wherever the
//! paragraph breaks. A float that can break (text, a box with a height) is split at the page end instead, and
//! carries on at the top of the next page. Each expectation is what Chrome prints for the same markup. Images
//! and `break-inside: avoid` floats aren't split.

use crate::layout::elements::{LayoutVisitor, TextBlock};
use crate::layout::engine::{Page, visit_layout_tree};
use crate::layout::paginate::element_float;
use crate::layout::paginate::estimate_element_height;
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

/// The floats on a page, with their tops and heights.
fn float_boxes(page: &Page) -> Vec<(f32, f32)> {
    page.elements
        .iter()
        .filter(|(_, element)| element_float(element.as_ref()) != Float::None)
        .map(|(top, element)| (*top, estimate_element_height(element.as_ref())))
        .collect()
}

/// `count` lines `{word}1` to `{word}{count}`.
fn numbered(word: &str, count: usize) -> String {
    (1..=count)
        .map(|index| format!("{word}{index}"))
        .collect::<Vec<_>>()
        .join("<br>")
}

/// Words enough for `count` lines beside a 100pt float on a 240pt page.
fn words(count: usize) -> String {
    (1..=count * 2)
        .map(|index| format!("w{index}"))
        .collect::<Vec<_>>()
        .join(" ")
}

#[test]
fn a_float_with_text_is_split_at_the_page_end_and_carries_on_on_the_next_page() {
    // 100pt are left below the 300pt box: five of the float's ten 18pt lines
    let pages = pages(&format!(
        r#"<div style="height:300pt"></div>
        <div style="float:left;width:100pt">{}</div><p>{}</p>"#,
        numbered("f", 10),
        words(5)
    ));
    assert_eq!(pages.len(), 2);
    assert_eq!(
        float_boxes(&pages[0]),
        [(300.0, 100.0)],
        "the first part fills the page"
    );
    assert_eq!(line_top(&pages[0], "f5"), Some(372.0));
    assert_eq!(
        line_top(&pages[0], "w1"),
        Some(300.0),
        "the text beside it stays"
    );
    assert_eq!(float_tops(&pages[1]), [0.0], "the rest tops page 2");
    assert_eq!(line_top(&pages[1], "f6"), Some(0.0));
    assert_eq!(line_top(&pages[1], "f10"), Some(72.0));
}

#[test]
fn a_float_with_a_height_is_split_at_the_page_end() {
    let pages = pages(
        r#"<div style="height:300pt"></div>
        <div style="float:left;width:100pt;height:200pt;border:3pt solid red"></div><p>text</p>"#,
    );
    assert_eq!(pages.len(), 2);
    assert_eq!(float_boxes(&pages[0]), [(300.0, 100.0)]);
    assert_eq!(
        float_boxes(&pages[1]),
        [(0.0, 106.0)],
        "the rest of its 206pt"
    );
}

#[test]
fn a_float_that_avoids_breaks_moves_to_the_next_page_whole() {
    let pages = pages(&format!(
        r#"<div style="height:300pt"></div>
        <div style="float:left;width:100pt;break-inside:avoid">{}</div><p>text</p>"#,
        numbered("f", 10)
    ));
    assert!(float_tops(&pages[0]).is_empty());
    assert_eq!(float_boxes(&pages[1]), [(0.0, 180.0)]);
    assert_eq!(line_top(&pages[1], "f1"), Some(0.0));
}

#[test]
fn a_float_taller_than_a_page_carries_on_over_several() {
    let pages = pages(&format!(
        r#"<div style="float:left;width:100pt">{}</div>"#,
        numbered("f", 50)
    ));
    assert_eq!(pages.len(), 3, "22 lines to a page");
    assert_eq!(line_top(&pages[1], "f23"), Some(0.0));
    assert_eq!(line_top(&pages[2], "f45"), Some(0.0));
    assert_eq!(line_top(&pages[2], "f50"), Some(90.0));
}

#[test]
fn a_float_with_text_in_a_paragraph_is_split_beside_its_line() {
    // Line 1 of the paragraph is at 342pt, with 58pt left: three of the float's eight lines
    let float = format!(
        r#"<span style="float:right;width:60pt">{}</span>"#,
        numbered("f", 8)
    );
    let pages = pages(&format!(
        r#"<div style="height:324pt"></div><p>{}</p>"#,
        lines_with_float(8, 1, &float)
    ));
    assert_eq!(pages.len(), 2);
    assert_eq!(line_top(&pages[0], "line1"), Some(342.0));
    assert_eq!(float_boxes(&pages[0]), [(342.0, 58.0)]);
    assert_eq!(line_top(&pages[0], "f3"), Some(378.0));
    assert_eq!(float_tops(&pages[1]), [0.0]);
    assert_eq!(line_top(&pages[1], "f4"), Some(0.0));
    assert_eq!(
        line_top(&pages[1], "line4"),
        Some(0.0),
        "the paragraph carries on beside it"
    );
}

#[test]
fn floats_after_a_split_float_stack_below_its_rest() {
    // Two columns of 108pt cards (six lines): the fourth row starts at 324pt and breaks at the page end, keeping
    // four lines (the part fills the page) and carrying two
    let card = format!(
        r#"<div style="float:left;width:110pt">{}</div>"#,
        numbered("c", 6)
    );
    let pages = pages(&card.repeat(10));
    assert_eq!(pages.len(), 2);
    assert_eq!(
        float_boxes(&pages[0]),
        [
            (0.0, 108.0),
            (0.0, 108.0),
            (108.0, 108.0),
            (108.0, 108.0),
            (216.0, 108.0),
            (216.0, 108.0),
            (324.0, 76.0),
            (324.0, 76.0)
        ]
    );
    assert_eq!(
        float_boxes(&pages[1]),
        [(0.0, 36.0), (0.0, 36.0), (36.0, 108.0), (36.0, 108.0)],
        "the fourth row's rests, then the fifth row below them"
    );
}

#[test]
fn a_box_that_clears_a_float_moved_to_the_next_page_goes_below_its_bottom_margin() {
    let pages = pages(
        r#"<div style="height:350pt"></div>
        <div style="float:left;width:100pt;height:100pt;margin-bottom:6pt;break-inside:avoid"></div>
        <p style="clear:both">after</p>"#,
    );
    assert_eq!(pages.len(), 2);
    assert_eq!(float_tops(&pages[1]), [0.0]);
    assert_eq!(line_top(&pages[1], "after"), Some(106.0));
}

#[test]
fn a_box_that_clears_a_split_float_goes_below_its_rest() {
    let pages = pages(&format!(
        r#"<div style="height:300pt"></div>
        <div style="float:left;width:100pt">{}</div><p style="clear:both">after</p>"#,
        numbered("f", 10)
    ));
    assert_eq!(pages.len(), 2);
    assert_eq!(float_boxes(&pages[1]), [(0.0, 90.0)]);
    assert_eq!(line_top(&pages[1], "after"), Some(90.0));
}
