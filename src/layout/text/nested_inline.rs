//! Elements inside inline content that aren't text.
//!
//! A block container lays out its own block-level and atomic children, but the children of an inline element
//! (`<span>`, `<a>`, `<strong>`) and the contents of an inline-block reach only the run collector. It dropped every
//! element there that wasn't text or an inline-block with an inline tag, with everything inside it: an `<img>` in a
//! link, the `<div>`s in an inline-block, an inline-flex in a `<span>`. Each one is now inline content: an atomic
//! inline box stays a box in the line and an image becomes an image box. A block inside an inline element is laid out
//! as a block between the lines by the block that holds them, which records where it goes
//! ([`crate::layout::inline_formatting::blocks_in_inline`]). Where nothing lays it out that way (an inline-block's
//! contents), a block becomes lines of its own, as do the blocks inside it, without their own backgrounds, borders or
//! margins.

use super::{
    AncestorInfo, AtomicInlineKind, ComputedStyle, CounterState, CssRule, ElementNode,
    GeneratedContentStyles, HashMap, HtmlTag, InlineBox, InlineBoxPaint, InlineContentSequence,
    InlineFormattingRole, InlineRunContext, LayoutBorder, TextRun, TtfFont,
    collect_text_runs_inner, resolve_style_font_family, text_run_line_height_factor,
    used_font_size,
};
use crate::layout::images::{ElementImage, decode_element_image, raster_image_size};
use crate::layout::inline_formatting::is_block_in_inline;

use crate::parser::css::SelectorContext;
use crate::style::computed::Float;
use crate::types::EdgeSizes;

/// How the run collector takes an element.
pub(super) enum CollectedAs {
    /// Text runs, or one atomic inline box
    Runs,
    /// An atomic inline box where it appears: a float in inline content, which the collector can't move to a side
    FloatBox,
    /// An image inline box
    Image,
    /// A block inside inline content: laid out between the lines when the collector records blocks, otherwise lines
    /// of its own, between forced breaks
    Lines,
    /// Nothing: laid out by its formatting context, or not shown
    Skipped,
}

impl CollectedAs {
    /// `nested` is true inside an inline element or an inline-block's contents, where nothing but the collector
    /// lays out the element.
    pub(super) fn of(
        context: InlineRunContext,
        nested: bool,
        role: InlineFormattingRole,
        element: &ElementNode,
        style: &ComputedStyle,
    ) -> Self {
        let image = matches!(
            role,
            InlineFormattingRole::Atomic(
                AtomicInlineKind::ReplacedImage | AtomicInlineKind::ReplacedSvg
            )
        );
        if context.accepts(role, element) {
            return if image { Self::Image } else { Self::Runs };
        }
        if !nested {
            return Self::Skipped;
        }
        match role {
            InlineFormattingRole::Atomic(
                AtomicInlineKind::ReplacedImage | AtomicInlineKind::ReplacedSvg,
            ) => Self::Image,
            InlineFormattingRole::Atomic(_) => Self::Runs,
            _ if is_block_in_inline(element, role, style) => Self::Lines,
            InlineFormattingRole::Outside if matches!(style.float, Float::Left | Float::Right) => {
                Self::FloatBox
            }
            InlineFormattingRole::Outside => Self::Skipped,
            InlineFormattingRole::Hidden
            | InlineFormattingRole::OutOfFlow
            | InlineFormattingRole::Text => Self::Skipped,
        }
    }
}

/// A forced line break in `style`'s font, as `<br>` makes.
pub(super) fn line_break(style: &ComputedStyle, fonts: &HashMap<String, TtfFont>) -> TextRun {
    TextRun {
        text: "\n".to_string(),
        font_size: used_font_size(style, fonts),
        font_family: resolve_style_font_family(style, fonts),
        line_height_factor: text_run_line_height_factor(style, fonts),
        ..Default::default()
    }
}

