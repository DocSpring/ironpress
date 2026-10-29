//! Blocks in an auto-sized table cell: the paragraphs, divs and lists in a
//! cell size its column (CSS 2.1 §17.5.2.2) and keep their margins inside it.

use super::support::layout_pages_at;
use crate::layout::cells::TableRowCells;
use crate::layout::elements::{LayoutVisitor, TableRow, TextBlock};
use crate::layout::engine::visit_layout_tree;
use crate::types::PageSize;

#[derive(Default)]
struct Rows {
    column_widths: Vec<Vec<f32>>,
    heights: Vec<f32>,
    /// Each text block's words, one string per line.
    text_lines: Vec<Vec<String>>,
}

impl LayoutVisitor for Rows {
    fn visit_table_row(&mut self, row: &TableRow) {
        self.column_widths.push(row.content.column_widths.clone());
        self.heights.push(row.content.cells.row_block_extent());
    }

    fn visit_text_block(&mut self, block: &TextBlock) {
        let lines = block
            .lines
            .iter()
            .map(|line| {
                line.runs
                    .iter()
                    .map(|run| run.text.as_str())
                    .collect::<String>()
            })
            .map(|line| line.split_whitespace().collect::<Vec<_>>().join(" "))
            .filter(|line| !line.is_empty())
            .collect::<Vec<_>>();
        if !lines.is_empty() {
            self.text_lines.push(lines);
        }
    }
}

fn rows(markup: &str) -> Rows {
    let mut rows = Rows::default();
    for page in layout_pages_at(
        &format!("<style>td {{ padding: 0 }} table {{ border-spacing: 0 }}</style>{markup}"),
        PageSize::new(400.0, 800.0),
    ) {
        for (_, element) in &page.elements {
            visit_layout_tree(element.as_ref(), &mut rows);
        }
    }
    rows
}

#[test]
fn auto_table_cell_is_as_wide_as_its_paragraph() {
    let rows = rows("<table><tr><td><p>Paragraph text in a cell</p></td><td>x</td></tr></table>");
    assert!(
        rows.text_lines
            .contains(&vec!["Paragraph text in a cell".to_string()]),
        "{:?}",
        rows.text_lines
    );
}

#[test]
fn auto_table_cell_is_as_wide_as_its_div_and_list() {
    let rows = rows(
        "<table><tr><td><div>Div text in a cell</div></td><td><ul><li>List item</li></ul></td><td>end</td></tr></table>",
    );
    assert!(
        rows.text_lines
            .contains(&vec!["Div text in a cell".to_string()]),
        "{:?}",
        rows.text_lines
    );
    // The list's column is its 30pt indent and its item's text, not the
    // width its item's block was laid out at.
    let widths = &rows.column_widths[0];
    assert!(widths[1] < 100.0, "{widths:?}");
}

#[test]
fn paragraph_margins_in_a_cell_are_counted_once() {
    let rows =
        rows(r#"<table><tr><td><p style="margin:10px 0;line-height:20px">A</p></td></tr></table>"#);
    // 10px + 20px + 10px.
    assert_eq!(rows.heights, [30.0]);
}
