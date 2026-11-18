//! Border, outline and column rule widths are snapped as CSS Values 4 §7.7 says and Blink does: under a CSS pixel
//! is one pixel, and anything wider is cut to whole pixels. Widths here are in pt, 0.75pt to the pixel.

use super::computed::{ComputedStyle, compute_style};
use crate::layout::elements::OutlinePaint;
use crate::parser::dom::HtmlTag;

fn style(declarations: &str) -> ComputedStyle {
    compute_style(HtmlTag::Div, Some(declarations), &ComputedStyle::default())
}

fn border_width(declarations: &str) -> f32 {
    style(&format!("border-style: solid; {declarations}"))
        .border
        .top
        .used_width()
}

#[test]
fn a_border_is_cut_to_whole_pixels() {
    // 2pt is 2.67px, 2.5px and 2.9px are under 3px, and 1pt is 1.33px
    for (width, pixels) in [
        ("2pt", 2.0),
        ("2.5px", 2.0),
        ("2.9px", 2.0),
        ("1pt", 1.0),
        ("3px", 3.0),
        ("1.5pt", 2.0),
        ("thin", 1.0),
        ("medium", 3.0),
        ("thick", 5.0),
    ] {
        assert_eq!(
            border_width(&format!("border-width: {width}")),
            pixels * 0.75,
            "{width}"
        );
    }
}

#[test]
fn a_border_under_a_pixel_is_one_pixel_and_zero_stays_zero() {
    assert_eq!(border_width("border-width: 0.5px"), 0.75);
    assert_eq!(border_width("border-width: 0.1pt"), 0.75);
    assert_eq!(border_width("border-width: 0"), 0.0);
}

#[test]
fn a_border_in_ems_is_snapped_once_resolved() {
    // 0.15em of 16px is 2.4px
    assert_eq!(border_width("font-size: 16px; border-width: 0.15em"), 1.5);
}

#[test]
fn the_snapped_width_sizes_the_box() {
    let style = style("border: 2pt solid; padding: 0");
    assert_eq!(style.border.horizontal_width(), 3.0);
    assert_eq!(style.border.vertical_width(), 3.0);
}

#[test]
fn outlines_and_column_rules_are_snapped_as_borders_are() {
    assert_eq!(
        OutlinePaint::from_style(&style("outline: 2pt solid")).width,
        1.5
    );
    assert_eq!(
        OutlinePaint::from_style(&style("outline: 0.5px solid")).width,
        0.75
    );
    assert_eq!(
        style("column-rule: 2.5px solid").column_rule.used_width(),
        1.5
    );
}
