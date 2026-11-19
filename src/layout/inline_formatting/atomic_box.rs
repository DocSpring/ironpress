//! Atomic inline boxes laid out as boxes.
//!
//! An inline-block, inline-flex, inline-grid or inline-table in a line is laid out as a block lays out its box, by
//! [`flatten_element`]: at its own width or, without one, its fit-content width (CSS 2.1 §10.3.9), and without its
//! margins, which the line places it by. The nodes go in the line's [`InlineBox`], whose painter draws them where the
//! box goes, so the box has everything a block's box has: its children, backgrounds, borders, shadows, filters and
//! blending. Its baseline is its last line box's for an inline-block and its first line box's for the others, or its
//! bottom margin edge without one (§10.8.1, css-align-3 §9).

use std::collections::HashMap;

use crate::layout::context::{LayoutContext, LayoutEnv};
use crate::layout::elements::{
    Container, IntoLayoutNode, LayoutElement, LayoutNode, LayoutVisitor, PositioningOwner,
    TextBlock,
};
use crate::layout::engine::{InlineBox, TextLine, flatten_element, visit_layout_tree};
use crate::layout::grid::GridItemHeightMemo;
use crate::layout::paginate::{BlockFlowAccumulator, simulate_block_flow};
use crate::layout::table::TableCellSizingContext;
use crate::layout::traversal::{ElementSiblingContext, LayoutTreeContext, MarginApplication};
use crate::parser::css::{CssRule, SelectorContext};
use crate::parser::dom::{ElementNode, HtmlTag};
use crate::parser::ttf::TtfFont;
use crate::style::computed::{ComputedStyle, Display, IntrinsicWidthKeyword, Position};

/// What laying out an atomic inline box needs besides what the run collector holds.
pub(crate) struct AtomicInlineLayout<'a> {
    layout: LayoutContext,
    filter_defs: &'a HashMap<String, ElementNode>,
    filter_dpi: f32,
    table_cell_sizing: TableCellSizingContext<'a>,
    grid_item_heights: &'a mut GridItemHeightMemo,
    print_fit_widening: Option<crate::layout::print_scale::PrintFitWidening>,
}

/// The parts of the layout environment the run collector holds, lent back to lay out a box.
pub(crate) struct CollectorEnv<'a, 'r> {
    pub(crate) rules: &'a [CssRule],
    pub(crate) fonts: &'a HashMap<String, TtfFont>,
    pub(crate) counter_state: &'r mut crate::layout::engine::CounterState,
    pub(crate) resources: &'r mut crate::security::resources::ResourceLoader,
}

impl std::fmt::Debug for AtomicInlineLayout<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AtomicInlineLayout")
            .field("layout", &self.layout)
            .finish_non_exhaustive()
    }
}

impl<'a> AtomicInlineLayout<'a> {
    /// The parts of `env` other than the ones the run collector holds, with `layout` the context of the block whose
    /// lines the boxes go in.
    pub(crate) fn new(
        layout: LayoutContext,
        filter_defs: &'a HashMap<String, ElementNode>,
        filter_dpi: f32,
        table_cell_sizing: TableCellSizingContext<'a>,
        grid_item_heights: &'a mut GridItemHeightMemo,
        print_fit_widening: Option<crate::layout::print_scale::PrintFitWidening>,
    ) -> Self {
        Self {
            layout,
            filter_defs,
            filter_dpi,
            table_cell_sizing,
            grid_item_heights,
            print_fit_widening,
        }
    }

