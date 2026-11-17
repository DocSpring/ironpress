//! Inline boxes in the bidi algorithm (CSS Writing Modes 4 §2.4, HTML
//! §15.3.5): `dir` isolates an inline element's text, `<bdi>` isolates it in
//! its own direction, `<bdo>` overrides it, and `unicode-bidi` brackets it.
//! Each expectation is the visual order Chrome lays out for the same markup.

use crate::layout::elements::{LayoutVisitor, TextBlock};
use crate::layout::engine::{Page, TextLine, visit_layout_tree};
use crate::types::PageSize;

use super::support::layout_pages_at;

fn page(markup: &str) -> Page {
    layout_pages_at(
        &format!(
            "<style>body {{ margin: 0; font: 12pt/18pt sans-serif }} p {{ margin: 0 }}</style>\
             {markup}"
        ),
        PageSize::new(400.0, 300.0),
    )
    .into_iter()
    .next()
    .expect("one page")
}

#[derive(Default)]
struct Lines(Vec<TextLine>);

impl LayoutVisitor for Lines {
    fn visit_text_block(&mut self, block: &TextBlock) {
        self.0.extend(block.lines.iter().cloned());
    }
}

fn lines(markup: &str) -> Vec<TextLine> {
    let page = page(markup);
    let mut lines = Lines::default();
    for (_, element) in &page.elements {
        visit_layout_tree(element.as_ref(), &mut lines);
    }
    lines.0
}

/// A line's text as it reads on the page, left to right: each right-to-left
/// piece's characters reversed, as it is drawn.
fn visual(line: &TextLine) -> String {
    line.runs
        .iter()
        .map(|run| {
            if run.metadata.bidi_rtl {
                run.text.chars().rev().collect()
            } else {
                run.text.clone()
            }
        })
        .collect()
}

fn first_line(markup: &str) -> String {
    let lines = lines(markup);
    visual(lines.first().expect("a line"))
}

#[test]
fn bdi_isolates_a_right_to_left_name_from_the_number_after_it() {
    assert_eq!(
        first_line("<p>User <bdi>إيان</bdi>: 3 posts.</p>"),
        "User نايإ: 3 posts."
    );
    // Without the isolate the number joins the name's right-to-left run.
    assert_eq!(
        first_line("<p>User <span>إيان</span>: 3 posts.</p>"),
        "User 3 :نايإ posts."
    );
}

#[test]
fn dir_isolates_an_inline_elements_text() {
    assert_eq!(
        first_line(r#"<p>User <span dir="rtl">إيان</span>: 3 posts.</p>"#),
        "User نايإ: 3 posts."
    );
    assert_eq!(
        first_line(r#"<p dir="rtl">עברית <span dir="ltr">C++</span> עוד</p>"#),
        "דוע C++ תירבע"
    );
}

#[test]
fn bdo_lays_its_text_out_in_its_direction() {
    assert_eq!(
        first_line(r#"<p>Say <bdo dir="rtl">Hello world</bdo> now.</p>"#),
        "Say dlrow olleH now."
    );
    assert_eq!(first_line(r#"<p><bdo dir="ltr">שלום</bdo>.</p>"#), "שלום.");
}

#[test]
fn sibling_bdo_elements_are_reversed_separately() {
    assert_eq!(
        first_line(r#"<p><bdo dir="rtl">ab</bdo><bdo dir="rtl">cd</bdo>.</p>"#),
        "badc."
    );
}

#[test]
fn unicode_bidi_values_bracket_an_inline_elements_text() {
    let span = |css: &str| {
        first_line(&format!(
            r#"<p>X <span style="{css}">abc, def!</span> Y</p>"#
        ))
    };
    assert_eq!(
        span("unicode-bidi:bidi-override;direction:rtl"),
        "X !fed ,cba Y"
    );
    assert_eq!(span("unicode-bidi:embed;direction:rtl"), "X !abc, def Y");
    assert_eq!(span("unicode-bidi:isolate;direction:rtl"), "X !abc, def Y");
    // `direction` alone doesn't change an inline element's text.
    assert_eq!(span("direction:rtl"), "X abc, def! Y");
}

#[test]
fn an_auto_direction_takes_the_first_strong_characters_direction() {
    let expected = "X !abc ,םולש Y";
    assert_eq!(
        first_line(r#"<p>X <span dir="auto">שלום, abc!</span> Y</p>"#),
        expected
    );
    assert_eq!(first_line("<p>X <bdi>שלום, abc!</bdi> Y</p>"), expected);
    assert_eq!(
        first_line(r#"<p>X <span style="unicode-bidi:plaintext">שלום, abc!</span> Y</p>"#),
        expected
    );
}

#[test]
fn nested_brackets_nest() {
    assert_eq!(
        first_line(
            r#"<p>A <span dir="rtl">שלום <span dir="ltr">Hello, 12!</span> עולם</span> Z.</p>"#
        ),
        "A םלוע Hello, 12! םולש Z."
    );
    assert_eq!(
        first_line(
            r#"<p>A <bdo dir="rtl">xy <b>bold</b> <bdo dir="ltr">kept</bdo> end</bdo> Z.</p>"#
        ),
        "A dne kept dlob yx Z."
    );
}

#[test]
fn elements_without_a_display_of_their_own_are_inline() {
    let lines = lines(
        "<p>Press <kbd>Ctrl</kbd> and <time>today</time> or <my-widget>this</my-widget>.</p>",
    );
    assert_eq!(lines.len(), 1, "one line");
    assert_eq!(visual(&lines[0]), "Press Ctrl and today or this.");
}

#[test]
fn hidden_and_unrendered_elements_take_no_room() {
    let lines = lines(
        "<p>a</p><p hidden>hidden</p><noscript>noscript</noscript><datalist><option>x</option></datalist>",
    );
    let text: Vec<String> = lines.iter().map(visual).collect();
    assert_eq!(text, vec!["a".to_string()]);
}
