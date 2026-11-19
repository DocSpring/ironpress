//! Atomic inline boxes (images, inline-blocks, inline-flex, inline-grid) share their line with the text around
//! them, and the line box takes their height (CSS 2.1 §10.8).

use crate::layout::elements::{FlexRow, GridRow, LayoutElementTestExt, LayoutVisitor, TextBlock};
use crate::layout::engine::{InlineBox, TextLine, visit_layout_tree};
use crate::types::PageSize;

use super::support::{layout_pages_at, layout_pages_at_with_fonts};

const IMAGE: &str = "data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAgAAAAECAIAAAA8r+mnAAAAIElEQVR42mM4IScHR3I9NnDEgFNCzu0EHN1ZJQJHOCUAni4lgeO2HLIAAAAASUVORK5CYII=";

/// The lines holding atomic inline boxes, in order, those inside the boxes after their own line: a block's, and a
/// flex or grid item's own lines.
fn lines_with_boxes(pages: &[crate::layout::engine::Page]) -> Vec<TextLine> {
    #[derive(Default)]
    struct Lines(Vec<TextLine>);
    impl Lines {
        fn take(&mut self, lines: &[TextLine]) {
            self.0.extend(
                lines
                    .iter()
                    .filter(|line| line.runs.iter().any(|run| run.inline_box.is_some()))
                    .cloned(),
            );
        }
    }
    impl LayoutVisitor for Lines {
        fn visit_text_block(&mut self, block: &TextBlock) {
            self.take(&block.lines);
        }
        fn visit_flex_row(&mut self, row: &FlexRow) {
            for cell in &row.content.cells {
                self.take(&cell.lines);
            }
        }
        fn visit_grid_row(&mut self, row: &GridRow) {
            for cell in &row.content.cells {
                self.take(&cell.layout.content.lines);
            }
        }
    }
    let mut lines = Lines::default();
    for page in pages {
        for (_, element) in &page.elements {
            visit_layout_tree(element.as_ref(), &mut lines);
        }
    }
    lines.0
}

fn boxes(line: &TextLine) -> Vec<&InlineBox> {
    line.runs
        .iter()
        .filter_map(|run| run.inline_box.as_deref())
        .collect()
}

fn text(line: &TextLine) -> String {
    line.runs
        .iter()
        .filter(|run| run.inline_box.is_none())
        .map(|run| run.text.as_str())
        .collect()
}

/// Two adjacent inline-blocks that just overflow the line go on lines of their own: there's a soft wrap opportunity
/// between them, as Chrome breaks there.
#[test]
fn inline_blocks_wrap_when_they_just_overflow() {
    let per_line = |width: &str| -> Vec<usize> {
        let pages = layout_pages_at(
            &format!(
                r#"<div style="width:20pt;font-size:0"><div style="display:inline-block;width:{width};height:1pt"></div><div style="display:inline-block;width:{width};height:1pt"></div></div>"#
            ),
            PageSize::A4,
        );
        lines_with_boxes(&pages)
            .iter()
            .map(|line| boxes(line).len())
            .collect()
    };
    assert_eq!(per_line("10pt"), [2]);
    assert_eq!(per_line("10.01pt"), [1, 1]);
}

#[test]
fn an_inline_grid_keeps_the_spaces_on_each_side() {
    let pages = layout_pages_at(
        r#"<div style="font-size:20px;line-height:1.5">L <span style="display:inline-grid;grid-template-columns:40px 40px;grid-template-rows:40px"><span></span><span></span></span> R</div>"#,
        PageSize::new(264.0, 120.0),
    );
    let lines = lines_with_boxes(&pages);
    assert_eq!(lines.len(), 1);
    let runs: Vec<&str> = lines[0].runs.iter().map(|run| run.text.as_str()).collect();
    let at = lines[0]
        .runs
        .iter()
        .position(|run| run.inline_box.is_some())
        .expect("the inline-grid");
    assert!(
        runs[..at].concat().ends_with(' '),
        "the space before the inline-grid stays: {runs:?}"
    );
    assert!(
        runs[at + 1..].concat().starts_with(' '),
        "the space after the inline-grid stays: {runs:?}"
    );
}

