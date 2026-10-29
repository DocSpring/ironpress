//! CSS 2.1 §9.2.1.1: inline content beside block-level siblings is wrapped in
//! anonymous block boxes that keep their place among those siblings.

use crate::layout::elements::{Container, FlexRow, Image, LayoutVisitor, TableRow, TextBlock};
use crate::layout::engine::{TextLine, visit_layout_tree};

use super::support::layout_pages;

const PIXEL: &str = "data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mP8/5+hHgAHggJ/PchI7wAAAABJRU5ErkJggg==";

/// The painted content of a document in tree order: one entry per line box
/// sequence (its words, with `<img>` for an atomic inline) and `<img>` per
/// block image.
#[derive(Default)]
struct ContentOrder {
    entries: Vec<String>,
    /// Images already listed as items of a mixed inline row.
    listed_images: usize,
}

impl ContentOrder {
    fn push(&mut self, words: &str) {
        let words = words.split_whitespace().collect::<Vec<_>>().join(" ");
        if !words.is_empty() {
            self.entries.push(words);
        }
    }
}

fn line_words<'a>(lines: impl IntoIterator<Item = &'a TextLine>) -> String {
    lines
        .into_iter()
        .flat_map(|line| &line.runs)
        .map(|run| {
            if run.inline_box.is_some() {
                " <img> ".to_string()
            } else {
                run.text.clone()
            }
        })
        .collect()
}

#[derive(Default)]
struct ImageCount(usize);

impl LayoutVisitor for ImageCount {
    fn visit_image(&mut self, _element: &Image) {
        self.0 += 1;
    }
}

impl LayoutVisitor for ContentOrder {
    fn visit_text_block(&mut self, block: &TextBlock) {
        self.push(&line_words(&block.lines));
    }

    /// A mixed inline row lays each text run and atomic inline out as a cell.
    fn visit_flex_row(&mut self, row: &FlexRow) {
        let mut words = String::new();
        for cell in &row.content.cells {
            let mut images = ImageCount::default();
            for nested in &cell.nested_elements {
                visit_layout_tree(nested.as_ref(), &mut images);
            }
            self.listed_images += images.0;
            words.push(' ');
            words.push_str(&line_words(&cell.lines));
            words.push_str(&" <img> ".repeat(images.0));
        }
        self.push(&words);
    }

    /// A cell's own lines come before the boxes laid out inside it.
    fn visit_table_row(&mut self, row: &TableRow) {
        for cell in &row.content.cells {
            self.push(&line_words(&cell.layout.content.lines));
        }
    }

    fn visit_image(&mut self, _element: &Image) {
        if self.listed_images > 0 {
            self.listed_images -= 1;
        } else {
            self.entries.push("<img>".to_string());
        }
    }
}

fn content_order(markup: &str) -> Vec<String> {
    let mut order = ContentOrder::default();
    for page in layout_pages(markup) {
        for (_, element) in page.elements {
            visit_layout_tree(element.as_ref(), &mut order);
        }
    }
    order.entries
}

#[test]
fn grid_item_keeps_inline_content_after_its_block_children() {
    let order = content_order(&format!(
        r#"<div style="display:grid;grid-template-columns:200px">
            <div><div style="border:1px solid">Card</div><img style="width:20px;height:20px" src="{PIXEL}"></div>
            <div><div style="border:1px solid">Second</div>Text after <span>span</span></div>
        </div>"#
    ));

    assert_eq!(order, ["Card", "<img>", "Second", "Text after span"]);
}

#[test]
fn grid_item_keeps_inline_runs_between_block_children_in_source_order() {
    let order = content_order(&format!(
        r#"<div style="display:grid;grid-template-columns:200px">
            <div><img style="width:10px;height:10px" src="{PIXEL}"> lead
                <div>One</div> middle <b>bold</b> <div>Two</div> tail
            </div>
        </div>"#
    ));

    assert_eq!(order, ["<img> lead", "One", "middle bold", "Two", "tail"]);
}

#[test]
fn painted_block_keeps_inline_runs_between_block_children_in_source_order() {
    let order = content_order(&format!(
        r#"<div style="background:#ddd">Lead <b>bold</b>
            <div>One</div>
            mid <i>it</i> <img style="width:10px;height:10px" src="{PIXEL}"> after
            <div>Two</div>
            tail
        </div>"#
    ));

    assert_eq!(
        order,
        ["Lead bold", "One", "mid it <img> after", "Two", "tail"]
    );
}

