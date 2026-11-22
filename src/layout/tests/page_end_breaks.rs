//! A box that crosses the end of a page breaks inside it, between lines or children, at the last break that fits
//! and that `break-inside`, `orphans` and `widows` allow (CSS Fragmentation 3 §4.4), and its first fragment reaches
//! the page end. It moves to the next page whole only when no such break fits. An image inside it that fits a page
//! moves to the next page whole. Each expectation is what Chrome prints for the same markup.

use crate::layout::elements::{LayoutVisitor, TextBlock};
use crate::layout::engine::{Page, visit_layout_tree};
use crate::types::PageSize;

use super::support::layout_pages_at;

/// Pages 240pt by 400pt with no margins: 22 lines of 18pt fit on each.
fn pages(markup: &str) -> Vec<Page> {
    layout_pages_at(
        &format!(
            "<style>body {{ margin: 0; font: 12pt/18pt sans-serif }} p {{ margin: 0 }}</style>\
             {markup}"
        ),
        PageSize::new(240.0, 400.0),
    )
}

/// `count` lines, `{prefix}0` to `{prefix}{count - 1}`.
fn lines(prefix: &str, count: usize) -> String {
    (0..count)
        .map(|index| format!("{prefix}{index}"))
        .collect::<Vec<_>>()
        .join("<br>")
}

/// The paragraph fragments on a page, top-level or nested: the first word of each line and the fragment's height.
fn paragraphs(page: &Page) -> Vec<(Vec<String>, f32)> {
    struct Collect(Vec<(Vec<String>, f32)>);
    impl LayoutVisitor for Collect {
        fn visit_text_block(&mut self, block: &TextBlock) {
            let words = block
                .lines
                .iter()
                .filter_map(|line| {
                    let text: String = line.runs.iter().map(|run| run.text.as_str()).collect();
                    text.split_whitespace().next().map(str::to_owned)
                })
                .collect();
            let lines = block.lines.iter().map(|line| line.height).sum::<f32>();
            let natural = block.box_model.padding.vertical() + lines;
            self.0
                .push((words, block.box_model.size.height.resolve(natural)));
        }
    }
    let mut collect = Collect(Vec::new());
    for (_, element) in &page.elements {
        visit_layout_tree(element.as_ref(), &mut collect);
    }
    collect.0.retain(|(words, _)| !words.is_empty());
    collect.0
}

fn words(page: &Page) -> Vec<String> {
    paragraphs(page)
        .into_iter()
        .flat_map(|(words, _)| words)
        .collect()
}

fn names(prefix: &str, range: std::ops::Range<usize>) -> Vec<String> {
    range.map(|index| format!("{prefix}{index}")).collect()
}

#[test]
fn a_paragraph_that_crosses_the_page_end_breaks_between_its_lines() {
    let pages = pages(&format!(
        r#"<div style="height:300pt"></div><p>{}</p>"#,
        lines("a", 10)
    ));
    assert_eq!(pages.len(), 2);
    assert_eq!(
        words(&pages[0]),
        names("a", 0..5),
        "five lines fit in 100pt"
    );
    assert_eq!(words(&pages[1]), names("a", 5..10));
    assert_eq!(pages[1].elements[0].0, 0.0, "the rest starts page 2");
}

#[test]
fn a_paragraph_moves_whole_when_one_line_would_be_left_before_the_break() {
    let pages = pages(&format!(
        r#"<div style="height:380pt"></div><p>{}</p>"#,
        lines("b", 6)
    ));
    assert_eq!(words(&pages[0]), Vec::<String>::new(), "orphans: 2");
    assert_eq!(words(&pages[1]), names("b", 0..6));
}

#[test]
fn a_paragraph_leaves_two_lines_for_the_next_page() {
    let pages = pages(&format!(
        r#"<div style="height:310pt"></div><p>{}</p>"#,
        lines("c", 6)
    ));
    assert_eq!(
        words(&pages[0]),
        names("c", 0..4),
        "five lines fit, but widows: 2 keeps two together"
    );
    assert_eq!(words(&pages[1]), names("c", 4..6));
}

