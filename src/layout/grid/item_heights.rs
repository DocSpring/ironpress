//! Natural heights of auto-height grid items, measured once per layout.
//!
//! Row sizing lays out an auto-height item to measure its height, and the cell pass then lays it out again. An item
//! holding another grid repeats both layouts for each of its own items, so the work doubled with every level of
//! nesting: twenty nested grids took about a minute. Remembering each measurement means the cell pass and any
//! enclosing measurement reuse it, and the work grows with the square of the depth instead.

use std::collections::HashMap;
use std::hash::{DefaultHasher, Hash, Hasher};

use crate::layout::context::LayoutContext;
use crate::parser::dom::{DomNode, ElementNode};

/// The measurements taken so far in one layout, which owns this memo and drops it with the DOM it describes.
#[derive(Debug, Default)]
pub(crate) struct GridItemHeightMemo(HashMap<GridItemMeasurement, f32>);

impl GridItemHeightMemo {
    /// The height measured earlier for this item under the same constraints.
    pub(crate) fn remembered(&self, measurement: &GridItemMeasurement) -> Option<f32> {
        self.0.get(measurement).copied()
    }

    pub(crate) fn remember(&mut self, measurement: GridItemMeasurement, height: f32) {
        self.0.insert(measurement, height);
    }
}

/// Everything an item's natural height depends on: the item, the content width it wraps to, and the sizes its
/// percentages resolve against.
///
/// The item is its address and a fingerprint of its subtree. The address alone isn't enough: an item inside an element
/// that layout copied (a block split across pages, a flex item laid out at a forced size) lives only as long as the
/// copy, and a later copy can be allocated at the same address. Keyed on the address alone, that item took the height
/// measured for whatever was there before it: a receipt's second line moved down by the height of the shop name above
/// it, depending on where the allocator put things.
#[derive(Debug, PartialEq, Eq, Hash)]
pub(crate) struct GridItemMeasurement {
    element: usize,
    subtree: u64,
    content_width: u32,
    parent_width: u32,
    parent_height: Option<u32>,
    percent_width_basis: u32,
    percent_height_basis: Option<u32>,
    font_size: u32,
}

impl GridItemMeasurement {
    pub(crate) fn new(element: &ElementNode, content_width: f32, ctx: &LayoutContext) -> Self {
        Self {
            element: std::ptr::from_ref(element) as usize,
            subtree: subtree_fingerprint(element),
            content_width: content_width.to_bits(),
            parent_width: ctx.parent.content_width.to_bits(),
            parent_height: ctx.parent.content_height.map(f32::to_bits),
            percent_width_basis: ctx.parent.percent_width_basis.to_bits(),
            percent_height_basis: ctx.percent_height_cb.map(|cb| cb.height.to_bits()),
            font_size: ctx.parent.font_size.to_bits(),
        }
    }
}

/// A hash of an element's tag, attributes, and children, all the way down
fn subtree_fingerprint(element: &ElementNode) -> u64 {
    let mut hasher = DefaultHasher::new();
    hash_element(element, &mut hasher);
    hasher.finish()
}

fn hash_element(element: &ElementNode, hasher: &mut DefaultHasher) {
    element.raw_tag_name.hash(hasher);
    // The same attributes in any order
    element
        .attributes
        .iter()
        .map(|(name, value)| {
            let mut attribute = DefaultHasher::new();
            name.hash(&mut attribute);
            value.hash(&mut attribute);
            attribute.finish()
        })
        .fold(0u64, u64::wrapping_add)
        .hash(hasher);
    element.children.len().hash(hasher);
    for child in &element.children {
        match child {
            DomNode::Element(child) => {
                0u8.hash(hasher);
                hash_element(child, hasher);
            }
            DomNode::Text(text) => {
                1u8.hash(hasher);
                text.hash(hasher);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::context::{ParentBox, Viewport};
    use crate::parser::dom::HtmlTag;

    fn context(width: f32) -> LayoutContext {
        LayoutContext {
            viewport: Viewport {
                width,
                height: 800.0,
            },
            parent: ParentBox {
                content_width: width,
                content_height: None,
                font_size: 12.0,
                percent_width_basis: width,
            },
            containing_block: None,
            percent_height_cb: None,
            root_font_size: 12.0,
        }
    }

    fn paragraph(text: &str) -> ElementNode {
        let mut element = ElementNode::new(HtmlTag::P);
        element.children.push(DomNode::Text(text.to_string()));
        element
    }

    #[test]
    fn a_different_item_at_a_remembered_address_is_measured_again() {
        // Layout copies elements and drops the copies, so the next copy can be allocated where an earlier one was. An
        // item there must not take the earlier item's height.
        let ctx = context(200.0);
        let mut slot = Box::new(paragraph("Paihia Corner Store"));
        let mut memo = GridItemHeightMemo::default();
        memo.remember(GridItemMeasurement::new(&slot, 200.0, &ctx), 19.0);
        *slot = paragraph("Receipt R0048213");
        assert_eq!(
            memo.remembered(&GridItemMeasurement::new(&slot, 200.0, &ctx)),
            None
        );
        // The same item is still remembered
        memo.remember(GridItemMeasurement::new(&slot, 200.0, &ctx), 13.0);
        assert_eq!(
            memo.remembered(&GridItemMeasurement::new(&slot, 200.0, &ctx)),
            Some(13.0)
        );
    }
}
