//! `counter(page)` and `counter(pages)` inside a running element.
//!
//! A running element (css-gcpm-3 §2.4, `position: running()`) is laid out once,
//! where it occurs in the document, but it is displayed in a page-margin box on
//! every page that refers to it, and the page counters there take that page's
//! values (css-page-3 §4.3). Layout therefore leaves a placeholder for each such
//! counter, and the renderer resolves it on each page that displays the element.
//!
//! A placeholder is measured as the widest value it can take, so the text
//! painted on any page fits the space laid out for it. The page count isn't
//! known until the document is paginated: the first layout pass measures a
//! one-digit number, and when the document needs more room for its page
//! numbers, layout runs again with the page count ([`page_count_to_measure`]).
//! Running elements are out of flow, so the second pass paginates the same way.

use std::borrow::Cow;

use crate::layout::elements::{
    FlexRow, GridRow, LayoutElement, LayoutNode, LayoutVisitor, LayoutVisitorMut, TableRow,
    TextBlock, visit_layout_tree, visit_layout_tree_mut,
};
use crate::layout::engine::{TextLine, TextRun};
use crate::layout::list_markers::format_counter_value;
use crate::style::computed::{ListStyleType, TARGET_PLACEHOLDER_END, TARGET_PLACEHOLDER_START};

const PAYLOAD_PREFIX: &str = "page-counter|";

/// The placeholder for `counter(<name>, <style>)` when `name` is a page
/// counter, or `None` for any other counter. `page_count` is the document's
/// page count when layout knows it, which the placeholder is measured with.
pub(crate) fn page_counter_placeholder(
    name: &str,
    style: &ListStyleType,
    page_count: Option<usize>,
) -> Option<String> {
    let counter = if name.eq_ignore_ascii_case("page") {
        "page"
    } else if name.eq_ignore_ascii_case("pages") {
        "pages"
    } else {
        return None;
    };
    let keyword = style_keyword(style);
    Some(format!(
        "{TARGET_PLACEHOLDER_START}{PAYLOAD_PREFIX}{counter}|{keyword}|{}{TARGET_PLACEHOLDER_END}",
        measured_value(counter, keyword, page_count)
    ))
}

/// Whether a placeholder payload is a page counter's.
pub(crate) fn is_page_counter_payload(payload: &str) -> bool {
    payload.starts_with(PAYLOAD_PREFIX)
}

/// The text a page counter placeholder is measured as, or `None` for another
/// placeholder.
pub(crate) fn placeholder_measure_text(payload: &str) -> Option<&str> {
    let mut parts = payload.strip_prefix(PAYLOAD_PREFIX)?.split('|');
    Some(parts.nth(2).unwrap_or("0"))
}

/// The widest text a page counter can show: the page count for `pages`, and
/// for `page` the value with the most characters up to the page count. Without
/// a page count, a one-digit number.
fn measured_value(counter: &str, keyword: &str, page_count: Option<usize>) -> String {
    let Some(page_count) = page_count.filter(|count| *count > 0) else {
        return "0".to_string();
    };
    let style = keyword_style(keyword);
    let format =
        |value: usize| format_counter_value(&style, i32::try_from(value).unwrap_or(i32::MAX));
    if counter == "pages" {
        return format(page_count);
    }
    (1..=page_count)
        .map(format)
        .reduce(|widest, value| {
            if value.chars().count() > widest.chars().count() {
                value
            } else {
                widest
            }
        })
        .unwrap_or_else(|| "0".to_string())
}

/// The page count to lay the document out again with, when a page counter
/// placeholder in a running element was measured with fewer characters than
/// the widest value it shows across `pages`.
pub(crate) fn page_count_to_measure(pages: &[crate::layout::engine::Page]) -> Option<usize> {
    let page_count = pages.len();
    let mut payloads = PayloadCollector::default();
    for page in pages {
        for element in page.generated_content.running_elements() {
            visit_layout_tree(element.as_ref(), &mut payloads);
        }
    }
    payloads
        .payloads
        .iter()
        .any(|payload| {
            let mut parts = payload.trim_start_matches(PAYLOAD_PREFIX).split('|');
            let counter = parts.next().unwrap_or_default();
            let keyword = parts.next().unwrap_or("decimal");
            let measured = parts.next().unwrap_or("0");
            measured_value(counter, keyword, Some(page_count))
                .chars()
                .count()
                > measured.chars().count()
        })
        .then_some(page_count)
}

