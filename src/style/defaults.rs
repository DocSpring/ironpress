use crate::parser::css::{CssValue, SpecifiedColor, StyleMap};
use crate::parser::dom::HtmlTag;
use crate::types::Color;

/// Returns the default (user-agent) styles for a given HTML tag.
pub fn default_style(tag: HtmlTag) -> StyleMap {
    let mut style = StyleMap::new();

    match tag {
        // Chrome UA stylesheet uses em-relative values for both font-size and
        // margins. CssValue::Em represents an em multiplier, resolved in
        // apply_style_map by multiplying with the current/inherited font-size.
        // This makes headings scale with body font-size (e.g. body { font-size:
        // 14px } yields H1 at 2em = 28px, not a fixed 32px).
        HtmlTag::H1 => {
            style.set("font-size", CssValue::Em(2.0));
            style.set("font-weight", CssValue::Keyword("bold".into()));
            style.set("margin-top", CssValue::Em(0.67));
            style.set("margin-bottom", CssValue::Em(0.67));
        }
        HtmlTag::H2 => {
            style.set("font-size", CssValue::Em(1.5));
            style.set("font-weight", CssValue::Keyword("bold".into()));
            style.set("margin-top", CssValue::Em(0.83));
            style.set("margin-bottom", CssValue::Em(0.83));
        }
        HtmlTag::H3 => {
            style.set("font-size", CssValue::Em(1.17));
            style.set("font-weight", CssValue::Keyword("bold".into()));
            style.set("margin-top", CssValue::Em(1.0));
            style.set("margin-bottom", CssValue::Em(1.0));
        }
        HtmlTag::H4 => {
            // Chrome UA does not set font-size on H4 (inherits parent, 1em).
            style.set("font-weight", CssValue::Keyword("bold".into()));
            style.set("margin-top", CssValue::Em(1.33));
            style.set("margin-bottom", CssValue::Em(1.33));
        }
        HtmlTag::H5 => {
            style.set("font-size", CssValue::Em(0.83));
            style.set("font-weight", CssValue::Keyword("bold".into()));
            style.set("margin-top", CssValue::Em(1.67));
            style.set("margin-bottom", CssValue::Em(1.67));
        }
        HtmlTag::H6 => {
            style.set("font-size", CssValue::Em(0.67));
            style.set("font-weight", CssValue::Keyword("bold".into()));
            style.set("margin-top", CssValue::Em(2.33));
            style.set("margin-bottom", CssValue::Em(2.33));
        }
        HtmlTag::P => {
            style.set("margin-top", CssValue::Em(1.0));
            style.set("margin-bottom", CssValue::Em(1.0));
        }
        HtmlTag::Strong | HtmlTag::B => {
            style.set("font-weight", CssValue::Keyword("bold".into()));
        }
        HtmlTag::Em | HtmlTag::I | HtmlTag::Cite => {
            style.set("font-style", CssValue::Keyword("italic".into()));
        }
        HtmlTag::U | HtmlTag::Ins => {
            style.set("text-decoration", CssValue::Keyword("underline".into()));
        }
        HtmlTag::Del | HtmlTag::S => {
            style.set("text-decoration", CssValue::Keyword("line-through".into()));
        }
        HtmlTag::A => {
            style.set(
                "color",
                CssValue::Color(SpecifiedColor::Absolute(Color::rgb(0, 0, 238))),
            );
            style.set("text-decoration", CssValue::Keyword("underline".into()));
        }
        // Chrome UA stylesheet: `center { text-align: -webkit-center }`.
        HtmlTag::Center => {
            style.set("text-align", CssValue::Keyword("-webkit-center".into()));
        }
        // Chrome UA stylesheet: `hr { color: gray; border-style: inset;
        // border-width: 1px; margin-block: 0.5em; margin-inline: auto;
        // overflow: hidden; unicode-bidi: isolate }`.
        HtmlTag::Hr => {
            style.set(
                "color",
                CssValue::Color(SpecifiedColor::Absolute(Color::rgb(128, 128, 128))),
            );
            for side in ["top", "right", "bottom", "left"] {
                style.set(
                    &format!("border-{side}-style"),
                    CssValue::Keyword("inset".into()),
                );
                style.set(&format!("border-{side}-width"), CssValue::Length(0.75));
            }
            style.set("margin-top", CssValue::Em(0.5));
            style.set("margin-bottom", CssValue::Em(0.5));
            style.set("margin-left", CssValue::Keyword("auto".into()));
            style.set("margin-right", CssValue::Keyword("auto".into()));
            style.set("overflow", CssValue::Keyword("hidden".into()));
            style.set("unicode-bidi", CssValue::Keyword("isolate".into()));
        }
        HtmlTag::Ul | HtmlTag::Ol => {
            // Chrome UA: margin 1em top/bottom, padding-left 40px (≈30pt).
            // Use padding-left (not margin-left) so user CSS `padding-left:0`
            // correctly resets list indentation.
            style.set("margin-top", CssValue::Em(1.0));
            style.set("margin-bottom", CssValue::Em(1.0));
            style.set("padding-left", CssValue::Length(30.0));
            // Chrome UA stylesheet: `ul { list-style-type: disc }`, `ol {
            // list-style-type: decimal }`. `list-style-type` is inherited, so the
            // marker is resolved from the <li>'s computed value; without these
            // explicit defaults an <ol> would fall back to the global initial
            // (`disc`) and render bullets instead of `1.`/`2.`. Setting both also
            // ensures a <ul> nested inside an <ol> resets to `disc` rather than
            // inheriting the enclosing `decimal`.
            style.set(
                "list-style-type",
                CssValue::Keyword(
                    if matches!(tag, HtmlTag::Ol) {
                        "decimal"
                    } else {
                        "disc"
                    }
                    .into(),
                ),
            );
        }
        HtmlTag::Table => {
            style.set("box-sizing", CssValue::Keyword("border-box".into()));
            style.set("border-spacing", CssValue::Length(1.5));
            style.set("text-indent", CssValue::Keyword("initial".into()));
        }
        // Chrome UA stylesheet: `dl { margin-block: 1em }`, `dd {
        // margin-inline-start: 40px }`; `<dt>` is a plain block.
        HtmlTag::Dl => {
            style.set("margin-top", CssValue::Em(1.0));
            style.set("margin-bottom", CssValue::Em(1.0));
        }
        HtmlTag::Dd => {
            style.set("margin-left", CssValue::Length(30.0));
        }
        HtmlTag::Td => {
            style.set("padding-top", CssValue::Length(0.75));
            style.set("padding-right", CssValue::Length(0.75));
            style.set("padding-bottom", CssValue::Length(0.75));
            style.set("padding-left", CssValue::Length(0.75));
            // Chrome UA stylesheet: row groups/rows set `vertical-align: middle`
            // and `td,th { vertical-align: inherit }`, so a cell with no author
            // `vertical-align` resolves to `middle` (NOT the `baseline` initial
            // value). A single line of content in an over-tall cell is therefore
            // vertically centered, matching Chrome. An explicit author
            // `vertical-align` (e.g. `baseline`/`top`) still wins (UA precedence).
            style.set("vertical-align", CssValue::Keyword("middle".into()));
        }
        HtmlTag::Th => {
            style.set("padding-top", CssValue::Length(0.75));
            style.set("padding-right", CssValue::Length(0.75));
            style.set("padding-bottom", CssValue::Length(0.75));
            style.set("padding-left", CssValue::Length(0.75));
            style.set("font-weight", CssValue::Keyword("bold".into()));
            // Chrome UA stylesheet: th { text-align: center }. text-align is an
            // inherited property and is not reset in compute_style_with_context,
            // so without this default a th would inherit the row/body alignment
            // (typically left) rather than centering its content like Chrome.
            style.set("text-align", CssValue::Keyword("center".into()));
            // Same `vertical-align: middle` UA default as <td> (see above).
            style.set("vertical-align", CssValue::Keyword("middle".into()));
        }
        // Chrome UA stylesheet: `margin-block: 1em; margin-inline: 40px`, in
        // the inherited font style (not italic).
        HtmlTag::Blockquote => {
            style.set("margin-top", CssValue::Em(1.0));
            style.set("margin-bottom", CssValue::Em(1.0));
            style.set("margin-left", CssValue::Length(30.0));
            style.set("margin-right", CssValue::Length(30.0));
        }
        // Chrome UA stylesheet: `pre { font-family: monospace; white-space:
        // pre; margin: 1em 0 }`. The generic `monospace` family makes its
        // text 13px where nothing set an absolute size
        // (`crate::style::font_size`).
        HtmlTag::Pre => {
            style.set("font-family", CssValue::Keyword("monospace".into()));
            style.set("white-space", CssValue::Keyword("pre".into()));
            style.set("margin-top", CssValue::Em(1.0));
            style.set("margin-bottom", CssValue::Em(1.0));
        }
        // `code, kbd, samp, tt { font-family: monospace }`.
        HtmlTag::Code => {
            style.set("font-family", CssValue::Keyword("monospace".into()));
        }
        HtmlTag::Small => {
            style.set("font-size", CssValue::Keyword("smaller".into()));
        }
        HtmlTag::Big => {
            style.set("font-size", CssValue::Keyword("larger".into()));
        }
        // Chrome's UA stylesheet shrinks `sub`/`sup` with `font-size:
        // smaller` (the parent's size / 1.2); 0.83em is within 0.4% of it and
        // is what the parity suite pins (`tests/parity/ua-pins.css`).
        HtmlTag::Sub => {
            style.set("vertical-align", CssValue::Keyword("sub".into()));
            style.set("font-size", CssValue::Em(0.83));
        }
        HtmlTag::Sup => {
            style.set("vertical-align", CssValue::Keyword("super".into()));
            style.set("font-size", CssValue::Em(0.83));
        }
        // `mark { background-color: Mark; color: MarkText }`.
        HtmlTag::Mark => {
            style.set(
                "background-color",
                CssValue::Color(SpecifiedColor::Absolute(Color::rgb(255, 255, 0))),
            );
            style.set(
                "color",
                CssValue::Color(SpecifiedColor::Absolute(Color::rgb(0, 0, 0))),
            );
        }
        HtmlTag::Address => {
            style.set("font-style", CssValue::Keyword("italic".into()));
        }
        // `figure { margin-block: 1em; margin-inline: 40px }`; `<figcaption>`
        // is a plain block.
        HtmlTag::Figure => {
            style.set("margin-top", CssValue::Em(1.0));
            style.set("margin-bottom", CssValue::Em(1.0));
            style.set("margin-left", CssValue::Length(30.0));
            style.set("margin-right", CssValue::Length(30.0));
        }
        HtmlTag::Caption => {
            // Chrome UA stylesheet: caption { display: table-caption;
            // text-align: center }. It does NOT bold the caption, so do not set
            // font-weight here (that previously made captions render heavier
            // than Chrome).
            style.set("text-align", CssValue::Keyword("center".into()));
        }
        HtmlTag::Summary => {
            style.set("font-weight", CssValue::Keyword("bold".into()));
        }
        HtmlTag::Input => {
            // An inline-level box of its own in the line, as in Chrome's UA stylesheet: as an inline element, a text
            // field drew nothing and a select drew every option's text
            style.set("display", CssValue::Keyword("inline-block".into()));
            style.set("padding-top", CssValue::Length(2.0));
            style.set("padding-right", CssValue::Length(4.0));
            style.set("padding-bottom", CssValue::Length(2.0));
            style.set("padding-left", CssValue::Length(4.0));
            style.set("border-width", CssValue::Length(1.0));
            style.set(
                "border-color",
                CssValue::Color(SpecifiedColor::Absolute(Color::rgb(169, 169, 169))),
            );
        }
        HtmlTag::Select => {
            // An inline-level box of its own in the line, as in Chrome's UA stylesheet: as an inline element, a text
            // field drew nothing and a select drew every option's text
            style.set("display", CssValue::Keyword("inline-block".into()));
            style.set("padding-top", CssValue::Length(2.0));
            style.set("padding-right", CssValue::Length(4.0));
            style.set("padding-bottom", CssValue::Length(2.0));
            style.set("padding-left", CssValue::Length(4.0));
            style.set("border-width", CssValue::Length(1.0));
            style.set(
                "border-color",
                CssValue::Color(SpecifiedColor::Absolute(Color::rgb(169, 169, 169))),
            );
        }
        HtmlTag::Textarea => {
            style.set("padding-top", CssValue::Length(4.0));
            style.set("padding-right", CssValue::Length(4.0));
            style.set("padding-bottom", CssValue::Length(4.0));
            style.set("padding-left", CssValue::Length(4.0));
            style.set("border-width", CssValue::Length(1.0));
            style.set(
                "border-color",
                CssValue::Color(SpecifiedColor::Absolute(Color::rgb(169, 169, 169))),
            );
            style.set("font-size", CssValue::Length(10.0));
        }
        HtmlTag::Video => {
            style.set("margin-top", CssValue::Length(4.0));
            style.set("margin-bottom", CssValue::Length(4.0));
            style.set(
                "background-color",
                CssValue::Color(SpecifiedColor::Absolute(Color::rgb(0, 0, 0))),
            );
        }
        HtmlTag::Audio => {
            style.set("margin-top", CssValue::Length(2.0));
            style.set("margin-bottom", CssValue::Length(2.0));
            style.set(
                "background-color",
                CssValue::Color(SpecifiedColor::Absolute(Color::rgb(240, 240, 240))),
            );
        }
        HtmlTag::Progress => {
            // Chromium's UA sheet: a gauge is an inline-block
            style.set("display", CssValue::Keyword("inline-block".into()));
            style.set("margin-top", CssValue::Length(2.0));
            style.set("margin-bottom", CssValue::Length(2.0));
        }
        HtmlTag::Meter => {
            // Chromium's UA sheet: a gauge is an inline-block
            style.set("display", CssValue::Keyword("inline-block".into()));
            style.set("margin-top", CssValue::Length(2.0));
            style.set("margin-bottom", CssValue::Length(2.0));
        }
        HtmlTag::Bdi => style.set("unicode-bidi", CssValue::Keyword("isolate".into())),
        HtmlTag::Bdo => style.set("unicode-bidi", CssValue::Keyword("isolate-override".into())),
        HtmlTag::NotRendered => style.set("display", CssValue::Keyword("none".into())),
        _ => {}
    }

    style
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn h1_defaults() {
        let s = default_style(HtmlTag::H1);
        assert!(s.get("font-size").is_some());
        assert!(s.get("font-weight").is_some());
    }

    #[test]
    fn all_heading_defaults() {
        for tag in [
            HtmlTag::H1,
            HtmlTag::H2,
            HtmlTag::H3,
            HtmlTag::H4,
            HtmlTag::H5,
            HtmlTag::H6,
        ] {
            let s = default_style(tag);
            assert!(
                s.get("font-weight").is_some(),
                "Missing font-weight for {:?}",
                tag
            );
        }
    }

    #[test]
    fn inline_defaults() {
        assert!(default_style(HtmlTag::Strong).get("font-weight").is_some());
        assert!(default_style(HtmlTag::Em).get("font-style").is_some());
        assert!(default_style(HtmlTag::U).get("text-decoration").is_some());
        assert!(default_style(HtmlTag::A).get("color").is_some());
        assert!(default_style(HtmlTag::Del).get("text-decoration").is_some());
        assert!(default_style(HtmlTag::S).get("text-decoration").is_some());
        assert!(default_style(HtmlTag::Ins).get("text-decoration").is_some());
        assert!(
            default_style(HtmlTag::Mark)
                .get("background-color")
                .is_some()
        );
        assert!(matches!(
            default_style(HtmlTag::Small).get("font-size"),
            Some(CssValue::Keyword(k)) if k == "smaller"
        ));
        // `<code>` sets only the generic family; its size follows from it.
        assert!(default_style(HtmlTag::Code).get("font-size").is_none());
        assert!(
            default_style(HtmlTag::Code)
                .get("background-color")
                .is_none()
        );
    }

    #[test]
    fn ua_colors_are_explicit_absolute_specified_values() {
        assert!(matches!(
            default_style(HtmlTag::A).get("color"),
            Some(CssValue::Color(SpecifiedColor::Absolute(color)))
                if *color == Color::rgb(0, 0, 238)
        ));
        assert!(matches!(
            default_style(HtmlTag::Mark).get("background-color"),
            Some(CssValue::Color(SpecifiedColor::Absolute(color)))
                if *color == Color::rgb(255, 255, 0)
        ));
    }

    #[test]
    fn list_defaults() {
        assert!(default_style(HtmlTag::Ul).get("padding-left").is_some());
        assert!(default_style(HtmlTag::Ol).get("padding-left").is_some());
        // Chrome's UA stylesheet gives list items no margins.
        assert!(default_style(HtmlTag::Li).get("margin-bottom").is_none());
        assert!(default_style(HtmlTag::Dl).get("margin-top").is_some());
        // Chrome doesn't bold a `<dt>`.
        assert!(default_style(HtmlTag::Dt).get("font-weight").is_none());
        assert!(default_style(HtmlTag::Dd).get("margin-left").is_some());
    }

    #[test]
    fn table_defaults() {
        assert!(default_style(HtmlTag::Td).get("padding-top").is_some());
        assert!(default_style(HtmlTag::Th).get("font-weight").is_some());
        // th defaults center its content like Chrome's UA stylesheet.
        assert!(matches!(
            default_style(HtmlTag::Th).get("text-align"),
            Some(CssValue::Keyword(k)) if k == "center"
        ));
        // caption is centered but NOT bold (matching Chrome's UA stylesheet).
        assert!(matches!(
            default_style(HtmlTag::Caption).get("text-align"),
            Some(CssValue::Keyword(k)) if k == "center"
        ));
        assert!(default_style(HtmlTag::Caption).get("font-weight").is_none());
    }

    #[test]
    fn block_element_defaults() {
        assert!(
            default_style(HtmlTag::Blockquote)
                .get("margin-left")
                .is_some()
        );
        // Chrome gives a `<pre>` no background, padding or font size.
        let pre = default_style(HtmlTag::Pre);
        assert!(pre.get("background-color").is_none());
        assert!(pre.get("padding-left").is_none());
        assert!(pre.get("font-size").is_none());
        assert!(default_style(HtmlTag::Address).get("font-style").is_some());
        assert!(default_style(HtmlTag::Figure).get("margin-left").is_some());
        // Chrome's `<figcaption>` is a plain block.
        assert!(default_style(HtmlTag::Figcaption).properties.is_empty());
        assert!(default_style(HtmlTag::Summary).get("font-weight").is_some());
    }

    #[test]
    fn blockquote_defaults_match_chrome() {
        let blockquote = default_style(HtmlTag::Blockquote);
        assert!(matches!(
            blockquote.get("margin-top"),
            Some(CssValue::Em(1.0))
        ));
        assert!(matches!(
            blockquote.get("margin-bottom"),
            Some(CssValue::Em(1.0))
        ));
        assert!(matches!(
            blockquote.get("margin-left"),
            Some(CssValue::Length(30.0))
        ));
        assert!(matches!(
            blockquote.get("margin-right"),
            Some(CssValue::Length(30.0))
        ));
        assert!(blockquote.get("font-style").is_none());
    }

    #[test]
    fn form_element_defaults() {
        let input = default_style(HtmlTag::Input);
        assert!(input.get("padding-top").is_some());
        assert!(input.get("border-width").is_some());
        assert!(input.get("border-color").is_some());

        let select = default_style(HtmlTag::Select);
        assert!(select.get("padding-top").is_some());
        assert!(select.get("border-width").is_some());

        let textarea = default_style(HtmlTag::Textarea);
        assert!(textarea.get("padding-top").is_some());
        assert!(textarea.get("font-size").is_some());
        assert!(textarea.get("border-width").is_some());
    }

    #[test]
    fn media_element_defaults() {
        let video = default_style(HtmlTag::Video);
        assert!(video.get("background-color").is_some());
        assert!(video.get("margin-top").is_some());

        let audio = default_style(HtmlTag::Audio);
        assert!(audio.get("background-color").is_some());
        assert!(audio.get("margin-top").is_some());
    }

    #[test]
    fn progress_meter_defaults() {
        let progress = default_style(HtmlTag::Progress);
        assert!(progress.get("margin-top").is_some());

        let meter = default_style(HtmlTag::Meter);
        assert!(meter.get("margin-top").is_some());
    }

    #[test]
    fn unknown_tag_has_no_defaults() {
        let s = default_style(HtmlTag::Unknown);
        assert!(s.properties.is_empty());
    }

    #[test]
    fn p_has_no_default_font_size() {
        let s = default_style(HtmlTag::P);
        assert!(
            s.get("font-size").is_none(),
            "P tag should not have a default font-size (it inherits from parent)"
        );
    }
}
