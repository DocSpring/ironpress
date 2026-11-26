#[cfg(test)]
use super::geometry::LayoutBoxGeometry;
use super::geometry::{PdfPoint, PdfRect};
use super::*;
use crate::layout::elements::LayoutNode;
#[cfg(test)]
use crate::types::CornerRadii;

pub(super) struct TextRenderContext<'a> {
    pub(super) page_height: f32,
    pub(super) custom_fonts: &'a HashMap<String, TtfFont>,
    pub(super) prepared_custom_fonts: &'a PreparedCustomFonts,
    pub(super) annotations: &'a mut Vec<PageAnnotation>,
    // Threaded so `render_cell_text` can embed blurred `text-shadow` image
    // XObjects (it rasterizes + blurs the shadow glyphs, like the page path).
    pub(super) pdf_writer: &'a mut PdfWriter,
    pub(super) page_images: &'a mut Vec<ImageRef>,
}

impl<'a> TextRenderContext<'a> {
    pub(super) fn new(
        page_height: f32,
        custom_fonts: &'a HashMap<String, TtfFont>,
        prepared_custom_fonts: &'a PreparedCustomFonts,
        annotations: &'a mut Vec<PageAnnotation>,
        pdf_writer: &'a mut PdfWriter,
        page_images: &'a mut Vec<ImageRef>,
    ) -> Self {
        Self {
            page_height,
            custom_fonts,
            prepared_custom_fonts,
            annotations,
            pdf_writer,
            page_images,
        }
    }

    /// Write a page annotation: a link covers only what the `overflow` clips around it leave visible, and a link they
    /// hide is left out, as Chromium writes it
    pub(super) fn push_annotation(&mut self, annotation: PageAnnotation) {
        if let Some(annotation) = visible_annotation(annotation, self.pdf_writer) {
            self.annotations.push(annotation);
        }
    }

    pub(super) fn annotation_marker(&self) -> usize {
        self.annotations.len()
    }

    pub(super) fn discard_annotations_since(&mut self, marker: usize) {
        self.annotations.truncate(marker);
    }
}

pub(super) struct PageRenderContext<'a> {
    pub(super) paint_box: PdfRect,
    /// Top-left of the initial fixed containing block (the page area in paged
    /// media), in PDF page coordinates.
    pub(super) initial_fixed_origin: PdfPoint,
    pub(super) shadings: &'a mut Vec<ShadingEntry>,
    pub(super) shading_counter: &'a mut usize,
    pub(super) page_ext_gstates: &'a mut Vec<(String, f32)>,
    pub(super) bg_alpha_counter: &'a mut usize,
    pub(super) stacking: StackingTraversal,
    pub(super) text: TextRenderContext<'a>,
}

impl<'a> PageRenderContext<'a> {
    #[allow(clippy::too_many_arguments)]
    pub(super) fn new(
        pdf_writer: &'a mut PdfWriter,
        page_images: &'a mut Vec<ImageRef>,
        custom_fonts: &'a HashMap<String, TtfFont>,
        prepared_custom_fonts: &'a PreparedCustomFonts,
        shadings: &'a mut Vec<ShadingEntry>,
        shading_counter: &'a mut usize,
        page_ext_gstates: &'a mut Vec<(String, f32)>,
        bg_alpha_counter: &'a mut usize,
        annotations: &'a mut Vec<PageAnnotation>,
        paint_box: PdfRect,
        page_height: f32,
    ) -> Self {
        Self {
            paint_box,
            initial_fixed_origin: PdfPoint::new(paint_box.left, paint_box.top()),
            shadings,
            shading_counter,
            page_ext_gstates,
            bg_alpha_counter,
            stacking: StackingTraversal::default(),
            text: TextRenderContext::new(
                page_height,
                custom_fonts,
                prepared_custom_fonts,
                annotations,
                pdf_writer,
                page_images,
            ),
        }
    }

    pub(super) const fn with_initial_fixed_origin(mut self, origin: PdfPoint) -> Self {
        self.initial_fixed_origin = origin;
        self
    }
}

#[derive(Clone, Copy)]
pub(super) struct NestedLayoutFrame {
    origin: PdfPoint,
    initial_origin: PdfPoint,
    available_width: f32,
}

impl NestedLayoutFrame {
    pub(super) const fn new(
        origin: PdfPoint,
        initial_origin: PdfPoint,
        available_width: f32,
    ) -> Self {
        Self {
            origin,
            initial_origin,
            available_width,
        }
    }
}

