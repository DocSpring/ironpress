//! Lists: the numbers of their items in every formatting context, and the
//! list's own box.

use super::support::{layout_pages_at, visible_runs};
use crate::layout::elements::{Container, FlexRow, LayoutNode, LayoutVisitor};
use crate::types::PageSize;

fn words(markup: &str) -> String {
    let runs = visible_runs(markup);
    runs.iter()
        .flat_map(|run| run.text.split_whitespace())
        .collect::<Vec<_>>()
        .join(" ")
}

#[test]
fn list_that_is_a_grid_item_numbers_its_items() {
    let grid = r#"<div style="display:grid;grid-template-columns:300px">"#;

    assert_eq!(
        words(&format!(
            r#"{grid}<ol start="3"><li>Three</li><li>Four</li></ol></div>"#
        )),
        "3. Three 4. Four"
    );
    assert_eq!(
        words(&format!(
            r#"{grid}<ol reversed><li><p>Two</p></li><li><p>One</p></li></ol></div>"#
        )),
        "2. Two 1. One"
    );
}

#[test]
fn list_in_a_flex_container_numbers_its_items() {
    assert_eq!(
        words(r#"<div style="display:flex"><ol start="3"><li>Three</li><li>Four</li></ol></div>"#),
        "3. Three 4. Four"
    );
}

/// The page-level boxes of a one-page document, with their tops.
fn page_level_boxes(markup: &str) -> Vec<(f32, LayoutNode)> {
    let mut pages = layout_pages_at(markup, PageSize::new(400.0, 800.0));
    assert_eq!(pages.len(), 1, "{markup}");
    pages.remove(0).elements
}

fn page_level_tops(markup: &str) -> Vec<f32> {
    page_level_boxes(markup)
        .iter()
        .map(|(top, _)| *top)
        .collect()
}

/// 20px lines, in points; items without margins of their own.
const LINE: f32 = 15.0;
const STYLE: &str = "<style>body{font-size:10px;line-height:20px} p,li{margin:0}</style>";

#[test]
fn list_margins_separate_it_from_its_neighbours() {
    let tops = page_level_tops(&format!(
        r#"{STYLE}<p>start</p><ol style="margin:20px 0"><li>a</li><li>b</li></ol><p>after</p>"#
    ));
    assert_eq!(tops, [0.0, 2.0 * LINE, 3.0 * LINE, 5.0 * LINE]);
}

#[test]
fn margins_of_adjacent_lists_collapse() {
    let tops = page_level_tops(&format!(
        r#"{STYLE}<ul style="margin:20px 0"><li>a</li></ul><ol style="margin:10px 0"><li>b</li></ol>"#
    ));
    // The first list's top margin collapses through the body to the page.
    assert_eq!(tops[1] - tops[0], 2.0 * LINE, "{tops:?}");
}

#[test]
fn list_nested_in_a_list_has_no_vertical_margins() {
    let tops = page_level_tops(&format!(
        r#"{STYLE}<ul style="margin:20px 0"><li>a<ul><li>b</li></ul><ol><li>c</li></ol></li></ul><p>after</p>"#
    ));
    assert_eq!(
        tops[1..],
        [tops[0] + LINE, tops[0] + 2.0 * LINE, tops[0] + 4.0 * LINE]
    );
}

#[derive(Default)]
struct Boxes {
    containers: Vec<Container>,
    flex_rows: usize,
}

impl LayoutVisitor for Boxes {
    fn visit_container(&mut self, container: &Container) {
        self.containers.push(container.clone());
    }

    fn visit_flex_row(&mut self, _row: &FlexRow) {
        self.flex_rows += 1;
    }
}

fn boxes(markup: &str) -> Boxes {
    let mut boxes = Boxes::default();
    for (_, node) in page_level_boxes(markup) {
        node.accept(&mut boxes);
    }
    boxes
}

#[test]
fn list_with_a_background_keeps_its_box_around_its_items() {
    let markup = format!(
        r#"{STYLE}<ol start="3" style="background:#eee;padding:10px 0 10px 40px"><li>c</li><li>d</li></ol>"#
    );
    let list = boxes(&markup);
    assert_eq!(list.containers.len(), 1);
    let list = &list.containers[0];
    assert!(list.paint.background.color.is_some());
    assert_eq!(list.box_model.padding.top, 7.5);
    assert_eq!(list.box_model.padding.left, 30.0);
    assert_eq!(list.children.len(), 2);
    assert_eq!(words(&markup), "3. c 4. d");
}

#[test]
fn list_with_a_box_nested_in_a_list_item_starts_at_the_item_content_edge() {
    let list = boxes(&format!(
        r#"{STYLE}<ul><li>a<ul style="border:1px solid red"><li>b</li></ul></li></ul>"#
    ));
    assert_eq!(list.containers.len(), 1);
    assert_eq!(list.containers[0].positioning.insets.left, 30.0);
}

#[test]
fn flex_list_without_markers_lays_its_items_out_in_a_row() {
    let list =
        boxes(r#"<ul style="display:flex;list-style:none;padding:0"><li>a</li><li>b</li></ul>"#);
    assert_eq!(list.flex_rows, 1);
}
