//! Renderer-independent geometry for horizontal CSS text decorations.

use std::collections::HashMap;

use crate::layout::engine::TextRun;
use crate::parser::ttf::TtfFont;

mod ink_skip;

pub(crate) use ink_skip::ink_skip_intervals;

/// An inline-axis interval relative to the shaped run origin.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct InlineInterval {
    pub start: f32,
    pub end: f32,
}

impl InlineInterval {
    pub(crate) const fn new(start: f32, end: f32) -> Self {
        Self { start, end }
    }

    pub(crate) const fn translated(self, distance: f32) -> Self {
        Self::new(self.start + distance, self.end + distance)
    }
}

/// Portions of `bounds` that remain after ordered exclusion intervals.
pub(crate) fn visible_segments(
    bounds: InlineInterval,
    exclusions: impl IntoIterator<Item = InlineInterval>,
) -> Vec<InlineInterval> {
    let mut cursor = bounds.start;
    let mut visible = Vec::new();
    for exclusion in merge_intervals(exclusions.into_iter().collect()) {
        let start = exclusion.start.clamp(bounds.start, bounds.end);
        let end = exclusion.end.clamp(bounds.start, bounds.end);
        if start > cursor {
            visible.push(InlineInterval::new(cursor, start));
        }
        cursor = cursor.max(end);
    }
    if cursor < bounds.end {
        visible.push(InlineInterval::new(cursor, bounds.end));
    }
    visible
}

/// Which decoration line is being painted.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum DecorationLine {
    Underline,
    LineThrough,
    Overline,
}

impl DecorationLine {
    pub(crate) const fn can_skip_ink(self) -> bool {
        matches!(self, Self::Underline | Self::Overline)
    }
}

/// A decoration's thickness as Blink resolves it (`ComputeDecorationThickness`
/// in `text_decoration_info.cc`): `auto` is a tenth of the font size, and a
/// length is rounded to whole CSS px; at least 1px either way. Wavy lines keep
/// their own automatic thickness.
pub(crate) fn thickness(run: &TextRun, decoration: &crate::style::computed::TextDecoration) -> f32 {
    let px = crate::fonts::PT_PER_CSS_PX;
    let wavy = decoration.style == crate::style::computed::TextDecorationStyle::Wavy;
    let thickness = match decoration.thickness {
        Some(thickness) if wavy => thickness,
        Some(thickness) => (thickness / px).round() * px,
        None if wavy => run.font_size * 0.075,
        None => run.font_size / 10.0,
    };
    thickness.max(px)
}

/// The thickness Blink paints a line with: solid and double lines are drawn
/// without antialiasing, so their thickness is rounded down to whole CSS px
/// (`RoundDownThickness`); dotted, dashed and wavy lines keep the resolved
/// thickness.
pub(crate) fn painted_thickness(
    run: &TextRun,
    decoration: &crate::style::computed::TextDecoration,
) -> f32 {
    let resolved = thickness(run, decoration);
    if decoration.style.is_antialiased() {
        return resolved;
    }
    solid_thickness(resolved)
}

/// How far below the baseline the underline's centre is. Blink leaves a gap of
/// half the resolved thickness, rounded up to whole CSS px, between the
/// baseline and the line's top; an authored `text-underline-offset` replaces
/// that gap.
pub(crate) fn underline_distance_from_baseline(
    run: &TextRun,
    decoration: &crate::style::computed::TextDecoration,
) -> f32 {
    let top_offset = decoration
        .underline_offset
        .unwrap_or_else(|| crate::fonts::ceil_to_css_pixel(thickness(run, decoration) / 2.0));
    top_offset + painted_thickness(run, decoration) / 2.0
}

/// How far above the baseline a line-through's centre is. Blink puts the
/// line's top a third of the font's ascent (rounded to whole px) plus half its
/// thickness above the baseline (`TextDecorationInfo::SetLineThroughLineData`),
/// and a solid or double line's top on a whole pixel. Dotted and dashed lines
/// snap their centre when they're painted, so they get it unrounded.
/// `ascender_ratio` is the font's ascent per unit of font size.
pub(crate) fn line_through_axis_above_baseline(
    run: &TextRun,
    decoration: &crate::style::computed::TextDecoration,
    ascender_ratio: f32,
) -> f32 {
    let top = rounded_ascent(run, ascender_ratio) / 3.0 + thickness(run, decoration) / 2.0;
    snapped_axis(run, decoration, top)
}

/// How far above the baseline an overline's centre is: Blink puts its bottom
/// on the font's ascent, rounded to whole px, so it sits on the text's top
/// edge. For dotted and dashed lines that edge is the solid line's thickness
/// above the ascent, as for solid lines.
pub(crate) fn overline_axis_above_baseline(
    run: &TextRun,
    decoration: &crate::style::computed::TextDecoration,
    ascender_ratio: f32,
) -> f32 {
    let solid = solid_thickness(thickness(run, decoration));
    let top = rounded_ascent(run, ascender_ratio) + solid;
    snapped_axis(run, decoration, top)
}

