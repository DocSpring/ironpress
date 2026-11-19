//! Chromium's UA stylesheet for text-level and grouping elements, and the
//! font-size rules it depends on: keyword sizes, `smaller`/`larger`, and the
//! 13px that the generic `monospace` family gets where no absolute length set
//! the size. Each expectation is Chrome's computed value for the same markup
//! (Chrome 156, default 16px proportional and 13px monospace sizes).

use crate::layout::engine::TextRun;
use crate::style::computed::{FontStyle, TextDecorationStyle};
use crate::types::Color;

use super::support::{layout_pages, visible_runs};

use crate::fonts::PT_PER_CSS_PX;

/// The run holding `word`.
fn run<'a>(runs: &'a [TextRun], word: &str) -> &'a TextRun {
    runs.iter()
        .find(|run| {
            run.text
                .split_whitespace()
                .any(|candidate| candidate == word)
        })
        .unwrap_or_else(|| panic!("no run holds {word:?}: {:?}", texts(runs)))
}

fn texts(runs: &[TextRun]) -> Vec<&str> {
    runs.iter().map(|run| run.text.as_str()).collect()
}

/// `word`'s font size in CSS px.
fn px(runs: &[TextRun], word: &str) -> f32 {
    run(runs, word).font_size / PT_PER_CSS_PX
}

fn assert_px(runs: &[TextRun], word: &str, expected: f32) {
    let actual = px(runs, word);
    assert!(
        (actual - expected).abs() < 0.01,
        "{word}: expected {expected}px, got {actual}px"
    );
}

#[test]
fn monospace_elements_are_13px_where_no_absolute_size_was_set() {
    let runs = visible_runs(
        "<p>x <code>code</code> <kbd>kbd</kbd> <samp>samp</samp> <tt>tt</tt></p>\
         <pre>pre <code>inpre</code> <span style=\"font-size:2em\">twice</span></pre>",
    );
    for word in ["code", "kbd", "samp", "tt", "pre", "inpre"] {
        assert_px(&runs, word, 13.0);
    }
    assert_px(&runs, "twice", 26.0);
    assert_eq!(run(&runs, "code").background_color, None);
}

#[test]
fn monospace_text_in_relative_sizes_scales_by_13_16() {
    let runs = visible_runs(
        "<h1>H <code>heading</code></h1>\
         <p style=\"font-size:120%\">pct <code>percent</code></p>\
         <p style=\"font: 1em monospace\">shorthand</p>",
    );
    assert_px(&runs, "heading", 26.0);
    assert_px(&runs, "percent", 15.6);
    assert_px(&runs, "shorthand", 13.0);
}

#[test]
fn an_absolute_size_keeps_monospace_text_its_size() {
    let runs = visible_runs(
        "<p style=\"font-size:14px\">abs <code>fixed</code></p>\
         <div style=\"font-size:12pt\"><pre>block</pre></div>",
    );
    assert_px(&runs, "fixed", 14.0);
    assert_px(&runs, "block", 16.0);
}

#[test]
fn a_rem_size_is_absolute_so_monospace_text_in_it_keeps_its_size() {
    // Chrome 157's computed sizes: Blink counts `rem` as an absolute size
    // (StyleBuilderConverterBase::ConvertFontSize).
    let runs = visible_runs(
        "<p><code style=\"font-size:0.875rem\">remcode</code></p>\
         <pre style=\"font-size:1rem\">rempre</pre>\
         <div style=\"font-size:1.125rem\"><code>underrem</code></div>",
    );
    assert_px(&runs, "remcode", 14.0);
    assert_px(&runs, "rempre", 16.0);
    assert_px(&runs, "underrem", 18.0);
}

#[test]
fn the_font_shorthands_em_line_height_follows_the_monospace_size() {
    // Chrome 157: `font: 1em/2em monospace` is 13px text on a 26px line.
    let runs = visible_runs("<p style=\"font: 1em/2em monospace\">shorthandline</p>");
    let run = run(&runs, "shorthandline");
    let line_height_px = run.line_height_factor * run.font_size / PT_PER_CSS_PX;
    assert!(
        (line_height_px - 26.0).abs() < 0.01,
        "expected a 26px line, got {line_height_px}px"
    );
}

