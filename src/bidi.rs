//! Bidirectional text reordering (UAX #9) for mixed LTR/RTL content.

use crate::layout::engine::{ShapingDirection, TextRun};
use unicode_bidi::{BidiInfo, Level};
use unicode_script::{Script, UnicodeScript};

const OBJECT_REPLACEMENT_CHARACTER: char = '\u{fffc}';

/// Reorder text runs according to the Unicode Bidirectional Algorithm.
///
/// Takes a list of text runs in logical order and returns them split into
/// one run per level-run, arranged in visual left-to-right order. Each
/// resulting run's text is kept in LOGICAL order (not pre-reversed) so that
/// the downstream text shaper (rustybuzz, via `guess_segment_properties`)
/// can apply the correct RTL shaping itself. Pre-reversing here would cause
/// rustybuzz to reverse a second time, undoing the bidi reorder.
pub(crate) fn reorder_runs_bidi(
    runs: &[TextRun],
    paragraph_rtl: bool,
    bidi_override: bool,
) -> Vec<TextRun> {
    if runs.is_empty() {
        return Vec::new();
    }

    let mut full_text = String::new();
    for run in runs {
        if run.text.is_empty() && run.inline_box.is_some() {
            full_text.push(OBJECT_REPLACEMENT_CHARACTER);
        } else {
            full_text.push_str(&run.text);
        }
    }
    if full_text.is_empty() {
        return runs.to_vec();
    }

    // `unicode-bidi: bidi-override`: the characters' intrinsic bidi classes are
    // ignored and the content is laid out strictly in sequence according to
    // `direction` (css-writing-modes-4 §2.4): every character is at the
    // override's level, so each run is shaped in that direction with its text
    // in logical order, and for `rtl` the runs are in reverse order.
    if bidi_override {
        return override_runs(runs, paragraph_rtl);
    }

    let default_level = if paragraph_rtl {
        Level::rtl()
    } else {
        Level::ltr()
    };
    let bidi_info = BidiInfo::new(&full_text, Some(default_level));

    if bidi_info.paragraphs.is_empty() {
        return runs.to_vec();
    }

    let para = &bidi_info.paragraphs[0];
    let line = para.range.clone();

    // Get visual runs: `vis_levels` is per-byte resolved levels; `vis_ranges`
    // are byte ranges already in visual (left-to-right) order. We look up a
    // run's level via its starting byte.
    let (vis_levels, vis_ranges) = bidi_info.visual_runs(para, line);

    // Check if purely LTR — return unchanged
    if vis_ranges.len() == 1
        && vis_ranges[0].start < vis_levels.len()
        && vis_levels[vis_ranges[0].start].is_ltr()
    {
        return runs.to_vec();
    }

    // Build per-char mapping. Atomic inline boxes participate in UAX #9 as an
    // object replacement character, but retain their empty authored text.
    struct CharInfo {
        ch: char,
        byte_offset: usize,
        run_idx: usize,
        object: bool,
        /// The run's last character: its piece owns the run's boundary advance.
        ends_run: bool,
    }
    let mut char_info: Vec<CharInfo> = Vec::new();
    let mut byte_offset = 0;
    for (run_idx, run) in runs.iter().enumerate() {
        if run.text.is_empty() && run.inline_box.is_some() {
            char_info.push(CharInfo {
                ch: OBJECT_REPLACEMENT_CHARACTER,
                byte_offset,
                run_idx,
                object: true,
                ends_run: true,
            });
            byte_offset += OBJECT_REPLACEMENT_CHARACTER.len_utf8();
            continue;
        }
        let count = run.text.chars().count();
        for (index, ch) in run.text.chars().enumerate() {
            char_info.push(CharInfo {
                ch,
                byte_offset,
                run_idx,
                object: false,
                ends_run: index + 1 == count,
            });
            byte_offset += ch.len_utf8();
        }
    }

    let scripts = resolved_scripts(char_info.iter().map(|info| info.ch));

    let mut result: Vec<TextRun> = Vec::new();

    for byte_range in vis_ranges.iter() {
        // Each visual run's characters remain in *logical* order inside the
        // run — the shaper (rustybuzz) will flip RTL glyphs itself. Only the
        // *order of runs* is visual (left-to-right on the page), so we emit
        // runs in the order `visual_runs` gives them.
        let range_is_rtl = vis_levels
            .get(byte_range.start)
            .is_some_and(|level| level.is_rtl());
        // One piece per run in the range, in logical order. A piece that
        // doesn't end its run leaves the run's boundary advance to the piece
        // that does.
        let piece = |(run_idx, script, text, ends_run): (usize, Script, String, bool)| {
            let mut piece = runs[run_idx].text_fragment(text, ends_run);
            piece.metadata.bidi_rtl = range_is_rtl;
            piece.metadata.script = Some(script);
            piece.shaping.direction = if range_is_rtl {
                ShapingDirection::RightToLeft
            } else {
                ShapingDirection::LeftToRight
            };
            piece
        };
        let mut range_runs = Vec::new();
        let mut current: Option<(usize, Script, String, bool)> = None;
        for (info, &script) in char_info
            .iter()
            .zip(&scripts)
            .filter(|(info, _)| byte_range.contains(&info.byte_offset))
        {
            if info.object {
                if let Some(open) = current.take() {
                    range_runs.push(piece(open));
                }
                let mut object = runs[info.run_idx].clone();
                if range_is_rtl {
                    object.reverse_inline_edge_role();
                }
                range_runs.push(object);
                continue;
            }
            match current.as_mut() {
                Some((run_idx, open_script, text, ends_run))
                    if *run_idx == info.run_idx && *open_script == script =>
                {
                    text.push(info.ch);
                    *ends_run = info.ends_run;
                }
                _ => {
                    if let Some(open) = current.take() {
                        range_runs.push(piece(open));
                    }
                    current = Some((info.run_idx, script, info.ch.to_string(), info.ends_run));
                }
            }
        }
        if let Some(open) = current {
            range_runs.push(piece(open));
        }
        if range_is_rtl {
            move_boundaries_to_visual_right(&mut range_runs);
            range_runs.reverse();
        }
        result.extend(range_runs);
    }

    // Each piece keeps its text in logical order and is shaped in its own
    // direction, so a space at either end of a right-to-left piece already
    // falls between the words it separates.
    if result.is_empty() {
        runs.to_vec()
    } else {
        result
    }
}

