//! A margin at an unforced break is truncated (CSS Fragmentation 3 §5.2), so
//! a block whose border box fits at the bottom of a page stays there even when
//! its bottom margin doesn't. Chrome keeps it; the parity fixture
//! `paged-bottom-margin-truncated-at-break` holds its output.

use crate::types::PageSize;

use super::support::layout_pages_at;

/// How many in-flow boxes each page holds.
fn flow_boxes_per_page(markup: &str) -> Vec<usize> {
    layout_pages_at(markup, PageSize::new(200.0, 100.0))
        .iter()
        .map(|page| {
            page.elements
                .iter()
                .filter(|(_, element)| element.contributes_to_normal_flow())
                .count()
        })
        .collect()
}

#[test]
fn a_block_whose_border_box_fits_stays_on_the_page() {
    // 80pt, then a 15pt line ending at 95pt of the 100pt page; its 12pt
    // bottom margin runs past the page's end.
    let pages = flow_boxes_per_page(
        r#"<style>body { margin: 0; font: 12pt Helvetica; line-height: 15pt }</style><div style="height:80pt"></div><p style="margin:0 0 12pt">Last</p><p style="margin:0">Next</p>"#,
    );
    assert_eq!(pages, vec![2, 1], "the paragraph stays, the next one moves");
}

#[test]
fn a_block_whose_border_box_does_not_fit_moves() {
    let pages = flow_boxes_per_page(
        r#"<style>body { margin: 0; font: 12pt Helvetica; line-height: 15pt }</style><div style="height:90pt"></div><p style="margin:0 0 12pt">Last</p>"#,
    );
    assert_eq!(pages, vec![1, 1]);
}
