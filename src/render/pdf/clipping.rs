//! Paint-time clipping shared by recursive formatting contexts.

use super::PdfWriter;
use super::geometry::{PdfPoint, RoundedRect};
use super::stacking::{ActiveClip, StackingTraversal};

/// The positioned depth of the first containing block at or inside a clipping box, from the box's own positioned
/// depth (`Positioning::containing_block_depth`) and whether it's a containing block itself: positioned boxes whose
/// containing block is shallower escape its clip
pub(super) const fn first_containing_block_inside(
    positioned_depth: usize,
    establishes: bool,
) -> usize {
    if establishes {
        positioned_depth
    } else {
        positioned_depth + 1
    }
}

/// Enter an `overflow` clip: `command` clips the paint, and link annotations inside are cut to `shape`.
/// `inner_depth` is [`first_containing_block_inside`] the clipping box. Returns the annotation clip to restore
/// with [`leave_overflow_clip`]
pub(super) fn enter_overflow_clip(
    output: &mut String,
    stacking: &mut StackingTraversal,
    pdf_writer: &mut PdfWriter,
    command: String,
    shape: RoundedRect,
    inner_depth: usize,
) -> Option<Vec<PdfPoint>> {
    output.push_str(&command);
    let outline = pdf_writer.annotation_outline(shape);
    stacking.push_clip(ActiveClip::new(command, inner_depth, outline.clone()));
    pdf_writer.enter_annotation_outline(outline)
}

pub(super) fn leave_overflow_clip(
    output: &mut String,
    stacking: &mut StackingTraversal,
    pdf_writer: &mut PdfWriter,
    prior_annotation_clip: Option<Vec<PdfPoint>>,
) {
    stacking.pop_clip();
    output.push_str("Q\n");
    pdf_writer.restore_annotation_clip(prior_annotation_clip);
}

/// One PDF graphics-state clip mirrored into deferred stacking fragments.
///
/// Layout retains complete content. Every renderer enters this scope around
/// the content phase so glyph ink, nested blocks, and positioned descendants
/// are clipped by the same geometry even when stacking defers their paint.
pub(super) struct ContentClip {
    command: String,
    /// The clip's shape, which link annotations inside it are cut to
    shape: RoundedRect,
    /// [`first_containing_block_inside`] the clipping box
    inner_depth: usize,
    /// The annotation clip to restore when the scope finishes
    prior_annotation_clip: Option<Vec<PdfPoint>>,
}

impl ContentClip {
    /// A clip to `shape`: its outline, rounded where it has radii. `inner_depth` is
    /// [`first_containing_block_inside`] the clipping box
    pub(super) fn of(shape: RoundedRect, inner_depth: usize) -> Self {
        Self {
            command: format!("q\n{}W n\n", shape.path_or_rect()),
            shape,
            inner_depth,
            prior_annotation_clip: None,
        }
    }

    pub(super) fn rounded_padding_box(
        geometry: super::PaintBoxGeometry,
        radii: crate::types::CornerRadii,
        inner_depth: usize,
    ) -> Self {
        Self::of(geometry.rounded_padding_box(radii), inner_depth)
    }

    pub(super) fn begin(
        &mut self,
        output: &mut String,
        stacking: &mut StackingTraversal,
        pdf_writer: &mut PdfWriter,
    ) {
        self.prior_annotation_clip = enter_overflow_clip(
            output,
            stacking,
            pdf_writer,
            self.command.clone(),
            self.shape,
            self.inner_depth,
        );
    }

    pub(super) fn finish(
        &self,
        output: &mut String,
        stacking: &mut StackingTraversal,
        pdf_writer: &mut PdfWriter,
    ) {
        leave_overflow_clip(
            output,
            stacking,
            pdf_writer,
            self.prior_annotation_clip.clone(),
        );
    }
}