/// The runs of a `bidi-override` line in visual order (`reorder_runs_bidi`).
fn override_runs(runs: &[TextRun], rtl: bool) -> Vec<TextRun> {
    let mut pieces: Vec<TextRun> = runs
        .iter()
        .filter(|run| !run.text.is_empty() || run.inline_box.is_some())
        .cloned()
        .map(|mut run| {
            if run.inline_box.is_some() && run.text.is_empty() {
                if rtl {
                    run.reverse_inline_edge_role();
                }
                return run;
            }
            run.metadata.bidi_rtl = rtl;
            run.shaping.direction = if rtl {
                ShapingDirection::RightToLeft
            } else {
                ShapingDirection::LeftToRight
            };
            run
        })
        .collect();
    if rtl {
        move_boundaries_to_visual_right(&mut pieces);
        pieces.reverse();
    }
    pieces
}

/// Each character's script for shaping, as Chromium itemizes a paragraph
/// (UAX #24): a space, punctuation, a digit or a combining mark takes the
/// script of the character before it, and those at the start take the first
/// script after them. So a space between Hebrew and a Latin word is shaped
/// with the Hebrew, not kerned with the Latin letter after it.
fn resolved_scripts(chars: impl Iterator<Item = char>) -> Vec<Script> {
    let mut scripts: Vec<Script> = chars.map(|ch| ch.script()).collect();
    let real =
        |script: Script| !matches!(script, Script::Common | Script::Inherited | Script::Unknown);
    let mut previous = scripts
        .iter()
        .copied()
        .find(|script| real(*script))
        .unwrap_or(Script::Common);
    for script in &mut scripts {
        if real(*script) {
            previous = *script;
        } else {
            *script = previous;
        }
    }
    scripts
}

