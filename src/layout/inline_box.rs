use crate::style::computed::{BorderImagePaint, VerticalAlign};
use crate::types::{Color, CornerRadii, EdgeSizes};

use super::engine::{LayoutBorder, LayoutBorderSide, RasterImageAsset, TextLine};

/// A solid vector stroke centered on a box contour.
///
/// Unlike a CSS border, half of this stroke paints outside the nominal box.
/// UA-supplied geometric markers use that contour without changing layout.
#[derive(Debug, Clone, Copy)]
pub struct CenteredStroke {
    side: LayoutBorderSide,
}

impl CenteredStroke {
    pub const fn solid(width: f32, color: Color) -> Self {
        Self {
            side: LayoutBorderSide::solid(width, color),
        }
    }

    pub(crate) const fn side(self) -> LayoutBorderSide {
        self.side
    }

    pub(crate) fn transform_color(&mut self, transform: impl FnOnce(Color) -> Color) {
        self.side.color = transform(self.side.color);
    }
}

/// Paint owned by one atomic inline box.
///
/// Grouping these values keeps decoration decisions independent from inline
/// sizing, baseline participation, and replaced content.
#[derive(Debug, Clone, Default)]
pub struct InlineBoxPaint {
    pub background_color: Option<Color>,
    pub border: LayoutBorder,
    pub border_image: Option<BorderImagePaint>,
    pub border_radii: CornerRadii,
    pub centered_stroke: Option<CenteredStroke>,
    /// The filled part of a `<progress>` or `<meter>` gauge, painted over the
    /// background from the box's start edge.
    pub gauge_fill: Option<GaugeFill>,
}

/// The filled share of a gauge's border box.
#[derive(Debug, Clone, Copy)]
pub struct GaugeFill {
    /// Filled share of the box width, from 0 to 1.
    pub fraction: f32,
    pub color: Color,
}

/// An inline `<svg>`'s drawing and how it fits its box.
#[derive(Debug, Clone)]
pub(crate) struct InlineSvg {
    pub(crate) tree: crate::parser::svg::SvgTree,
    pub(crate) replaced: crate::layout::elements::ReplacedContent,
}

/// An atomic inline-level box laid out inside a line of text.
#[derive(Debug, Clone, Default)]
pub struct InlineBox {
    /// Border-box width (the painted box width).
    pub width: f32,
    /// Border-box height (used to grow the line box and for vertical-align).
    pub height: f32,
    /// Horizontal margins add inline advance but are not painted.
    pub margin_left: f32,
    pub margin_right: f32,
    /// Vertical margins: the margin box, not the border box, is the box's
    /// extent in its line and what `vertical-align` aligns (CSS 2.1 §10.8).
    pub margin_top: f32,
    pub margin_bottom: f32,
    pub paint: InlineBoxPaint,
    pub padding: EdgeSizes,
    /// CSS `vertical-align` relative to the line baseline.
    pub vertical_align: VerticalAlign,
    /// Distance from the top border edge to this box's baseline.
    pub baseline_ascent: Option<f32>,
    /// Pre-wrapped inner text lines (empty for content-less boxes).
    pub lines: Vec<TextLine>,
    /// Replaced content painted into the content box.
    pub image: Option<RasterImageAsset>,
    /// How `image` is fitted into the content box (`object-fit`, `object-position`, `image-rendering`).
    pub(crate) image_sampling: crate::layout::elements::ImageSampling,
    /// An inline `<svg>` drawn into the content box.
    pub(crate) svg: Option<Box<InlineSvg>>,
    /// The atomic inline's own box laid out as a block lays out its box: an inline-flex, inline-grid, inline-table
    /// or inline-block with its contents, or a replaced element. Its border box is this box's, so the line paints it
    /// where the box goes, with everything a block's box paints (backgrounds, borders, shadows, filters, blending,
    /// its children).
    pub(crate) content: Vec<crate::layout::elements::LayoutNode>,
    /// A replaced element inside an inline element, sized again against its block's width when its line is wrapped.
    pub(crate) replaced_sizing:
        Option<Box<crate::layout::text::nested_inline::InlineReplacedSizing>>,
    /// A built-in list bullet: Chromium lays it out as text in the marker's
    /// font, so the run's font and line-height count in its line box as a
    /// text run's do, whatever the size of the painted shape.
    pub symbol_marker: bool,
    /// CSS relative-position paint offset (right, down) in points.
    pub rel_offset_x: f32,
    pub rel_offset_y: f32,
}

impl InlineBox {
    /// A non-painting inline advance.
    pub fn advance_only(advance: f32) -> Self {
        Self {
            margin_right: advance,
            ..Self::default()
        }
    }

    /// Total inline advance including horizontal margins.
    pub fn outer_width(&self) -> f32 {
        self.width + self.margin_left + self.margin_right
    }

    /// The margin box's height: the box's extent in its line.
    pub fn outer_height(&self) -> f32 {
        self.margin_top + self.height + self.margin_bottom
    }

    /// The distance from the top margin edge to the box's baseline: its last
    /// line's baseline, or with none its bottom margin edge (CSS 2.1 §10.8.1).
    pub fn outer_baseline_ascent(&self) -> f32 {
        self.margin_top
            + self
                .baseline_ascent
                .unwrap_or(self.height + self.margin_bottom)
    }
}
