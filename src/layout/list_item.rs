//! The block-level children of a list item.
//!
//! A list item lays its marker and inline content out as one text block, and
//! its nested lists after it. Other block-level children (a `<div>`, a second
//! `<p>`, an element with `display: block`) and inline content that follows a
//! block are laid out here, in source order and at the item's content edge.

use crate::layout::context::{LayoutContext, LayoutEnv};
use crate::layout::elements::LayoutNode;
use crate::layout::engine::{ListContext, TextRun, flatten_element};
use crate::layout::helpers::has_background_paint;
use crate::layout::inline_formatting::{
    AnonymousBlockFlow, GeneratedInlineContent, InlineContentSequence, InlineFormattingChild,
    InlineFormattingContext, InlineFormattingRole,
};
use crate::layout::text::InlineRunCollector;
use crate::layout::traversal::LayoutTreeContext;
use crate::parser::css::AncestorInfo;
use crate::parser::dom::{DomNode, ElementNode, HtmlTag};
use crate::style::computed::{ComputedStyle, Display};

/// The content of a list item after its marker's text block.
pub(crate) struct ListItemBlocks {
    /// The source index the content after the marker's text block starts at.
    tail_start: usize,
    /// Margins of a first block whose text joined the marker's line.
    pub(crate) margin_top: f32,
    pub(crate) margin_bottom: f32,
}

/// Collect the runs of a list item's marker line: its inline content before
/// its first block-level child, or, when there is none, the text of a first
/// block that holds only text and has no box of its own (as a browser puts the
/// marker beside the first line of that block).
///
/// Returns `None`, collecting nothing, when the item's children are inline
/// content followed only by nested lists (and a loose `<p>` first): the list
/// item's own path lays those out.
pub(crate) fn collect_list_item_marker_line<'dom>(
    item: &'dom ElementNode,
    style: &ComputedStyle,
    ancestors: &[AncestorInfo<'dom>],
    env: &mut LayoutEnv<'_>,
    runs: &mut Vec<TextRun>,
) -> Option<ListItemBlocks> {
    if !item
        .children
        .iter()
        .any(|node| matches!(node, DomNode::Element(_)))
    {
        return None;
    }
    let children = InlineFormattingContext::new(style, env.rules, ancestors, env.font_metrics())
        .children(InlineContentSequence::new(&item.children));
    let mut first_block = None;
    let mut content_before_block = false;
    let mut content_after_block = false;
    let mut other_blocks = false;
    let mut element_index = 0;
    for (node_index, node) in item.children.iter().enumerate() {
        let content = match node {
            DomNode::Text(text) => !text.trim().is_empty(),
            DomNode::Element(element) => {
                let child = children.get(element_index)?;
                element_index += 1;
                match child.role {
                    InlineFormattingRole::Hidden | InlineFormattingRole::OutOfFlow => false,
                    InlineFormattingRole::Outside => {
                        if first_block.is_none() {
                            first_block = Some((node_index, element, child));
                        } else if !is_nested_list(element) {
                            other_blocks = true;
                        }
                        false
                    }
                    _ => true,
                }
            }
        };
        if content && first_block.is_some() {
            content_after_block = true;
        } else if content {
            content_before_block = true;
        }
    }
    let (block_index, block, block_child) = first_block?;
    let loose_paragraph = item.tag == HtmlTag::Li && block.tag == HtmlTag::P;
    let item_path_places_first_block =
        is_nested_list(block) || loose_paragraph && !content_before_block;
    if item_path_places_first_block && !other_blocks && !content_after_block {
        return None;
    }

    let leading_runs = runs.len();
    InlineRunCollector::new(env.rules, env.fonts, env.counter_state, &mut *env.resources).collect(
        InlineContentSequence::segment(&item.children, 0, block_index),
        style,
        runs,
        None,
        ancestors,
    );
    if runs.len() == leading_runs
        && (loose_paragraph || holds_only_text(block, block_child, style, ancestors, env))
    {
        let mut block_ancestors = ancestors.to_vec();
        block_ancestors.push(block_child.source().ancestor(block, false));
        InlineRunCollector::new(env.rules, env.fonts, env.counter_state, &mut *env.resources)
            .collect_box_content(
                &block.children,
                &block_child.style,
                runs,
                None,
                &block_ancestors,
            );
        return Some(ListItemBlocks {
            tail_start: block_index + 1,
            margin_top: block_child.style.margin.top,
            margin_bottom: block_child.style.margin.bottom,
        });
    }
    Some(ListItemBlocks {
        tail_start: block_index,
        margin_top: 0.0,
        margin_bottom: 0.0,
    })
}

