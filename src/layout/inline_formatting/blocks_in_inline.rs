//! Block-level boxes inside a block's inline elements.
//!
//! A `<table>`, `<ul>` or `<div>` inside a `<span>`, `<font>`, `<b>` or a custom element, at any depth, is a block box
//! among the block's lines: the inline elements around it are split, the inline content before it and after it each
//! go in an anonymous block, and it's laid out between them as a block, with its own box (CSS 2.1 §9.2.1.1). It still
//! inherits from the inline element it's in, and selectors see it there.
//!
//! The run collector passes over each one when it's recording ([`super::super::text::InlineRunCollector::
//! recording_blocks`]), noting how many runs come before it, and [`blocks_in_inline`] finds the same elements in the
//! same order, with what laying them out needs.

use crate::parser::css::{CssRule, CssValue, SelectorContext, StyleMap};
use crate::parser::dom::{DomNode, ElementNode, HtmlTag};
use crate::style::computed::{ComputedStyle, Float, compute_style_with_context_with_font_metrics};
use crate::style::font_metrics::FontMetrics;

use super::{InlineFormattingRole, InlineSiblingCursor};

/// A block-level element inside an inline element.
pub(crate) struct BlockInInline<'dom> {
    pub(crate) element: &'dom ElementNode,
    /// The style of the inline element it's in, which it inherits from.
    pub(crate) parent_style: ComputedStyle,
    pub(crate) selector_context: SelectorContext<'dom>,
}

/// Whether the run collector gathers `element`'s children into the lines it's in: an inline element, other than a
/// footnote or math, whose contents it gathers in their own way.
pub(crate) fn collects_contents(
    element: &ElementNode,
    role: InlineFormattingRole,
    style: &ComputedStyle,
) -> bool {
    role == InlineFormattingRole::Text
        && style.float != Float::Footnote
        && !element.attributes.contains_key("data-math")
}

/// Whether an element inside an inline element is a block box among the lines: block-level, in flow and not
/// floated. A block-level `<svg>` is left out, as the run collector leaves it out.
pub(crate) fn is_block_in_inline(
    element: &ElementNode,
    role: InlineFormattingRole,
    style: &ComputedStyle,
) -> bool {
    role == InlineFormattingRole::Outside
        && element.tag != HtmlTag::Svg
        && !matches!(style.float, Float::Left | Float::Right)
}

/// Whether an element inside one of `children` could be block-level: its tag is a block by default, a style attribute
/// names `display`, or a rule that gives `display` a value that can make an element block-level could match it.
/// Finding the blocks means computing the style of every element inside the inline elements, so content that can't
/// hold one skips that.
pub(crate) fn may_hold_block_in_inline(children: &[DomNode], rules: &[CssRule]) -> bool {
    let mut pending: Vec<&DomNode> = children
        .iter()
        .flat_map(|child| match child {
            DomNode::Element(element) if element.tag != HtmlTag::Svg => element.children.iter(),
            _ => [].iter(),
        })
        .collect();
    if pending.is_empty() {
        return false;
    }
    let block_rules: Vec<&CssRule> = rules
        .iter()
        .filter(|rule| rule.pseudo_element.is_none() && displays_block(&rule.declarations))
        .collect();
    while let Some(node) = pending.pop() {
        let DomNode::Element(element) = node else {
            continue;
        };
        let names_display = element
            .style_attr()
            .is_some_and(|style| style.to_ascii_lowercase().contains("display"));
        if !element.tag.is_inline()
            || names_display
            || block_rules
                .iter()
                .any(|rule| selector_may_match(&rule.selector, element))
        {
            return true;
        }
        if element.tag != HtmlTag::Svg {
            pending.extend(element.children.iter());
        }
    }
    false
}

/// Whether `declarations` give `display` a value that can make an element block-level.
fn displays_block(declarations: &StyleMap) -> bool {
    const INLINE_LEVEL: [&str; 6] = [
        "none",
        "contents",
        "initial",
        "unset",
        "revert",
        "revert-layer",
    ];
    declarations.properties.get("display").is_some_and(|value| {
        !matches!(value, CssValue::Keyword(keyword)
            if keyword.to_ascii_lowercase().starts_with("inline")
                || INLINE_LEVEL.iter().any(|inline| keyword.eq_ignore_ascii_case(inline)))
    })
}

/// Whether a selector list could match `element`, judged by the last compound selector of each selector in it: false
/// only when that compound is plain type, class and ID selectors that `element` doesn't have. Anything else (pseudo-
/// classes, attribute selectors, functional notation) could match.
fn selector_may_match(selector_list: &str, element: &ElementNode) -> bool {
    if selector_list.contains(['(', '[', ':', '|', '\\']) {
        return true;
    }
    selector_list.split(',').any(|selector| {
        let compound = selector
            .trim()
            .rsplit(|c: char| c.is_whitespace() || matches!(c, '>' | '+' | '~'))
            .next()
            .unwrap_or_default();
        compound_may_match(compound, element)
    })
}

fn compound_may_match(compound: &str, element: &ElementNode) -> bool {
    let type_end = compound.find(['.', '#']).unwrap_or(compound.len());
    let type_name = &compound[..type_end];
    if !type_name.is_empty()
        && type_name != "*"
        && !type_name.eq_ignore_ascii_case(element.tag_name())
    {
        return false;
    }
    let classes = element.class_list();
    let mut rest = &compound[type_end..];
    while let Some(kind) = rest.chars().next() {
        let name_end = rest[1..].find(['.', '#']).map_or(rest.len(), |end| end + 1);
        let name = &rest[1..name_end];
        let has = match kind {
            '.' => classes.contains(&name),
            _ => element.id() == Some(name),
        };
        if !has {
            return false;
        }
        rest = &rest[name_end..];
    }
    true
}

