//! `<progress>` and `<meter>` gauges, and atomic boxes that reach a line only
//! through an inline element.

use crate::layout::elements::{LayoutVisitor, ProgressBar, TextBlock};
use crate::layout::engine::{InlineBox, visit_layout_tree};

use super::support::layout_pages;

#[derive(Default)]
struct Gauges {
    /// Inline boxes in text lines, with the text of the line they sit on.
    inline: Vec<(InlineBox, String)>,
    /// Gauges laid out as blocks, not as the content of an inline box.
    blocks: usize,
    /// Every gauge, inline boxes' content included.
    bars: usize,
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
        self.bars += 1;
    }
}

fn gauges(markup: &str) -> Gauges {
    let mut gauges = Gauges::default();
    for page in layout_pages(markup) {
        for (_, element) in &page.elements {
            visit_layout_tree(element.as_ref(), &mut gauges);
        }
    }
    let inline_bars = gauges
        .inline
        .iter()
        .filter(|(inline, _)| gauge(inline).is_some())
        .count();
    gauges.blocks = gauges.bars - inline_bars;
    gauges
}

/// The gauge an inline box was laid out as.
fn gauge(inline: &InlineBox) -> Option<ProgressBar> {
    #[derive(Default)]
    struct Found(Option<ProgressBar>);
    impl LayoutVisitor for Found {
        fn visit_progress_bar(&mut self, bar: &ProgressBar) {
            self.0.get_or_insert_with(|| bar.clone());
        }
    }
    let mut found = Found::default();
    for node in &inline.content {
        visit_layout_tree(node.as_ref(), &mut found);
    }
    found.0
}

fn fill_fraction(inline: &InlineBox) -> Option<f32> {
    gauge(inline)
        .map(|bar| bar.fraction)
        .filter(|fraction| *fraction > 0.0)
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
    let bar = gauge(inline).expect("the box is the gauge");
    assert_eq!((bar.size.width, bar.size.height), (150.0, 12.0));
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
        // Its text is in its lines, or in the nodes its box was laid out as
        let inner: Vec<String> = found
            .inline
            .iter()
            .flat_map(|(inline, _)| {
                #[derive(Default)]
                struct Words(Vec<String>);
                impl LayoutVisitor for Words {
                    fn visit_text_block(&mut self, block: &TextBlock) {
                        let runs = block.lines.iter().flat_map(|line| &line.runs);
                        self.0.extend(runs.map(|run| run.text.clone()));
                    }
                }
                let mut nested = Words::default();
                for node in &inline.content {
                    visit_layout_tree(node.as_ref(), &mut nested);
                }
                inline
                    .lines
                    .iter()
                    .flat_map(|line| &line.runs)
                    .map(|run| run.text.clone())
                    .chain(nested.0)
                    .collect::<Vec<_>>()
            })
            .filter(|text| !text.trim().is_empty())
            .collect();
        assert_eq!(inner, ["nested"], "{markup}");
    }
}
