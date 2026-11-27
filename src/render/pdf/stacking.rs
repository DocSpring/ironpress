//! Paint fragments participating in a CSS stacking context.
//!
//! Layout is always traversed in source order. Paint fragments are collected
//! separately and sorted only when the nearest stacking context is complete.
//! This lets positioned descendants escape ordinary structural ancestors
//! without losing the geometry already resolved by the recursive renderer.

use super::annotation_clip;
use super::geometry::PdfPoint;
use crate::layout::elements::{LayoutElement, StackingLevel};

#[derive(Debug)]
pub(super) struct StackingPaintFragment {
    level: StackingLevel,
    content: String,
    clips: Vec<String>,
}

impl StackingPaintFragment {
    fn paint_into(self, output: &mut String, ambient_clips: &[String]) {
        let shared_clips = self
            .clips
            .iter()
            .zip(ambient_clips)
            .take_while(|(fragment, ambient)| fragment == ambient)
            .count();
        let additional_clips = &self.clips[shared_clips..];

        for clip in additional_clips {
            output.push_str(clip);
        }
        output.push_str(&self.content);
        for _ in additional_clips {
            output.push_str("Q\n");
        }
    }
}

/// Stable paint schedule for one CSS stacking context.
#[derive(Debug, Default)]
pub(super) struct StackingPaintPlan {
    fragments: Vec<StackingPaintFragment>,
}

impl StackingPaintPlan {
    pub(super) fn push(&mut self, fragment: StackingPaintFragment) {
        if !fragment.content.is_empty() {
            self.fragments.push(fragment);
        }
    }

    pub(super) fn extend(&mut self, fragments: impl IntoIterator<Item = StackingPaintFragment>) {
        self.fragments.extend(
            fragments
                .into_iter()
                .filter(|fragment| !fragment.content.is_empty()),
        );
    }

    pub(super) fn paint_into(mut self, output: &mut String, ambient_clips: &[String]) {
        self.fragments.sort_by_key(|fragment| fragment.level);
        for fragment in self.fragments {
            fragment.paint_into(output, ambient_clips);
        }
    }
}

/// An `overflow` clip around the boxes being painted.
#[derive(Debug, Clone)]
pub(super) struct ActiveClip {
    command: String,
    /// The positioned depth of the first containing block at or inside the clipping box. A positioned box whose
    /// containing block is shallower than this lies outside the clipping box, so the clip doesn't apply to it or to
    /// what it contains (CSS 2.1 §11.1.1)
    inner_depth: usize,
    /// What the clip leaves visible, in page layout coordinates, which link annotations inside it are cut to
    outline: Vec<PdfPoint>,
    /// How many positioned boxes being painted have escaped this clip
    escaped_by: usize,
}

impl ActiveClip {
    pub(super) fn new(command: String, inner_depth: usize, outline: Vec<PdfPoint>) -> Self {
        Self {
            command,
            inner_depth,
            outline,
            escaped_by: 0,
        }
    }

    fn applies(&self) -> bool {
        self.escaped_by == 0
    }
}

/// The clips a positioned box escaped while it's painted, to give back afterwards
#[derive(Debug)]
#[must_use]
pub(super) struct ClipEscape {
    escaped: Vec<usize>,
    prior_annotation_clip: Option<Vec<PdfPoint>>,
}

/// State shared by recursive renderers while they seek their nearest stacking
/// context and preserve any overflow clips crossed on the way there.
#[derive(Debug, Default)]
pub(super) struct StackingTraversal {
    deferred: Vec<StackingPaintFragment>,
    clips: Vec<ActiveClip>,
    /// How many of `clips` were active when the nearest stacking context began: its paint plan is painted inside
    /// them, so no box in it can escape them
    context_floor: usize,
}

impl StackingTraversal {
    pub(super) fn fork(&self) -> Self {
        Self {
            deferred: Vec::new(),
            clips: self.clips.clone(),
            context_floor: self.context_floor,
        }
    }

    pub(super) fn fragment(&self, level: StackingLevel, content: String) -> StackingPaintFragment {
        StackingPaintFragment {
            level,
            content,
            clips: self.active_clips(),
        }
    }

    pub(super) fn defer(&mut self, level: StackingLevel, content: String) {
        let fragment = self.fragment(level, content);
        if !fragment.content.is_empty() {
            self.deferred.push(fragment);
        }
    }

    pub(super) fn marker(&self) -> usize {
        self.deferred.len()
    }

    pub(super) fn take_since(&mut self, marker: usize) -> Vec<StackingPaintFragment> {
        self.deferred.split_off(marker.min(self.deferred.len()))
    }

    pub(super) fn restore(&mut self, fragments: impl IntoIterator<Item = StackingPaintFragment>) {
        self.deferred.extend(fragments);
    }