/// A run's boundary advance (letter-spacing and pair kerning across an inline
/// boundary) is laid out and painted after the run, to its right. In a
/// right-to-left range the boundary after a piece in the text is at its
/// visual left, the right of the piece after it, so each piece takes the
/// boundary of the piece before it. The range's first piece takes its last
/// piece's, which keeps the range's width. `pieces` is in logical order.
fn move_boundaries_to_visual_right(pieces: &mut [TextRun]) {
    let Some(last) = pieces.last().map(|piece| piece.metadata.boundary) else {
        return;
    };
    let mut carried = last;
    for piece in pieces.iter_mut() {
        carried = std::mem::replace(&mut piece.metadata.boundary, carried);
    }
}

/// Returns true if the text contains any RTL characters (Arabic, Hebrew, etc.)
pub(crate) fn has_rtl_chars(text: &str) -> bool {
    text.chars().any(|ch| {
        let c = ch as u32;
        (0x0600..=0x06FF).contains(&c)
            || (0x0750..=0x077F).contains(&c)
            || (0x08A0..=0x08FF).contains(&c)
            || (0xFB50..=0xFDFF).contains(&c)
            || (0xFE70..=0xFEFF).contains(&c)
            || (0x0590..=0x05FF).contains(&c)
            || (0xFB1D..=0xFB4F).contains(&c)
    })
}

