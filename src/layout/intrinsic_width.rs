//! Intrinsic inline sizing shared by block and flex formatting contexts.

use super::context::LayoutEnv;
use super::engine::CounterState;
use super::helpers::{
    collects_as_inline_text, recurses_as_layout_child, resolve_content_with_quotes,
};
use super::inline_formatting::GeneratedContentStyles;
use super::text::{
    collapse_whitespace, estimate_word_width, is_collapsible_space, resolve_style_font_family,
};
use super::traversal::ElementSiblingPosition;
use crate::parser::css::{AncestorInfo, CssRule, SelectorContext};
use crate::parser::dom::{DomNode, ElementNode};
use crate::parser::ttf::TtfFont;
use crate::style::computed::{
    BoxSizing, ComputedStyle, Display, FontFamily, FontWeight, GridTrack, IntrinsicWidthKeyword,
    compute_style_with_context,
};
use std::collections::{HashMap, VecDeque};

/// css-sizing-3 § 5.1 intrinsic inline sizes for one box edge.
///
/// The function returning this value defines whether that edge is the content
/// box or border box. Keeping both sizes together prevents callers from mixing
/// a min-content measurement with a max-content box adjustment.
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct IntrinsicWidths {
    min_content: f32,
    max_content: f32,
}

impl IntrinsicWidths {
    /// Resolve a CSS intrinsic keyword against the available border-box width.
    pub(crate) fn resolve(self, keyword: IntrinsicWidthKeyword, available_width: f32) -> f32 {
        match keyword {
            IntrinsicWidthKeyword::MinContent => self.min_content,
            IntrinsicWidthKeyword::MaxContent => self.max_content,
            IntrinsicWidthKeyword::FitContent => self
                .max_content
                .min(self.min_content.max(available_width.max(0.0))),
        }
    }
}

/// The widths of one run of inline content (text, inline elements and atomic inlines) between blocks. Whitespace
/// collapses across the whole run, as layout collapses it, not within each text node: the space at the end of
/// `Page ` before `<span>1</span>` (or at the start of the span's text) is one space between them, while a space at
/// the run's end hangs and doesn't count.
#[derive(Clone, Copy, Debug, Default)]
struct InlineWidths {
    widths: IntrinsicWidths,
    /// Whether the run has anything in it yet, so a space before it doesn't count
    has_content: bool,
    /// The width of a collapsible space the run ended with, owed if more content follows
    pending_space: f32,
    /// The widest max-content line a forced break has ended
    ended_lines_max: f32,
}

impl InlineWidths {
    /// Append one atomic inline box to the current inline formatting sequence.
    /// Its max-content contribution stays on the same unwrapped line, while
    /// its min-content contribution remains one unbreakable unit.
    fn append_atomic(&mut self, box_widths: IntrinsicWidths) {
        self.widths.max_content += self.pending_space;
        self.widths.min_content = self.widths.min_content.max(box_widths.min_content);
        self.widths.max_content += box_widths.max_content;
        self.has_content = true;
        self.pending_space = 0.0;
    }

    /// End the line at a `<br>`: the content after it starts a new max-content line.
    fn forced_break(&mut self) {
        self.ended_lines_max = self.ended_lines_max.max(self.widths.max_content);
        self.widths.max_content = 0.0;
        self.has_content = false;
        self.pending_space = 0.0;
    }

    /// The run's widths: its min-content and its widest line
    fn finished(&self) -> IntrinsicWidths {
        IntrinsicWidths {
            min_content: self.widths.min_content,
            max_content: self.widths.max_content.max(self.ended_lines_max),
        }
    }
}

fn box_horizontal_extra(style: &ComputedStyle) -> f32 {
    style.padding.horizontal() + style.border.horizontal_width()
}

/// Inputs required at one step of a recursive intrinsic-size measurement.
/// Selector ancestry belongs here because it is part of computed style.
#[derive(Clone, Copy)]
struct IntrinsicMeasurement<'context, 'dom> {
    rules: &'context [CssRule],
    fonts: &'context HashMap<String, TtfFont>,
    /// The counters where the measured element is, for generated content
    counters: &'context CounterState,
    ancestors: &'context [AncestorInfo<'dom>],
    selector: &'context SelectorContext<'dom>,
}

