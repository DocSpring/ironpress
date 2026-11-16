//! The body's own box: its border, and its background when the root's
//! background is a different one (otherwise the body's background is the
//! canvas's, CSS 2.1 §14.2).
//!
//! ironpress lays out the body's children straight onto the pages, so the body
//! has no box of its own in the flow. Its box is painted behind them, cut into
//! one slice per page as Chrome prints it (`box-decoration-break: slice`): the
//! side borders on every page, the top border on the first page only, and the
//! bottom border on the last page, below its content and bottom padding.

use super::{
    BoxFragmentation, BoxFragmentationOwner, BoxModel, BoxPaint, BoxPaintOwner, LayoutElement,
    LayoutNode, LayoutSize, LayoutVisitor, LayoutVisitorMut, PageAreaInFlowSpace, PageContentRole,
    PaintGroup, PaintGroupOwner, Positioning, PositioningOwner, Stacking, StackingRole, TextBlock,
    TextFragmentation,
};
use crate::layout::engine::LayoutBorder;
use crate::style::computed::{ComputedStyle, ZIndex};
use crate::types::Point;

/// The body's box in flow coordinates: x 0 is the body's content edge, y 0
/// the page area's top.
#[derive(Debug, Clone, Copy)]
pub(crate) struct BodyBoxGeometry {
    /// The left edge of the border box.
    pub(crate) left: f32,
    /// The border box's width.
    pub(crate) width: f32,
    /// The top of the border box on the first page.
    pub(crate) top: f32,
    /// Bottom padding.
    pub(crate) padding_bottom: f32,
    /// Whether bottom padding or a bottom border keeps the last child's
    /// bottom margin inside the body (CSS 2.1 §8.3.1).
    pub(crate) holds_last_margin: bool,
    /// The border box's height, when the body's `height` sets it.
    pub(crate) height: Option<f32>,
}

/// Where the body's flow ends on its last page.
#[derive(Debug, Clone, Copy)]
pub(crate) struct FlowEnd {
    /// The bottom of the last in-flow box's border box.
    pub(crate) content_bottom: f32,
    /// That box's bottom margin.
    pub(crate) margin_bottom: f32,
}

#[derive(Debug, Clone)]
pub(crate) struct BodyBox {
    paint_box: TextBlock,
    border: LayoutBorder,
    geometry: BodyBoxGeometry,
}

impl BodyBox {
    /// `paints_background`: whether the body's background is its own, not the
    /// canvas's.
    pub(crate) fn new(
        style: &ComputedStyle,
        geometry: BodyBoxGeometry,
        paints_background: bool,
    ) -> Self {
        let mut paint = BoxPaint {
            group: PaintGroup {
                stacking: Stacking {
                    z_index: ZIndex::integer(-1),
                    role: StackingRole::PageBackdrop,
                },
                ..Default::default()
            },
            ..Default::default()
        };
        if paints_background {
            paint.background = super::BackgroundPaint::from_style(style);
        }
        let paint_box = TextBlock {
            paint,
            fragmentation: TextFragmentation {
                box_fragmentation: BoxFragmentation {
                    content_role: PageContentRole::RepeatedDecoration,
                    ..Default::default()
                },
                ..Default::default()
            },
            // Placed on each page by `fit_page`.
            positioning: Positioning::absolute_at(Point::new(geometry.left, geometry.top)),
            ..Default::default()
        };
        Self {
            paint_box,
            border: LayoutBorder::from_computed(&style.border, style.color),
            geometry,
        }
    }