#[test]
fn an_image_line_reserves_the_text_below_its_baseline() {
    let font_bytes = std::fs::read(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/parity/fonts/ParitySans.ttf"),
    )
    .expect("ParitySans fixture font");
    let font = crate::parser::ttf::parse_ttf(font_bytes).expect("valid ParitySans fixture");
    let fonts = std::collections::HashMap::from([("paritysans".to_string(), font)]);
    let pages = layout_pages_at_with_fonts(
        &format!(
            r#"<div style="box-sizing:border-box;width:320px;margin:24px;padding:10px;border:2px solid #22223b;background:#c9ada7;font-family:ParitySans;font-size:26px;line-height:1.2"><img style="width:40px;height:40px" src="{IMAGE}"></div>"#
        ),
        PageSize::new(336.0, 90.0),
        crate::types::Margin::uniform(0.0),
        &fonts,
    );
    assert_eq!(pages.len(), 1, "the image is laid out once");
    let lines = lines_with_boxes(&pages);
    assert_eq!(lines.len(), 1);
    // Chrome: 40px of image above the baseline and the strut's 7.1875px below it (its 7px descent and the leading
    // the line-height leaves below, 31.1875px in 1/64px less the font's 31px)
    assert!(
        (lines[0].height - 35.390_625).abs() < 0.01,
        "Chrome's line box is 47.1875px; got {}pt",
        lines[0].height
    );
}

#[test]
fn an_inline_block_image_in_a_contextual_selector_shares_the_text_line() {
    let pages = layout_pages_at(
        &format!(
            r#"<style>
                .asset {{ display: none; }}
                .feature > .own > .asset {{ display: inline-block; width: 34px; height: 24px; }}
                .own {{ height: 22px; }}
            </style>
            <div class="feature"><div class="own"><span>A</span><img class="asset" alt="" src="{IMAGE}"><span>B</span></div></div>"#
        ),
        PageSize::new(400.0, 90.0),
    );
    let lines = lines_with_boxes(&pages);
    assert_eq!(lines.len(), 1);
    assert_eq!(boxes(&lines[0]).len(), 1);
    assert_eq!(text(&lines[0]), "AB");
    let height = crate::layout::paginate::estimate_element_height(pages[0].elements[0].1.as_ref());
    assert!(
        (height - 16.5).abs() < 0.01,
        "the taller line overflows the definite 22px box; got {height}pt"
    );
}

#[test]
fn an_out_of_flow_sibling_does_not_split_the_line() {
    let pages = layout_pages_at(
        &format!(
            r#"<style>
                .asset {{ display: inline-block; width: 34px; height: 24px; }}
                .out {{ position: absolute; right: 4px; }}
            </style>
            <div style="position:relative"><span>A</span><span class="out">B</span><img class="asset" alt="" src="{IMAGE}"></div>"#
        ),
        PageSize::new(400.0, 90.0),
    );
    let lines = lines_with_boxes(&pages);
    assert_eq!(
        lines.len(),
        1,
        "the positioned sibling leaves the line whole"
    );
    assert_eq!(text(&lines[0]), "A", "the image follows the in-flow text");
    assert_eq!(boxes(&lines[0]).len(), 1);
}

#[test]
fn a_flex_items_max_content_width_holds_its_text_and_image() {
    let pages = layout_pages_at(
        include_str!(
            "../../../tests/parity/cases/interactions/interactions-cartesian-flexbox-x-images-replaced.html"
        ),
        PageSize::new(390.0, 150.0),
    );
    let lines = lines_with_boxes(&pages);
    let stages = lines
        .iter()
        .find(|line| boxes(line).len() == 3)
        .expect("the three inline-flex stages share one line");
    let first_stage = boxes(stages)[0].content[0]
        .inspect_flex(Clone::clone)
        .expect("the first stage is a flex formatting context");
    let node = first_stage.content.cells[0].nested_elements[0]
        .inspect_flex(Clone::clone)
        .expect("the node is its own flex formatting context");
    let item = &node.content.cells[0];
    assert!(
        (50.0..60.0).contains(&item.width),
        "the item is as wide as both words and the 25.5pt image: {}",
        item.width,
    );
    assert!(
        item.x_offset > 10.0,
        "justify-content:center places the shrink-wrapped item: {}",
        item.x_offset,
    );
}

#[test]
fn a_positioned_ancestor_keeps_the_image_after_the_text() {
    let pages = layout_pages_at(
        include_str!(
            "../../../tests/parity/cases/interactions/interactions-cartesian-images-replaced-x-positioning.html"
        ),
        PageSize::new(390.0, 150.0),
    );
    let mut images_after_text = Vec::new();
    for line in &lines_with_boxes(&pages) {
        let mut seen_text = false;
        for run in &line.runs {
            let image = run.inline_box.as_deref().is_some_and(|inline| {
                inline
                    .content
                    .iter()
                    .any(|node| node.inspect_image(|_| ()).is_some())
            });
            if image {
                images_after_text.push(seen_text);
            } else if !run.text.trim().is_empty() {
                seen_text = true;
            }
        }
    }
    assert_eq!(images_after_text.len(), 3, "{images_after_text:?}");
    assert!(
        images_after_text[0],
        "the positioned ancestor doesn't move the image to the start of its line"
    );
}

