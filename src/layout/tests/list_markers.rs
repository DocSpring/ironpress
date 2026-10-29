//! The UA's built-in markers for nested lists.

use super::support::collected_runs;

#[derive(Debug, PartialEq)]
enum Bullet {
    Disc,
    Circle,
    Square,
}

fn bullets(markup: &str) -> Vec<Bullet> {
    collected_runs(markup)
        .into_iter()
        .filter_map(|run| {
            let paint = run.inline_box?.paint;
            if paint.centered_stroke.is_some() {
                Some(Bullet::Circle)
            } else if paint.background_color.is_some() && paint.border_radii.is_zero() {
                Some(Bullet::Square)
            } else if paint.background_color.is_some() {
                Some(Bullet::Disc)
            } else {
                None
            }
        })
        .collect()
}

#[test]
fn nested_unordered_lists_use_circle_then_square_markers() {
    assert_eq!(
        bullets(
            "<ul><li>One<ul><li>Two<ul><li>Three<ul><li>Four</li></ul></li></ul></li></ul></li></ul>"
        ),
        [Bullet::Disc, Bullet::Circle, Bullet::Square, Bullet::Square]
    );
}

#[test]
fn ordered_lists_count_as_list_nesting_for_unordered_markers() {
    assert_eq!(
        bullets(
            "<ol><li>One<ul><li>Two<ol><li>Three<ul><li>Four</li></ul></li></ol></li></ul></li></ol>"
        ),
        [Bullet::Circle, Bullet::Square]
    );
}

#[test]
fn author_list_style_overrides_the_nested_marker() {
    assert_eq!(
        bullets(
            r#"<style>ul ul { list-style-type: disc; }</style>
            <ul><li>One<ul><li>Two</li></ul></li></ul>"#
        ),
        [Bullet::Disc, Bullet::Disc]
    );
}