#[test]
fn only_the_lone_generic_monospace_family_changes_the_size() {
    let runs = visible_runs(
        "<p><code style=\"font-family:monospace,monospace\">doubled</code> \
         <code style=\"font-family:Courier\">named</code> \
         <code><span style=\"font-family:serif\">serif</span></code></p>\
         <div style=\"font-family:monospace\"><span style=\"font-family:sans-serif\">back</span>\
         <span style=\"font-size:2em\">monotwice</span></div>",
    );
    assert_px(&runs, "doubled", 16.0);
    assert_px(&runs, "named", 16.0);
    assert_px(&runs, "serif", 16.0);
    assert_px(&runs, "back", 16.0);
    assert_px(&runs, "monotwice", 26.0);
}

#[test]
fn keyword_sizes_follow_blinks_table_for_the_family() {
    let runs = visible_runs(
        "<p><code style=\"font-size:small\">monosmall</code> \
         <span style=\"font-family:monospace;font-size:small\">spansmall</span> \
         <span style=\"font-size:x-large\">xlarge</span> \
         <span style=\"font-size:larger\">larger</span></p>\
         <p style=\"font: small monospace\">fontsmall</p>",
    );
    assert_px(&runs, "monosmall", 12.0);
    assert_px(&runs, "spansmall", 12.0);
    assert_px(&runs, "xlarge", 24.0);
    assert_px(&runs, "larger", 19.2);
    assert_px(&runs, "fontsmall", 12.0);
}

#[test]
fn small_and_big_scale_the_parent_size_by_1_2() {
    let runs = visible_runs(
        "<p>x <small>small</small> <big>big</big> <small><small>twice</small></small></p>\
         <h1>H <small>inheading</small></h1>\
         <p style=\"font-size:14px\">abs <small>inabs</small></p>",
    );
    assert_px(&runs, "small", 16.0 / 1.2);
    assert_px(&runs, "big", 19.2);
    assert_px(&runs, "twice", 16.0 / 1.44);
    assert_px(&runs, "inheading", 32.0 / 1.2);
    assert_px(&runs, "inabs", 14.0 / 1.2);
}

#[test]
fn phrase_elements_match_chromes_ua_stylesheet() {
    let runs = visible_runs(
        "<p><mark>marked</mark> <cite>cited</cite> <abbr title=\"t\">titled</abbr> \
         <abbr>bare</abbr> <var>variable</var> <dfn>defined</dfn></p>\
         <address>addressed</address><dl><dt>term</dt><dd>definition</dd></dl>",
    );
    let marked = run(&runs, "marked");
    assert_eq!(marked.background_color, Some(Color::rgb(255, 255, 0)));
    assert_eq!(marked.color, Color::rgb(0, 0, 0));
    for word in ["cited", "variable", "defined", "addressed"] {
        assert_eq!(run(&runs, word).font_style, FontStyle::Italic, "{word}");
    }
    let titled = run(&runs, "titled");
    assert!(
        titled.decorations.iter().any(|decoration| {
            decoration.lines.underline && decoration.style == TextDecorationStyle::Dotted
        }),
        "abbr[title] has a dotted underline: {:?}",
        titled.decorations
    );
    assert!(run(&runs, "bare").decorations.is_empty());
    assert!(!run(&runs, "term").bold, "Chrome doesn't bold a <dt>");
}

#[test]
fn a_q_element_is_quoted() {
    let text: String = visible_runs("<p>He said <q>stop</q> twice.</p>")
        .iter()
        .map(|run| run.text.as_str())
        .collect();
    assert!(text.contains("\u{201c}stop\u{201d}"), "{text:?}");
}

