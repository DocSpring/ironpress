use super::*;

/// Cross-axis geometry of a vertical line.
///
/// `over_edge` is the right edge in `vertical-rl` and the left edge in
/// `vertical-lr`. Keeping its direction alongside the coordinate prevents a
/// fallback glyph's font-derived central baseline from being reflected twice.
#[derive(Clone, Copy)]
pub(super) struct VerticalLineCrossAxis {
    over_edge: f32,
    advances_right: bool,
}

impl VerticalLineCrossAxis {
    pub(super) const fn from_content_edges(
        content_left: f32,
        content_right: f32,
        vertical_lr: bool,
    ) -> Self {
        Self {
            over_edge: if vertical_lr {
                content_left
            } else {
                content_right
            },
            advances_right: vertical_lr,
        }
    }

    fn central_baseline_x(self, distance_from_over: f32) -> f32 {
        if self.advances_right {
            self.over_edge + distance_from_over
        } else {
            self.over_edge - distance_from_over
        }
    }
}

/// The origins used by upright vertical text.
///
/// Horizontal `text-combine-upright` occupies a physical one-em square whose
/// left edge is `composition_origin.x`. Ordinary glyphs instead use the
/// font's vertical baseline. Keeping both named avoids treating a horizontal
/// composition origin as a vertical one.
#[derive(Clone, Copy)]
pub(super) struct UprightLinePosition {
    composition_origin: PdfPoint,
    cross_axis: VerticalLineCrossAxis,
}

impl UprightLinePosition {
    pub(super) const fn new(
        composition_origin: PdfPoint,
        cross_axis: VerticalLineCrossAxis,
    ) -> Self {
        Self {
            composition_origin,
            cross_axis,
        }
    }

    fn vertical_baseline_x(self, run: &TextRun, custom_fonts: &HashMap<String, TtfFont>) -> f32 {
        let fallback_distance = run.font_size.max(0.0) * 0.5;
        let distance_from_over = crate::text::contains_cjk_vertical_text(&run.text)
            .then(|| crate::text::upright_vertical_font_metrics(run, custom_fonts))
            .flatten()
            .map_or(fallback_distance, |metrics| {
                metrics.central_baseline_from_over_ratio() * run.font_size.max(0.0)
            });
        self.cross_axis.central_baseline_x(distance_from_over)
    }
}

/// Paint an atomic inline box (`display: inline-block`) inside a line of text.
///
/// `box_x` is the left edge of the box in PDF coordinates; `baseline_y` is the
/// text baseline of the enclosing line; `line_top_y`/`line_bottom_y` bound the
/// line box. The box is positioned vertically per its `vertical_align`, then its
/// background, border, and pre-wrapped inner text are drawn.
/// The edges an atomic inline box aligns to in one line (CSS 2.1 §10.8.1): the line box's top and bottom, the parent
/// text content area's (the line box's when the line has no parent text), and the parent font's x-height ratio.
#[derive(Debug, Clone, Copy)]
pub(super) struct InlineBoxLine {
    pub(super) top: f32,
    pub(super) bottom: f32,
    pub(super) text_top: f32,
    pub(super) text_bottom: f32,
    pub(super) x_height_ratio: f32,
}

impl InlineBoxLine {
    pub(super) fn new(
        line: &crate::layout::engine::TextLine,
        merged: &[crate::layout::engine::TextRun],
        baseline_y: f32,
        metrics: LineBoxMetrics,
        custom_fonts: &HashMap<String, TtfFont>,
    ) -> Self {
        let top = baseline_y + metrics.ascender + metrics.half_leading;
        let bottom = baseline_y - metrics.descender - metrics.half_leading;
        let (text_ascent, text_descent) = line_text_content_extents(line, custom_fonts);
        Self {
            top,
            bottom,
            text_top: if text_ascent > 0.0 {
                baseline_y + text_ascent
            } else {
                top
            },
            text_bottom: if text_descent > 0.0 {
                baseline_y - text_descent
            } else {
                bottom
            },
            x_height_ratio: line_primary_x_height_ratio(merged, custom_fonts),
        }
    }
}

/// Where an atomic inline box goes in its line: `x` is its border box's left edge (PDF), `baseline` the line's
/// baseline, `line` the edges it can align to, and `font_size` and `line_height` its run's, for `vertical-align`.
#[derive(Debug, Clone, Copy)]
pub(super) struct InlineBoxSlot {
    pub(super) x: f32,
    pub(super) baseline: f32,
    pub(super) line: InlineBoxLine,
    pub(super) font_size: f32,
    pub(super) line_height: f32,
}