    pub(super) fn commit(
        &mut self,
        scope: StackingScope,
        output: &mut String,
        local_plan: &mut StackingPaintPlan,
        level: StackingLevel,
        content: String,
        descendants: Vec<StackingPaintFragment>,
    ) {
        if scope.is_local() {
            local_plan.push(self.fragment(level, content));
            local_plan.extend(descendants);
            return;
        }

        if level.is_in_flow() {
            output.push_str(&content);
        } else {
            self.defer(level, content);
        }
        self.restore(descendants);
    }

    pub(super) fn paint_plan(&self, plan: StackingPaintPlan, output: &mut String) {
        plan.paint_into(output, &self.active_clips());
    }

    /// Start collecting a stacking context's paint plan, which is painted inside the clips active now. Returns the
    /// enclosing context's floor, for [`Self::end_context`]
    pub(super) fn begin_context(&mut self, scope: StackingScope) -> usize {
        let prior = self.context_floor;
        if scope.is_local() {
            self.context_floor = self.clips.len();
        }
        prior
    }

    pub(super) fn end_context(&mut self, prior: usize) {
        self.context_floor = prior;
    }

    pub(super) fn push_clip(&mut self, clip: ActiveClip) {
        self.clips.push(clip);
    }

    pub(super) fn pop_clip(&mut self) {
        self.clips.pop();
    }

    /// The clips that apply to what's painted now, outermost first
    pub(super) fn active_clips(&self) -> Vec<String> {
        self.clips
            .iter()
            .filter(|clip| clip.applies())
            .map(|clip| clip.command.clone())
            .collect()
    }

    /// Leave the clips of the boxes inside a positioned box's containing block, at `containing_block_depth`, while
    /// that box is painted: those boxes aren't its ancestors in the containing block chain, so their clips don't
    /// apply to it (CSS 2.1 §11.1.1). Its link annotations, which `annotation_clip` cuts, escape them too. `None`
    /// when it escapes nothing
    pub(super) fn escape_clips(
        &mut self,
        containing_block_depth: usize,
        annotation_clip: &mut Option<Vec<PdfPoint>>,
    ) -> Option<ClipEscape> {
        let escaped: Vec<usize> = (self.context_floor..self.clips.len())
            .filter(|&index| self.clips[index].inner_depth > containing_block_depth)
            .collect();
        if escaped.is_empty() {
            return None;
        }
        for &index in &escaped {
            self.clips[index].escaped_by += 1;
        }
        let visible = self.clips.iter().filter(|clip| clip.applies()).fold(
            None,
            |visible: Option<Vec<PdfPoint>>, clip| {
                Some(match visible {
                    None => clip.outline.clone(),
                    Some(outer) => annotation_clip::intersect(&clip.outline, &outer),
                })
            },
        );
        Some(ClipEscape {
            escaped,
            prior_annotation_clip: std::mem::replace(annotation_clip, visible),
        })
    }

    pub(super) fn restore_clips(
        &mut self,
        escape: ClipEscape,
        annotation_clip: &mut Option<Vec<PdfPoint>>,
    ) {
        for index in escape.escaped {
            self.clips[index].escaped_by -= 1;
        }
        *annotation_clip = escape.prior_annotation_clip;
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(super) enum StackingScope {
    /// This box is a stacking context; resolve deferred descendants here.
    #[default]
    Local,
    /// This box is ordinary; non-in-flow descendants participate in the
    /// nearest ancestor stacking context.
    Ancestor,
}

impl StackingScope {
    pub(super) fn for_element(element: &dyn LayoutElement) -> Self {
        if crate::layout::engine::layout_element_establishes_stacking_context(element) {
            Self::Local
        } else {
            Self::Ancestor
        }
    }

    pub(super) const fn is_local(self) -> bool {
        matches!(self, Self::Local)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plan_is_stable_within_a_stacking_level() {
        let traversal = StackingTraversal::default();
        let mut plan = StackingPaintPlan::default();
        plan.push(traversal.fragment(StackingLevel::positive(2), "second\n".into()));
        plan.push(traversal.fragment(StackingLevel::negative(-1), "first\n".into()));
        plan.push(traversal.fragment(StackingLevel::positive(2), "third\n".into()));

        let mut output = String::new();
        plan.paint_into(&mut output, &[]);
        assert_eq!(output, "first\nsecond\nthird\n");
    }

    #[test]
    fn fragment_reapplies_only_clips_outside_the_ambient_scope() {
        let mut traversal = StackingTraversal::default();
        traversal.push_clip(ActiveClip::new("q\nouter W n\n".into(), 1, Vec::new()));
        traversal.push_clip(ActiveClip::new("q\ninner W n\n".into(), 1, Vec::new()));
        let fragment = traversal.fragment(StackingLevel::in_flow(), "paint\n".into());

        let mut output = String::new();
        fragment.paint_into(&mut output, &["q\nouter W n\n".into()]);
        assert_eq!(output, "q\ninner W n\npaint\nQ\n");
    }
}
