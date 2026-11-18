// Dotted, dashed and double text decorations, and decoration thickness, as
// Chromium paints them. The expectations are read from Chrome 156's PDF
// output for 16px, 25px, 40px and 12px text (in CSS px; ironpress works in
// pt).

const PX: f32 = crate::fonts::PT_PER_CSS_PX;
const DECORATION_BASELINE: f32 = 100.0;

fn decorated_run(font_px: f32, style: crate::style::computed::TextDecorationStyle) -> TextRun {
    let mut run = test_text_run("MMMM");
    run.font_size = font_px * PX;
    run.decorations
        .push(crate::style::computed::TextDecoration {
            style,
            ..Default::default()
        });
    run
}

/// The decoration of `run` between `start_px` and `end_px`, on a baseline at
/// `DECORATION_BASELINE` pt.
fn paint_decoration(run: &TextRun, line: DecorationLine, start_px: f32, end_px: f32) -> String {
    let decoration = &run.decorations[0];
    let above_px = match line {
        DecorationLine::Underline => {
            (DECORATION_BASELINE - underline_center_y(run, decoration, DECORATION_BASELINE)) / -PX
        }
        DecorationLine::LineThrough => 6.0,
        DecorationLine::Overline => 15.0,
    };
    paint_decoration_at(run, line, above_px, start_px, end_px)
}

/// The decoration of `run` with its axis `above_px` above the baseline.
fn paint_decoration_at(
    run: &TextRun,
    line: DecorationLine,
    above_px: f32,
    start_px: f32,
    end_px: f32,
) -> String {
    let decoration = &run.decorations[0];
    let axis_y = DECORATION_BASELINE + above_px * PX;
    let mut content = String::new();
    push_decoration_stroke(
        &mut content,
        run,
        decoration,
        DecorationStroke::new(
            (0.0, 0.0, 0.0),
            line,
            start_px * PX,
            end_px * PX,
            axis_y,
            axis_y - DECORATION_BASELINE,
        ),
    );
    content
}

/// Each filled rect or four-sided path as (x, top below the baseline, width,
/// height) in px.
fn rects_px(content: &str) -> Vec<(f32, f32, f32, f32)> {
    let numbers = |text: &str| -> Vec<f32> {
        text.split_whitespace()
            .map(|value| value.parse().expect("a number"))
            .collect()
    };
    let px_box = |x: f32, y: f32, width: f32, height: f32| {
        (
            x / PX,
            (DECORATION_BASELINE - y - height) / PX,
            width / PX,
            height / PX,
        )
    };
    let mut boxes = Vec::new();
    let mut path: Vec<(f32, f32)> = Vec::new();
    for line in content.lines() {
        if let Some(rect) = line.strip_suffix(" re") {
            let [x, y, width, height] = numbers(rect)[..] else {
                panic!("not a rect: {rect}");
            };
            boxes.push(px_box(x, y, width, height));
        } else if let Some(point) = line.strip_suffix(" m").or_else(|| line.strip_suffix(" l")) {
            let [x, y] = numbers(point)[..] else {
                panic!("not a point: {point}");
            };
            path.push((x, y));
        } else if line == "h" && !path.is_empty() {
            let xs = path.iter().map(|point| point.0);
            let ys = path.iter().map(|point| point.1);
            let (left, right) = (xs.clone().fold(f32::MAX, f32::min), xs.fold(f32::MIN, f32::max));
            let (bottom, top) = (ys.clone().fold(f32::MAX, f32::min), ys.fold(f32::MIN, f32::max));
            boxes.push(px_box(left, bottom, right - left, top - bottom));
            path.clear();
        }
    }
    boxes
}

fn assert_close(actual: f32, expected: f32, what: &str) {
    assert!(
        (actual - expected).abs() < 0.01,
        "{what}: expected {expected}, got {actual}"
    );
}

