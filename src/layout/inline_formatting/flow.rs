use std::collections::HashMap;

use crate::layout::context::{LayoutContext, LayoutEnv};
use crate::layout::elements::LayoutNode;
use crate::layout::engine::TextRun;
use crate::layout::inline::layout_inline_mixed_sequence_with_env;
use crate::layout::text::{InlineRunCollector, InlineRunContext};
use crate::parser::css::{AncestorInfo, SelectorContext};
use crate::parser::dom::{DomNode, ElementNode, HtmlTag};
use crate::style::computed::{ComputedStyle, compute_style_with_context_with_font_metrics};

use super::{
    AnonymousInlineFormattingContext, InlineContentSequence, InlineFormattingChild,
    InlineFormattingContext, InlineFormattingRole,
};

/// Context-specific ownership of boxes outside an inline run sequence.
///
/// Implementations lay out only independent boxes. Text, generated content,
/// selector positions, and counter traversal remain owned by
/// [`layout_mixed_flow_children`].
pub(crate) trait IndependentFlowLayout {
    fn inline_run_context(&self) -> InlineRunContext {
        InlineRunContext::Standard
    }

    fn lays_out_independently(&self, element: &ElementNode, child: &InlineFormattingChild) -> bool;

    fn layout_independently(
        &mut self,
        element: &ElementNode,
        child: &InlineFormattingChild,
        env: &mut LayoutEnv<'_>,
    );
}

/// Route one mixed child sequence through the shared inline collector.
///
/// Every DOM item is classified against the same complete sibling sequence.
/// Inline participants are collected by [`InlineRunCollector`]; only a policy-
/// approved independent box escapes to the formatting-context implementation.
pub(crate) fn layout_mixed_flow_children<'dom>(
    sequence: InlineContentSequence<'dom>,
    parent_style: &ComputedStyle,
    runs: &mut Vec<TextRun>,
    ancestors: &[AncestorInfo<'dom>],
    env: &mut LayoutEnv<'_>,
    independent_layout: &mut impl IndependentFlowLayout,
) {
    let children =
        InlineFormattingContext::new(parent_style, env.rules, ancestors, env.font_metrics())
            .children(sequence);
    let mut element_index = 0;
    let run_context = independent_layout.inline_run_context();

    for (node_index, node) in sequence.nodes().iter().enumerate() {
        let DomNode::Element(element) = node else {
            InlineRunCollector::new(env.rules, env.fonts, env.counter_state, &mut *env.resources)
                .in_context(run_context)
                .collect(
                    sequence.item(node_index),
                    parent_style,
                    runs,
                    None,
                    ancestors,
                );
            continue;
        };

        let Some(child) = children.get(element_index) else {
            element_index = element_index.saturating_add(1);
            continue;
        };
        element_index = element_index.saturating_add(1);

        if child.role == InlineFormattingRole::Hidden {
            continue;
        }
        if independent_layout.lays_out_independently(element, child) {
            independent_layout.layout_independently(element, child, env);
        } else {
            InlineRunCollector::new(env.rules, env.fonts, env.counter_state, &mut *env.resources)
                .in_context(run_context)
                .collect(
                    sequence.item(node_index),
                    parent_style,
                    runs,
                    None,
                    ancestors,
                );
        }
    }
}

/// Context-specific layout of the block-level and out-of-flow boxes in an
/// [`AnonymousBlockFlow`]. The flow owns the inline content around them.
pub(crate) trait BlockLevelLayout {
    fn layout_block(
        &mut self,
        element: &ElementNode,
        child: &InlineFormattingChild,
        output: &mut Vec<LayoutNode>,
        env: &mut LayoutEnv<'_>,
    );
}

impl<F> BlockLevelLayout for F
where
    F: FnMut(&ElementNode, &InlineFormattingChild, &mut Vec<LayoutNode>, &mut LayoutEnv<'_>),
{
    fn layout_block(
        &mut self,
        element: &ElementNode,
        child: &InlineFormattingChild,
        output: &mut Vec<LayoutNode>,
        env: &mut LayoutEnv<'_>,
    ) {
        self(element, child, output, env);
    }
}

/// The children of a block container whose inline-level content sits beside
/// block-level boxes, laid out in source order.
///
/// CSS 2.1 §9.2.1.1 wraps every maximal run of inline content between
/// block-level siblings in an anonymous block box. Each run therefore keeps
/// its place among the blocks: text after a block stays after it, and inline
/// elements (`<span>`, `<img>`) in the run are laid out with the text around
/// them rather than collected elsewhere or dropped.
pub(crate) struct AnonymousBlockFlow<'a, 'dom> {
    parent_style: &'a ComputedStyle,
    /// The container's content box, which every child is laid out in.
    content: &'a LayoutContext,
    ancestors: &'a [AncestorInfo<'dom>],
}