#[test]
fn painted_block_keeps_generated_content_around_its_anonymous_blocks() {
    let order = content_order(
        r#"<style>.x::before{content:"[b]"} .x::after{content:"[a]"}</style>
        <div class="x" style="border:1px solid">head<div>One</div>foot</div>"#,
    );

    assert_eq!(order, ["[b]head", "One", "foot[a]"]);
}

#[test]
fn table_cell_keeps_the_text_after_its_block_children_after_them() {
    let order = content_order(
        r#"<table><tr><td>Before <div>block</div> after <i>it</i><div>end</div></td></tr></table>"#,
    );

    assert_eq!(order, ["Before", "block", "after it", "end"]);
}

#[test]
fn list_item_keeps_its_block_children_and_the_text_after_them_in_source_order() {
    let order = content_order(
        r#"<ol>
            <li>Text before <div>block</div> text <b>after</b></li>
            <li><span style="display:block">Span block</span></li>
        </ol>"#,
    );

    assert_eq!(
        order,
        ["1. Text before", "block", "text after", "2. Span block"]
    );
}

/// Each page-level text block's words and the x of its content edge.
#[derive(Default)]
struct ContentEdges(Vec<(String, f32)>);

impl LayoutVisitor for ContentEdges {
    fn visit_text_block(&mut self, block: &TextBlock) {
        let words = line_words(&block.lines);
        let words = words.split_whitespace().collect::<Vec<_>>().join(" ");
        let edge = block.positioning.insets.left
            + block.box_model.border.left.width
            + block.box_model.padding.left;
        self.0.push((words, edge));
    }
}

#[test]
fn list_item_lays_out_its_block_children_at_its_content_edge() {
    let mut edges = ContentEdges::default();
    for page in layout_pages(
        r#"<style>ds-line { display: block }</style>
        <ul style="margin:0;padding-left:40px">
            <li style="padding-left:8px"><ds-line>One</ds-line><div>Two</div><ds-line>Three</ds-line></li>
            <li><p style="margin:0">Four</p><p style="margin:0">Five</p></li>
        </ul>"#,
    ) {
        for (_, element) in &page.elements {
            element.accept(&mut edges);
        }
    }

    // 40px of list padding is 30pt, and the first item's 8px is 6pt more.
    let expected = [
        ("<img> One", 36.0),
        ("Two", 36.0),
        ("Three", 36.0),
        ("<img> Four", 30.0),
        ("Five", 30.0),
    ];
    assert_eq!(
        edges.0,
        expected.map(|(words, edge)| (words.to_string(), edge))
    );
}

#[derive(Default)]
struct ItemBoxes(Vec<(f32, Vec<String>)>);

impl LayoutVisitor for ItemBoxes {
    fn visit_container(&mut self, container: &Container) {
        let mut content = ContentOrder::default();
        for child in &container.children {
            visit_layout_tree(child.as_ref(), &mut content);
        }
        self.0
            .push((container.positioning.insets.left, content.entries));
    }
}

fn item_boxes(markup: &str) -> Vec<(f32, Vec<String>)> {
    let mut boxes = ItemBoxes::default();
    for page in layout_pages(markup) {
        for (_, element) in &page.elements {
            element.accept(&mut boxes);
        }
    }
    boxes.0
}

#[test]
fn list_item_with_a_box_keeps_it_around_its_block_children() {
    let boxes = item_boxes(
        r#"<ul style="margin:0;padding-left:40px">
            <li style="border:1px solid blue">Inline only</li>
            <li style="background:#efe"><p>One</p><p>Two</p></li>
            <li style="border-bottom:1px solid">Three<ul><li>Nested</li></ul></li>
        </ul>"#,
    );

    // The first item's text block paints its own box; the others keep a box
    // at the list's 30pt indent around all of their content.
    assert_eq!(
        boxes,
        [
            (30.0, vec!["<img> One".to_string(), "Two".to_string()]),
            (
                30.0,
                vec!["<img> Three".to_string(), "<img> Nested".to_string()]
            ),
        ]
    );
}
