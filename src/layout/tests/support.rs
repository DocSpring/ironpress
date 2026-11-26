use crate::layout::elements::{GridRow, LayoutVisitor, TableRow, TextBlock};
use crate::layout::engine::{
    FlexRow, Page, TextRun, layout_with_rules, layout_with_rules_and_fonts, visit_layout_tree,
};
use crate::parser::css::parse_stylesheet;
use crate::parser::html::parse_html_with_styles;
use crate::types::{Margin, PageSize};
use std::collections::HashMap;

#[derive(Default)]
struct RunCollector(Vec<TextRun>);

impl LayoutVisitor for RunCollector {
    fn visit_text_block(&mut self, block: &TextBlock) {
        self.0.extend(
            block
                .lines
                .iter()
                .flat_map(|line| line.runs.iter().cloned()),
        );
    }

    fn visit_flex_row(&mut self, row: &FlexRow) {
        self.0.extend(
            row.content
                .cells
                .iter()
                .flat_map(|cell| cell.lines.iter())
                .flat_map(|line| line.runs.iter().cloned()),
        );
    }

    fn visit_grid_row(&mut self, row: &GridRow) {
        self.0.extend(
            row.content
                .cells
                .iter()
                .flat_map(|cell| &cell.layout.content.lines)
                .flat_map(|line| line.runs.iter().cloned()),
        );
    }

    fn visit_table_row(&mut self, row: &TableRow) {
        self.0.extend(
            row.content
                .cells
                .iter()
                .flat_map(|cell| &cell.layout.content.lines)
                .flat_map(|line| line.runs.iter().cloned()),
        );
    }
}

pub(super) fn layout_pages(markup: &str) -> Vec<Page> {
    layout_pages_at(markup, PageSize::new(390.0, 150.0))
}

pub(super) fn layout_pages_at(markup: &str, page_size: PageSize) -> Vec<Page> {
    let document = parse_html_with_styles(markup).expect("valid layout fixture");
    let rules = document
        .stylesheets
        .iter()
        .flat_map(|stylesheet| parse_stylesheet(stylesheet))
        .collect::<Vec<_>>();
    layout_with_rules(&document.nodes, page_size, Margin::uniform(0.0), &rules)
}

pub(super) fn layout_pages_with_fonts(
    markup: &str,
    fonts: &HashMap<String, crate::parser::ttf::TtfFont>,
) -> Vec<Page> {
    layout_pages_at_with_fonts(
        markup,
        PageSize::new(390.0, 150.0),
        Margin::uniform(0.0),
        fonts,
    )
}

pub(super) fn layout_pages_at_with_fonts(
    markup: &str,
    page_size: PageSize,
    margin: Margin,
    fonts: &HashMap<String, crate::parser::ttf::TtfFont>,
) -> Vec<Page> {
    let document = parse_html_with_styles(markup).expect("valid layout fixture");
    let rules = document
        .stylesheets
        .iter()
        .flat_map(|stylesheet| parse_stylesheet(stylesheet))
        .collect::<Vec<_>>();
    layout_with_rules_and_fonts(
        &document.nodes,
        page_size,
        margin,
        &rules,
        fonts,
        None,
        300.0,
        crate::layout::paginate::FootnoteAreaLayout::default(),
    )
}

pub(super) fn collected_runs(markup: &str) -> Vec<TextRun> {
    let mut collector = RunCollector::default();
    for page in layout_pages(markup) {
        for (_, element) in page.elements {
            visit_layout_tree(element.as_ref(), &mut collector);
        }
    }
    collector.0
}

pub(super) fn visible_runs(markup: &str) -> Vec<TextRun> {
    collected_runs(markup)
        .into_iter()
        .filter(|run| !run.text.trim().is_empty())
        .collect()
}

/// The visible text runs inside the atomic inline boxes of `markup`'s outermost lines: a box's own lines, or the text
/// of the nodes it was laid out as.
pub(super) fn visible_inline_box_runs(markup: &str) -> Vec<TextRun> {
    // A text block's children are the nodes its inline boxes were laid out as, which their own box counts
    fn walk(element: &dyn crate::layout::elements::LayoutElement, boxes: &mut InlineBoxCollector) {
        element.accept(boxes);
        if crate::layout::elements::LayoutElementTestExt::inspect_text(element, |_| ()).is_none() {
            element.visit_children(&mut |child| walk(child, boxes));
        }
    }
    let mut boxes = InlineBoxCollector::default();
    for page in layout_pages(markup) {
        for (_, element) in page.elements {
            walk(element.as_ref(), &mut boxes);
        }
    }
    boxes
        .0
        .into_iter()
        .flat_map(|inline| {
            let mut collector = RunCollector::default();
            for node in &inline.content {
                visit_layout_tree(node.as_ref(), &mut collector);
            }
            inline
                .lines
                .into_iter()
                .flat_map(|line| line.runs)
                .chain(collector.0)
        })
        .filter(|run| !run.text.trim().is_empty())
        .collect()
}

/// The inline boxes in the lines it visits.
#[derive(Default)]
struct InlineBoxCollector(Vec<crate::layout::engine::InlineBox>);

impl LayoutVisitor for InlineBoxCollector {
    fn visit_text_block(&mut self, block: &TextBlock) {
        self.0.extend(
            block
                .lines
                .iter()
                .flat_map(|line| &line.runs)
                .filter_map(|run| run.inline_box.as_deref().cloned()),
        );
    }
}

/// The atomic inline boxes in `markup`'s lines, in order.
pub(super) fn inline_boxes(markup: &str) -> Vec<crate::layout::engine::InlineBox> {
    collected_runs(markup)
        .into_iter()
        .filter_map(|run| run.inline_box.map(|inline| *inline))
        .collect()
}

/// The visible words on each page, in layout tree order
pub(super) fn words_per_page(pages: &[Page]) -> Vec<Vec<String>> {
    pages
        .iter()
        .map(|page| {
            let mut collector = RunCollector::default();
            for (_, element) in &page.elements {
                visit_layout_tree(element.as_ref(), &mut collector);
            }
            collector
                .0
                .iter()
                .flat_map(|run| {
                    run.text
                        .split_whitespace()
                        .map(str::to_owned)
                        .collect::<Vec<_>>()
                })
                .collect()
        })
        .collect()
}
