use super::{
    BlockFlowParticipant, FragmentPlacement, FragmentPlacementOwner, InlineFlowExtent,
    LayoutElement, LayoutNode, LayoutVisitor, LayoutVisitorMut, PaintGroup, PaintGroupOwner,
};
use crate::layout::engine::LayoutBorderSide;
use crate::layout::engine::{CenteredStroke, GaugeFill, InlineBox, InlineBoxPaint};
use crate::layout::flow_metrics::{BlockMargins, MarginHolder};
use crate::parser::dom::{ElementNode, HtmlTag};
use crate::style::computed::ComputedStyle;
use crate::types::{Color, Size};

#[derive(Debug, Clone, Default)]
pub(crate) struct ColumnRule {
    /// Document-order column immediately before this rule.
    pub(crate) gap_after: usize,
    pub(crate) placement: FragmentPlacement,
    pub(crate) height: f32,
    pub(crate) paint: LayoutBorderSide,
}

impl FragmentPlacementOwner for ColumnRule {
    fn fragment_placement(&self) -> FragmentPlacement {
        self.placement
    }

    fn fragment_source(&self) -> &dyn LayoutElement {
        self
    }
}

impl LayoutElement for ColumnRule {
    fn clone_box(&self) -> LayoutNode {
        Box::new(self.clone())
    }

    fn accept(&self, visitor: &mut dyn LayoutVisitor) {
        visitor.visit_column_rule(self);
    }

    fn accept_mut(&mut self, visitor: &mut dyn LayoutVisitorMut) {
        visitor.visit_column_rule(self);
    }

    fn fragment_placement_owner(&self) -> Option<&dyn FragmentPlacementOwner> {
        Some(self)
    }

    fn contributes_to_normal_flow(&self) -> bool {
        false
    }
}

#[derive(Debug, Clone, Default)]
pub(crate) struct HorizontalRule {
    pub(crate) margins: BlockMargins,
    pub(crate) group: PaintGroup,
}

impl MarginHolder for HorizontalRule {
    fn margins(&self) -> &BlockMargins {
        &self.margins
    }

    fn margins_mut(&mut self) -> &mut BlockMargins {
        &mut self.margins
    }
}

impl PaintGroupOwner for HorizontalRule {
    fn paint_group(&self) -> &PaintGroup {
        &self.group
    }

    fn paint_group_mut(&mut self) -> &mut PaintGroup {
        &mut self.group
    }
}

impl LayoutElement for HorizontalRule {
    fn clone_box(&self) -> LayoutNode {
        Box::new(self.clone())
    }

    fn accept(&self, visitor: &mut dyn LayoutVisitor) {
        visitor.visit_horizontal_rule(self);
    }

    fn accept_mut(&mut self, visitor: &mut dyn LayoutVisitorMut) {
        visitor.visit_horizontal_rule(self);
    }

    fn margin_holder(&self) -> Option<&dyn MarginHolder> {
        Some(self)
    }

    fn margin_holder_mut(&mut self) -> Option<&mut dyn MarginHolder> {
        Some(self)
    }

    fn block_flow_participant(&self) -> Option<&dyn BlockFlowParticipant> {
        Some(self)
    }

    fn block_flow_participant_mut(&mut self) -> Option<&mut dyn BlockFlowParticipant> {
        Some(self)
    }

    fn paint_group_owner(&self) -> Option<&dyn PaintGroupOwner> {
        Some(self)
    }

    fn paint_group_owner_mut(&mut self) -> Option<&mut dyn PaintGroupOwner> {
        Some(self)
    }
}

impl BlockFlowParticipant for HorizontalRule {
    fn collapses_outer_margins(&self) -> bool {
        true
    }

    fn is_in_flow_block(&self) -> bool {
        true
    }
}

#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct ProgressColors {
    pub(crate) fill: Color,
    pub(crate) track: Color,
}

#[derive(Debug, Clone, Default)]
pub(crate) struct ProgressBar {
    pub(crate) fraction: f32,
    pub(crate) size: Size,
    pub(crate) colors: ProgressColors,
    pub(crate) margins: BlockMargins,
    pub(crate) group: PaintGroup,
}

impl ProgressBar {
    const DEFAULT_WIDTH: f32 = 150.0;
    const DEFAULT_HEIGHT: f32 = 12.0;
    const OUTLINE_WIDTH: f32 = 0.5;
    const OUTLINE_GRAY: f32 = 0.6;

    /// The gauge drawn for a `<progress>` or `<meter>` element.
    ///
    /// Block-level and inline-level gauges share this geometry and colouring;
    /// `max_width` caps the default or authored width (a block gauge never
    /// overflows its containing block).
    pub(crate) fn for_element(el: &ElementNode, style: &ComputedStyle, max_width: f32) -> Self {
        let number =
            |name: &str| -> Option<f32> { el.attributes.get(name).and_then(|s| s.parse().ok()) };
        let value = number("value").unwrap_or(0.0);
        let max = number("max").unwrap_or(1.0);
        let fraction = if max > 0.0 {
            (value / max).clamp(0.0, 1.0)
        } else {
            0.0
        };

        let fill_color = if el.tag == HtmlTag::Progress {
            (0.12, 0.53, 0.90)
        } else {
            let low = number("low").unwrap_or(max * 0.25);
            let high = number("high").unwrap_or(max * 0.75);
            if value <= low {
                (0.90, 0.20, 0.20)
            } else if value >= high {
                (0.20, 0.78, 0.35)
            } else {
                (0.95, 0.77, 0.06)
            }
        };

        Self {
            fraction,
            size: Size::new(
                style.width.unwrap_or(Self::DEFAULT_WIDTH).min(max_width),
                style.height.unwrap_or(Self::DEFAULT_HEIGHT),
            ),
            colors: ProgressColors {
                fill: Color::from_srgb(fill_color.0, fill_color.1, fill_color.2, 1.0),
                track: Color::from_srgb(0.88, 0.88, 0.88, 1.0),
            },
            margins: BlockMargins::new(style.margin.top, style.margin.bottom),
            group: PaintGroup::from_style(style),
        }
    }