#[test]
fn listing_xmp_and_plaintext_are_preformatted() {
    let runs = visible_runs("<xmp>raw <b>tag</b></xmp><listing>listed</listing>");
    let text: String = texts(&runs).concat();
    assert!(
        text.contains("<b>tag</b>"),
        "xmp keeps its markup as text: {text:?}"
    );
    assert_px(&runs, "listed", 13.0);
}

#[test]
fn block_margins_match_chrome() {
    // Each block's top is its predecessor's bottom margin collapsed with its
    // own top margin; the page body has no margin in this fixture.
    let pages = layout_pages(
        "<style>body { margin: 0; font: 16px/18px serif }</style>\
         <dl><dt>t</dt><dd>d</dd></dl><figure><figcaption>c</figcaption></figure>\
         <pre>p</pre><hr><hr style=\"width:50%\"><hr style=\"width:50%;margin:4px 0\">",
    );
    let tops: Vec<f32> = pages[0]
        .elements
        .iter()
        .map(|(top, _)| top / PT_PER_CSS_PX)
        .collect();
    // Chrome's border-box tops for the dt, dd, figure, pre and the three hrs.
    let expected = [16.0, 34.0, 68.0, 102.0, 136.0, 146.0, 156.0];
    assert_eq!(tops.len(), expected.len(), "{tops:?}");
    for (actual, expected) in tops.iter().zip(expected) {
        assert!((actual - expected).abs() < 0.05, "tops {tops:?}");
    }

    // An `<hr>` with a width is centred (`margin-inline: auto`) unless the
    // author's margins replace the auto ones.
    // Border boxes: half the 390pt content width plus the 1px borders.
    let half = 390.0 / 2.0 + 2.0 * PT_PER_CSS_PX;
    let boxes: Vec<(f32, f32)> = pages[0].elements[5..]
        .iter()
        .map(|(_, element)| crate::layout::block::child_x_and_width(element.as_ref()))
        .collect();
    let expected = [((390.0 - half) / 2.0, half), (0.0, half)];
    for ((x, width), (expected_x, expected_width)) in boxes.iter().zip(expected) {
        assert!(
            (x - expected_x).abs() < 0.05 && (width - expected_width).abs() < 0.05,
            "hr boxes {boxes:?}"
        );
    }
}

/// The left edge of the text block holding `word`, from its containing block.
fn block_left(markup: &str, word: &str) -> f32 {
    use crate::layout::elements::{LayoutVisitor, TextBlock};
    struct Find<'a> {
        word: &'a str,
        left: Option<f32>,
    }
    impl LayoutVisitor for Find<'_> {
        fn visit_text_block(&mut self, block: &TextBlock) {
            let holds = block.lines.iter().any(|line| {
                line.runs
                    .iter()
                    .any(|run| run.text.split_whitespace().any(|w| w == self.word))
            });
            if holds {
                self.left = Some(block.positioning.insets.left);
            }
        }
    }
    let pages = super::support::layout_pages_at(markup, crate::types::PageSize::new(400.0, 400.0));
    let mut find = Find { word, left: None };
    for (_, element) in &pages[0].elements {
        crate::layout::engine::visit_layout_tree(element.as_ref(), &mut find);
    }
    find.left
        .unwrap_or_else(|| panic!("no text block holds {word:?}"))
}

