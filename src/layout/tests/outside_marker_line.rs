//! An `outside` list marker is aligned with its item's first line, as Chrome
//! lays it out: a marker that reaches higher than the line's text pushes the
//! line down, but a smaller marker whose line-height reaches lower than the
//! text doesn't make the line taller. The parity fixture
//! `lists-counters-marker-small-font-size` holds Chrome's output.

use crate::layout::elements::{LayoutVisitor, TextBlock};
use crate::types::PageSize;

use super::support::layout_pages_at;

/// The first line's height.
fn first_line_height(marker_css: &str) -> f32 {
    struct First(Option<f32>);
    impl LayoutVisitor for First {
        fn visit_text_block(&mut self, block: &TextBlock) {
            if self.0.is_none() {
                self.0 = block.lines.first().map(|line| line.height);
            }
        }
    }
    let mut first = First(None);
    let markup = format!(
        "<style>ol{{margin:0;font:15pt Helvetica;line-height:28.5pt;padding-left:48pt}}\
         li::marker{{{marker_css}}}</style><ol><li>Alpha</li></ol>"
    );
    for page in layout_pages_at(&markup, PageSize::new(300.0, 200.0)) {
        for (_, element) in &page.elements {
            element.accept(&mut first);
        }
    }
    first.0.expect("the item has a line")
}

#[test]
fn a_smaller_marker_leaves_the_line_its_line_height() {
    let height = first_line_height("font-size:9pt");
    assert!((height - 28.5).abs() < 0.01, "line height {height}");
}

#[test]
fn a_larger_marker_pushes_the_line_down() {
    assert!(first_line_height("font-size:27pt") > 28.5 + 1.0);
}

/// The first line's height in a `ul` with a disc marker in `marker_css`.
fn first_bullet_line_height(marker_css: &str) -> f32 {
    struct First(Option<f32>);
    impl LayoutVisitor for First {
        fn visit_text_block(&mut self, block: &TextBlock) {
            if self.0.is_none() {
                self.0 = block.lines.first().map(|line| line.height);
            }
        }
    }
    let mut first = First(None);
    let markup = format!(
        "<style>ul{{margin:0;font:15pt Helvetica;line-height:28.5pt;padding-left:48pt}}\
         li::marker{{{marker_css}}}</style><ul><li>Alpha</li></ul>"
    );
    for page in layout_pages_at(&markup, PageSize::new(300.0, 200.0)) {
        for (_, element) in &page.elements {
            element.accept(&mut first);
        }
    }
    first.0.expect("the item has a line")
}

#[test]
fn a_large_disc_pushes_the_line_down_as_its_text_would() {
    // Chromium lays a disc out as text in the marker's font: at 30pt its
    // ascent and half-leading reach above the 15pt text's, however small the
    // painted disc is.
    assert!(first_bullet_line_height("font-size:30pt") > 28.5 + 1.0);
    let small = first_bullet_line_height("font-size:9pt");
    assert!((small - 28.5).abs() < 0.01, "line height {small}");
}
