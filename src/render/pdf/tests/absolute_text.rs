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
        r#"<div style="position:relative;height:40pt"><div style="position:absolute;left:0;top:0;padding-left:8pt;border-left:1.5pt solid #f00;font:12pt Helvetica">Box</div></div>"#,
    );
    assert_eq!(x, PAGE_LEFT + 1.5 + 8.0);
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

/// The rectangle that fills the first blue background: (x, y, w, h).
fn blue_box(markup: &str) -> (f32, f32, f32, f32) {
    let pdf = crate::HtmlConverter::new()
        .compress(false)
        .convert(markup)
        .expect("the document converts");
    let content = String::from_utf8_lossy(&pdf);
    let rect = content
        .split("0 0 1 rg\n")
        .nth(1)
        .and_then(|rest| rest.lines().next())
        .expect("the blue fill's rectangle");
    let fields: Vec<f32> = rect
        .trim_end_matches(" re")
        .split_whitespace()
        .map(|field| field.parse().expect("a number"))
        .collect();
    (fields[0], fields[1], fields[2], fields[3])
}

#[test]
fn a_box_without_a_positioned_ancestor_is_placed_in_the_page_area() {
    // A4, with the default 72pt margins: the page area's top is 72pt below
    // the page's top edge. The box's static parent starts lower, after a
    // paragraph, and is indented by its padding.
    let page_area_top = 841.89 - 72.0;
    let (x, y, _, height) = blue_box(
        r#"<p>Before</p><div style="padding-left:30pt"><div style="position:absolute;top:100pt;width:20pt;height:10pt;background:#00f"></div></div>"#,
    );
    // Box edges snap to the 0.25pt paint grid.
    assert!(
        (y + height - (page_area_top - 100.0)).abs() <= 0.25,
        "its top {} is 100pt below the page area's top",
        y + height
    );
    assert_eq!(x, PAGE_LEFT + 30.0, "its left stays at its static position");
}

#[test]
fn a_fixed_box_is_placed_in_the_page_area_not_inside_the_body() {
    let (x, _, _, _) = blue_box(
        r#"<style>body { margin: 20pt; padding-left: 15pt }</style><div style="position:fixed;left:0;top:0;width:20pt;height:10pt;background:#00f"></div><p>Flow</p>"#,
    );
    assert_eq!(x, PAGE_LEFT, "at the page area's left edge");
}

#[test]
fn a_positioned_table_cell_is_its_absolute_childrens_containing_block() {
    let (x, _, _, _) = blue_box(
        r#"<table style="border-collapse:collapse;margin-left:40pt"><tr><td style="width:100pt;height:50pt">A</td><td style="position:relative;width:100pt;height:50pt;padding:0"><div style="position:absolute;left:5pt;top:5pt;width:20pt;height:10pt;background:#00f"></div>B</td></tr></table>"#,
    );
    assert!(
        x > PAGE_LEFT + 40.0 + 100.0,
        "x {x} is inside the second cell, not at the page area's left"
    );
}