#[test]
fn auto_thickness_is_a_tenth_of_the_font_size_and_solid_lines_round_it_down() {
    use crate::render::text_decoration::painted_thickness;
    use crate::style::computed::TextDecorationStyle::{Dotted, Solid};

    for (font_px, resolved_px, solid_px) in [(16.0, 1.6, 1.0), (25.0, 2.5, 2.0), (40.0, 4.0, 4.0)] {
        let solid = decorated_run(font_px, Solid);
        assert_close(decoration_thickness(&solid, &solid.decorations[0]) / PX, resolved_px, "resolved");
        assert_close(painted_thickness(&solid, &solid.decorations[0]) / PX, solid_px, "solid");
        let dotted = decorated_run(font_px, Dotted);
        assert_close(painted_thickness(&dotted, &dotted.decorations[0]) / PX, resolved_px, "dotted");
    }

    // A length is rounded to whole pixels: 3.5px paints 4px.
    let mut run = decorated_run(16.0, Solid);
    run.decorations[0].thickness = Some(3.5 * PX);
    assert_close(painted_thickness(&run, &run.decorations[0]) / PX, 4.0, "3.5px");

    // The gap below the baseline comes from the resolved thickness: 25px text
    // has a 2.5px line, a 2px gap and a 2px solid line.
    let rects = rects_px(&paint_decoration(&decorated_run(25.0, Solid), DecorationLine::Underline, 10.0, 50.0));
    assert_eq!(rects.len(), 1);
    assert_close(rects[0].1, 2.0, "solid top");
    assert_close(rects[0].3, 2.0, "solid height");
}

#[test]
fn dotted_lines_are_whole_pixel_dots_from_the_lines_start() {
    use crate::style::computed::TextDecorationStyle::Dotted;

    // 16px: 2px dots 1.6px tall, 4px apart from x=10, centred 1px below the
    // baseline, the line's ends truncated to whole pixels.
    let rects = rects_px(&paint_decoration(&decorated_run(16.0, Dotted), DecorationLine::Underline, 10.4, 148.05));
    assert_eq!(rects.len(), 35, "{rects:?}");
    for (index, &(x, top, width, height)) in rects.iter().enumerate() {
        assert_close(x, 10.0 + 4.0 * index as f32, "dot x");
        assert_close(top, 0.2, "dot top");
        assert_close(width, 2.0, "dot width");
        assert_close(height, 1.6, "dot height");
    }

    // 25px: 3px dots 6px apart; an odd unit puts the centre half a pixel off
    // the pixel grid, 3.5px below the baseline. The last dot is cut where the
    // line ends.
    let rects = rects_px(&paint_decoration(&decorated_run(25.0, Dotted), DecorationLine::Underline, 61.09, 147.375));
    assert_eq!(rects.len(), 15, "{rects:?}");
    assert_close(rects[0].0, 61.0, "first dot");
    assert_close(rects[0].1, 3.5 - 1.25, "dot top");
    assert_close(rects[14].0, 145.0, "last dot");
    assert_close(rects[14].2, 2.0, "last dot is cut");

    // 12px: 1px dots, 1.2px tall, centred 1.5px below the baseline.
    let rects = rects_px(&paint_decoration(&decorated_run(12.0, Dotted), DecorationLine::Underline, 10.0, 72.125));
    assert_eq!(rects.len(), 31);
    assert_close(rects[0].1, 1.5 - 0.6, "dot top");
    assert_close(rects[30].0, 70.0, "last dot");
}

#[test]
fn thick_dotted_lines_are_round_dots() {
    use crate::style::computed::TextDecorationStyle::Dotted;

    let content = paint_decoration(&decorated_run(40.0, Dotted), DecorationLine::Underline, 10.0, 148.05);
    assert!(rects_px(&content).is_empty(), "{content}");
    assert!(content.contains(&format!("{} w\n1 J\n", 4.0 * PX)), "{content}");
    // Centres from x=12, a dot's radius in from the start; 4px below the
    // baseline.
    assert!(content.contains(&format!("{} {} m\n", 12.0 * PX, DECORATION_BASELINE - 4.0 * PX)), "{content}");
}

