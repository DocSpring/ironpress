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
}

impl<'a> BoundaryScope<'a> {
    pub(crate) fn of(style: &ComputedStyle, fonts: &'a HashMap<String, TtfFont>) -> Self {
        Self {
            spacing: TextSpacing::from_style(style),
            // Bidi reordering puts the runs of a right-to-left line in another order than the source's, and the
            // advance belongs to the run before the boundary in the source.
            kerning: (!style.direction_rtl).then_some(fonts),
        }
    }

    #[cfg(test)]
    pub(crate) fn tracking(spacing: TextSpacing) -> Self {
        Self {
            spacing,
            kerning: None,
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
                        let kerning = boundary_kerning(previous_run, current_run, fonts);
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

/// The pair kerning of the last glyph before a boundary and the first after it, unless either one is right-to-left.
/// The caller has checked that nothing between the runs (an inline element's margin, border or padding, an atomic
/// inline) separates the glyphs.
fn boundary_kerning(left: &TextRun, right: &TextRun, fonts: &HashMap<String, TtfFont>) -> f32 {
    let right_to_left = |character: Option<char>| {
        character.is_some_and(|character| {
            matches!(
                bidi_class(character),
                BidiClass::R | BidiClass::AL | BidiClass::AN
            )
        })
    };
    if right_to_left(left.text.chars().next_back()) || right_to_left(right.text.chars().next()) {
        return 0.0;
    }
    crate::text::inline_boundary_kerning_advance(left, right, fonts)
}