#[test]
fn center_centers_its_text_and_blocks_narrower_than_it() {
    // Chrome: `center { text-align: -webkit-center }`, which centers a block
    // child without auto margins (Blink's `ResolveInlineAutoMargins`), by the
    // container's alignment and at any depth. 150pt blocks on a 400pt page.
    let cases = [
        (
            r#"<center><div style="width:150pt">plain</div></center>"#,
            "plain",
            125.0,
        ),
        (
            r#"<center><div style="width:150pt; text-align:left">own</div></center>"#,
            "own",
            125.0,
        ),
        (
            r#"<center><div style="width:150pt; margin-left:40pt">margin</div></center>"#,
            "margin",
            145.0,
        ),
        (
            r#"<center><div><div style="width:150pt">deep</div></div></center>"#,
            "deep",
            125.0,
        ),
        (
            r#"<div style="text-align:-webkit-center"><div style="width:150pt">value</div></div>"#,
            "value",
            125.0,
        ),
        (
            r#"<center><p style="width:150pt; margin:0 0 0 auto">auto</p></center>"#,
            "auto",
            250.0,
        ),
        (
            r#"<center><div style="text-align:left"><div style="width:150pt">left</div></div></center>"#,
            "left",
            0.0,
        ),
        (
            r#"<center><div style="width:150pt; float:left">float</div></center>"#,
            "float",
            0.0,
        ),
    ];
    for (markup, word, expected) in cases {
        let left = block_left(markup, word);
        assert!(
            (left - expected).abs() < 0.01,
            "{word}: expected {expected}pt, got {left}pt in {markup}"
        );
    }
    let center = crate::style::computed::compute_style(
        crate::parser::dom::HtmlTag::Center,
        None,
        &Default::default(),
    );
    assert_eq!(center.text_align, crate::style::computed::TextAlign::Center);
    assert!(center.text_align_webkit_center);
}

#[test]
fn hr_attributes_are_presentational_hints() {
    use crate::parser::dom::HtmlTag;
    use crate::style::computed::{BorderStyle, ComputedStyle, compute_style_with_context};
    use std::collections::HashMap;

    let hr = |attributes: &[(&str, &str)], inline_style: Option<&str>| {
        let attributes: HashMap<String, String> = attributes
            .iter()
            .map(|(name, value)| (name.to_string(), value.to_string()))
            .collect();
        compute_style_with_context(
            HtmlTag::Hr,
            inline_style,
            &ComputedStyle::default(),
            &[],
            "hr",
            &[],
            None,
            &attributes,
            &Default::default(),
        )
    };
    // Chrome's computed values for the same markup
    let left = hr(&[("align", "left")], Some("width: 200px"));
    assert!(left.margin.left == 0.0 && !left.margin_left_auto && left.margin_right_auto);
    assert_eq!(left.width, Some(150.0), "the author's width");
    let right = hr(&[("width", "200"), ("align", "right")], None);
    assert!(right.margin_left_auto && !right.margin_right_auto && right.margin.right == 0.0);
    assert_eq!(right.width, Some(150.0));
    let other = hr(&[("align", "bogus"), ("width", "120px")], None);
    assert!(other.margin_left_auto && other.margin_right_auto);
    assert_eq!(other.width, Some(90.0));
    let percent = hr(&[("width", "50%")], None);
    assert_eq!(percent.percentage_sizing.width, Some(50.0));

    let thin = hr(&[("size", "1")], None);
    assert_eq!(thin.border.bottom.used_width(), 0.0);
    assert_eq!(thin.border.top.used_width(), 0.75);
    let tall = hr(&[("size", "6")], None);
    assert_eq!(tall.height, Some(3.0), "4px inside two 1px borders");

    let noshade = hr(&[("noshade", "")], None);
    let gray = Color::rgb(128, 128, 128);
    assert_eq!(noshade.border.top.style, BorderStyle::Solid);
    assert_eq!(
        noshade.border.top.color,
        crate::parser::css::SpecifiedColor::Absolute(gray)
    );
    assert_eq!(noshade.background_color, Some(gray));
    let colored = hr(&[("noshade", ""), ("color", "#00f")], None);
    assert_eq!(colored.border.left.style, BorderStyle::Solid);
    assert_eq!(colored.background_color, Some(Color::rgb(0, 0, 255)));
    let bare_hex = hr(&[("color", "ff0000")], None);
    assert_eq!(bare_hex.background_color, Some(Color::rgb(255, 0, 0)));
    let plain = hr(&[], None);
    assert_eq!(plain.border.top.style, BorderStyle::Inset);
    assert_eq!(plain.background_color, None);
}