    /// `element`, an atomic inline box with `style` at `selector_context` in an element with `parent_style`, laid out
    /// as its box.
    pub(crate) fn lay_out(
        &mut self,
        element: &ElementNode,
        style: &ComputedStyle,
        selector_context: &SelectorContext<'_>,
        parent_style: &ComputedStyle,
        collector: CollectorEnv<'_, '_>,
    ) -> InlineBox {
        if matches!(element.tag, HtmlTag::Progress | HtmlTag::Meter) {
            return Self::lay_out_gauge(element, style);
        }
        let mut env = LayoutEnv {
            rules: collector.rules,
            fonts: collector.fonts,
            counter_state: collector.counter_state,
            resources: collector.resources,
            filter_defs: self.filter_defs,
            filter_dpi: self.filter_dpi,
            table_cell_sizing: self.table_cell_sizing.reborrow(),
            grid_item_heights: &mut *self.grid_item_heights,
            floats: Default::default(),
            print_fit_widening: self.print_fit_widening,
        };
        let available_width = self.layout.available_width().max(0.0);
        // Without a width of its own, the box shrinks to fit (CSS 2.1 §10.3.9), and a `min-content`, `max-content` or
        // `fit-content` width is measured the same way; a table shrinks to fit already (§17.5.2). A text field, select
        // or text area has a size of its own, as a replaced element does, and draws its value or selected option, not
        // its children
        let is_form_control = matches!(
            element.tag,
            HtmlTag::Input | HtmlTag::Select | HtmlTag::Textarea
        );
        let intrinsic_keyword =
            (style.width.is_none() && style.percentage_sizing.width.is_none() && !is_form_control)
                .then(|| {
                    style
                        .width_keyword
                        .unwrap_or(IntrinsicWidthKeyword::FitContent)
                });
        let used_width = intrinsic_keyword
            .filter(|_| style.display != Display::InlineTable)
            .map(|keyword| {
                crate::layout::intrinsic_width::resolve_intrinsic_keyword_width(
                    element,
                    style,
                    keyword,
                    available_width,
                    &env,
                )
            });
        let siblings = ElementSiblingContext::new(
            selector_context.child_index,
            selector_context.sibling_count,
        )
        .with_neighbors(
            &selector_context.preceding_siblings,
            &selector_context.following_siblings,
        );
        let context =
            LayoutTreeContext::new(parent_style, &self.layout, &selector_context.ancestors)
                .for_element(siblings)
                .with_margin_application(MarginApplication::ByFormattingItem);
        let context = match used_width {
            Some(width) => context.with_used_width(width),
            None => context,
        };
        let mut content = Vec::new();
        flatten_element(element, context, &mut content, &mut env);

        let width = used_width.unwrap_or_else(|| border_box_width(&content));
        let height = simulate_block_flow(&content).height;
        let baseline_ascent = if style.overflow.clips() {
            None
        } else {
            let which = if style.display == Display::InlineBlock {
                LineBox::Last
            } else {
                LineBox::First
            };
            flow_baseline(&content, which, env.fonts)
        };
        InlineBox {
            width,
            height,
            margin_left: style.margin.left.max(0.0),
            margin_right: style.margin.right.max(0.0),
            margin_top: style.margin.top.max(0.0),
            margin_bottom: style.margin.bottom.max(0.0),
            vertical_align: style.vertical_align,
            baseline_ascent,
            content,
            ..InlineBox::default()
        }
    }
}