fn is_nested_list(element: &ElementNode) -> bool {
    matches!(element.tag, HtmlTag::Ul | HtmlTag::Ol)
}

/// Whether a block's text can join the marker's line: the block has no box
/// of its own to paint or size, aligns its text as the item does, and holds
/// only inline text.
fn holds_only_text(
    block: &ElementNode,
    child: &InlineFormattingChild,
    item_style: &ComputedStyle,
    ancestors: &[AncestorInfo],
    env: &LayoutEnv<'_>,
) -> bool {
    let style = &child.style;
    let padding = style.padding;
    let has_box = has_background_paint(style)
        || style.has_border_decoration()
        || [padding.top, padding.right, padding.bottom, padding.left]
            .iter()
            .any(|side| *side != 0.0)
        || style.width.is_some()
        || style.height.is_some()
        || style.min_height.is_some()
        || style.position.is_positioned()
        || style.opacity < 1.0
        || !style.box_shadow.is_empty();
    if style.display != Display::Block || has_box || style.text_align != item_style.text_align {
        return false;
    }
    let mut block_ancestors = ancestors.to_vec();
    block_ancestors.push(child.source().ancestor(block, false));
    let sequence = InlineContentSequence::new(&block.children);
    let children =
        InlineFormattingContext::new(style, env.rules, &block_ancestors, env.font_metrics())
            .children(sequence);
    let mut elements = block.children.iter().filter_map(|node| match node {
        DomNode::Element(element) => Some(element),
        DomNode::Text(_) => None,
    });
    children.iter().all(|child| {
        elements.next().is_some_and(|element| {
            child.role == InlineFormattingRole::Hidden || child.role.uses_text_run_layout(element)
        })
    })
}

/// Where a list item's content after its marker's text block is laid out.
pub(crate) struct ListItemContent<'a, 'dom> {
    /// The item's content box: its width, and its children's containing block.
    pub(crate) layout: &'a LayoutContext,
    /// The item's ancestors and the item itself.
    pub(crate) ancestors: &'a [AncestorInfo<'dom>],
    /// The list the item belongs to.
    pub(crate) list: Option<&'a ListContext>,
    pub(crate) positioned_depth: usize,
    /// The item's content edge, relative to the edge the list's own content
    /// is laid out from.
    pub(crate) inline_start: f32,
}

/// Lay out a list item's content after its marker's text block, in source
/// order: block-level children, and its other inline content in anonymous
/// blocks (CSS 2.1 §9.2.1.1), with the item's `::after` at the end.
pub(crate) fn layout_list_item_blocks(
    item: &ElementNode,
    style: &ComputedStyle,
    after: Option<&ComputedStyle>,
    blocks: &ListItemBlocks,
    content: ListItemContent<'_, '_>,
    output: &mut Vec<LayoutNode>,
    env: &mut LayoutEnv<'_>,
) {
    // A nested list's indent is measured from this item's content edge, which
    // every box laid out here is moved to below.
    let nested_list = content.list.map(|list| match *list {
        ListContext::Unordered { .. } => ListContext::Unordered { indent: 0.0 },
        ListContext::Ordered { index, step, .. } => ListContext::Ordered {
            index,
            step,
            indent: 0.0,
        },
    });
    let sequence = InlineContentSequence::with_generated(
        &item.children,
        GeneratedInlineContent::new(item, None, after),
    )
    .tail(blocks.tail_start);
    let start = output.len();
    AnonymousBlockFlow::new(style, content.layout, content.ancestors).layout(
        sequence,
        output,
        env,
        &mut |element: &ElementNode,
              child: &InlineFormattingChild,
              output: &mut Vec<LayoutNode>,
              env: &mut LayoutEnv<'_>| {
            let list = if is_nested_list(element) {
                nested_list.as_ref()
            } else {
                None
            };
            flatten_element(
                element,
                LayoutTreeContext::new(style, content.layout, content.ancestors)
                    .with_list(list)
                    .with_positioned_ancestor_depth(content.positioned_depth)
                    .for_element(child.source().as_context()),
                output,
                env,
            );
        },
    );
    for node in &mut output[start..] {
        if let Some(owner) = node.positioning_owner_mut() {
            let positioning = owner.positioning_mut();
            if !positioning.scheme.is_absolute() {
                positioning.insets.left += content.inline_start;
            }
        }
    }
}