impl<'a, 'dom> AnonymousBlockFlow<'a, 'dom> {
    pub(crate) const fn new(
        parent_style: &'a ComputedStyle,
        content: &'a LayoutContext,
        ancestors: &'a [AncestorInfo<'dom>],
    ) -> Self {
        Self {
            parent_style,
            content,
            ancestors,
        }
    }

    /// Lay out `sequence` into `output`: anonymous blocks for the inline runs,
    /// and `blocks` for each block-level or out-of-flow child. An out-of-flow
    /// box does not end the inline run around it.
    pub(crate) fn layout(
        &self,
        sequence: InlineContentSequence<'_>,
        output: &mut Vec<LayoutNode>,
        env: &mut LayoutEnv<'_>,
        blocks: &mut impl BlockLevelLayout,
    ) {
        let formatting = InlineFormattingContext::new(
            self.parent_style,
            env.rules,
            self.ancestors,
            env.font_metrics(),
        );
        let children = formatting.children(sequence);
        let mut atomic_segments = formatting
            .atomic_layout_segments(sequence)
            .into_iter()
            .peekable();
        let mut runs = Vec::new();
        // A row of atomic inlines at either end of the sequence lays out the
        // generated content at that end; otherwise the runs carry it.
        let mut before_pending = true;
        let mut after_pending = true;
        if atomic_segments
            .peek()
            .is_none_or(|segment| segment.start() != sequence.start())
        {
            before_pending = false;
            sequence.append_before(&mut runs, env.fonts, env.counter_state, &mut *env.resources);
        }

        // The row's box is the anonymous block's: it inherits from the
        // container, but the container's own box (border, padding, background,
        // margins, size) stays on the container.
        let anonymous_style = atomic_segments.peek().map(|_| {
            compute_style_with_context_with_font_metrics(
                HtmlTag::Div,
                None,
                self.parent_style,
                &[],
                "",
                &[],
                None,
                &HashMap::new(),
                &SelectorContext::default(),
                env.font_metrics(),
            )
        });

        let mut atomic_segment_end = sequence.start();
        let mut element_index = 0;
        for node_index in sequence.start()..sequence.end() {
            let Some(node) = sequence.source_nodes().get(node_index) else {
                break;
            };
            if let Some(segment) = atomic_segments.next_if(|segment| segment.start() == node_index)
            {
                self.flush(&mut runs, output, env);
                if layout_inline_mixed_sequence_with_env(
                    segment.bounded_by(sequence),
                    anonymous_style.as_ref().unwrap_or(self.parent_style),
                    self.content,
                    output,
                    self.ancestors,
                    env,
                ) {
                    atomic_segment_end = segment.end();
                    after_pending &= segment.end() != sequence.end();
                } else if before_pending {
                    sequence.append_before(
                        &mut runs,
                        env.fonts,
                        env.counter_state,
                        &mut *env.resources,
                    );
                }
                before_pending = false;
            }
            let child = match node {
                DomNode::Text(_) => None,
                DomNode::Element(element) => {
                    let child = children.get(element_index).map(|child| (element, child));
                    element_index += 1;
                    child
                }
            };
            if node_index < atomic_segment_end {
                continue;
            }
            let item =
                InlineContentSequence::segment(sequence.source_nodes(), node_index, node_index + 1);
            match child {
                Some((_, child)) if child.role == InlineFormattingRole::Hidden => {}
                Some((element, child)) if child.role == InlineFormattingRole::Outside => {
                    self.flush(&mut runs, output, env);
                    blocks.layout_block(element, child, output, env);
                }
                Some((element, child)) if child.role == InlineFormattingRole::OutOfFlow => {
                    blocks.layout_block(element, child, output, env);
                }
                _ => InlineRunCollector::new(
                    env.rules,
                    env.fonts,
                    env.counter_state,
                    &mut *env.resources,
                )
                .collect(item, self.parent_style, &mut runs, None, self.ancestors),
            }
        }
        if after_pending {
            sequence.append_after(&mut runs, env.fonts, env.counter_state, &mut *env.resources);
        }
        self.flush(&mut runs, output, env);
    }

    fn flush(&self, runs: &mut Vec<TextRun>, output: &mut Vec<LayoutNode>, env: &LayoutEnv<'_>) {
        output.extend(
            AnonymousInlineFormattingContext::new(
                self.parent_style,
                self.content.available_width(),
                env.fonts,
            )
            .layout_runs(std::mem::take(runs)),
        );
    }
}