/// The centre of a line whose top is `top` above the baseline: on whole
/// pixels for a solid or double line, and as computed for an antialiased one.
fn snapped_axis(
    run: &TextRun,
    decoration: &crate::style::computed::TextDecoration,
    top: f32,
) -> f32 {
    let px = crate::fonts::PT_PER_CSS_PX;
    if decoration.style.is_antialiased() {
        return top - thickness(run, decoration) / 2.0;
    }
    (top / px).round() * px - painted_thickness(run, decoration) / 2.0
}

/// The font's ascent rounded to whole CSS px, as Blink's `FontMetrics::Ascent`
/// gives it.
fn rounded_ascent(run: &TextRun, ascender_ratio: f32) -> f32 {
    let px = crate::fonts::PT_PER_CSS_PX;
    (ascender_ratio * run.font_size / px).round() * px
}

fn solid_thickness(resolved: f32) -> f32 {
    let px = crate::fonts::PT_PER_CSS_PX;
    ((resolved / px).floor() * px).max(px)
}

/// Width of leading and trailing whitespace excluded from a decoration line.
pub(crate) fn whitespace_insets(
    run: &TextRun,
    custom_fonts: &HashMap<String, TtfFont>,
) -> (f32, f32) {
    if run.inline_box.is_some() {
        return (0.0, 0.0);
    }
    let leading: String = run.text.chars().take_while(|c| c.is_whitespace()).collect();
    let trailing: String = run
        .text
        .chars()
        .rev()
        .take_while(|c| c.is_whitespace())
        .collect();
    let measure = |text: &str| {
        if text.is_empty() {
            0.0
        } else {
            crate::layout::text::estimate_word_width(
                text,
                run.font_size,
                &run.font_family,
                run.bold,
                run.font_style.is_slanted(),
                custom_fonts,
            )
        }
    };
    (measure(&leading), measure(&trailing))
}

pub(super) fn merge_intervals(mut intervals: Vec<InlineInterval>) -> Vec<InlineInterval> {
    intervals.sort_by(|left, right| left.start.total_cmp(&right.start));
    let mut merged: Vec<InlineInterval> = Vec::with_capacity(intervals.len());
    for interval in intervals {
        if let Some(previous) = merged.last_mut()
            && interval.start <= previous.end
        {
            previous.end = previous.end.max(interval.end);
        } else {
            merged.push(interval);
        }
    }
    merged
}

#[cfg(test)]
mod tests {
    use super::{DecorationLine, ink_skip_intervals};
    use crate::layout::engine::TextRun;
    use crate::style::computed::FontFamily;
    use std::collections::HashMap;

    fn parity_sans() -> HashMap<String, crate::parser::ttf::TtfFont> {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/parity/fonts/ParitySans.ttf");
        let font = crate::parser::ttf::parse_ttf(std::fs::read(path).unwrap()).unwrap();
        HashMap::from([("ParitySans".to_owned(), font)])
    }

    fn underlined_run(font_size: f32, bold: bool) -> TextRun {
        TextRun {
            text: "AgBb".to_owned(),
            font_size,
            bold,
            font_family: FontFamily::Custom("ParitySans".to_owned()),
            decorations: vec![crate::style::computed::TextDecoration {
                lines: crate::style::computed::TextDecorationLines {
                    underline: true,
                    ..Default::default()
                },
                thickness: Some(1.5),
                underline_offset: Some(2.25),
                ..Default::default()
            }],
            ..Default::default()
        }
    }

    #[test]
    fn ink_skip_ignores_a_tangent_descender_but_breaks_a_crossing_descender() {
        let fonts = parity_sans();
        let normal = ink_skip_intervals(
            &underlined_run(12.0, false),
            &underlined_run(12.0, false).decorations[0],
            DecorationLine::Underline,
            -3.0,
            &fonts,
        );
        let bold = ink_skip_intervals(
            &underlined_run(13.5, true),
            &underlined_run(13.5, true).decorations[0],
            DecorationLine::Underline,
            -3.0,
            &fonts,
        );
        assert!(normal.is_empty(), "normal: {normal:?}; bold: {bold:?}");
        assert_eq!(bold.len(), 1, "normal: {normal:?}; bold: {bold:?}");
    }

    #[test]
    fn skip_ink_none_keeps_the_decoration_continuous() {
        let fonts = parity_sans();
        let mut run = underlined_run(13.5, true);
        run.decorations[0].skip_ink = crate::style::computed::TextDecorationSkipInk::None;

        assert!(
            ink_skip_intervals(
                &run,
                &run.decorations[0],
                DecorationLine::Underline,
                -3.0,
                &fonts
            )
            .is_empty()
        );
    }
}
