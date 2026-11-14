//! Elements inside inline content that aren't text.
//!
//! A block container lays out its own block-level and atomic children, but the children of an inline element
//! (`<span>`, `<a>`, `<strong>`) and the contents of an inline-block reach only the run collector. It dropped every
//! element there that wasn't text or an inline-block with an inline tag, with everything inside it: an `<img>` in a
//! link, the `<div>`s in an inline-block, an inline-flex in a `<span>`. Each one is now inline content: an atomic
//! inline box stays a box in the line, an image becomes an image box, and a block becomes lines of its own (CSS 2.1
//! §9.2.1.1), without its own background, border or margins.

use super::{
    AncestorInfo, AtomicInlineKind, ComputedStyle, CounterState, CssRule, ElementNode,
    GeneratedContentStyles, HashMap, HtmlTag, InlineBox, InlineBoxPaint, InlineContentSequence,
    InlineFormattingRole, InlineRunContext, LayoutBorder, TextRun, TtfFont,
    collect_text_runs_inner, resolve_style_font_family, text_run_line_height_factor,
    used_font_size,
};
use crate::layout::images::{ElementImage, decode_element_image, raster_image_size};

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
    /// Lines of its own, between forced breaks
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
        if context.accepts(role, element) {
            return Self::Runs;
        }
        if !nested {
            return Self::Skipped;
        }
        match role {
            InlineFormattingRole::Atomic(AtomicInlineKind::ReplacedImage) => Self::Image,
            InlineFormattingRole::Atomic(_) => Self::Runs,
            InlineFormattingRole::Outside if element.tag == HtmlTag::Svg => Self::Skipped,
            InlineFormattingRole::Outside if matches!(style.float, Float::Left | Float::Right) => {
                Self::FloatBox
            }
            InlineFormattingRole::Outside => Self::Lines,
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
pub(super) struct CollectorState<'a, 'r> {
    pub(super) rules: &'a [CssRule],
    pub(super) fonts: &'a HashMap<String, TtfFont>,
    pub(super) counter_state: &'r mut CounterState,
    pub(super) resources: &'r mut crate::security::resources::ResourceLoader,
    pub(super) next_inline_decoration: &'r mut usize,
}

/// Collect a block's contents as lines of their own: a forced break before them unless a line has just ended (or
/// nothing came before), their runs in the block's style, and a forced break after them.
pub(super) fn collect_block_lines(
    block: NestedBlock<'_, '_>,
    parent_style: &ComputedStyle,
    first_run: usize,
    runs: &mut Vec<TextRun>,
    state: CollectorState<'_, '_>,
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
        state.next_inline_decoration,
    );
    if runs.len() > block_start && !ends_line(runs, block_start) {
        runs.push(line_break(block.style, state.fonts));
    }
}

/// An `<img>` in inline content as an atomic image box, sized as the block path sizes it. An SVG source has no
/// inline box yet and is left out.
pub(super) fn image_box(
    element: &ElementNode,
    style: &ComputedStyle,
    resources: &mut crate::security::resources::ResourceLoader,
) -> Option<InlineBox> {
    let ElementImage::Raster(image) = decode_element_image(resources, element)? else {
        return None;
    };
    let size = raster_image_size(&image, element, f32::INFINITY, style);
    Some(InlineBox {
        width: size.width,
        height: size.height,
        margin_left: style.margin.left.max(0.0),
        margin_right: style.margin.right.max(0.0),
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
        ..InlineBox::default()
    })
}
