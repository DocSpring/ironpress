//! Dashed and dotted strokes as Blink lays them out (`StrokeData`). Dashed borders, rounded borders, collapsed table
//! borders, borders painted into a filter's source image and dotted or dashed text decorations all take their dashes
//! from here, so none of them can drift to another ratio or gap rule.

/// The nominal dash and gap of a dashed or dotted stroke, in whatever unit its width is in.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct DashPattern {
    dash: f32,
    gap: f32,
}

/// Whether a stroke has two ends (a border side, a decoration) or goes round to where it started (a rounded border's
/// outline).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum StrokePath {
    Open,
    Closed,
}

/// How a dashed stroke of a given length is drawn.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum DashLayout {
    /// Too short for the pattern: one solid line.
    Solid,
    /// `count` dashes `dash` long with `gap` between them. An open stroke starts and ends with a dash; a closed one
    /// has as many gaps as dashes.
    Dashes { count: usize, dash: f32, gap: f32 },
}

/// Whether a dotted stroke `width_px` CSS px thick is drawn as round dots. Blink draws one up to 3px thick as square
/// dots, dashes a width long ([`DashPattern::dotted`]), and only a thicker one with round caps
/// (`StrokeData::StrokeIsDashed`).
pub(crate) fn dotted_dots_are_round(width_px: f32) -> bool {
    width_px > 3.0
}

impl DashPattern {
    /// The dashes of a stroke `width` thick, `width_px` CSS px (`StrokeData::DashLengthRatio`, `DashGapRatio`):
    /// dashes three times the width with gaps twice it below 3px, and twice and once the width from 3px.
    pub(crate) fn dashed(width: f32, width_px: f32) -> Self {
        let (dash, gap) = if width_px >= 3.0 {
            (2.0, 1.0)
        } else {
            (3.0, 2.0)
        };
        Self {
            dash: width * dash,
            gap: width * gap,
        }
    }

    /// The dots of a stroke `width` thick: a width long and a width apart.
    pub(crate) const fn dotted(width: f32) -> Self {
        Self {
            dash: width,
            gap: width,
        }
    }

    /// The gap nearest the nominal one that fits a whole number of dashes in `length` (`SelectBestDashGap`). An open
    /// stroke ends with a dash, so it has one gap fewer than dashes.
    pub(crate) fn best_gap(self, length: f32, path: StrokePath) -> f32 {
        let Self { dash, gap } = self;
        let available = match path {
            StrokePath::Open => length + gap,
            StrokePath::Closed => length,
        };
        let fewer = (available / (dash + gap)).floor().max(1.0);
        let more = fewer + 1.0;
        let gaps = |dashes: f32| match path {
            StrokePath::Open => dashes - 1.0,
            StrokePath::Closed => dashes,
        };
        // One dash on an open stroke has no gap: the stroke is longer than two dashes, so more of them fit
        let fewer_gap = if gaps(fewer) > 0.0 {
            (length - fewer * dash) / gaps(fewer)
        } else {
            f32::INFINITY
        };
        let more_gap = (length - more * dash) / gaps(more);
        if more_gap <= 0.0 || (fewer_gap - gap).abs() < (more_gap - gap).abs() {
            fewer_gap.max(0.0)
        } else {
            more_gap
        }
    }

