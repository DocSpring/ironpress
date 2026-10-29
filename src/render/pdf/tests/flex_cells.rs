use super::*;

#[test]
fn nested_flex_cell_paints_both_shadow_phases() {
    use crate::parser::css::parse_stylesheet;

    let document = crate::parser::html::parse_html_with_styles(
        r#"
        <style>
            .host { width: 160px; height: 100px; background: white; }
            .row { display: flex; width: 120px; height: 60px; }
            .item {
                width: 80px;
                height: 40px;
                background: white;
                box-shadow: 7px 5px 0 #ef476f, inset 0 0 0 2px #ffd166;
            }
        </style>
        <div class="host"><div class="row"><div class="item">shadow</div></div></div>
        "#,
    )
    .unwrap();
    let rules = document
        .stylesheets
        .iter()
        .flat_map(|stylesheet| parse_stylesheet(stylesheet))
        .collect::<Vec<_>>();
    let pages = crate::layout::engine::layout_with_rules(
        &document.nodes,
        PageSize::A4,
        Margin::uniform(0.0),
        &rules,
    );
    let pdf = render_pdf(&pages, PageSize::A4, Margin::uniform(0.0)).unwrap();
    let content = String::from_utf8_lossy(&pdf);

    assert!(
        content.contains("0.9372549 0.2784314 0.43529412 rg"),
        "nested flex item must paint its outset shadow"
    );
    assert!(
        content.contains("1 0.81960785 0.4 rg"),
        "nested flex item must paint its inset shadow"
    );
}

#[test]
fn grid_item_paints_its_inline_blocks() {
    for (label, markup) in [
        (
            "between text",
            r#"<div style="display:grid"><div>a <span style="display:inline-block;width:30pt;height:10pt;background:#00f"></span> c</div></div>"#,
        ),
        (
            "alone",
            r#"<div style="display:grid"><div><span style="display:inline-block;width:30pt;height:10pt;background:#00f"></span></div></div>"#,
        ),
        (
            "nested grid",
            r#"<div style="display:grid"><div style="display:grid"><div>a <span style="display:inline-block;width:30pt;height:10pt;background:#00f"></span></div></div></div>"#,
        ),
    ] {
        let pdf = crate::html_to_pdf(markup).unwrap();
        let content = String::from_utf8_lossy(&pdf);
        // The blue fill is followed by the 30pt-wide box's rectangle.
        let painted = content
            .split("0 0 1 rg\n")
            .skip(1)
            .filter_map(|rest| rest.lines().next())
            .any(|rect| {
                let fields: Vec<&str> = rect.split_whitespace().collect();
                fields.len() == 5 && fields[4] == "re" && fields[2] == "30"
            });
        assert!(
            painted,
            "the inline-block's background is missing ({label})"
        );
    }
}