/// Paints a line's atomic inline box (`render_inline_box`) with the annotation of the link its run is in
pub(super) fn render_run_inline_box(
    content: &mut String,
    run: &TextRun,
    inline: &crate::layout::engine::InlineBox,
    slot: InlineBoxSlot,
    ctx: &mut PageRenderContext<'_>,
) {
    let border_box = render_inline_box(content, inline, slot, ctx);
    if let Some(annotation) = inline_box_link_annotation(run, border_box) {
        ctx.text.annotations.push(annotation);
    }
}

/// Paints an atomic inline box in its line and returns its border box where it's painted on the page, through its own
/// and its ancestors' transforms: where a link around it puts its annotation (`inline_box_link_annotation`)
pub(super) fn render_inline_box(
    content: &mut String,
    inline: &crate::layout::engine::InlineBox,
    slot: InlineBoxSlot,
    ctx: &mut PageRenderContext<'_>,
) -> PdfRect {
    let InlineBoxSlot {
        x: box_x,
        baseline: baseline_y,
        line,
        font_size: line_font_size,
        line_height,
    } = slot;
    let (line_top_y, line_bottom_y, line_text_top_y, line_text_bottom_y, parent_x_height_ratio) = (
        line.top,
        line.bottom,
        line.text_top,
        line.text_bottom,
        line.x_height_ratio,
    );
    // The box is aligned by its margin box (CSS 2.1 §10.8). Boxes placed against the baseline reach `above` it, as
    // the line box's metrics placed them; the others sit at the line box's or the parent's text edges.
    let aligned = crate::layout::vertical_align::AlignedBox::of(inline);
    let h = aligned.height;
    let basis = crate::layout::vertical_align::AlignmentBasis {
        parent_font_size: line_font_size,
        parent_x_height: line_font_size * parent_x_height_ratio,
        line_height,
    };
    // Bottom edge of the box (PDF, y-up)
    let box_bottom = match (inline.vertical_align, aligned.baseline_extents(basis)) {
        (_, Some((above, _))) => baseline_y + above - h,
        (VerticalAlign::Top, None) => line_top_y - h,
        (VerticalAlign::Bottom, None) => line_bottom_y,
        // text-top and text-bottom: the parent's text content area (CSS 2.1 §10.8.1)
        (VerticalAlign::TextTop, None) => line_text_top_y - h,
        (_, None) => line_text_bottom_y,
    };

    // CSS `position: relative` shifts the painted box (and its inner content)
    // without changing its in-flow slot: x right, y down (PDF y is up, so the
    // downward shift subtracts from y).
    let box_x = box_x + inline.rel_offset_x;
    let box_bottom = box_bottom + inline.margin_bottom - inline.rel_offset_y;
    let geometry = LayoutBoxGeometry::from_layout(
        PdfRect::new(box_x, box_bottom, inline.width, inline.height),
        &inline.paint.border,
        inline.padding,
        inline.paint.border_image.as_ref(),
    );
    // The box and everything in it are one paint unit, snapped from its own position in the line
    let prior_grid = ctx
        .text
        .pdf_writer
        .enter_atomic_inline_paint_grid(geometry.border_box.top_left());
    let box_geometry = ctx.text.pdf_writer.resolve_box_geometry(geometry);
    let paint_geometry = box_geometry.painting();
    let fragment_geometry = box_geometry.fragment(Default::default());
    let background_box =
        paint_geometry.background_clip_box(BackgroundClip::Border, inline.paint.border_radii);

    // Background fill.
    if let Some(background) = inline.paint.background_color {
        paint_solid_background(
            content,
            background,
            background_box,
            ctx.page_ext_gstates,
            ctx.bg_alpha_counter,
        );
    }
    if let Some(fill) = inline.paint.gauge_fill {
        let track = background_box.rect;
        let filled = PdfRect::new(
            track.left,
            track.bottom,
            track.width * fill.fraction,
            track.height,
        );
        paint_solid_background(
            content,
            fill.color,
            RoundedRect::new(filled, CornerRadii::default()),
            ctx.page_ext_gstates,
            ctx.bg_alpha_counter,
        );
    }

    // A replaced image: an `<img>` in a line, or a pseudo-element's
    // `content: url(...)`
    if let Some(image) = &inline.image {
        super::page_elements::paint_replaced_raster(
            content,
            image,
            paint_geometry.content_box(),
            inline.image_sampling,
            ctx.text.pdf_writer,
            ctx.text.page_images,
        );
    }

    if let Some(svg) = &inline.svg {
        super::svg_drawing::draw_svg_in_box(
            content,
            &svg.tree,
            svg.replaced,
            paint_geometry.content_box(),
            super::svg_drawing::SvgDrawResources {
                shadings: ctx.shadings,
                shading_counter: ctx.shading_counter,
                ext_gstates: ctx.page_ext_gstates,
                pdf_writer: ctx.text.pdf_writer,
                page_images: ctx.text.page_images,
                custom_fonts: ctx.text.custom_fonts,
                prepared_custom_fonts: ctx.text.prepared_custom_fonts,
            },
        );
    }

    // Border (drawn inside the border box, matching border-box sizing).
    paint_box_decoration(
        content,
        fragment_geometry,
        &inline.paint.border,
        inline.paint.border_radii,
        inline.paint.border_image.as_ref(),
        BorderPaintResources {
            shadings: ctx.shadings,
            shading_counter: ctx.shading_counter,
            page_ext_gstates: ctx.page_ext_gstates,
            alpha_counter: ctx.bg_alpha_counter,
            pdf_writer: ctx.text.pdf_writer,
            page_images: ctx.text.page_images,
        },
    );
    if let Some(stroke) = inline.paint.centered_stroke {
        paint_centered_inline_stroke(
            content,
            paint_geometry.rounded_border_box(inline.paint.border_radii),
            stroke,
            ctx.page_ext_gstates,
            ctx.bg_alpha_counter,
        );
    }

    if !inline.content.is_empty() {
        let top = box_bottom + inline.height;
        let origin = PdfPoint::new(box_x, top);
        render_container_children(
            content,
            &inline.content,
            ContainerFrame::new(
                origin,
                crate::types::Size::new(inline.width, inline.height),
                origin,
            ),
            &mut HashMap::new(),
            ctx,
            ContainerRenderOptions {
                // Painted as one unit, as an inline-block is, with its positioned descendants and those that make a
                // stacking context in its parent's stacking context (CSS 2.1 Appendix E)
                stacking_scope: StackingScope::Ancestor,
                ..Default::default()
            },
        );
    }

    // Inner text lines, laid out from the content-box top downward.
    let content_box = geometry.content_box();
    let mut baseline_cursor = ctx.text.pdf_writer.text_baseline_cursor(content_box.top());
    for line in &inline.lines {
        let metrics = line_box_metrics(line, ctx.text.custom_fonts);
        let inner_y = baseline_cursor.next_horizontal(metrics);
        let merged = crate::text::coalesce_text_runs(&line.runs);
        let parent_font_size = crate::layout::text::line_primary_font_size(&merged);
        let line_ascender = line_text_top(line, ctx.text.custom_fonts);
        // Line-box and parent text content-area edges for the vertical
        // alignment of nested atomic inline boxes, as in a block's lines.
        let line_top_y = inner_y + metrics.ascender + metrics.half_leading;
        let line_bottom_y = inner_y - metrics.descender - metrics.half_leading;
        let (text_ascent, text_descent) = line_text_content_extents(line, ctx.text.custom_fonts);
        let line_text_top_y = if text_ascent > 0.0 {
            inner_y + text_ascent
        } else {
            line_top_y
        };
        let line_text_bottom_y = if text_descent > 0.0 {
            inner_y - text_descent
        } else {
            line_bottom_y
        };
        let mut x = content_box.left;
        for (run_index, run) in merged.iter().enumerate() {
            if let Some(advance) = run.atomic_inline_advance() {
                // An inline-block nested in this one paints in its slot.
                if let Some(nested) = run.inline_box.as_deref() {
                    if !run.is_inline_edge() {
                        render_run_inline_box(
                            content,
                            run,
                            nested,
                            InlineBoxSlot {
                                x: x + nested.margin_left,
                                baseline: inner_y,
                                line: InlineBoxLine {
                                    top: line_top_y,
                                    bottom: line_bottom_y,
                                    text_top: line_text_top_y,
                                    text_bottom: line_text_bottom_y,
                                    x_height_ratio: line_primary_x_height_ratio(
                                        &merged,
                                        ctx.text.custom_fonts,
                                    ),
                                },
                                font_size: run.font_size,
                                line_height: run_line_height_for_vertical_align(run),
                            },
                            ctx,
                        );
                    }
                }
                x += advance;
                continue;
            }
            if run.text.is_empty() {
                continue;
            }
            let run_y =
                inner_y + drop_cap_baseline_shift(run, line_ascender, ctx.text.custom_fonts);
            let run_width = estimate_run_width_with_fonts(run, ctx.text.custom_fonts);
            let previous = merged[..run_index]
                .iter()
                .rev()
                .find(|previous| previous.inline_box.is_none() && !previous.text.is_empty());
            let decoration =
                HorizontalRunDecorations::new(run, x, run_width, run_y, ctx.text.custom_fonts)
                    .continuing_after(previous);
            x += decoration.paint_text(
                content,
                parent_font_size,
                ctx.text.prepared_custom_fonts,
                0.0,
                ctx.text.pdf_writer,
                ctx.text.page_images,
            );
        }
    }
    if let Some(prior) = prior_grid {
        ctx.text.pdf_writer.restore_box_paint_grid(prior);
    }
    // The box's own transform is its content's (the box laid out as a block), which paints it transformed
    let own_transform = inline
        .content
        .first()
        .and_then(|node| node.paint_group_owner())
        .and_then(|owner| resolved_group_transform(owner.paint_group(), fragment_geometry))
        .map(|transform| transform.matrix());
    ctx.text
        .pdf_writer
        .painted_rect_within(paint_geometry.border_box, own_transform)
}