/// The predefined counter style a placeholder names. Other styles format as
/// decimal numbers.
fn style_keyword(style: &ListStyleType) -> &'static str {
    match style {
        ListStyleType::DecimalLeadingZero => "decimal-leading-zero",
        ListStyleType::LowerAlpha => "lower-alpha",
        ListStyleType::UpperAlpha => "upper-alpha",
        ListStyleType::LowerRoman => "lower-roman",
        ListStyleType::UpperRoman => "upper-roman",
        ListStyleType::CjkDecimal => "cjk-decimal",
        _ => "decimal",
    }
}

fn keyword_style(keyword: &str) -> ListStyleType {
    match keyword {
        "decimal-leading-zero" => ListStyleType::DecimalLeadingZero,
        "lower-alpha" => ListStyleType::LowerAlpha,
        "upper-alpha" => ListStyleType::UpperAlpha,
        "lower-roman" => ListStyleType::LowerRoman,
        "upper-roman" => ListStyleType::UpperRoman,
        "cjk-decimal" => ListStyleType::CjkDecimal,
        _ => ListStyleType::Decimal,
    }
}

/// The page counters of the page that displays a running element.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct PageCounterValues {
    /// The `page` counter's value on this page.
    pub(crate) page: usize,
    /// The number of pages in the document.
    pub(crate) pages: usize,
}

impl PageCounterValues {
    /// A copy of `element` with its page counters resolved, or `None` when it
    /// has none.
    pub(crate) fn resolve(self, element: &dyn LayoutElement) -> Option<LayoutNode> {
        let mut search = PlaceholderSearch::default();
        visit_layout_tree(element, &mut search);
        if !search.found {
            return None;
        }
        let mut resolved = element.clone_box();
        visit_layout_tree_mut(resolved.as_mut(), &mut PlaceholderResolver(self));
        Some(resolved)
    }

    /// The runs of `element` that hold page counters, resolved: the text the
    /// renderer paints for them, which font subsetting must cover.
    pub(crate) fn resolved_runs(self, element: &dyn LayoutElement) -> Vec<TextRun> {
        let mut collector = ResolvedRuns {
            values: self,
            runs: Vec::new(),
        };
        visit_layout_tree(element, &mut collector);
        collector.runs
    }

    fn resolve_text(self, text: &str) -> Cow<'_, str> {
        if !text.contains(TARGET_PLACEHOLDER_START) {
            return Cow::Borrowed(text);
        }
        let mut out = String::new();
        let mut rest = text;
        while let Some(start) = rest.find(TARGET_PLACEHOLDER_START) {
            out.push_str(&rest[..start]);
            let payload_start = start + TARGET_PLACEHOLDER_START.len();
            let Some(end) = rest[payload_start..].find(TARGET_PLACEHOLDER_END) else {
                break;
            };
            let payload = &rest[payload_start..payload_start + end];
            let marker_end = payload_start + end + TARGET_PLACEHOLDER_END.len();
            match self.payload_value(payload) {
                Some(value) => out.push_str(&value),
                None => out.push_str(&rest[start..marker_end]),
            }
            rest = &rest[marker_end..];
        }
        out.push_str(rest);
        Cow::Owned(out)
    }

    fn payload_value(self, payload: &str) -> Option<String> {
        let mut parts = payload.strip_prefix(PAYLOAD_PREFIX)?.split('|');
        let value = match parts.next()? {
            "page" => self.page,
            "pages" => self.pages,
            _ => return None,
        };
        let style = keyword_style(parts.next().unwrap_or("decimal"));
        Some(format_counter_value(
            &style,
            i32::try_from(value).unwrap_or(i32::MAX),
        ))
    }
}

/// Every line box a running element paints text from.
trait LineBoxes {
    fn line_boxes(&self) -> Vec<&TextLine>;
    fn line_boxes_mut(&mut self) -> Vec<&mut TextLine>;
}

impl LineBoxes for TextBlock {
    fn line_boxes(&self) -> Vec<&TextLine> {
        self.lines.iter().collect()
    }

    fn line_boxes_mut(&mut self) -> Vec<&mut TextLine> {
        self.lines.iter_mut().collect()
    }
}

impl LineBoxes for FlexRow {
    fn line_boxes(&self) -> Vec<&TextLine> {
        self.content
            .cells
            .iter()
            .flat_map(|cell| &cell.lines)
            .collect()
    }

    fn line_boxes_mut(&mut self) -> Vec<&mut TextLine> {
        self.content
            .cells
            .iter_mut()
            .flat_map(|cell| &mut cell.lines)
            .collect()
    }
}

impl LineBoxes for GridRow {
    fn line_boxes(&self) -> Vec<&TextLine> {
        self.content
            .cells
            .iter()
            .flat_map(|cell| &cell.layout.content.lines)
            .collect()
    }

    fn line_boxes_mut(&mut self) -> Vec<&mut TextLine> {
        self.content
            .cells
            .iter_mut()
            .flat_map(|cell| &mut cell.layout.content.lines)
            .collect()
    }
}

