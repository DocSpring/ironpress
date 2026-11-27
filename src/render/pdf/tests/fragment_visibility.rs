//! A box taller than a page, broken across pages, draws every line of it once and inside the page it's on: a page
//! break that cuts a table row, re-aligns a flex item in its shorter fragment or clamps it to its whole `min-height`
//! again moves text off the page, where nobody sees it. Each case is what Chromium prints for the same markup.

/// Every page's lines of text, with the y of each (from the page's bottom), from an uncompressed PDF. The body font
/// is embedded and its text drawn as glyph IDs, which the font's ToUnicode map (one font, so one map) turns back into
/// text
fn page_lines(body: &str) -> Vec<Vec<(String, f32)>> {
    let markup = format!(
        "<html><head><style>@page {{ size: 240pt 400pt; margin: 0 }} body {{ margin: 0; font: 12pt/18pt Helvetica }} \
         p {{ margin: 0 }} table {{ border-collapse: collapse }} td {{ padding: 0 }}</style></head><body>{body}</body></html>"
    );
    let pdf = crate::HtmlConverter::new()
        .compress(false)
        .convert(&markup)
        .expect("the document converts");
    let content = String::from_utf8_lossy(&pdf);
    let glyphs = to_unicode(&content);
    content
        .split("endstream")
        .filter(|stream| stream.contains("] TJ"))
        .map(|stream| stream_lines(stream, &glyphs))
        .collect()
}

/// A content stream's text, with the y on the page of each piece: its text matrix's origin through the current
/// transformation matrix (`q`, `Q` and `cm`), since some text is drawn in a CSS pixel space
fn stream_lines(
    stream: &str,
    glyphs: &std::collections::HashMap<String, char>,
) -> Vec<(String, f32)> {
    type Matrix = [f32; 6];
    let multiply = |m: Matrix, n: Matrix| -> Matrix {
        [
            m[0] * n[0] + m[1] * n[2],
            m[0] * n[1] + m[1] * n[3],
            m[2] * n[0] + m[3] * n[2],
            m[2] * n[1] + m[3] * n[3],
            m[4] * n[0] + m[5] * n[2] + n[4],
            m[4] * n[1] + m[5] * n[3] + n[5],
        ]
    };
    let mut ctm: Matrix = [1.0, 0.0, 0.0, 1.0, 0.0, 0.0];
    let mut saved = Vec::new();
    let mut operands: Vec<f32> = Vec::new();
    let mut array: Option<String> = None;
    let mut y = f32::NAN;
    let mut lines = Vec::new();
    for token in stream.split_whitespace() {
        if token.starts_with('[') {
            array = Some(String::new());
        }
        if let Some(text) = array.as_mut() {
            let glyph = token.trim_start_matches('[').trim_end_matches(']');
            if let Some(hex) = glyph
                .strip_prefix('<')
                .and_then(|rest| rest.strip_suffix('>'))
            {
                text.push(
                    *glyphs
                        .get(hex)
                        .unwrap_or_else(|| panic!("glyph {hex} in the ToUnicode map")),
                );
            }
            if token.ends_with(']') {
                lines.push((array.take().expect("an array"), y));
            }
            continue;
        }
        match token {
            "q" => saved.push(ctm),
            "Q" => ctm = saved.pop().expect("a saved state"),
            "cm" => {
                let m: Matrix = operands[operands.len() - 6..]
                    .try_into()
                    .expect("six operands");
                ctm = multiply(m, ctm);
            }
            "Tm" => {
                let m = &operands[operands.len() - 6..];
                y = m[4] * ctm[1] + m[5] * ctm[3] + ctm[5];
            }
            _ => {}
        }
        match token.parse::<f32>() {
            Ok(number) => operands.push(number),
            Err(_) => operands.clear(),
        }
    }
    lines
}

