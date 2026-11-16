//! A collapsible space between two inline elements keeps the style of the
//! element whose text holds it (CSS Text 3 §4.1.1 keeps the first space of a
//! sequence): the end of the text before, a text node of only spaces between
//! them, or the start of the text after. Chrome sizes and kerns it that way, so
//! `AAA <span style="font-size:40px">BIG</span>` has a 12pt space, not a 40pt
//! one.

use crate::layout::elements::{LayoutVisitor, TextBlock};
use crate::types::PageSize;

use super::support::layout_pages_at;

/// The first line's runs: their text and font size.
fn runs(content: &str) -> Vec<(String, f32)> {
    struct Runs(Vec<(String, f32)>);
    impl LayoutVisitor for Runs {
        fn visit_text_block(&mut self, block: &TextBlock) {
            if let Some(line) = block.lines.first() {
                self.0.extend(
                    line.runs
                        .iter()
                        .filter(|run| !run.text.is_empty())
                        .map(|run| (run.text.clone(), run.font_size)),
                );
            }
        }
    }
    let mut found = Runs(Vec::new());
    for page in layout_pages_at(
        &format!(r#"<p style="margin:0;font:12pt Helvetica">{content}</p>"#),
        PageSize::new(400.0, 200.0),
    ) {
        for (_, element) in &page.elements {
            element.accept(&mut found);
        }
    }
    found.0
}

fn texts_and_sizes(pairs: &[(&str, f32)]) -> Vec<(String, f32)> {
    pairs
        .iter()
        .map(|(text, size)| ((*text).to_string(), *size))
        .collect()
}

#[test]
fn a_space_ending_the_text_before_an_element_stays_in_that_text() {
    assert_eq!(
        runs(r#"AAA <span style="font-size:30pt">BIG</span>"#),
        texts_and_sizes(&[("AAA ", 12.0), ("BIG", 30.0)])
    );
}

#[test]
fn a_space_starting_an_elements_text_is_that_elements() {
    assert_eq!(
        runs(r#"AAA<span style="font-size:30pt"> BIG</span>"#),
        texts_and_sizes(&[("AAA", 12.0), (" BIG", 30.0)])
    );
}

#[test]
fn a_space_between_two_elements_keeps_the_style_around_them() {
    assert_eq!(
        runs(r#"<b>AAA</b> <span style="font-size:30pt">BIG</span>"#),
        texts_and_sizes(&[("AAA", 12.0), (" ", 12.0), ("BIG", 30.0)])
    );
}

#[test]
fn a_space_before_a_padded_element_stays_in_the_text_before_it() {
    assert_eq!(
        runs(r#"AAA <span style="padding-left:3pt;font-size:30pt">BIG</span>"#),
        texts_and_sizes(&[("AAA ", 12.0), ("BIG", 30.0)])
    );
}
