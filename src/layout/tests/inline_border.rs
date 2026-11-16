//! A non-replaced inline box's border takes room in the line, its left and
//! right widths with the padding and margin, and each line fragment paints its
//! own piece (CSS 2.1 §8.6, §9.4.2): the top and bottom on every fragment, the
//! left side only on the fragment that holds the box's start and the right side
//! only on the one that holds its end. The parity fixture
//! `inline-text-inline-border` holds Chrome's output.

use crate::layout::elements::{LayoutVisitor, TextBlock};
use crate::layout::engine::TextRun;
use crate::style::computed::FontFamily;
use crate::types::PageSize;

use super::support::layout_pages_at;

/// Each line's runs.
fn lines(content: &str, width: f32) -> Vec<Vec<TextRun>> {
    struct Lines(Vec<Vec<TextRun>>);
    impl LayoutVisitor for Lines {
        fn visit_text_block(&mut self, block: &TextBlock) {
            self.0
                .extend(block.lines.iter().map(|line| line.runs.clone()));
        }
    }
    let mut found = Lines(Vec::new());
    for page in layout_pages_at(
        &format!(r#"<p style="margin:0;width:{width}pt;font:12pt Helvetica">{content}</p>"#),
        PageSize::new(400.0, 200.0),
    ) {
        for (_, element) in &page.elements {
            element.accept(&mut found);
        }
    }
    found.0
}

fn text_width(text: &str) -> f32 {
    crate::fonts::str_width(text, 12.0, &FontFamily::Helvetica, false)
}

/// Each text run's text and its borders' (show left, show right), innermost
/// first.
fn shown_sides(line: &[TextRun]) -> Vec<(String, Vec<(bool, bool)>)> {
    line.iter()
        .filter(|run| !run.text.is_empty())
        .map(|run| {
            (
                run.text.clone(),
                run.inline_borders
                    .iter()
                    .map(|border| (border.show_left, border.show_right))
                    .collect(),
            )
        })
        .collect()
}

#[test]
fn the_left_and_right_borders_take_room_in_the_line() {
    let line = &lines(
        r#"A <span style="border:3pt solid red;padding:0 2pt">B</span> C"#,
        300.0,
    )[0];
    let advance: f32 = line
        .iter()
        .map(|run| {
            run.atomic_inline_advance()
                .unwrap_or_else(|| text_width(&run.text))
        })
        .sum();
    let expected = text_width("A B C") + 2.0 * (3.0 + 2.0);
    assert!(
        (advance - expected).abs() < 0.01,
        "the line advances {advance}, the text and both edges {expected}"
    );
}

#[test]
fn a_wrapped_box_shows_its_left_side_on_the_first_line_and_its_right_on_the_last() {
    let lines = lines(
        r#"One <span style="border:2pt solid red">alpha beta gamma delta</span> end"#,
        80.0,
    );
    assert!(lines.len() >= 2, "the box wraps: {}", lines.len());
    let first = shown_sides(&lines[0]);
    let last = shown_sides(lines.last().expect("a last line"));
    let boxed = |sides: &[(String, Vec<(bool, bool)>)]| -> Vec<(bool, bool)> {
        sides
            .iter()
            .filter(|(_, borders)| !borders.is_empty())
            .map(|(_, borders)| borders[0])
            .collect()
    };
    let first = boxed(&first);
    let last = boxed(&last);
    assert_eq!(
        first.first(),
        Some(&(true, false)),
        "the box starts on the first line"
    );
    assert!(first.iter().all(|sides| !sides.1), "and doesn't end there");
    assert_eq!(
        last.last(),
        Some(&(false, true)),
        "the box ends on the last line"
    );
    assert!(last.iter().all(|sides| !sides.0), "and doesn't start there");
}

#[test]
fn text_in_nested_boxes_carries_both_borders() {
    let line = &lines(
        r#"A <span style="border:2pt solid red"><span style="border:1pt dashed blue">in</span> out</span> B"#,
        300.0,
    )[0];
    let sides = shown_sides(line);
    let inner = sides
        .iter()
        .find(|(text, _)| text.contains("in"))
        .expect("the inner box's text");
    assert_eq!(inner.1.len(), 2, "the inner border, then the outer one");
    assert_eq!(
        inner.1[0],
        (true, true),
        "the inner box starts and ends on the line"
    );
    let outer = sides
        .iter()
        .find(|(text, _)| text.contains("out"))
        .expect("the outer box's own text");
    assert_eq!(
        outer.1,
        vec![(false, true)],
        "the outer box ends after its own text"
    );
}

#[test]
fn a_box_without_a_border_gives_its_runs_none() {
    let line = &lines(r#"A <span style="padding:0 2pt">B</span> C"#, 300.0)[0];
    assert!(line.iter().all(|run| run.inline_borders.is_empty()));
}
