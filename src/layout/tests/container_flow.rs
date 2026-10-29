//! Normal flow through a block's principal box (`Container`): the box keeps
//! its own geometry, and its children's adjacent margins collapse
//! (CSS 2.1 §8.3.1) both inside it and when the page flow places what
//! follows it.

use super::support::layout_pages_at;
use crate::layout::elements::{Container, LayoutVisitor, TextBlock};
use crate::layout::engine::visit_layout_tree;
use crate::layout::flow_metrics::BlockMargins;
use crate::style::computed::Transform;
use crate::types::PageSize;

/// 20px line boxes and 10px margins, in points.
const LINE: f32 = 15.0;
const MARGIN: f32 = 7.5;

fn paragraphs(count: usize) -> String {
    r#"<p style="margin:10px 0;line-height:20px;font-size:10px">P</p>"#.repeat(count)
}

/// The top of every page-level box on the first page, in flow order.
fn page_level_tops(markup: &str) -> Vec<f32> {
    let pages = layout_pages_at(markup, PageSize::new(300.0, 800.0));
    pages
        .first()
        .map(|page| page.elements.iter().map(|(y, _)| *y).collect())
        .unwrap_or_default()
}

#[test]
fn block_after_a_container_follows_its_collapsed_child_margins() {
    let tops = page_level_tops(&format!(
        r#"<div style="background:#eee">{}</div><p style="margin:0">after</p>"#,
        paragraphs(10)
    ));

    // The first and last paragraph margins collapse through the container;
    // the nine pairs between paragraphs collapse to one margin each.
    let container_bottom = MARGIN + 10.0 * LINE + 9.0 * MARGIN;
    assert_eq!(tops.len(), 2);
    assert!(
        (tops[1] - (container_bottom + MARGIN)).abs() < 0.01,
        "{tops:?}"
    );
}

#[derive(Default)]
struct TextBlockCount(usize);

impl LayoutVisitor for TextBlockCount {
    fn visit_text_block(&mut self, block: &TextBlock) {
        self.0 += usize::from(!block.lines.is_empty());
    }
}

