//! Painting a text block's box and its lines, wherever its layout placed it:
//! in its container's flow or at an absolute position.

use super::*;
use crate::layout::elements::TextBlock;

/// Where a text block's border box lies on the page, and what to paint.
#[derive(Clone, Copy)]
pub(super) struct TextBoxPlacement {
    pub(super) geometry: LayoutBoxGeometry,
    pub(super) phase: ElementPaintPhase,
    pub(super) device_space_available: bool,
}

/// Paint `child`'s shadows, backgrounds, borders and `lines` in its border
/// box. Its content box, inside its borders and padding, holds the lines.
pub(super) fn paint_text_box(
    content: &mut String,
    child: &TextBlock,
    lines: &[TextLine],
    placement: TextBoxPlacement,
    ctx: &mut PageRenderContext<'_>,
) {
    let TextBoxPlacement {
        geometry: tb_geometry,
        phase,
        device_space_available,
    } = placement;
    let form_widget = is_form_widget(child, &ctx.text.pdf_writer.opts);
    let border = &child.box_model.border;
    let background_color = &child.paint.background.color;
    let tb_bg_gradient = &child.paint.background.layers.gradient;
    let tb_bg_radial = &child.paint.background.layers.radial_gradient;
    let tb_bg_conic = &child.paint.background.layers.conic_gradient;
    let tb_bg_svg = ctx
        .text
        .pdf_writer
        .resolve_background_svg(&child.paint.background.layers);
    let tb_bg_svg = &tb_bg_svg;
    let tb_bg_blur = &child.paint.background.layers.blur_radius;
    let tb_bg_size = &child.paint.background.layers.size;
    let tb_bg_position = &child.paint.background.layers.position;
    let tb_bg_repeat = &child.paint.background.layers.repeat;
    let tb_bg_origin = &child.paint.background.layers.origin;
    let tb_bg_clip = &child.paint.background.layers.clip;
    let text_align = &child.text.alignment;
    let tb_box_shadows = &child.paint.shadows;
    let tb_bg_blend = &child.paint.background.blend_mode;
    let tb_clip_rect = &child.clipping.rect;
    let tb_transform = &child.paint.group.transform.value;
    let tb_radii = &child.paint.border_radii;
    let tb_text_indent = &child.text.indent;
    let tb_writing_mode = &child.text.writing_mode;
    let tb_box_geometry = ctx.text.pdf_writer.resolve_box_geometry(tb_geometry);
    let tb_paint_geometry = tb_box_geometry.painting();
    if form_widget
        && phase.paints_contents()
        && let Some(widget) = form_widget_annotation(child, tb_paint_geometry.border_box)
    {
        ctx.text.annotations.push(widget);
    }
    let tb_fragment_geometry = tb_box_geometry.fragment(child.fragmentation.box_fragmentation);
    if phase.paints_contents()
        && let Some(link) =
            text_block_link_annotation(child, tb_fragment_geometry, ctx.text.pdf_writer)
    {
        ctx.text.annotations.push(link);
    }
    let tb_background_geometry = tb_box_geometry.background(*tb_bg_origin, *tb_bg_clip, *tb_radii);
    let tb_background_box = tb_background_geometry.painting_box;
    let tb_gradient_area = LayerPaintArea::new(
        tb_background_geometry
            .positioning_area
            .generated_image_box(),
        tb_background_geometry.image_destination_box,
    );

    let tb_group = PaintGroupScope::begin(content, child, tb_fragment_geometry, ctx);

    if phase.paints_decoration() {
        render_box_shadows(
            content,
            tb_box_shadows,
            tb_fragment_geometry,
            *tb_radii,
            ctx.page_ext_gstates,
            ctx.bg_alpha_counter,
            ctx.text.pdf_writer,
        );

        // Draw child background
        if let Some(color) = background_color {
            let (r, g, b, a) = color.to_f32_rgba();
            let needs_alpha = a < 1.0;
            if needs_alpha {
                let gs_name = format!("GScca{}", ctx.bg_alpha_counter);
                *ctx.bg_alpha_counter += 1;
                ctx.page_ext_gstates.push((gs_name.clone(), a));
                content.push_str(&format!("/{gs_name} gs\n"));
            }
            let color = PdfRgb::from((r, g, b));
            let uses_device_css_clip = device_space_available
                && tb_transform.is_none()
                && is_device_clippable_box_background(
                    *tb_bg_clip,
                    tb_background_box.radii,
                    ctx.text.pdf_writer.page_content_transform,
                    tb_background_box.rect,
                )
                && paint_device_clipped_css_solid(
                    content,
                    ctx.text.pdf_writer.page_content_transform,
                    tb_paint_geometry.border_box,
                    tb_background_box.rect,
                    color,
                );
            if !uses_device_css_clip {
                content.push_str(&color.fill_operator());
                content.push_str(&tb_background_box.path_or_rect());
                content.push_str("f\n");
            }
            if needs_alpha {
                content.push_str("/GSDefault gs\n");
            }
        }

        // `background-blend-mode`: the background image layers (gradient)
        // blend against the background color painted above. Scope the
        // blend gstate to a `q`..`Q` around the gradient paint.
        let bg_blend_mode = tb_bg_blend.background_layer(0);
        let bg_blended = bg_blend_mode != crate::style::computed::BlendMode::Normal;
        // Draw linear gradient background
        if let Some(gradient) = tb_bg_gradient {
            if bg_blended {
                content.push_str("q\n");
                begin_blend_mode(content, ctx.page_ext_gstates, bg_blend_mode);
            }
            let rounded_clip = tb_background_box.push_rounded_clip(content);
            render_linear_gradient(
                content,
                gradient,
                GradientBackdrop::isolated_linear_layer(
                    *background_color,
                    tb_bg_radial.is_some() || tb_bg_conic.is_some() || tb_bg_svg.is_some(),
                    bg_blend_mode,
                ),
                tb_gradient_area,
                ctx.shadings,
                ctx.shading_counter,
                ctx.text.pdf_writer,
                ctx.text.page_images,
            );
            if rounded_clip {
                content.push_str("Q\n");
            }
            if bg_blended {
                content.push_str("Q\n");
            }
        }

        // Draw radial gradient background
        if let Some(gradient) = tb_bg_radial {
            if bg_blended {
                content.push_str("q\n");
                begin_blend_mode(content, ctx.page_ext_gstates, bg_blend_mode);
            }
            let rounded_clip = tb_background_box.push_rounded_clip(content);
            render_radial_gradient(
                content,
                gradient,
                tb_gradient_area,
                ctx.shadings,
                ctx.shading_counter,
                ctx.text.pdf_writer,
                ctx.text.page_images,
            );
            if rounded_clip {
                content.push_str("Q\n");
            }
            if bg_blended {
                content.push_str("Q\n");
            }
        }

        // Draw conic gradient background
        if let Some(gradient) = tb_bg_conic {
            if bg_blended {
                content.push_str("q\n");
                begin_blend_mode(content, ctx.page_ext_gstates, bg_blend_mode);
            }
            let rounded_clip = tb_background_box.push_rounded_clip(content);
            render_conic_gradient(
                content,
                gradient,
                tb_gradient_area,
                ctx.text.pdf_writer,
                ctx.text.page_images,
            );
            if rounded_clip {
                content.push_str("Q\n");
            }
            if bg_blended {
                content.push_str("Q\n");
            }
        }

        if let Some(svg_tree) = tb_bg_svg {
            render_block_svg_background(
                content,
                svg_tree,
                PdfBackgroundResources::new(
                    ctx.text.pdf_writer,
                    ctx.text.page_images,
                    ctx.shadings,
                    ctx.shading_counter,
                    Some(ctx.page_ext_gstates),
                )
                .with_custom_fonts(ctx.text.custom_fonts, ctx.text.prepared_custom_fonts),
                BlockBackground {
                    geometry: tb_box_geometry,
                    border_radii: *tb_radii,
                    size: *tb_bg_size,
                    position: *tb_bg_position,
                    repeat: *tb_bg_repeat,
                    origin: *tb_bg_origin,
                    clip: *tb_bg_clip,
                    blur_radius: *tb_bg_blur,
                    blend_mode: bg_blend_mode,
                },
            );
        }

        render_box_shadows_inset(
            content,
            tb_box_shadows,
            tb_fragment_geometry,
            *tb_radii,
            ctx.page_ext_gstates,
            ctx.bg_alpha_counter,
            ctx.text.pdf_writer,
        );
        paint_box_decoration(
            content,
            tb_fragment_geometry,
            border,
            *tb_radii,
            child.paint.border_image.as_ref(),
            BorderPaintResources::from_page(ctx),
        );
    }

    // Apply the overflow clip (if any) around the text so overflowing
    // lines are cut at the padding box, matching Chrome. The
    // background and border above are drawn unclipped so the border
    // stays fully visible; only the inline/line-box content is
    // clipped. `tb_clip_rect` is set by layout for
    // overflow:hidden/clip/scroll/auto.
    let tb_needs_clip = phase.paints_contents() && tb_clip_rect.is_some();
    if tb_needs_clip {
        tb_paint_geometry
            .rounded_padding_box(*tb_radii)
            .push_clip(content);
    }

    // Draw child text. Inset from the border-box top by BOTH the top
    // border width and the top padding (matching the primary text path
    // at the top of this fn); omitting the border placed the first
    // baseline `border-top` px too high inside bordered clip boxes.
    let content_top = tb_geometry.content_box().top();
    let mut baseline_cursor = ctx.text.pdf_writer.text_baseline_cursor(content_top);
    let line_metadata = lines
        .first()
        .map_or(Default::default(), |line| line.metadata);
    let vertical_lr = line_metadata.writing_mode.is_vertical_lr();
    let upright_vertical = line_metadata.text_orientation_upright;
    let vertical = phase.paints_contents() && tb_writing_mode.is_vertical() && !upright_vertical;
    let vertical_transform = if vertical {
        let content_left = tb_geometry.content_box().left;
        let content_right = tb_geometry.content_box().right();
        let column_x = if vertical_lr {
            content_left + lines.first().map_or(0.0, |line| line.height)
        } else {
            content_right
        };
        let e = column_x - content_top;
        let f = content_top + content_left;
        content.push_str("q\n");
        content.push_str(&format!("0 -1 1 0 {e} {f} cm\n"));
        Some((e, f))
    } else {
        None
    };
    let mut tb_first_line = true;
    for line in lines.iter().filter(|_| phase.paints_contents()) {
        let metrics = line_box_metrics(line, ctx.text.custom_fonts);
        let text_y = if vertical {
            baseline_cursor.next_raw(metrics)
        } else {
            baseline_cursor.next_horizontal(metrics)
        };
        let merged = crate::text::coalesce_text_runs(&line.runs);
        let line_width: f32 = merged
            .iter()
            .map(|run| {
                if upright_vertical {
                    text_combine_advance(run, ctx.text.custom_fonts).unwrap_or_else(|| {
                        estimate_run_width_with_fonts(run, ctx.text.custom_fonts)
                    })
                } else {
                    estimate_run_width_with_fonts(run, ctx.text.custom_fonts)
                }
            })
            .sum();
        // CSS `text-indent` shifts only the first line's start. List
        // items pass a negative value so an `outside` marker (the
        // leading run) hangs left into the padding while the text
        // lands at the content edge.
        let first_line_indent = if tb_first_line { *tb_text_indent } else { 0.0 };
        tb_first_line = false;
        // Horizontal insets from the border-box edge. `render_x`/`render_w`
        // are the BORDER box, so the content box starts after the left
        // border + left padding and is narrowed by both horizontal borders
        // and paddings — mirroring the primary text path
        // (`padding_box_x = block_x + border_left`,
        // content = `padding_box_x + padding.left`) and the vertical inset
        // in this same arm (`render_y - border.top.width - padding.top`).
        // For left/justify the text starts at the content-box left; for
        // right/center it is aligned within the content box. (Previously
        // this branch used `render_x + padding.left`, dropping the left
        // border so text in bordered clip/nested boxes sat `border-left`
        // px too far left.)
        let content_x = tb_geometry.content_box().left;
        let content_w = tb_geometry.content_box().width;
        // Drop-cap float exclusion: shift the line right so its text
        // wraps beside the floated `::first-letter` (css2 §9.5).
        let line_inset = line.x_offset;
        // Floats beside the line take room from either side (css2 §9.5).
        let floats = line.float_insets;
        let room = content_w - floats.horizontal();
        let text_x = match text_align {
            TextAlign::Right => content_x + floats.left + (room - line_width).max(0.0),
            TextAlign::Center => content_x + floats.left + (room - line_width).max(0.0) / 2.0,
            _ => content_x + first_line_indent + floats.left + line_inset,
        };
        let line_top_y = text_y + metrics.ascender + metrics.half_leading;
        let line_bottom_y = text_y - metrics.descender - metrics.half_leading;
        // Parent text content-area edges for `text-top`/`text-bottom`.
        let (text_ascent, text_descent) = line_text_content_extents(line, ctx.text.custom_fonts);
        let line_text_top_y = if text_ascent > 0.0 {
            text_y + text_ascent
        } else {
            line_top_y
        };
        let line_text_bottom_y = if text_descent > 0.0 {
            text_y - text_descent
        } else {
            line_bottom_y
        };
        let mut lx = text_x;
        let mut border_pieces = InlineBorderPieces::default();
        for (run_index, run) in merged.iter().enumerate() {
            // Atomic inline box (e.g. a `list-style-image` marker):
            // paint the box/image and advance by its outer width;
            // The glyph painter would shape its empty text and draw
            // nothing, dropping the marker entirely.
            if let Some(inline) = run.inline_box.as_deref() {
                let advance = run.atomic_inline_advance().unwrap_or_default();
                border_pieces.add(run, lx, advance, text_y);
                if !run.is_inline_edge() {
                    render_run_inline_box(
                        content,
                        run,
                        inline,
                        InlineBoxSlot {
                            x: lx + inline.margin_left,
                            baseline: text_y,
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
                lx += run.atomic_inline_advance().unwrap_or_default();
                continue;
            }
            if run.text.is_empty() {
                continue;
            }
            let run_width = if upright_vertical {
                text_combine_advance(run, ctx.text.custom_fonts)
                    .unwrap_or_else(|| estimate_run_width_with_fonts(run, ctx.text.custom_fonts))
            } else {
                estimate_run_width_with_fonts(run, ctx.text.custom_fonts)
            };
            // Per-run inline background (e.g. a `::first-letter`/
            // `::first-line` `background-color`, or a highlighted
            // inline span): paint the rectangle behind the glyphs
            // before drawing the text. Mirrors the other line-box
            // render paths (table cells, absolute boxes).
            if let Some(background) = run.background_color {
                let (br, bgc, bb, ba) = background.to_f32_rgba();
                let needs_inline_bg_alpha = ba < 1.0;
                if needs_inline_bg_alpha {
                    let gs_name = format!("GStbiba{}", ctx.bg_alpha_counter);
                    *ctx.bg_alpha_counter += 1;
                    ctx.page_ext_gstates.push((gs_name.clone(), ba));
                    content.push_str(&format!("/{gs_name} gs\n"));
                }
                let rx = lx - run.padding.left;
                let rw2 = run_width + run.padding.horizontal();
                let (ry, rh) =
                    inline_background_y_and_height(run, text_y, run.padding, ctx.text.custom_fonts);
                content.push_str(&PdfRgb::from((br, bgc, bb)).fill_operator());
                content.push_str(
                    &ctx.text
                        .pdf_writer
                        .snap_border_box(PdfRect::new(rx, ry, rw2, rh))
                        .rounded(run.border_radii)
                        .path_or_rect(),
                );
                content.push_str("f\n");
                if needs_inline_bg_alpha {
                    content.push_str("/GSDefault gs\n");
                }
            }
            border_pieces.add(run, lx, run_width, text_y);
            // A floated `::first-letter` drop cap is lowered so its
            // glyph top sits on the line's text top (css-pseudo-4 §2.2).
            let run_y = text_y
                + drop_cap_baseline_shift(
                    run,
                    line_text_top(line, ctx.text.custom_fonts),
                    ctx.text.custom_fonts,
                );
            let decoration = (!vertical).then(|| {
                let previous = merged[..run_index]
                    .iter()
                    .rev()
                    .find(|previous| previous.inline_box.is_none() && !previous.text.is_empty());
                HorizontalRunDecorations::new(run, lx, run_width, run_y, ctx.text.custom_fonts)
                    .continuing_after(previous)
            });
            if let Some(decoration) = &decoration {
                decoration.paint_shadows(content);
                render_run_text_shadows(
                    content,
                    run,
                    lx,
                    run_y,
                    crate::layout::text::line_primary_font_size(&merged),
                    ctx.text.custom_fonts,
                    ctx.text.prepared_custom_fonts,
                    0.0,
                    ctx.text.pdf_writer,
                    ctx.text.page_images,
                );
                decoration.paint_below_text(content);
            }
            let rw = if *tb_bg_blur > 0.0
                && render_text_shadow_blur(
                    content,
                    run,
                    lx,
                    run_y,
                    *tb_bg_blur * 2.0,
                    run.color.to_f32_rgba(),
                    ctx.text.custom_fonts,
                    ctx.text.pdf_writer,
                    ctx.text.page_images,
                ) {
                if upright_vertical {
                    text_combine_advance(run, ctx.text.custom_fonts).unwrap_or_else(|| {
                        estimate_run_width_with_fonts(run, ctx.text.custom_fonts)
                    })
                } else {
                    estimate_run_width_with_fonts(run, ctx.text.custom_fonts)
                }
            } else if upright_vertical && run.metadata.text_combine_upright.is_active() {
                render_text_combine_run(
                    content,
                    run,
                    lx,
                    run_y,
                    crate::layout::text::line_primary_font_size(&merged),
                    ctx.text.custom_fonts,
                    ctx.text.prepared_custom_fonts,
                    0.0,
                    ctx.text.pdf_writer,
                    ctx.text.page_images,
                )
            } else if let Some((vertical_e, vertical_f)) = vertical_transform
                && vertical_mixed_upright_run(run)
            {
                render_vertical_mixed_upright_run(
                    content,
                    run,
                    lx,
                    run_y,
                    crate::layout::text::line_primary_font_size(&merged),
                    ctx.text.custom_fonts,
                    ctx.text.prepared_custom_fonts,
                    0.0,
                    vertical_e,
                    vertical_f,
                    ctx.text.pdf_writer,
                    ctx.text.page_images,
                )
            } else if decoration.is_some() {
                render_run_glyphs_without_shadows(
                    content,
                    run,
                    lx,
                    run_y,
                    crate::layout::text::line_primary_font_size(&merged),
                    ctx.text.custom_fonts,
                    ctx.text.prepared_custom_fonts,
                    0.0,
                    ctx.text.pdf_writer,
                    ctx.text.page_images,
                )
            } else {
                render_run_glyphs(
                    content,
                    run,
                    lx,
                    run_y,
                    crate::layout::text::line_primary_font_size(&merged),
                    ctx.text.custom_fonts,
                    ctx.text.prepared_custom_fonts,
                    0.0,
                    ctx.text.pdf_writer,
                    ctx.text.page_images,
                )
            };
            if let Some(decoration) = &decoration {
                decoration.paint_above_text(content);
            }
            lx += rw;
        }
        border_pieces.paint(content, ctx);
    }
    if vertical {
        content.push_str("Q\n");
    }
    if tb_needs_clip {
        content.push_str("Q\n");
    }
    tb_group.finish(content, ctx);
}
