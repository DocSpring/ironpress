//! Drawing an SVG tree into a box, shared by block `<svg>` elements and inline
//! `<svg>` boxes in lines of text.

use super::*;

/// The page resources an SVG drawing may add to.
pub(super) struct SvgDrawResources<'a> {
    pub(super) shadings: &'a mut Vec<ShadingEntry>,
    pub(super) shading_counter: &'a mut usize,
    pub(super) ext_gstates: &'a mut Vec<(String, f32)>,
    pub(super) pdf_writer: &'a mut PdfWriter,
    pub(super) page_images: &'a mut Vec<ImageRef>,
    pub(super) custom_fonts: &'a HashMap<String, TtfFont>,
    pub(super) prepared_custom_fonts: &'a PreparedCustomFonts,
}

/// Draw `tree` into `area` (PDF coordinates), scaled and placed by the box's
/// `object-fit` and `object-position`, and clipped to the area.
pub(super) fn draw_svg_in_box(
    content: &mut String,
    tree: &crate::parser::svg::SvgTree,
    replaced: crate::layout::elements::ReplacedContent,
    area: PdfRect,
    resources: SvgDrawResources<'_>,
) {
    let (content_w, content_h) = (area.width, area.height);
    content.push_str("q\n");
    // Position on page and flip y-axis for SVG coordinates
    content.push_str(&format!("1 0 0 -1 {} {} cm\n", area.left, area.top()));
    if let Some(placement) = crate::render::svg_geometry::compute_replaced_svg_placement(
        tree,
        crate::types::Size::new(content_w, content_h),
        replaced,
    ) {
        content.push_str("q\n");
        content.push_str(&format!("0 0 {content_w} {content_h} re\nW n\n"));
        content.push_str(&format!(
            "{sx} 0 0 {sy} {tx} {ty} cm\n",
            sx = placement.scale_x,
            sy = placement.scale_y,
            tx = placement.translate_x,
            ty = placement.translate_y,
        ));
        let mut image_sink = SvgPageImageSink {
            pdf_writer: resources.pdf_writer,
            page_images: resources.page_images,
        };
        let mut svg_resources = crate::render::svg_to_pdf::SvgPdfResources {
            shadings: resources.shadings,
            shading_counter: resources.shading_counter,
            ext_gstates: Some(resources.ext_gstates),
            image_sink: Some(&mut image_sink),
            raster_scale_x: placement.scale_x.abs(),
            raster_scale_y: placement.scale_y.abs(),
            custom_fonts: Some(resources.custom_fonts),
            prepared_custom_fonts: Some(resources.prepared_custom_fonts),
        };
        crate::render::svg_to_pdf::render_svg_tree_with_resources(
            tree,
            content,
            &mut svg_resources,
        );
        content.push_str("Q\n");
    }
    content.push_str("Q\n");
}