fn paint_centered_inline_stroke(
    content: &mut String,
    contour: RoundedRect,
    stroke: crate::layout::engine::CenteredStroke,
    page_ext_gstates: &mut Vec<(String, f32)>,
    alpha_counter: &mut usize,
) {
    let side = stroke.side();
    let alpha = begin_border_alpha(content, page_ext_gstates, alpha_counter, side.color.alpha());
    content.push_str(&PdfRgb::from(side.color).stroke_operator());
    content.push_str("0 J\n0 j\n4 M\n");
    content.push_str(&format!("{} w\n", side.width));
    content.push_str(&contour.path_or_rect());
    content.push_str("S\n");
    end_border_alpha(content, alpha);
}

#[allow(clippy::too_many_arguments)]
pub(super) fn render_line_glyphs_without_shadows_in_space(
    content: &mut String,
    runs: &[TextRun],
    start_x: f32,
    y: f32,
    custom_fonts: &HashMap<String, TtfFont>,
    prepared_custom_fonts: &PreparedCustomFonts,
    word_spacing: f32,
    line_ascender: f32,
    pdf_writer: &mut PdfWriter,
    page_images: &mut Vec<ImageRef>,
    text_space: PdfContentSpace,
) {
    // Keep text runs plus any atomic inline boxes (empty text but real advance).
    let non_empty: Vec<&TextRun> = runs
        .iter()
        .filter(|r| !r.text.is_empty() || r.inline_box.is_some())
        .collect();
    if non_empty.is_empty() {
        return;
    }

    // A `vertical-align: super`/`sub` run is shifted by a fraction of the parent
    // (surrounding) font size; resolve it once for the whole line.
    let parent_font_size = crate::layout::text::line_primary_font_size(runs);

    // Every font family, inline box, and effect advances through the same
    // explicit run path. Besides keeping fallback and custom-font behavior
    // homogeneous, this avoids PDF `Tc` adding an unowned trailing advance
    // when a standard-font run ends.
    let mut x = start_x;
    for run in &non_empty {
        // Inline boxes are painted in Phase 1; here they only advance.
        if let Some(advance) = run.atomic_inline_advance() {
            x += advance;
            continue;
        }
        // A floated `::first-letter` drop cap is lowered so its glyph top
        // sits on the line's text top (css-pseudo-4 §2.2).
        let run_y = y + drop_cap_baseline_shift(run, line_ascender, custom_fonts);
        x += render_run_glyph_layers_in_space(
            content,
            run,
            x,
            run_y,
            parent_font_size,
            custom_fonts,
            prepared_custom_fonts,
            word_spacing,
            pdf_writer,
            page_images,
            text_space,
            TextShadowPaint::Skip,
        );
    }
}