fn ends_line(runs: &[TextRun], first_run: usize) -> bool {
    runs.len() <= first_run || runs.last().is_some_and(|run| run.text == "\n")
}

/// What a block nested in inline content needs to collect its contents.
pub(super) struct NestedBlock<'a, 'dom> {
    pub(super) element: &'dom ElementNode,
    pub(super) style: &'a ComputedStyle,
    pub(super) selector_context: &'a SelectorContext<'dom>,
    pub(super) link_url: Option<&'a str>,
    pub(super) context: InlineRunContext,
}

/// The collector's state shared by every element it visits.
pub(super) struct CollectorState<'a, 'r, 'c> {
    pub(super) rules: &'a [CssRule],
    pub(super) fonts: &'a HashMap<String, TtfFont>,
    pub(super) counter_state: &'r mut CounterState,
    pub(super) resources: &'r mut crate::security::resources::ResourceLoader,
    pub(super) collection: &'r mut super::CollectionState<'c>,
}

/// Collect a block's contents as lines of their own: a forced break before them unless a line has just ended (or
/// nothing came before), their runs in the block's style, and a forced break after them.
pub(super) fn collect_block_lines(
    block: NestedBlock<'_, '_>,
    parent_style: &ComputedStyle,
    first_run: usize,
    runs: &mut Vec<TextRun>,
    state: CollectorState<'_, '_, '_>,
) {
    if !ends_line(runs, first_run) {
        runs.push(line_break(parent_style, state.fonts));
    }
    let generated = GeneratedContentStyles::resolve(
        block.element,
        block.style,
        state.rules,
        block.selector_context,
        state.fonts,
    );
    let mut ancestors: Vec<AncestorInfo> = block.selector_context.ancestors.clone();
    ancestors.push(block.selector_context.as_ancestor(block.element));
    let block_start = runs.len();
    // Blocks inside it are lines of their own too, rather than dropped
    state.collection.block_lines_depth += 1;
    collect_text_runs_inner(
        InlineContentSequence::with_generated(
            &block.element.children,
            generated.boxes(block.element),
        ),
        block.style,
        runs,
        block.link_url,
        state.rules,
        state.fonts,
        block.context,
        false,
        &ancestors,
        state.counter_state,
        state.resources,
        state.collection,
    );
    state.collection.block_lines_depth -= 1;
    if runs.len() > block_start && !ends_line(runs, block_start) {
        runs.push(line_break(block.style, state.fonts));
    }
}

/// An `<img>` or an inline `<svg>` in inline content as an atomic box, sized as the block path sizes it. Runs are
/// collected before the block's width is known, so the box is sized here as if the width were unknown (a percentage
/// is `auto`) and keeps what it needs to be sized again ([`InlineReplacedSizing`]) when its line is wrapped.
pub(super) fn image_box(
    element: &ElementNode,
    style: &ComputedStyle,
    resources: &mut crate::security::resources::ResourceLoader,
) -> Option<InlineBox> {
    let mut boxed = if element.tag == HtmlTag::Svg {
        let svg =
            crate::layout::images::svg_element_box(element, style, f32::INFINITY, f32::INFINITY)?;
        svg_box(svg, style)
    } else {
        match decode_element_image(resources, element)? {
            ElementImage::Raster(image) => raster_box(image, element, style, f32::INFINITY),
            ElementImage::Svg(tree) => svg_box(
                crate::layout::images::svg_image_box(
                    *tree,
                    element,
                    f32::INFINITY,
                    f32::INFINITY,
                    style,
                ),
                style,
            ),
        }
    };
    boxed.replaced_sizing = Some(Box::new(InlineReplacedSizing {
        element: element.clone(),
        style: style.clone(),
    }));
    Some(boxed)
}

/// What an `<img>` or `<svg>` inside an inline element needs to be sized against the width of the block it's in, as
/// one directly in the block is: its element and its style.
#[derive(Debug, Clone)]
pub(crate) struct InlineReplacedSizing {
    element: ElementNode,
    style: ComputedStyle,
}

