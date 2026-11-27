//! A positioned box escapes the `overflow` clips of the boxes between it and its containing block, which aren't its
//! ancestors in the containing block chain (CSS 2.1 §11.1.1), as Chromium paints it: an absolutely positioned box in
//! a clipped box whose containing block is outside that box shows outside it, and so do its links

const MARKER: &str = r#"<div class="marker" style="position: absolute; top: 100pt"></div>"#;

/// How many clips are active where each red box is painted, from an uncompressed PDF. The page area's own clip
/// counts as one
fn clips_around_red(body: &str) -> Vec<usize> {
    let markup = format!(
        "<html><head><style>@page {{ size: 240pt 400pt; margin: 0 }} body {{ margin: 0; font: 12pt/18pt Helvetica }} \
         .marker {{ left: 100pt; width: 20pt; height: 20pt; background: red }}</style></head><body>{body}</body></html>"
    );
    let pdf = crate::HtmlConverter::new()
        .compress(false)
        .convert(&markup)
        .expect("the document converts");
    let content = String::from_utf8_lossy(&pdf);
    let mut found = Vec::new();
    for stream in content
        .split("endstream")
        .filter(|stream| stream.contains("1 0 0 rg"))
    {
        let mut saved = Vec::new();
        let mut clips = 0usize;
        let tokens: Vec<&str> = stream.split_whitespace().collect();
        for (index, token) in tokens.iter().enumerate() {
            match *token {
                "q" => saved.push(clips),
                "Q" => clips = saved.pop().expect("a q before each Q"),
                "W" | "W*" => clips += 1,
                "rg" if tokens[index.saturating_sub(3)..index] == ["1", "0", "0"] => {
                    found.push(clips)
                }
                _ => {}
            }
        }
    }
    assert!(!found.is_empty(), "a red box is painted: {body}");
    found
}

#[test]
fn an_absolute_box_escapes_the_clips_inside_its_containing_block() {
    // Its containing block is outside the clipping box: the box shows outside it
    for body in [
        format!(
            r#"<div style="position: relative; height: 300pt"><div style="overflow: hidden; height: 20pt">x{MARKER}</div></div>"#
        ),
        format!(r#"<div style="overflow: hidden; height: 20pt">x{MARKER}</div>"#),
        format!(
            r#"<div style="overflow: hidden; height: 20pt">x{}</div>"#,
            MARKER.replace("absolute", "fixed")
        ),
        format!(
            r#"<div style="position: relative; height: 300pt"><div style="overflow: hidden; height: 20pt; border-radius: 5pt"><div><div>x{MARKER}</div></div></div></div>"#
        ),
        format!(
            r#"<div style="position: relative; height: 300pt"><div style="display: flex; overflow: hidden; height: 20pt"><div>x</div>{MARKER}</div></div>"#
        ),
        format!(
            r#"<div style="position: relative; height: 300pt"><div style="display: grid; overflow: hidden; height: 20pt"><div>x</div>{MARKER}</div></div>"#
        ),
        format!(
            r#"<div style="position: relative; height: 300pt"><table><tr><td><div style="overflow: hidden; height: 20pt">x{MARKER}</div></td></tr></table></div>"#
        ),
        // What the box contains escapes with it
        format!(
            r#"<div style="position: relative; height: 300pt"><div style="overflow: hidden; height: 20pt">x<div style="position: absolute; top: 100pt; left: 100pt; width: 20pt"><div class="marker" style="position: relative"></div></div></div></div>"#
        ),
    ] {
        assert!(
            clips_around_red(&body).iter().all(|&clips| clips == 1),
            "{body}"
        );
    }
}

#[test]
fn an_absolute_box_keeps_the_clips_of_its_containing_block_and_around_it() {
    for (body, clips) in [
        // The clipping box is its containing block, or around it
        (
            format!(
                r#"<div style="overflow: hidden; height: 20pt; position: relative">x{MARKER}</div>"#
            ),
            2,
        ),
        (
            format!(
                r#"<div style="overflow: hidden; height: 20pt"><div style="position: relative">x{MARKER}</div></div>"#
            ),
            2,
        ),
        (
            format!(
                r#"<div style="overflow: hidden; height: 20pt; transform: translateX(0)">x{}</div>"#,
                MARKER.replace("absolute", "fixed")
            ),
            2,
        ),
        (
            format!(
                r#"<div style="display: grid; overflow: hidden; height: 20pt; position: relative"><div>x</div>{MARKER}</div>"#
            ),
            2,
        ),
        (
            format!(
                r#"<table><tr><td style="position: relative"><div style="overflow: hidden; height: 20pt; position: relative">x{MARKER}</div></td></tr></table>"#
            ),
            2,
        ),
        // It escapes the inner clip and keeps its containing block's
        (
            format!(
                r#"<div style="position: relative; overflow: hidden; height: 50pt"><div style="overflow: hidden; height: 20pt">x{MARKER}</div></div>"#
            ),
            2,
        ),
    ] {
        assert!(
            clips_around_red(&body).iter().all(|&found| found == clips),
            "{body}: {:?}",
            clips_around_red(&body)
        );
    }
}

#[test]
fn render_link_annotation_for_an_absolute_link_that_escapes_a_clip() {
    let link = r#"<a href="https://example.com/placed" style="position: absolute; top: 100pt; left: 100pt; display: block; width: 20pt; height: 20pt"></a>"#;
    let escaped = format!(
        r#"<div style="position: relative; height: 300pt"><div style="overflow: hidden; height: 20pt">x{link}</div></div>"#
    );
    assert_eq!(super::placed_link_rects(&escaped).len(), 1, "{escaped}");
    // A link inside the absolute box escapes with it
    let nested = escaped.replace(
        link,
        r#"<div style="position: absolute; top: 100pt; left: 100pt; width: 20pt"><a href="https://example.com/placed">LINK</a></div>"#,
    );
    assert_eq!(super::placed_link_rects(&nested).len(), 1, "{nested}");
    // A clipping box that's a stacking context paints the boxes in it inside its clip (its transparency group), so
    // they keep it, and their links stay cut to it
    let in_a_stacking_context = escaped.replace("height: 20pt\">", "height: 20pt; opacity: 0.5\">");
    assert_ne!(in_a_stacking_context, escaped);
    assert_eq!(
        super::placed_link_rects(&in_a_stacking_context),
        Vec::<[f32; 4]>::new()
    );
    // Its containing block clips it
    let clipped =
        format!(r#"<div style="position: relative; overflow: hidden; height: 20pt">x{link}</div>"#);
    assert_eq!(
        super::placed_link_rects(&clipped),
        Vec::<[f32; 4]>::new(),
        "{clipped}"
    );
}
