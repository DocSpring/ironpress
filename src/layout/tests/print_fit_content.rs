//! Print fitting counts the scrollable overflow of lines and flex items
//! (css-overflow-3 §2.2), as Chromium does when it shrinks a printed page.
//! The parity fixtures `paged-print-fit-*` hold Chrome's output for each case.

use super::support::layout_pages_at;
use crate::layout::elements::{LayoutVisitor, TextBlock};
use crate::layout::engine::visit_layout_tree;
use crate::types::PageSize;
use std::collections::HashMap;

fn page() -> PageSize {
    PageSize::new(240.0, 120.0)
}

fn flex_item_scale(item_width: &str) -> f32 {
    let pages = layout_pages_at(
        &format!(
            r#"<style>
                * {{ margin:0; padding:0 }}
                .row {{ display:flex }}
                .item {{ flex:none; width:{item_width}; height:20pt }}
            </style>
            <div class="row"><div class="item">wide</div></div>"#
        ),
        page(),
    );
    pages[0].print_content_scale.factor()
}

#[test]
fn a_flex_item_wider_than_the_page_shrinks_the_document() {
    let scale = flex_item_scale("300pt");
    assert!(
        (scale - 0.8).abs() < 1e-4,
        "the 300pt item must fit the 240pt page, got scale {scale}"
    );
}

#[test]
fn the_document_shrinks_to_two_thirds_at_most() {
    let scale = flex_item_scale("400pt");
    assert!(
        (scale - 2.0 / 3.0).abs() < 1e-4,
        "a document is laid out at most 1.5 times the page width, got scale {scale}"
    );
}

#[test]
fn text_wraps_at_the_widened_page_width() {
    #[derive(Default)]
    struct Lines(usize);
    impl LayoutVisitor for Lines {
        fn visit_text_block(&mut self, block: &TextBlock) {
            self.0 += block.lines.len();
        }
    }
    let lines = |wide_block: &str, page: PageSize| {
        let pages = layout_pages_at(
            &format!(
                r#"<style>* {{ margin:0; padding:0 }} p {{ font-size:12pt }}</style>
                <p>One two three four five six seven eight nine ten eleven twelve thirteen fourteen
                fifteen sixteen seventeen eighteen nineteen twenty twenty-one twenty-two</p>{wide_block}"#
            ),
            page,
        );
        let mut lines = Lines::default();
        for (_, element) in &pages[0].elements {
            visit_layout_tree(element.as_ref(), &mut lines);
        }
        lines.0
    };
    let widened = lines(
        r#"<div style="width:300pt;height:10pt"></div>"#,
        PageSize::new(240.0, 120.0),
    );
    assert_eq!(
        widened,
        lines("", PageSize::new(300.0, 150.0)),
        "beside a 300pt block, the paragraph wraps as on a 300pt page"
    );
    assert!(
        widened < lines("", page()),
        "and in fewer lines than at 240pt"
    );
}

#[test]
fn a_line_wider_than_its_box_shrinks_the_document() {
    #[derive(Default)]
    struct Line(Option<f32>);
    impl LayoutVisitor for Line {
        fn visit_text_block(&mut self, block: &TextBlock) {
            if let Some(line) = block.lines.first() {
                let width = crate::layout::helpers::measure_runs_width(&line.runs, &HashMap::new());
                self.0 = Some(
                    self.0
                        .unwrap_or(0.0)
                        .max(block.positioning.insets.left + width),
                );
            }
        }
    }

    let pages = layout_pages_at(
        r#"<style>
            * { margin:0; padding:0 }
            div { width:40pt; font-size:24pt }
        </style>
        <div>Unbreakablewordwiderthanthepage</div>"#,
        page(),
    );
    let mut line = Line::default();
    for (_, element) in &pages[0].elements {
        visit_layout_tree(element.as_ref(), &mut line);
    }
    let line_right = line.0.expect("the word is laid out as a line");
    assert!(
        line_right > 240.0,
        "the fixture's word must overflow the page: {line_right}"
    );
    let scale = pages[0].print_content_scale.factor();
    assert!(
        (scale * line_right - 240.0).abs() < 0.01,
        "the line's end must land on the page's edge: scale {scale}, line end {line_right}"
    );
}

#[test]
fn overflow_hidden_keeps_its_content_from_shrinking_the_document() {
    let pages = layout_pages_at(
        r#"<style>
            * { margin:0; padding:0 }
            .clip { width:100pt; overflow:hidden }
            .row { display:flex; width:100pt; overflow:hidden }
            .item { flex:none; width:400pt; height:20pt }
            p { font-size:24pt }
        </style>
        <div class="clip"><p>Unbreakablewordwiderthanthepage</p></div>
        <div class="row"><div class="item">wide</div></div>"#,
        page(),
    );
    assert!(
        pages[0].print_content_scale.is_identity(),
        "clipped overflow must not shrink the page, got {}",
        pages[0].print_content_scale.factor()
    );
}

#[test]
fn a_running_element_is_laid_out_at_the_physical_page_width() {
    // Its page-margin box is on the physical page: only the page area's
    // content is scaled back down onto it
    #[derive(Default)]
    struct ItemOffsets(Vec<f32>);
    impl LayoutVisitor for ItemOffsets {
        fn visit_flex_row(&mut self, row: &crate::layout::elements::FlexRow) {
            self.0
                .extend(row.content.cells.iter().map(|cell| cell.x_offset));
        }
    }
    let header_items = |wide_block: &str| {
        let pages = layout_pages_at(
            &format!(
                r#"<style>
                    * {{ margin:0; padding:0 }}
                    .hdr {{ position:running(hdr); display:flex; justify-content:space-between }}
                </style>
                <div class="hdr"><span>Left</span><span>Right</span></div>
                <p>Body</p>{wide_block}"#
            ),
            page(),
        );
        let header = pages[0]
            .generated_content
            .running_element(&crate::parser::css::PageContentReference::new(
                "hdr".into(),
                crate::parser::css::PageContentPolicy::Last,
            ))
            .expect("the running header");
        let mut items = ItemOffsets::default();
        visit_layout_tree(header, &mut items);
        (pages[0].print_content_scale.factor(), items.0)
    };
    let (fitting_scale, fitting) = header_items("");
    let (widened_scale, widened) = header_items(r#"<div style="width:300pt;height:10pt"></div>"#);
    assert_eq!(fitting_scale, 1.0);
    assert!(widened_scale < 1.0, "the 300pt block widens the layout");
    assert_eq!(fitting.len(), 2, "both header items: {fitting:?}");
    assert_eq!(
        widened, fitting,
        "the header items sit as on the physical page"
    );
}