/// The ToUnicode map's glyph IDs (as the hex the content streams draw them with) and their characters
fn to_unicode(content: &str) -> std::collections::HashMap<String, char> {
    let mut glyphs = std::collections::HashMap::new();
    for block in content.split("beginbfchar").skip(1) {
        let block = block
            .split("endbfchar")
            .next()
            .expect("the bfchar block ends");
        for line in block.lines() {
            let mut codes = line
                .split_whitespace()
                .filter_map(|code| code.strip_prefix('<')?.strip_suffix('>'));
            if let (Some(glyph), Some(unicode)) = (codes.next(), codes.next()) {
                let character = u32::from_str_radix(unicode, 16)
                    .ok()
                    .and_then(char::from_u32)
                    .expect("a character");
                glyphs.insert(glyph.to_string(), character);
            }
        }
    }
    assert!(!glyphs.is_empty(), "the document has a ToUnicode map");
    glyphs
}

/// Checks that each of `prefix`1 to `prefix``count` is drawn exactly once, inside its page
fn assert_each_drawn_once_on_its_page(pages: &[Vec<(String, f32)>], prefix: &str, count: usize) {
    for number in 1..=count {
        let token = format!("{prefix}{number}");
        let drawn: Vec<(usize, f32)> = pages
            .iter()
            .enumerate()
            .flat_map(|(page, lines)| {
                lines
                    .iter()
                    .filter(|(text, _)| text == &token)
                    .map(move |(_, y)| (page, *y))
            })
            .collect();
        assert_eq!(drawn.len(), 1, "{token} drawn once: {drawn:?} in {pages:?}");
        let (page, y) = drawn[0];
        assert!(
            (0.0..400.0).contains(&y),
            "{token} at y {y} on page {}, outside it",
            page + 1
        );
    }
}

fn rows(prefix: &str, count: usize) -> String {
    (1..=count)
        .map(|index| format!("<tr><td>{prefix}{index}</td></tr>"))
        .collect()
}

fn paragraphs(prefix: &str, count: usize) -> String {
    (1..=count)
        .map(|index| format!("<p>{prefix}{index}</p>"))
        .collect()
}

#[test]
fn an_avoided_table_taller_than_a_page_keeps_the_row_at_the_page_end() {
    for row_style in ["", " style=\"break-inside: avoid\""] {
        let table = rows("ROW", 40).replace("<tr>", &format!("<tr{row_style}>"));
        let pages = page_lines(&format!(
            r#"<div style="break-inside: avoid"><table>{table}</table></div>"#
        ));
        assert_each_drawn_once_on_its_page(&pages, "ROW", 40);
    }
}

#[test]
fn a_flex_item_with_a_top_margin_breaks_with_the_others() {
    let pages = page_lines(&format!(
        r#"<div style="display: flex"><div style="flex: 1; margin-top: 5pt">{}</div><div style="flex: 1">{}</div></div>"#,
        paragraphs("LEFT", 40),
        paragraphs("RIGHT", 30)
    ));
    assert_each_drawn_once_on_its_page(&pages, "LEFT", 40);
    assert_each_drawn_once_on_its_page(&pages, "RIGHT", 30);
    // Side by side: both start on the first page
    for token in ["LEFT1", "RIGHT1"] {
        assert!(
            pages[0].iter().any(|(text, _)| text == token),
            "{token} on page 1: {pages:?}"
        );
    }
}

#[test]
fn a_wrapped_flex_line_taller_than_a_page_breaks_before_the_next_line() {
    let pages = page_lines(&format!(
        r#"<div style="display: flex; flex-wrap: wrap"><div style="width: 120pt">{}</div><div style="width: 120pt">{}</div><div style="width: 240pt">LAST1</div></div>"#,
        paragraphs("LEFT", 40),
        paragraphs("RIGHT", 30)
    ));
    assert_each_drawn_once_on_its_page(&pages, "LEFT", 40);
    assert_each_drawn_once_on_its_page(&pages, "RIGHT", 30);
    assert_each_drawn_once_on_its_page(&pages, "LAST", 1);
}

