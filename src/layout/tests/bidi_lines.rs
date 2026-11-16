//! A paragraph with right-to-left text is wrapped in logical order and each
//! line is then put in visual order (UAX #9 §3.4), as Chrome lays it out: a
//! right-to-left paragraph's first words are on its first line, whatever the
//! numbers, Latin words or inline elements among them. Its direction is its
//! `direction` (or `dir`), never inferred from its text. The parity fixture
//! `text-advanced-bidi-mixed-lines` holds Chrome's output.

use crate::layout::elements::{LayoutVisitor, TextBlock};
use crate::types::PageSize;

use super::support::layout_pages_at;

/// Each line's runs joined in visual order (each run's text is in logical
/// order), and the line's x offset.
fn lines(markup: &str) -> Vec<(String, f32)> {
    struct Lines(Vec<(String, f32)>);
    impl LayoutVisitor for Lines {
        fn visit_text_block(&mut self, block: &TextBlock) {
            for line in &block.lines {
                let text = line.runs.iter().map(|run| run.text.as_str()).collect();
                self.0.push((text, line.x_offset));
            }
        }
    }
    let mut found = Lines(Vec::new());
    let pages = layout_pages_at(
        &format!(
            "<style>body{{margin:0;font:15pt Helvetica;line-height:22.5pt}} p{{margin:0}}</style>{markup}"
        ),
        PageSize::new(240.0, 200.0),
    );
    for (_, element) in &pages[0].elements {
        element.accept(&mut found);
    }
    found.0
}

#[test]
fn a_right_to_left_paragraph_with_a_number_starts_with_its_first_words() {
    let lines = lines(r#"<p dir="rtl">אחד שתיים שלוש 120 ארבע חמש שש שבע שמונה תשע עשר</p>"#);
    assert_eq!(lines.len(), 2, "{lines:?}");
    assert!(
        lines[0].0.contains("אחד") && lines[0].0.contains("120"),
        "{lines:?}"
    );
    assert!(
        lines[1].0.contains("עשר") && !lines[1].0.contains("אחד"),
        "{lines:?}"
    );
}

#[test]
fn a_right_to_left_paragraph_with_inline_elements_wraps_in_text_order() {
    let lines = lines(
        r#"<p dir="rtl">אחד שתיים <b>שלוש</b> ארבע <i>חמש שש</i> שבע שמונה תשע עשר אחד עשר</p>"#,
    );
    assert_eq!(lines.len(), 2, "{lines:?}");
    for word in ["אחד", "שתיים", "שלוש", "ארבע", "חמש"] {
        assert!(
            lines[0].0.contains(word),
            "{word} on the first line: {lines:?}"
        );
    }
    for word in ["תשע", "עשר"] {
        assert!(
            lines[1].0.contains(word),
            "{word} on the second line: {lines:?}"
        );
    }
}

#[test]
fn a_hebrew_paragraph_without_dir_is_laid_out_left_to_right() {
    let lines = lines("<p>שלום AV</p>");
    assert_eq!(lines.len(), 1);
    assert_eq!(lines[0].1, 0.0, "it starts at the left edge");
    assert!(lines[0].0.starts_with("שלום"), "{lines:?}");
}