impl LineBoxes for TableRow {
    fn line_boxes(&self) -> Vec<&TextLine> {
        self.content
            .cells
            .iter()
            .flat_map(|cell| &cell.layout.content.lines)
            .collect()
    }

    fn line_boxes_mut(&mut self) -> Vec<&mut TextLine> {
        self.content
            .cells
            .iter_mut()
            .flat_map(|cell| &mut cell.layout.content.lines)
            .collect()
    }
}

/// The text runs of `lines`, including those inside atomic inline boxes.
fn runs_mut<'a>(lines: Vec<&'a mut TextLine>, out: &mut Vec<&'a mut TextRun>) {
    for line in lines {
        for run in &mut line.runs {
            let run: &'a mut TextRun = run;
            if run.inline_box.is_some() {
                let inline = run.inline_box.as_deref_mut();
                if let Some(inline) = inline {
                    runs_mut(inline.lines.iter_mut().collect(), out);
                }
            } else {
                out.push(run);
            }
        }
    }
}

fn has_placeholder(lines: Vec<&TextLine>) -> bool {
    lines.into_iter().flat_map(|line| &line.runs).any(|run| {
        run.text.contains(TARGET_PLACEHOLDER_START)
            || run
                .inline_box
                .as_ref()
                .is_some_and(|inline| has_placeholder(inline.lines.iter().collect()))
    })
}

/// The payloads of the page counter placeholders in line boxes.
#[derive(Default)]
struct PayloadCollector {
    payloads: Vec<String>,
}

impl PayloadCollector {
    fn collect(&mut self, boxes: &dyn LineBoxes) {
        collect_payloads(boxes.line_boxes(), &mut self.payloads);
    }
}

fn collect_payloads(lines: Vec<&TextLine>, out: &mut Vec<String>) {
    for run in lines.into_iter().flat_map(|line| &line.runs) {
        if let Some(inline) = &run.inline_box {
            collect_payloads(inline.lines.iter().collect(), out);
            continue;
        }
        let mut rest = run.text.as_str();
        while let Some(start) = rest.find(TARGET_PLACEHOLDER_START) {
            let payload_start = start + TARGET_PLACEHOLDER_START.len();
            let Some(end) = rest[payload_start..].find(TARGET_PLACEHOLDER_END) else {
                break;
            };
            let payload = &rest[payload_start..payload_start + end];
            if is_page_counter_payload(payload) {
                out.push(payload.to_string());
            }
            rest = &rest[payload_start + end + TARGET_PLACEHOLDER_END.len()..];
        }
    }
}

impl LayoutVisitor for PayloadCollector {
    fn visit_text_block(&mut self, element: &TextBlock) {
        self.collect(element);
    }

    fn visit_flex_row(&mut self, element: &FlexRow) {
        self.collect(element);
    }

    fn visit_grid_row(&mut self, element: &GridRow) {
        self.collect(element);
    }

    fn visit_table_row(&mut self, element: &TableRow) {
        self.collect(element);
    }
}

#[derive(Default)]
struct PlaceholderSearch {
    found: bool,
}

impl PlaceholderSearch {
    fn search(&mut self, boxes: &dyn LineBoxes) {
        self.found |= has_placeholder(boxes.line_boxes());
    }
}

impl LayoutVisitor for PlaceholderSearch {
    fn visit_text_block(&mut self, element: &TextBlock) {
        self.search(element);
    }

    fn visit_flex_row(&mut self, element: &FlexRow) {
        self.search(element);
    }

    fn visit_grid_row(&mut self, element: &GridRow) {
        self.search(element);
    }

    fn visit_table_row(&mut self, element: &TableRow) {
        self.search(element);
    }
}

struct ResolvedRuns {
    values: PageCounterValues,
    runs: Vec<TextRun>,
}

impl ResolvedRuns {
    fn collect(&mut self, boxes: &dyn LineBoxes) {
        collect_resolved(boxes.line_boxes(), self.values, &mut self.runs);
    }
}

fn collect_resolved(lines: Vec<&TextLine>, values: PageCounterValues, out: &mut Vec<TextRun>) {
    for run in lines.into_iter().flat_map(|line| &line.runs) {
        if let Some(inline) = &run.inline_box {
            collect_resolved(inline.lines.iter().collect(), values, out);
        } else if let Cow::Owned(text) = values.resolve_text(&run.text) {
            out.push(TextRun {
                text,
                ..run.clone()
            });
        }
    }
}

impl LayoutVisitor for ResolvedRuns {
    fn visit_text_block(&mut self, element: &TextBlock) {
        self.collect(element);
    }

