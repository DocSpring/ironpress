use super::*;

pub(in crate::render::pdf) use crate::render::text_decoration::DecorationLine;
pub(in crate::render::pdf) use crate::render::text_decoration::thickness as decoration_thickness;

pub(in crate::render::pdf) fn underline_center_y(
    run: &TextRun,
    decoration: &crate::style::computed::TextDecoration,
    baseline_y: f32,
) -> f32 {
    // `text-underline-offset` is measured from the underline-position zero
    // point. For the supported horizontal `auto` position that point is the
    // alphabetic baseline; the line thickness extends outward from there.
    // Blink's automatic near-edge gap is half the resolved stroke width,
    // rounded outward to its CSS-pixel grid. An authored length (including
    // zero or a negative length) remains exact.
    baseline_y - crate::render::text_decoration::underline_distance_from_baseline(run, decoration)
}

#[cfg(test)]
pub(in crate::render::pdf) fn decoration_is_wavy(
    decoration: &crate::style::computed::TextDecoration,
) -> bool {
    decoration.style == crate::style::computed::TextDecorationStyle::Wavy
}

impl DecorationLine {
    fn wavy_axis_offset(self, thickness: f32) -> f32 {
        let offset = thickness + crate::fonts::PT_PER_CSS_PX;
        match self {
            // PDF coordinates increase upward, the inverse of CSS's block
            // direction. Wavy underlines need one decoration gap below their
            // solid-line axis, and overlines need the symmetric adjustment.
            Self::Underline => -offset,
            Self::LineThrough => 0.0,
            Self::Overline => offset,
        }
    }
}

/// Blink-compatible dimensions for a wavy text decoration. Its geometry is
/// based on the resolved decoration thickness, with a two-CSS-pixel minimum,
/// rather than the surrounding font size.
#[derive(Clone, Copy)]
struct WavyDecorationMetrics {
    step: f32,
    control_distance: f32,
}

impl WavyDecorationMetrics {
    fn from_thickness(thickness: f32) -> Self {
        let unit = thickness.max(2.0 * crate::fonts::PT_PER_CSS_PX);
        Self {
            step: unit * 2.5,
            control_distance: unit * 3.5,
        }
    }
}

#[derive(Clone, Copy)]
pub(in crate::render::pdf) struct DecorationStroke {
    pub(in crate::render::pdf) color: PdfRgb,
    pub(in crate::render::pdf) line: DecorationLine,
    pub(in crate::render::pdf) span: crate::render::text_decoration::InlineInterval,
    /// The whole line a dotted or dashed pattern is laid out on. `span` is
    /// the part of it painted: the pattern keeps its phase across the gaps
    /// ink skipping cuts.
    pub(in crate::render::pdf) pattern: crate::render::text_decoration::InlineInterval,
    pub(in crate::render::pdf) axis_y: f32,
    pub(in crate::render::pdf) axis_from_baseline: f32,
}

impl DecorationStroke {
    pub(in crate::render::pdf) fn new(
        color: (f32, f32, f32),
        line: DecorationLine,
        start: f32,
        end: f32,
        axis_y: f32,
        axis_from_baseline: f32,
    ) -> Self {
        Self {
            color: PdfRgb::from(color),
            line,
            span: crate::render::text_decoration::InlineInterval::new(start, end),
            pattern: crate::render::text_decoration::InlineInterval::new(start, end),
            axis_y,
            axis_from_baseline,
        }
    }
}