impl AtomicInlineLayout<'_> {
    /// `element`, an `<img>` or `<svg>` with `style`, laid out as a replaced box in the block's lines: sized against
    /// the block's width, as a replaced block is, with the replaced node painted where the box goes.
    /// A `<progress>` or `<meter>`, a replaced element drawn at its own size.
    fn lay_out_gauge(element: &ElementNode, style: &ComputedStyle) -> InlineBox {
        let mut gauge =
            crate::layout::elements::ProgressBar::for_element(element, style, f32::INFINITY);
        // The line places the box by its margins
        gauge.margins = crate::layout::flow_metrics::BlockMargins::ZERO;
        InlineBox {
            width: gauge.size.width,
            height: gauge.size.height,
            margin_left: style.margin.left.max(0.0),
            margin_right: style.margin.right.max(0.0),
            margin_top: style.margin.top.max(0.0),
            margin_bottom: style.margin.bottom.max(0.0),
            vertical_align: style.vertical_align,
            content: vec![gauge.boxed()],
            ..InlineBox::default()
        }
    }

    pub(crate) fn lay_out_replaced(
        &self,
        element: &ElementNode,
        style: &ComputedStyle,
        resources: &mut crate::security::resources::ResourceLoader,
    ) -> Option<InlineBox> {
        let available_width = self.layout.available_width().max(0.0);
        let available_height = self.layout.available_height();
        let mut node = if element.tag == crate::parser::dom::HtmlTag::Svg {
            crate::layout::images::svg_element_node(
                element,
                style,
                available_width,
                available_height,
            )?
        } else {
            crate::layout::images::load_image_from_element(
                resources,
                element,
                available_width,
                available_height,
                style,
                self.filter_dpi,
            )?
        };
        let mut filter_style = style.clone();
        let filter =
            crate::layout::filter::ResolvedFilter::from_style(&mut filter_style, self.filter_defs);
        if filter.requires_source_surface() {
            crate::layout::filter::retain_for_fragmentation(node.as_mut(), &filter);
        }
        let replaced = node.replaced_element_mut()?;
        // The line places the box by its margins
        replaced.geometry_mut().flow.margins = crate::layout::flow_metrics::BlockMargins::ZERO;
        let size = replaced.geometry().size;
        Some(InlineBox {
            width: size.width,
            height: size.height,
            margin_left: style.margin.left.max(0.0),
            margin_right: style.margin.right.max(0.0),
            margin_top: style.margin.top.max(0.0),
            margin_bottom: style.margin.bottom.max(0.0),
            vertical_align: style.vertical_align,
            content: vec![node],
            ..InlineBox::default()
        })
    }
}

/// The widest box among `nodes`: a box's own width, or an auto-width table's. A relative offset moves where a box is
/// painted, not how wide it is (CSS 2.1 §9.4.3).
fn border_box_width(nodes: &[LayoutNode]) -> f32 {
    nodes
        .iter()
        .filter_map(|node| {
            let extent = node.inline_flow_extent()?;
            // Its used width, which a grid's or a flex container's outlasts its tracks' or items'
            let right = extent
                .normal_flow_right_edge()
                .or_else(|| extent.max_content_outer_extent())?;
            let shift = node
                .positioning_owner()
                .map(PositioningOwner::positioning)
                .filter(|positioning| positioning.scheme == Position::Relative)
                .map_or(0.0, |positioning| positioning.insets.left);
            Some(right - shift)
        })
        .fold(0.0, f32::max)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LineBox {
    First,
    Last,
}

/// The distance from the top of a block flow of `nodes` to the baseline of its first or last line box.
fn flow_baseline(
    nodes: &[LayoutNode],
    which: LineBox,
    fonts: &HashMap<String, TtfFont>,
) -> Option<f32> {
    let mut flow = BlockFlowAccumulator::default();
    let mut found = None;
    for (index, node) in nodes.iter().enumerate() {
        let Some(top) = flow.place(index, node.as_ref()) else {
            continue;
        };
        if let Some(baseline) = node_baseline(node.as_ref(), which, fonts) {
            found = Some(top + baseline);
            if which == LineBox::First {
                break;
            }
        }
    }
    found
}

/// The distance from `node`'s border-box top to the baseline of its first or last line box.
fn node_baseline(
    node: &dyn LayoutElement,
    which: LineBox,
    fonts: &HashMap<String, TtfFont>,
) -> Option<f32> {
    struct Baseline<'a> {
        which: LineBox,
        fonts: &'a HashMap<String, TtfFont>,
        found: Option<f32>,
    }

    impl LayoutVisitor for Baseline<'_> {
        fn visit_text_block(&mut self, element: &TextBlock) {
            let start = element.box_model.border.top.width + element.box_model.padding.top;
            self.found = lines_baseline(&element.lines, self.which, self.fonts)
                .map(|baseline| start + baseline);
        }

        fn visit_container(&mut self, element: &Container) {
            let start = element.box_model.border.top.width + element.box_model.padding.top;
            self.found = flow_baseline(&element.children, self.which, self.fonts)
                .map(|baseline| start + baseline);
        }
    }

    let mut visitor = Baseline {
        which,
        fonts,
        found: None,
    };
    node.accept(&mut visitor);
    visitor.found.or_else(|| first_line_baseline(node, fonts))
}

