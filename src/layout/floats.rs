//! Float placement and the space floats leave for line boxes (CSS 2.1 §9.5).
//!
//! A block formatting context keeps one [`FloatExclusions`]. Coordinates are
//! px from the formatting context's content-box origin: inline offsets from its
//! left edge, block offsets from its top. Each placed float occupies its margin
//! box; line boxes and later floats avoid it.

use crate::style::computed::Float;

mod avoid;
mod flow;
mod in_text;
pub(crate) use avoid::FloatAvoidance;
pub(crate) use flow::{FloatFrames, LineFloats, float_box};
pub(crate) use in_text::FloatsInText;

#[cfg(test)]
mod tests;

/// A horizontal extent `[start, end]` in px.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub(crate) struct InlineSpan {
    pub(crate) start: f32,
    pub(crate) end: f32,
}

impl InlineSpan {
    pub(crate) const fn new(start: f32, end: f32) -> Self {
        Self { start, end }
    }

    pub(crate) fn width(self) -> f32 {
        (self.end - self.start).max(0.0)
    }

    /// Whether `width` fits, allowing for float rounding in the widths that
    /// produced it.
    fn fits(self, width: f32) -> bool {
        width <= self.width() + FIT_TOLERANCE
    }
}

/// A box's left and right margins.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub(crate) struct InlineMargins {
    pub(crate) left: f32,
    pub(crate) right: f32,
}

/// Widths come from summed f32 lengths; a box that should exactly fill a span
/// must not be pushed down by rounding.
const FIT_TOLERANCE: f32 = 0.01;

/// The physical side a float is pushed to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum FloatSide {
    Left,
    Right,
}

impl FloatSide {
    /// The side of a box that floats; `None` for one in normal flow (or a
    /// footnote, which leaves the flow for the page's footnote area).
    pub(crate) const fn of(float: Float) -> Option<Self> {
        match float {
            Float::Left => Some(Self::Left),
            Float::Right => Some(Self::Right),
            Float::None | Float::Footnote => None,
        }
    }
}

/// A float's side and margin-box size.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct FloatBox {
    pub(crate) side: FloatSide,
    pub(crate) width: f32,
    pub(crate) height: f32,
}

impl FloatBox {
    pub(crate) const fn request(self, min_top: f32, container: InlineSpan) -> FloatRequest {
        FloatRequest {
            side: self.side,
            width: self.width,
            height: self.height,
            min_top,
            container,
        }
    }
}

/// A float to place: its margin box and where its containing block allows it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct FloatRequest {
    pub(crate) side: FloatSide,
    pub(crate) width: f32,
    pub(crate) height: f32,
    /// The highest its top may be: the line or block position where it occurs.
    pub(crate) min_top: f32,
    /// Its containing block's content box (CSS 2.1 §9.5.1 rules 1 and 2).
    pub(crate) container: InlineSpan,
}

/// How far a float's formatting context moved it from where it would sit with
/// no earlier floats: its margin box against its containing block's edge on its
/// side, at its position in the flow. Positive `inline` is rightwards (a left
/// float beside earlier ones), negative leftwards (a right float); `block` is
/// downwards.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub(crate) struct FloatOffset {
    pub(crate) inline: f32,
    pub(crate) block: f32,
}

/// A placed float's margin box.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct PlacedFloat {
    pub(crate) side: FloatSide,
    pub(crate) inline: InlineSpan,
    pub(crate) top: f32,
    pub(crate) bottom: f32,
}

impl PlacedFloat {
    fn overlaps(&self, top: f32, bottom: f32) -> bool {
        self.top < bottom && self.bottom > top
    }
}

/// The floats placed so far in one block formatting context.
#[derive(Debug, Clone, Default, PartialEq)]
pub(crate) struct FloatExclusions {
    floats: Vec<PlacedFloat>,
}

impl FloatExclusions {
    pub(crate) fn is_empty(&self) -> bool {
        self.floats.is_empty()
    }