pub(in crate::render::pdf) fn push_decoration_stroke(
    content: &mut String,
    run: &TextRun,
    decoration: &crate::style::computed::TextDecoration,
    stroke: DecorationStroke,
) {
    let thickness = decoration_thickness(run, decoration);
    if stroke.span.end <= stroke.span.start {
        return;
    }
    let painted = crate::render::text_decoration::painted_thickness(run, decoration);
    match decoration.style {
        crate::style::computed::TextDecorationStyle::Solid => {
            push_solid_line(content, stroke, stroke.axis_y, painted);
            return;
        }
        crate::style::computed::TextDecorationStyle::Double => {
            // Blink draws the second line the resolved thickness and a pixel
            // away (`TextDecorationInfo::SetLineData`), snapped to whole
            // pixels: below an underline, above an overline, and below a
            // line-through by the offset rounded down.
            let px = crate::fonts::PT_PER_CSS_PX;
            let offset = thickness / px + 1.0;
            let second = match stroke.line {
                DecorationLine::Underline => stroke.axis_y - offset.round() * px,
                DecorationLine::Overline => stroke.axis_y + offset.round() * px,
                DecorationLine::LineThrough => stroke.axis_y - offset.floor() * px,
            };
            push_solid_line(content, stroke, stroke.axis_y, painted);
            push_solid_line(content, stroke, second, painted);
            return;
        }
        crate::style::computed::TextDecorationStyle::Dotted
        | crate::style::computed::TextDecorationStyle::Dashed => {
            push_dashed_line(content, decoration.style, stroke, thickness);
            return;
        }
        crate::style::computed::TextDecorationStyle::Wavy => {}
    }

    let stroke_width = thickness;
    let metrics = WavyDecorationMetrics::from_thickness(stroke_width);
    let axis_y = stroke.axis_y + stroke.line.wavy_axis_offset(stroke_width);
    let clip_y = axis_y - metrics.control_distance - stroke_width * 2.0;
    let clip_h = (metrics.control_distance + stroke_width * 2.0) * 2.0;
    let mut x = stroke.span.start - 2.0 * metrics.step;
    let end_x = stroke.span.end + 4.0 * metrics.step;
    let mut path = format!("{x} {axis_y} m\n");
    while x + 2.0 * metrics.step <= end_x {
        let cx = x + metrics.step;
        x += 2.0 * metrics.step;
        path.push_str(&format!(
            "{cx} {} {cx} {} {x} {axis_y} c\n",
            axis_y - metrics.control_distance,
            axis_y + metrics.control_distance
        ));
    }
    content.push_str(&format!(
        "q\n{} {clip_y} {} {clip_h} re\nW\nn\n",
        stroke.span.start,
        stroke.span.end - stroke.span.start
    ));
    content.push_str(&stroke.color.stroke_operator());
    content.push_str(&format!("{stroke_width} w\n0 J\n1 j\n{path}S\nQ\n"));
}

fn push_solid_line(content: &mut String, stroke: DecorationStroke, axis_y: f32, thickness: f32) {
    let rect = PdfRect::new(
        stroke.span.start,
        axis_y - thickness / 2.0,
        stroke.span.end - stroke.span.start,
        thickness,
    );
    content.push_str(&stroke.color.fill_operator());
    content.push_str(&rect.rect_path());
    content.push_str("f\n");
}

/// A dotted or dashed decoration as Chromium paints it
/// (`GraphicsContext::DrawLine` with `StrokeData`'s dash pattern). The line
/// runs between whole CSS pixels, truncating its ends, and is as thick as the
/// resolved thickness. Its pattern unit is that thickness rounded to whole
/// pixels: dots one unit long a unit apart, dashes three units with two-unit
/// gaps (two units and one from 3px up), with the dash gaps stretched so the
/// line starts and ends on a dash. Dots from 4px up are round. The line's
/// centre is on a whole pixel, or half a pixel off one when the unit is odd.
fn push_dashed_line(
    content: &mut String,
    style: crate::style::computed::TextDecorationStyle,
    stroke: DecorationStroke,
    thickness: f32,
) {
    // Float error in lengths summed in pt mustn't move a pixel boundary.
    const SNAP: f32 = 1e-3;
    let px = crate::fonts::PT_PER_CSS_PX;
    let unit = (thickness / px).round().max(1.0);
    let start = (stroke.pattern.start / px + SNAP).floor();
    let end = (stroke.pattern.end / px + SNAP).floor();
    let length = end - start;
    if length <= 0.0 {
        return;
    }
    let baseline = stroke.axis_y - stroke.axis_from_baseline;
    let odd = if unit % 2.0 == 1.0 { 0.5 } else { 0.0 };
    let below = (-stroke.axis_from_baseline / px + SNAP).floor() + odd;
    let center = baseline - below * px;
    // Ink skipping paints part of the line: clip the whole line's pattern to
    // it, as Blink clips out the gaps.
    let clipped = stroke.span != stroke.pattern;
    if clipped {
        content.push_str(&format!(
            "q\n{}W\nn\n",
            PdfRect::new(
                stroke.span.start,
                center - thickness * 2.0,
                stroke.span.end - stroke.span.start,
                thickness * 4.0,
            )
            .rect_path()
        ));
    }
    let line = DashedLine {
        start,
        length,
        unit,
        center,
        thickness,
    };
    push_dash_pattern(content, style, stroke, line);
    if clipped {
        content.push_str("Q\n");
    }
}

