use super::*;

fn solid(width: f32, color: crate::types::Color) -> crate::layout::engine::LayoutBorderSide {
    crate::layout::engine::LayoutBorderSide {
        width,
        color,
        style: BorderStyle::Solid,
    }
}

fn double(width: f32, color: crate::types::Color) -> crate::layout::engine::LayoutBorderSide {
    crate::layout::engine::LayoutBorderSide {
        width,
        color,
        style: BorderStyle::Double,
    }
}

#[test]
fn square_uniform_border_uses_the_canonical_centerline_stroke() {
    let mut content = String::new();
    let mut states = Vec::new();
    let mut counter = 0;

    assert!(paint_uniform_border(
        &mut content,
        RoundedRect::new(PdfRect::new(0.0, 0.0, 100.0, 50.0), CornerRadii::ZERO),
        solid(4.0, crate::types::Color::BLACK),
        PdfContentSpace::Points,
        &mut states,
        &mut counter,
    ));

    assert!(content.ends_with("S\n"));
    assert!(!content.contains("f*\n"));
}

#[test]
fn square_uniform_border_retains_page_css_serialization() {
    let mut content = String::new();
    let mut states = Vec::new();
    let mut counter = 0;
    let content_space =
        PdfContentSpace::page_css(PageContentTransform::print(PdfVector::new(150.0, 150.0)));

    assert!(paint_uniform_border(
        &mut content,
        RoundedRect::new(PdfRect::new(12.0, 12.0, 117.0, 123.0), CornerRadii::ZERO,),
        solid(0.75, crate::types::Color::BLACK),
        content_space,
        &mut states,
        &mut counter,
    ));

    assert!(content.starts_with("q\n"));
    assert!(content.contains("1 w\n"));
    assert!(content.ends_with("S\nQ\n"));
}

#[test]
fn circular_uniform_border_uses_the_canonical_centerline_stroke() {
    let mut content = String::new();
    let mut states = Vec::new();
    let mut counter = 0;

    assert!(paint_uniform_border(
        &mut content,
        RoundedRect::new(
            PdfRect::new(0.0, 0.0, 100.0, 50.0),
            CornerRadii::circular(12.0),
        ),
        solid(4.0, crate::types::Color::BLACK),
        PdfContentSpace::Points,
        &mut states,
        &mut counter,
    ));

    assert!(content.ends_with("S\n"));
    assert!(!content.contains("f*\n"));
}

#[test]
fn elliptical_uniform_border_uses_the_css_border_ring() {
    let mut content = String::new();
    let mut states = Vec::new();
    let mut counter = 0;

    assert!(paint_uniform_border(
        &mut content,
        RoundedRect::new(
            PdfRect::new(0.0, 0.0, 100.0, 50.0),
            CornerRadii::uniform(crate::types::CornerRadius::new(18.0, 9.0)),
        ),
        solid(4.0, crate::types::Color::BLACK),
        PdfContentSpace::Points,
        &mut states,
        &mut counter,
    ));

    assert!(content.ends_with("f*\n"));
    assert!(!content.contains("S\n"));
}

#[test]
fn circular_uniform_double_border_uses_two_centerline_strokes() {
    let mut content = String::new();
    let mut states = Vec::new();
    let mut counter = 0;

    assert!(paint_uniform_border(
        &mut content,
        RoundedRect::new(
            PdfRect::new(0.0, 0.0, 100.0, 50.0),
            CornerRadii::circular(12.0),
        ),
        double(3.0, crate::types::Color::BLACK),
        PdfContentSpace::Points,
        &mut states,
        &mut counter,
    ));

    assert_eq!(content.lines().filter(|line| *line == "S").count(), 2);
    assert!(!content.contains("f*\n"));
}

#[test]
fn elliptical_uniform_double_border_keeps_exact_css_rings() {
    let mut content = String::new();
    let mut states = Vec::new();
    let mut counter = 0;

    assert!(paint_uniform_border(
        &mut content,
        RoundedRect::new(
            PdfRect::new(0.0, 0.0, 100.0, 50.0),
            CornerRadii::uniform(crate::types::CornerRadius::new(18.0, 9.0)),
        ),
        double(3.0, crate::types::Color::BLACK),
        PdfContentSpace::Points,
        &mut states,
        &mut counter,
    ));

    assert_eq!(content.lines().filter(|line| *line == "f*").count(), 2);
    assert!(!content.contains("S\n"));
}