/// Render an upright vertical line. A retained `text-combine-upright` marker
/// denotes one atomic horizontal composition; all other runs use the ordinary
/// text path unchanged.
#[allow(clippy::too_many_arguments)]
pub(super) fn render_upright_vertical_line_text(
    content: &mut String,
    runs: &[TextRun],
    position: UprightLinePosition,
    custom_fonts: &HashMap<String, TtfFont>,
    prepared_custom_fonts: &PreparedCustomFonts,
    word_spacing: f32,
    line_ascender: f32,
    pdf_writer: &mut PdfWriter,
    page_images: &mut Vec<ImageRef>,
) {
    let parent_font_size = crate::layout::text::line_primary_font_size(runs);
    let mut x = position.composition_origin.x;
    for run in runs
        .iter()
        .filter(|run| !run.text.is_empty() || run.inline_box.is_some())
    {
        if let Some(advance) = run.atomic_inline_advance() {
            x += advance;
            continue;
        }
        let run_y = position.composition_origin.y
            + drop_cap_baseline_shift(run, line_ascender, custom_fonts);
        let advance = if run.metadata.text_combine_upright.is_active() {
            render_text_combine_run(
                content,
                run,
                x,
                run_y,
                parent_font_size,
                custom_fonts,
                prepared_custom_fonts,
                word_spacing,
                pdf_writer,
                page_images,
            )
        } else {
            render_upright_vertical_run(
                content,
                run,
                position.vertical_baseline_x(run, custom_fonts),
                run_y,
                parent_font_size,
                custom_fonts,
                prepared_custom_fonts,
            )
            .unwrap_or_else(|| {
                render_run_glyphs(
                    content,
                    run,
                    x,
                    run_y,
                    parent_font_size,
                    custom_fonts,
                    prepared_custom_fonts,
                    word_spacing,
                    pdf_writer,
                    page_images,
                )
            })
        };
        x += advance;
    }
}