    /// A dashed stroke `length` long (`StrokeData::SetupPaintDashPathEffect`). It's solid when no longer than two
    /// dashes. A closed one no longer than two dashes and two gaps shrinks the pattern to fit twice; anything longer
    /// keeps the dash and stretches or squeezes the gap.
    pub(crate) fn layout(self, length: f32, path: StrokePath) -> DashLayout {
        if length.is_nan() || length <= self.dash * 2.0 {
            return DashLayout::Solid;
        }
        let twice = 2.0 * (self.dash + self.gap);
        let (dash, gap) = if path == StrokePath::Closed && length <= twice {
            let scale = length / twice;
            (self.dash * scale, self.gap * scale)
        } else {
            (self.dash, self.best_gap(length, path))
        };
        let pitch = dash + gap;
        let count = match path {
            StrokePath::Open => (length + gap) / pitch,
            StrokePath::Closed => length / pitch,
        };
        DashLayout::Dashes {
            count: count.round().max(1.0) as usize,
            dash,
            gap,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn thin_strokes_have_long_dashes_and_thick_ones_short() {
        assert_eq!(
            DashPattern::dashed(2.0, 2.0),
            DashPattern {
                dash: 6.0,
                gap: 4.0
            }
        );
        assert_eq!(
            DashPattern::dashed(3.0, 3.0),
            DashPattern {
                dash: 6.0,
                gap: 3.0
            }
        );
        // In pt, decided by the width in px
        assert_eq!(
            DashPattern::dashed(1.5, 2.0),
            DashPattern {
                dash: 4.5,
                gap: 3.0
            }
        );
    }

    #[test]
    fn an_open_stroke_starts_and_ends_with_a_dash() {
        // 138px of 6px dashes: 14 dashes with 13 gaps of 54/13px
        let DashLayout::Dashes { count, dash, gap } =
            DashPattern::dashed(2.0, 2.0).layout(138.0, StrokePath::Open)
        else {
            panic!("dashes");
        };
        assert_eq!((count, dash), (14, 6.0));
        assert!((gap - 54.0 / 13.0).abs() < 1e-4, "{gap}");
        assert!(((count - 1) as f32 * (dash + gap) + dash - 138.0).abs() < 1e-3);
    }

    #[test]
    fn a_short_open_stroke_takes_the_dash_count_whose_gap_is_nearest_the_nominal_one() {
        // Blink fits a 20px side of a 2px dashed border with three 6px dashes
        // and 1px gaps (a gap 3px from the nominal 4px), not two dashes and an
        // 8px gap (4px from it), as Chrome draws a bordered span's sides.
        assert_eq!(
            DashPattern::dashed(2.0, 2.0).layout(20.0, StrokePath::Open),
            DashLayout::Dashes {
                count: 3,
                dash: 6.0,
                gap: 1.0
            }
        );
    }

    #[test]
    fn a_stroke_no_longer_than_two_dashes_is_solid() {
        let pattern = DashPattern::dashed(2.0, 2.0);
        assert_eq!(pattern.layout(12.0, StrokePath::Open), DashLayout::Solid);
        assert_eq!(pattern.layout(12.0, StrokePath::Closed), DashLayout::Solid);
        assert_eq!(
            pattern.layout(f32::NAN, StrokePath::Open),
            DashLayout::Solid
        );
        assert!(matches!(
            pattern.layout(12.5, StrokePath::Open),
            DashLayout::Dashes { count: 2, .. }
        ));
    }

    #[test]
    fn a_short_closed_stroke_shrinks_the_pattern_to_fit_twice() {
        // 2 x (6 + 4) = 20px is the most that shrinks; 15px is three quarters of it
        assert_eq!(
            DashPattern::dashed(2.0, 2.0).layout(15.0, StrokePath::Closed),
            DashLayout::Dashes {
                count: 2,
                dash: 4.5,
                gap: 3.0
            }
        );
    }

    #[test]
    fn a_closed_stroke_has_as_many_gaps_as_dashes() {
        // 100px of 6px dashes and 3px gaps: 11 dashes leave 34px for 11 gaps
        let pattern = DashPattern::dashed(3.0, 3.0);
        assert!((pattern.best_gap(100.0, StrokePath::Closed) - 34.0 / 11.0).abs() < 1e-4);
        assert!(matches!(
            pattern.layout(100.0, StrokePath::Closed),
            DashLayout::Dashes { count: 11, .. }
        ));
    }

    #[test]
    fn dots_are_a_width_long_and_a_width_apart() {
        // 21px of 1px dots: 11 dots 1px apart
        assert_eq!(
            DashPattern::dotted(1.0).best_gap(21.0, StrokePath::Open),
            1.0
        );
    }
}
