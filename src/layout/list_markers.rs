use crate::style::computed::{CounterStyle, CounterStyleSystem, ListStyleType, VerticalAlign};
use crate::types::{Color, CornerRadii, EdgeSizes};

use super::engine::{CenteredStroke, InlineBox, InlineBoxPaint};

/// Where a built-in unordered-list bullet sits: hanging before the item's
/// content (`outside`) or in its first line (`inside`), which Chromium gives
/// different margins.
#[derive(Debug, Clone, Copy, Default)]
pub(crate) enum BuiltInBulletSlot {
    #[default]
    List,
    Inside,
}

/// A predefined unordered-list marker whose appearance is supplied by the UA.
///
/// These are shapes rather than Unicode stand-ins. Keeping that distinction at
/// the list-marker boundary prevents font fallback, glyph metrics, or text
/// shaping from changing their paint.
#[derive(Debug, Clone, Copy)]
enum BuiltInBulletShape {
    Disc,
    Circle,
    Square,
}

impl BuiltInBulletShape {
    const CIRCLE_STROKE_WIDTH: f32 = crate::fonts::PT_PER_CSS_PX;

    fn from_style(style: &ListStyleType) -> Option<Self> {
        match style {
            ListStyleType::Disc => Some(Self::Disc),
            ListStyleType::Circle => Some(Self::Circle),
            ListStyleType::Square => Some(Self::Square),
            _ => None,
        }
    }

    fn paint(self, color: Color, size: f32) -> InlineBoxPaint {
        let border_radii = match self {
            Self::Disc | Self::Circle => CornerRadii::circular(size / 2.0),
            Self::Square => CornerRadii::default(),
        };
        InlineBoxPaint {
            background_color: matches!(self, Self::Disc | Self::Square).then_some(color),
            border_radii,
            centered_stroke: matches!(self, Self::Circle)
                .then(|| CenteredStroke::solid(Self::CIRCLE_STROKE_WIDTH, color)),
            ..InlineBoxPaint::default()
        }
    }
}

/// Geometry shared by Chromium's built-in list bullets, from the marker
/// font's ascent in whole CSS pixels (Blink's `ListMarker::WidthOfSymbol`,
/// `RelativeSymbolMarkerRect`, `InlineMarginsForOutside` and
/// `InlineMarginsForInside`).
///
/// Painted bounds, inline advance, and baseline position form one unit. Using
/// Unicode glyph advances would move the three predefined shapes to different
/// positions even though Chromium places them in the same marker slot.
#[derive(Debug, Clone, Copy, PartialEq)]
struct BuiltInBulletMetrics {
    /// The bullet's width and height.
    size: f32,
    /// From the marker's inline start to the bullet.
    inset: f32,
    /// The marker's whole inline advance: for an outside marker, how far its
    /// start hangs before the list item's content edge.
    advance: f32,
    /// From the bullet's top down to the baseline.
    top_above_baseline: f32,
}

impl BuiltInBulletMetrics {
    /// Blink's padding between an outside symbol marker and the content.
    const MARKER_PADDING_PX: i32 = 7;

    /// `ascent_px`: the marker font's ascent, rounded to a whole CSS pixel as
    /// Blink's `FontMetrics::Ascent` rounds it; `font_size` in points.
    fn from_ascent(ascent_px: i32, font_size: f32, slot: BuiltInBulletSlot) -> Self {
        let px = crate::fonts::PT_PER_CSS_PX;
        let two_thirds = ascent_px * 2 / 3;
        let size = (two_thirds + 1) / 2;
        // The bullet sits 1px into the marker box, whose top is the text top.
        let top_below_text_top = 3 * (ascent_px - two_thirds) / 2;
        let advance = match slot {
            // The box starts this far before the content edge.
            BuiltInBulletSlot::List => (two_thirds + Self::MARKER_PADDING_PX + 1) as f32 * px,
            // An inside marker starts 1px early, is two wider than its bullet
            // and ends with a 1em margin.
            BuiltInBulletSlot::Inside => (size + 1) as f32 * px + font_size,
        };
        let inset = match slot {
            BuiltInBulletSlot::List => px,
            BuiltInBulletSlot::Inside => 0.0,
        };
        Self {
            size: size as f32 * px,
            inset,
            advance,
            top_above_baseline: (ascent_px - top_below_text_top) as f32 * px,
        }
    }