/// Paint a non-combined upright run with the font's vertical origin metrics.
///
/// The legacy horizontal path remains the safe fallback for effects it owns
/// (shadows and synthetic faces). Basic upright CJK uses this path, which is
/// enough to preserve the actual `vmtx`/`VORG` origin instead of centring by a
/// line-height heuristic.
fn render_upright_vertical_run(
    content: &mut String,
    run: &TextRun,
    vertical_baseline_x: f32,
    horizontal_baseline_y: f32,
    parent_font_size: f32,
    custom_fonts: &HashMap<String, TtfFont>,
    prepared_custom_fonts: &PreparedCustomFonts,
) -> Option<f32> {
    if !crate::text::contains_cjk_vertical_text(&run.text)
        || !run.text_shadow.is_empty()
        || run.synthetic_bold_stroke_width(custom_fonts).is_some()
        || (matches!(run.font_family, FontFamily::Custom(_))
            && crate::system_fonts::needs_faux_italic(
                custom_fonts,
                run.font_family.name(),
                run.bold,
                run.font_style.is_slanted(),
            ))
    {
        return None;
    }

    let shaped = crate::text::shape_upright_vertical_run(run, custom_fonts)?;
    let vertical_origin_offset = shaped.shaped.glyphs.first()?.y_offset;
    let glyph_baseline_y = horizontal_baseline_y
        + run_vertical_align_shift(run, parent_font_size)
        + text_emphasis_baseline_shift(run);
    // `horizontal_baseline_y` is the old renderer's physical glyph baseline.
    // The TTB shaper reports the glyph origin relative to the vertical
    // top-centre baseline, so invert that offset to preserve the line's block
    // position while using its real cross-axis vertical origin.
    let origin = PdfPoint::new(
        vertical_baseline_x,
        glyph_baseline_y - vertical_origin_offset,
    );

    content.push_str(&PdfRgb::from(run.color).fill_operator());
    content.push_str("BT\n");
    let font_size = custom_fonts
        .get(shaped.font_key)
        .map_or(run.font_size, |font| font.adjusted_font_size(run.font_size));
    content.push_str(&format!(
        "/{} {} Tf\n",
        sanitize_pdf_name(shaped.font_key),
        font_size
    ));
    append_positioned_vertical_shaped_text(
        content,
        origin,
        &shaped.shaped,
        prepared_custom_fonts.get(shaped.font_key),
    );
    content.push_str("ET\n");
    Some(run.inline_advance(run.font_size.max(0.0)))
}

