//! The inline room each line box has while a paragraph wraps: the containing
//! block's width less the first line's indent, a dropped initial's exclusion
//! and the floats beside the line (CSS 2.1 §9.5).

use super::{TextWrapOptions, width_exceeds_limit};
use crate::layout::engine::TextLine;
use crate::layout::floats::{FloatBox, LineFloats};

pub(super) struct LineRoom<'a> {
    options: TextWrapOptions,
    floats: Option<LineFloats>,
    /// The floats that occur in the paragraph's own text, in order.
    inline_floats: &'a [FloatBox],
    /// The line top each of those floats was placed at, relative to the
    /// content-box top, once placed.
    inline_float_tops: Vec<Option<f32>>,
    /// Floats that didn't fit on the line they occur on, waiting for the next
    /// line, and the line they occurred on.
    deferred: Vec<(usize, usize)>,
    /// Space left above a line moved down past floats it didn't fit beside,
    /// by line index.
    gaps: Vec<(usize, f32)>,
}

impl<'a> LineRoom<'a> {
    pub(super) fn new(
        options: TextWrapOptions,
        floats: Option<LineFloats>,
        inline_floats: &'a [FloatBox],
    ) -> Self {
        Self {
            options,
            floats,
            inline_floats,
            inline_float_tops: vec![None; inline_floats.len()],
            deferred: Vec::new(),
            gaps: Vec::new(),
        }
    }

    /// A float occurs in the text, `used` into line `lines.len()`: it goes
    /// on this line when it fits beside what the line holds so far, and on
    /// the next line otherwise (CSS 2.1 §9.5.1 rule 8: as high as possible).
    pub(super) fn float_occurs(
        &mut self,
        anchor: usize,
        lines: &[TextLine],
        line_height: f32,
        used: f32,
    ) {
        let Some(float) = self.inline_floats.get(anchor).copied() else {
            return;
        };
        let index = lines.len();
        let fits = used <= 0.0
            || !width_exceeds_limit(
                used + float.width,
                self.max_width(lines, line_height, index),
            );
        if fits && self.deferred.is_empty() {
            self.place_inline(anchor, float, self.top(lines, line_height, index));
        } else {
            self.deferred.push((anchor, index));
        }
    }

    /// Place the floats waiting for a line once a later line has started, or
    /// below the last line when the text has ended.
    pub(super) fn place_deferred(&mut self, lines: &[TextLine], line_height: f32, ended: bool) {
        let index = lines.len();
        let ready: Vec<usize> = self
            .deferred
            .iter()
            .filter(|(_, occurred_on)| ended || *occurred_on < index)
            .map(|(anchor, _)| *anchor)
            .collect();
        if ready.is_empty() {
            return;
        }
        self.deferred.retain(|(anchor, _)| !ready.contains(anchor));
        let top = self.top(lines, line_height, index);
        for anchor in ready {
            if let Some(float) = self.inline_floats.get(anchor).copied() {
                self.place_inline(anchor, float, top);
            }
        }
    }

    /// The line top each float in the text was placed at, relative to the
    /// content-box top.
    pub(super) fn inline_float_tops(&self) -> Vec<f32> {
        self.inline_float_tops
            .iter()
            .map(|top| top.unwrap_or_default())
            .collect()
    }

    fn place_inline(&mut self, anchor: usize, float: FloatBox, top: f32) {
        if let Some(floats) = self.floats.as_mut() {
            floats.place(float, top);
        }
        if let Some(slot) = self.inline_float_tops.get_mut(anchor) {
            *slot = Some(top);
        }
    }

    /// The width line `index` may fill. `lines` are the lines already made;
    /// a line not started yet is assumed to be `line_height` tall, like the
    /// one being filled.
    pub(super) fn max_width(&self, lines: &[TextLine], line_height: f32, index: usize) -> f32 {
        let drop_cap = self.options.drop_cap_exclusion_width(index);
        let indent = if index == 0 {
            self.options.text_indent
        } else {
            0.0
        };
        let floats = self.floats.as_ref().map_or(0.0, |floats| {
            floats
                .insets(self.top(lines, line_height, index), line_height)
                .horizontal()
        });
        (self.options.max_width - indent - drop_cap - floats).max(0.0)
    }

    /// Before the first piece of the next line, `width` wide, is placed: when
    /// it doesn't fit beside the floats there, move the line down past them
    /// until it fits or no float shortens it.
    pub(super) fn make_room(&mut self, lines: &[TextLine], line_height: f32, width: f32) {
        let Some(floats) = self.floats.as_ref() else {
            return;
        };
        let index = lines.len();
        while width_exceeds_limit(width, self.max_width(lines, line_height, index)) {
            let top = self.top(lines, line_height, index);
            let Some(next) = floats.next_line_top(top, line_height) else {
                return;
            };
            self.gaps.push((index, next - top));
        }
    }

    /// Give each line the room the floats beside it leave, and the space
    /// above the lines that moved down past them.
    pub(super) fn finish(&self, lines: &mut [TextLine]) {
        let Some(floats) = self.floats.as_ref() else {
            return;
        };
        let mut top = 0.0;
        for (index, line) in lines.iter_mut().enumerate() {
            let gap = self.gap_before(index);
            top += gap;
            line.float_insets = floats.insets(top, line.height);
            top += line.height;
            if gap > 0.0 {
                line.height += gap;
                line.baseline_ascent = line.baseline_ascent.map(|ascent| ascent + gap);
            }
        }
    }

    fn gap_before(&self, index: usize) -> f32 {
        self.gaps
            .iter()
            .filter(|(line, _)| *line == index)
            .map(|(_, gap)| gap)
            .sum()
    }

    fn top(&self, lines: &[TextLine], line_height: f32, index: usize) -> f32 {
        let made: f32 = lines.iter().take(index).map(|line| line.height).sum();
        let gaps: f32 = self
            .gaps
            .iter()
            .filter(|(line, _)| *line <= index)
            .map(|(_, gap)| gap)
            .sum();
        made + gaps + line_height * index.saturating_sub(lines.len()) as f32
    }
}