#[test]
fn dashed_lines_stretch_their_gaps_to_end_on_a_dash() {
    use crate::style::computed::TextDecorationStyle::Dashed;

    // 16px: 6px dashes, 4px gaps stretched to 4.154px over 138px.
    let rects = rects_px(&paint_decoration(&decorated_run(16.0, Dashed), DecorationLine::Underline, 10.0, 148.05));
    assert_eq!(rects.len(), 14, "{rects:?}");
    for (index, &(x, top, width, height)) in rects.iter().enumerate() {
        assert_close(x, 10.0 + index as f32 * (6.0 + 54.0 / 13.0), "dash x");
        assert_close(top, 0.2, "dash top");
        assert_close(width, 6.0, "dash width");
        assert_close(height, 1.6, "dash height");
    }

    // 40px: 8px dashes with 4px gaps (two units and one from 3px up),
    // stretched to 3.818px.
    let rects = rects_px(&paint_decoration(&decorated_run(40.0, Dashed), DecorationLine::Underline, 160.77, 298.8));
    assert_eq!(rects.len(), 12, "{rects:?}");
    assert_close(rects[1].0 - rects[0].0, 8.0 + 42.0 / 11.0, "dash pitch");
    assert_close(rects[0].1, 2.0, "dash top");

    // 12px, a short span: 3px dashes with 2px gaps squeezed to 1.5px.
    let rects = rects_px(&paint_decoration(&decorated_run(12.0, Dashed), DecorationLine::Underline, 162.6, 183.3));
    let xs: Vec<f32> = rects.iter().map(|rect| rect.0).collect();
    for (actual, expected) in xs.iter().zip([162.0, 166.5, 171.0, 175.5, 180.0]) {
        assert_close(*actual, expected, "short dashes");
    }
    assert_eq!(xs.len(), 5);
}

#[test]
fn double_lines_are_the_resolved_thickness_and_a_pixel_apart() {
    use crate::style::computed::TextDecorationStyle::Double;

    let run = decorated_run(16.0, Double);
    let tops = |line| {
        rects_px(&paint_decoration(&run, line, 10.0, 50.0))
            .iter()
            .map(|rect| (rect.1, rect.3))
            .collect::<Vec<_>>()
    };
    // 1.6px + 1px, rounded: 3px apart below an underline and above an
    // overline, and 2px (rounded down) below a line-through. Each line is 1px.
    let underline = tops(DecorationLine::Underline);
    assert_close(underline[1].0 - underline[0].0, 3.0, "underline");
    assert_close(underline[0].1, 1.0, "line height");
    let overline = tops(DecorationLine::Overline);
    assert_close(overline[0].0 - overline[1].0, 3.0, "overline");
    let line_through = tops(DecorationLine::LineThrough);
    assert_close(line_through[1].0 - line_through[0].0, 2.0, "line-through");

    let run = decorated_run(40.0, Double);
    let rects = rects_px(&paint_decoration(&run, DecorationLine::Underline, 10.0, 50.0));
    assert_close(rects[1].1 - rects[0].1, 5.0, "40px");
    assert_close(rects[0].3, 4.0, "40px line");
}

#[test]
fn overlines_and_line_throughs_sit_where_chrome_puts_them() {
    use crate::render::text_decoration::{
        line_through_axis_above_baseline, overline_axis_above_baseline,
    };
    use crate::style::computed::TextDecorationStyle::{Dashed, Dotted, Solid};

    // ParitySans: an ascent of 1901/2048 em, 15px at 16px and 37px at 40px.
    let ascent = 1901.0 / 2048.0;
    let above = |run: &TextRun, axis: fn(&TextRun, &_, f32) -> f32| {
        axis(run, &run.decorations[0], ascent) / PX
    };

    // 16px solid: the overline's 1px line is on top of the 15px ascent, and
    // the line-through's top is 5px + 0.8px above the baseline, rounded to 6.
    let run = decorated_run(16.0, Solid);
    assert_close(above(&run, overline_axis_above_baseline), 15.5, "overline");
    assert_close(above(&run, line_through_axis_above_baseline), 5.5, "line-through");

    // 40px: Chrome centres a dotted overline 39px above the baseline and a
    // dashed line-through 13px above it, once painting floors the centre.
    let dotted = decorated_run(40.0, Dotted);
    assert_close(above(&dotted, overline_axis_above_baseline), 39.0, "dotted overline");
    let dashed = decorated_run(40.0, Dashed);
    assert_close(
        above(&dashed, line_through_axis_above_baseline),
        37.0 / 3.0,
        "dashed line-through",
    );
    let content = paint_decoration_at(&dashed, DecorationLine::LineThrough, 37.0 / 3.0, 10.0, 60.0);
    let rects = rects_px(&content);
    assert_close(rects[0].1 + 2.0, -13.0, "painted line-through centre");
}