    fn for_font(
        style: &crate::style::computed::ComputedStyle,
        fonts: &std::collections::HashMap<String, crate::parser::ttf::TtfFont>,
        slot: BuiltInBulletSlot,
    ) -> Self {
        let font_size = super::text::used_font_size(style, fonts);
        let family = super::text::resolve_style_font_family(style, fonts);
        let metrics = crate::fonts::font_line_metrics(
            &family,
            font_size,
            style.font_weight.is_bold(),
            style.font_style.is_slanted(),
            fonts,
        );
        let ascent_px = (metrics.ascent / crate::fonts::PT_PER_CSS_PX).round() as i32;
        Self::from_ascent(ascent_px.max(0), font_size, slot)
    }
}

/// Build a predefined `disc`, `circle`, or `square` marker as an atomic box,
/// in the marker's own style.
///
/// The returned border box owns the complete marker geometry and inline slot.
/// Author-provided marker strings stay on the textual path.
pub(crate) fn build_list_bullet_marker(
    marker_style: &crate::style::computed::ComputedStyle,
    fonts: &std::collections::HashMap<String, crate::parser::ttf::TtfFont>,
    slot: BuiltInBulletSlot,
) -> Option<InlineBox> {
    let shape = BuiltInBulletShape::from_style(&marker_style.list_style_type)?;
    let metrics = BuiltInBulletMetrics::for_font(marker_style, fonts, slot);
    Some(InlineBox {
        width: metrics.size,
        height: metrics.size,
        margin_left: metrics.inset,
        margin_right: (metrics.advance - metrics.inset - metrics.size).max(0.0),
        paint: shape.paint(marker_style.color, metrics.size),
        vertical_align: VerticalAlign::Baseline,
        baseline_ascent: Some(metrics.top_above_baseline),
        padding: EdgeSizes::ZERO,
        symbol_marker: true,
        ..InlineBox::default()
    })
}

#[cfg(test)]
mod bullet_metrics_tests {
    use super::{BuiltInBulletMetrics, BuiltInBulletSlot};

    const PX: f32 = crate::fonts::PT_PER_CSS_PX;

    #[test]
    fn an_outside_bullet_at_a_22px_font_hangs_20px_before_the_content() {
        // ParitySans' ascent rounds to 20px at 22px. Chrome's oracle: a 7px
        // bullet whose left edge is 20px before the content edge, its top
        // 10px above the baseline.
        let metrics = BuiltInBulletMetrics::from_ascent(20, 22.0 * PX, BuiltInBulletSlot::List);
        assert_eq!(metrics.size, 7.0 * PX);
        assert_eq!(metrics.advance - metrics.inset, 20.0 * PX);
        assert_eq!(metrics.top_above_baseline, 10.0 * PX);
    }

    #[test]
    fn an_outside_bullet_at_a_10px_font_is_3px_and_14px_out() {
        // ParitySans' ascent rounds to 9px at 10px: `lists-counters-marker-
        // small-font-size` has Chrome's 3px disc centred 11.5px before the
        // content edge.
        let metrics = BuiltInBulletMetrics::from_ascent(9, 10.0 * PX, BuiltInBulletSlot::List);
        assert_eq!(metrics.size, 3.0 * PX);
        assert_eq!(metrics.advance, 14.0 * PX);
        assert_eq!(metrics.inset, PX);
        assert_eq!(metrics.top_above_baseline, 5.0 * PX);
    }

    #[test]
    fn an_inside_bullet_at_a_30px_font_takes_a_40px_slot() {
        let metrics = BuiltInBulletMetrics::from_ascent(26, 30.0 * PX, BuiltInBulletSlot::Inside);
        assert_eq!(metrics.size, 9.0 * PX);
        assert_eq!(metrics.advance, 40.0 * PX);
        assert_eq!(metrics.top_above_baseline, 13.0 * PX);
    }
}

pub(crate) fn format_list_marker(list_style_type: &ListStyleType, index: i32) -> String {
    match list_style_type {
        ListStyleType::Disc => "\u{2022} ".to_string(),
        ListStyleType::Circle => "\u{25E6} ".to_string(),
        ListStyleType::Square => "\u{25AA} ".to_string(),
        ListStyleType::Decimal => format!("{index}. "),
        ListStyleType::DecimalLeadingZero => {
            if index < 0 {
                format!("-{:02}. ", (index as i64).unsigned_abs())
            } else {
                format!("{index:02}. ")
            }
        }
        ListStyleType::LowerAlpha => format_positive_marker(index, to_alpha_lower),
        ListStyleType::UpperAlpha => format_positive_marker(index, to_alpha_upper),
        ListStyleType::LowerRoman => format_positive_marker(index, to_roman_lower),
        ListStyleType::UpperRoman => format_positive_marker(index, to_roman_upper),
        ListStyleType::CjkDecimal if index > 0 => {
            format!("{}、", to_cjk_decimal(index as usize))
        }
        ListStyleType::CjkDecimal => format!("{index}、"),
        ListStyleType::String(marker) => marker.clone(),
        ListStyleType::CounterStyle(style) => format_custom_counter(style, index, true),
        ListStyleType::Custom(_) => format!("{index}. "),
        ListStyleType::None => String::new(),
    }
}

