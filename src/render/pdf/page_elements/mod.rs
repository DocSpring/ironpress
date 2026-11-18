use super::*;

mod container;
mod container_decoration;
mod flex;
mod media;
mod misc;
mod tables;
mod text;
mod text_lines;

pub(super) use container::render_container;
pub(super) use flex::render_flex_row;
pub(super) use media::{paint_image_box, paint_svg_box, render_image, render_svg};
pub(super) use misc::{paint_math_block, paint_progress_bar};
pub(super) use tables::{render_grid_row, render_table_row};
pub(super) use text::render_text_block;

#[derive(Clone, Copy)]
pub(super) struct PageElementFrame<'a> {
    pub(super) occlusion_coverers: &'a [(PdfRect, usize)],
    pub(super) page_size: PageSize,
    pub(super) margin: Margin,
    pub(super) available_width: f32,
    pub(super) y_pos: f32,
    pub(super) element_index: usize,
    pub(super) page_index: usize,
}

impl PageElementFrame<'_> {
    /// The x of a page-level positioned box's border box. An axis whose
    /// insets are both `auto` keeps the box's static position, in the flow
    /// (CSS 2.1 §10.3.7). Otherwise a box in the initial containing block
    /// (depth 0: a fixed box, or an absolute one with no positioned ancestor)
    /// is placed in the page area, at `page_area_left`, and any other from its
    /// containing block in the flow.
    pub(super) fn positioned_x(
        &self,
        positioning: &crate::layout::elements::Positioning,
        page_area_left: f32,
    ) -> f32 {
        let offset_left = positioning.insets.left;
        match positioning.containing_block {
            _ if positioning.has_static_inline_position() => self.margin.left + offset_left,
            Some(cb) if cb.depth == 0 => page_area_left + cb.x + offset_left,
            Some(cb) => self.margin.left + cb.x + offset_left,
            None => self.margin.left + offset_left,
        }
    }
}