#[test]
fn split_container_fills_the_page_with_collapsed_child_margins() {
    let pages = layout_pages_at(
        &format!(r#"<div style="background:#eee">{}</div>"#, paragraphs(20)),
        PageSize::new(300.0, 300.0),
    );

    let per_page = pages
        .iter()
        .map(|page| {
            let mut count = TextBlockCount::default();
            for (_, element) in &page.elements {
                visit_layout_tree(element.as_ref(), &mut count);
            }
            count.0
        })
        .collect::<Vec<_>>();
    // Below the container's collapsed top margin, n paragraphs take
    // n lines and n - 1 collapsed margins: 13 fit in 300pt, 14 do not.
    assert_eq!(per_page, [13, 7]);
}

/// The page-level boxes of a one-page document.
fn page_level_boxes(markup: &str) -> Vec<crate::layout::elements::LayoutNode> {
    let mut pages = layout_pages_at(markup, PageSize::new(400.0, 800.0));
    assert_eq!(pages.len(), 1);
    pages
        .remove(0)
        .elements
        .into_iter()
        .map(|(_, element)| element)
        .collect()
}

#[derive(Default)]
struct OnlyContainer(Vec<Container>);

impl LayoutVisitor for OnlyContainer {
    fn visit_container(&mut self, container: &Container) {
        self.0.push(container.clone());
    }
}

/// The principal box of a document whose only page-level box is a block
/// with block children.
fn principal_box(markup: &str) -> Container {
    let boxes = page_level_boxes(markup);
    assert_eq!(boxes.len(), 1, "{markup}");
    let mut container = OnlyContainer::default();
    boxes[0].accept(&mut container);
    container
        .0
        .pop()
        .expect("the block keeps its principal box")
}

#[test]
fn block_with_block_children_keeps_its_width_and_inline_offset() {
    let shifted = principal_box(r#"<div style="width:200px;margin-left:50px"><div>A</div></div>"#);
    assert_eq!(shifted.box_model.size.width.fixed_value(), Some(150.0));
    assert_eq!(shifted.positioning.insets.left, 37.5);

    let centered = principal_box(r#"<div style="width:200px;margin:0 auto"><div>A</div></div>"#);
    assert_eq!(centered.positioning.insets.left, 125.0);

    let indented = principal_box(r#"<div style="margin:0 40px"><p>Quoted</p></div>"#);
    assert_eq!(indented.positioning.insets.left, 30.0);
    assert_eq!(indented.box_model.size.width.fixed_value(), Some(340.0));
}

#[test]
fn block_with_block_children_keeps_its_transform() {
    let rotated = principal_box(r#"<div style="transform:rotate(30deg)"><div>A</div></div>"#);
    assert!(matches!(
        rotated.paint.group.transform.value,
        Some(Transform::Rotate(angle)) if angle == 30.0
    ));
}

#[test]
fn block_with_block_children_keeps_its_height_and_overflow_clip() {
    let clipped =
        principal_box(r#"<div style="height:30px;overflow:hidden"><div>A</div><div>B</div></div>"#);
    assert!(clipped.overflow.combined.clips());
    assert_eq!(clipped.box_model.size.height.used(), Some(22.5));
}

#[derive(Default)]
struct TextMargins(Vec<(String, BlockMargins)>);

impl LayoutVisitor for TextMargins {
    fn visit_text_block(&mut self, block: &TextBlock) {
        let text = block
            .lines
            .iter()
            .flat_map(|line| &line.runs)
            .map(|run| run.text.as_str())
            .collect::<String>();
        self.0
            .push((text.trim().to_string(), block.box_model.margins));
    }
}

fn text_margins(markup: &str) -> Vec<(String, f32, f32)> {
    let mut margins = TextMargins::default();
    for element in page_level_boxes(markup) {
        visit_layout_tree(element.as_ref(), &mut margins);
    }
    margins
        .0
        .into_iter()
        .map(|(text, margins)| (text, margins.start, margins.end))
        .collect()
}

fn margins(entries: &[(&str, f32, f32)]) -> Vec<(String, f32, f32)> {
    entries
        .iter()
        .map(|(text, start, end)| (text.to_string(), *start, *end))
        .collect()
}

#[test]
fn margins_of_a_block_without_a_box_collapse_with_its_outer_children() {
    // Nothing separates the block's margins from its first and last
    // children's (CSS 2.1 §8.3.1), and the anonymous blocks around its
    // inline content have no margins of their own.
    assert_eq!(
        text_margins(r#"<div style="margin:20px 0"><div>A</div><div>B</div></div><div>C</div>"#),
        margins(&[("A", 15.0, 0.0), ("B", 0.0, 15.0), ("C", 0.0, 0.0)])
    );
    assert_eq!(
        text_margins(r#"<div style="margin:20px 0">lead<p style="margin:10px 0">P</p>tail</div>"#),
        margins(&[("lead", 15.0, 0.0), ("P", 7.5, 7.5), ("tail", 0.0, 15.0)])
    );
    assert_eq!(
        text_margins(r#"<div style="margin:20px 0"><p style="margin:30px 0 0">P</p></div>"#),
        margins(&[("P", 22.5, 15.0)])
    );
}

#[derive(Default)]
struct MathBlockCount(usize);

impl LayoutVisitor for MathBlockCount {
    fn visit_math_block(&mut self, _block: &crate::layout::elements::MathBlock) {
        self.0 += 1;
    }
}

#[test]
fn principal_box_lays_out_its_math_children_once() {
    for decoration in [
        "background:#eee",
        "margin-left:20px",
        "transform:rotate(2deg)",
    ] {
        let mut count = MathBlockCount::default();
        for element in page_level_boxes(&format!(
            r#"<div style="{decoration}"><p>Before</p><div data-math="x+y" class="math-display"></div></div>"#
        )) {
            visit_layout_tree(element.as_ref(), &mut count);
        }
        assert_eq!(count.0, 1, "{decoration}");
    }
}
