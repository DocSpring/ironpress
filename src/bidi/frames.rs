//! The part an inline box plays in the bidi algorithm (CSS Writing Modes 4
//! §2.4.2): a box whose `unicode-bidi` isn't `normal` brackets its text with
//! UAX #9's explicit formatting characters, so its text is embedded in,
//! isolated from or overrides the text around it.

use crate::style::computed::{ComputedStyle, UnicodeBidi};

const LRE: char = '\u{202a}';
const RLE: char = '\u{202b}';
const PDF: char = '\u{202c}';
const LRO: char = '\u{202d}';
const RLO: char = '\u{202e}';
const LRI: char = '\u{2066}';
const RLI: char = '\u{2067}';
const FSI: char = '\u{2068}';
const PDI: char = '\u{2069}';

/// One inline box's bidi bracket. Each run carries the frames of the boxes
/// around it, outermost first; `id` tells two sibling boxes apart, so each
/// gets its own bracket.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct BidiFrame {
    id: usize,
    control: BidiControl,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum BidiControl {
    Embed {
        rtl: bool,
    },
    Isolate {
        rtl: bool,
    },
    Override {
        rtl: bool,
    },
    IsolateOverride {
        rtl: bool,
    },
    /// An isolate in the direction of its first strong character.
    FirstStrongIsolate,
}

impl BidiFrame {
    /// The bracket of inline box `id`, styled `style`, if it has one.
    /// `auto_direction` is set when its direction comes from its text
    /// (`dir="auto"`, or a `<bdi>` without a `dir`).
    pub(crate) fn of(style: &ComputedStyle, auto_direction: bool, id: usize) -> Option<Self> {
        let rtl = style.direction_rtl;
        let control = match style.unicode_bidi {
            UnicodeBidi::Normal => return None,
            UnicodeBidi::Embed => BidiControl::Embed { rtl },
            UnicodeBidi::Isolate if auto_direction => BidiControl::FirstStrongIsolate,
            UnicodeBidi::Isolate => BidiControl::Isolate { rtl },
            UnicodeBidi::BidiOverride => BidiControl::Override { rtl },
            UnicodeBidi::IsolateOverride => BidiControl::IsolateOverride { rtl },
            UnicodeBidi::Plaintext => BidiControl::FirstStrongIsolate,
        };
        Some(Self { id, control })
    }

    /// Whether the bracket can reorder text with no right-to-left characters.
    pub(crate) const fn reorders_left_to_right_text(self) -> bool {
        matches!(
            self.control,
            BidiControl::Embed { rtl: true }
                | BidiControl::Isolate { rtl: true }
                | BidiControl::Override { rtl: true }
                | BidiControl::IsolateOverride { rtl: true }
        )
    }

    fn push_opening(self, text: &mut String) {
        let pick =
            |rtl: bool, ltr_char: char, rtl_char: char| if rtl { rtl_char } else { ltr_char };
        match self.control {
            BidiControl::Embed { rtl } => text.push(pick(rtl, LRE, RLE)),
            BidiControl::Isolate { rtl } => text.push(pick(rtl, LRI, RLI)),
            BidiControl::Override { rtl } => text.push(pick(rtl, LRO, RLO)),
            BidiControl::IsolateOverride { rtl } => {
                text.push(pick(rtl, LRI, RLI));
                text.push(pick(rtl, LRO, RLO));
            }
            BidiControl::FirstStrongIsolate => text.push(FSI),
        }
    }

    fn push_closing(self, text: &mut String) {
        match self.control {
            BidiControl::Embed { .. } | BidiControl::Override { .. } => text.push(PDF),
            BidiControl::Isolate { .. } | BidiControl::FirstStrongIsolate => text.push(PDI),
            BidiControl::IsolateOverride { .. } => {
                text.push(PDF);
                text.push(PDI);
            }
        }
    }
}

/// Push the formatting characters that take the text from inside the boxes
/// `from` to inside the boxes `to`: closing those `to` doesn't share,
/// innermost first, then opening its own, outermost first.
pub(super) fn push_transition(text: &mut String, from: &[BidiFrame], to: &[BidiFrame]) {
    let shared = from
        .iter()
        .zip(to)
        .take_while(|(left, right)| left == right)
        .count();
    for frame in from[shared..].iter().rev() {
        frame.push_closing(text);
    }
    for frame in &to[shared..] {
        frame.push_opening(text);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frame(id: usize, unicode_bidi: UnicodeBidi, rtl: bool) -> BidiFrame {
        let style = ComputedStyle {
            unicode_bidi,
            direction_rtl: rtl,
            ..ComputedStyle::default()
        };
        BidiFrame::of(&style, false, id).expect("a frame")
    }

    #[test]
    fn a_box_with_normal_unicode_bidi_has_no_bracket() {
        let style = ComputedStyle {
            direction_rtl: true,
            ..ComputedStyle::default()
        };
        assert_eq!(BidiFrame::of(&style, false, 0), None);
    }

    #[test]
    fn nested_boxes_open_outermost_first_and_close_innermost_first() {
        let outer = frame(0, UnicodeBidi::Isolate, true);
        let inner = frame(1, UnicodeBidi::BidiOverride, false);
        let mut text = String::new();
        push_transition(&mut text, &[], &[outer, inner]);
        assert_eq!(text, format!("{RLI}{LRO}"));
        text.clear();
        push_transition(&mut text, &[outer, inner], &[]);
        assert_eq!(text, format!("{PDF}{PDI}"));
    }

    #[test]
    fn sibling_boxes_get_a_bracket_each() {
        let first = frame(0, UnicodeBidi::BidiOverride, true);
        let second = frame(1, UnicodeBidi::BidiOverride, true);
        let mut text = String::new();
        push_transition(&mut text, &[first], &[second]);
        assert_eq!(text, format!("{PDF}{RLO}"));
    }

    #[test]
    fn an_isolate_override_is_an_override_inside_an_isolate() {
        let both = frame(0, UnicodeBidi::IsolateOverride, true);
        let mut text = String::new();
        push_transition(&mut text, &[], &[both]);
        push_transition(&mut text, &[both], &[]);
        assert_eq!(text, format!("{RLI}{RLO}{PDF}{PDI}"));
    }

    #[test]
    fn an_auto_direction_isolate_takes_its_first_strong_characters_direction() {
        let style = ComputedStyle {
            unicode_bidi: UnicodeBidi::Isolate,
            ..ComputedStyle::default()
        };
        let mut text = String::new();
        let auto = BidiFrame::of(&style, true, 0).expect("a frame");
        push_transition(&mut text, &[], &[auto]);
        assert_eq!(text, FSI.to_string());
    }
}