impl<'context, 'dom> IntrinsicMeasurement<'context, 'dom> {
    fn new(
        env: &'context LayoutEnv,
        ancestors: &'context [AncestorInfo<'dom>],
        selector: &'context SelectorContext<'dom>,
    ) -> Self {
        Self {
            rules: env.rules,
            fonts: env.fonts,
            counters: env.counter_state,
            ancestors,
            selector,
        }
    }

    /// The same measurement one level down, for a child with these ancestors and selector context
    fn child(
        self,
        ancestors: &'context [AncestorInfo<'dom>],
        selector: &'context SelectorContext<'dom>,
    ) -> Self {
        Self {
            ancestors,
            selector,
            ..self
        }
    }

    /// An element's `::before` and `::after` styles
    fn generated(self, el: &ElementNode, style: &ComputedStyle) -> GeneratedContentStyles {
        GeneratedContentStyles::resolve(el, style, self.rules, self.selector, self.fonts)
    }

    /// Add a generated box's text to the run it's in, or as a block of its own
    fn add_generated(
        self,
        el: &ElementNode,
        pseudo: Option<&ComputedStyle>,
        inline: &mut InlineWidths,
        contributions: Option<&mut Vec<IntrinsicWidths>>,
    ) {
        let Some(style) = pseudo.filter(|style| {
            !style.content.is_empty()
                && style.display != Display::None
                && !style.position.is_absolute()
        }) else {
            return;
        };
        let text = resolve_content_with_quotes(
            &style.content,
            &el.attributes,
            &mut self.counters.clone(),
            style.quotes.as_deref(),
        );
        let measure = TextMeasurement::new(style, self.fonts);
        let outer = |text: &str| {
            let mut own = InlineWidths::default();
            measure.accumulate(text, &mut own);
            own.widths
                .with_extra(box_horizontal_extra(style) + style.margin.horizontal())
        };
        match (style.display, contributions) {
            (Display::Block | Display::ListItem, Some(contributions)) => {
                flush_inline(contributions, inline);
                contributions.push(outer(&text));
            }
            (Display::InlineBlock, _) => inline.append_atomic(outer(&text)),
            _ => measure.accumulate(&text, inline),
        }
    }
}

/// Compute an element's intrinsic border-box widths without its outer margins.
pub(crate) fn intrinsic_border_box_widths(
    el: &ElementNode,
    style: &ComputedStyle,
    env: &LayoutEnv,
    ancestors: &[AncestorInfo],
    selector: &SelectorContext,
) -> IntrinsicWidths {
    intrinsic_border_box_widths_with(
        el,
        style,
        IntrinsicMeasurement::new(env, ancestors, selector),
    )
}

/// Compute content-driven border-box widths while ignoring the preferred
/// `width` and min/max constraints, as required by `flex-basis: content`.
pub(crate) fn content_intrinsic_border_box_widths(
    el: &ElementNode,
    style: &ComputedStyle,
    env: &LayoutEnv,
    ancestors: &[AncestorInfo],
    selector: &SelectorContext,
) -> IntrinsicWidths {
    let inner = content_intrinsic_widths(
        el,
        style,
        IntrinsicMeasurement::new(env, ancestors, selector),
    );
    let extra = box_horizontal_extra(style);
    IntrinsicWidths {
        min_content: inner.min_content + extra,
        max_content: inner.max_content + extra,
    }
}

fn intrinsic_border_box_widths_with(
    el: &ElementNode,
    style: &ComputedStyle,
    measurement: IntrinsicMeasurement<'_, '_>,
) -> IntrinsicWidths {
    let content = content_intrinsic_widths(el, style, measurement);
    let content = style.width.map_or(content, |width| {
        let width = if style.box_sizing == BoxSizing::BorderBox {
            (width - box_horizontal_extra(style)).max(0.0)
        } else {
            width
        };
        IntrinsicWidths {
            min_content: width,
            max_content: width,
        }
    });

    let mut minimum = content.min_content;
    let mut maximum = content.max_content;
    if let Some(width) = style.max_width {
        let cap = content_box_constraint(width, style);
        minimum = minimum.min(cap);
        maximum = maximum.min(cap);
    }
    if let Some(width) = style.min_width {
        let floor = content_box_constraint(width, style);
        minimum = minimum.max(floor);
        maximum = maximum.max(floor);
    }
    let extra = box_horizontal_extra(style);
    IntrinsicWidths {
        min_content: minimum + extra,
        max_content: maximum + extra,
    }
}