/// An inline-block in a list item's, a flex item's or a grid item's lines is laid out as its box: its block child is
/// a block inside it, not text approximated into lines.
#[test]
fn an_inline_block_in_list_flex_and_grid_items_is_laid_out_as_its_box() {
    let inline_block = r#"<span style="display:inline-block;width:60px;border:1px solid"><div style="height:30px">A</div></span>"#;
    for (case, markup) in [
        ("list item", format!("<ul><li>x {inline_block} y</li></ul>")),
        (
            "flex item",
            format!(
                r#"<div style="display:flex"><div><span>x {inline_block} y</span></div></div>"#
            ),
        ),
        (
            "grid item",
            format!(
                r#"<div style="display:grid"><div><span>x {inline_block} y</span></div></div>"#
            ),
        ),
    ] {
        let pages = layout_pages_at(&markup, PageSize::new(300.0, 200.0));
        let lines = lines_with_boxes(&pages);
        let inline = lines
            .iter()
            .flat_map(boxes)
            .find(|inline| inline.width > 40.0)
            .unwrap_or_else(|| panic!("{case}: the inline-block shares a line"));
        assert!(
            !inline.content.is_empty() && inline.lines.is_empty(),
            "{case}: the inline-block's contents are laid out in its box"
        );
        assert!(
            (inline.outer_height() - 24.0).abs() < 0.01,
            "{case}: 30px of content and two 1px borders are 24pt, got {}",
            inline.outer_height()
        );
    }
}

/// A relatively positioned inline-block keeps its width in the line: its offset moves where it's painted, not the
/// boxes after it (CSS 2.1 §9.4.3).
#[test]
fn a_relative_offset_does_not_widen_an_inline_block() {
    let pages = layout_pages_at(
        r#"<div style="font-size:0"><span style="display:inline-block;width:40px;height:20px;position:relative;left:20px;top:10px"></span><span style="display:inline-block;width:40px;height:20px"></span></div>"#,
        PageSize::new(300.0, 100.0),
    );
    let lines = lines_with_boxes(&pages);
    assert_eq!(lines.len(), 1);
    let widths: Vec<f32> = boxes(&lines[0]).iter().map(|inline| inline.width).collect();
    assert_eq!(widths, [30.0, 30.0], "each 40px box is 30pt wide");
}

/// An inline grid wider than its tracks takes its own width in the line, not its tracks' (CSS Grid 1 §5.1).
#[test]
fn an_inline_grid_takes_its_own_width_in_the_line() {
    let pages = layout_pages_at(
        r#"<div><div style="display:inline-grid;grid-template-columns:35px 35px;width:76px;height:20px"><span></span><span></span></div>x</div>"#,
        PageSize::new(300.0, 100.0),
    );
    let lines = lines_with_boxes(&pages);
    assert_eq!(lines.len(), 1);
    let widths: Vec<f32> = boxes(&lines[0]).iter().map(|inline| inline.width).collect();
    assert_eq!(
        widths,
        [57.0],
        "the 76px grid is 57pt wide, not its tracks' 52.5pt"
    );
}

/// Where a filter inside an inline box is rasterized: each box's place in its line, after the text before it and
/// moved by `text-align`, as the line is painted.
#[test]
fn inline_box_origins_follow_the_line() {
    for (align, first_x) in [("left", 0.0), ("center", 60.0)] {
        let pages = layout_pages_at(
            &format!(
                r#"<div style="width:200px;font-size:0;text-align:{align}"><span style="display:inline-block;width:20px;height:10px"><b>a</b></span><span style="display:inline-block;width:20px;height:10px"><b>b</b></span></div>"#
            ),
            PageSize::new(300.0, 100.0),
        );
        let block = pages[0].elements[0]
            .1
            .inspect_text(Clone::clone)
            .expect("the boxes' block");
        let origins = crate::layout::filter::surface::inline_box_origins(
            &block.lines,
            Some(150.0),
            block.text.alignment,
            0.0,
            &std::collections::HashMap::new(),
            |baseline| baseline,
        )
        .expect("the boxes measure");
        let xs: Vec<f32> = origins.iter().map(|origin| origin.x).collect();
        assert_eq!(xs, [first_x, first_x + 15.0], "{align}");
    }
}
