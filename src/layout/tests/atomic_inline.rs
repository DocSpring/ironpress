use crate::layout::elements::{FlexRow, LayoutVisitor, TextBlock};
use crate::layout::engine::visit_layout_tree;

use super::support::{inline_boxes, layout_pages, visible_runs};

#[derive(Default)]
struct AtomicBoxCount {
    embedded: usize,
    independent_cells: usize,
}

impl LayoutVisitor for AtomicBoxCount {
    fn visit_text_block(&mut self, block: &TextBlock) {
        self.embedded += block
            .lines
            .iter()
            .flat_map(|line| &line.runs)
            .filter(|run| run.inline_box.is_some())
            .count();
    }

    fn visit_flex_row(&mut self, row: &FlexRow) {
        self.independent_cells += row.content.cells.len();
    }
}

#[test]
fn inline_block_embedded_between_text_is_not_laid_out_twice() {
    let pages = layout_pages(
        r#"<style>
            .line { width: 280px; padding: 8px; border: 2px solid; font-size: 18px; }
            .box { display: inline-block; width: 28px; height: 28px; }
        </style>
        <div class="line">Ax<span class="box"></span>yQ</div>"#,
    );

    let mut count = AtomicBoxCount::default();
    for page in &pages {
        for (_, element) in &page.elements {
            visit_layout_tree(element.as_ref(), &mut count);
        }
    }

    assert_eq!(pages.len(), 1);
    assert_eq!(count.embedded, 1);
    assert_eq!(count.independent_cells, 0);
}

#[test]
fn computed_inline_level_boxes_survive_adjacent_block_siblings() {
    for tag in ["div", "p", "section", "label"] {
        for display in ["inline", "inline-block", "inline-flex"] {
            for inline_first in [true, false] {
                let inline = format!(r#"<{tag} class="subject">STEM</{tag}>"#);
                let children = if inline_first {
                    format!(r#"{inline}<div>SIB</div>"#)
                } else {
                    format!(r#"<div>SIB</div>{inline}"#)
                };
                let markup = format!(
                    r#"<style>.subject {{ display: {display}; }}</style><div>{children}</div>"#
                );
                // The tree walk reaches the text of the nodes an inline box was laid out as
                let visible_text = visible_runs(&markup)
                    .into_iter()
                    .map(|run| run.text)
                    .collect::<String>();
                let case = format!("tag={tag}, display={display}, inline_first={inline_first}");

                assert_eq!(
                    visible_text.matches("STEM").count(),
                    1,
                    "{case}: {visible_text:?}"
                );
                assert_eq!(
                    visible_text.matches("SIB").count(),
                    1,
                    "{case}: {visible_text:?}"
                );
            }
        }
    }
}

#[test]
fn an_inline_block_is_as_wide_as_its_widest_line_between_forced_breaks() {
    let widths = |contents: &str| {
        let markup = format!(
            r#"<style>.box {{ display: inline-block; padding: 0 5px; border: 2px solid }}</style>
            <p>Before <span class="box">{contents}</span> after</p>"#
        );
        inline_boxes(&markup)
            .first()
            .map(|inline| inline.width)
            .expect("an inline box")
    };
    let one_line = widths("inline block");
    let broken = widths("inline block<br>two");
    assert!(
        (broken - one_line).abs() < 0.01,
        "a <br> ends a max-content line: {broken} vs {one_line}"
    );
    assert!(widths("inline block two") > one_line + 10.0);
}

#[test]
fn inline_level_boxes_are_laid_out_as_their_boxes() {
    for display in ["inline-block", "inline-flex", "inline-grid", "inline-table"] {
        let markup = format!(
            r#"<style>.box {{ display: {display}; border: 2px solid; padding: 3px }}</style>
            <p>Before <span class="box"><span>alpha</span> <span>beta</span></span> after</p>"#
        );
        let boxes = inline_boxes(&markup);
        assert_eq!(boxes.len(), 1, "{display}");
        assert!(!boxes[0].content.is_empty(), "{display}: laid out as nodes");
        let text: Vec<String> = visible_runs(&markup)
            .into_iter()
            .map(|run| run.text.trim().to_string())
            .collect();
        assert!(
            text.iter().any(|t| t.contains("alpha")) && text.iter().any(|t| t.contains("beta")),
            "{display}: {text:?}"
        );
    }
}