impl InlineBox {
    /// Size this box again against `available_width`, the width of the block its line is in, when it's a replaced
    /// element collected from inside an inline element. Each wrap sizes it from its element and style, so a
    /// measuring pass at another width doesn't stick.
    pub(crate) fn size_replaced_within(&mut self, available_width: f32) {
        let Some(sizing) = self.replaced_sizing.take() else {
            return;
        };
        let InlineReplacedSizing { element, style } = &*sizing;
        if let Some(image) = self.image.take() {
            let resized = raster_box(image, element, style, available_width);
            self.width = resized.width;
            self.height = resized.height;
            self.paint.border_radii = resized.paint.border_radii;
            self.image = resized.image;
        } else if let Some(svg) = self.svg.take() {
            let resized = if element.tag == HtmlTag::Svg {
                crate::layout::images::svg_element_box(
                    element,
                    style,
                    available_width,
                    f32::INFINITY,
                )
            } else {
                Some(crate::layout::images::svg_image_box(
                    svg.tree.clone(),
                    element,
                    available_width,
                    f32::INFINITY,
                    style,
                ))
            };
            match resized.map(|resized| svg_box(resized, style)) {
                Some(resized) => {
                    self.width = resized.width;
                    self.height = resized.height;
                    self.paint.border_radii = resized.paint.border_radii;
                    self.svg = resized.svg;
                }
                None => self.svg = Some(svg),
            }
        }
        self.replaced_sizing = Some(sizing);
    }
}

/// A raster `<img>` as an atomic box `available_width` can constrain, fitted into its content box as `object-fit`
/// says.
fn raster_box(
    image: crate::layout::engine::RasterImageAsset,
    element: &ElementNode,
    style: &ComputedStyle,
    available_width: f32,
) -> InlineBox {
    let size = raster_image_size(&image, element, available_width, style);
    InlineBox {
        width: size.width,
        height: size.height,
        margin_left: style.margin.left.max(0.0),
        margin_right: style.margin.right.max(0.0),
        margin_top: style.margin.top.max(0.0),
        margin_bottom: style.margin.bottom.max(0.0),
        paint: InlineBoxPaint {
            background_color: style.background_color,
            border: LayoutBorder::from_computed(&style.border, style.color),
            border_image: style.border_image.paint(),
            border_radii: style.resolve_corner_radii(size.width, size.height),
            ..InlineBoxPaint::default()
        },
        padding: EdgeSizes::ZERO,
        vertical_align: style.vertical_align,
        image: Some(image),
        image_sampling: crate::layout::elements::ImageSampling {
            replaced: crate::layout::elements::ReplacedContent {
                object_fit: style.object_fit,
                object_position: style.object_position,
                fragment: None,
            },
            rendering: style.image_rendering,
        },
        ..InlineBox::default()
    }
}

/// An SVG replaced box (an inline `<svg>`, or an `<img>` showing an SVG document) as an atomic box whose content box
/// draws it.
fn svg_box(svg: crate::layout::elements::Svg, style: &ComputedStyle) -> InlineBox {
    let size = svg.geometry.size;
    InlineBox {
        width: size.width,
        height: size.height,
        margin_left: style.margin.left.max(0.0),
        margin_right: style.margin.right.max(0.0),
        margin_top: style.margin.top.max(0.0),
        margin_bottom: style.margin.bottom.max(0.0),
        paint: InlineBoxPaint {
            background_color: style.background_color,
            border: svg.geometry.border,
            border_image: svg.paint.border_image,
            border_radii: svg.paint.border_radii,
            ..InlineBoxPaint::default()
        },
        padding: EdgeSizes::ZERO,
        vertical_align: style.vertical_align,
        svg: Some(Box::new(crate::layout::inline_box::InlineSvg {
            tree: svg.tree,
            replaced: svg.replaced,
        })),
        ..InlineBox::default()
    }
}
