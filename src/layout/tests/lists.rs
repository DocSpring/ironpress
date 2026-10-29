//! Lists: the numbers of their items in every formatting context.

use super::support::visible_runs;

fn words(markup: &str) -> String {
    let runs = visible_runs(markup);
    runs.iter()
        .flat_map(|run| run.text.split_whitespace())
        .collect::<Vec<_>>()
        .join(" ")
}

#[test]
fn list_that_is_a_grid_item_numbers_its_items() {
    let grid = r#"<div style="display:grid;grid-template-columns:300px">"#;

    assert_eq!(
        words(&format!(
            r#"{grid}<ol start="3"><li>Three</li><li>Four</li></ol></div>"#
        )),
        "3. Three 4. Four"
    );
    assert_eq!(
        words(&format!(
            r#"{grid}<ol reversed><li><p>Two</p></li><li><p>One</p></li></ol></div>"#
        )),
        "2. Two 1. One"
    );
}

#[test]
fn list_in_a_flex_container_numbers_its_items() {
    assert_eq!(
        words(r#"<div style="display:flex"><ol start="3"><li>Three</li><li>Four</li></ol></div>"#),
        "3. Three 4. Four"
    );
}