#[derive(Clone, Copy)]
pub(super) struct CellTextPlacement {
    origin: PdfPoint,
    col_width: f32,
    /// Extra horizontal offset applied to the FIRST rendered line only (CSS
    /// `text-indent`). Negative values pull the first line left, used to hang a
    /// list marker into the surrounding padding.
    first_line_indent: f32,
}

impl CellTextPlacement {
    pub(super) const fn new(origin: PdfPoint, col_width: f32) -> Self {
        Self {
            origin,
            col_width,
            first_line_indent: 0.0,
        }
    }

    #[cfg(test)]
    pub(super) const fn with_first_line_indent(mut self, first_line_indent: f32) -> Self {
        self.first_line_indent = first_line_indent;
        self
    }
}

#[derive(Clone, Copy)]
pub(super) struct CellRenderBox {
    origin: PdfPoint,
    col_width: f32,
    row_height: f32,
}

impl CellRenderBox {
    pub(super) const fn new(origin: PdfPoint, col_width: f32, row_height: f32) -> Self {
        Self {
            origin,
            col_width,
            row_height,
        }
    }
}

#[cfg(test)]
pub(super) struct NestedTextBlock<'a> {
    pub(super) lines: &'a [TextLine],
    pub(super) text_align: TextAlign,
    pub(super) padding: EdgeSizes,
    pub(super) border: crate::layout::engine::LayoutBorder,
    pub(super) block_width: Option<f32>,
    pub(super) block_height: Option<f32>,
    /// Whether the box clips overflow (`overflow: hidden`/`scroll`). When true a
    /// definite `block_height` is a hard size and content is clipped to it rather
    /// than growing the box.
    pub(super) clips: bool,
    pub(super) background_color: Option<crate::types::Color>,
    pub(super) background_svg: Option<&'a crate::parser::svg::SvgTree>,
    pub(super) background_blur_radius: f32,
    pub(super) background_size: BackgroundSize,
    pub(super) background_position: BackgroundPosition,
    pub(super) background_repeat: BackgroundRepeat,
    pub(super) background_origin: BackgroundOrigin,
    pub(super) background_clip: BackgroundClip,
    pub(super) background_blur_canvas_box: Option<SvgViewportBox>,
    pub(super) border_radii: CornerRadii,
    /// CSS `text-indent` applied to the first line only. List items use a
    /// negative value here to hang an `outside` marker into the padding band.
    pub(super) text_indent: f32,
}

/// Compute a grid row's painted height. Unlike a table row, a grid track size is
/// resolved during layout (css-grid-1 §11): the row track already accounts for
/// each item's definite/auto height, and a grid item with a definite height does
/// NOT grow its track when its content is taller — the content overflows the box
/// instead. So the painted row height is the track height carried on each cell as
/// `min_content_height`, never grown by the cells' intrinsic content height.
pub(super) fn compute_grid_row_height(cells: &[GridCell]) -> f32 {
    cells
        .iter()
        .map(|cell| cell.layout.box_model.minimum_block_size)
        .fold(0.0f32, f32::max)
}

pub(super) fn render_cell_content(
    content: &mut String,
    cell: &CellBox,
    placement: CellRenderBox,
    inherited_abs_origins: &HashMap<usize, PdfPoint>,
    ctx: &mut PageRenderContext<'_>,
) {
    let content_top = cell_content_top(cell, placement.origin.y, placement.row_height);
    if !cell.content.children.is_empty() {
        let text_h: f32 = cell.content.lines.iter().map(|line| line.height).sum();
        render_cell_text(
            content,
            cell,
            CellTextPlacement::new(
                PdfPoint::new(placement.origin.x, content_top),
                placement.col_width,
            ),
            ctx,
        );
        render_cell_child_elements(
            content,
            &cell.content.children,
            NestedLayoutFrame::new(
                PdfPoint::new(
                    placement.origin.x + cell.box_model.content_insets.left,
                    // `content_top` is already the content-box top (row top minus the
                    // cell's top padding). Nested block content starts just below any
                    // cell text; it must NOT be shifted down by the bottom padding.
                    content_top - text_h,
                ),
                PdfPoint::new(
                    placement.origin.x + cell.box_model.border_insets.left,
                    placement.origin.y - cell.box_model.border_insets.top,
                ),
                (placement.col_width - cell.box_model.content_insets.horizontal()).max(0.0),
            ),
            if cell.establishes_stacking_context() {
                StackingScope::Local
            } else {
                StackingScope::Ancestor
            },
            cell,
            inherited_abs_origins,
            ctx,
        );
        return;
    }

    render_cell_text(
        content,
        cell,
        CellTextPlacement::new(
            PdfPoint::new(placement.origin.x, content_top),
            placement.col_width,
        ),
        ctx,
    );
}

