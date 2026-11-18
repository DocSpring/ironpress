use crate::style::computed::BorderStyle;
use crate::types::PhysicalSide;

/// Used stripe and gap allocation for a CSS `double` border.
///
/// CSS leaves the allocation implementation-defined. Blink rounds the outer
/// third in integer CSS pixels; the inner stripe has the same width and the
/// middle gap owns the remainder. Fractional authored widths retain exact
/// thirds rather than being silently enlarged to a device-dependent integer.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub(crate) struct DoubleBorderMetrics {
    width: f32,
    stripe: f32,
}

impl DoubleBorderMetrics {
    pub(crate) fn new(width: f32) -> Self {
        let width = width.max(0.0);
        let css_width = width / crate::fonts::PT_PER_CSS_PX;
        let snapped_css_width = css_width.round();
        let stripe = if snapped_css_width >= 3.0 && (css_width - snapped_css_width).abs() < 0.001 {
            ((snapped_css_width + 1.0) / 3.0).floor() * crate::fonts::PT_PER_CSS_PX
        } else {
            width / 3.0
        }
        .min(width / 2.0);
        Self { width, stripe }
    }

    pub(crate) const fn stripe_width(self) -> f32 {
        self.stripe
    }

    pub(crate) fn inner_inset(self) -> f32 {
        self.width - self.stripe
    }

    pub(crate) fn outer_centerline_inset(self) -> f32 {
        self.stripe * 0.5
    }

    pub(crate) fn inner_centerline_inset(self) -> f32 {
        self.inner_inset() + self.stripe * 0.5
    }

    pub(crate) fn centerline_insets(self) -> [f32; 2] {
        [self.outer_centerline_inset(), self.inner_centerline_inset()]
    }

    pub(crate) fn paints(self, offset: f32) -> bool {
        offset < self.stripe || offset >= self.inner_inset()
    }
}

const LIGHTENED_BLACK: f32 = 84.0 / 255.0;
const VALUE_STEP: f32 = 0.33;

fn to_legacy_3d_component(component: f32) -> f32 {
    (component.clamp(0.0, 1.0) * 255.999_98).floor() / 255.0
}

/// Blink's `Color::Light`: raise the brightest channel by a third, keeping
/// the hue; black becomes a dark gray.
fn lighter((r, g, b): (f32, f32, f32)) -> (f32, f32, f32) {
    let value = r.max(g).max(b);
    if value == 0.0 {
        return (LIGHTENED_BLACK, LIGHTENED_BLACK, LIGHTENED_BLACK);
    }
    let multiplier = (value + VALUE_STEP).min(1.0) / value;
    (
        to_legacy_3d_component(r * multiplier),
        to_legacy_3d_component(g * multiplier),
        to_legacy_3d_component(b * multiplier),
    )
}

/// Blink's `Color::Dark`: lower the brightest channel by a third.
fn darker((r, g, b): (f32, f32, f32)) -> (f32, f32, f32) {
    let value = r.max(g).max(b);
    if value == 0.0 {
        return (0.0, 0.0, 0.0);
    }
    let multiplier = (value - VALUE_STEP).max(0.0) / value;
    (
        to_legacy_3d_component(r * multiplier),
        to_legacy_3d_component(g * multiplier),
        to_legacy_3d_component(b * multiplier),
    )
}

/// WCAG relative luminance of a color, from its 8-bit sRGB channels.
fn relative_luminance((r, g, b): (f32, f32, f32)) -> f32 {
    let linear = |channel: f32| {
        let channel = (channel.clamp(0.0, 1.0) * 255.0).round() / 255.0;
        if channel <= 0.040_45 {
            channel / 12.92
        } else {
            ((channel + 0.055) / 1.055).powf(2.4)
        }
    };
    0.2126 * linear(r) + 0.7152 * linear(g) + 0.0722 * linear(b)
}

/// The shadow and highlight colors Chromium paints a 3D border's edges with:
/// the color darkened and lightened by a third. A color no brighter than
/// `#202020` (by relative luminance) is lightened once for the shadow and
/// twice for the highlight; one brighter than `#ebebeb` is its own highlight.
/// Measured against Chrome 156, where `#490000` and `#1a1a2e` count as dark
/// and `#4a0000`, `#282828` and `#003000` don't, and `#f0f0a0` keeps its own
/// highlight.
fn bevel_colors(base: (f32, f32, f32)) -> ((f32, f32, f32), (f32, f32, f32)) {
    let gray = |value: f32| relative_luminance((value / 255.0, value / 255.0, value / 255.0));
    let luminance = relative_luminance(base);
    if luminance <= gray(32.0) {
        let shadow = lighter(base);
        return (shadow, lighter(shadow));
    }
    if luminance > gray(235.0) {
        return (darker(base), base);
    }
    (darker(base), lighter(base))
}

