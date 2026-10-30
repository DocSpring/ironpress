//! Fillable AcroForm fields for form controls (`RenderOpts::pdf_forms`).
//!
//! Each control's text block becomes a widget annotation over its padding box,
//! with appearance streams that draw its value, so viewers show the value
//! without regenerating it. The page still paints the control's background and
//! border. Text is drawn in Helvetica (WinAnsi), as viewers draw a field they
//! regenerate from its `/DA`. Radio buttons with one `name` are one field with a
//! widget per button; every other field is a single merged field and widget.

use super::*;
use crate::layout::elements::{FormField, FormFieldKind};
use std::collections::HashSet;

/// The inset of a field's text from its widget's edge, as viewers draw it
const TEXT_INSET: f32 = 2.0;
/// Helvetica's ascender and descender, as fractions of the font size
const HELVETICA_ASCENT: f32 = 0.718;
const HELVETICA_DESCENT: f32 = 0.207;
/// The distance between a multi-line field's baselines, in font sizes
const MULTILINE_LEADING: f32 = 1.15;

// Field flags (PDF 32000-1:2008, 12.7.3.1 and 12.7.4)
const FLAG_READ_ONLY: u32 = 1;
const FLAG_REQUIRED: u32 = 1 << 1;
const FLAG_MULTILINE: u32 = 1 << 12;
const FLAG_PASSWORD: u32 = 1 << 13;
const FLAG_NO_TOGGLE_TO_OFF: u32 = 1 << 14;
const FLAG_RADIO: u32 = 1 << 15;
const FLAG_COMBO: u32 = 1 << 17;

/// The fields written so far
#[derive(Default)]
pub(super) struct FormWriterState {
    /// Helvetica and ZapfDingbats, for the appearance streams and `/DR`
    fonts: Option<(usize, usize)>,
    names: HashSet<String>,
    unnamed: usize,
    /// Top-level fields, for `/AcroForm /Fields`
    fields: Vec<usize>,
    radio_groups: Vec<RadioGroup>,
}

/// A radio group: one field, whose kids are its buttons' widgets
struct RadioGroup {
    /// The buttons' HTML `name`, which the group is found by
    html_name: Option<String>,
    name: String,
    /// The group's object, written when the document is finished
    parent_id: usize,
    kids: Vec<usize>,
    /// The checked button's appearance state
    value: Option<String>,
    flags: u32,
}

/// A widget's entries, prepared (with its appearance streams) before its page,
/// so its page's object id is known when it's written
pub(super) struct PreparedWidget {
    entries: String,
    radio_group: Option<usize>,
}

/// A page annotation ready to write: a form widget with its appearance
/// streams written, or a link
pub(super) enum PreparedAnnotation<'a> {
    Widget(PdfRect, PreparedWidget),
    Link(PdfRect, &'a str),
}