fn render_cell_child_elements(
    content: &mut String,
    elements: &[LayoutNode],
    frame: NestedLayoutFrame,
    stacking_scope: StackingScope,
    cell: &CellBox,
    inherited_abs_origins: &HashMap<usize, PdfPoint>,
    ctx: &mut PageRenderContext<'_>,
) {
    let mut abs_origins = inherited_abs_origins.clone();
    if let Some(depth) = cell.established_containing_block_depth() {
        abs_origins.insert(depth, frame.initial_origin);
    }
    render_container_children(
        content,
        elements,
        ContainerFrame::new(
            frame.origin,
            crate::types::Size::new(frame.available_width, f32::INFINITY),
            frame.initial_origin,
        ),
        &mut abs_origins,
        ctx,
        ContainerRenderOptions {
            stacking_scope,
            ..Default::default()
        },
    );
}

pub(super) fn render_cell_text(
    content: &mut String,
    cell: &CellBox,
    placement: CellTextPlacement,
    ctx: &mut PageRenderContext<'_>,
) {
    let cell_inner_w = placement.col_width - cell.box_model.content_insets.horizontal();
    let mut baseline_cursor = ctx.text.pdf_writer.text_baseline_cursor(placement.origin.y);
    let mut first_drawn_line = true;
    for line in &cell.content.lines {
        let metrics = line_box_metrics(line, ctx.text.custom_fonts);
        let text_y = baseline_cursor.next_horizontal(metrics);
        let line_annotation_bottom = text_y - metrics.descender - metrics.half_leading;
        let line_annotation_height =
            metrics.ascender + metrics.descender + 2.0 * metrics.half_leading;
        // An atomic inline box keeps its run's `text` empty, so a line that
        // holds only inline-blocks is still line content (CSS 2.1 §9.2.2) —
        // the same rule the page-level line painter applies.
        let has_text = line.runs.iter().any(|run| !run.text.is_empty());
        let has_inline_box = line.runs.iter().any(|run| run.inline_box.is_some());
        if !has_text && !has_inline_box {
            continue;
        }
        // CSS `text-indent` shifts the start of the first rendered line. List
        // items pass a negative value so an `outside` marker (the first run)
        // hangs left into the padding while the following text lands at the
        // content edge.
        let first_line_indent = if first_drawn_line {
            placement.first_line_indent
        } else {
            0.0
        };
        first_drawn_line = false;
        let merged = crate::text::coalesce_text_runs(&line.runs);
        let line_width: f32 = merged
            .iter()
            .map(|run| estimate_run_width_with_fonts(run, ctx.text.custom_fonts))
            .sum();
        let text_x = match cell.alignment.inline {
            TextAlign::Right => {
                placement.origin.x
                    + cell.box_model.content_insets.left
                    + (cell_inner_w - line_width).max(0.0)
            }
            TextAlign::Center => {
                placement.origin.x
                    + cell.box_model.content_insets.left
                    + ((cell_inner_w - line_width) / 2.0).max(0.0)
            }
            _ => placement.origin.x + cell.box_model.content_insets.left + first_line_indent,
        };
        // Line-box edges for inline-box vertical alignment, mirroring the
        // page-level text painter's geometry.
        let line_top_y = text_y + metrics.ascender + metrics.half_leading;
        let line_bottom_y = text_y - metrics.descender - metrics.half_leading;
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
        let mut x = text_x;
        // Relatively positioned inline boxes belong to the positioned layer:
        // they paint above the line's in-flow content, in source order
        // (CSS 2.1 Appendix E, step 8). Collect them here and paint them after
        // the in-flow runs — the same deferral the page-level line painter
        // applies.
        let mut deferred_inline: Vec<(&TextRun, &crate::layout::engine::InlineBox, f32, f32, f32)> =
            Vec::new();
        let mut border_pieces = InlineBorderPieces::default();
        for (run_index, run) in merged.iter().enumerate() {
            // Atomic inline box (display: inline-block) in the cell's text
            // flow: paint the box and its inner content, then advance —
            // the same treatment the page-level line painter applies.
            if let Some(inline) = run.inline_box.as_deref() {
                if !run.is_inline_edge() {
                    let box_x = x + inline.margin_left;
                    let run_line_height = run_line_height_for_vertical_align(run);
                    if inline.rel_offset_x != 0.0 || inline.rel_offset_y != 0.0 {
                        deferred_inline.push((run, inline, box_x, run.font_size, run_line_height));
                    } else {
                        render_run_inline_box(
                            content,
                            run,
                            inline,
                            InlineBoxSlot {
                                x: box_x,
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
                                line_height: run_line_height,
                            },
                            ctx,
                        );
                    }
                }
                x += run.atomic_inline_advance().unwrap_or_default();
                continue;
            }
            if let Some(advance) = run.atomic_inline_advance() {
                border_pieces.add(run, x, advance, text_y);
                x += advance;
                continue;
            }
            if run.text.is_empty() {
                continue;
            }
            let run_width = estimate_run_width_with_fonts(run, ctx.text.custom_fonts);
            let previous = merged[..run_index]
                .iter()
                .rev()
                .find(|previous| previous.inline_box.is_none() && !previous.text.is_empty());
            let decoration =
                HorizontalRunDecorations::new(run, x, run_width, text_y, ctx.text.custom_fonts)
                    .continuing_after(previous);

            if let Some(background) = run.background_color {
                let (background_r, background_g, background_b) = background.to_f32_rgb();
                let rx = x - run.padding.left;
                let ry = text_y - 2.0 - run.padding.bottom;
                let rw2 = run_width + run.padding.horizontal();
                let rh = run.font_size + 2.0 + run.padding.vertical();
                content.push_str(&format!(
                    "{background_r} {background_g} {background_b} rg\n"
                ));
                content.push_str(
                    &ctx.text
                        .pdf_writer
                        .snap_border_box(PdfRect::new(rx, ry, rw2, rh))
                        .rounded(run.border_radii)
                        .path_or_rect(),
                );
                content.push_str("f\n");
            }
            border_pieces.add(run, x, run_width, text_y);

            decoration.paint_text(
                content,
                crate::layout::text::line_primary_font_size(&merged),
                ctx.text.prepared_custom_fonts,
                0.0,
                ctx.text.pdf_writer,
                ctx.text.page_images,
            );

            if let Some(annotation) = text_run_link_annotation(
                run,
                ctx.text.pdf_writer.painted_rect(PdfRect::new(
                    x,
                    line_annotation_bottom,
                    run_width,
                    line_annotation_height,
                )),
            ) {
                ctx.text.push_annotation(annotation);
            }

            x += run_width;
        }
        border_pieces.paint(content, ctx);

        // Paint deferred relatively positioned inline boxes above the line's
        // in-flow content, preserving their source order.
        for (run, inline, box_x, font_size, run_line_height) in deferred_inline {
            render_run_inline_box(
                content,
                run,
                inline,
                InlineBoxSlot {
                    x: box_x,
                    baseline: text_y,
                    line: InlineBoxLine {
                        top: line_top_y,
                        bottom: line_bottom_y,
                        text_top: line_text_top_y,
                        text_bottom: line_text_bottom_y,
                        x_height_ratio: line_primary_x_height_ratio(&merged, ctx.text.custom_fonts),
                    },
                    font_size,
                    line_height: run_line_height,
                },
                ctx,
            );
        }
    }
}

