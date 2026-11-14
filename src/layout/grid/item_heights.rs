//! Natural heights of auto-height grid items, measured once per layout.
//!
//! Row sizing lays out an auto-height item to measure its height, and the cell pass then lays it out again. An item
//! holding another grid repeats both layouts for each of its own items, so the work doubled with every level of
//! nesting: twenty nested grids took about a minute. Remembering each measurement means the cell pass and any
//! enclosing measurement reuse it, and the work grows with the square of the depth instead.

use std::collections::HashMap;

use crate::layout::context::LayoutContext;
use crate::parser::dom::ElementNode;

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

/// Everything an item's natural height depends on: the item (identified by its address in the DOM, which outlives the
/// memo), the content width it wraps to, and the sizes its percentages resolve against.
#[derive(Debug, PartialEq, Eq, Hash)]
pub(crate) struct GridItemMeasurement {
    element: usize,
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
            content_width: content_width.to_bits(),
            parent_width: ctx.parent.content_width.to_bits(),
            parent_height: ctx.parent.content_height.map(f32::to_bits),
            percent_width_basis: ctx.parent.percent_width_basis.to_bits(),
            percent_height_basis: ctx.percent_height_cb.map(|cb| cb.height.to_bits()),
            font_size: ctx.parent.font_size.to_bits(),
        }
    }
}