pub(super) fn vertical_mixed_upright_run(run: &TextRun) -> bool {
    run.inline_box.is_none()
        && !run.text.is_empty()
        && run.text.chars().any(is_cjk_codepoint)
        && run
            .text
            .chars()
            .all(|ch| ch.is_whitespace() || is_cjk_codepoint(ch))
}

#[allow(clippy::too_many_arguments)]
pub(super) fn render_vertical_mixed_upright_run(
    content: &mut String,
    run: &TextRun,
    x: f32,
    y: f32,
    parent_font_size: f32,
    custom_fonts: &HashMap<String, TtfFont>,
    prepared_custom_fonts: &PreparedCustomFonts,
    word_spacing: f32,
    vertical_e: f32,
    vertical_f: f32,
    pdf_writer: &mut PdfWriter,
    page_images: &mut Vec<ImageRef>,
) -> f32 {
    let run_width = estimate_run_width_with_fonts(run, custom_fonts);
    let page_x = y + vertical_e - (run.font_size - run_width).max(0.0) * 0.5;
    let page_y = vertical_f - x - run.font_size * 0.75;
    content.push_str("q\n");
    content.push_str(&format!("0 1 -1 0 {vertical_f} {} cm\n", -vertical_e));
    let width = render_run_glyphs(
        content,
        run,
        page_x,
        page_y,
        parent_font_size,
        custom_fonts,
        prepared_custom_fonts,
        word_spacing,
        pdf_writer,
        page_images,
    );
    content.push_str("Q\n");
    width
}

#[allow(clippy::too_many_arguments)]
pub(super) fn render_vertical_mixed_line_text(
    content: &mut String,
    runs: &[TextRun],
    start_x: f32,
    y: f32,
    custom_fonts: &HashMap<String, TtfFont>,
    prepared_custom_fonts: &PreparedCustomFonts,
    word_spacing: f32,
    line_ascender: f32,
    vertical_e: f32,
    vertical_f: f32,
    pdf_writer: &mut PdfWriter,
    page_images: &mut Vec<ImageRef>,
) {
    let parent_font_size = crate::layout::text::line_primary_font_size(runs);
    let mut x = start_x;
    for run in runs
        .iter()
        .filter(|r| !r.text.is_empty() || r.inline_box.is_some())
    {
        if let Some(advance) = run.atomic_inline_advance() {
            x += advance;
            continue;
        }
        let run_y = y + drop_cap_baseline_shift(run, line_ascender, custom_fonts);
        let run_width = if vertical_mixed_upright_run(run) {
            render_vertical_mixed_upright_run(
                content,
                run,
                x,
                run_y,
                parent_font_size,
                custom_fonts,
                prepared_custom_fonts,
                word_spacing,
                vertical_e,
                vertical_f,
                pdf_writer,
                page_images,
            )
        } else {
            render_run_glyphs(
                content,
                run,
                x,
                run_y,
                parent_font_size,
                custom_fonts,
                prepared_custom_fonts,
                word_spacing,
                pdf_writer,
                page_images,
            )
        };
        x += run_width;
    }
}

