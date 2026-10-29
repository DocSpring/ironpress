use super::support::{layout_pages, visible_runs};
use crate::layout::elements::test_support::LayoutElementTestExt;
use crate::layout::elements::{Container, Image, LayoutVisitor, TextBlock};
use crate::layout::engine::visit_layout_tree;
use crate::parser::css::{PageContentPolicy, PageContentReference};
use crate::style::computed::Position;

#[test]
fn running_element_retains_image_and_table_descendants() {
    let pages = layout_pages(
        r#"<style>
            .header { position: running(issue245); width: 180pt; height: 36pt }
        </style>
        <div class="header">
            <img src="data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mP8/5+hHgAHggJ/PchI7wAAAABJRU5ErkJggg=="
                 style="width: 12pt; height: 12pt" alt="">
            <table><tr><td>ISSUE245</td></tr></table>
        </div>
        <div>Body</div>"#,
    );
    let running = pages[0]
        .generated_content
        .running_element(&PageContentReference::new(
            "issue245".into(),
            PageContentPolicy::Last,
        ))
        .expect("running element");

    #[derive(Default)]
    struct RichDescendants {
        images: usize,
        table_rows: usize,
    }

    impl LayoutVisitor for RichDescendants {
        fn visit_image(&mut self, _image: &Image) {
            self.images += 1;
        }

        fn visit_table_row(&mut self, _row: &crate::layout::elements::TableRow) {
            self.table_rows += 1;
        }
    }

    let mut descendants = RichDescendants::default();
    visit_layout_tree(running, &mut descendants);
    assert_eq!(descendants.images, 1, "captured image descendant");
    assert_eq!(descendants.table_rows, 1, "captured table descendant");
}

#[test]
fn generated_content_survives_grid_item_block_content_layout() {
    let runs = visible_runs(
        r#"<style>
            .grid { display: grid; grid-template-columns: 100pt; }
            .item::before { content: "before"; }
            .item::after { content: "after"; }
        </style>
        <div class="grid">
            <div class="item"><div>body</div></div>
        </div>"#,
    );
    let text = runs.iter().map(|run| run.text.as_str()).collect::<String>();

    assert!(text.contains("before"), "missing ::before in {text:?}");
    assert!(text.contains("body"), "missing principal child in {text:?}");
    assert!(text.contains("after"), "missing ::after in {text:?}");
}

#[test]
fn generated_content_is_source_ordered_as_flex_items() {
    let runs = visible_runs(
        r#"<style>
            .flex { display: flex; width: 240pt; }
            .flex::before { content: "BEFORE"; color: red; }
            .flex::after { content: "AFTER"; color: green; }
        </style>
        <div class="flex"><strong>BODY</strong></div>"#,
    );
    let text = runs.iter().map(|run| run.text.as_str()).collect::<String>();

    assert_eq!(text, "BEFOREBODYAFTER");
}

#[test]
fn absolute_generated_flex_child_does_not_replace_in_flow_items() {
    let runs = visible_runs(
        r#"<style>
            .flex { display: flex; position: relative; width: 240pt; height: 80pt; }
            .flex::before {
                content: "ABSOLUTE";
                position: absolute;
                left: 0;
                top: 0;
            }
        </style>
        <div class="flex"><strong>FLOW</strong></div>"#,
    );
    let text = runs.iter().map(|run| run.text.as_str()).collect::<String>();

    assert!(
        text.contains("ABSOLUTE"),
        "missing generated child in {text:?}"
    );
    assert!(text.contains("FLOW"), "missing principal child in {text:?}");
}

#[test]
fn flex_counter_traversal_follows_generated_and_out_of_flow_source_order() {
    let runs = visible_runs(
        r#"<style>
            .flex { display: flex; position: relative; counter-reset: item; }
            .flex::before {
                counter-increment: item;
                content: "B" counter(item);
            }
            .absolute {
                position: absolute;
                counter-increment: item;
            }
            .absolute::before { content: "X" counter(item); }
            .flow { counter-increment: item; }
            .flow::before { content: "F" counter(item); }
            .flex::after {
                counter-increment: item;
                content: "A" counter(item);
            }
        </style>
        <div class="flex"><span class="absolute"></span><span class="flow"></span></div>"#,
    );
    let text = runs.iter().map(|run| run.text.as_str()).collect::<String>();

    for expected in ["B1", "X2", "F3", "A4"] {
        assert!(text.contains(expected), "missing {expected:?} in {text:?}");
    }
}

