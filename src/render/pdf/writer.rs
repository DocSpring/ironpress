use super::*;

#[derive(Clone, Copy)]
pub(crate) struct RenderOpts {
    pub compress: bool,
    pub jpeg_quality: u8,
    pub auto_resize_images: bool,
    /// One conversion-owned policy for all raster fallbacks. Keeping this
    /// grouped prevents render paths from silently choosing their own density.
    pub raster_quality: crate::style::raster_quality::RasterQuality,
    /// Skip embedding raster images fully covered by a later fully-opaque
    /// rectangular element (default false). Conservative; zero visual change.
    pub occlusion_cull: bool,
    /// Turn form controls into fillable AcroForm fields (default false)
    pub pdf_forms: bool,
}

impl Default for RenderOpts {
    fn default() -> Self {
        Self {
            compress: true,
            jpeg_quality: DEFAULT_JPEG_QUALITY,
            auto_resize_images: true,
            raster_quality: crate::style::raster_quality::RasterQuality::default(),
            occlusion_cull: false,
            pdf_forms: false,
        }
    }
}

/// Minimal PDF writer that produces valid PDF files.
#[derive(Default)]
pub(crate) struct PdfWriter {
    pub(super) resources: crate::security::resources::ResourceLoader,
    pub(super) objects: Vec<String>,
    /// Raw binary objects stored separately (index corresponds to objects slot).
    pub(super) binary_objects: std::collections::HashMap<usize, Vec<u8>>,
    /// Document images keyed by source identity and final display sampling.
    pub(super) layout_image_objects:
        std::collections::HashMap<super::writer_images::LayoutImageCacheKey, usize>,
    pub(super) page_ids: Vec<usize>,
    /// Annotation object IDs grouped by page index.
    pub(super) page_annotations: Vec<Vec<usize>>,
    /// Image references grouped by page index.
    pub(super) page_images: Vec<Vec<ImageRef>>,
    /// ExtGState entries (name, opacity) grouped by page index.
    pub(super) page_ext_gstates: Vec<Vec<(String, f32)>>,
    /// Shading dictionary entries grouped by page index.
    pub(super) page_shadings: Vec<Vec<ShadingEntry>>,
    /// Function-based conic shadings are document-global resources, like the shared
    /// page resource dictionary that names them.
    pub(super) conic_shadings: Vec<ConicShadingEntry>,
    /// Custom TrueType font entries.
    pub(super) custom_font_entries: Vec<CustomFontEntry>,
    /// CSS `mask-image` transparency-group masks. Names are global because PDF
    /// page resource dictionaries share the entries.
    pub(super) soft_mask_gstates: Vec<PdfSoftMaskGState>,
    /// Repeated-background pattern cells and their page-visible local forms.
    pub(super) tiling_patterns: Vec<PdfTilingPatternEntry>,
    /// Direct CSS gradients whose Pattern matrix lives in default page space.
    pub(super) pdf_patterns: Vec<PdfPatternEntry>,
    /// Page-visible Forms whose content needs an exact, acyclic resource map.
    pub(super) local_forms: Vec<PdfLocalFormEntry>,
    pub(super) svg_defs: crate::parser::svg::SvgDefs,
    pub(super) opts: RenderOpts,
    pub(super) page_content_transform: PageContentTransform,
    pub(super) graphics_state: PdfGraphicsState,
    /// AcroForm fields (`RenderOpts::pdf_forms`)
    pub(super) forms: FormWriterState,
}

/// Renderer-owned subset of the PDF graphics state which cannot be recovered
/// from a resource after that resource has been registered.
#[derive(Debug, Clone, Copy, Default)]
pub(super) struct PdfGraphicsState {
    /// CSS paint-space transform from local layout coordinates into page layout
    /// space. PDF shading-pattern matrices are defined in default user space,
    /// so they must explicitly inherit this matrix from every ancestor.
    local_to_layout: PdfMatrix,
    box_paint_grid: BoxPaintGrid,
    /// The part of the page the `overflow` clips being painted inside leave visible, where link annotations are
    /// written (the bounding box of each clip through the transforms around it). `None` when nothing clips
    annotation_clip: Option<PdfRect>,
}