#[allow(clippy::too_many_arguments)]
pub(super) fn push_line_text_clip(
    content: &mut String,
    runs: &[TextRun],
    start_x: f32,
    y: f32,
    custom_fonts: &HashMap<String, TtfFont>,
    prepared_custom_fonts: &PreparedCustomFonts,
    word_spacing: f32,
    line_ascender: f32,
) -> bool {
    let non_empty: Vec<&TextRun> = runs
        .iter()
        .filter(|r| !r.text.is_empty() || r.inline_box.is_some())
        .collect();
    if non_empty.is_empty() {
        return false;
    }

    let parent_font_size = crate::layout::text::line_primary_font_size(runs);
    let has_inline_box = non_empty.iter().any(|r| r.inline_box.is_some());
    let has_drop_cap = non_empty.iter().any(|run| is_drop_cap_run(run));
    let all_standard = !has_inline_box
        && !has_drop_cap
        && non_empty.iter().all(|run| {
            crate::text::resolve_custom_font(
                &run.font_family,
                run.bold,
                run.font_style.is_slanted(),
                custom_fonts,
            )
            .is_none()
                && crate::text::shape_with_unicode_fallback(run, custom_fonts).is_none()
        });

    content.push_str("BT\n7 Tr\n");
    if all_standard {
        let mut cur_baseline = y;
        let mut first = true;
        for run in &non_empty {
            let font_name = resolve_font_name(run, None, None, custom_fonts);
            content.push_str(&format!("/{font_name} {} Tf\n", run.font_size));
            let target_baseline = y
                + run_vertical_align_shift(run, parent_font_size)
                + drop_cap_baseline_shift(run, line_ascender, custom_fonts);
            if first {
                content.push_str(&format!(
                    "{} {} Td\n",
                    format_pdf_number(start_x),
                    format_pdf_number(target_baseline),
                ));
                cur_baseline = target_baseline;
                first = false;
            } else if (target_baseline - cur_baseline).abs() > f32::EPSILON {
                content.push_str(&format!(
                    "0 {} Td\n",
                    format_pdf_number(target_baseline - cur_baseline),
                ));
                cur_baseline = target_baseline;
            }
            let encoded = encode_pdf_text(&run.unshaped_drawing_text());
            content.push_str(&format!("({encoded}) Tj\n"));
        }
    } else {
        let mut x = start_x;
        for run in &non_empty {
            if let Some(advance) = run.atomic_inline_advance() {
                x += advance;
                continue;
            }
            if run.text.is_empty() {
                continue;
            }
            let run_y = y
                + run_vertical_align_shift(run, parent_font_size)
                + drop_cap_baseline_shift(run, line_ascender, custom_fonts);
            let shaped = crate::text::shape_text_run(run, custom_fonts);
            let run_width = shaped.as_ref().map_or_else(
                || estimate_run_width_with_fonts(run, custom_fonts),
                |shaped| shaped.width,
            );
            let custom_font = crate::text::resolve_custom_font(
                &run.font_family,
                run.bold,
                run.font_style.is_slanted(),
                custom_fonts,
            );
            let font_name = resolve_font_name(run, custom_font, shaped.as_ref(), custom_fonts);
            let font_size = custom_font.map_or(run.font_size, |(_, font)| {
                font.adjusted_font_size(run.font_size)
            });
            content.push_str(&format!("/{font_name} {font_size} Tf\n"));
            let prepared_font_name = custom_font.map(|(resolved_name, _)| {
                prepared_font_name_for_run(resolved_name, run, custom_fonts)
            });
            let prepared_font = prepared_font_name
                .as_deref()
                .and_then(|name| prepared_custom_fonts.get(name));
            let synthetic_bold_width = run
                .synthetic_bold_stroke_width(custom_fonts)
                .filter(|_| prepared_font.is_none_or(|font| !font.embeds_synthetic_weight()));
            let shear = run.synthetic_italic_shear(custom_fonts).unwrap_or_default();
            if let (Some((_, font)), Some(shaped)) = (custom_font, shaped.as_ref()) {
                // Text render mode 7 clips to fills only, so expand a synthetic
                // weight by repeating the outline around half its stroke width.
                let embolden = synthetic_bold_width.unwrap_or_default() / 2.0;
                let offsets = [
                    (0.0, 0.0),
                    (-embolden, 0.0),
                    (embolden, 0.0),
                    (0.0, embolden),
                    (0.0, -embolden),
                    (-embolden, embolden),
                    (embolden, embolden),
                    (0.0, embolden * 2.0),
                    (-embolden * 1.5, embolden),
                ];
                let offset_count = synthetic_bold_width.map_or(1, |_| offsets.len());
                for (dx, dy) in offsets.iter().take(offset_count) {
                    let render = ShapedTextRender::new(
                        PdfPoint::new(x + dx, run_y + dy),
                        run.font_size,
                        font,
                        shaped,
                        prepared_font,
                        PdfContentSpace::Points,
                    )
                    .with_word_spacing(word_spacing)
                    .with_shear(shear);
                    if render.has_complex_offsets() {
                        append_positioned_shaped_text(content, render);
                    } else {
                        append_tj_shaped_text(content, render);
                    }
                }
            } else {
                let encoded = encode_pdf_text(&run.unshaped_drawing_text());
                content.push_str(&format!(
                    "1 0 0 1 {} {} Tm\n",
                    format_pdf_number(x),
                    format_pdf_number(run_y),
                ));
                content.push_str(&format!("({encoded}) Tj\n"));
            }
            x += run_width;
        }
    }
    content.push_str("ET\n");
    true
}
