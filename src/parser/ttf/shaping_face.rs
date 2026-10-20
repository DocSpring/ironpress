//! The shaping view a parsed font owns over its own bytes.

use std::fmt;
use std::sync::Arc;

/// A `harfrust::Font` built over a parsed font's bytes.
///
/// Building the font parses its shaping tables and harfrust caches the layout
/// data inside it, so shaping a run only wraps it in a `ShaperFont`. Layout
/// shapes every run at least twice — once to measure, again to paint — which
/// is why the parsed font keeps this value instead of rebuilding it per run.
pub(crate) struct ShapingFace {
    font: harfrust::Font,
    bytes: usize,
}

impl ShapingFace {
    /// Build the shaping font for `face_index` within `data`.
    ///
    /// `None` means harfrust rejected the bytes. Shaping then declines the run
    /// rather than substituting another face.
    pub(crate) fn parse(data: Arc<Vec<u8>>, face_index: u32) -> Option<Self> {
        let bytes = data.len();
        let blob: Arc<dyn AsRef<[u8]> + Send + Sync> = data;
        let font = harfrust::Font::new(blob, face_index)?;
        Some(Self { font, bytes })
    }

    /// The shaping font.
    pub(crate) fn font(&self) -> &harfrust::Font {
        &self.font
    }
}

impl fmt::Debug for ShapingFace {
    /// Dumping the parsed tables would bury the surrounding font. The buffer
    /// size is the part that identifies which face a font is carrying.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ShapingFace")
            .field("bytes", &self.bytes)
            .finish()
    }
}