impl PdfWriter {
    #[cfg(test)]
    pub(super) fn new() -> Self {
        Self::default()
    }

    pub(super) fn with_resources(resources: crate::security::resources::ResourceLoader) -> Self {
        Self {
            resources,
            ..Self::default()
        }
    }

    pub(crate) fn load_document_resource(
        &mut self,
        source: &str,
    ) -> Option<crate::security::resources::LoadedResource> {
        self.resources.load_document_resource(source)
    }

    pub(super) fn resolve_background_svg(
        &mut self,
        background: &crate::layout::helpers::BackgroundFields,
    ) -> Option<crate::parser::svg::SvgTree> {
        background.svg.clone().or_else(|| {
            background.raster_source.as_deref().and_then(|source| {
                crate::layout::images::build_raster_background_tree(&mut self.resources, source)
            })
        })
    }

    pub(super) fn paint_matrix(&self, local: PdfMatrix) -> PdfMatrix {
        self.graphics_state.local_to_layout * local
    }

    /// Where a rectangle in the current paint group's coordinates lands on the page's layout: through every CSS
    /// transform of the boxes being painted, as the box's own paint does (the smallest rectangle around it)
    pub(super) fn painted_rect(&self, rect: PdfRect) -> PdfRect {
        self.painted_rect_within(rect, None)
    }

    /// Where a box's `rect` is painted on the page, through its own transform (the paint group it hasn't entered yet)
    /// as well as the ones it's inside: their bounding box
    pub(super) fn painted_rect_within(
        &self,
        rect: PdfRect,
        own_transform: Option<PdfMatrix>,
    ) -> PdfRect {
        let matrix = own_transform.map_or(self.graphics_state.local_to_layout, |own| {
            self.graphics_state.local_to_layout * own
        });
        if matrix == PdfMatrix::IDENTITY {
            return rect;
        }
        let corners = [
            PdfPoint::new(rect.left, rect.bottom),
            PdfPoint::new(rect.right(), rect.bottom),
            PdfPoint::new(rect.left, rect.top()),
            PdfPoint::new(rect.right(), rect.top()),
        ]
        .map(|corner| matrix.transform_point(corner));
        let left = corners
            .iter()
            .map(|corner| corner.x)
            .fold(f32::INFINITY, f32::min);
        let right = corners
            .iter()
            .map(|corner| corner.x)
            .fold(f32::NEG_INFINITY, f32::max);
        let bottom = corners
            .iter()
            .map(|corner| corner.y)
            .fold(f32::INFINITY, f32::min);
        let top = corners
            .iter()
            .map(|corner| corner.y)
            .fold(f32::NEG_INFINITY, f32::max);
        PdfRect::new(left, bottom, right - left, top - bottom)
    }

    /// Enter an `overflow` clip of `bounds` in the current paint group's coordinates: link annotations written inside
    /// it cover only what it leaves visible, as Chromium writes them. Returns the clip to restore after it
    pub(super) fn enter_annotation_clip(&mut self, bounds: PdfRect) -> Option<PdfRect> {
        let prior = self.graphics_state.annotation_clip;
        let painted = self.painted_rect(bounds);
        let visible = match prior {
            None => painted,
            Some(outer) => outer.intersection(painted).unwrap_or(PdfRect::new(
                painted.left,
                painted.bottom,
                0.0,
                0.0,
            )),
        };
        self.graphics_state.annotation_clip = Some(visible);
        prior
    }

    pub(super) fn restore_annotation_clip(&mut self, prior: Option<PdfRect>) {
        self.graphics_state.annotation_clip = prior;
    }

    /// The part of a link annotation's `rect` (on the page's layout) that the clips around it leave visible, or
    /// `None` when they hide all of it
    pub(super) fn visible_annotation_rect(&self, rect: PdfRect) -> Option<PdfRect> {
        match self.graphics_state.annotation_clip {
            None => Some(rect),
            Some(clip) => clip.intersection(rect),
        }
    }