struct Walker<'a> {
    rules: &'a [CssRule],
    font_metrics: FontMetrics<'a>,
}

/// Every block-level element inside `element`, an inline element with `style` at `selector_context`, in source order:
/// the ones the run collector passes over when it's recording. The search doesn't go into the blocks it finds, which
/// lay out their own contents.
pub(crate) fn blocks_in_inline<'dom>(
    element: &'dom ElementNode,
    style: &ComputedStyle,
    selector_context: &SelectorContext<'dom>,
    rules: &[CssRule],
    font_metrics: FontMetrics<'_>,
) -> Vec<BlockInInline<'dom>> {
    let mut found = Vec::new();
    Walker {
        rules,
        font_metrics,
    }
    .walk(element, style, selector_context, &mut |block| {
        found.push(block);
        true
    });
    found
}

/// Whether `element`, an inline element with `style` at `selector_context`, holds a block-level element.
pub(crate) fn holds_block<'dom>(
    element: &'dom ElementNode,
    style: &ComputedStyle,
    selector_context: &SelectorContext<'dom>,
    rules: &[CssRule],
    font_metrics: FontMetrics<'_>,
) -> bool {
    // Only an element can be a block: most inline elements hold only text
    if !element
        .children
        .iter()
        .any(|child| matches!(child, DomNode::Element(_)))
    {
        return false;
    }
    let mut found = false;
    Walker {
        rules,
        font_metrics,
    }
    .walk(element, style, selector_context, &mut |_| {
        found = true;
        false
    });
    found
}

impl Walker<'_> {
    /// Calls `found` with each block in `element` until it returns false; returns false once it has.
    fn walk<'dom>(
        &self,
        element: &'dom ElementNode,
        style: &ComputedStyle,
        selector_context: &SelectorContext<'dom>,
        found: &mut dyn FnMut(BlockInInline<'dom>) -> bool,
    ) -> bool {
        let mut ancestors = selector_context.ancestors.clone();
        ancestors.push(selector_context.as_ancestor(element));
        let mut siblings = InlineSiblingCursor::starting_at(&element.children, 0);
        for child in &element.children {
            let DomNode::Element(child) = child else {
                continue;
            };
            let child_context = siblings.next_context(child, &ancestors);
            if child.tag == HtmlTag::Br {
                continue;
            }
            let classes = child.class_list();
            let child_style = compute_style_with_context_with_font_metrics(
                child.tag,
                child.style_attr(),
                style,
                self.rules,
                child.tag_name(),
                &classes,
                child.id(),
                &child.attributes,
                &child_context,
                self.font_metrics,
            );
            let role = InlineFormattingRole::of(child, &child_style);
            if is_block_in_inline(child, role, &child_style) {
                let block = BlockInInline {
                    element: child,
                    parent_style: style.clone(),
                    selector_context: child_context,
                };
                if !found(block) {
                    return false;
                }
            } else if collects_contents(child, role, &child_style)
                && !matches!(child.tag, HtmlTag::Progress | HtmlTag::Meter)
                && !self.walk(child, &child_style, &child_context, found)
            {
                return false;
            }
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::css::parse_stylesheet;
    use crate::parser::html::parse_html;

    /// The children of the first `<div>` in `markup`.
    fn div_children(markup: &str) -> Vec<DomNode> {
        fn find(nodes: &[DomNode]) -> Option<Vec<DomNode>> {
            nodes.iter().find_map(|node| match node {
                DomNode::Element(element) if element.tag == HtmlTag::Div => {
                    Some(element.children.clone())
                }
                DomNode::Element(element) => find(&element.children),
                DomNode::Text(_) => None,
            })
        }
        find(&parse_html(markup).unwrap()).expect("a div")
    }

    fn may_hold(markup: &str, css: &str) -> bool {
        may_hold_block_in_inline(&div_children(markup), &parse_stylesheet(css))
    }

    #[test]
    fn content_that_may_hold_a_block_in_an_inline_element() {
        // A block by default, a style attribute or a rule that can match makes one possible
        assert!(may_hold("<div><span><table></table></span></div>", ""));
        assert!(may_hold(
            r#"<div><b><i style="display:block">x</i></b></div>"#,
            ""
        ));
        for css in [
            ".b { display: block }",
            "span .b { display: flex }",
            "i { display: list-item }",
            "* { display: table }",
            "#me { display: block }",
            "i:first-child { display: block }",
            "[data-x] { display: block }",
            ".a, .b { display: block }",
        ] {
            assert!(
                may_hold(r#"<div><span><i class="b" id="me">x</i></span></div>"#, css),
                "{css}"
            );
        }
    }

    #[test]
    fn content_that_cannot_hold_a_block_in_an_inline_element() {
        assert!(!may_hold("<div>text <span>only text</span></div>", ""));
        assert!(!may_hold("<div><span><b>x</b> <i>y</i></span></div>", ""));
        for css in [
            ".logo { display: block }",
            "div.b { display: block }",
            "i.b.c { display: block }",
            "i { display: inline-block }",
            "i { display: none }",
            "i::before { display: block }",
        ] {
            assert!(
                !may_hold(r#"<div><span><i class="b" id="me">x</i></span></div>"#, css),
                "{css}"
            );
        }
    }
}