#[test]
fn a_centered_flex_item_with_a_min_height_breaks_from_its_top() {
    let pages = page_lines(&format!(
        r#"<div style="display: flex; align-items: center"><div style="flex: 1; min-height: 800pt">{}</div><div style="flex: 1">{}</div></div><p>AFTER1</p>"#,
        paragraphs("LEFT", 40),
        paragraphs("RIGHT", 30)
    ));
    assert_each_drawn_once_on_its_page(&pages, "LEFT", 40);
    assert_each_drawn_once_on_its_page(&pages, "RIGHT", 30);
    assert_each_drawn_once_on_its_page(&pages, "AFTER", 1);
    // The 800pt item starts at the top of page 1 and ends on page 2
    assert!(
        pages[0].iter().any(|(text, _)| text == "LEFT1"),
        "{pages:?}"
    );
}

#[test]
fn wrapped_flex_lines_taller_than_a_page_each_break_across_pages() {
    let item = |prefix: &str, count: usize| {
        format!(
            r#"<div style="width: 120pt">{}</div>"#,
            paragraphs(prefix, count)
        )
    };
    let pages = page_lines(&format!(
        r#"<div style="display: flex; flex-wrap: wrap">{}{}{}{}</div><p>AFTER1</p>"#,
        item("FIRST", 40),
        item("SECOND", 30),
        item("THIRD", 40),
        item("FOURTH", 30)
    ));
    for (prefix, count) in [
        ("FIRST", 40),
        ("SECOND", 30),
        ("THIRD", 40),
        ("FOURTH", 30),
        ("AFTER", 1),
    ] {
        assert_each_drawn_once_on_its_page(&pages, prefix, count);
    }
}