    pub(super) fn has_active_paint_transform(&self) -> bool {
        self.graphics_state.local_to_layout != PdfMatrix::IDENTITY
    }

    /// Select the browser-like serialization space for the current page.
    ///
    /// CSS transforms operate on the same top-down CSS coordinates as the
    /// content they transform. Keep that coordinate system inside an active
    /// transform as well; falling back to bottom-up points would reverse the
    /// box's block axis before the transform is applied.
    pub(super) fn current_content_space(&self) -> PdfContentSpace {
        PdfContentSpace::page_css(self.page_content_transform)
    }

    /// Snap a border box to the current paint grid, as `resolve_box_geometry`
    /// snaps a box's edges.
    pub(super) fn snap_border_box(&self, border_box: PdfRect) -> PdfRect {
        self.graphics_state
            .box_paint_grid
            .resolve(self.page_content_transform, border_box)
    }

    /// A cursor for a text block's baselines from `content_top`, snapping them on the current paint grid, as its
    /// boxes are
    pub(super) fn text_baseline_cursor(&self, content_top: f32) -> TextBaselineCursor {
        TextBaselineCursor::new(
            content_top,
            self.page_content_transform,
            self.graphics_state.box_paint_grid,
        )
    }

    pub(super) fn resolve_box_geometry(&self, layout: LayoutBoxGeometry) -> BoxPaintGeometry {
        layout.for_paint(
            self.page_content_transform,
            self.graphics_state.box_paint_grid,
        )
    }

    /// Root an outer atomic inline subtree at its layout position snapped to the page's CSS-pixel grid, where
    /// Chromium paints the box.
    ///
    /// Nested atomic boxes inherit the existing grid: the subtree is one paint
    /// unit in the parent inline formatting context, not a sequence of
    /// independently shifted descendants.
    pub(super) fn enter_atomic_inline_paint_grid(
        &mut self,
        origin: PdfPoint,
    ) -> Option<BoxPaintGrid> {
        let origin = self
            .page_content_transform
            .snap_layout_box(PdfRect::new(origin.x, origin.y, 0.0, 0.0))
            .top_left();
        matches!(self.graphics_state.box_paint_grid, BoxPaintGrid::Page).then(|| {
            std::mem::replace(
                &mut self.graphics_state.box_paint_grid,
                BoxPaintGrid::AtomicInline(origin),
            )
        })
    }

    pub(super) fn restore_box_paint_grid(&mut self, grid: BoxPaintGrid) {
        self.graphics_state.box_paint_grid = grid;
    }

    pub(super) fn transformed_paint_space(&self, page_bounds: PdfRect) -> Option<PdfPaintSpace> {
        let local_to_layout = self.graphics_state.local_to_layout;
        (local_to_layout != PdfMatrix::IDENTITY)
            .then(|| PdfPaintSpace::new(local_to_layout, self.page_content_transform, page_bounds))
    }

    pub(super) fn enter_paint_transform(&mut self, local: PdfMatrix) -> PdfMatrix {
        let prior = self.graphics_state.local_to_layout;
        self.graphics_state.local_to_layout = prior * local;
        prior
    }

    pub(super) fn restore_paint_transform(&mut self, prior: PdfMatrix) {
        self.graphics_state.local_to_layout = prior;
    }

    pub(super) fn next_id(&self) -> usize {
        self.objects.len() + 1
    }

    pub(crate) fn image_dimensions(&self, object_id: usize) -> Option<RasterDimensions> {
        let object = self.objects.get(object_id.checked_sub(1)?)?;
        let value_after = |name| {
            object
                .split_ascii_whitespace()
                .zip(object.split_ascii_whitespace().skip(1))
                .find_map(|(key, value)| (key == name).then(|| value.parse().ok()).flatten())
        };
        Some(RasterDimensions {
            width: value_after("/Width")?,
            height: value_after("/Height")?,
        })
    }

