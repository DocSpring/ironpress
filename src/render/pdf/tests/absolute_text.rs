//! An absolutely positioned text block draws its lines in its content box,
//! inside its border and padding (CSS 2.1 §8.1), aligned there, like a text
//! block in flow.

use crate::style::computed::FontFamily;

/// The x of the text matrix that draws the absolute box's line.
fn text_x(markup: &str) -> f32 {
    let pdf = crate::HtmlConverter::new()
        .compress(false)
        .convert(markup)
        .expect("the document converts");
    let content = String::from_utf8_lossy(&pdf);
    let matrix = content
        .lines()
        .find(|line| line.starts_with("1 0 0 1 ") && line.ends_with(" Tm"))
        .expect("the line's text matrix");
    matrix
        .split_whitespace()
        .nth(4)
        .and_then(|x| x.parse().ok())
        .expect("the text matrix's x")
}

const PAGE_LEFT: f32 = 72.0;

#[test]
fn its_line_starts_inside_its_left_border_and_padding() {
    let x = text_x(
        r#"<div style="position:relative;height:40pt"><div style="position:absolute;left:0;top:0;padding-left:8pt;border-left:2pt solid #f00;font:12pt Helvetica">Box</div></div>"#,
    );
    assert_eq!(x, PAGE_LEFT + 2.0 + 8.0);
}

#[test]
fn a_right_aligned_line_ends_at_its_content_box_edge() {
    let x = text_x(
        r#"<div style="position:relative;height:40pt"><div style="position:absolute;left:0;top:0;width:100pt;padding-right:8pt;box-sizing:border-box;text-align:right;font:12pt Helvetica">Box</div></div>"#,
    );
    let text_width = crate::fonts::str_width("Box", 12.0, &FontFamily::Helvetica, false);
    assert!(
        (x + text_width - (PAGE_LEFT + 100.0 - 8.0)).abs() < 0.01,
        "the line ends at {}, the content box's right edge",
        x + text_width
    );
}

#[test]
fn a_right_anchored_box_with_padding_keeps_its_text_inside_the_padding() {
    let x = text_x(
        r#"<div style="position:relative;width:200pt;height:40pt"><div style="position:absolute;right:0;top:0;padding:0 8pt;font:12pt Helvetica">Box</div></div>"#,
    );
    let text_width = crate::fonts::str_width("Box", 12.0, &FontFamily::Helvetica, false);
    assert!(
        (x + text_width - (PAGE_LEFT + 200.0 - 8.0)).abs() < 0.01,
        "the line ends at {}, 8pt inside the containing block's right edge",
        x + text_width
    );
}
