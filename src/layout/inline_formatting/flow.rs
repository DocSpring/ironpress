use crate::layout::context::{LayoutContext, LayoutEnv};
use crate::layout::elements::LayoutNode;
use crate::layout::engine::TextRun;
use crate::layout::floats::FloatsInText;
use crate::layout::text::{InlineRunCollector, InlineRunContext};
use crate::parser::css::AncestorInfo;
use crate::parser::dom::{DomNode, ElementNode};
use crate::style::computed::{Clear, ComputedStyle};

use crate::layout::block::trim_leading_collapsible_space;
use crate::layout::engine::{
    ElementSiblingContext, LayoutTreeContext, element_is_empty, flatten_element,
};

use super::{
    AnonymousInlineFormattingContext, BlockInInline, InlineContentSequence, InlineFormattingChild,
    InlineFormattingContext, InlineFormattingRole, blocks_in_inline,
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

    /// Where the lines go, for laying out their atomic inline boxes as their boxes.
    fn inline_layout(&self) -> LayoutContext;

    fn lays_out_independently(&self, element: &ElementNode, child: &InlineFormattingChild) -> bool;

    fn layout_independently(
        &mut self,
        element: &ElementNode,
        child: &InlineFormattingChild,
        env: &mut LayoutEnv<'_>,
    );

    /// Whether this context lays out a block-level element inside an inline
    /// child with [`Self::layout_block_in_inline`], beside the runs. Otherwise
    /// the run collector gathers it as lines of its own.
    fn lays_out_blocks_in_inline(&self) -> bool {
        false
    }

    fn layout_block_in_inline(&mut self, _block: &BlockInInline<'_>, _env: &mut LayoutEnv<'_>) {}
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
    let inline_layout = independent_layout.inline_layout();

    for (node_index, node) in sequence.nodes().iter().enumerate() {
        let DomNode::Element(element) = node else {
            InlineRunCollector::for_layout(env, &inline_layout)
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
        } else if child.holds_block && independent_layout.lays_out_blocks_in_inline() {
            let selector_context = child
                .source()
                .as_context()
                .selector_context(ancestors, element_is_empty(element));
            InlineRunCollector::for_layout(env, &inline_layout)
                .in_context(run_context)
                .recording_blocks()
                .collect(
                    sequence.item(node_index),
                    parent_style,
                    runs,
                    None,
                    ancestors,
                );
            for block in blocks_in_inline(
                element,
                &child.style,
                &selector_context,
                env.rules,
                env.font_metrics(),
            ) {
                independent_layout.layout_block_in_inline(&block, env);
            }
        } else {
            InlineRunCollector::for_layout(env, &inline_layout)
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
///
/// A block-level element inside an inline child, at any depth, is laid out the
/// same way: the inline content before it and after it each go in an anonymous
/// block, and it goes between them as a block ([`super::blocks_in_inline`]).
pub(crate) struct AnonymousBlockFlow<'a, 'dom> {
    parent_style: &'a ComputedStyle,
    /// The container's content box, which every child is laid out in.
    content: &'a LayoutContext,
    ancestors: &'a [AncestorInfo<'dom>],
    /// How many positioned boxes contain the container's children.
    positioned_depth: usize,
    /// The first child is an inline element holding a block, and its inline
    /// content before that block is already laid out (on a list item's marker
    /// line).
    first_lead_laid_out: bool,
}

impl<'a, 'dom> AnonymousBlockFlow<'a, 'dom> {
    pub(crate) const fn new(
        parent_style: &'a ComputedStyle,
        content: &'a LayoutContext,
        ancestors: &'a [AncestorInfo<'dom>],
        positioned_depth: usize,
    ) -> Self {
        Self {
            parent_style,
            content,
            ancestors,
            positioned_depth,
            first_lead_laid_out: false,
        }
    }

    /// The first child's inline content before its first block is already
    /// laid out, on a list item's marker line, when `laid_out`.
    pub(crate) const fn with_first_lead_laid_out(mut self, laid_out: bool) -> Self {
        self.first_lead_laid_out = laid_out;
        self
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
        let mut runs = Vec::new();
        let mut floats_in_text = FloatsInText::default();
        sequence.append_before(&mut runs, env.fonts, env.counter_state, &mut *env.resources);

        let mut element_index = 0;
        for node_index in sequence.start()..sequence.end() {
            let Some(node) = sequence.source_nodes().get(node_index) else {
                break;
            };
            let child = match node {
                DomNode::Text(_) => None,
                DomNode::Element(element) => {
                    let child = children.get(element_index).map(|child| (element, child));
                    element_index += 1;
                    child
                }
            };
            let item =
                InlineContentSequence::segment(sequence.source_nodes(), node_index, node_index + 1);
            match child {
                Some((_, child)) if child.role == InlineFormattingRole::Hidden => {}
                // A float after some text floats from the line it occurs on,
                // and the text carries on around it.
                Some((element, child))
                    if child.role == InlineFormattingRole::Outside
                        && FloatsInText::occurs_in_text(
                            &child.style,
                            &runs,
                            &env.floats,
                            output,
                        ) =>
                {
                    let mut laid_out = Vec::new();
                    blocks.layout_block(element, child, &mut laid_out, env);
                    floats_in_text.take(laid_out, &mut runs, output);
                }
                Some((element, child)) if child.role == InlineFormattingRole::Outside => {
                    self.flush(&mut runs, &mut floats_in_text, output, env);
                    blocks.layout_block(element, child, output, env);
                }
                Some((element, child)) if child.role == InlineFormattingRole::OutOfFlow => {
                    blocks.layout_block(element, child, output, env);
                }
                Some((element, child)) if child.holds_block => {
                    let lead_laid_out = self.first_lead_laid_out && node_index == sequence.start();
                    self.split_around_blocks(
                        item,
                        lead_laid_out,
                        element,
                        child,
                        &mut runs,
                        &mut floats_in_text,
                        output,
                        env,
                    );
                }
                _ => InlineRunCollector::for_layout(env, self.content).collect(
                    item,
                    self.parent_style,
                    &mut runs,
                    None,
                    self.ancestors,
                ),
            }
        }
        sequence.append_after(&mut runs, env.fonts, env.counter_state, &mut *env.resources);
        self.flush(&mut runs, &mut floats_in_text, output, env);
    }

    /// Collect `element`, an inline child, into `runs`, and lay out each block-level element inside it as a block
    /// between the runs before it and the runs after it.
    #[allow(clippy::too_many_arguments)]
    fn split_around_blocks(
        &self,
        item: InlineContentSequence<'_>,
        lead_laid_out: bool,
        element: &'dom ElementNode,
        child: &InlineFormattingChild,
        runs: &mut Vec<TextRun>,
        floats_in_text: &mut FloatsInText,
        output: &mut Vec<LayoutNode>,
        env: &mut LayoutEnv<'_>,
    ) {
        let selector_context = child
            .source()
            .as_context()
            .selector_context(self.ancestors, element_is_empty(element));
        let blocks = blocks_in_inline(
            element,
            &child.style,
            &selector_context,
            env.rules,
            env.font_metrics(),
        );
        let runs_before = runs.len();
        let mut collector = InlineRunCollector::for_layout(env, self.content).recording_blocks();
        collector.collect(item, self.parent_style, runs, None, self.ancestors);
        let boundaries = collector.block_boundaries();
        debug_assert_eq!(
            boundaries.len(),
            blocks.len(),
            "the collector passes over the blocks the search finds"
        );
        // Split from the end, so each boundary still counts runs from the start
        let mut after: Vec<Vec<TextRun>> = boundaries
            .iter()
            .rev()
            .map(|&boundary| runs.split_off(boundary.min(runs.len())))
            .collect();
        after.reverse();
        if lead_laid_out {
            runs.truncate(runs_before);
        }
        for (block, mut runs_after) in blocks.iter().zip(after) {
            self.flush(runs, floats_in_text, output, env);
            let context = &block.selector_context;
            flatten_element(
                block.element,
                LayoutTreeContext::new(&block.parent_style, self.content, &context.ancestors)
                    .with_positioned_ancestor_depth(self.positioned_depth)
                    .for_element(
                        ElementSiblingContext::new(context.child_index, context.sibling_count)
                            .with_neighbors(
                                &context.preceding_siblings,
                                &context.following_siblings,
                            ),
                    ),
                output,
                env,
            );
            // The space before the next text starts its line, where it collapses away
            trim_leading_collapsible_space(&mut runs_after);
            runs.append(&mut runs_after);
        }
    }

    fn flush(
        &self,
        runs: &mut Vec<TextRun>,
        floats_in_text: &mut FloatsInText,
        output: &mut Vec<LayoutNode>,
        env: &LayoutEnv<'_>,
    ) {
        if runs.is_empty() {
            floats_in_text.emit(&[], None, output);
            return;
        }
        let width = self.content.available_width();
        // An anonymous block: no margins, padding or clearance of its own
        let around = floats_in_text.around_block(&env.floats, output, 0.0, Clear::None);
        let origin = around.as_ref().map(|floats| floats.text_origin(0.0));
        let floats = around.map(|floats| floats.lines(0.0, 0.0, width));
        let (block, tops) =
            AnonymousInlineFormattingContext::new(self.parent_style, width, env.fonts)
                .layout_runs_among_floats(std::mem::take(runs), floats, &floats_in_text.boxes());
        floats_in_text.emit(&tops, origin, output);
        output.extend(block);
    }
}
