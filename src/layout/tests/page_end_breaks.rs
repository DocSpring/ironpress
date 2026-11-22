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

/// Where the paragraph that starts with `word` is: its page and its top on that page.
fn find(pages: &[Page], word: &str) -> (usize, f32) {
    pages
        .iter()
        .enumerate()
        .find_map(|(index, page)| {
            page.elements
                .iter()
                .find(|(_, element)| {
                    words_in(element.as_ref())
                        .first()
                        .is_some_and(|line| line.starts_with(word))
                })
                .map(|(top, _)| (index, *top))
        })
        .unwrap_or_else(|| panic!("{word} isn't on any page"))
}

#[test]
fn an_empty_box_with_a_height_breaks_at_the_page_end() {
    let pages = pages(
        r#"<div style="height:300pt"></div><div style="min-height:150pt;background:#ccc"></div><p>after</p>"#,
    );
    assert_eq!(
        find(&pages, "after"),
        (1, 50.0),
        "100pt of the box ends page 1, and its last 50pt start page 2"
    );
}

#[test]
fn an_empty_box_that_avoids_breaks_inside_moves_whole() {
    let pages = pages(
        r#"<div style="height:300pt"></div><div style="min-height:150pt;break-inside:avoid;background:#ccc"></div><p>after</p>"#,
    );
    assert_eq!(find(&pages, "after"), (1, 150.0));
}

/// A box with 12pt of padding and border above and below its content, below a spacer that leaves it 90pt: its 72pt
/// of content fits with the top 12pt, but not with the bottom 12pt as well.
fn padded(display: &str, content: &str) -> Vec<Page> {
    pages(&format!(
        r#"<div style="height:310pt"></div>
        <div style="display:{display};padding:10pt;border:2pt solid #333;background:#eee">{content}</div>"#
    ))
}

#[test]
fn a_box_whose_bottom_padding_crosses_the_page_end_breaks_before_its_last_child() {
    let children = (0..4).map(|i| format!("<p>b{i}</p>")).collect::<String>();
    let pages = padded("block", &children);
    assert_eq!(pages.len(), 2);
    assert_eq!(
        words(&pages[0]),
        names("b", 0..3),
        "the padding and border go with b3"
    );
    assert_eq!(words(&pages[1]), names("b", 3..4));
}

#[test]
fn a_paragraph_whose_bottom_padding_crosses_the_page_end_breaks_before_its_last_lines() {
    let pages = padded("block", &format!("<p>{}</p>", lines("c", 4)));
    assert_eq!(pages.len(), 2);
    assert_eq!(
        words(&pages[0]),
        names("c", 0..2),
        "three lines fit with the padding, and widows: 2 moves c2 too"
    );
    assert_eq!(words(&pages[1]), names("c", 2..4));
}

#[test]
fn a_grid_whose_bottom_padding_crosses_the_page_end_moves_whole() {
    let children = (0..4).map(|i| format!("<p>g{i}</p>")).collect::<String>();
    let pages = padded("grid", &children);
    assert_eq!(pages.len(), 2);
    assert_eq!(pages[0].elements.len(), 1, "page 1 holds only the spacer");
    assert_eq!(
        pages[1].elements.len(),
        1,
        "the grid, not its padding and border alone"
    );
}

/// A two-column grid 60pt above the page end: six lines on the left, and on the right one line below `right_style`.
fn grid_with_a_lower_item(right_style: &str) -> Vec<Page> {
    pages(&format!(
        r#"<div style="height:340pt"></div>
        <div style="display:grid;grid-template-columns:120pt 120pt">
        <div>{}</div><div style="{right_style}">right</div></div>"#,
        lines("l", 6)
    ))
}

#[test]
fn a_grid_item_placed_too_low_to_start_on_the_page_continues_on_the_next() {
    for right_style in ["margin-top:50pt", "padding-top:50pt"] {
        let pages = grid_with_a_lower_item(right_style);
        assert_eq!(pages.len(), 2, "{right_style}");
        let words: Vec<_> = pages.iter().map(words_with_grid_items).collect();
        assert_eq!(
            words[0],
            names("l", 0..3),
            "{right_style}: the items fragment independently (CSS Break 3 parallel flows), so three lines on the \
             left stay on page 1, and the right item, whose line would be below the page end there, isn't drawn on it"
        );
        let mut page_2 = names("l", 3..6);
        page_2.push("right".to_owned());
        assert_eq!(words[1], page_2, "{right_style}: nothing is lost");
    }
}

#[test]
fn a_grid_row_breaks_when_every_item_starts_on_the_page() {
    let pages = grid_with_a_lower_item("margin-top:10pt");
    assert_eq!(pages.len(), 2);
    assert_eq!(pages[0].elements.len(), 2, "the row breaks after l2");
}

/// The first word of each line on a page, in paragraphs, table cells and grid items.
fn words_with_grid_items(page: &Page) -> Vec<String> {
    struct Collect(Vec<String>);
    impl Collect {
        fn lines(&mut self, lines: &[crate::layout::engine::TextLine]) {
            for line in lines {
                let text: String = line.runs.iter().map(|run| run.text.as_str()).collect();
                self.0
                    .extend(text.split_whitespace().next().map(str::to_owned));
            }
        }
    }
    impl LayoutVisitor for Collect {
        fn visit_text_block(&mut self, block: &TextBlock) {
            self.lines(&block.lines);
        }
        fn visit_table_row(&mut self, element: &crate::layout::elements::TableRow) {
            for cell in &element.content.cells {
                self.lines(&cell.layout.content.lines);
            }
        }
        fn visit_grid_row(&mut self, element: &crate::layout::elements::GridRow) {
            for cell in &element.content.cells {
                self.lines(&cell.layout.content.lines);
            }
        }
    }
    let mut collect = Collect(Vec::new());
    for (_, element) in &page.elements {
        visit_layout_tree(element.as_ref(), &mut collect);
    }
    collect.0
}

#[test]
fn a_table_in_a_grid_item_that_has_no_room_for_its_first_row_starts_the_next_page() {
    let pages = pages(
        r#"<div style="height:370pt"></div>
        <div style="display:grid"><div><p>Heading</p></div>
        <div><table><tr style="break-inside:avoid"><td>rowa</td></tr><tr><td>rowb</td></tr></table></div></div>"#,
    );
    assert_eq!(pages.len(), 2);
    assert!(
        !words_with_grid_items(&pages[0]).contains(&"rowa".to_owned()),
        "the first row isn't drawn past the end of page 1"
    );
    assert!(words_with_grid_items(&pages[1]).contains(&"rowa".to_owned()));
}
