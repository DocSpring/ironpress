//! Normal flow through a block's principal box (`Container`): the box keeps
//! its own geometry, and its children's adjacent margins collapse
//! (CSS 2.1 §8.3.1) both inside it and when the page flow places what
//! follows it.

use super::support::layout_pages_at;
use crate::layout::elements::{LayoutVisitor, TextBlock};
use crate::layout::engine::visit_layout_tree;
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
