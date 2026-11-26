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
        .map(|stream| {
            let mut y = f32::NAN;
            let mut lines = Vec::new();
            for line in stream.lines() {
                if line.ends_with(" Tm") {
                    y = line
                        .split_whitespace()
                        .nth(5)
                        .and_then(|value| value.parse().ok())
                        .expect("the text matrix's y");
                } else if let Some(array) = line
                    .strip_suffix("] TJ")
                    .and_then(|rest| rest.strip_prefix('['))
                {
                    let text: String = array
                        .split_whitespace()
                        .filter_map(|item| item.strip_prefix('<')?.strip_suffix('>'))
                        .map(|glyph| {
                            *glyphs
                                .get(glyph)
                                .unwrap_or_else(|| panic!("glyph {glyph} in the ToUnicode map"))
                        })
                        .collect();
                    lines.push((text, y));
                }
            }
            lines
        })
        .collect()
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
