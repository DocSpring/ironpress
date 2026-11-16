//! Chromium's second layout for a document wider than its pages.
//!
//! Blink lays a document out on its pages, and when the document is wider it
//! lays it out again on pages whose area is as wide as the document, at most
//! 1.5 times the page width, keeping the page's proportions. It then scales
//! that layout down onto the physical pages and clips what's still wider. Text
//! therefore wraps at the wider width, and each page holds more. The parity
//! fixtures `paged-print-fit-*` hold Chrome's output: content that needs a
//! scale of 0.635 prints at 2/3.

use super::PrintContentScale;
use crate::layout::engine::Page;
use crate::types::{Margin, PageSize};

/// The widest page area, relative to the physical one, that a document is
/// laid out on before it's scaled down.
const MAXIMUM_WIDENING: f32 = 1.5;

/// How many times wider and taller than the physical page area a document
/// is laid out.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct PrintFitWidening(f32);

impl PrintFitWidening {
    /// The widening for a document whose layout on the physical pages needs
    /// shrinking to fit, or `None` when it fits.
    pub(crate) fn for_pages(pages: &[Page]) -> Option<Self> {
        let scale = pages.first()?.print_content_scale.factor();
        (scale.is_finite() && scale > 0.0 && scale < 1.0)
            .then(|| Self((1.0 / scale).min(MAXIMUM_WIDENING)))
    }

    pub(crate) const fn factor(self) -> f32 {
        self.0
    }

    /// The physical page size of a widened page.
    pub(crate) fn physical_page_size(self, widened: PageSize, page_margin: Margin) -> PageSize {
        PageSize::new(
            page_margin.horizontal() + (widened.width - page_margin.horizontal()) / self.0,
            page_margin.vertical() + (widened.height - page_margin.vertical()) / self.0,
        )
    }

    /// Give each paginated page its physical geometry back.
    pub(crate) fn restore_physical_page_geometry(self, pages: &mut [Page]) {
        for page in pages {
            page.geometry = page
                .geometry
                .map(|geometry| geometry.with_page_area_scaled(1.0 / self.0));
        }
    }

    /// Scale every page's content onto its physical page, and fit the
    /// propagated canvas to each physical page area.
    pub(crate) fn assign_page_print_scales(self, pages: &mut [Page]) {
        let scale = PrintContentScale(1.0 / self.0);
        for page in pages {
            page.print_content_scale = scale;
            let geometry = page.geometry;
            for (_, element) in &mut page.elements {
                if let Some(background) = element.page_area_background_mut() {
                    if let Some(geometry) = geometry {
                        background.fit_page_area(geometry.page_area_in_flow_space());
                    }
                    background.apply_print_content_scale(scale);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_widened_page_size_maps_back_to_the_physical_one() {
        let widening = PrintFitWidening(1.5);
        let margin = Margin::new(10.0, 20.0, 30.0, 40.0);
        let physical = PageSize::new(300.0, 500.0);
        let widened = PageSize::new(60.0 + 240.0 * 1.5, 40.0 + 460.0 * 1.5);
        let restored = widening.physical_page_size(widened, margin);
        assert!((restored.width - physical.width).abs() < 1e-3);
        assert!((restored.height - physical.height).abs() < 1e-3);
    }
}