/// The baseline of the first or last of `lines`, from the top of the first.
fn lines_baseline(
    lines: &[TextLine],
    which: LineBox,
    fonts: &HashMap<String, TtfFont>,
) -> Option<f32> {
    let index = match which {
        LineBox::First => 0,
        LineBox::Last => lines.len().checked_sub(1)?,
    };
    let line = lines.get(index)?;
    let above: f32 = lines[..index].iter().map(|line| line.height).sum();
    Some(
        above
            + line.baseline_ascent.unwrap_or_else(|| {
                crate::layout::filter::surface::line_baseline_ascent(line, fonts)
            }),
    )
}

/// For a box that isn't a block container (a flex, grid or table row): the baseline of its first row, from its top,
/// the lowest first baseline among the items in that row.
fn first_line_baseline(node: &dyn LayoutElement, fonts: &HashMap<String, TtfFont>) -> Option<f32> {
    struct FirstRow<'a> {
        fonts: &'a HashMap<String, TtfFont>,
        found: Option<f32>,
    }

    impl FirstRow<'_> {
        /// The first baseline of a cell's contents, from the top of its content box
        fn contents(&self, lines: &[TextLine], children: &[LayoutNode]) -> Option<f32> {
            match lines
                .iter()
                .position(|line| line.runs.iter().any(|run| !run.text.is_empty()))
            {
                Some(index) => lines_baseline(&lines[..=index], LineBox::Last, self.fonts),
                None => flow_baseline(children, LineBox::First, self.fonts),
            }
        }

        fn cell_box(&self, cell: &crate::layout::cells::CellBox) -> Option<f32> {
            self.contents(&cell.content.lines, &cell.content.children)
                .map(|baseline| cell.box_model.content_insets.top + baseline)
        }
    }

    impl LayoutVisitor for FirstRow<'_> {
        fn visit_flex_row(&mut self, element: &crate::layout::elements::FlexRow) {
            if self.found.is_some() {
                return;
            }
            let start = element.box_model.border.top.width + element.box_model.padding.top;
            self.found = element
                .content
                .cells
                .iter()
                .filter_map(|cell| {
                    self.contents(&cell.lines, &cell.nested_elements)
                        .map(|baseline| {
                            cell.y_offset + cell.border.top.width + cell.padding.top + baseline
                        })
                })
                .reduce(f32::max)
                .map(|baseline| start + baseline);
        }

        fn visit_grid_row(&mut self, element: &crate::layout::elements::GridRow) {
            if self.found.is_some() {
                return;
            }
            let start = element.box_model.border.top.width + element.box_model.padding.top;
            self.found = element
                .content
                .cells
                .iter()
                .filter_map(|cell| self.cell_box(&cell.layout))
                .reduce(f32::max)
                .map(|baseline| start + baseline);
        }

        fn visit_table_row(&mut self, element: &crate::layout::elements::TableRow) {
            if self.found.is_some() {
                return;
            }
            // The baseline-aligned cells' baseline, or without one the bottom content edge of the lowest cell
            // (CSS 2.1 §17.5.3)
            self.found = element
                .content
                .cells
                .iter()
                .filter(|cell| {
                    cell.layout.alignment.block == crate::style::computed::VerticalAlign::Baseline
                })
                .filter_map(|cell| self.cell_box(&cell.layout))
                .reduce(f32::max)
                .or_else(|| {
                    element
                        .content
                        .cells
                        .iter()
                        .map(|cell| {
                            cell.layout.intrinsic_block_extent()
                                - cell.layout.box_model.content_insets.bottom
                        })
                        .reduce(f32::max)
                })
                // The table's border and spacing above its first row
                .map(|baseline| element.flow.internal.start + baseline);
        }
    }

    let mut visitor = FirstRow { fonts, found: None };
    visit_layout_tree(node, &mut visitor);
    visitor.found
}