#[test]
fn empty_string_generated_box_retains_absolute_background_and_filter() {
    #[derive(Default)]
    struct GeneratedBox {
        found: bool,
        text_boxes: Vec<(Position, bool, bool)>,
        images: Vec<(Position, bool)>,
    }

    impl LayoutVisitor for GeneratedBox {
        fn visit_text_block(&mut self, block: &TextBlock) {
            let has_background = block.paint.background.color.is_some();
            let has_filter = block
                .paint
                .group
                .filter
                .as_ref()
                .is_some_and(|filter| filter.requires_source_surface());
            self.text_boxes
                .push((block.positioning.scheme, has_background, has_filter));
            self.found |=
                block.positioning.scheme == Position::Absolute && has_background && has_filter;
        }

        fn visit_image(&mut self, image: &Image) {
            let has_filter_overflow = !image.paint.raster_overflow.is_zero();
            self.images
                .push((image.positioning.scheme, has_filter_overflow));
            self.found |= image.positioning.scheme == Position::Absolute && has_filter_overflow;
        }
    }

    let pages = layout_pages(
        r#"<style>
            .wrap { position: relative; width: 72pt; height: 72pt; }
            .wrap::before {
                content: "";
                position: absolute;
                inset: 0;
                width: 72pt;
                height: 72pt;
                background: #336699;
                filter: blur(4pt);
            }
        </style><div class="wrap"></div>"#,
    );
    let mut generated = GeneratedBox::default();
    for (_, element) in &pages[0].elements {
        visit_layout_tree(element.as_ref(), &mut generated);
    }

    assert!(
        generated.found,
        "content: \"\" must generate its styled pseudo-element box; text={:?}, images={:?}",
        generated.text_boxes, generated.images,
    );
}

#[test]
fn generated_block_remains_inside_its_absolute_originating_box() {
    #[derive(Default)]
    struct AbsoluteGeneratedBox(Vec<(f32, f32, f32, usize)>);

    impl LayoutVisitor for AbsoluteGeneratedBox {
        fn visit_container(&mut self, container: &Container) {
            if container.positioning.scheme == Position::Absolute
                && container.children.iter().any(|child| {
                    child
                        .inspect_text(|block| block.paint.background.color.is_some())
                        .unwrap_or(false)
                })
            {
                self.0.push((
                    container.positioning.insets.left,
                    container.positioning.insets.top,
                    container.box_model.size.width.resolve(0.0),
                    container.children.len(),
                ));
            }
        }
    }

    let pages = layout_pages(
        r#"<style>
            * { margin:0; padding:0; box-sizing:border-box }
            .panel { position:relative; width:120pt; height:108pt }
            .top { position:absolute; left:12pt; top:9pt; width:72pt; height:72pt }
            .top::before {
                content:"";
                display:block;
                width:72pt;
                height:72pt;
                background:#ef233c;
            }
        </style><div class="panel"><div class="top"></div></div>"#,
    );
    let mut boxes = AbsoluteGeneratedBox::default();
    for (_, element) in &pages[0].elements {
        visit_layout_tree(element.as_ref(), &mut boxes);
    }

    assert_eq!(boxes.0, [(12.0, 9.0, 72.0, 1)]);
}

/// A running element is laid out once but shows each page's number, so the
/// space laid out for `counter(page)` and `counter(pages)` must hold the widest
/// value on any page: here "10/12", not the one-digit "0/0" layout starts with.
#[test]
fn running_page_numbers_get_room_for_their_widest_value() {
    use crate::layout::engine::{FlexRow, TextRun};
    use crate::style::computed::TARGET_PLACEHOLDER_START;

    let body = (2..=12)
        .map(|page| format!(r#"<div style="break-before: page">Page {page}</div>"#))
        .collect::<String>();
    let pages = layout_pages(&format!(
        r#"<style>
            .header {{ position: running(header); display: flex }}
            .number::after {{ content: counter(page) "/" counter(pages) }}
        </style>
        <div class="header"><span class="number"></span><span>end</span></div>
        <div>Page 1</div>{body}"#
    ));
    assert_eq!(pages.len(), 12);
    let running = pages[0]
        .generated_content
        .running_element(&PageContentReference::new(
            "header".into(),
            PageContentPolicy::Last,
        ))
        .expect("running element");

    #[derive(Default)]
    struct NumberCell(Option<(f32, TextRun)>);

    impl LayoutVisitor for NumberCell {
        fn visit_flex_row(&mut self, row: &FlexRow) {
            for cell in &row.content.cells {
                let run = cell
                    .lines
                    .iter()
                    .flat_map(|line| &line.runs)
                    .find(|run| run.text.contains(TARGET_PLACEHOLDER_START));
                if let Some(run) = run {
                    self.0 = Some((cell.width, run.clone()));
                }
            }
        }
    }

    let mut number = NumberCell::default();
    visit_layout_tree(running, &mut number);
    let (width, run) = number.0.expect("the page number's flex item");
    let widest = TextRun {
        text: "10/12".into(),
        ..run
    };
    let widest_width =
        crate::layout::text::measure_text_run_advance(&widest, &std::collections::HashMap::new());
    assert!(
        width >= widest_width - 0.01,
        "the item is {width}pt wide, and \"10/12\" needs {widest_width}pt"
    );
}
