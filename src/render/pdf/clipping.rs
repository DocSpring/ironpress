//! Paint-time clipping shared by recursive formatting contexts.

use super::PdfWriter;
use super::geometry::{PdfPoint, RoundedRect};
use super::stacking::StackingTraversal;

/// One PDF graphics-state clip mirrored into deferred stacking fragments.
///
/// Layout retains complete content. Every renderer enters this scope around
/// the content phase so glyph ink, nested blocks, and positioned descendants
/// are clipped by the same geometry even when stacking defers their paint.
pub(super) struct ContentClip {
    command: String,
    /// The clip's shape, which link annotations inside it are cut to
    shape: RoundedRect,
    /// The annotation clip to restore when the scope finishes
    prior_annotation_clip: Option<Vec<PdfPoint>>,
}

impl ContentClip {
    /// A clip to `shape`: its outline, rounded where it has radii
    pub(super) fn of(shape: RoundedRect) -> Self {
        Self {
            command: format!("q\n{}W n\n", shape.path_or_rect()),
            shape,
            prior_annotation_clip: None,
        }
    }

    pub(super) fn rounded_padding_box(
        geometry: super::PaintBoxGeometry,
        radii: crate::types::CornerRadii,
    ) -> Self {
        Self::of(geometry.rounded_padding_box(radii))
    }

    pub(super) fn begin(
        &mut self,
        output: &mut String,
        stacking: &mut StackingTraversal,
        pdf_writer: &mut PdfWriter,
    ) {
        output.push_str(&self.command);
        stacking.push_clip(self.command.clone());
        self.prior_annotation_clip = pdf_writer.enter_annotation_clip(self.shape);
    }

    pub(super) fn finish(
        &self,
        output: &mut String,
        stacking: &mut StackingTraversal,
        pdf_writer: &mut PdfWriter,
    ) {
        stacking.pop_clip();
        output.push_str("Q\n");
        pdf_writer.restore_annotation_clip(self.prior_annotation_clip.clone());
    }
}