pub(crate) fn is_bevel_style(style: BorderStyle) -> bool {
    matches!(
        style,
        BorderStyle::Groove | BorderStyle::Ridge | BorderStyle::Inset | BorderStyle::Outset
    )
}

pub(crate) fn bevel_edge_color(
    style: BorderStyle,
    side: PhysicalSide,
    inner_band: bool,
    base: (f32, f32, f32),
) -> (f32, f32, f32) {
    let high_edge = matches!(side, PhysicalSide::Top | PhysicalSide::Left);
    let light_on_high_edge = match style {
        BorderStyle::Outset => true,
        BorderStyle::Inset => false,
        BorderStyle::Ridge => !inner_band,
        BorderStyle::Groove => inner_band,
        _ => return base,
    };
    let (shadow, highlight) = bevel_colors(base);
    if high_edge == light_on_high_edge {
        highlight
    } else {
        shadow
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rgb(hex: u32) -> (f32, f32, f32) {
        let channel = |shift: u32| ((hex >> shift) & 0xff) as f32 / 255.0;
        (channel(16), channel(8), channel(0))
    }

    fn hex((r, g, b): (f32, f32, f32)) -> String {
        let byte = |channel: f32| (channel * 255.0).round() as u8;
        format!("#{:02x}{:02x}{:02x}", byte(r), byte(g), byte(b))
    }

    /// Chrome 156's shadow and highlight for a `2px inset` border of each
    /// color, read from its PDF output.
    #[test]
    fn bevel_colors_match_chrome() {
        let cases = [
            (0x00897b, "#00352f", "#00dec7"),
            (0xc0392b, "#6c2018", "#ff4b39"),
            (0x808080, "#2c2c2c", "#d4d4d4"),
            (0x2e86c1, "#1a4b6d", "#3db1ff"),
            (0x000000, "#545454", "#a8a8a8"),
            (0xffffff, "#ababab", "#ffffff"),
            (0xeeeeee, "#9a9a9a", "#eeeeee"),
            (0xff0000, "#ab0000", "#ff0000"),
            (0x00ff00, "#00ab00", "#00ff00"),
            (0x0000ff, "#0000ab", "#0000ff"),
            (0x202020, "#747474", "#c8c8c8"),
            (0xebebeb, "#979797", "#ffffff"),
            (0x336699, "#172e45", "#4f9eee"),
            (0x999999, "#454545", "#eeeeee"),
            (0x5a5a5a, "#050505", "#aeaeae"),
            (0xd0d0d0, "#7c7c7c", "#ffffff"),
            // Dark and light by luminance, not by distance from black or white.
            (0x490000, "#9d0000", "#f20000"),
            (0x4a0000, "#000000", "#9e0000"),
            (0x1a1a2e, "#494982", "#7878d6"),
            (0x212121, "#000000", "#757575"),
            (0x003000, "#000000", "#008400"),
            (0x000060, "#0000b4", "#0000ff"),
            (0xf0f0a0, "#9c9c68", "#f0f0a0"),
            (0xececec, "#989898", "#ececec"),
        ];
        for (base, shadow, highlight) in cases {
            let (actual_shadow, actual_highlight) = bevel_colors(rgb(base));
            assert_eq!(
                (hex(actual_shadow).as_str(), hex(actual_highlight).as_str()),
                (shadow, highlight),
                "#{base:06x}"
            );
        }
    }

    #[test]
    fn inset_and_outset_swap_the_edges_and_groove_and_ridge_swap_the_bands() {
        let base = rgb(0x808080);
        let (shadow, highlight) = bevel_colors(base);
        let top = |style, inner| bevel_edge_color(style, PhysicalSide::Top, inner, base);
        let bottom = |style, inner| bevel_edge_color(style, PhysicalSide::Bottom, inner, base);
        assert_eq!(
            (
                top(BorderStyle::Inset, false),
                bottom(BorderStyle::Inset, false)
            ),
            (shadow, highlight)
        );
        assert_eq!(
            (
                top(BorderStyle::Outset, false),
                bottom(BorderStyle::Outset, false)
            ),
            (highlight, shadow)
        );
        assert_eq!(
            (
                top(BorderStyle::Groove, false),
                top(BorderStyle::Groove, true)
            ),
            (shadow, highlight)
        );
        assert_eq!(
            (
                top(BorderStyle::Ridge, false),
                top(BorderStyle::Ridge, true)
            ),
            (highlight, shadow)
        );
        assert_eq!(top(BorderStyle::Solid, false), base);
    }
}