impl PdfWriter {
    /// Write a widget's appearance streams (and the form fonts, the first time)
    pub(super) fn prepare_form_widget(
        &mut self,
        rect: PdfRect,
        field: &FormField,
    ) -> PreparedWidget {
        let (helvetica, _) = self.form_fonts();
        let (r, g, b) = field.color.to_f32_rgb();
        let rgb = format!(
            "{} {} {}",
            format_pdf_number(r),
            format_pdf_number(g),
            format_pdf_number(b)
        );
        let mut flags = 0;
        if field.read_only {
            flags |= FLAG_READ_ONLY;
        }
        if field.required {
            flags |= FLAG_REQUIRED;
        }
        let quadding = match field.align {
            TextAlign::Center => " /Q 1",
            TextAlign::Right => " /Q 2",
            TextAlign::Left | TextAlign::Justify => "",
        };
        let text_da = format!("/Helv {} Tf {rgb} rg", format_pdf_number(field.font_size));
        let mark_da = format!("/ZaDb 0 Tf {rgb} rg");
        match &field.kind {
            FormFieldKind::Text {
                value,
                multiline,
                password,
                max_length,
            } => {
                if *multiline {
                    flags |= FLAG_MULTILINE;
                }
                if *password {
                    flags |= FLAG_PASSWORD;
                }
                let shown = if *password {
                    "*".repeat(value.chars().count())
                } else {
                    value.clone()
                };
                let appearance = text_appearance(&shown, rect, field, &rgb, *multiline);
                let ap = self.add_appearance_stream(rect, &appearance, Some(helvetica));
                let name = self.forms.unique_name(field.name.as_deref());
                let max_length = max_length.map_or(String::new(), |n| format!(" /MaxLen {n}"));
                PreparedWidget {
                    entries: format!(
                        "/FT /Tx /T {} /V {} /Ff {flags} /DA ({text_da}){quadding}{max_length} /AP << /N {ap} 0 R >>",
                        pdf_text_string(&name),
                        pdf_text_string(value),
                    ),
                    radio_group: None,
                }
            }
            FormFieldKind::Checkbox { checked } => {
                let on = self.add_appearance_stream(rect, &check_appearance(rect, &rgb), None);
                let off = self.add_appearance_stream(rect, "", None);
                let name = self.forms.unique_name(field.name.as_deref());
                let state = if *checked { "Yes" } else { "Off" };
                PreparedWidget {
                    entries: format!(
                        "/FT /Btn /T {} /V /{state} /AS /{state} /Ff {flags} /DA ({mark_da}) /MK << /CA (4) >> /AP << /N << /Yes {on} 0 R /Off {off} 0 R >> >>",
                        pdf_text_string(&name),
                    ),
                    radio_group: None,
                }
            }
            FormFieldKind::Radio { value, checked } => {
                let state = pdf_name_token(value);
                let on = self.add_appearance_stream(rect, &dot_appearance(rect, &rgb), None);
                let off = self.add_appearance_stream(rect, "", None);
                let group = self.radio_group(field.name.as_deref(), flags);
                if *checked {
                    self.forms.radio_groups[group].value = Some(state.clone());
                }
                let shown = if *checked { state.as_str() } else { "Off" };
                PreparedWidget {
                    entries: format!(
                        "/Parent {} 0 R /AS /{shown} /DA ({mark_da}) /MK << /CA (l) >> /AP << /N << /{state} {on} 0 R /Off {off} 0 R >> >>",
                        self.forms.radio_groups[group].parent_id,
                    ),
                    radio_group: Some(group),
                }
            }
            FormFieldKind::Choice { options, selected } => {
                let (value, label) = selected
                    .and_then(|index| options.get(index))
                    .map_or((String::new(), String::new()), |(value, label)| {
                        (value.clone(), label.clone())
                    });
                let appearance = text_appearance(&label, rect, field, &rgb, false);
                let ap = self.add_appearance_stream(rect, &appearance, Some(helvetica));
                let name = self.forms.unique_name(field.name.as_deref());
                let opt = options
                    .iter()
                    .map(|(value, label)| {
                        if value == label {
                            pdf_text_string(value)
                        } else {
                            format!("[{} {}]", pdf_text_string(value), pdf_text_string(label))
                        }
                    })
                    .collect::<Vec<_>>()
                    .join(" ");
                PreparedWidget {
                    entries: format!(
                        "/FT /Ch /T {} /V {} /Opt [{opt}] /Ff {} /DA ({text_da}){quadding} /AP << /N {ap} 0 R >>",
                        pdf_text_string(&name),
                        pdf_text_string(&value),
                        flags | FLAG_COMBO,
                    ),
                    radio_group: None,
                }
            }
        }
    }

    /// The widget's object, and its field in the form
    pub(super) fn add_form_widget(
        &mut self,
        annot_id: usize,
        page_id: usize,
        rect: PdfRect,
        widget: PreparedWidget,
    ) {
        self.objects.push(format!(
            "{annot_id} 0 obj\n<< /Type /Annot /Subtype /Widget /P {page_id} 0 R /Rect [{} {} {} {}] /F 4 {} >>\nendobj",
            format_pdf_number(rect.left),
            format_pdf_number(rect.bottom),
            format_pdf_number(rect.right()),
            format_pdf_number(rect.top()),
            widget.entries,
        ));
        match widget.radio_group {
            Some(group) => self.forms.radio_groups[group].kids.push(annot_id),
            None => self.forms.fields.push(annot_id),
        }
    }

