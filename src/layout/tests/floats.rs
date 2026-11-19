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
    /// How wide its text is
    width: f32,
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
                width: crate::layout::helpers::measure_runs_width(
                    &line.runs,
                    &std::collections::HashMap::new(),
                ),
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
        .and_then(|owner| owner.block_flow().float_line)
        .expect("a float in text has its line")
        .line_top
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

/// A paragraph with `clear: both` after a left float, holding a right float in its own text after `text`.
fn cleared_paragraph(text: &str) -> Page {
    page(&format!(
        r#"<div style="float:left;width:100pt;height:30pt"></div>
        <p style="clear:both">{text} <span style="float:right;width:50pt;height:12pt"></span> due today</p>"#
    ))
}

#[test]
fn a_paragraph_clears_earlier_floats_but_not_the_floats_in_its_own_text() {
    for (text, path) in [
        ("Total", "one text block"),
        ("Invoice<br>Total", "a forced break"),
    ] {
        let page = cleared_paragraph(text);
        let (top, _) = in_flow_after_floats(&page);
        assert_near(
            top,
            30.0,
            &format!("{path}: the paragraph starts below the left float"),
        );
        let line = blocks(&page).line_with("Total").insets;
        assert_near(
            line.left,
            0.0,
            &format!("{path}: the left float is above the paragraph"),
        );
        assert_near(
            line.right,
            50.0,
            &format!("{path}: the line ends at its own float"),
        );
        let tops: Vec<f32> = floats(&page).iter().map(|(top, _)| *top).collect();
        let line_top = if text.contains("<br>") { 48.0 } else { 30.0 };
        assert_eq!(tops, [0.0, line_top], "{path}: the float sits on its line");
    }
}

fn border_box_width(element: &dyn LayoutElement) -> f32 {
    crate::layout::block::child_x_and_width(element).1
}

#[test]
fn a_float_without_a_width_is_as_wide_as_its_widest_block() {
    let page = page(&format!(
        r#"<div style="float:right"><div><img style="width:80pt;height:10pt" src="{PIXEL}"></div>
        <div><img style="width:60pt;height:10pt" src="{PIXEL}"></div></div>
        <p>Bill to a customer whose name and address run over several lines.</p>"#
    ));
    let floats = floats(&page);
    assert_near(border_box_width(floats[0].1), 80.0, "shrink-to-fit width");
    assert_near(
        blocks(&page).line_with("Bill").insets.right,
        80.0,
        "the line beside it ends at it",
    );
}

#[test]
fn shrink_to_fit_text_wraps_at_the_width_available() {
    // A box whose text wraps is as wide as the width available (CSS 2.1 §10.3.5: min(max(min-content, available),
    // max-content)), not its longest line
    for (style, what, width) in [
        ("float:left", "a float", 240.0),
        (
            "float:left;margin-right:6pt",
            "a float with a margin",
            234.0,
        ),
        ("position:absolute", "an absolute box", 240.0),
    ] {
        let page = page(&format!(
            r#"<div style="{style}">Twelve words of text that cannot all fit on one line of this page.</div>"#
        ));
        let lines = blocks(&page).block_with("Twelve").len();
        assert!(lines > 1, "{what}'s text wraps: {lines} line");
        let (_, element) = page
            .elements
            .iter()
            .find(|(_, element)| {
                let mut found = Blocks::default();
                visit_layout_tree(element.as_ref(), &mut found);
                !found.0.is_empty()
            })
            .expect("the box");
        assert_near(border_box_width(element.as_ref()), width, what);
    }
}

#[test]
fn a_letter_spaced_float_is_as_wide_as_its_tracked_text() {
    // Chrome tracks after every letter, the space and the last letter too: "Total due" at 2px letter spacing and
    // 4px word spacing is nine letter spacings and one word spacing wider than without. A run split between its
    // words lost one, and the last letter's was never counted
    let width = |style: &str| {
        let page = page(&format!(
            r#"<div style="float:left;{style}">Total due</div><p>x</p>"#
        ));
        border_box_width(floats(&page)[0].1)
    };
    let plain = width("");
    assert_near(
        width("letter-spacing:2px;word-spacing:4px"),
        plain + 9.0 * 1.5 + 3.0,
        "tracked width",
    );
}

#[test]
fn a_letter_spaced_float_keeps_its_text_on_one_line() {
    let page = page(
        r#"<div style="float:left;letter-spacing:4pt;word-spacing:6pt">Total due</div><p>x</p>"#,
    );
    assert_eq!(blocks(&page).block_with("Total").len(), 1);
}

#[test]
fn no_line_runs_under_a_float_in_its_text_however_much_text_comes_before_it() {
    // An A4 page with 72pt margins and 14px text, the float's width varying with the text before it: the pre-review
    // build drew the float over the text on its line in most of these
    let words = "Alpha be gamma delta epsilon zeta eta theta iota kappa lambda mu nu xi omicron pi rho sigma tau \
        upsilon phi chi psi omega"
        .split_whitespace()
        .collect::<Vec<_>>();
    let content_width = crate::types::PageSize::A4.width - 144.0;
    for side in ["left", "right"] {
        for count in (1..120).step_by(3) {
            let before: Vec<String> = (0..count)
                .map(|index| {
                    let word = words[index % words.len()];
                    if index % 7 == 6 {
                        format!("{word}.")
                    } else {
                        word.to_string()
                    }
                })
                .collect();
            let width = 60 + (count * 13) % 90;
            let pages = super::support::layout_pages_at_with_fonts(
                &format!(
                    r#"<style>body {{ margin: 0; font: 14px sans-serif }}</style>
                    <p>{} <span style="float:{side};width:{width}px;height:30px"></span> After words continue for
                    a while so the paragraph has more lines after the float here.</p>"#,
                    before.join(" ")
                ),
                crate::types::PageSize::A4,
                crate::types::Margin::uniform(72.0),
                &std::collections::HashMap::new(),
            );
            for line in blocks(&pages[0]).block_with("Alpha") {
                assert!(
                    line.width <= content_width - line.insets.horizontal() + 0.01,
                    "{side} float after {count} words: {:?} ({} wide) runs under it ({:?})",
                    line.words,
                    line.width,
                    line.insets
                );
            }
        }
    }
}

#[test]
fn a_float_is_as_wide_as_its_text_snapped_up_to_a_layout_unit() {
    // Chromium sizes a box from its text's width rounded up to 1/64px (`ShapeResult::SnappedWidth`) and draws the
    // glyphs at their exact positions, so a table column or float is a little wider than its text: a cell beside it
    // starts at a whole number of layout units
    let page = page(r#"<div style="float:left">Qty 2</div><p>x</p>"#);
    let width_px = border_box_width(floats(&page)[0].1) / crate::fonts::PT_PER_CSS_PX;
    let units = width_px * 64.0;
    assert!(
        (units - units.round()).abs() < 1e-3,
        "{width_px}px is not a whole number of layout units"
    );
}
