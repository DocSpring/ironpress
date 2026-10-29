//! `<progress>` and `<meter>` gauges, and atomic boxes that reach a line only
//! through an inline element.

use crate::layout::elements::{LayoutVisitor, ProgressBar, TextBlock};
use crate::layout::engine::{InlineBox, visit_layout_tree};

use super::support::layout_pages;

#[derive(Default)]
struct Gauges {
    /// Inline boxes in text lines, with the text of the line they sit on.
    inline: Vec<(InlineBox, String)>,
    blocks: usize,
}

impl LayoutVisitor for Gauges {
    fn visit_text_block(&mut self, block: &TextBlock) {
        for line in &block.lines {
            let text: String = line.runs.iter().map(|run| run.text.as_str()).collect();
            let text = text.split_whitespace().collect::<Vec<_>>().join(" ");
            for run in &line.runs {
                if let Some(inline) = &run.inline_box {
                    self.inline
                        .push(((**inline).clone(), text.trim().to_string()));
                }
            }
        }
    }

    fn visit_progress_bar(&mut self, _bar: &ProgressBar) {
        self.blocks += 1;
    }
}

fn gauges(markup: &str) -> Gauges {
    let mut gauges = Gauges::default();
    for page in layout_pages(markup) {
        for (_, element) in &page.elements {
            visit_layout_tree(element.as_ref(), &mut gauges);
        }
    }
    gauges
}

fn fill_fraction(inline: &InlineBox) -> Option<f32> {
    inline.paint.gauge_fill.map(|fill| fill.fraction)
}

#[test]
fn progress_and_meter_sit_in_their_line_of_text() {
    let found = gauges(
        r#"<p>Done <progress value="1" max="4"></progress> and
           <meter value="3" max="4"></meter> used</p>"#,
    );

    assert_eq!(found.blocks, 0, "inline gauges must not become block bars");
    let fractions: Vec<_> = found
        .inline
        .iter()
        .map(|(inline, _)| fill_fraction(inline))
        .collect();
    assert_eq!(fractions, [Some(0.25), Some(0.75)]);
    for (inline, line_text) in &found.inline {
        assert_eq!((inline.width, inline.height), (150.0, 12.0));
        assert_eq!(line_text, "Done and used");
    }
}

#[test]
fn empty_progress_keeps_its_track() {
    let found = gauges(r#"<p>Not started <progress value="0" max="1"></progress></p>"#);

    assert_eq!(found.inline.len(), 1);
    let (inline, _) = &found.inline[0];
    assert_eq!(fill_fraction(inline), None);
    assert!(inline.paint.background_color.is_some());
}

#[test]
fn progress_before_a_block_child_of_a_painted_box_is_kept() {
    let found = gauges(
        r#"<div style="background:#eee"><progress value="1" max="2"></progress><p>y</p></div>"#,
    );

    assert_eq!(found.blocks, 0);
    assert_eq!(found.inline.len(), 1);
    assert_eq!(fill_fraction(&found.inline[0].0), Some(0.5));
}

#[test]
fn block_progress_keeps_its_block_bar() {
    let found = gauges(r#"<progress style="display:block" value="1" max="2"></progress>"#);

    assert_eq!(found.blocks, 1);
    assert!(found.inline.is_empty());
}

#[test]
fn inline_block_inside_an_inline_element_of_a_painted_box_is_kept() {
    for markup in [
        r#"<div style="background:#eee"><span><b style="display:inline-block">nested</b></span></div>"#,
        r#"<div style="background:#eee"><span><b style="display:inline-block">nested</b></span><p>y</p></div>"#,
    ] {
        let found = gauges(markup);
        let inner: Vec<String> = found
            .inline
            .iter()
            .flat_map(|(inline, _)| &inline.lines)
            .flat_map(|line| &line.runs)
            .map(|run| run.text.clone())
            .collect();
        assert_eq!(inner, ["nested"], "{markup}");
    }
}