#[test]
fn a_box_that_clips_its_overflow_breaks_across_pages() {
    // An auto-height box has nothing below its end to hide, so Chromium breaks it like any other box, and each piece
    // clips to its own part of the box
    let lines = (1..=40)
        .map(|index| format!("A{index}<br>"))
        .collect::<String>();
    for overflow in ["hidden", "auto", "clip"] {
        for html in [
            format!(
                r#"<div style="overflow: {overflow}">{}</div>"#,
                paragraphs("A", 40)
            ),
            format!(
                r#"<div style="overflow: {overflow}"><div>{}</div></div>"#,
                paragraphs("A", 40)
            ),
            format!(r#"<div style="overflow: {overflow}">{lines}</div>"#),
            format!(
                r#"<p>Intro</p><div style="overflow: {overflow}; border: 2pt solid; padding: 5pt">{}</div>"#,
                paragraphs("A", 40)
            ),
            format!(
                r#"<div style="display: flex; overflow: {overflow}"><div>{}</div></div>"#,
                paragraphs("A", 40)
            ),
            format!(
                r#"<div style="display: flex"><div style="overflow: {overflow}">{}</div></div>"#,
                paragraphs("A", 40)
            ),
            format!(
                r#"<table><tr><td><div style="overflow: {overflow}">{}</div></td></tr></table>"#,
                paragraphs("A", 40)
            ),
        ] {
            assert_each_drawn_once_on_its_page(&page_lines(&html), "A", 40);
        }
    }
}

#[test]
fn a_box_with_a_height_taller_than_a_page_breaks_across_pages() {
    for style in [
        "",
        "; overflow: hidden",
        "; background: #eee",
        "; border: 2pt solid",
    ] {
        let pages = page_lines(&format!(
            r#"<p>Intro</p><div style="height: 600pt{style}">{}</div><p>A30</p>"#,
            paragraphs("A", 29)
        ));
        assert_each_drawn_once_on_its_page(&pages, "A", 30);
        // What follows the box comes after the rest of its height, on the next page
        assert!(
            pages[1]
                .iter()
                .any(|(text, y)| text == "A30" && *y < 400.0 - 200.0),
            "{style}: {pages:?}"
        );
    }
}

#[test]
fn a_box_that_clips_keeps_hiding_what_runs_past_its_height_on_the_next_page() {
    // 500pt of the box: 400pt on page 1 and 100pt on page 2, so A23 to A27 show on page 2 and the rest stays hidden
    let markup = format!(
        r#"<div style="height: 500pt; overflow: hidden">{}</div>"#,
        paragraphs("A", 40)
    );
    let pages = page_lines(&markup);
    assert_eq!(pages.len(), 2);
    assert_each_drawn_once_on_its_page(&pages, "A", 27);
    let pdf = crate::HtmlConverter::new()
        .compress(false)
        .convert(&format!(
            "<html><head><style>@page {{ size: 240pt 400pt; margin: 0 }} body {{ margin: 0; font: 12pt/18pt Helvetica }} \
             p {{ margin: 0 }}</style></head><body>{markup}</body></html>"
        ))
        .expect("the document converts");
    let content = String::from_utf8_lossy(&pdf);
    let second = content
        .split("endstream")
        .filter(|stream| stream.contains("] TJ"))
        .nth(1)
        .expect("a second page");
    let clip = second.find("W n").expect("page 2 clips the box");
    assert!(
        clip < second.find("] TJ").expect("text"),
        "the clip comes before the text"
    );
}

#[test]
fn a_box_with_a_height_and_box_decoration_break_clone_breaks_across_pages() {
    // Each piece draws the box's border and padding again, and the rest of its height carries on to the next page
    let section = format!(
        r#"<div style="height: 600pt; border: 2pt solid; padding: 6pt; box-decoration-break: clone{{overflow}}">{}</div>"#,
        paragraphs("A", 29)
    );
    for overflow in ["", "; overflow: hidden"] {
        let section = section.replace("{overflow}", overflow);
        for html in [
            format!("<p>Intro</p>{section}<p>A30</p>"),
            format!(
                r#"<p>Intro</p><div style="display: flex"><div style="width: 180pt">{section}</div></div><p>A30</p>"#
            ),
        ] {
            assert_each_drawn_once_on_its_page(&page_lines(&html), "A", 30);
        }
    }
}

#[test]
fn overflow_past_a_height_carries_on_over_the_next_pages() {
    // 500pt of the box: 382pt on page 1 below the intro and 118pt on page 2, while its 60 lines (1080pt) run on to
    // page 3. What follows the box goes below its end on page 2, under the overflow, as Chromium draws it
    for (html, boxed) in [
        (
            format!(
                r#"<p>Intro</p><div style="height: 500pt">{}</div><p>F1</p>"#,
                paragraphs("A", 60)
            ),
            60,
        ),
        (
            format!(
                r#"<p>Intro</p><div style="height: 500pt">{}</div><p>F1</p>"#,
                (1..=60)
                    .map(|index| format!("A{index}<br>"))
                    .collect::<String>()
            ),
            60,
        ),
        (
            format!(
                r#"<p>Intro</p><div><div style="height: 500pt; border: 2pt solid; padding-left: 10pt">{}</div></div><p>F1</p>"#,
                paragraphs("A", 60)
            ),
            60,
        ),
        // A short box near the end of the page, whose overflow runs on to the next one
        (
            format!(
                r#"<div style="height: 340pt"></div><div><div style="height: 40pt">{}</div></div><p>F1</p>"#,
                paragraphs("A", 10)
            ),
            10,
        ),
    ] {
        let pages = page_lines(&html);
        assert_each_drawn_once_on_its_page(&pages, "A", boxed);
        let follower: Vec<(usize, f32)> = pages
            .iter()
            .enumerate()
            .flat_map(|(page, lines)| {
                lines
                    .iter()
                    .filter(|(text, _)| text == "F1")
                    .map(move |(_, y)| (page, *y))
            })
            .collect();
        let expected = if boxed == 60 {
            // On page 2, 118pt down: its baseline about 132pt below the page's top
            (1, 400.0 - 132.0)
        } else {
            // Right below the box, at 380pt down on page 1
            (0, 400.0 - 394.0)
        };
        assert_eq!(follower.len(), 1, "{html}: {pages:?}");
        assert_eq!(follower[0].0, expected.0, "{html}: {pages:?}");
        assert!(
            (follower[0].1 - expected.1).abs() < 4.0,
            "{html}: F1 at {:?}, expected near {expected:?}",
            follower[0]
        );
    }
}

#[test]
fn a_box_that_scrolls_keeps_room_for_its_scrollbars() {
    // A `scroll` box's horizontal scrollbar sits below its content, which its auto height makes room for, so
    // nothing at its end is drawn under the bar: not on one page, and not on the last of several
    let marker = r#"<a href="https://example.com/placed" style="display: inline-block; width: 60pt; height: 15pt; background: black"></a>"#;
    let document = |body: String| {
        format!(
            "<html><head><style>@page {{ size: 240pt 400pt; margin: 0 }} body {{ margin: 0; font: 12pt/18pt Helvetica }} \
             p {{ margin: 0 }}</style></head><body>{body}</body></html>"
        )
    };
    for count in [3, 42] {
        for decoration in [
            "",
            "; border: 2pt solid; padding: 6pt; box-decoration-break: clone",
        ] {
            let html = document(format!(
                r#"<div style="overflow: scroll{decoration}">{}<p>{marker}</p></div>"#,
                paragraphs("A", count)
            ));
            let rects = super::placed_link_rects(&html);
            assert_eq!(rects.len(), 1, "{count}{decoration}: {rects:?}");
            let [_, bottom, _, top] = rects[0];
            assert!(
                (top - bottom - 15.0).abs() < 0.1,
                "{count}{decoration}: the marker is cut to {rects:?}"
            );
        }
    }
    // The first page's part of a box taller than the page shows no horizontal scrollbar at its page end, where its
    // content runs on: its clip reaches the bottom of the page
    let pdf = crate::HtmlConverter::new()
        .compress(false)
        .convert(&document(format!(
            r#"<div style="overflow: scroll">{}</div>"#,
            paragraphs("A", 42)
        )))
        .expect("the document converts");
    let content = String::from_utf8_lossy(&pdf);
    let first = content
        .split("endstream")
        .find(|stream| stream.contains("] TJ"))
        .expect("a first page");
    let clips: Vec<Vec<f32>> = first
        .lines()
        .filter(|line| line.ends_with("re W n"))
        .map(|line| {
            line.split_whitespace()
                .take(4)
                .map(|value| value.parse().expect("a number"))
                .collect()
        })
        .collect();
    assert!(!clips.is_empty(), "the box clips: {first}");
    for clip in &clips {
        // A scrollbar there would stop it 11.25pt up
        assert!(clip[1].abs() < 1.0, "{clip:?} stops above the page's end");
    }
    // Its text ends 11.25pt further down when the box has a horizontal scrollbar
    for (overflow, top) in [("scroll", 54.0 + 11.25), ("hidden", 54.0)] {
        let pages = page_lines(&format!(
            r#"<div style="overflow: {overflow}">{}</div><p>F1</p>"#,
            paragraphs("A", 3)
        ));
        let (_, y) = pages[0]
            .iter()
            .find(|(text, _)| text == "F1")
            .expect("F1 drawn");
        let a1 = pages[0]
            .iter()
            .find(|(text, _)| text == "A1")
            .expect("A1 drawn")
            .1;
        assert!(
            ((a1 - y) - top).abs() < 0.1,
            "{overflow}: F1 {} below A1",
            a1 - y
        );
    }
}

#[test]
fn a_box_in_columns_draws_each_line_inside_its_column() {
    // 42 lines and a taller one: more than two columns of a page hold, so the box breaks inside itself at the
    // page's end and carries on in the columns of the next page. Its top border and padding take room in its first
    // column. A box with nothing to draw of its own lays out as its children, which break between the columns too
    let marker = r#"<span style="display: inline-block; width: 60pt; height: 15pt; background: black"></span>"#;
    for decoration in [
        "",
        "; border: 2pt solid; padding: 6pt",
        "; border: 2pt solid; padding: 20pt; border-radius: 18pt",
        "; border: 2pt solid; padding: 6pt; box-decoration-break: clone",
    ] {
        for overflow in ["visible", "hidden"] {
            let html = format!(
                r#"<div style="column-count: 2; column-gap: 12pt"><div style="overflow: {overflow}{decoration}">{}<p>{marker}</p></div></div><p>A43</p>"#,
                paragraphs("A", 42)
            );
            assert_each_drawn_once_on_its_page(&page_lines(&html), "A", 43);
        }
    }
}