fn content_box_constraint(width: f32, style: &ComputedStyle) -> f32 {
    if style.box_sizing == BoxSizing::BorderBox {
        (width - box_horizontal_extra(style)).max(0.0)
    } else {
        width
    }
}

/// Compute content-box min/max-content widths from an element's children.
fn content_intrinsic_widths(
    el: &ElementNode,
    style: &ComputedStyle,
    measurement: IntrinsicMeasurement<'_, '_>,
) -> IntrinsicWidths {
    // A gauge is replaced: its fallback content isn't shown
    if matches!(
        el.tag,
        crate::parser::dom::HtmlTag::Progress | crate::parser::dom::HtmlTag::Meter
    ) {
        let width = crate::layout::elements::ProgressBar::DEFAULT_WIDTH;
        return IntrinsicWidths {
            min_content: width,
            max_content: width,
        };
    }
    let mut contributions = Vec::new();
    let mut inline = InlineWidths::default();
    let text = TextMeasurement::new(style, measurement.fonts);
    let mut positions = element_positions(el);
    let mut child_ancestors = measurement.ancestors.to_vec();
    child_ancestors.push(measurement.selector.as_ancestor(el));
    let generated = measurement.generated(el, style);
    measurement.add_generated(
        el,
        generated.before(),
        &mut inline,
        Some(&mut contributions),
    );

    for child in &el.children {
        match child {
            DomNode::Text(value) => text.accumulate(value, &mut inline),
            DomNode::Element(child_el) => {
                let Some(child_position) = positions.pop_front() else {
                    continue;
                };
                let child_selector = child_position
                    .as_context()
                    .selector_context(&child_ancestors, child_el.children.is_empty());
                let child_style = child_style(child_el, style, measurement.rules, &child_selector);
                if child_style.display == Display::None || child_style.position.is_absolute() {
                    continue;
                }
                if child_el.tag == crate::parser::dom::HtmlTag::Br {
                    inline.forced_break();
                    continue;
                }

                let is_flex_item = matches!(style.display, Display::Flex | Display::InlineFlex);
                let is_atomic_inline = matches!(child_el.tag, crate::parser::dom::HtmlTag::Img)
                    || matches!(
                        child_style.display,
                        Display::InlineBlock
                            | Display::InlineFlex
                            | Display::InlineGrid
                            | Display::InlineTable
                    );
                let establishes_block = is_flex_item
                    || matches!(
                        child_style.display,
                        Display::Block
                            | Display::ListItem
                            | Display::Flex
                            | Display::Grid
                            | Display::Table
                    )
                    || (recurses_as_layout_child(child_el.tag) && !is_atomic_inline);
                if establishes_block {
                    flush_inline(&mut contributions, &mut inline);
                    let border_box = intrinsic_border_box_widths_with(
                        child_el,
                        &child_style,
                        measurement.child(&child_ancestors, &child_selector),
                    );
                    contributions.push(border_box.with_extra(child_style.margin.horizontal()));
                } else if is_atomic_inline {
                    let border_box = intrinsic_border_box_widths_with(
                        child_el,
                        &child_style,
                        measurement.child(&child_ancestors, &child_selector),
                    );
                    inline.append_atomic(border_box.with_extra(child_style.margin.horizontal()));
                } else {
                    accumulate_inline_element(
                        child_el,
                        &child_style,
                        measurement.child(&child_ancestors, &child_selector),
                        &mut inline,
                    );
                }
            }
        }
    }
    measurement.add_generated(el, generated.after(), &mut inline, Some(&mut contributions));
    flush_inline(&mut contributions, &mut inline);
    combine_contributions(style, &contributions)
}

impl IntrinsicWidths {
    fn with_extra(self, extra: f32) -> Self {
        Self {
            min_content: self.min_content + extra,
            max_content: self.max_content + extra,
        }
    }
}

fn combine_contributions(
    style: &ComputedStyle,
    contributions: &[IntrinsicWidths],
) -> IntrinsicWidths {
    if matches!(style.display, Display::Flex | Display::InlineFlex) && style.flex_direction.is_row()
    {
        let gap = style.column_gap.max(style.gap) * contributions.len().saturating_sub(1) as f32;
        let max_content = contributions
            .iter()
            .map(|width| width.max_content)
            .sum::<f32>()
            + gap;
        let min_content = if style.flex_wrap.wraps() {
            contributions
                .iter()
                .map(|width| width.min_content)
                .fold(0.0, f32::max)
        } else {
            contributions
                .iter()
                .map(|width| width.min_content)
                .sum::<f32>()
                + gap
        };
        IntrinsicWidths {
            min_content,
            max_content,
        }
    } else if matches!(style.display, Display::Grid | Display::InlineGrid)
        && !style.grid_template_columns.is_empty()
    {
        grid_columns_widths(style, contributions)
    } else {
        IntrinsicWidths {
            min_content: contributions
                .iter()
                .map(|width| width.min_content)
                .fold(0.0, f32::max),
            max_content: contributions
                .iter()
                .map(|width| width.max_content)
                .fold(0.0, f32::max),
        }
    }
}