/// A dotted or dashed line on the pixel grid: where it starts and how long it
/// is in whole CSS px, its pattern unit in px, and its centre and thickness in
/// pt.
#[derive(Clone, Copy)]
struct DashedLine {
    start: f32,
    length: f32,
    unit: f32,
    center: f32,
    thickness: f32,
}

/// The dots or dashes of `line`.
fn push_dash_pattern(
    content: &mut String,
    style: crate::style::computed::TextDecorationStyle,
    stroke: DecorationStroke,
    line: DashedLine,
) {
    let DashedLine {
        start,
        length,
        unit,
        center,
        thickness,
    } = line;
    let px = crate::fonts::PT_PER_CSS_PX;
    let end = start + length;
    let bottom = center - thickness / 2.0;
    content.push_str(&stroke.color.fill_operator());

    if style == crate::style::computed::TextDecorationStyle::Dotted {
        if unit > 3.0 {
            push_round_dots(content, stroke, line);
            return;
        }
        // Square dots, a dot apart from the line's start, the last one cut
        // where the line ends. Each is filled on its own, as Chromium does,
        // so a rasterizer can snap a lone rectangle to its pixels.
        let mut x = start;
        while x < end {
            let width = unit.min(end - x);
            content.push_str(&PdfRect::new(x * px, bottom, width * px, thickness).rect_path());
            content.push_str("f\n");
            x += unit * 2.0;
        }
        return;
    }

    let (dash, gap) = if unit >= 3.0 {
        (unit * 2.0, unit)
    } else {
        (unit * 3.0, unit * 2.0)
    };
    if length <= dash * 2.0 {
        let whole = DecorationStroke {
            span: stroke.pattern,
            ..stroke
        };
        push_solid_line(content, whole, center, thickness);
        return;
    }
    let gap = best_dash_gap(length, dash, gap);
    let count = ((length + gap) / (dash + gap)).round() as usize;
    for index in 0..count {
        let x = start + index as f32 * (dash + gap);
        // Chromium draws the dashes as one path of four-sided subpaths, which
        // rasterizers antialias rather than snapping to pixels as they do a
        // lone rectangle.
        let (left, right, top) = (x * px, (x + dash) * px, bottom + thickness);
        content.push_str(&format!(
            "{left} {bottom} m\n{right} {bottom} l\n{right} {top} l\n{left} {top} l\nh\n"
        ));
    }
    content.push_str("f\n");
}

/// Dots 4px and up are circles of the line's thickness, spaced as dashes a
/// unit long with stretched gaps, the first and last touching the line's ends.
fn push_round_dots(content: &mut String, stroke: DecorationStroke, line: DashedLine) {
    let DashedLine {
        start,
        length,
        unit,
        center,
        thickness,
    } = line;
    let px = crate::fonts::PT_PER_CSS_PX;
    if length < unit {
        return;
    }
    let gap = if length > unit * 2.0 {
        best_dash_gap(length, unit, unit)
    } else {
        length - unit
    };
    let pitch = unit + gap;
    let count = ((length + gap) / pitch).round().max(1.0) as usize;
    let first = start + unit / 2.0;
    let last = first + (count - 1) as f32 * pitch;
    content.push_str(&format!(
        "q\n{}{thickness} w\n1 J\n[0 {}] 0 d\n{} {center} m\n{} {center} l\nS\nQ\n",
        stroke.color.stroke_operator(),
        pitch * px,
        first * px,
        // A hair past the last dot, so float error can't drop it.
        last * px + 0.01,
    ));
}

/// Blink's `StrokeData::SelectBestDashGap` for an open line: the gap nearest
/// `gap` that fits a whole number of dashes with one at each end.
fn best_dash_gap(length: f32, dash: f32, gap: f32) -> f32 {
    let fewer = ((length + gap) / (dash + gap)).floor();
    let more = fewer + 1.0;
    let fewer_gap = (length - fewer * dash) / (fewer - 1.0);
    let more_gap = (length - more * dash) / (more - 1.0);
    if more_gap <= 0.0 || (fewer_gap - gap).abs() < (more_gap - gap).abs() {
        fewer_gap
    } else {
        more_gap
    }
}