fn cell_content_top(cell: &CellBox, row_y: f32, row_height: f32) -> f32 {
    let offset = cell.content_block_offset(row_height);
    row_y - offset - cell.box_model.content_insets.top
}

#[cfg(test)]
mod table_cell_alignment_tests {
    use super::*;

    #[test]
    fn middle_cell_rounds_a_half_css_pixel_remainder_toward_block_start() {
        let cell = CellBox {
            content: crate::layout::cells::CellContent {
                lines: vec![TextLine {
                    // A 14px font at line-height: 1.5 occupies 21 CSS pixels.
                    height: 21.0 * crate::fonts::PT_PER_CSS_PX,
                    ..Default::default()
                }],
                ..Default::default()
            },
            alignment: crate::layout::cells::CellAlignment {
                block: VerticalAlign::Middle,
                ..Default::default()
            },
            ..Default::default()
        };

        // A 40px row has 19px of surplus. Chrome assigns its half-pixel to
        // the block-start side, so the content begins 10px below the top.
        assert_eq!(
            cell_content_top(
                &cell,
                40.0 * crate::fonts::PT_PER_CSS_PX,
                40.0 * crate::fonts::PT_PER_CSS_PX,
            ),
            30.0 * crate::fonts::PT_PER_CSS_PX,
        );
    }
}

#[cfg(test)]
mod test_support;
#[cfg(test)]
pub(super) use test_support::*;