#[test]
fn open_fragment_border_is_one_connected_full_span_fill() {
    let color = crate::types::Color::rgba8(87, 117, 144, 194);
    let border = PhysicalEdges::new(
        solid(1.5, color),
        solid(1.5, color),
        Default::default(),
        solid(1.5, color),
    );
    let mut content = String::new();
    let mut states = Vec::new();
    let mut counter = 0;

    assert!(paint_open_square_solid_border(
        &mut content,
        PdfRect::new(11.25, 0.0, 94.5, 124.5),
        &border,
        CornerRadii::ZERO,
        &mut states,
        &mut counter,
    ));

    assert_eq!(content.lines().filter(|line| *line == "f").count(), 1);
    assert!(content.contains("104.25 0 1.5 124.5 re\n"));
    assert!(content.contains("11.25 0 1.5 124.5 re\n"));
    assert_eq!(states, vec![("GSbd0".to_string(), 194.0 / 255.0)]);
}

#[test]
fn opaque_open_fragment_border_matches_full_span_browser_decomposition() {
    let color = crate::types::Color::rgb(87, 117, 144);
    let border = PhysicalEdges::new(
        solid(1.5, color),
        solid(1.5, color),
        Default::default(),
        solid(1.5, color),
    );
    let mut content = String::new();
    let mut states = Vec::new();
    let mut counter = 0;

    assert!(paint_open_square_solid_border(
        &mut content,
        PdfRect::new(11.25, 0.0, 94.5, 124.5),
        &border,
        CornerRadii::ZERO,
        &mut states,
        &mut counter,
    ));

    assert_eq!(content.lines().filter(|line| *line == "f").count(), 3);
    assert!(content.contains("104.25 0 1.5 124.5 re\n"));
    assert!(content.contains("11.25 0 1.5 124.5 re\n"));
    assert!(states.is_empty());
}

/// The widths of the dashes `paint_square_dashes` draws along the bottom side.
fn bottom_dash_widths(width: f32) -> Vec<f32> {
    let mut content = String::new();
    paint_square_dashes(&mut content, PdfRect::new(0.0, 0.0, 120.0, 60.0), width);
    content
        .lines()
        .filter_map(|line| line.strip_suffix(" re"))
        .map(|rect| {
            rect.split(' ')
                .map(|value| value.parse::<f32>().expect("a number"))
                .collect::<Vec<_>>()
        })
        .filter(|rect| rect[1] == 0.0 && rect[3] == width)
        .map(|rect| rect[2])
        .collect()
}

#[test]
fn a_thin_dashed_border_has_dashes_three_times_its_width() {
    // 2 CSS px: Blink draws 6px dashes with 4px gaps.
    let width = 2.0 * crate::fonts::PT_PER_CSS_PX;
    let dashes = bottom_dash_widths(width);
    assert!(!dashes.is_empty());
    assert!(
        dashes.iter().all(|dash| (dash - 3.0 * width).abs() < 0.01),
        "{dashes:?}"
    );
}

#[test]
fn a_dashed_border_from_three_pixels_has_dashes_twice_its_width() {
    let width = 4.0 * crate::fonts::PT_PER_CSS_PX;
    let dashes = bottom_dash_widths(width);
    assert!(
        dashes.iter().all(|dash| (dash - 2.0 * width).abs() < 0.01),
        "{dashes:?}"
    );
}

#[test]
fn a_short_side_takes_the_dash_count_whose_gap_is_nearest_the_nominal_one() {
    // Blink fits a 20px side of a 2px dashed border with three 6px dashes
    // and 1px gaps (a gap 3px from the nominal 4px), not two dashes and an
    // 8px gap (4px from it), as Chrome draws a bordered span's sides.
    assert_eq!(
        SideDashes::fit(20.0, 6.0, 4.0),
        SideDashes {
            count: 3,
            dash: 6.0,
            gap: 1.0
        }
    );
}

#[test]
fn a_side_no_longer_than_two_dashes_is_solid() {
    assert_eq!(
        SideDashes::fit(12.0, 6.0, 4.0),
        SideDashes {
            count: 1,
            dash: 12.0,
            gap: 0.0
        }
    );
}