    pub(super) fn add_conic_shading(
        &mut self,
        domain: PdfRect,
        function: ConicShadingFunction,
    ) -> String {
        let name = format!("ShConic{}", self.conic_shadings.len());
        self.conic_shadings.push(ConicShadingEntry {
            name: name.clone(),
            domain,
            function,
        });
        name
    }

    pub(super) fn add_transparency_group_form(
        &mut self,
        stream: String,
        paint_box: PdfRect,
    ) -> ImageRef {
        self.add_local_form(
            stream,
            paint_box,
            Some("/Group << /Type /Group /S /Transparency /CS /DeviceRGB /I true /K false >>"),
        )
    }

    pub(super) fn register_soft_mask(&mut self, name: String, form_id: usize) {
        self.soft_mask_gstates.push(PdfSoftMaskGState {
            name,
            form_id,
            mode: PdfSoftMaskMode::Luminosity,
        });
    }

    /// Register a transparency form whose composited alpha is the mask.
    ///
    /// Layered CSS masks must retain alpha while their sources are composed;
    /// reducing the final group to luminosity would lose that compositing
    /// result for overlapping translucent coverage.
    pub(super) fn register_alpha_soft_mask(&mut self, name: String, form_id: usize) {
        self.soft_mask_gstates.push(PdfSoftMaskGState {
            name,
            form_id,
            mode: PdfSoftMaskMode::Alpha,
        });
    }

    pub(super) fn add_plain_local_form(&mut self, stream: String, paint_box: PdfRect) -> ImageRef {
        self.add_local_form(stream, paint_box, None)
    }

    fn add_local_form(
        &mut self,
        stream: String,
        paint_box: PdfRect,
        group: Option<&str>,
    ) -> ImageRef {
        let form_id = self.next_id();
        let [x, x1, y, y1] = paint_box.xy_domain();
        let resources = PdfResourceUsage::from_stream(&stream);
        let bytes = stream.into_bytes();
        let group = group.map_or_else(String::new, |group| format!(" {group}"));
        self.objects.push(format!(
            "{form_id} 0 obj\n<< /Type /XObject /Subtype /Form /FormType 1 /BBox [{x} {y} {x1} {y1}]{group} /Resources __IP_LOCAL_FORM_RESOURCES__ /Length {len} >>\nstream\n",
            len = bytes.len(),
        ));
        self.binary_objects.insert(form_id, bytes);
        self.local_forms
            .push(PdfLocalFormEntry { form_id, resources });
        ImageRef {
            name: format!("Fm{form_id}"),
            obj_id: form_id,
        }
    }
}

/// The PDF soft-mask channel extracted from its transparency group.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(super) enum PdfSoftMaskMode {
    #[default]
    Luminosity,
    Alpha,
}

impl PdfSoftMaskMode {
    pub(super) const fn pdf_name(self) -> &'static str {
        match self {
            Self::Luminosity => "/Luminosity",
            Self::Alpha => "/Alpha",
        }
    }
}

/// One named PDF transparency-group mask.
pub(super) struct PdfSoftMaskGState {
    pub(super) name: String,
    pub(super) form_id: usize,
    pub(super) mode: PdfSoftMaskMode,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nested_paint_scopes_compose_and_restore_resource_matrices() {
        let mut writer = PdfWriter::new();
        let parent = PdfMatrix::rotate_around(PdfPoint::new(40.0, 30.0), 0.5, 0.866_025_4);
        let child = PdfMatrix::translate(PdfPoint::new(7.0, -3.0));
        let resource = PdfMatrix::scale(PdfVector::new(0.75, -0.75));

        let root = writer.enter_paint_transform(parent);
        let parent_state = writer.enter_paint_transform(child);
        assert_eq!(writer.paint_matrix(resource), parent * child * resource);

        writer.restore_paint_transform(parent_state);
        assert_eq!(writer.paint_matrix(resource), parent * resource);

        writer.restore_paint_transform(root);
        assert_eq!(writer.paint_matrix(resource), resource);
    }
}