#[test]
fn a_piece_of_a_dotted_or_dashed_line_draws_only_its_own_marks() {
    use crate::style::computed::TextDecorationStyle::{Dashed, Dotted};

    // Ink skipping paints a line in pieces. Each keeps the whole line's
    // pattern, clipped to the piece, and draws only the marks that reach it.
    let piece = |run: &TextRun, from_px: f32, to_px: f32| {
        let decoration = &run.decorations[0];
        let axis_y = underline_center_y(run, decoration, DECORATION_BASELINE);
        let whole = DecorationStroke::new(
            (0.0, 0.0, 0.0),
            DecorationLine::Underline,
            10.0 * PX,
            148.05 * PX,
            axis_y,
            axis_y - DECORATION_BASELINE,
        );
        let mut content = String::new();
        push_decoration_stroke(
            &mut content,
            run,
            decoration,
            DecorationStroke {
                span: crate::render::text_decoration::InlineInterval::new(from_px * PX, to_px * PX),
                ..whole
            },
        );
        assert!(content.contains("W\nn\n"), "the piece is clipped: {content}");
        // Leave out the clip rectangle, four times the line's thickness tall
        rects_px(&content)
            .into_iter()
            .filter(|rect| rect.3 < 2.0)
            .map(|rect| rect.0)
            .collect::<Vec<_>>()
    };

    // 16px dots 4px apart from x=10: the dot at 38 ends before the piece and
    // the one at 50 starts where it ends.
    let dots = piece(&decorated_run(16.0, Dotted), 40.5, 50.0);
    assert_eq!(dots.len(), 2, "{dots:?}");
    assert_close(dots[0], 42.0, "first dot");
    assert_close(dots[1], 46.0, "second dot");

    // 16px dashes 6px long, 6 + 54/13 px apart from x=10
    let dashes = piece(&decorated_run(16.0, Dashed), 40.5, 60.0);
    let pitch = 6.0 + 54.0 / 13.0;
    assert_eq!(dashes.len(), 2, "{dashes:?}");
    assert_close(dashes[0], 10.0 + 3.0 * pitch, "first dash");
    assert_close(dashes[1], 10.0 + 4.0 * pitch, "second dash");
}

#[test]
fn a_decoration_longer_than_any_page_is_painted_only_near_it() {
    use crate::style::computed::TextDecorationStyle::{Dashed, Dotted, Double, Solid, Wavy};

    // Letter spacing can make a run any length. Past twice the largest PDF
    // page from the page's origin a line can't be seen, so painting stops
    // there rather than taking memory in proportion to the line.
    let reach_px = 2.0 * 14_400.0 / PX;
    for (font_px, style) in [(16.0, Solid), (16.0, Double), (16.0, Dotted), (40.0, Dotted), (16.0, Dashed), (16.0, Wavy)] {
        for (start_px, end_px) in [(0.0, 1e30), (-1e30, 1e30), (1e30, 2e30)] {
            let content =
                paint_decoration(&decorated_run(font_px, style), DecorationLine::Underline, start_px, end_px);
            assert!(content.len() < 2_000_000, "{style:?} from {start_px}: {} bytes", content.len());
            // Positions within the reach, and lengths up to twice it
            for number in content.split_whitespace().filter_map(|word| word.parse::<f32>().ok()) {
                assert!(
                    number.is_finite() && number.abs() <= 2.0 * reach_px * PX + 100.0,
                    "{style:?} from {start_px}: {number}"
                );
            }
        }
    }
}
