//! Reordering wrapped lines for display (UAX #9). Lines are wrapped in logical order first, then each bidi paragraph's
//! levels are resolved over all its lines and each line is reordered on its own (UAX #9 §3.4).

use std::collections::HashMap;
use std::ops::Range;

use crate::layout::engine::{TextLine, TextRun};
use crate::parser::ttf::TtfFont;

/// How a paragraph's lines are reordered: in its `direction`, and strictly in sequence under
/// `unicode-bidi: bidi-override`. Reordering happens after wrapping (UAX #9 §3.4): reordering first would wrap the
/// paragraph in visual order, so a right-to-left paragraph's later words came first.
#[derive(Clone, Copy)]
pub(super) struct LineBidi {
    pub(super) rtl: bool,
    pub(super) bidi_override: bool,
}

/// Where the bidi paragraphs of a block's wrapped lines end. A forced break is a line feed (HTML renders `<br>` as
/// one, and `white-space: pre` keeps the text's own), which UAX #9 treats as a paragraph separator (class B), so the
/// text after it doesn't take levels from the text before it.
#[derive(Debug, Default)]
pub(super) struct BidiParagraphs {
    /// Line counts after which a paragraph ends, in increasing order.
    ends: Vec<usize>,
}

impl BidiParagraphs {
    /// A forced break ended the paragraph after the first `line_count` lines.
    pub(super) fn end_after(&mut self, line_count: usize) {
        if self.ends.last() != Some(&line_count) {
            self.ends.push(line_count);
        }
    }

    fn ranges(&self, line_count: usize) -> Vec<Range<usize>> {
        let mut ranges = Vec::with_capacity(self.ends.len() + 1);
        let mut start = 0;
        for &end in self.ends.iter().chain(std::iter::once(&line_count)) {
            let end = end.min(line_count);
            if end > start {
                ranges.push(start..end);
                start = end;
            }
        }
        ranges
    }

    /// Reorders each paragraph's lines in the block's direction.
    pub(super) fn reorder(&self, lines: &mut [TextLine], bidi: LineBidi) {
        for range in self.ranges(lines.len()) {
            reorder_lines(&mut lines[range], bidi.rtl, bidi.bidi_override);
        }
    }

    /// `unicode-bidi: plaintext`: each paragraph takes its direction from its first strong character (UAX #9 P2, P3)
    /// and starts on that side of the line.
    pub(super) fn reorder_plaintext(
        &self,
        lines: &mut [TextLine],
        max_width: f32,
        fonts: &HashMap<String, TtfFont>,
    ) {
        for range in self.ranges(lines.len()) {
            let paragraph = &mut lines[range];
            let text: String = paragraph
                .iter()
                .flat_map(|line| line.runs.iter())
                .map(|run| run.text.as_str())
                .collect();
            if text.is_empty() {
                continue;
            }
            let rtl = crate::bidi::first_strong_is_rtl(&text);
            let reorders = rtl
                || crate::bidi::has_rtl_chars(&text)
                || paragraph
                    .iter()
                    .any(|line| crate::bidi::inline_brackets_reorder(&line.runs));
            if reorders {
                reorder_lines(paragraph, rtl, false);
            }
            if rtl && max_width.is_finite() {
                for line in paragraph.iter_mut() {
                    let width = crate::layout::helpers::measure_runs_width(&line.runs, fonts);
                    line.x_offset += (max_width - width).max(0.0);
                }
            }
        }
    }
}

fn reorder_lines(lines: &mut [TextLine], rtl: bool, bidi_override: bool) {
    let runs: Vec<&[TextRun]> = lines.iter().map(|line| line.runs.as_slice()).collect();
    let reordered = crate::bidi::reorder_lines_bidi(&runs, rtl, bidi_override);
    for (line, runs) in lines.iter_mut().zip(reordered) {
        line.runs = runs;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn forced_breaks_split_the_lines_into_paragraphs() {
        let mut paragraphs = BidiParagraphs::default();
        assert_eq!(paragraphs.ranges(3), vec![0..3]);
        paragraphs.end_after(1);
        paragraphs.end_after(1);
        paragraphs.end_after(2);
        assert_eq!(paragraphs.ranges(4), vec![0..1, 1..2, 2..4]);
        // A break that ends the last line leaves no empty paragraph after it
        paragraphs.end_after(4);
        assert_eq!(paragraphs.ranges(4), vec![0..1, 1..2, 2..4]);
    }

    fn line(text: &str) -> TextLine {
        TextLine {
            runs: vec![TextRun {
                text: text.to_string(),
                ..Default::default()
            }],
            ..Default::default()
        }
    }

    fn texts(line: &TextLine) -> Vec<&str> {
        line.runs.iter().map(|run| run.text.as_str()).collect()
    }

    #[test]
    fn a_forced_break_ends_the_bidi_paragraph() {
        // After a `<br>`, "12" starts a paragraph: it follows no strong character, so it takes the paragraph's
        // left-to-right direction (W7) and is drawn first. In the same paragraph as the Hebrew before the break,
        // it would be part of the right-to-left text and drawn last.
        let bidi = LineBidi {
            rtl: false,
            bidi_override: false,
        };
        let mut joined = vec![line("שלום"), line("12 עולם")];
        BidiParagraphs::default().reorder(&mut joined, bidi);
        assert_eq!(texts(&joined[1]), vec![" עולם", "12"]);

        let mut broken = vec![line("שלום"), line("12 עולם")];
        let mut paragraphs = BidiParagraphs::default();
        paragraphs.end_after(1);
        paragraphs.reorder(&mut broken, bidi);
        assert_eq!(texts(&broken[1]), vec!["12 ", "עולם"]);
    }

    #[test]
    fn a_plaintext_paragraph_takes_its_direction_from_its_first_strong_character() {
        // The second line of a Hebrew paragraph starts with English but is still right to left
        let mut lines = vec![line("שלום עולם"), line("hello שוב")];
        BidiParagraphs::default().reorder_plaintext(&mut lines, f32::INFINITY, &HashMap::new());
        assert_eq!(texts(&lines[1]), vec!["שוב", " ", "hello"]);
    }
}