    /// Cut the box to one page's slice, and return its top. `first`: the page
    /// holds the start of the body; `end`: where the flow ends, on the body's
    /// last page.
    pub(crate) fn fit_page(
        &mut self,
        page_area: PageAreaInFlowSpace,
        first: bool,
        end: Option<FlowEnd>,
    ) -> f32 {
        let geometry = self.geometry;
        let mut border = self.border;
        let page_area_bottom = page_area.origin.y + page_area.size.height;
        // A set height that ends on the first page ends the box there, and the
        // later pages have none of it. A taller one is cut at each page.
        if let Some(height) = geometry
            .height
            .filter(|height| geometry.top + height <= page_area_bottom)
        {
            let (width, height) = if first {
                (geometry.width, height)
            } else {
                border = LayoutBorder::default();
                (0.0, 0.0)
            };
            self.paint_box.box_model = BoxModel {
                size: LayoutSize::fixed(width, Some((height - border.vertical_width()).max(0.0))),
                border,
                ..Default::default()
            };
            self.paint_box.positioning =
                Positioning::absolute_at(Point::new(geometry.left, geometry.top));
            return geometry.top;
        }
        let top = if first {
            geometry.top
        } else {
            border.top.width = 0.0;
            page_area.origin.y
        };
        let bottom = match end {
            Some(end) => {
                let margin = if geometry.holds_last_margin {
                    end.margin_bottom
                } else {
                    0.0
                };
                end.content_bottom + margin + geometry.padding_bottom + border.bottom.width
            }
            None => {
                border.bottom.width = 0.0;
                page_area_bottom
            }
        };
        let border_box_height = (bottom - top).max(0.0);
        let padding_box_height = (border_box_height - border.vertical_width()).max(0.0);
        self.paint_box.box_model = BoxModel {
            size: LayoutSize::fixed(geometry.width, Some(padding_box_height)),
            border,
            ..Default::default()
        };
        self.paint_box.positioning = Positioning::absolute_at(Point::new(geometry.left, top));
        top
    }
}

impl LayoutElement for BodyBox {
    fn clone_box(&self) -> LayoutNode {
        Box::new(self.clone())
    }

    fn accept(&self, visitor: &mut dyn LayoutVisitor) {
        visitor.visit_text_block(&self.paint_box);
    }

    fn accept_mut(&mut self, visitor: &mut dyn LayoutVisitorMut) {
        visitor.visit_text_block(&mut self.paint_box);
    }

    fn positioning_owner(&self) -> Option<&dyn PositioningOwner> {
        Some(&self.paint_box)
    }

    fn positioning_owner_mut(&mut self) -> Option<&mut dyn PositioningOwner> {
        Some(&mut self.paint_box)
    }

    fn paint_group_owner(&self) -> Option<&dyn PaintGroupOwner> {
        Some(&self.paint_box)
    }

    fn paint_group_owner_mut(&mut self) -> Option<&mut dyn PaintGroupOwner> {
        Some(&mut self.paint_box)
    }

    fn box_reference_geometry(&self) -> Option<&dyn super::BoxReferenceGeometry> {
        Some(&self.paint_box.box_model)
    }

    fn box_paint_owner(&self) -> Option<&dyn BoxPaintOwner> {
        Some(&self.paint_box)
    }

    fn box_paint_owner_mut(&mut self) -> Option<&mut dyn BoxPaintOwner> {
        Some(&mut self.paint_box)
    }

    fn in_flow_paint_phase_owner(&self) -> Option<&dyn BoxPaintOwner> {
        Some(&self.paint_box)
    }

    fn box_fragmentation_owner(&self) -> Option<&dyn BoxFragmentationOwner> {
        Some(&self.paint_box)
    }

    fn box_fragmentation_owner_mut(&mut self) -> Option<&mut dyn BoxFragmentationOwner> {
        Some(&mut self.paint_box)
    }

    fn body_box_mut(&mut self) -> Option<&mut BodyBox> {
        Some(self)
    }

    fn has_own_page_spanning_graphical_effect(&self) -> bool {
        self.paint_box.has_own_page_spanning_graphical_effect()
    }

    fn contributes_to_normal_flow(&self) -> bool {
        false
    }

    fn page_content_role(&self) -> PageContentRole {
        PageContentRole::RepeatedDecoration
    }
}

#[cfg(test)]
impl BodyBox {
    /// The slice's box and paint, as `fit_page` left them.
    pub(crate) fn slice(&self) -> (&BoxModel, &BoxPaint) {
        (&self.paint_box.box_model, &self.paint_box.paint)
    }
}