    fn visit_flex_row(&mut self, element: &FlexRow) {
        self.collect(element);
    }

    fn visit_grid_row(&mut self, element: &GridRow) {
        self.collect(element);
    }

    fn visit_table_row(&mut self, element: &TableRow) {
        self.collect(element);
    }
}

struct PlaceholderResolver(PageCounterValues);

impl PlaceholderResolver {
    fn resolve(&self, boxes: &mut dyn LineBoxes) {
        let mut runs = Vec::new();
        runs_mut(boxes.line_boxes_mut(), &mut runs);
        for run in runs {
            if let Cow::Owned(text) = self.0.resolve_text(&run.text) {
                run.text = text;
            }
        }
    }
}

impl LayoutVisitorMut for PlaceholderResolver {
    fn visit_text_block(&mut self, element: &mut TextBlock) {
        self.resolve(element);
    }

    fn visit_flex_row(&mut self, element: &mut FlexRow) {
        self.resolve(element);
    }

    fn visit_grid_row(&mut self, element: &mut GridRow) {
        self.resolve(element);
    }

    fn visit_table_row(&mut self, element: &mut TableRow) {
        self.resolve(element);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const VALUES: PageCounterValues = PageCounterValues { page: 4, pages: 12 };

    #[test]
    fn only_page_counters_get_placeholders() {
        assert!(page_counter_placeholder("page", &ListStyleType::Decimal, None).is_some());
        assert!(page_counter_placeholder("PAGES", &ListStyleType::Decimal, None).is_some());
        assert!(page_counter_placeholder("chapter", &ListStyleType::Decimal, None).is_none());
    }

    #[test]
    fn placeholders_resolve_to_the_page_values_in_their_counter_style() {
        let text = format!(
            "Page {} of {} ({})",
            page_counter_placeholder("page", &ListStyleType::Decimal, None).unwrap_or_default(),
            page_counter_placeholder("pages", &ListStyleType::Decimal, Some(12))
                .unwrap_or_default(),
            page_counter_placeholder("page", &ListStyleType::UpperRoman, Some(12))
                .unwrap_or_default(),
        );
        assert_eq!(VALUES.resolve_text(&text), "Page 4 of 12 (IV)");
    }

    #[test]
    fn placeholders_are_measured_as_their_widest_value() {
        let measured = |name: &str, style: &ListStyleType, page_count| {
            let placeholder = page_counter_placeholder(name, style, page_count).unwrap_or_default();
            let payload = placeholder
                .trim_start_matches(TARGET_PLACEHOLDER_START)
                .trim_end_matches(TARGET_PLACEHOLDER_END)
                .to_string();
            placeholder_measure_text(&payload)
                .unwrap_or_default()
                .to_string()
        };

        assert_eq!(measured("page", &ListStyleType::Decimal, None), "0");
        assert_eq!(measured("page", &ListStyleType::Decimal, Some(12)), "10");
        assert_eq!(measured("pages", &ListStyleType::Decimal, Some(12)), "12");
        assert_eq!(
            measured("page", &ListStyleType::UpperRoman, Some(12)),
            "VIII"
        );
        assert_eq!(
            measured("pages", &ListStyleType::UpperRoman, Some(12)),
            "XII"
        );
        assert_eq!(placeholder_measure_text("text|#x"), None);
    }

    #[test]
    fn only_wider_page_numbers_need_another_layout_pass() {
        let needs_pass = |page_count: usize, style: &ListStyleType| {
            let placeholder = page_counter_placeholder("pages", style, None).unwrap_or_default();
            let lines = vec![TextLine {
                runs: vec![TextRun {
                    text: placeholder,
                    ..Default::default()
                }],
                ..Default::default()
            }];
            let mut pages = (0..page_count)
                .map(|_| crate::layout::engine::Page::default())
                .collect::<Vec<_>>();
            pages[0].generated_content.capture_running(
                "header".into(),
                crate::layout::elements::IntoLayoutNode::boxed(TextBlock::plain(lines)),
                true,
            );
            page_count_to_measure(&pages)
        };

        assert_eq!(needs_pass(9, &ListStyleType::Decimal), None);
        assert_eq!(needs_pass(10, &ListStyleType::Decimal), Some(10));
        assert_eq!(needs_pass(1, &ListStyleType::UpperRoman), None);
        assert_eq!(needs_pass(2, &ListStyleType::UpperRoman), Some(2));
    }

    #[test]
    fn other_placeholders_are_left_for_their_own_resolver() {
        let text = format!("{TARGET_PLACEHOLDER_START}text|#x{TARGET_PLACEHOLDER_END}");
        assert_eq!(VALUES.resolve_text(&text), text);
    }
}
