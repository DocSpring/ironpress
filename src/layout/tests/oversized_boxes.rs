//! A box taller than a page breaks across pages whatever stops it breaking: `break-inside: avoid` can't keep it on
//! one page, so it breaks as if it were `auto` (CSS Fragmentation 3 §4.4, "a break inside may be forced when the box
//! doesn't fit"), and a flex row's items each continue on the next page (§5.2). Nothing in it is lost and every
//! page holds some of it. Each expectation is what Chrome prints for the same markup.

use crate::layout::engine::Page;
use crate::types::PageSize;

use super::support::{layout_pages_at, words_per_page};

/// Pages 240pt by 400pt with no margins: 22 rows or lines of 18pt fit on each.
fn pages(markup: &str) -> Vec<Page> {
    layout_pages_at(
        &format!(
            "<style>body {{ margin: 0; font: 12pt/18pt sans-serif }} p {{ margin: 0 }} \
             table {{ border-collapse: collapse }} td {{ padding: 0 }}</style>{markup}"
        ),
        PageSize::new(240.0, 400.0),
    )
}

fn rows(prefix: &str, count: usize) -> String {
    (0..count)
        .map(|index| format!("<tr><td>{prefix}{index}</td></tr>"))
        .collect()
}

fn paragraphs(prefix: &str, count: usize) -> String {
    (0..count)
        .map(|index| format!("<p>{prefix}{index}</p>"))
        .collect()
}

fn names(prefix: &str, count: usize) -> Vec<String> {
    (0..count).map(|index| format!("{prefix}{index}")).collect()
}

/// Every page's words, in order, with those starting `prefix` picked out
fn picked(pages: &[Page], prefix: &str) -> Vec<Vec<String>> {
    words_per_page(pages)
        .into_iter()
        .map(|words| {
            words
                .into_iter()
                .filter(|word| word.starts_with(prefix))
                .collect()
        })
        .collect()
}

fn assert_all_once_in_order(pages: &[Page], prefix: &str, count: usize) {
    let per_page = picked(pages, prefix);
    assert_eq!(
        per_page.concat(),
        names(prefix, count),
        "every {prefix} once, in order: {per_page:?}"
    );
    assert!(
        per_page.iter().filter(|words| !words.is_empty()).count() > 1,
        "{prefix} spans pages: {per_page:?}"
    );
}

#[test]
fn a_table_taller_than_a_page_in_a_box_that_avoids_breaks_breaks_anyway() {
    let pages = pages(&format!(
        r#"<div style="break-inside: avoid"><table>{}</table></div><p>after</p>"#,
        rows("r", 40)
    ));
    assert_all_once_in_order(&pages, "r", 40);
    assert_eq!(
        words_per_page(&pages).concat().last().map(String::as_str),
        Some("after")
    );
}

#[test]
fn a_box_that_avoids_breaks_with_a_tall_table_after_other_content_breaks_anyway() {
    let pages = pages(&format!(
        r#"<div style="height: 100pt"></div><div style="break-inside: avoid"><p>heading</p><table>{}</table></div>"#,
        rows("r", 40)
    ));
    assert_all_once_in_order(&pages, "r", 40);
}

#[test]
fn a_table_taller_than_a_page_that_avoids_breaks_breaks_anyway() {
    let pages = pages(&format!(
        r#"<table style="break-inside: avoid">{}</table>"#,
        rows("r", 40)
    ));
    assert_all_once_in_order(&pages, "r", 40);
}

#[test]
fn a_kept_table_whose_rows_avoid_breaks_breaks_between_rows() {
    let pages = pages(&format!(
        r#"<div style="page-break-inside: avoid"><table>{}</table></div>"#,
        rows("r", 40).replace("<tr>", r#"<tr style="break-inside: avoid">"#)
    ));
    assert_all_once_in_order(&pages, "r", 40);
}

#[test]
fn a_kept_box_that_fits_a_fresh_page_still_moves_whole() {
    let pages = pages(&format!(
        r#"<div style="height: 300pt"></div><div style="break-inside: avoid"><table>{}</table></div>"#,
        rows("r", 10)
    ));
    let per_page = picked(&pages, "r");
    assert_eq!(per_page[0], Vec::<String>::new());
    assert_eq!(per_page[1], names("r", 10));
}

#[test]
fn a_flex_row_taller_than_a_page_continues_each_item_on_the_next_page() {
    let pages = pages(&format!(
        r#"<div style="display: flex"><div style="flex: 1">{}</div><div style="flex: 1">{}</div></div><p>after</p>"#,
        paragraphs("c", 40),
        paragraphs("d", 40)
    ));
    assert_all_once_in_order(&pages, "c", 40);
    assert_all_once_in_order(&pages, "d", 40);
    assert_eq!(
        words_per_page(&pages).concat().last().map(String::as_str),
        Some("after")
    );
}

#[test]
fn a_flex_row_with_a_table_taller_than_a_page_continues_it_on_the_next_page() {
    let pages = pages(&format!(
        r#"<div style="display: flex"><div style="flex: 1"><table>{}</table></div><div style="flex: 1">side</div></div>"#,
        rows("t", 40)
    ));
    assert_all_once_in_order(&pages, "t", 40);
}

#[test]
fn a_kept_flex_row_taller_than_a_page_breaks_anyway() {
    let pages = pages(&format!(
        r#"<div style="display: flex; break-inside: avoid"><div style="flex: 1">{}</div><div style="flex: 1">{}</div></div>"#,
        paragraphs("c", 40),
        paragraphs("d", 30)
    ));
    assert_all_once_in_order(&pages, "c", 40);
    assert_all_once_in_order(&pages, "d", 30);
}
