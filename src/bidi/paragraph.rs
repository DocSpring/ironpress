//! A paragraph's bidi levels, resolved once over the text of all its lines and applied to each line. UAX #9 resolves
//! levels on the paragraph (§3.3) and applies only the line rules (L1, L2) per line (§3.4), so a number or a mark at
//! the edge of a line takes its level from the text around it on the lines before and after.

use std::ops::Range;

use unicode_bidi::{BidiClass, BidiInfo, Level};

use super::{BidiFrame, OBJECT_REPLACEMENT_CHARACTER, frames};
use crate::layout::engine::TextRun;

/// One character of the paragraph's text: where it is in the text and which line and run it came from.
pub(super) struct CharInfo {
    pub(super) ch: char,
    pub(super) byte_offset: usize,
    pub(super) run_idx: usize,
    pub(super) object: bool,
    /// The run's last character: its piece owns the run's boundary advance.
    pub(super) ends_run: bool,
}

/// The text of a paragraph's lines in logical order, with each inline box's bracket around its text (`BidiFrame`).
/// Atomic inline boxes take part as an object replacement character but keep their empty authored text.
pub(super) struct ParagraphText {
    pub(super) text: String,
    pub(super) chars: Vec<CharInfo>,
    /// Each line's bytes of `text`, from its first bracket or character to the next line's.
    line_bytes: Vec<Range<usize>>,
    /// Each line's entries in `chars`.
    line_chars: Vec<Range<usize>>,
}

impl ParagraphText {
    pub(super) fn new(lines: &[&[TextRun]], paragraph_rtl: bool, bidi_override: bool) -> Self {
        let mut text = String::new();
        let mut chars = Vec::new();
        let mut line_starts = Vec::with_capacity(lines.len());
        let mut line_char_starts = Vec::with_capacity(lines.len());
        if bidi_override {
            text.push(if paragraph_rtl {
                '\u{202e}'
            } else {
                '\u{202d}'
            });
        }
        let mut open_frames: &[BidiFrame] = &[];
        for (line_index, runs) in lines.iter().enumerate() {
            line_starts.push(if line_index == 0 { 0 } else { text.len() });
            line_char_starts.push(chars.len());
            for (run_idx, run) in runs.iter().enumerate() {
                frames::push_transition(&mut text, open_frames, &run.bidi_frames);
                open_frames = &run.bidi_frames;
                if run.text.is_empty() && run.inline_box.is_some() {
                    chars.push(CharInfo {
                        ch: OBJECT_REPLACEMENT_CHARACTER,
                        byte_offset: text.len(),
                        run_idx,
                        object: true,
                        ends_run: true,
                    });
                    text.push(OBJECT_REPLACEMENT_CHARACTER);
                    continue;
                }
                let count = run.text.chars().count();
                for (index, ch) in run.text.chars().enumerate() {
                    chars.push(CharInfo {
                        ch,
                        byte_offset: text.len(),
                        run_idx,
                        object: false,
                        ends_run: index + 1 == count,
                    });
                    text.push(ch);
                }
            }
        }
        frames::push_transition(&mut text, open_frames, &[]);
        let line_bytes = ranges_from_starts(&line_starts, text.len());
        let line_chars = ranges_from_starts(&line_char_starts, chars.len());
        Self {
            text,
            chars,
            line_bytes,
            line_chars,
        }
    }

    pub(super) fn line_chars(&self, line: usize) -> Range<usize> {
        self.line_chars[line].clone()
    }

    /// Line `line`'s visual runs: byte ranges of `text` in left-to-right order on the page, each with whether it's
    /// at a right-to-left level. A line that crosses a paragraph separator (U+2029) has each paragraph's part in
    /// turn.
    pub(super) fn visual_runs(
        &self,
        info: &BidiInfo<'_>,
        line: usize,
    ) -> Vec<(Range<usize>, bool)> {
        let bytes = self.line_bytes[line].clone();
        info.paragraphs
            .iter()
            .filter_map(|paragraph| {
                let part =
                    bytes.start.max(paragraph.range.start)..bytes.end.min(paragraph.range.end);
                (part.start < part.end).then(|| line_visual_runs(info, paragraph.level, part))
            })
            .flatten()
            .map(|(range, level)| (range, level.is_rtl()))
            .collect()
    }
}

fn ranges_from_starts(starts: &[usize], end: usize) -> Vec<Range<usize>> {
    starts
        .iter()
        .enumerate()
        .map(|(index, &start)| start..starts.get(index + 1).copied().unwrap_or(end))
        .collect()
}

