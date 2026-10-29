//! The blocks inside a flex item that has no box of its own keep their own
//! margins, width and indent, as they would in any other block container.

use crate::layout::elements::{FlexRow, LayoutVisitor, TextBlock};
use crate::layout::engine::TextLine;

use super::support::layout_pages;

fn words(lines: &[TextLine]) -> String {
    lines
        .iter()
        .flat_map(|line| &line.runs)
        .flat_map(|run| run.text.split_whitespace())
        .collect::<Vec<_>>()
        .join(" ")
}

/// Each flex cell's text: its words, the x of their content edge and the
/// width they are laid out in. A cell's own lines start at the cell's content
/// edge; a nested block's at its own offset within the cell.
#[derive(Default)]
struct CellText {
    cell_x: f32,
    blocks: Vec<(String, f32, Option<f32>)>,
    row_heights: Vec<f32>,
}

impl LayoutVisitor for CellText {
    fn visit_text_block(&mut self, block: &TextBlock) {
        let edge = self.cell_x
            + block.positioning.insets.left
            + block.box_model.border.left.width
            + block.box_model.padding.left;
        self.blocks.push((
            words(&block.lines),
            edge,
            block.box_model.size.width.fixed_value(),
        ));
    }

    fn visit_flex_row(&mut self, row: &FlexRow) {
        self.row_heights.push(row.content.row_height);
        for cell in &row.content.cells {
            if !cell.lines.is_empty() {
                self.blocks.push((
                    words(&cell.lines),
                    cell.x_offset + cell.padding.left,
                    Some(cell.width),
                ));
            }
            self.cell_x = cell.x_offset;
            for element in &cell.nested_elements {
                element.accept(self);
            }
            self.cell_x = 0.0;
        }
    }
}

fn cell_text(markup: &str) -> CellText {
    let mut text = CellText::default();
    for page in layout_pages(markup) {
        for (_, element) in &page.elements {
            element.accept(&mut text);
        }
    }
    text.blocks.retain(|(words, _, _)| !words.is_empty());
    text
}

fn edges(text: &CellText) -> Vec<(&str, f32)> {
    text.blocks
        .iter()
        .map(|(words, edge, _)| (words.as_str(), *edge))
        .collect()
}

#[test]
fn block_in_a_flex_item_keeps_its_margin_and_padding() {
    let text = cell_text(
        r#"<style>p { margin: 0 }</style>
        <div style="display:flex"><div style="width:300px">
            <p style="margin-left:40px">Offset</p>
            <p>Plain</p>
            <p style="padding-left:20px">Padded</p>
        </div></div>"#,
    );

    // 40px is 30pt and 20px is 15pt.
    assert_eq!(
        edges(&text),
        [("Offset", 30.0), ("Plain", 0.0), ("Padded", 15.0)]
    );
}

#[test]
fn block_in_a_column_flex_item_keeps_its_margin_and_width() {
    let text = cell_text(
        r#"<style>p { margin: 0 }</style>
        <div style="display:flex;flex-direction:column"><div style="width:300px">
            <p style="margin-left:40px">Offset</p>
            <p style="width:80px">Narrow</p>
        </div></div>"#,
    );

    assert_eq!(
        text.blocks,
        [
            ("Offset".to_string(), 30.0, Some(195.0)),
            ("Narrow".to_string(), 0.0, Some(60.0))
        ]
    );
}

#[test]
fn block_in_a_flex_item_keeps_its_width() {
    let text = cell_text(
        r#"<div style="display:flex"><div style="width:300px">
            <p style="margin:0;width:80px">Narrow block of text</p>
        </div></div>"#,
    );

    let narrow = text
        .blocks
        .iter()
        .find(|(words, _, _)| words.starts_with("Narrow"))
        .expect("the narrow block is laid out");
    assert_eq!(narrow.2, Some(60.0), "80px is 60pt");
}

#[test]
fn list_in_a_flex_item_keeps_its_indent() {
    let markup = |list: &str| {
        cell_text(&format!(
            r#"<div style="display:flex">{list}</div>
            <div style="display:flex"><div>{list}</div></div>"#
        ))
    };
    let text = markup(
        r#"<ol start="3" style="margin:0;padding-left:40px"><li>Three</li><li>Four</li></ol>"#,
    );

    // The items' content edge is the list's 40px of padding, 30pt, whether the
    // list is the flex item or inside one.
    assert_eq!(
        edges(&text),
        [
            ("3. Three", 30.0),
            ("4. Four", 30.0),
            ("3. Three", 30.0),
            ("4. Four", 30.0)
        ]
    );
}

#[test]
fn flex_item_holds_the_margins_of_its_blocks() {
    let text = cell_text(
        r#"<div style="display:flex;line-height:20px">
            <div><p style="margin:10px 0">One</p><p style="margin:10px 0">Two</p></div>
            <div style="width:100px"><p style="margin:10px 0">Three</p></div>
        </div>"#,
    );

    // Two 15pt lines, 7.5pt margins above, between (collapsed) and below. Flex
    // items establish a formatting context, so the margins stay inside them.
    assert_eq!(text.row_heights, [52.5]);
}

#[test]
fn column_wrap_flex_item_keeps_every_block() {
    let text = cell_text(
        r#"<style>p { margin: 0 }</style>
        <div style="display:flex;flex-direction:column;flex-wrap:wrap;height:60px">
            <div><p>One</p><p>Two</p></div>
            <div><p style="margin-left:20px">Three</p><p>Four</p></div>
            <div><p>Five</p></div>
        </div>"#,
    );

    let words = text
        .blocks
        .iter()
        .map(|(words, _, _)| words.as_str())
        .collect::<Vec<_>>();
    assert_eq!(words, ["One", "Two", "Three", "Four", "Five"]);
    let edges = edges(&text);
    let edge = |word: &str| {
        edges
            .iter()
            .find(|(words, _)| *words == word)
            .map(|(_, edge)| *edge)
            .expect("the block is laid out")
    };
    assert_eq!(edge("Three") - edge("Four"), 15.0, "20px is 15pt");
}

#[test]
fn column_flex_item_collapses_the_margins_between_its_blocks() {
    let mut tops = Vec::new();
    for page in layout_pages(
        r#"<div style="display:flex;flex-direction:column;padding-top:10px;line-height:20px">
            <div><p style="margin:10px 0">One</p><p style="margin:10px 0">Two</p></div>
            <div><p style="margin:10px 0">Three</p><p style="margin:10px 0">Four</p></div>
        </div>"#,
    ) {
        for (top, element) in &page.elements {
            let mut text = CellText::default();
            element.accept(&mut text);
            tops.extend(text.blocks.into_iter().map(|(words, _, _)| (words, *top)));
        }
    }

    // Lines are 15pt and margins 7.5pt. The margins between the blocks of one
    // item collapse; those of two flex items don't.
    let expected = [
        ("One", 15.0),
        ("Two", 37.5),
        ("Three", 67.5),
        ("Four", 90.0),
    ];
    assert_eq!(tops, expected.map(|(words, top)| (words.to_string(), top)));
}
