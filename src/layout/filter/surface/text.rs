//! Text and decoration painting into a filter `SourceGraphic`.

use std::collections::HashMap;

use crate::layout::engine::{FlexCell, TextLine, TextRun};
use crate::layout::vertical_align::{AlignedBox, AlignmentBasis, LineExtents};
use crate::parser::ttf::TtfFont;
use crate::style::computed::{AlignItems, TextAlign};
use crate::types::{Color, Point, Size};

use super::canvas::SurfaceRect;
use super::painter::SourcePainter;

/// A line box in the surface: its top, height and baseline.
#[derive(Clone, Copy)]
struct LineSlot {
    top: f32,
    height: f32,
    baseline: f32,
}

#[derive(Clone, Copy)]
struct DecorationSpan {
    start: f32,
    width: f32,
    baseline: f32,
    offset: Point,
}

#[derive(Clone, Copy)]
enum DecorationPhase {
    All,
    BelowText,
    AboveText,
}

impl DecorationPhase {
    const fn paints_below_text(self) -> bool {
        matches!(self, Self::All | Self::BelowText)
    }

    const fn paints_above_text(self) -> bool {
        matches!(self, Self::All | Self::AboveText)
    }
}

impl SourcePainter<'_> {
    pub(super) fn paint_text_lines(
        &mut self,
        lines: &[TextLine],
        content: SurfaceRect,
        alignment: TextAlign,
        indent: f32,
    ) -> Option<()> {
        let mut baseline_cursor = crate::render::blur::RasterBaselineCursor::new(
            content.origin.y,
            self.space.css_pixel_grid_origin.y,
        );
        let mut line_top = content.origin.y;
        for (line_index, line) in lines.iter().enumerate() {
            let baseline_ascent = line_baseline_ascent(line, self.fonts);
            let baseline = baseline_cursor.next(crate::render::blur::RasterBaselineAdvance::new(
                baseline_ascent,
                (line.height - baseline_ascent).max(0.0),
            ));
            let runs = merged_runs(&line.runs);
            let parent_font_size = crate::layout::text::line_primary_font_size(&runs);
            let line_width = runs
                .iter()
                .map(|run| run_width(run, self.fonts))
                .sum::<Option<f32>>()?;
            let line_x = content.origin.x
                + line_start(
                    line,
                    line_index == 0,
                    Some(content.size.width),
                    alignment,
                    indent,
                    line_width,
                );
            self.canvas.include_paint_bounds(SurfaceRect::new(
                Point::new(line_x, line_top),
                Size::new(line_width, line.height),
            ));
            self.paint_text_runs(
                &runs,
                parent_font_size,
                line_x,
                LineSlot {
                    top: line_top,
                    height: line.height,
                    baseline,
                },
            )?;
            line_top += line.height;
        }
        Some(())
    }

    fn paint_text_runs(
        &mut self,
        runs: &[TextRun],
        parent_font_size: f32,
        mut run_x: f32,
        line: LineSlot,
    ) -> Option<()> {
        let baseline = line.baseline;
        for run in runs {
            if let Some(inline) = run.inline_box.as_deref() {
                self.paint_inline_box(run, inline, runs, parent_font_size, run_x, line)?;
                run_x += run.atomic_inline_advance()?;
                continue;
            }
            if run.background_color.is_some() || run.text.is_empty() {
                return None;
            }
            let (_, font) = crate::text::resolve_custom_font(
                &run.font_family,
                run.bold,
                run.font_style.is_slanted(),
                self.fonts,
            )?;
            let mut shaped = crate::text::shape_text_run(run, self.fonts)?;
            apply_authored_spacing(run, &mut shaped.glyphs);
            let run_baseline = baseline - run.glyph_baseline_shift(parent_font_size);
            let run_origin =
                crate::render::blur::GlyphBaselineOrigin::top_down(run_x, run_baseline);
            let raster = crate::render::blur::rasterize_run_alpha(
                crate::render::blur::GlyphRasterRequest {
                    font,
                    font_size: font.adjusted_font_size(run.font_size),
                    glyphs: &shaped.glyphs,
                    style: crate::render::blur::GlyphRasterStyle {
                        embolden: run
                            .synthetic_bold_stroke_width(self.fonts)
                            .unwrap_or_default(),
                        shear: run.synthetic_italic_shear(self.fonts).unwrap_or_default(),
                    },
                    origin: run_origin,
                    dpi: self.filter_dpi,
                },
            )?;
            let advance = run_width(run, self.fonts)?;
            let span = DecorationSpan {
                start: run_x,
                width: advance,
                baseline: run_baseline,
                offset: Point::ORIGIN,
            };
            self.paint_text_shadows(run, &raster, span)?;
            self.paint_run_decorations(run, span, None, DecorationPhase::BelowText)?;
            self.canvas
                .composite_mask(&raster.mask, raster.placement.mask_origin, run.color);
            self.canvas.include_paint_bounds(
                raster.paint_bounds_at(run_origin, self.canvas.pixels_per_point)?,
            );
            self.paint_run_decorations(run, span, None, DecorationPhase::AboveText)?;
            run_x += advance;
        }
        Some(())
    }

    /// An atomic inline box laid out as its box, in its slot on the line (CSS 2.1 §10.8.1). A box approximated from
    /// its text has no content to paint here, so the surface can't be drawn.
    fn paint_inline_box(
        &mut self,
        run: &TextRun,
        inline: &crate::layout::engine::InlineBox,
        runs: &[TextRun],
        parent_font_size: f32,
        run_x: f32,
        line: LineSlot,
    ) -> Option<()> {
        if run.is_inline_edge() {
            return Some(());
        }
        if inline.content.is_empty() {
            return None;
        }
        let border_box = SurfaceRect::new(
            inline_box_origin(run, inline, runs, parent_font_size, run_x, line, self.fonts),
            Size::new(inline.width, inline.height),
        );
        self.canvas.include_paint_bounds(border_box);
        self.paint_inline_box_content(&inline.content, border_box)
    }

    fn paint_text_shadows(
        &mut self,
        run: &TextRun,
        raster: &crate::render::blur::GlyphRaster,
        span: DecorationSpan,
    ) -> Option<()> {
        for shadow in run.text_shadow.iter().rev() {
            if shadow.blur > 0.0 {
                return None;
            }
            let offset = Point::new(shadow.offset_x, shadow.offset_y);
            let shadow_span = DecorationSpan { offset, ..span };
            let shadow_origin = crate::render::blur::GlyphBaselineOrigin::top_down(
                span.start + offset.x,
                span.baseline + offset.y,
            );
            self.paint_run_decorations(run, shadow_span, Some(shadow.color), DecorationPhase::All)?;
            self.canvas.composite_mask(
                &raster.mask,
                raster
                    .placement
                    .mask_origin_at(shadow_origin, self.canvas.pixels_per_point)?,
                shadow.color,
            );
            self.canvas.include_paint_bounds(
                raster.paint_bounds_at(shadow_origin, self.canvas.pixels_per_point)?,
            );
        }
        Some(())
    }

    fn paint_run_decorations(
        &mut self,
        run: &TextRun,
        span: DecorationSpan,
        color_override: Option<Color>,
        phase: DecorationPhase,
    ) -> Option<()> {
        if run.decorations.is_empty() {
            return Some(());
        }
        if run.decorations.iter().any(|decoration| {
            decoration.style != crate::style::computed::TextDecorationStyle::Solid
        }) {
            return None;
        }
        let (leading, trailing) =
            crate::render::text_decoration::whitespace_insets(run, self.fonts);
        let start = span.start + leading + span.offset.x;
        let width = (span.width - leading - trailing).max(0.0);
        for decoration in &run.decorations {
            let color = color_override.unwrap_or_else(|| decoration.resolved_color(run.color));
            let thickness = crate::render::text_decoration::painted_thickness(run, decoration);
            let mut paint_line = |line, center_y: f32| {
                let axis_from_baseline = span.baseline + span.offset.y - center_y;
                let exclusions = crate::render::text_decoration::ink_skip_intervals(
                    run,
                    decoration,
                    line,
                    axis_from_baseline,
                    self.fonts,
                )
                .into_iter()
                .map(|interval| interval.translated(span.start + span.offset.x));
                for segment in crate::render::text_decoration::visible_segments(
                    crate::render::text_decoration::InlineInterval::new(start, start + width),
                    exclusions,
                ) {
                    self.canvas.fill(
                        SurfaceRect::new(
                            Point::new(segment.start, center_y - thickness / 2.0),
                            Size::new(segment.end - segment.start, thickness),
                        ),
                        color,
                    );
                }
            };
            if decoration.lines.underline && phase.paints_below_text() {
                paint_line(
                    crate::render::text_decoration::DecorationLine::Underline,
                    span.baseline
                        + crate::render::text_decoration::underline_distance_from_baseline(
                            run, decoration,
                        )
                        + span.offset.y,
                );
            }
            let (ascender_ratio, _) = crate::fonts::font_metrics_ratios(
                run.css_font_family(),
                run.bold,
                run.font_style.is_slanted(),
                self.fonts,
            );
            if decoration.lines.line_through && phase.paints_above_text() {
                paint_line(
                    crate::render::text_decoration::DecorationLine::LineThrough,
                    span.baseline
                        - crate::render::text_decoration::line_through_axis_above_baseline(
                            run,
                            decoration,
                            ascender_ratio,
                        )
                        + span.offset.y,
                );
            }
            if decoration.lines.overline && phase.paints_below_text() {
                paint_line(
                    crate::render::text_decoration::DecorationLine::Overline,
                    span.baseline
                        - crate::render::text_decoration::overline_axis_above_baseline(
                            run,
                            decoration,
                            ascender_ratio,
                        )
                        + span.offset.y,
                );
            }
        }
        Some(())
    }
}