    /// Helvetica and ZapfDingbats, written the first time a field needs them
    fn form_fonts(&mut self) -> (usize, usize) {
        if let Some(fonts) = self.forms.fonts {
            return fonts;
        }
        let helvetica = self.next_id();
        self.objects.push(format!(
            "{helvetica} 0 obj\n<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica /Encoding /WinAnsiEncoding >>\nendobj"
        ));
        let zapf = self.next_id();
        self.objects.push(format!(
            "{zapf} 0 obj\n<< /Type /Font /Subtype /Type1 /BaseFont /ZapfDingbats >>\nendobj"
        ));
        self.forms.fonts = Some((helvetica, zapf));
        (helvetica, zapf)
    }

    fn add_appearance_stream(
        &mut self,
        rect: PdfRect,
        content: &str,
        font: Option<usize>,
    ) -> usize {
        let id = self.next_id();
        let resources = font.map_or(String::new(), |font| {
            format!("/Font << /Helv {font} 0 R >>")
        });
        self.objects.push(format!(
            "{id} 0 obj\n<< /Type /XObject /Subtype /Form /BBox [0 0 {} {}] /Resources << {resources} >> /Length {} >>\nstream\n{content}\nendstream\nendobj",
            format_pdf_number(rect.width),
            format_pdf_number(rect.height),
            content.len(),
        ));
        id
    }

    /// The group for a radio button: the one for its name, or a new one (a
    /// button without a name is a group of its own)
    fn radio_group(&mut self, html_name: Option<&str>, flags: u32) -> usize {
        if let Some(name) = html_name
            && let Some(index) = self
                .forms
                .radio_groups
                .iter()
                .position(|group| group.html_name.as_deref() == Some(name))
        {
            return index;
        }
        let name = self.forms.unique_name(html_name);
        let parent_id = self.next_id();
        // Written with its kids when the document is finished
        self.objects.push(String::new());
        self.forms.fields.push(parent_id);
        self.forms.radio_groups.push(RadioGroup {
            html_name: html_name.map(str::to_string),
            name,
            parent_id,
            kids: Vec::new(),
            value: None,
            flags: flags | FLAG_RADIO | FLAG_NO_TOGGLE_TO_OFF,
        });
        self.forms.radio_groups.len() - 1
    }
}

impl FormWriterState {
    /// A field name no other field has: the HTML name, or `field_<n>` for a
    /// control without one, with `_2`, `_3`... after a name that's taken. A
    /// period separates the parts of a field's full name, so it becomes `_`.
    fn unique_name(&mut self, html_name: Option<&str>) -> String {
        let base = match html_name {
            Some(name) => name.replace('.', "_"),
            None => {
                self.unnamed += 1;
                format!("field_{}", self.unnamed)
            }
        };
        let mut name = base.clone();
        let mut n = 2;
        while self.names.contains(&name) {
            name = format!("{base}_{n}");
            n += 1;
        }
        self.names.insert(name.clone());
        name
    }

    /// The radio groups' objects, in their reserved slots
    pub(super) fn write_radio_groups(&self, objects: &mut [String]) {
        for group in &self.radio_groups {
            let kids = group
                .kids
                .iter()
                .map(|id| format!("{id} 0 R"))
                .collect::<Vec<_>>()
                .join(" ");
            objects[group.parent_id - 1] = format!(
                "{} 0 obj\n<< /FT /Btn /T {} /V /{} /Ff {} /Kids [{kids}] >>\nendobj",
                group.parent_id,
                pdf_text_string(&group.name),
                group.value.as_deref().unwrap_or("Off"),
                group.flags,
            );
        }
    }

    /// The catalog's `/AcroForm` entry, when the document has fields
    pub(super) fn acroform_entry(&self) -> String {
        let Some((helvetica, zapf)) = self.fonts else {
            return String::new();
        };
        if self.fields.is_empty() {
            return String::new();
        }
        let fields = self
            .fields
            .iter()
            .map(|id| format!("{id} 0 R"))
            .collect::<Vec<_>>()
            .join(" ");
        format!(
            " /AcroForm << /Fields [{fields}] /DR << /Font << /Helv {helvetica} 0 R /ZaDb {zapf} 0 R >> >> /DA (/Helv 0 Tf 0 g) >>"
        )
    }
}