/// The runs of one level in `line`, in visual order, from levels resolved on the whole paragraph (UAX #9 L1, L2).
/// Works on the line's own bytes, so wrapping a long paragraph stays linear.
fn line_visual_runs(
    info: &BidiInfo<'_>,
    paragraph_level: Level,
    line: Range<usize>,
) -> Vec<(Range<usize>, Level)> {
    let classes = &info.original_classes[line.clone()];
    let mut levels = info.levels[line.clone()].to_vec();
    // L1: separators, and whitespace and isolate controls before a separator or at the end of the line, take the
    // paragraph's level. Characters X9 removed (embedding controls, BN) don't end such a sequence.
    let mut trailing = true;
    for (class, level) in classes.iter().zip(levels.iter_mut()).rev() {
        match class {
            BidiClass::S | BidiClass::B => {
                *level = paragraph_level;
                trailing = true;
            }
            BidiClass::WS
            | BidiClass::FSI
            | BidiClass::LRI
            | BidiClass::RLI
            | BidiClass::PDI
            | BidiClass::BN
            | BidiClass::LRE
            | BidiClass::RLE
            | BidiClass::LRO
            | BidiClass::RLO
            | BidiClass::PDF => {
                if trailing {
                    *level = paragraph_level;
                }
            }
            _ => trailing = false,
        }
    }
    let mut runs: Vec<(Range<usize>, Level)> = Vec::new();
    for (offset, &level) in levels.iter().enumerate() {
        let byte = line.start + offset;
        match runs.last_mut() {
            Some((range, run_level)) if *run_level == level => range.end = byte + 1,
            _ => runs.push((byte..byte + 1, level)),
        }
    }
    // L2: from the highest level down to the lowest odd one, reverse every sequence of runs at that level or higher
    let highest = runs.iter().map(|(_, level)| level.number()).max();
    let lowest_odd = runs
        .iter()
        .map(|(_, level)| level.number())
        .filter(|level| level % 2 == 1)
        .min();
    if let (Some(highest), Some(lowest_odd)) = (highest, lowest_odd) {
        for at_least in (lowest_odd..=highest).rev() {
            let mut index = 0;
            while index < runs.len() {
                if runs[index].1.number() < at_least {
                    index += 1;
                    continue;
                }
                let start = index;
                while index < runs.len() && runs[index].1.number() >= at_least {
                    index += 1;
                }
                runs[start..index].reverse();
            }
        }
    }
    runs
}

#[cfg(test)]
mod tests {
    use crate::bidi::reorder_lines_bidi;
    use crate::layout::engine::TextRun;

    fn run(text: &str) -> TextRun {
        TextRun {
            text: text.to_string(),
            ..Default::default()
        }
    }

    fn texts(runs: &[TextRun]) -> Vec<&str> {
        runs.iter().map(|run| run.text.as_str()).collect()
    }

    #[test]
    fn a_comma_at_the_end_of_a_line_stays_with_the_english_it_follows() {
        // Right-to-left paragraph, wrapped after "hello,": the comma is between "hello" and "world" (W4 doesn't
        // apply, N1: L on both sides), so it's at the English level and drawn right after "hello". Taken as a
        // paragraph of its own, the line ends at the comma, which then takes the paragraph's direction and moved
        // to the far left of the line.
        let first = [run("שלום hello,")];
        let second = [run(" world עולם")];
        let lines = reorder_lines_bidi(&[&first, &second], true, false);
        assert_eq!(texts(&lines[0]), vec!["hello,", "שלום "]);
        // The space between "world" and "עולם" is between English and Hebrew in a right-to-left paragraph (N2), so
        // it's in the Hebrew's range, a piece of its own as it takes the Latin script before it (UAX #24)
        assert_eq!(texts(&lines[1]), vec!["עולם", " ", " world"]);
    }

    #[test]
    fn a_number_starting_a_line_keeps_the_level_of_the_hebrew_before_it() {
        // `Ship: רחוב הרצל 12 תל אביב` wrapped before "12": the number follows Hebrew, so it stays a European
        // number (W2, W7) inside the right-to-left address and is drawn at its right end, after "תל אביב".
        let first = [run("Ship: רחוב הרצל ")];
        let second = [run("12 תל אביב")];
        let lines = reorder_lines_bidi(&[&first, &second], false, false);
        assert_eq!(texts(&lines[1]), vec![" תל אביב", "12"]);
        let alone = reorder_lines_bidi(&[&second], false, false);
        assert_eq!(texts(&alone[0]), vec!["12 ", "תל אביב"]);
    }

    #[test]
    fn text_after_a_paragraph_separator_is_kept() {
        let line = [run("Hello שלום\u{2029}World after separator")];
        let lines = reorder_lines_bidi(&[&line], false, false);
        let drawn: String = lines[0].iter().map(|run| run.text.as_str()).collect();
        assert_eq!(drawn.chars().count(), line[0].text.chars().count());
        assert!(drawn.contains("World after separator"));
        assert!(drawn.starts_with("Hello "));
    }

    #[test]
    fn each_paragraph_in_a_line_is_reordered_in_turn() {
        // A right-to-left paragraph: each paragraph's text is reordered within its own part of the line. The
        // separator ends the first paragraph and is at its level (L1), so it's drawn at that paragraph's left end.
        let line = [run("אחד one\u{2029}שתיים two")];
        let lines = reorder_lines_bidi(&[&line], true, false);
        assert_eq!(
            texts(&lines[0]),
            vec!["\u{2029}", "one", "אחד ", "two", "שתיים "]
        );
    }

    #[test]
    fn a_line_of_only_inline_boxes_or_empty_runs_is_unchanged() {
        let empty = [run("")];
        let text = [run("שלום")];
        let lines = reorder_lines_bidi(&[&empty, &text, &[]], true, false);
        assert_eq!(texts(&lines[0]), vec![""]);
        assert_eq!(texts(&lines[1]), vec!["שלום"]);
        assert!(lines[2].is_empty());
    }
}
