//! The body's border, and its background when the root has a different one,
//! are painted as one box cut at each page, as Chrome prints it: the side
//! borders on every page, the top border on the first page and the bottom
//! border on the last, after the content, the last box's bottom margin and the
//! bottom padding. The parity fixture `block-box-model-body-border-pages` holds
//! Chrome's output.

use crate::types::PageSize;

use super::support::layout_pages_at;

/// Each page's body slice: its top, its padding-box height, its top, right,
/// bottom and left border widths, and whether it paints a background.
fn slices(markup: &str) -> Vec<Option<(f32, f32, [f32; 4], bool)>> {
    layout_pages_at(markup, PageSize::new(200.0, 100.0))
        .into_iter()
        .map(|mut page| {
            page.elements.iter_mut().find_map(|(y, element)| {
                let y = *y;
                element.body_box_mut().map(|body| {
                    let (model, paint) = body.slice();
                    let border = model.border;
                    (
                        y,
                        model.size.height.used().unwrap_or_default(),
                        [
                            border.top.width,
                            border.right.width,
                            border.bottom.width,
                            border.left.width,
                        ],
                        paint.background.color.is_some(),
                    )
                })
            })
        })
        .collect()
}

const BODY: &str =
    "margin:6pt 0;padding:5pt;border:2pt solid #c00;font:10pt Helvetica;line-height:12pt";

/// `body_css` as the body's rule, then `content`.
fn page(body_css: &str, content: &str) -> String {
    format!("<style>body {{ {body_css} }}</style>{content}")
}

#[test]
fn the_top_border_is_on_the_first_page_and_the_bottom_on_the_last() {
    let pages = slices(&page(
        BODY,
        r#"<div style="height:180pt"></div><p style="margin:0 0 4pt">End</p>"#,
    ));
    assert_eq!(pages.len(), 3, "the content runs over three pages");
    let borders: Vec<[f32; 4]> = pages
        .iter()
        .map(|slice| slice.expect("every page has a body slice").2)
        .collect();
    assert_eq!(
        borders,
        vec![
            [2.0, 2.0, 0.0, 2.0],
            [0.0, 2.0, 0.0, 2.0],
            [0.0, 2.0, 2.0, 2.0]
        ]
    );
}

#[test]
fn the_first_slice_starts_at_the_bodys_top_margin() {
    let pages = slices(&page(BODY, r#"<p style="margin:0">One</p>"#));
    let (top, _, _, _) = pages[0].expect("a body slice");
    assert_eq!(top, 6.0);
}

#[test]
fn the_last_slice_ends_after_the_last_margin_and_the_bottom_padding() {
    let pages = slices(&page(BODY, r#"<p style="margin:0 0 4pt">One</p>"#));
    assert_eq!(pages.len(), 1);
    let (top, padding_box, border, _) = pages[0].expect("a body slice");
    // From the top margin: border, padding, one 12pt line, the line's 4pt
    // bottom margin (the padding keeps it inside the body), padding, border.
    let bottom = top + border[0] + padding_box + border[2];
    assert!(
        (bottom - (6.0 + 2.0 + 5.0 + 12.0 + 4.0 + 5.0 + 2.0)).abs() < 0.01,
        "bottom {bottom}"
    );
}

#[test]
fn the_body_paints_its_background_only_when_the_root_has_another() {
    let own = slices(&format!(
        "<style>html {{ background: #fff }}</style>{}",
        page(&format!("{BODY};background:#eef"), "<p>One</p>")
    ));
    assert!(own[0].expect("a body slice").3, "the body's own background");
    let canvas = slices(&page(&format!("{BODY};background:#eef"), "<p>One</p>"));
    assert!(
        !canvas[0].expect("a body slice").3,
        "the body's background is the canvas's, painted on the page area"
    );
}

#[test]
fn a_body_without_a_border_or_its_own_background_has_no_box() {
    let pages = slices(&page("margin:6pt;padding:5pt", "<p>One</p>"));
    assert_eq!(pages, vec![None]);
}

#[test]
fn a_set_height_sizes_the_box_whatever_its_content() {
    // `height` is the content box: 30pt, then 5pt padding and a 2pt border on
    // each side. The 12pt line inside doesn't change it.
    let pages = slices(&page(
        &format!("{BODY};height:30pt"),
        "<p style=\"margin:0\">One</p>",
    ));
    let (top, padding_box, border, _) = pages[0].expect("a body slice");
    assert_eq!(top, 6.0);
    assert!(
        (border[0] + padding_box + border[2] - 44.0).abs() < 0.01,
        "border box {}",
        border[0] + padding_box + border[2]
    );
}

#[test]
fn a_border_box_height_includes_the_padding_and_border() {
    let pages = slices(&page(
        &format!("{BODY};height:30pt;box-sizing:border-box"),
        "<p style=\"margin:0\">One</p>",
    ));
    let (_, padding_box, border, _) = pages[0].expect("a body slice");
    assert!((border[0] + padding_box + border[2] - 30.0).abs() < 0.01);
}
