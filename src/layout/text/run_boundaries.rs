//! What happens between two paint runs on a line.
//!
//! An inline element, a pseudo-element or a change of colour splits text into separate paint runs, but CSS Text
//! keeps the typography across the split where the glyph formatting doesn't change. Two things cross it: the
//! `letter-spacing` of the innermost element holding both letters (CSS Text 3 §10.1), and the pair kerning of the two
//! glyphs either side (CSS Text 3 §7.3, and Chromium shapes them together). Each boundary is settled once, by the
//! innermost scope that holds both runs, and stored on the run before it, so layout and painting use the same advance.

use super::{ComputedStyle, HashMap, TextRun, TtfFont};
use crate::layout::elements::TextSpacing;
use unicode_bidi::{BidiClass, bidi_class};

/// The scope that settles the boundaries between its runs: the element holding them.
#[derive(Clone, Copy)]
pub(crate) struct BoundaryScope<'a> {
    spacing: TextSpacing,
    /// The fonts to kern with, or `None` when this scope doesn't kern across boundaries
    kerning: Option<&'a HashMap<String, TtfFont>>,
    /// The paragraph is right to left.
    rtl: bool,
}

impl<'a> BoundaryScope<'a> {
    pub(crate) fn of(style: &ComputedStyle, fonts: &'a HashMap<String, TtfFont>) -> Self {
        Self {
            spacing: TextSpacing::from_style(style),
            kerning: Some(fonts),
            rtl: style.direction_rtl,
        }
    }

    #[cfg(test)]
    pub(crate) fn tracking(spacing: TextSpacing) -> Self {
        Self {
            spacing,
            kerning: None,
            rtl: false,
        }
    }
}

/// Settle the boundaries between flattened paint runs while their innermost common inline ancestor is known.
///
/// A boundary already claimed by a nested inline stays as it was when an outer scope is resolved, so tracking
/// belongs to the innermost element containing both typographic units.
pub(crate) trait InlineTextSequence {
    fn resolve_unclaimed_boundaries(&mut self, scope: BoundaryScope<'_>);
}

impl InlineTextSequence for [TextRun] {
    fn resolve_unclaimed_boundaries(&mut self, scope: BoundaryScope<'_>) {
        let mut previous: Option<usize> = None;
        for current in 0..self.len() {
            if self[current].forces_line_break() {
                previous = None;
                continue;
            }
            if !self[current].has_typographic_unit() {
                continue;
            }
            if let Some(previous_index) = previous {
                let separated = self[previous_index + 1..current]
                    .iter()
                    .any(|run| run.is_inline_edge() || run.inline_box.is_some());
                let (before, after) = self.split_at_mut(current);
                let previous_run = &mut before[previous_index];
                let current_run = &after[0];
                if previous_run.metadata.boundary.is_unresolved() {
                    let joins = previous_run.joins_typographically(current_run);
                    let advance = if joins {
                        scope.spacing.letter
                    } else {
                        Default::default()
                    };
                    if joins
                        && !separated
                        && let Some(fonts) = scope.kerning
                    {
                        let kerning = boundary_kerning(previous_run, current_run, scope.rtl, fonts);
                        if kerning != 0.0 {
                            previous_run
                                .metadata
                                .boundary
                                .set_contextual_shaping(kerning);
                        }
                    }
                    previous_run
                        .metadata
                        .boundary
                        .resolve_letter_spacing(advance);
                }
            }
            previous = Some(current);
        }
    }
}

/// The pair kerning of the last glyph before a boundary and the first after it, when both are in the same bidi
/// level run: two right-to-left letters, or two left-to-right characters (in a right-to-left paragraph, two strong
/// left-to-right letters, as a neutral or a digit there can take another level). Bidi reordering moves the advance
/// of a right-to-left pair to the right side of the piece after it (`reorder_runs_bidi`). The caller has checked
/// that nothing between the runs (an inline element's margin, border or padding, an atomic inline) separates the
/// glyphs.
fn boundary_kerning(
    left: &TextRun,
    right: &TextRun,
    paragraph_rtl: bool,
    fonts: &HashMap<String, TtfFont>,
) -> f32 {
    let (Some(left_char), Some(right_char)) =
        (left.text.chars().next_back(), right.text.chars().next())
    else {
        return 0.0;
    };
    let (left_class, right_class) = (bidi_class(left_char), bidi_class(right_char));
    let right_to_left = |class: BidiClass| matches!(class, BidiClass::R | BidiClass::AL);
    let left_to_right = |class: BidiClass| {
        if paragraph_rtl {
            class == BidiClass::L
        } else {
            !right_to_left(class) && class != BidiClass::AN
        }
    };
    let same_run = (right_to_left(left_class) && right_to_left(right_class))
        || (left_to_right(left_class) && left_to_right(right_class));
    if !same_run {
        return 0.0;
    }
    crate::text::inline_boundary_kerning_advance(left, right, fonts)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::style::computed::FontFamily;

    fn parity_sans_fonts() -> HashMap<String, TtfFont> {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
        let font = crate::parser::ttf::parse_ttf(
            std::fs::read(root.join("tests/parity/fonts/ParitySans.ttf"))
                .expect("ParitySans test font"),
        )
        .expect("valid ParitySans TTF");
        HashMap::from([(
            crate::system_fonts::font_variant_key("ParitySans", false, false),
            font,
        )])
    }

    fn run(text: &str) -> TextRun {
        TextRun {
            text: text.to_string(),
            font_size: 18.0,
            font_family: FontFamily::Custom("ParitySans".to_string()),
            ..Default::default()
        }
    }

    /// The kerning left after the first of two runs, in a paragraph of `direction_rtl`.
    fn kerning(left: &str, right: &str, direction_rtl: bool) -> f32 {
        let fonts = parity_sans_fonts();
        let style = ComputedStyle {
            direction_rtl,
            ..Default::default()
        };
        let mut runs = vec![run(left), run(right)];
        runs.as_mut_slice()
            .resolve_unclaimed_boundaries(BoundaryScope::of(&style, &fonts));
        runs[0].metadata.boundary.total()
    }

    #[test]
    fn left_to_right_letters_kern_across_a_boundary_in_a_right_to_left_paragraph() {
        // Two strong left-to-right letters share a level run whatever the
        // paragraph's direction, as Chrome shapes them together.
        let ltr = kerning("F", "irst", false);
        assert!(ltr < 0.0, "F/i kern: {ltr}");
        assert_eq!(kerning("F", "irst", true), ltr);
    }

    #[test]
    fn a_neutral_at_the_boundary_of_a_right_to_left_paragraph_does_not_kern() {
        // A neutral there can take the paragraph's level while the letter
        // after it doesn't, so they may not be neighbours on the line.
        assert_eq!(kerning("(", "F", true), 0.0);
    }
}