    /// The same floats in coordinates whose origin is `(-dx, -dy)` in these.
    pub(crate) fn translated(&self, dx: f32, dy: f32) -> Self {
        Self {
            floats: self
                .floats
                .iter()
                .map(|float| PlacedFloat {
                    inline: InlineSpan::new(float.inline.start + dx, float.inline.end + dx),
                    top: float.top + dy,
                    bottom: float.bottom + dy,
                    ..*float
                })
                .collect(),
        }
    }

    /// Place a float per CSS 2.1 §9.5.1: as high as possible, no higher than
    /// an earlier float (rule 6) or its own position in the flow (rule 4), then
    /// as far to its side as possible, beside earlier floats when it fits
    /// (rules 2, 3 and 7) and below them when it doesn't. One wider than its
    /// containing block goes below every float and overflows.
    pub(crate) fn place(&mut self, request: FloatRequest) -> PlacedFloat {
        let earliest = self
            .floats
            .last()
            .map_or(request.min_top, |last| request.min_top.max(last.top));
        let top = self
            .candidate_tops(earliest)
            .find(|&top| {
                self.space(top, top + request.height, request.container)
                    .fits(request.width)
            })
            .unwrap_or_else(|| self.lowest_bottom().max(earliest));
        let space = self.space(top, top + request.height, request.container);
        let inline = match request.side {
            FloatSide::Left => InlineSpan::new(space.start, space.start + request.width),
            FloatSide::Right => InlineSpan::new(space.end - request.width, space.end),
        };
        let placed = PlacedFloat {
            side: request.side,
            inline,
            top,
            bottom: top + request.height,
        };
        self.floats.push(placed);
        placed
    }

    /// The part of `container` that a line box from `top` to `top + height`
    /// may use (CSS 2.1 §9.5: line boxes beside a float are shortened).
    pub(crate) fn line_space(&self, top: f32, height: f32, container: InlineSpan) -> InlineSpan {
        self.space(top, top + height, container)
    }

    /// Where a line box that doesn't fit beside the floats at `top` tries next:
    /// the nearest bottom edge, below `top`, of a float overlapping the line.
    /// `None` when no float shortens it.
    pub(crate) fn next_line_top(&self, top: f32, height: f32) -> Option<f32> {
        self.floats
            .iter()
            .filter(|float| float.overlaps(top, top + height))
            .map(|float| float.bottom)
            .filter(|&bottom| bottom > top)
            .min_by(f32::total_cmp)
    }

    /// The lowest float bottom: how far a formatting context root grows to
    /// contain its floats (CSS 2.1 §10.6.7).
    pub(crate) fn lowest_bottom(&self) -> f32 {
        self.floats
            .iter()
            .map(|float| float.bottom)
            .fold(0.0, f32::max)
    }

    /// Block offsets worth trying for a box that can't go higher than
    /// `earliest`: that offset, then each float bottom below it, where the
    /// available space can widen.
    fn candidate_tops(&self, earliest: f32) -> impl Iterator<Item = f32> {
        let mut bottoms: Vec<f32> = self
            .floats
            .iter()
            .map(|float| float.bottom)
            .filter(|&bottom| bottom > earliest)
            .collect();
        bottoms.sort_by(f32::total_cmp);
        bottoms.dedup();
        std::iter::once(earliest).chain(bottoms)
    }

    /// The part of `container` beside the floats present at block offset
    /// `top`: those that start at or above it and end below it.
    fn space_at(&self, top: f32, container: InlineSpan) -> InlineSpan {
        self.space(top, top + FIT_TOLERANCE, container)
    }

    fn space(&self, top: f32, bottom: f32, container: InlineSpan) -> InlineSpan {
        // A zero-height probe still meets the floats that straddle its offset.
        let bottom = bottom.max(top);
        self.floats
            .iter()
            .filter(|float| float.overlaps(top, bottom))
            .fold(container, |space, float| match float.side {
                FloatSide::Left => InlineSpan::new(space.start.max(float.inline.end), space.end),
                FloatSide::Right => InlineSpan::new(space.start, space.end.min(float.inline.start)),
            })
    }
}