    /// The outline stroked around the track, centred on the gauge's edge.
    pub(crate) fn outline() -> CenteredStroke {
        CenteredStroke::solid(
            Self::OUTLINE_WIDTH,
            Color::from_srgb(
                Self::OUTLINE_GRAY,
                Self::OUTLINE_GRAY,
                Self::OUTLINE_GRAY,
                1.0,
            ),
        )
    }

    /// The same gauge as an atomic inline box, for a `<progress>` or
    /// `<meter>` that sits in a line of text (its default `display: inline`).
    pub(crate) fn into_inline_box(self, style: &ComputedStyle) -> InlineBox {
        InlineBox {
            width: self.size.width,
            height: self.size.height,
            margin_left: style.margin.left.max(0.0),
            margin_right: style.margin.right.max(0.0),
            paint: InlineBoxPaint {
                background_color: Some(self.colors.track),
                centered_stroke: Some(Self::outline()),
                gauge_fill: (self.fraction > 0.0).then_some(GaugeFill {
                    fraction: self.fraction,
                    color: self.colors.fill,
                }),
                ..InlineBoxPaint::default()
            },
            vertical_align: style.vertical_align,
            ..InlineBox::default()
        }
    }
}

impl MarginHolder for ProgressBar {
    fn margins(&self) -> &BlockMargins {
        &self.margins
    }

    fn margins_mut(&mut self) -> &mut BlockMargins {
        &mut self.margins
    }
}

impl InlineFlowExtent for ProgressBar {
    fn normal_flow_right_edge(&self) -> Option<f32> {
        self.size
            .width
            .is_finite()
            .then_some(self.size.width.max(0.0))
    }
}

impl PaintGroupOwner for ProgressBar {
    fn paint_group(&self) -> &PaintGroup {
        &self.group
    }

    fn paint_group_mut(&mut self) -> &mut PaintGroup {
        &mut self.group
    }
}

impl LayoutElement for ProgressBar {
    fn clone_box(&self) -> LayoutNode {
        Box::new(self.clone())
    }

    fn accept(&self, visitor: &mut dyn LayoutVisitor) {
        visitor.visit_progress_bar(self);
    }

    fn accept_mut(&mut self, visitor: &mut dyn LayoutVisitorMut) {
        visitor.visit_progress_bar(self);
    }

    fn margin_holder(&self) -> Option<&dyn MarginHolder> {
        Some(self)
    }

    fn margin_holder_mut(&mut self) -> Option<&mut dyn MarginHolder> {
        Some(self)
    }

    fn inline_flow_extent(&self) -> Option<&dyn InlineFlowExtent> {
        Some(self)
    }

    fn block_flow_participant(&self) -> Option<&dyn BlockFlowParticipant> {
        Some(self)
    }

    fn block_flow_participant_mut(&mut self) -> Option<&mut dyn BlockFlowParticipant> {
        Some(self)
    }

    fn paint_group_owner(&self) -> Option<&dyn PaintGroupOwner> {
        Some(self)
    }

    fn paint_group_owner_mut(&mut self) -> Option<&mut dyn PaintGroupOwner> {
        Some(self)
    }
}

impl BlockFlowParticipant for ProgressBar {
    fn collapses_outer_margins(&self) -> bool {
        true
    }

    fn is_in_flow_block(&self) -> bool {
        true
    }
}

#[derive(Debug, Clone)]
pub(crate) struct MathBlock {
    pub(crate) layout: crate::layout::math::MathLayout,
    pub(crate) display: bool,
    pub(crate) margins: BlockMargins,
    pub(crate) group: PaintGroup,
}

impl MarginHolder for MathBlock {
    fn margins(&self) -> &BlockMargins {
        &self.margins
    }

    fn margins_mut(&mut self) -> &mut BlockMargins {
        &mut self.margins
    }
}

impl BlockFlowParticipant for MathBlock {
    fn collapses_outer_margins(&self) -> bool {
        true
    }

    fn is_in_flow_block(&self) -> bool {
        true
    }
}

impl PaintGroupOwner for MathBlock {
    fn paint_group(&self) -> &PaintGroup {
        &self.group
    }

    fn paint_group_mut(&mut self) -> &mut PaintGroup {
        &mut self.group
    }
}

impl LayoutElement for MathBlock {
    fn clone_box(&self) -> LayoutNode {
        Box::new(self.clone())
    }

    fn accept(&self, visitor: &mut dyn LayoutVisitor) {
        visitor.visit_math_block(self);
    }

    fn accept_mut(&mut self, visitor: &mut dyn LayoutVisitorMut) {
        visitor.visit_math_block(self);
    }

    fn margin_holder(&self) -> Option<&dyn MarginHolder> {
        Some(self)
    }

    fn margin_holder_mut(&mut self) -> Option<&mut dyn MarginHolder> {
        Some(self)
    }

    fn block_flow_participant(&self) -> Option<&dyn BlockFlowParticipant> {
        Some(self)
    }

    fn block_flow_participant_mut(&mut self) -> Option<&mut dyn BlockFlowParticipant> {
        Some(self)
    }

    fn paint_group_owner(&self) -> Option<&dyn PaintGroupOwner> {
        Some(self)
    }

    fn paint_group_owner_mut(&mut self) -> Option<&mut dyn PaintGroupOwner> {
        Some(self)
    }
}
