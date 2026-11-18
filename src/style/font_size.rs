//! How Chromium computes `font-size` beyond plain lengths: the absolute-size
//! keywords (`small`, `x-large`...), the relative ones (`smaller`, `larger`),
//! and the smaller size it gives text whose only font family is the generic
//! `monospace` (13px where other text is 16px).
//!
//! Blink keeps two facts with every computed size: the keyword it came from,
//! if any, and whether an absolute length (`px`, `pt`, a viewport unit, `rem`, `calc()`)
//! decided it anywhere up the tree. When an element's family changes between
//! the generic `monospace` and anything else and no absolute length decided
//! its size, Blink resizes it: a keyword size is looked up again in the
//! monospace (or proportional) column, and any other size is scaled by 13/16
//! (or back). That is why unstyled `<code>` and `<pre>` text is 13px, while
//! `<code>` inside `body { font-size: 12pt }` keeps 12pt.

const PT_PER_PX: f32 = 0.75;

/// Chromium's default proportional and monospace font sizes, in px.
const DEFAULT_PX: f32 = 16.0;
const DEFAULT_MONOSPACE_PX: f32 = 13.0;

/// Blink's standard-mode keyword sizes, `xx-small` to `xxx-large`, for the
/// default proportional (16px) and monospace (13px) sizes
/// (`kStrictFontSizeTable` in `font_size_functions.cc`).
const KEYWORD_PX: [f32; 8] = [9.0, 10.0, 13.0, 16.0, 18.0, 24.0, 32.0, 48.0];
const MONOSPACE_KEYWORD_PX: [f32; 8] = [9.0, 10.0, 12.0, 13.0, 14.0, 19.0, 26.0, 39.0];

/// An absolute-size keyword, as its index from `xx-small` (0) to `xxx-large`
/// (7).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct FontSizeKeyword(u8);

impl FontSizeKeyword {
    pub(crate) const MEDIUM: Self = Self(3);

    pub(crate) fn parse(raw: &str) -> Option<Self> {
        let index = match raw.trim().to_ascii_lowercase().as_str() {
            "xx-small" => 0,
            "x-small" => 1,
            "small" => 2,
            "medium" => 3,
            "large" => 4,
            "x-large" => 5,
            "xx-large" => 6,
            "xxx-large" => 7,
            _ => return None,
        };
        Some(Self(index))
    }

    /// The keyword's size in pt for text in the generic `monospace` family or
    /// any other.
    pub(crate) fn size(self, monospace: bool) -> f32 {
        let table = if monospace {
            &MONOSPACE_KEYWORD_PX
        } else {
            &KEYWORD_PX
        };
        table[usize::from(self.0)] * PT_PER_PX
    }
}

/// `smaller` and `larger` (CSS Fonts 4 §2.5): Blink scales the parent's size
/// by 1.2 and keeps whether an absolute length decided it.
pub(crate) fn relative_keyword_factor(raw: &str) -> Option<f32> {
    match raw.trim().to_ascii_lowercase().as_str() {
        "smaller" => Some(1.0 / 1.2),
        "larger" => Some(1.2),
        _ => None,
    }
}

/// Whether `raw` is a `font-size` keyword, absolute or relative.
pub(crate) fn is_font_size_keyword(raw: &str) -> bool {
    FontSizeKeyword::parse(raw).is_some() || relative_keyword_factor(raw).is_some()
}

/// Where a computed font size came from, which decides how Blink resizes it
/// when the family changes to or from the generic `monospace`. Inherited with
/// the size.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct FontSizeOrigin {
    /// The absolute-size keyword it came from, if it did.
    pub(crate) keyword: Option<FontSizeKeyword>,
    /// Whether an absolute length decided it, here or in an ancestor.
    pub(crate) absolute: bool,
}

impl Default for FontSizeOrigin {
    /// The initial value, `medium`.
    fn default() -> Self {
        Self::keyword(FontSizeKeyword::MEDIUM)
    }
}

impl FontSizeOrigin {
    pub(crate) const fn keyword(keyword: FontSizeKeyword) -> Self {
        Self {
            keyword: Some(keyword),
            absolute: false,
        }
    }

    /// A size computed from the parent's: font-relative units, percentages
    /// and `smaller`/`larger`.
    pub(crate) const fn relative_to(parent: Self) -> Self {
        Self {
            keyword: None,
            absolute: parent.absolute,
        }
    }

    pub(crate) const ABSOLUTE: Self = Self {
        keyword: None,
        absolute: true,
    };
}

/// An element's font size before Blink resizes it for a change of generic
/// family: the size its own `font-size` declared, or the parent's.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct DeclaredFontSize {
    pub(crate) size: f32,
    pub(crate) origin: FontSizeOrigin,
}

/// The element's used size (CSS `font-size` computed value) from its declared
/// one, following Blink's `FontBuilder::CheckForGenericFamilyChange`.
/// `declared_here` is whether the element's own `font-size` produced it.
pub(crate) fn size_for_generic_family(
    declared: DeclaredFontSize,
    declared_here: bool,
    monospace: bool,
    parent_monospace: bool,
) -> f32 {
    if declared.origin.absolute {
        return declared.size;
    }
    if let Some(keyword) = declared.origin.keyword {
        // A keyword names a size in the column of the element's own family.
        if declared_here || monospace != parent_monospace {
            return keyword.size(monospace);
        }
        return declared.size;
    }
    if monospace == parent_monospace {
        return declared.size;
    }
    let ratio = DEFAULT_MONOSPACE_PX / DEFAULT_PX;
    if monospace {
        declared.size * ratio
    } else {
        declared.size / ratio
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn declared(size_px: f32, origin: FontSizeOrigin) -> DeclaredFontSize {
        DeclaredFontSize {
            size: size_px * PT_PER_PX,
            origin,
        }
    }

    #[test]
    fn keywords_use_blinks_table_for_each_family() {
        let small = FontSizeKeyword::parse("SMALL").expect("a keyword");
        assert_eq!(small.size(false), 13.0 * PT_PER_PX);
        assert_eq!(small.size(true), 12.0 * PT_PER_PX);
        assert_eq!(FontSizeKeyword::MEDIUM.size(true), 13.0 * PT_PER_PX);
        assert_eq!(FontSizeKeyword::parse("smaller"), None);
    }

    #[test]
    fn monospace_inherited_medium_is_13px() {
        let size = size_for_generic_family(
            declared(16.0, FontSizeOrigin::default()),
            false,
            true,
            false,
        );
        assert_eq!(size, 13.0 * PT_PER_PX);
    }

    #[test]
    fn monospace_in_relative_sizes_scales_by_13_16() {
        let h1 = FontSizeOrigin::relative_to(FontSizeOrigin::default());
        let size = size_for_generic_family(declared(32.0, h1), false, true, false);
        assert_eq!(size, 26.0 * PT_PER_PX);
        let back = size_for_generic_family(declared(26.0, h1), false, false, true);
        assert_eq!(back, 32.0 * PT_PER_PX);
    }

    #[test]
    fn an_absolute_size_anywhere_above_keeps_monospace_text_its_size() {
        let size =
            size_for_generic_family(declared(16.0, FontSizeOrigin::ABSOLUTE), false, true, false);
        assert_eq!(size, 16.0 * PT_PER_PX);
    }

    #[test]
    fn the_same_family_as_the_parent_keeps_the_size() {
        let size =
            size_for_generic_family(declared(13.0, FontSizeOrigin::default()), false, true, true);
        assert_eq!(size, 13.0 * PT_PER_PX);
    }
}