pub(crate) fn first_strong_is_rtl(text: &str) -> bool {
    for ch in text.chars() {
        let c = ch as u32;
        if (0x0041..=0x005a).contains(&c)
            || (0x0061..=0x007a).contains(&c)
            || (0x00c0..=0x02af).contains(&c)
        {
            return false;
        }
        if (0x0600..=0x06FF).contains(&c)
            || (0x0750..=0x077F).contains(&c)
            || (0x08A0..=0x08FF).contains(&c)
            || (0xFB50..=0xFDFF).contains(&c)
            || (0xFE70..=0xFEFF).contains(&c)
            || (0x0590..=0x05FF).contains(&c)
            || (0xFB1D..=0xFB4F).contains(&c)
        {
            return true;
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::engine::{
        InlineBox, InlineDecoration, InlineDecorationId, InlineEdge, InlineEdgeSide,
        TextRunMetadata,
    };

    fn make_run(text: &str) -> TextRun {
        TextRun {
            text: text.to_string(),
            ..Default::default()
        }
    }

    fn make_edge(side: InlineEdgeSide, width: f32) -> TextRun {
        TextRun {
            inline_box: Some(Box::new(InlineBox::advance_only(width))),
            metadata: TextRunMetadata {
                inline_edge: Some(InlineEdge {
                    side,
                    decoration: InlineDecoration {
                        id: InlineDecorationId::from_index(0),
                        background_color: None,
                        padding: crate::types::EdgeSizes::ZERO,
                        border_radii: crate::types::CornerRadii::ZERO,
                    },
                }),
                ..Default::default()
            },
            ..Default::default()
        }
    }

    #[test]
    fn pure_ltr_unchanged() {
        let runs = vec![make_run("Hello World")];
        let result = reorder_runs_bidi(&runs, false, false);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].text, "Hello World");
    }

    #[test]
    fn has_rtl_detects_arabic() {
        assert!(has_rtl_chars("مرحبا"));
        assert!(has_rtl_chars("שלום"));
        assert!(!has_rtl_chars("Hello World"));
        assert!(!has_rtl_chars("你好世界"));
    }

    #[test]
    fn mixed_ltr_rtl_reorders() {
        let runs = vec![make_run("Hello مرحبا World")];
        let result = reorder_runs_bidi(&runs, false, false);
        let combined: String = result.iter().map(|r| r.text.as_str()).collect();
        // Should contain all characters
        assert!(combined.contains("Hello"));
        assert!(combined.contains("World"));
        // Arabic chars should be present (possibly reversed)
        assert!(
            combined
                .chars()
                .any(|c| (0x0600..=0x06FF).contains(&(c as u32)))
        );
    }

    #[test]
    fn rtl_reordering_preserves_and_reverses_inline_objects() {
        let runs = vec![
            make_edge(InlineEdgeSide::Opening, 20.0),
            make_run("שלום"),
            make_edge(InlineEdgeSide::Closing, 10.0),
        ];

        let result = reorder_runs_bidi(&runs, true, false);

        assert_eq!(result.len(), 3);
        assert!(result[0].is_opening_inline_edge());
        assert_eq!(result[0].atomic_inline_advance(), Some(10.0));
        assert_eq!(result[1].text, "שלום");
        assert!(!result[1].is_inline_edge());
        assert!(!result[2].is_opening_inline_edge());
        assert_eq!(result[2].atomic_inline_advance(), Some(20.0));
    }

    #[test]
    fn empty_runs() {
        let result = reorder_runs_bidi(&[], false, false);
        assert!(result.is_empty());
    }

    #[test]
    fn bidi_override_rtl_reverses_ltr_text() {
        // `unicode-bidi: bidi-override` + `direction: rtl` forces strictly
        // sequential RTL layout, so Latin "ABCDEF" displays as "FEDCBA": the
        // runs are in reverse order, each shaped right to left.
        let runs = vec![make_run("ABC"), make_run("DEF")];
        let result = reorder_runs_bidi(&runs, true, true);
        let texts: Vec<&str> = result.iter().map(|run| run.text.as_str()).collect();
        assert_eq!(texts, vec!["DEF", "ABC"]);
        assert!(
            result
                .iter()
                .all(|run| run.shaping.direction == ShapingDirection::RightToLeft)
        );
        let drawn: String = result
            .iter()
            .map(|run| run.unshaped_drawing_text())
            .collect();
        assert_eq!(drawn, "FEDCBA");
    }

    #[test]
    fn an_unshaped_right_to_left_run_mirrors_its_brackets() {
        let mut run = make_run("(a)");
        run.shaping.direction = ShapingDirection::RightToLeft;
        assert_eq!(run.unshaped_drawing_text(), "(a)");
    }

    #[test]
    fn bidi_override_ltr_keeps_order() {
        let runs = vec![make_run("ABCDEF")];
        let result = reorder_runs_bidi(&runs, false, true);
        let combined: String = result.iter().map(|r| r.text.as_str()).collect();
        assert_eq!(combined, "ABCDEF");
    }

    fn with_boundary(text: &str, kerning: f32) -> TextRun {
        let mut run = make_run(text);
        run.metadata.boundary.set_contextual_shaping(kerning);
        run
    }

    #[test]
    fn rtl_pieces_keep_their_text_logical_and_their_spaces() {
        // One piece per word, as the wrapper leaves them: each keeps the space
        // before it in the text, which right-to-left shaping draws to its
        // right, between it and the word before.
        let runs = vec![make_run("אחד"), make_run(" שתיים"), make_run(" שלוש")];
        let result = reorder_runs_bidi(&runs, true, false);
        let texts: Vec<&str> = result.iter().map(|run| run.text.as_str()).collect();
        assert_eq!(texts, vec![" שלוש", " שתיים", "אחד"]);
        assert!(result.iter().all(|run| run.metadata.bidi_rtl));
    }

    #[test]
    fn rtl_boundaries_move_to_the_right_of_the_piece_after_them() {
        // The advance after "א" in the text is between it and "ב", which is
        // to its left: it becomes the advance to the right of "ב".
        let runs = vec![
            with_boundary("א", 1.0),
            with_boundary("ב", 2.0),
            with_boundary("ג", 3.0),
        ];
        let result = reorder_runs_bidi(&runs, true, false);
        let pieces: Vec<(&str, f32)> = result
            .iter()
            .map(|run| (run.text.as_str(), run.metadata.boundary.total()))
            .collect();
        assert_eq!(pieces, vec![("ג", 2.0), ("ב", 1.0), ("א", 3.0)]);
    }

    #[test]
    fn only_the_piece_that_ends_a_run_keeps_its_boundary() {
        let runs = vec![with_boundary("abc אבג", 1.5), make_run("def")];
        let result = reorder_runs_bidi(&runs, false, false);
        let total: f32 = result.iter().map(|run| run.metadata.boundary.total()).sum();
        assert_eq!(total, 1.5, "the advance is laid out once");
    }
}