fn apply_authored_spacing(run: &TextRun, glyphs: &mut [crate::text::ShapedGlyph]) {
    if run.metadata.spacing.letter != 0.0 {
        let spaced_glyphs = glyphs.len().saturating_sub(1);
        for glyph in glyphs.iter_mut().take(spaced_glyphs) {
            glyph.x_advance += run.metadata.spacing.letter;
        }
    }
    if run.metadata.spacing.word != 0.0 {
        for glyph in glyphs {
            if glyph.unicode.as_slice() == [0x0020] {
                glyph.x_advance += run.metadata.spacing.word;
            }
        }
    }
}

pub(crate) fn flex_cell_baseline(cell: &FlexCell, fonts: &HashMap<String, TtfFont>) -> Option<f32> {
    let mut prior = 0.0;
    let last = cell
        .lines
        .iter()
        .filter(|line| line.runs.iter().any(|run| !run.text.is_empty()))
        .inspect(|line| prior += line.height)
        .last();
    let Some(last) = last else {
        return cell
            .nested_elements
            .iter()
            .find_map(|element| element.atomic_inline_baseline())
            .map(|baseline| cell.border.top.width + cell.padding.top + baseline.baseline_offset());
    };
    prior -= last.height;
    Some(cell.border.top.width + cell.padding.top + prior + line_baseline_ascent(last, fonts))
}