/// A text field's (or a drop-down's) value, in Helvetica: one line in the
/// middle of the box, or wrapped lines from its top
fn text_appearance(
    text: &str,
    rect: PdfRect,
    field: &FormField,
    rgb: &str,
    multiline: bool,
) -> String {
    let size = field.font_size;
    let mut content = format!(
        "/Tx BMC\nq\n1 1 {} {} re W n\n",
        format_pdf_number((rect.width - 2.0).max(0.0)),
        format_pdf_number((rect.height - 2.0).max(0.0)),
    );
    if !text.is_empty() {
        let lines = if multiline {
            wrap_helvetica(text, size, rect.width - TEXT_INSET * 2.0)
        } else {
            vec![text.replace(['\n', '\r'], " ")]
        };
        let first_baseline = if multiline {
            rect.height - TEXT_INSET - size * HELVETICA_ASCENT
        } else {
            (rect.height - size * (HELVETICA_ASCENT + HELVETICA_DESCENT)) / 2.0
                + size * HELVETICA_DESCENT
        };
        content.push_str(&format!(
            "BT\n/Helv {} Tf\n{rgb} rg\n",
            format_pdf_number(size)
        ));
        let mut x_prev = 0.0;
        let mut y_prev = 0.0;
        for (index, line) in lines.iter().enumerate() {
            let width = helvetica_width(line, size);
            let x = match field.align {
                TextAlign::Center => (rect.width - width) / 2.0,
                TextAlign::Right => rect.width - TEXT_INSET - width,
                TextAlign::Left | TextAlign::Justify => TEXT_INSET,
            };
            let y = first_baseline - index as f32 * size * MULTILINE_LEADING;
            content.push_str(&format!(
                "{} {} Td\n({}) Tj\n",
                format_pdf_number(x - x_prev),
                format_pdf_number(y - y_prev),
                encode_pdf_text(line),
            ));
            x_prev = x;
            y_prev = y;
        }
        content.push_str("ET\n");
    }
    content.push_str("Q\nEMC");
    content
}

/// A checkbox's check: the mark the checkbox element in DocSpring's page
/// designer draws, in a square as large as fits, in the middle
fn check_appearance(rect: PdfRect, rgb: &str) -> String {
    let side = rect.width.min(rect.height);
    let left = (rect.width - side) / 2.0;
    let bottom = (rect.height - side) / 2.0;
    let at = |x: f32, y: f32| {
        format!(
            "{} {}",
            format_pdf_number(left + x * side),
            format_pdf_number(bottom + (1.0 - y) * side)
        )
    };
    format!(
        "q\n{rgb} RG\n{} w\n1 J\n1 j\n{} m\n{} l\n{} l\nS\nQ",
        format_pdf_number(side * 0.12),
        at(0.24, 0.52),
        at(0.43, 0.7),
        at(0.77, 0.3),
    )
}

/// A radio button's dot, in the middle
fn dot_appearance(rect: PdfRect, rgb: &str) -> String {
    let r = rect.width.min(rect.height) * 0.25;
    let (cx, cy) = (rect.width / 2.0, rect.height / 2.0);
    // A circle as four Bézier curves
    let k = r * 0.552_284_8;
    let n = |v: f32| format_pdf_number(v);
    format!(
        "q\n{rgb} rg\n{} {} m\n{} {} {} {} {} {} c\n{} {} {} {} {} {} c\n{} {} {} {} {} {} c\n{} {} {} {} {} {} c\nf\nQ",
        n(cx + r),
        n(cy),
        n(cx + r),
        n(cy + k),
        n(cx + k),
        n(cy + r),
        n(cx),
        n(cy + r),
        n(cx - k),
        n(cy + r),
        n(cx - r),
        n(cy + k),
        n(cx - r),
        n(cy),
        n(cx - r),
        n(cy - k),
        n(cx - k),
        n(cy - r),
        n(cx),
        n(cy - r),
        n(cx + k),
        n(cy - r),
        n(cx + r),
        n(cy - k),
        n(cx + r),
        n(cy),
    )
}

