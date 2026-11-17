//! Floats in a block formatting context (CSS 2.1 §9.5): the line boxes beside
//! them, floats inside a paragraph's text, their margins, and the boxes that
//! start formatting contexts of their own beside them. Each expectation is
//! what Chrome lays out for the same markup.

use crate::layout::elements::{FlexRow, LayoutElement, LayoutVisitor, TableRow, TextBlock};
use crate::layout::engine::{LineInsets, Page, TextLine, visit_layout_tree};
use crate::layout::paginate::element_float;
use crate::style::computed::Float;
use crate::types::PageSize;

use super::support::layout_pages_at;

const PIXEL: &str = "data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mP8/5+hHgAHggJ/PchI7wAAAABJRU5ErkJggg==";

fn page(markup: &str) -> Page {
    layout_pages_at(
        &format!(
            "<style>body {{ margin: 0; font: 12pt/18pt sans-serif }} p {{ margin: 0 }}</style>\
             {markup}"
        ),
        PageSize::new(240.0, 400.0),
    )
    .into_iter()
    .next()
    .expect("one page")
}

/// One line box: its words and the room floats take beside it.
struct Line {
    words: String,
    insets: LineInsets,
}

/// Every text block's lines, in tree order, each block's lines together.
#[derive(Default)]
struct Blocks(Vec<Vec<Line>>);

impl Blocks {
    fn push(&mut self, lines: &[TextLine]) {
        let lines: Vec<Line> = lines
            .iter()
            .map(|line| Line {
                words: line
                    .runs
                    .iter()
                    .map(|run| run.text.as_str())
                    .collect::<String>()
                    .split_whitespace()
                    .collect::<Vec<_>>()
                    .join(" "),
                insets: line.float_insets,
            })
            .filter(|line| !line.words.is_empty())
            .collect();
        if !lines.is_empty() {
            self.0.push(lines);
        }
    }

    /// The line holding `word`.
    fn line_with(&self, word: &str) -> &Line {
        self.0
            .iter()
            .flatten()
            .find(|line| line.words.split(' ').any(|candidate| candidate == word))
            .unwrap_or_else(|| panic!("no line holds {word:?}"))
    }

    /// The block holding `word`.
    fn block_with(&self, word: &str) -> &[Line] {
        self.0
            .iter()
            .find(|lines| {
                lines
                    .iter()
                    .any(|line| line.words.split(' ').any(|candidate| candidate == word))
            })
            .unwrap_or_else(|| panic!("no block holds {word:?}"))
    }
}

impl LayoutVisitor for Blocks {
    fn visit_text_block(&mut self, block: &TextBlock) {
        self.push(&block.lines);
    }

    fn visit_flex_row(&mut self, row: &FlexRow) {
        for cell in &row.content.cells {
            self.push(&cell.lines);
        }
    }

    fn visit_table_row(&mut self, row: &TableRow) {
        for cell in &row.content.cells {
            self.push(&cell.layout.content.lines);
        }
    }
}

fn blocks(page: &Page) -> Blocks {
    let mut blocks = Blocks::default();
    for (_, element) in &page.elements {
        visit_layout_tree(element.as_ref(), &mut blocks);
    }
    blocks
}

/// The page's top-level floats, with their tops.
fn floats(page: &Page) -> Vec<(f32, &dyn LayoutElement)> {
    page.elements
        .iter()
        .filter(|(_, element)| element_float(element.as_ref()) != Float::None)
        .map(|(top, element)| (*top, element.as_ref()))
        .collect()
}

/// The page's top-level in-flow boxes that hold `word`'s text, or the first
/// in-flow box after the floats.
fn in_flow_after_floats(page: &Page) -> (f32, &dyn LayoutElement) {
    page.elements
        .iter()
        .find(|(_, element)| {
            element.contributes_to_normal_flow() && element_float(element.as_ref()) == Float::None
        })
        .map(|(top, element)| (*top, element.as_ref()))
        .expect("an in-flow box")
}

fn line_offset(float: &dyn LayoutElement) -> f32 {
    float
        .block_flow_owner()
        .map(|owner| owner.block_flow().float_line_offset)
        .expect("a float has block flow")
}

fn inset_left(element: &dyn LayoutElement) -> f32 {
    element
        .positioning_owner()
        .map(|owner| owner.positioning().insets.left)
        .expect("a positioned box")
}

fn assert_near(actual: f32, expected: f32, what: &str) {
    assert!(
        (actual - expected).abs() < 0.01,
        "{what}: expected {expected}, got {actual}"
    );
}

#[test]
fn a_float_in_a_paragraphs_text_floats_from_its_line_and_keeps_the_text_one_paragraph() {
    let page = page(
        r#"<p>Total due <span style="float:right;width:60pt;height:12pt"></span> thanks for
        your business, it was a pleasure working with you.</p>"#,
    );
    let blocks = blocks(&page);
    let paragraph = blocks.block_with("Total");
    assert!(
        paragraph
            .iter()
            .any(|line| line.words.split(' ').any(|word| word == "thanks")),
        "the text after the float stays in the paragraph"
    );
    assert_near(
        paragraph[0].insets.right,
        60.0,
        "the first line ends at the float",
    );
    let floats = floats(&page);
    assert_eq!(floats.len(), 1);
    assert_near(
        line_offset(floats[0].1),
        0.0,
        "the float is on the first line",
    );
}