/// A grid's widths across its explicit columns (css-grid-1 §11.5): a fixed column is its length, and any other column
/// as wide as the widest item in it, with the items placed in order a row at a time.
fn grid_columns_widths(style: &ComputedStyle, items: &[IntrinsicWidths]) -> IntrinsicWidths {
    let columns = &style.grid_template_columns;
    let widest = |column: usize, of: fn(&IntrinsicWidths) -> f32| {
        items
            .iter()
            .skip(column)
            .step_by(columns.len())
            .map(of)
            .fold(0.0, f32::max)
    };
    let sum = |of: fn(&IntrinsicWidths) -> f32| {
        columns
            .iter()
            .enumerate()
            .map(|(column, track)| match *track {
                GridTrack::Fixed(length) => length,
                GridTrack::Minmax(min, max) => widest(column, of).clamp(min, max.max(min)),
                GridTrack::Fr(_) | GridTrack::Auto | GridTrack::Percent(_) => widest(column, of),
            })
            .sum::<f32>()
            + style.column_gap.max(style.gap) * columns.len().saturating_sub(1) as f32
    };
    IntrinsicWidths {
        min_content: sum(|widths| widths.min_content),
        max_content: sum(|widths| widths.max_content),
    }
}

fn element_positions(el: &ElementNode) -> VecDeque<ElementSiblingPosition> {
    let elements = el
        .children
        .iter()
        .filter_map(|child| match child {
            DomNode::Element(element) => Some(element),
            DomNode::Text(_) => None,
        })
        .collect::<Vec<_>>();
    (0..elements.len())
        .map(|index| ElementSiblingPosition::from_element_siblings(&elements, index))
        .collect()
}

fn child_style(
    el: &ElementNode,
    parent: &ComputedStyle,
    rules: &[CssRule],
    selector: &SelectorContext,
) -> ComputedStyle {
    let class_refs = el.class_list();
    compute_style_with_context(
        el.tag,
        el.style_attr(),
        parent,
        rules,
        el.tag_name(),
        &class_refs,
        el.id(),
        &el.attributes,
        selector,
    )
}

fn flush_inline(contributions: &mut Vec<IntrinsicWidths>, inline: &mut InlineWidths) {
    let widths = inline.finished();
    if widths.min_content > 0.0 || widths.max_content > 0.0 {
        contributions.push(widths);
    }
    *inline = InlineWidths::default();
}

struct TextMeasurement<'a> {
    font_size: f32,
    family: FontFamily,
    bold: bool,
    italic: bool,
    /// `letter-spacing` and `word-spacing`, added as layout adds them to a run
    spacing: crate::layout::elements::TextSpacing,
    /// `overflow-wrap: anywhere`, which lets min-content break between any two characters (CSS Text 3 §5.5)
    anywhere: bool,
    fonts: &'a HashMap<String, TtfFont>,
}

impl<'a> TextMeasurement<'a> {
    fn new(style: &ComputedStyle, fonts: &'a HashMap<String, TtfFont>) -> Self {
        Self {
            font_size: style.font_size,
            family: resolve_style_font_family(style, fonts),
            bold: style.font_weight == FontWeight::Bold,
            italic: style.font_style.is_slanted(),
            spacing: crate::layout::elements::TextSpacing::from_style(style),
            anywhere: style.overflow_wrap == crate::style::computed::OverflowWrap::Anywhere,
            fonts,
        }
    }

