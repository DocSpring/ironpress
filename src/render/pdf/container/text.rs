use super::*;
use crate::layout::elements::TextBlock;

use super::text_box_paint::{TextBoxPlacement, paint_text_box};

pub(super) fn render_text_child(
    content: &mut String,
    child: &TextBlock,
    child_index: usize,
    flow: &ContainerFlowContext<'_>,
    flow_position: FlowPosition,
    abs_origins: &mut HashMap<usize, PdfPoint>,
    ctx: &mut PageRenderContext<'_>,
) -> FlowPosition {
    let x = flow.frame.content_origin.x;
    let width = flow.frame.width();
    let self_pad_origin = flow.frame.padding_origin;
    let container_top_y = flow.container_top_y;
    let flow_top_by_index = flow.flow_top_by_index;
    let float_top_by_index = flow.float_top_by_index;
    let left_float_bottom = flow.left_float_bottom;
    let right_float_bottom = flow.right_float_bottom;
    let device_space_available = flow.device_space_available;
    let phase = flow.paint_phase;
    let FlowPosition {
        y: _,
        mut cursor_y,
        previous_margin_bottom: mut prev_margin_bottom,
    } = flow_position;
    let mut y;
    // A form field's widget draws its value, so the text block doesn't
    let form_widget = is_form_widget(child, &ctx.text.pdf_writer.opts);
    let no_lines = Vec::new();
    let lines = if form_widget { &no_lines } else { &child.lines };
    let margin_top = &child.box_model.margins.start;
    let margin_bottom = &child.box_model.margins.end;
    let padding = &child.box_model.padding;
    let border = &child.box_model.border;
    let block_height = &child.box_model.size.height;
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
    let tb_bg_clip = &child.paint.background.layers.clip;
    let tb_float = &child.flow.float;
    let tb_clear = &child.flow.clear;
    let position = &child.positioning.scheme;
    let offset_top = &child.positioning.insets.top;
    let offset_left = &child.positioning.insets.left;
    let offset_bottom = &child.positioning.insets.bottom;
    let tb_opacity = &child.paint.group.effects.opacity;
    let tb_mix_blend = &child.paint.group.effects.mix_blend_mode;
    let tb_box_shadows = &child.paint.shadows;
    let tb_block_width = &child.box_model.size.width;
    let tb_clip_rect = &child.clipping.rect;
    let tb_box_transform = &child.paint.group.transform;
    let tb_transform = &tb_box_transform.value;
    let tb_radii = &child.paint.border_radii;
    let tb_writing_mode = &child.text.writing_mode;
    let tb_containing_block = &child.positioning.containing_block;
    // Absolute-positioned children render at offset from the
    // containing block's padding box (CSS spec), not the content box.
    // Use container_top_y (original y before flow children advance it).
    if position.is_absolute() {
        let text_h: f32 = lines.iter().map(|l| l.height).sum();
        // `block_height` remains a padding-box height after pagination has
        // turned this text block into an absolute fragment.  Add the border
        // exactly once, just as the in-flow text path does.
        let content_pad_box = padding.vertical() + text_h;
        let pad_box_h = block_height.resolve(content_pad_box);
        let abs_h = pad_box_h + border.vertical_width();
        let abs_w = tb_block_width.resolve(width);
        // Anchor to the nearest positioned ancestor's padding box
        // (resolved by containing-block depth), skipping any static
        // intermediate container this box is nested inside.
        let anchor = abs_child_anchor(tb_containing_block, abs_origins, self_pad_origin);
        let abs_x = anchor.x + offset_left;
        let abs_y = anchor.y - offset_top;
        let tb_geometry = LayoutBoxGeometry::from_layout(
            PdfRect::from_top(abs_x, abs_y, abs_w, abs_h),
            border,
            *padding,
            child.paint.border_image.as_ref(),
        );
        if phase == ElementPaintPhase::All
            && let Some(t) = tb_transform
            && is_projected_transform(t)
            && lines.is_empty()
            && tb_bg_gradient.is_none()
            && tb_bg_radial.is_none()
            && tb_bg_conic.is_none()
            && tb_bg_svg.is_none()
            && tb_radii.is_zero()
            && tb_box_shadows.is_empty()
            && *tb_opacity == 1.0
            && *tb_mix_blend == crate::style::computed::BlendMode::Normal
            && child.paint.group.effects.masking.clip_path.is_none()
            && child.paint.group.effects.masking.image.is_none()
        {
            render_projected_solid_box(
                content,
                ctx.text.pdf_writer.page_content_transform,
                tb_box_transform,
                ctx.text
                    .pdf_writer
                    .resolve_box_geometry(tb_geometry)
                    .painting(),
                *background_color,
                border,
            );
            return flow_position;
        }
        paint_text_box(
            content,
            child,
            lines,
            TextBoxPlacement {
                geometry: tb_geometry,
                phase,
                device_space_available,
            },
            ctx,
        );
        // Don't advance cursor_y for absolute elements
        return flow_position;
    }

    // A floated block is out of normal flow: it is pinned at its
    // precomputed top (the flow cursor at its source position) and
    // does NOT advance `cursor_y`. An in-flow block collapses its
    // margin-top with the previous sibling, after first clearing any
    // floats it must drop below.
    let is_float = *tb_float != Float::None;
    let planned_flow_top = flow_top_by_index.get(&child_index).copied();
    if let Some(top) = planned_flow_top {
        y = top;
    } else if is_float {
        // Place the float from the shared simulator's top so its
        // paint position (it paints last) matches the flow.
        let rel_top = float_top_by_index.get(&child_index).copied().unwrap_or(0.0);
        y = container_top_y - rel_top;
    } else {
        if *tb_clear != Clear::None {
            cursor_y = clear_cursor(
                cursor_y,
                *tb_clear,
                left_float_bottom,
                right_float_bottom,
                &mut prev_margin_bottom,
            );
        }
        cursor_y -= collapsed_margin_top_extra(*margin_top, prev_margin_bottom);
        y = cursor_y;
    }
    let text_h: f32 = lines.iter().map(|l| l.height).sum();
    // `block_height` is a *padding-box* height (TextBlock convention),
    // so the painted border box adds the border on top. Compute the
    // padding-box height first (content vs. explicit), then add the
    // border once — mirroring paginate's `estimate_element_height`
    // (`effective_h + border.vertical_width()`) so the painted box
    // matches the flow. Folding the border into the value compared
    // against `block_height` (the old `max(content+border, bh)`)
    // rendered a border-box-sized child short by its border.
    let content_pad_box = padding.vertical() + text_h;
    // A provided `block_height` is the used padding-box height. Tall
    // inline content may overflow it, but it must not enlarge a box
    // with a definite CSS `height`.
    let pad_box_h = block_height.resolve(content_pad_box);
    let child_h = pad_box_h + border.vertical_width();

    let render_w = tb_block_width.resolve(width);
    let vertical_column_paint_h = if *offset_bottom > 0.0 && tb_writing_mode.is_vertical() {
        *offset_bottom + border.vertical_width()
    } else {
        child_h
    };

    // Resolve ordinary flow, relative translation, and sticky scrollport
    // constraints through the shared positioning contract. Sticky insets are
    // constraints from the scrollport edges, not unconditional translations.
    let normal_inline_offset = match tb_float {
        Float::Right => width - render_w,
        _ => 0.0,
    };
    let used_origin = child.positioning.resolve_in_flow_origin(
        crate::types::Point::new(normal_inline_offset, container_top_y - y),
        crate::types::Size::new(render_w, vertical_column_paint_h),
        flow.frame.size,
    );
    let render_x = x + used_origin.x;
    let render_y = container_top_y - used_origin.y;
    let tb_geometry = LayoutBoxGeometry::from_layout(
        PdfRect::from_top(render_x, render_y, render_w, vertical_column_paint_h),
        border,
        *padding,
        child.paint.border_image.as_ref(),
    );
    let tb_paint_geometry = ctx
        .text
        .pdf_writer
        .resolve_box_geometry(tb_geometry)
        .painting();
    // CSS `filter: blur()` on a nested solid box (css-filter-effects-1
    // §4.1): rasterize the bg fill + border, gaussian-blur it, and
    // embed it overflowing the border box. Restricted to a plain solid
    // box (no gradient/SVG bg, no text, no clip, square corners) so the
    // vector paint path is byte-unchanged for everything else.
    if phase == ElementPaintPhase::All
        && *tb_bg_blur > 0.0
        && child.paint.group.effects.is_identity()
        && child.paint.group.transform.value.is_none()
        && lines.is_empty()
        && tb_bg_gradient.is_none()
        && tb_bg_radial.is_none()
        && tb_bg_conic.is_none()
        && tb_bg_svg.is_none()
        && tb_clip_rect.is_none()
        && *tb_bg_clip == BackgroundClip::Border
        && tb_radii.is_zero()
        && tb_box_shadows.is_empty()
        && child.paint.outline.width == 0.0
        && child.paint.background.blend_mode == crate::style::computed::BlendMode::Normal
        && let Some(blurred) = crate::render::blur::blur_box(
            tb_paint_geometry.border_box.width,
            tb_paint_geometry.border_box.height,
            *background_color,
            border,
            *tb_bg_blur,
            ctx.text.pdf_writer.opts.raster_quality.filter_dpi,
        )
    {
        let img_obj_id = ctx.text.pdf_writer.add_image_object(
            &blurred.asset.data,
            blurred.asset.source_width,
            blurred.asset.source_height,
            blurred.asset.format,
            blurred.asset.png_metadata.as_ref(),
        );
        let img_name = format!("Im{img_obj_id}");
        let ov = blurred.overflow_pt;
        content.push_str(&format!(
            "q\n{w} 0 0 {h} {ix} {iy} cm\n/{name} Do\nQ\n",
            w = tb_paint_geometry.border_box.width + 2.0 * ov,
            h = tb_paint_geometry.border_box.height + 2.0 * ov,
            ix = tb_paint_geometry.border_box.left - ov,
            iy = tb_paint_geometry.border_box.bottom - ov,
            name = img_name,
        ));
        ctx.text.page_images.push(ImageRef {
            name: img_name,
            obj_id: img_obj_id,
        });
        // Advance the flow cursor exactly as the normal block path
        // below (the filter does not change layout).
        if is_float {
            prev_margin_bottom = 0.0;
        } else if planned_flow_top.is_some() {
            prev_margin_bottom = *margin_bottom;
        } else {
            cursor_y -= child_h + *margin_bottom;
            y = cursor_y;
            prev_margin_bottom = *margin_bottom;
        }
        return FlowPosition::new(y, cursor_y, prev_margin_bottom);
    }

    paint_text_box(
        content,
        child,
        lines,
        TextBoxPlacement {
            geometry: tb_geometry,
            phase,
            device_space_available,
        },
        ctx,
    );
    if is_float {
        // A float does not advance the flow cursor (its bottom is
        // already tracked via the simulator for `clear`). It breaks
        // the margin-collapse chain for the next in-flow sibling.
        let _ = child_h;
        prev_margin_bottom = 0.0;
    } else if planned_flow_top.is_some() {
        prev_margin_bottom = *margin_bottom;
    } else {
        // Advance past the box AND its margin-bottom so a following
        // in-flow sibling sits below the margin gap (e.g. stacked
        // `<p>`s inside a multicol column keep their `margin-bottom`).
        // Record this block's margin-bottom so the next sibling
        // collapses its margin-top against it (CSS adjacent-margin
        // collapsing), mirroring the Container arm below.
        cursor_y -= child_h + *margin_bottom;
        y = cursor_y;
        prev_margin_bottom = *margin_bottom;
    }

    FlowPosition::new(y, cursor_y, prev_margin_bottom)
}