#[test]
fn a_float_that_does_not_fit_beside_its_line_floats_from_the_next_one() {
    let page = page(
        r#"<p>These words fill most of the first line <span
        style="float:right;width:120pt;height:12pt"></span> and the text carries on below it.</p>"#,
    );
    let blocks = blocks(&page);
    let paragraph = blocks.block_with("These");
    assert_near(paragraph[0].insets.right, 0.0, "the first line is whole");
    assert_near(
        paragraph[1].insets.right,
        120.0,
        "the second line ends at the float",
    );
    let floats = floats(&page);
    assert_near(
        line_offset(floats[0].1),
        18.0,
        "the float is on the second line",
    );
}

#[test]
fn a_float_in_a_decorated_paragraphs_text_floats_from_its_line() {
    let page = page(
        r#"<p style="padding:4pt;background:#eee">Total <span
        style="float:right;width:50pt;height:12pt"></span> due today, and enough words to wrap
        onto a second line.</p>"#,
    );
    let blocks = blocks(&page);
    let line = blocks.line_with("Total");
    assert!(
        line.words.contains("due"),
        "the text carries on beside the float: {:?}",
        line.words
    );
    assert_near(line.insets.right, 50.0, "the line ends at the float");
}

#[test]
fn a_floated_image_shortens_the_lines_beside_it() {
    let page = page(&format!(
        r#"<p><img style="float:left;width:40pt;height:30pt" src="{PIXEL}">A logo floated at the
        start of the paragraph, with words that wrap beside it and then below it.</p>"#
    ));
    assert_near(
        blocks(&page).line_with("logo").insets.left,
        40.0,
        "the line starts after the image",
    );
}

#[test]
fn float_margins_do_not_collapse_and_the_margins_around_a_float_still_do() {
    let page = page(
        r#"<p style="margin:0 0 10pt">First</p><div
        style="float:left;width:30pt;height:15pt;margin-top:4pt"></div><p
        style="margin:20pt 0 0">Second</p>"#,
    );
    let floats = floats(&page);
    assert_near(
        floats[0].0,
        32.0,
        "the float is below First's margin and its own",
    );
    let second = page
        .elements
        .iter()
        .filter(|(_, element)| element_float(element.as_ref()) == Float::None)
        .map(|(top, _)| *top)
        .nth(1)
        .expect("the second paragraph");
    assert_near(second, 38.0, "the paragraphs' margins collapse to 20pt");
}

#[test]
fn a_table_too_wide_to_fit_beside_a_float_goes_below_it() {
    let page = page(
        r#"<div style="float:left;width:60pt;height:30pt"></div><table
        style="width:200pt"><tr><td>Cell</td></tr></table>"#,
    );
    let (top, table) = in_flow_after_floats(&page);
    assert_near(top, 30.0, "the table starts below the float");
    assert_near(inset_left(table), 0.0, "the table starts at the left edge");
}

#[test]
fn a_table_that_fits_beside_a_float_goes_beside_it() {
    let page = page(
        r#"<div style="float:left;width:60pt;height:30pt"></div><table
        style="width:150pt"><tr><td>Cell</td></tr></table>"#,
    );
    let (top, table) = in_flow_after_floats(&page);
    assert_near(top, 0.0, "the table starts at the float's top");
    assert_near(inset_left(table), 60.0, "the table starts after the float");
}

#[test]
fn a_flex_container_without_a_width_takes_the_space_beside_a_float() {
    let page = page(
        r#"<div style="float:left;width:60pt;height:30pt"></div><div
        style="display:flex"><div>One</div></div>"#,
    );
    let (top, flex) = in_flow_after_floats(&page);
    assert_near(top, 0.0, "the flex container starts at the float's top");
    assert_near(
        inset_left(flex),
        60.0,
        "the flex container starts after the float",
    );
    let (_, width) = crate::layout::block::child_x_and_width(flex);
    assert_near(
        width,
        180.0,
        "the flex container fills the rest of the line",
    );
}

#[test]
fn list_items_lines_go_beside_a_float() {
    let page = page(
        r#"<div style="float:left;width:60pt;height:60pt"></div><ul
        style="margin:0;padding-left:30pt"><li>First</li><li>Second</li></ul>"#,
    );
    let blocks = blocks(&page);
    assert_near(
        blocks.line_with("First").insets.left,
        30.0,
        "the item's text starts after the float",
    );
    assert_near(
        blocks.line_with("Second").insets.left,
        30.0,
        "so does the next item's",
    );
}

#[test]
fn a_table_cells_text_wraps_beside_its_own_float() {
    let page = page(
        r#"<table style="width:200pt;border-collapse:collapse"><tr><td style="padding:0"><div
        style="float:left;width:40pt;height:30pt"></div>Text in a cell</td></tr></table>"#,
    );
    assert_near(
        blocks(&page).line_with("Text").insets.left,
        40.0,
        "the cell's text starts after the float",
    );
}

#[test]
fn a_flex_items_text_wraps_beside_its_own_float() {
    let page = page(
        r#"<div style="display:flex"><div style="width:200pt"><div
        style="float:left;width:50pt;height:30pt"></div>Text in a flex item</div></div>"#,
    );
    assert_near(
        blocks(&page).line_with("Text").insets.left,
        50.0,
        "the item's text starts after the float",
    );
}