    fn accumulate(&self, text: &str, inline: &mut InlineWidths) {
        let starts_with_space = text.starts_with(is_collapsible_space);
        let ends_with_space = text.ends_with(is_collapsible_space);
        let collapsed = collapse_whitespace(text);
        if collapsed.is_empty() {
            if starts_with_space && inline.has_content && inline.pending_space == 0.0 {
                inline.pending_space = self.width(" ");
            }
            return;
        }
        if inline.has_content {
            inline.widths.max_content += if inline.pending_space > 0.0 {
                inline.pending_space
            } else if starts_with_space {
                self.width(" ")
            } else {
                0.0
            };
        }
        let widths = &mut inline.widths;
        widths.max_content += self.line_width(&collapsed);
        for word in collapsed.split(' ').filter(|word| !word.is_empty()) {
            let longest = if self.anywhere {
                word.char_indices()
                    .map(|(at, c)| self.width(&word[at..at + c.len_utf8()]))
                    .fold(0.0, f32::max)
            } else {
                self.width(word)
            };
            widths.min_content = widths.min_content.max(longest);
        }
        inline.has_content = true;
        inline.pending_space = if ends_with_space {
            self.width(" ")
        } else {
            0.0
        };
    }

    fn width(&self, text: &str) -> f32 {
        self.tracked(self.raw_width(text), text)
    }

    /// `raw` with the tracking after every typographic unit of `text`, the last one too: Chrome sizes a box to
    /// include the letter spacing after its last letter.
    fn tracked(&self, raw: f32, text: &str) -> f32 {
        let trailing = if text.is_empty() {
            0.0
        } else {
            self.spacing.letter
        };
        self.spacing.add_internal_advance(raw, text) + trailing
    }

    /// The width of collapsed text as its lines measure it: each word on its own and a space's advance between
    /// words. A font that kerns across a space (Liberation Sans before "T" and "Y") makes the text narrower measured
    /// as one string, and a box sized to that width wraps its last word.
    fn line_width(&self, text: &str) -> f32 {
        let space = self.raw_width(" ");
        let raw = text
            .split(' ')
            .enumerate()
            .map(|(index, word)| {
                let word_width = if word.is_empty() {
                    0.0
                } else {
                    self.raw_width(word)
                };
                if index == 0 {
                    word_width
                } else {
                    space + word_width
                }
            })
            .sum::<f32>();
        self.tracked(raw, text)
    }

    fn raw_width(&self, text: &str) -> f32 {
        estimate_word_width(
            text,
            self.font_size,
            &self.family,
            self.bold,
            self.italic,
            self.fonts,
        )
    }
}

fn accumulate_inline_element(
    el: &ElementNode,
    style: &ComputedStyle,
    measurement: IntrinsicMeasurement<'_, '_>,
    inline: &mut InlineWidths,
) {
    let text = TextMeasurement::new(style, measurement.fonts);
    let mut positions = element_positions(el);
    let mut child_ancestors = measurement.ancestors.to_vec();
    child_ancestors.push(measurement.selector.as_ancestor(el));
    let generated = measurement.generated(el, style);
    measurement.add_generated(el, generated.before(), inline, None);

    for child in &el.children {
        match child {
            DomNode::Text(value) => text.accumulate(value, inline),
            DomNode::Element(child_el) => {
                let Some(child_position) = positions.pop_front() else {
                    continue;
                };
                if !collects_as_inline_text(child_el.tag) {
                    continue;
                }
                let child_selector = child_position
                    .as_context()
                    .selector_context(&child_ancestors, child_el.children.is_empty());
                let child_style = child_style(child_el, style, measurement.rules, &child_selector);
                if child_el.tag == crate::parser::dom::HtmlTag::Br {
                    if child_style.display != Display::None {
                        inline.forced_break();
                    }
                    continue;
                }
                accumulate_inline_element(
                    child_el,
                    &child_style,
                    measurement.child(&child_ancestors, &child_selector),
                    inline,
                );
            }
        }
    }
    measurement.add_generated(el, generated.after(), inline, None);
}

/// Resolve a block box's border-box width for an intrinsic `width` keyword.
pub(crate) fn resolve_intrinsic_keyword_width(
    el: &ElementNode,
    style: &ComputedStyle,
    keyword: IntrinsicWidthKeyword,
    available_width: f32,
    env: &LayoutEnv,
) -> f32 {
    // A min/max-width keyword constrains the preferred width; it does not
    // measure that preferred width. Using `intrinsic_border_box_widths` here
    // would feed `width` back into its own constraint (for example making
    // `width: 200px; max-width: min-content` resolve to 200px).
    let widths =
        content_intrinsic_border_box_widths(el, style, env, &[], &SelectorContext::default());
    let stretch = (available_width - style.margin.horizontal()).max(0.0);
    widths.resolve(keyword, stretch).max(0.0)
}
