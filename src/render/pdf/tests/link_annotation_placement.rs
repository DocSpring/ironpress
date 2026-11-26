// A link's annotation is where its box is painted: through its own and its ancestors' CSS transforms, as Chromium
// writes it, and not over a box `visibility: hidden` hides

const PLACED_LINK: &str =
    r#"<a href="https://example.com/placed" style="display: inline-block; width: 120px; height: 20px"></a>"#;

fn placed_link_rects(html: &str) -> Vec<[f32; 4]> {
    let pdf = crate::html_to_pdf(html).unwrap();
    link_rects(&String::from_utf8_lossy(&pdf), "https://example.com/placed")
}

/// The link's one annotation in `transformed` is its one annotation in `plain` moved 30pt right and 45pt down
fn assert_moved_by_translate(plain: &str, transformed: &str) {
    let plain_rects = placed_link_rects(plain);
    let moved_rects = placed_link_rects(transformed);
    assert_eq!(plain_rects.len(), 1, "{plain}");
    assert_eq!(moved_rects.len(), 1, "{transformed}");
    let (before, after) = (plain_rects[0], moved_rects[0]);
    let expected = [before[0] + 30.0, before[1] - 45.0, before[2] + 30.0, before[3] - 45.0];
    for (actual, wanted) in after.iter().zip(expected) {
        assert!((actual - wanted).abs() < 0.1, "{after:?} should be {expected:?} for {transformed}");
    }
}

#[test]
fn render_link_annotation_through_a_flex_parents_transform() {
    let row = |style: &str| format!(r#"<div style="display: flex; width: 200px{style}">{PLACED_LINK}</div>"#);
    assert_moved_by_translate(&row(""), &row("; transform: translate(30pt, 45pt)"));
}

#[test]
fn render_link_annotation_through_a_paragraphs_transform() {
    let paragraph = |style: &str| format!(r#"<p style="margin: 0{style}">Sign {PLACED_LINK}</p>"#);
    assert_moved_by_translate(&paragraph(""), &paragraph("; transform: translate(30pt, 45pt)"));
}

#[test]
fn render_link_annotation_through_a_block_links_own_transform() {
    let link = |style: &str| {
        format!(
            r#"<a href="https://example.com/placed" style="display: block; width: 120px; height: 20px{style}"></a>"#
        )
    };
    assert_moved_by_translate(&link(""), &link("; transform: translate(30pt, 45pt)"));
}

#[test]
fn render_link_annotation_through_an_inline_block_links_own_transform() {
    assert_moved_by_translate(
        PLACED_LINK,
        &PLACED_LINK.replace("height: 20px", "height: 20px; transform: translate(30pt, 45pt)"),
    );
}

#[test]
fn render_link_annotation_over_a_flex_container_link() {
    let flex_link = PLACED_LINK.replace("display: inline-block", "display: flex");
    let rects = placed_link_rects(&flex_link);
    assert_eq!(rects.len(), 1, "{flex_link}");
    let [left, bottom, right, top] = rects[0];
    assert!((right - left - 90.0).abs() < 0.1 && (top - bottom - 15.0).abs() < 0.1, "{rects:?}");
    // Nested in a block, and through its own transform
    let nested = format!("<div>{flex_link}</div>");
    assert_moved_by_translate(&nested, &nested.replace("height: 20px", "height: 20px; transform: translate(30pt, 45pt)"));
}

#[test]
fn render_no_link_annotation_over_a_hidden_box() {
    let hidden = PLACED_LINK.replace("display: inline-block", "visibility: hidden; display: inline-block");
    assert!(placed_link_rects(&format!(r#"<div style="display: flex">{hidden}</div>"#)).is_empty());
    // A visible link inside a hidden box keeps its annotation
    let shown = PLACED_LINK.replace("display: inline-block", "visibility: visible; display: inline-block");
    assert_eq!(
        placed_link_rects(&format!(r#"<div style="display: flex; visibility: hidden">{shown}</div>"#)).len(),
        1
    );
}
