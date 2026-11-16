//! A block whose only content is an atomic inline (an image, an inline SVG)
//! is laid out as one inline row. Its `height` is a used height like any
//! block's (CSS 2.1 §10.5): a line taller than the content box overflows it
//! instead of growing it, and `box-sizing` decides what the height measures.
//! Chrome keeps a 40px border-box div with 10px padding and a 30px image at
//! 40px, and starts the next block at 40px.

use super::support::layout_pages_at;
use crate::types::PageSize;

const IMAGE: &str = r#"<img style="width:30px;height:30px" src="data:image/svg+xml,%3Csvg xmlns=%22http://www.w3.org/2000/svg%22 width=%221%22 height=%221%22/%3E">"#;

fn next_block_top(box_sizing: &str) -> f32 {
    let pages = layout_pages_at(
        &format!(
            r#"<style>* {{ margin:0 }} .s {{ height:40px; padding:10px; box-sizing:{box_sizing}; background:#1565c0 }}</style>
            <div class="s">{IMAGE}</div><div style="height:20px;background:#ccc"></div>"#
        ),
        PageSize::new(150.0, 150.0),
    );
    pages[0].elements[1].0
}

#[test]
fn a_border_box_height_holds_an_image_line() {
    let top = next_block_top("border-box");
    assert!(
        (top - 30.0).abs() < 0.01,
        "the 40px border box ends at 30pt, got {top}"
    );
}

#[test]
fn a_content_box_height_adds_the_padding() {
    let top = next_block_top("content-box");
    assert!(
        (top - 45.0).abs() < 0.01,
        "40px content plus 20px padding ends at 45pt, got {top}"
    );
}
