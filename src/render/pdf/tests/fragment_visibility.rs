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
