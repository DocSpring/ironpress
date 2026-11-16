//! An inline `<svg>` is a replaced element, laid out in its line like an
//! image (HTML §15.4.1, CSS 2.1 §10.3.2). The parity fixtures
//! `inline-text-svg-*` hold Chrome's output.

use crate::layout::elements::{FlexRow, LayoutVisitor, Svg, TextBlock};
use crate::layout::engine::visit_layout_tree;

use super::support::layout_pages;

#[derive(Default)]
struct Found {
    /// SVGs drawn inside a line's inline box: (box width, box height)
    inline_boxes: Vec<(f32, f32)>,
    /// SVGs laid out as their own replaced box
    replaced: usize,
    /// Text blocks of their own holding the paragraph's words
    texts: Vec<String>,
    /// Atomic cells in a line's row
    row_cells: usize,
    /// The inline background behind each inline box in a line
    inline_box_backgrounds: Vec<Option<crate::types::Color>>,
}

impl LayoutVisitor for Found {
    fn visit_text_block(&mut self, block: &TextBlock) {
        for run in block.lines.iter().flat_map(|line| &line.runs) {
            if let Some(inline) = run.inline_box.as_deref()
                && inline.svg.is_some()
            {
                self.inline_boxes.push((inline.width, inline.height));
            }
            if run.inline_box.is_some() {
                self.inline_box_backgrounds.push(run.background_color);
            }
        }
        let text: String = block
            .lines
            .iter()
            .flat_map(|line| &line.runs)
            .map(|run| run.text.as_str())
            .collect();
        if !text.trim().is_empty() {
            self.texts.push(text.trim().to_owned());
        }
    }

    fn visit_flex_row(&mut self, row: &FlexRow) {
        self.row_cells += row.content.cells.len();
    }

    fn visit_svg(&mut self, _svg: &Svg) {
        self.replaced += 1;
    }
}

fn found(markup: &str) -> Found {
    let mut found = Found::default();
    for page in layout_pages(markup) {
        for (_, element) in &page.elements {
            visit_layout_tree(element.as_ref(), &mut found);
        }
    }
    found
}

#[test]
fn an_svg_in_a_span_is_drawn_in_its_line() {
    let found = found(
        r#"<p>Before <span>in <svg width="24" height="16"><rect width="24" height="16"/></svg> span</span> after</p>"#,
    );
    assert_eq!(
        found.inline_boxes,
        vec![(18.0, 12.0)],
        "one 24x16px box in the line"
    );
}

#[test]
fn an_svg_between_words_stays_in_their_line() {
    let found = found(
        r#"<p style="width:400px">Before <svg width="24" height="16"><rect width="24" height="16"/></svg> after</p>"#,
    );
    assert_eq!(found.replaced, 1, "the SVG is drawn once");
    assert_eq!(
        (found.row_cells, found.texts.len()),
        (3, 0),
        "\"Before\", the SVG and \"after\" are three items of one line, not three blocks: {:?}",
        found.texts
    );
}

/// An `<img>` showing an SVG document is a replaced box like an inline
/// `<svg>`; inside a span it used to be left out of the line.
#[test]
fn an_svg_image_in_a_span_is_drawn_in_its_line() {
    let found = found(
        r#"<p>Before <span>in <img style="width:24px;height:16px" src="data:image/svg+xml,%3Csvg xmlns=%22http://www.w3.org/2000/svg%22 width=%221%22 height=%221%22/%3E"> span</span> after</p>"#,
    );
    assert_eq!(
        found.inline_boxes,
        vec![(18.0, 12.0)],
        "one 24x16px box in the line"
    );
}

/// A span's background covers its content area behind a replaced box in it,
/// as Chrome paints it (CSS 2.1 §14.2: the inline box's background).
#[test]
fn a_spans_background_lies_behind_its_replaced_box() {
    let found = found(
        r#"<p>Before <span style="background:#99ccff">in <svg width="24" height="16"><rect width="24" height="16"/></svg> span</span> after</p>"#,
    );
    assert_eq!(
        found.inline_box_backgrounds,
        vec![Some(crate::types::Color::rgb(0x99, 0xcc, 0xff))],
        "the svg's run carries the span's background"
    );
}