#[test]
fn a_paragraph_that_avoids_breaks_inside_moves_whole() {
    let pages = pages(&format!(
        r#"<div style="height:300pt"></div><p style="break-inside:avoid">{}</p>"#,
        lines("d", 10)
    ));
    assert_eq!(words(&pages[0]), Vec::<String>::new());
    assert_eq!(words(&pages[1]), names("d", 0..10));
}

#[test]
fn a_box_breaks_inside_its_paragraph_which_reaches_the_page_end() {
    let pages = pages(&format!(
        r#"<div style="height:300pt"></div>
        <div style="background:#ccc"><p>{}</p><p style="background:#fc0">{}</p></div>"#,
        lines("e", 2),
        lines("f", 6)
    ));
    assert_eq!(pages.len(), 2);
    let first = paragraphs(&pages[0]);
    assert_eq!(
        first[1].0,
        names("f", 0..3),
        "three lines fit below e0 and e1"
    );
    assert_eq!(
        first[1].1, 64.0,
        "the fragment fills the 64pt left, not just its 54pt of lines"
    );
    assert_eq!(words(&pages[1]), names("f", 3..6));
}

#[test]
fn an_image_in_a_box_that_crosses_the_page_end_moves_to_the_next_page_whole() {
    let pages = pages(&format!(
        r#"<div style="height:300pt"></div>
        <div><p>{}</p><svg width="50" height="120" style="display:block"><rect width="50" height="120"/></svg></div>"#,
        lines("g", 2)
    ));
    assert_eq!(pages.len(), 2);
    assert_eq!(
        words(&pages[0]),
        names("g", 0..2),
        "the box breaks before the image"
    );
    let image_page = pages[1]
        .elements
        .iter()
        .find(|(_, element)| words_in(element.as_ref()).is_empty());
    let (top, _) = image_page.expect("the image is on page 2");
    assert_eq!(*top, 0.0);
}

fn words_in(element: &dyn crate::layout::elements::LayoutElement) -> Vec<String> {
    struct Collect(Vec<String>);
    impl LayoutVisitor for Collect {
        fn visit_text_block(&mut self, block: &TextBlock) {
            self.0.extend(block.lines.iter().map(|line| {
                line.runs
                    .iter()
                    .map(|run| run.text.as_str())
                    .collect::<String>()
            }));
        }
    }
    let mut collect = Collect(Vec::new());
    visit_layout_tree(element, &mut collect);
    collect.0
}

#[test]
fn a_paragraph_taller_than_a_page_reaches_the_end_of_its_first_page() {
    let pages = pages(&format!(
        r#"<div style="height:10pt"></div><p style="padding:5pt;background:#fc0">{}</p>"#,
        lines("h", 30)
    ));
    let (words, height) = paragraphs(&pages[0]).remove(0);
    assert_eq!(
        words.len(),
        21,
        "21 lines of 18pt fit in the 385pt below the top padding"
    );
    assert_eq!(
        height, 390.0,
        "it reaches the page end, 7pt below its last line"
    );
}

#[test]
fn a_table_whose_first_body_row_does_not_fit_below_its_header_moves_whole() {
    let row = r#"<tr style="break-inside:avoid"><td>row</td></tr>"#;
    let pages = pages(&format!(
        r#"<div style="height:350pt"></div>
        <div><p>Title</p><table style="border-collapse:collapse"><thead><tr><th>Head</th></tr></thead>
        <tbody>{row}{row}{row}</tbody></table></div>"#
    ));
    assert_eq!(pages.len(), 2);
    assert_eq!(words(&pages[0]), vec!["Title"], "the title breaks away");
    assert_eq!(tables(&pages[0]), 0, "the header isn't left alone");
    assert_eq!(tables(&pages[1]), 1);
}

fn tables(page: &Page) -> usize {
    struct Count(usize);
    impl LayoutVisitor for Count {
        fn visit_table(&mut self, _element: &crate::layout::elements::Table) {
            self.0 += 1;
        }
    }
    let mut count = Count(0);
    for (_, element) in &page.elements {
        visit_layout_tree(element.as_ref(), &mut count);
    }
    count.0
}
