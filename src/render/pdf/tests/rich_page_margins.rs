const RED_PIXEL: &str = "data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mP8/5+hHgAHggJ/PchI7wAAAABJRU5ErkJggg==";

fn has_repeated_text_show(syntax: &str) -> bool {
    syntax
        .lines()
        .filter(|line| line.trim_end().ends_with("TJ"))
        .any(|command| syntax.lines().filter(|line| *line == command).count() >= 2)
}

/// CSS GCPM §1.2 requires `element()` to place the captured element complete
/// with its descendants in the page-margin box. Images and tables are layout
/// descendants, not a request to flatten the fragment to plain text.
#[test]
fn render_running_margin_container_with_image_and_table_on_each_page() {
    let html = format!(
        r#"<html><head><style>
            @page {{ size: 240pt 180pt; margin: 54pt 18pt 18pt;
                @top-center {{ content: element(issue245header) }} }}
            * {{ box-sizing: border-box }}
            .header {{ position: running(issue245header); width: 180pt; height: 36pt }}
            .header img {{ width: 12pt; height: 12pt }}
            .header table {{ display: inline-table; border-collapse: collapse }}
            .header td {{ border: 1pt solid #000; padding: 2pt }}
            .page {{ height: 100pt }}
        </style></head><body>
            <div class="header">
                <img src="{RED_PIXEL}" alt="">
                <table><tr><td>ISSUE245</td></tr></table>
            </div>
            <div class="page">First page</div>
            <div style="break-before: page" class="page">Second page</div>
        </body></html>"#
    );

    let pdf = crate::HtmlConverter::new()
        .compress(false)
        .convert(&html)
        .expect("rich running header PDF");
    let syntax = String::from_utf8_lossy(&pdf);

    assert_eq!(syntax.matches("/Type /Page ").count(), 2);
    assert!(
        has_repeated_text_show(&syntax),
        "the table's text-show command must repeat on both pages"
    );
    assert!(
        syntax.matches(" Do\n").count() >= 2,
        "the header image must paint on both pages"
    );
    assert!(syntax.contains("/Subtype /Image"), "header image object");
}

/// Issue #245's public contract: callers can provide an HTML fragment without
/// authoring GCPM rules themselves, while the converter still uses that shared
/// standards-based path internally.
#[test]
fn header_html_paints_an_image_and_table_on_each_page() {
    let header = format!(
        r#"<style>
            .issue245-header {{ display: flex; gap: 6pt; width: 180pt; height: 30pt }}
            .issue245-header img {{ width: 12pt; height: 12pt }}
            .issue245-header table {{ border-collapse: collapse }}
            .issue245-header td {{ border: 1pt solid #000; padding: 2pt }}
        </style>
        <div class="issue245-header">
            <img src="{RED_PIXEL}" alt="">
            <table><tr><td>ISSUE245-API</td></tr></table>
        </div>"#
    );
    let html = r#"
        <div style="height: 100pt">First page</div>
        <div style="break-before: page; height: 100pt">Second page</div>
    "#;

    let pdf = crate::HtmlConverter::new()
        .page_size(crate::PageSize::new(240.0, 180.0))
        .margin(crate::Margin::new(54.0, 18.0, 18.0, 18.0))
        .header_html(header)
        .compress(false)
        .convert(html)
        .expect("public rich header PDF");
    let syntax = String::from_utf8_lossy(&pdf);

    assert_eq!(syntax.matches("/Type /Page ").count(), 2);
    assert!(has_repeated_text_show(&syntax), "table text on both pages");
    assert!(syntax.matches(" Do\n").count() >= 2, "image on both pages");
}

#[test]
fn footer_html_paints_styled_nested_content_on_each_page() {
    let html = r#"
        <div style="height: 100pt">First page</div>
        <div style="break-before: page; height: 100pt">Second page</div>
    "#;
    let pdf = crate::HtmlConverter::new()
        .page_size(crate::PageSize::new(240.0, 180.0))
        .margin(crate::Margin::new(18.0, 18.0, 54.0, 18.0))
        .footer_html(
            r#"<div style="width:180pt;height:30pt;background:#e8edff">
                <strong style="color:#1736a4">ISSUE245-FOOTER</strong>
            </div>"#,
        )
        .compress(false)
        .convert(html)
        .expect("public rich footer PDF");
    let syntax = String::from_utf8_lossy(&pdf);

    assert_eq!(syntax.matches("/Type /Page ").count(), 2);
    assert!(
        has_repeated_text_show(&syntax),
        "nested footer text on both pages"
    );
    assert!(
        syntax.matches("0.9098 0.9294 1 rg\n").count() >= 2,
        "styled footer background on both pages"
    );
}

/// The first rectangle filled in `color`, as (x, y, width, height) in PDF
/// page space, whose y axis points up.
fn filled_rect(syntax: &str, color: &str) -> (f32, f32, f32, f32) {
    let mut lines = syntax.lines();
    while let Some(line) = lines.next() {
        if line == format!("{color} rg") {
            let rect = lines.next().expect("a path after the fill color");
            let numbers = rect
                .strip_suffix(" re")
                .expect("a rectangle after the fill color")
                .split_whitespace()
                .map(|number| number.parse::<f32>().expect("a number"))
                .collect::<Vec<_>>();
            return (numbers[0], numbers[1], numbers[2], numbers[3]);
        }
    }
    panic!("nothing is filled in {color}");
}

fn assert_rect_near(actual: (f32, f32, f32, f32), expected: (f32, f32, f32, f32)) {
    let near = |a: f32, b: f32| (a - b).abs() <= 0.5;
    assert!(
        near(actual.0, expected.0)
            && near(actual.1, expected.1)
            && near(actual.2, expected.2)
            && near(actual.3, expected.3),
        "painted {actual:?}, expected {expected:?}"
    );
}

/// CSS Paged Media 3 §5.3: each margin box has its own rectangle, which its
/// background fills. A running element is the box's block content: it starts
/// at the box's left edge (filling the box when it has no width of its own),
/// is centered in the top and bottom bands, and sits at the top, middle or
/// bottom of a side box.
#[test]
fn running_elements_take_their_margin_box_rectangles() {
    let pdf = crate::HtmlConverter::new()
        .compress(false)
        .convert(
            r#"<html><head><style>
                @page { size: 300pt 200pt; margin: 40pt 50pt 40pt 60pt;
                    @top-left-corner { content: element(a) }
                    @top-right-corner { content: element(b); background: #00ffff }
                    @left-middle { content: element(c) }
                    @right-bottom { content: element(d) }
                    @bottom-left { content: element(e); width: 100pt } }
                .r { width: 20pt; height: 10pt }
                #a { position: running(a); background: #ff0000 }
                #b { position: running(b); background: #00ff00 }
                #c { position: running(c); background: #0000ff; width: auto }
                #d { position: running(d); background: #ffff00 }
                #e { position: running(e); background: #ff00ff }
            </style></head><body>
                <div class="r" id="a"></div><div class="r" id="b"></div><div class="r" id="c"></div>
                <div class="r" id="d"></div><div class="r" id="e"></div><p>Body</p>
            </body></html>"#,
        )
        .expect("margin box PDF");
    let syntax = String::from_utf8_lossy(&pdf);

    // Top band: 40pt, so a 10pt element's bottom is 15pt below the band's top
    // edge at y=160. The corners are 60pt and 50pt wide.
    assert_rect_near(filled_rect(&syntax, "1 0 0"), (0.0, 175.0, 20.0, 10.0));
    assert_rect_near(filled_rect(&syntax, "0 1 0"), (250.0, 175.0, 20.0, 10.0));
    // The box's own background fills its rectangle, not the whole band.
    assert_rect_near(filled_rect(&syntax, "0 1 1"), (250.0, 160.0, 50.0, 40.0));
    // The left side runs from y=40 to y=160; its middle is y=100.
    assert_rect_near(filled_rect(&syntax, "0 0 1"), (0.0, 95.0, 60.0, 10.0));
    assert_rect_near(filled_rect(&syntax, "1 1 0"), (250.0, 40.0, 20.0, 10.0));
    // A declared width fixes the bottom-left box at the left page margin.
    assert_rect_near(filled_rect(&syntax, "1 0 1"), (60.0, 15.0, 20.0, 10.0));
}

/// A page-wide running element in a center box, the usual full-width header,
/// spans the page from its left edge.
#[test]
fn page_wide_running_element_in_a_center_box_spans_the_page() {
    let pdf = crate::HtmlConverter::new()
        .compress(false)
        .convert(
            r#"<html><head><style>
                @page { size: 300pt 200pt; margin: 40pt 50pt 40pt 60pt;
                    @top-center { content: element(h) } }
                #h { position: running(h); width: 300pt; height: 40pt; background: #ff0000 }
            </style></head><body><div id="h"></div><p>Body</p></body></html>"#,
        )
        .expect("running header PDF");
    let syntax = String::from_utf8_lossy(&pdf);

    assert_rect_near(filled_rect(&syntax, "1 0 0"), (0.0, 160.0, 300.0, 40.0));
}
