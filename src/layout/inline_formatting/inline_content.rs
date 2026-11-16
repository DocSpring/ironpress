//! Which atomic inline boxes a parent's text runs hold whole.

use crate::parser::dom::{DomNode, ElementNode};

use super::{
    AtomicInlineKind, InlineContentSequence, InlineFormattingChild, InlineFormattingContext,
    InlineFormattingRole,
};

impl InlineFormattingContext<'_> {
    /// Whether everything inside `element` (a child of this context) is inline content: text, inline elements,
    /// images and inline-blocks that hold only the same. The run collector lays such an inline-block out whole as
    /// an atomic box in its parent's line. A block, a table, a flex or grid container or an SVG inside it needs the
    /// layout of its own formatting context.
    pub(crate) fn holds_only_inline_content(
        &self,
        element: &ElementNode,
        child: &InlineFormattingChild,
    ) -> bool {
        let mut ancestors = self.ancestors.to_vec();
        ancestors.push(
            child
                .source()
                .ancestor(element, element.children.is_empty()),
        );
        let inner =
            InlineFormattingContext::new(&child.style, self.rules, &ancestors, self.font_metrics);
        let children = inner.children(InlineContentSequence::new(&element.children));
        element
            .children
            .iter()
            .filter_map(|node| match node {
                DomNode::Element(element) => Some(element),
                DomNode::Text(_) => None,
            })
            .zip(children.iter())
            .all(|(element, child)| match child.role {
                InlineFormattingRole::Hidden
                | InlineFormattingRole::Atomic(
                    AtomicInlineKind::ReplacedImage | AtomicInlineKind::ReplacedSvg,
                ) => true,
                InlineFormattingRole::Text
                | InlineFormattingRole::Atomic(AtomicInlineKind::InlineBlock) => {
                    inner.holds_only_inline_content(element, child)
                }
                InlineFormattingRole::Atomic(_)
                | InlineFormattingRole::OutOfFlow
                | InlineFormattingRole::Outside => false,
            })
    }
}