fn helvetica_width(text: &str, size: f32) -> f32 {
    text.chars()
        .map(|ch| crate::fonts::char_width(ch, size, &FontFamily::Helvetica, false))
        .sum()
}

/// A multi-line field's text in lines that fit its width: its own line breaks,
/// then breaks between words (and inside a word longer than the width)
fn wrap_helvetica(text: &str, size: f32, width: f32) -> Vec<String> {
    let mut lines = Vec::new();
    for paragraph in text.split('\n') {
        let paragraph = paragraph.trim_end_matches('\r');
        let mut line = String::new();
        for word in paragraph.split(' ') {
            let candidate = if line.is_empty() {
                word.to_string()
            } else {
                format!("{line} {word}")
            };
            if helvetica_width(&candidate, size) <= width || line.is_empty() && word.is_empty() {
                line = candidate;
                continue;
            }
            if !line.is_empty() {
                lines.push(std::mem::take(&mut line));
            }
            // A word wider than the field breaks where it reaches the edge
            for ch in word.chars() {
                line.push(ch);
                if helvetica_width(&line, size) > width && line.chars().count() > 1 {
                    line.pop();
                    lines.push(std::mem::take(&mut line));
                    line.push(ch);
                }
            }
        }
        lines.push(line);
    }
    lines
}

/// A PDF text string: literal for printable ASCII and line breaks, UTF-16BE otherwise
fn pdf_text_string(text: &str) -> String {
    if text
        .chars()
        .all(|ch| (' '..='~').contains(&ch) || matches!(ch, '\n' | '\r' | '\t'))
    {
        let escaped = escape_pdf_string(text)
            .replace('\n', "\\n")
            .replace('\r', "\\r")
            .replace('\t', "\\t");
        format!("({escaped})")
    } else {
        let hex: String = std::iter::once(0xFEFF_u16)
            .chain(text.encode_utf16())
            .map(|unit| format!("{unit:04X}"))
            .collect();
        format!("<{hex}>")
    }
}

/// A radio button's value as a PDF name's characters: letters, digits, `-`
/// and `_` kept, anything else as `#xx` (and `on` for an empty value)
fn pdf_name_token(value: &str) -> String {
    if value.is_empty() {
        return "on".to_string();
    }
    value
        .bytes()
        .map(|byte| match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' => (byte as char).to_string(),
            _ => format!("#{byte:02X}"),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_are_unique_and_unnamed_fields_are_numbered() {
        let mut state = FormWriterState::default();
        assert_eq!(state.unique_name(Some("city")), "city");
        assert_eq!(state.unique_name(Some("city")), "city_2");
        assert_eq!(state.unique_name(Some("a.b")), "a_b");
        assert_eq!(state.unique_name(None), "field_1");
        assert_eq!(state.unique_name(Some("field_2")), "field_2");
        assert_eq!(state.unique_name(None), "field_2_2");
    }

    #[test]
    fn text_strings_and_names_are_escaped() {
        assert_eq!(pdf_text_string("a (b)"), "(a \\(b\\))");
        assert_eq!(pdf_text_string("Zoë"), "<FEFF005A006F00EB>");
        assert_eq!(pdf_text_string("Two\nlines"), "(Two\\nlines)");
        assert_eq!(pdf_name_token("large size"), "large#20size");
        assert_eq!(pdf_name_token(""), "on");
    }

    #[test]
    fn multiline_text_wraps_at_words_and_line_breaks() {
        let lines = wrap_helvetica(
            "one two three\nfour",
            10.0,
            helvetica_width("one two", 10.0) + 0.1,
        );
        assert_eq!(lines, vec!["one two", "three", "four"]);
        let long = wrap_helvetica("abcdefgh", 10.0, helvetica_width("abc", 10.0) + 0.1);
        assert_eq!(long, vec!["abc", "def", "gh"]);
    }
}