pub(super) fn flex_line_max_baseline(
    cells: &[FlexCell],
    alignment: AlignItems,
    fonts: &HashMap<String, TtfFont>,
) -> Option<f32> {
    cells
        .iter()
        .filter(|cell| cell.effective_cross_alignment(alignment) == AlignItems::Baseline)
        .filter_map(|cell| flex_cell_baseline(cell, fonts))
        .reduce(f32::max)
}

pub(crate) fn line_baseline_ascent(line: &TextLine, fonts: &HashMap<String, TtfFont>) -> f32 {
    line.baseline_ascent.unwrap_or_else(|| {
        let (ascent, descent) = line
            .runs
            .iter()
            .filter(|run| run.inline_box.is_none())
            .fold((0.0_f32, 0.0_f32), |(ascent, descent), run| {
                let metrics = crate::fonts::font_metrics_ratios(
                    run.css_font_family(),
                    run.bold,
                    run.font_style.is_slanted(),
                    fonts,
                );
                (
                    ascent.max(metrics.0 * run.font_size),
                    descent.max(metrics.1 * run.font_size),
                )
            });
        ascent + ((line.height - ascent - descent) / 2.0).max(0.0)
    })
}

/// Where a line's runs start, from its block's content-box left edge: moved by `text-align` in a content box
/// `content_width` wide (without one, as `start`), and on the first line by `text-indent`.
fn line_start(
    line: &TextLine,
    first: bool,
    content_width: Option<f32>,
    alignment: TextAlign,
    indent: f32,
    line_width: f32,
) -> f32 {
    let first_indent = if first { indent } else { 0.0 };
    let free = content_width.map_or(0.0, |width| {
        (width - line.float_insets.horizontal() - first_indent - line_width).max(0.0)
    });
    let aligned = match alignment {
        TextAlign::Right => free,
        TextAlign::Center => free / 2.0,
        _ => 0.0,
    };
    first_indent + aligned + line.x_offset + line.float_insets.left
}