fn format_positive_marker(index: i32, formatter: fn(usize) -> String) -> String {
    if index <= 0 {
        format!("{index}. ")
    } else {
        format!("{}. ", formatter(index as usize))
    }
}

pub(crate) fn to_alpha_lower(n: usize) -> String {
    if n == 0 {
        return "a".to_string();
    }
    let mut result = String::new();
    let mut val = n;
    while val > 0 {
        val -= 1;
        result.insert(0, (b'a' + (val % 26) as u8) as char);
        val /= 26;
    }
    result
}

pub(crate) fn to_alpha_upper(n: usize) -> String {
    to_alpha_lower(n).to_uppercase()
}

pub(crate) fn to_roman_lower(n: usize) -> String {
    let vals = [
        (1000, "m"),
        (900, "cm"),
        (500, "d"),
        (400, "cd"),
        (100, "c"),
        (90, "xc"),
        (50, "l"),
        (40, "xl"),
        (10, "x"),
        (9, "ix"),
        (5, "v"),
        (4, "iv"),
        (1, "i"),
    ];
    let mut result = String::new();
    let mut remaining = n;
    for &(value, numeral) in &vals {
        while remaining >= value {
            result.push_str(numeral);
            remaining -= value;
        }
    }
    if result.is_empty() {
        "0".to_string()
    } else {
        result
    }
}

pub(crate) fn to_roman_upper(n: usize) -> String {
    to_roman_lower(n).to_uppercase()
}

fn to_cjk_decimal(n: usize) -> String {
    const DIGITS: [&str; 10] = ["〇", "一", "二", "三", "四", "五", "六", "七", "八", "九"];
    if n == 0 {
        return DIGITS[0].to_string();
    }
    n.to_string()
        .chars()
        .filter_map(|ch| ch.to_digit(10).map(|d| DIGITS[d as usize]))
        .collect()
}

/// Format a counter value without the marker prefix or suffix.
pub(crate) fn format_counter_value(style: &ListStyleType, value: i32) -> String {
    // Roman/alpha styles are defined only for positive integers; other values
    // fall back to decimal according to CSS Counter Styles.
    if value <= 0 && !matches!(style, ListStyleType::CounterStyle(_)) {
        return value.to_string();
    }
    let n = value as usize;
    match style {
        ListStyleType::DecimalLeadingZero => format!("{n:02}"),
        ListStyleType::LowerAlpha => to_alpha_lower(n),
        ListStyleType::UpperAlpha => to_alpha_upper(n),
        ListStyleType::LowerRoman => to_roman_lower(n),
        ListStyleType::UpperRoman => to_roman_upper(n),
        ListStyleType::CjkDecimal => to_cjk_decimal(n),
        ListStyleType::CounterStyle(custom) => format_custom_counter(custom, value, false),
        _ => value.to_string(),
    }
}

fn format_custom_counter(style: &CounterStyle, value: i32, include_affixes: bool) -> String {
    let negative = value < 0;
    let abs_value = (value as i64).unsigned_abs() as usize;
    let mut representation = match style.system {
        CounterStyleSystem::Cyclic if !style.symbols.is_empty() => {
            let index = if abs_value == 0 {
                0
            } else {
                (abs_value - 1) % style.symbols.len()
            };
            style.symbols[index].clone()
        }
        CounterStyleSystem::Cyclic | CounterStyleSystem::ExtendsDecimal => abs_value.to_string(),
    };
    if let Some((width, pad_symbol)) = &style.pad {
        while representation.chars().count() < *width {
            representation.insert_str(0, pad_symbol);
        }
    }
    if negative {
        representation = format!("{}{}{}", style.negative.0, representation, style.negative.1);
    }
    if include_affixes {
        format!("{}{}{}", style.prefix, representation, style.suffix)
    } else {
        representation
    }
}
