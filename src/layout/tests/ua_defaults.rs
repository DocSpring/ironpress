//! Chromium's UA stylesheet for text-level and grouping elements, and the
//! font-size rules it depends on: keyword sizes, `smaller`/`larger`, and the
//! 13px that the generic `monospace` family gets where no absolute length set
//! the size. Each expectation is Chrome's computed value for the same markup
//! (Chrome 156, default 16px proportional and 13px monospace sizes).

use crate::layout::engine::TextRun;
use crate::style::computed::{FontStyle, TextDecorationStyle};
use crate::types::Color;

use super::support::{layout_pages, visible_runs};

const PT_PER_PX: f32 = 0.75;

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
    run(runs, word).font_size / PT_PER_PX
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
        .map(|(top, _)| top / PT_PER_PX)
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
    let half = 390.0 / 2.0 + 2.0 * PT_PER_PX;
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