/// The top-left of an atomic inline box's border box in its line (CSS 2.1 §10.8.1), where `run_x` is where its run
/// starts.
fn inline_box_origin(
    run: &TextRun,
    inline: &crate::layout::engine::InlineBox,
    runs: &[TextRun],
    parent_font_size: f32,
    run_x: f32,
    line: LineSlot,
    fonts: &HashMap<String, TtfFont>,
) -> Point {
    // As the painter takes it (`run_line_height_for_vertical_align`)
    let factor = if run.line_height_factor.is_finite() {
        run.line_height_factor.max(0.0)
    } else {
        1.2
    };
    let basis = AlignmentBasis {
        parent_font_size,
        parent_x_height: crate::layout::text::line_primary_x_height_ratio(runs, fonts)
            * parent_font_size,
        line_height: run.line_height_font_size() * factor,
    };
    let extents = LineExtents {
        above: line.baseline - line.top,
        below: line.top + line.height - line.baseline,
    };
    let margin_top = line.top + extents.top_of(AlignedBox::of(inline), basis);
    Point::new(
        run_x + inline.margin_left + inline.rel_offset_x,
        margin_top + inline.margin_top + inline.rel_offset_y,
    )
}

/// The border-box top-left of every atomic inline box in `lines`, from the top-left of their block's content box, in
/// the order the lines hold them: where they're painted. `snap_baseline` puts a baseline on the pixel grid the
/// painter puts it on. `None` when a run can't be measured.
pub(crate) fn inline_box_origins(
    lines: &[TextLine],
    content_width: Option<f32>,
    alignment: TextAlign,
    indent: f32,
    fonts: &HashMap<String, TtfFont>,
    snap_baseline: impl Fn(f32) -> f32,
) -> Option<Vec<Point>> {
    let mut origins = Vec::new();
    let mut line_top = 0.0;
    for (line_index, line) in lines.iter().enumerate() {
        let slot = LineSlot {
            top: line_top,
            height: line.height,
            baseline: snap_baseline(line_top + line_baseline_ascent(line, fonts)),
        };
        let runs = merged_runs(&line.runs);
        let parent_font_size = crate::layout::text::line_primary_font_size(&runs);
        let line_width = runs
            .iter()
            .map(|run| run_width(run, fonts))
            .sum::<Option<f32>>()?;
        let mut run_x = line_start(
            line,
            line_index == 0,
            content_width,
            alignment,
            indent,
            line_width,
        );
        for run in &runs {
            if let Some(inline) = run.inline_box.as_deref() {
                origins.push(inline_box_origin(
                    run,
                    inline,
                    &runs,
                    parent_font_size,
                    run_x,
                    slot,
                    fonts,
                ));
            }
            run_x += run_width(run, fonts)?;
        }
        line_top += line.height;
    }
    Some(origins)
}

fn run_width(run: &TextRun, fonts: &HashMap<String, TtfFont>) -> Option<f32> {
    if run.inline_box.is_some() {
        return run.atomic_inline_advance();
    }
    let authored_spacing = || run.metadata.spacing.add_internal_advance(0.0, &run.text);
    crate::text::measure_text_width_with_shaping(
        &run.text,
        run.font_size,
        &run.font_family,
        run.bold,
        run.font_style.is_slanted(),
        run.shaping,
        fonts,
    )
    .map(|width| run.inline_advance(width + authored_spacing()))
    .or_else(|| {
        Some(run.inline_advance(
            crate::fonts::str_width(&run.text, run.font_size, &run.font_family, run.bold)
                + authored_spacing(),
        ))
    })
}

fn merged_runs(runs: &[TextRun]) -> Vec<TextRun> {
    let mut merged: Vec<TextRun> = Vec::new();
    for run in runs {
        if run.inline_box.is_some() {
            merged.push(run.clone());
            continue;
        }
        if run.text.is_empty() {
            continue;
        }
        let compatible = merged.last().is_some_and(|previous| {
            previous.inline_box.is_none()
                && previous.font_size == run.font_size
                && previous.bold == run.bold
                && previous.font_style == run.font_style
                && previous.color == run.color
                && previous.font_family == run.font_family
                && previous.font_synthesis == run.font_synthesis
                && previous.vertical_align == run.vertical_align
                && previous.font_variant_position == run.font_variant_position
                && previous.metadata.spacing == run.metadata.spacing
                && previous.background_color == run.background_color
                && previous.text_shadow.is_empty()
                && run.text_shadow.is_empty()
        });
        if compatible {
            if let Some(previous) = merged.last_mut() {
                previous.text.push_str(&run.text);
            }
        } else {
            merged.push(run.clone());
        }
    }
    merged
}
