//! Interactive form fields (AcroForm) for HTML form controls.
//!
//! Layout records what each `<input>`, `<textarea>` and `<select>` asks for on
//! the text block it lays the control out as. When the converter's `pdf_forms`
//! option is on, the PDF writer turns these into widget annotations, so the
//! PDF can be filled in a viewer, as WeasyPrint's `pdf_forms` option does.

use crate::parser::dom::{DomNode, ElementNode, HtmlTag};
use crate::style::computed::TextAlign;
use crate::types::Color;

/// A form control's field, as the HTML describes it.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct FormField {
    /// The `name` attribute. The writer names unnamed fields and makes repeated
    /// names unique (except a radio group's, which is one field).
    pub(crate) name: Option<String>,
    pub(crate) kind: FormFieldKind,
    /// `readonly` or `disabled`
    pub(crate) read_only: bool,
    pub(crate) required: bool,
    pub(crate) font_size: f32,
    pub(crate) color: Color,
    pub(crate) align: TextAlign,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum FormFieldKind {
    Text {
        value: String,
        multiline: bool,
        password: bool,
        max_length: Option<u32>,
    },
    Checkbox {
        checked: bool,
    },
    /// One button of the radio group named by the field's `name`
    Radio {
        value: String,
        checked: bool,
    },
    /// A drop-down list: `(export value, label)` pairs, and the selected index
    Choice {
        options: Vec<(String, String)>,
        selected: Option<usize>,
    },
}

/// Text-like `<input>` types, which become text fields
const TEXT_INPUT_TYPES: &[&str] = &[
    "text",
    "email",
    "number",
    "tel",
    "url",
    "search",
    "password",
    "date",
    "time",
    "datetime-local",
    "month",
    "week",
];

impl FormField {
    /// The field for a form control, or `None` for an element that isn't one
    /// (or an `<input>` type with no field, such as a button or a file input).
    pub(crate) fn from_element(
        el: &ElementNode,
        font_size: f32,
        color: Color,
        align: TextAlign,
    ) -> Option<Self> {
        let attribute = |name: &str| el.attributes.get(name).map(String::as_str);
        let kind = match el.tag {
            HtmlTag::Input => {
                let input_type = attribute("type").unwrap_or("text").to_ascii_lowercase();
                match input_type.as_str() {
                    "checkbox" => FormFieldKind::Checkbox {
                        checked: el.attributes.contains_key("checked"),
                    },
                    "radio" => FormFieldKind::Radio {
                        value: attribute("value").unwrap_or("on").to_string(),
                        checked: el.attributes.contains_key("checked"),
                    },
                    other if TEXT_INPUT_TYPES.contains(&other) => FormFieldKind::Text {
                        value: attribute("value").unwrap_or_default().to_string(),
                        multiline: false,
                        password: other == "password",
                        max_length: attribute("maxlength").and_then(|v| v.trim().parse().ok()),
                    },
                    _ => return None,
                }
            }
            HtmlTag::Textarea => FormFieldKind::Text {
                value: text_content(&el.children),
                multiline: true,
                password: false,
                max_length: attribute("maxlength").and_then(|v| v.trim().parse().ok()),
            },
            HtmlTag::Select => {
                let mut options = Vec::new();
                let mut selected = None;
                collect_options(&el.children, &mut options, &mut selected);
                // A drop-down shows its first option until one is selected
                if selected.is_none() && !options.is_empty() {
                    selected = Some(0);
                }
                FormFieldKind::Choice { options, selected }
            }
            _ => return None,
        };
        Some(Self {
            name: attribute("name")
                .map(str::trim)
                .filter(|name| !name.is_empty())
                .map(str::to_string),
            kind,
            read_only: el.attributes.contains_key("readonly")
                || el.attributes.contains_key("disabled"),
            required: el.attributes.contains_key("required"),
            font_size,
            color,
            align,
        })
    }
}

/// A `<select>`'s options, in order, including those in `<optgroup>`s
fn collect_options(
    children: &[DomNode],
    options: &mut Vec<(String, String)>,
    selected: &mut Option<usize>,
) {
    for child in children {
        let DomNode::Element(el) = child else {
            continue;
        };
        match el.raw_tag_name.as_str() {
            "option" => {
                let label = text_content(&el.children).trim().to_string();
                let value = el
                    .attributes
                    .get("value")
                    .cloned()
                    .unwrap_or_else(|| label.clone());
                if el.attributes.contains_key("selected") {
                    *selected = Some(options.len());
                }
                options.push((value, label));
            }
            "optgroup" => collect_options(&el.children, options, selected),
            _ => {}
        }
    }
}

fn text_content(children: &[DomNode]) -> String {
    let mut text = String::new();
    for child in children {
        match child {
            DomNode::Text(s) => text.push_str(s),
            DomNode::Element(el) => text.push_str(&text_content(&el.children)),
        }
    }
    text
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::html::parse_html;

    fn field(html: &str) -> Option<FormField> {
        let doc = parse_html(html).unwrap();
        fn find(nodes: &[DomNode]) -> Option<&ElementNode> {
            for node in nodes {
                if let DomNode::Element(el) = node {
                    if matches!(el.tag, HtmlTag::Input | HtmlTag::Textarea | HtmlTag::Select) {
                        return Some(el);
                    }
                    if let Some(found) = find(&el.children) {
                        return Some(found);
                    }
                }
            }
            None
        }
        let el = find(&doc)?;
        FormField::from_element(el, 10.0, Color::BLACK, TextAlign::Left)
    }

    #[test]
    fn text_inputs_keep_their_value_not_their_placeholder() {
        let f = field(
            r#"<input name="city" placeholder="City" value="Paris" maxlength="20" required>"#,
        )
        .unwrap();
        assert_eq!(f.name.as_deref(), Some("city"));
        assert!(f.required);
        assert_eq!(
            f.kind,
            FormFieldKind::Text {
                value: "Paris".into(),
                multiline: false,
                password: false,
                max_length: Some(20)
            }
        );
        let empty = field(r#"<input placeholder="City">"#).unwrap();
        assert!(matches!(empty.kind, FormFieldKind::Text { ref value, .. } if value.is_empty()));
        assert_eq!(empty.name, None);
    }

    #[test]
    fn buttons_and_file_inputs_are_not_fields() {
        assert_eq!(field(r#"<input type="submit" value="Send">"#), None);
        assert_eq!(field(r#"<input type="file">"#), None);
    }

    #[test]
    fn checkboxes_radios_and_textareas() {
        let check = field(r#"<input type="checkbox" checked disabled>"#).unwrap();
        assert_eq!(check.kind, FormFieldKind::Checkbox { checked: true });
        assert!(check.read_only);
        let radio = field(r#"<input type="radio" name="size" value="large">"#).unwrap();
        assert_eq!(
            radio.kind,
            FormFieldKind::Radio {
                value: "large".into(),
                checked: false
            }
        );
        let area = field("<textarea name=\"notes\">Line 1\nLine 2</textarea>").unwrap();
        assert!(
            matches!(area.kind, FormFieldKind::Text { multiline: true, ref value, .. } if value == "Line 1\nLine 2")
        );
    }

    #[test]
    fn selects_list_their_options_and_the_selected_one() {
        let f = field(
            r#"<select name="c"><option value="nz">New Zealand</option><optgroup label="E"><option selected>France</option></optgroup></select>"#,
        )
        .unwrap();
        assert_eq!(
            f.kind,
            FormFieldKind::Choice {
                options: vec![
                    ("nz".into(), "New Zealand".into()),
                    ("France".into(), "France".into())
                ],
                selected: Some(1)
            }
        );
        let first = field(r#"<select><option>A</option><option>B</option></select>"#).unwrap();
        assert!(matches!(
            first.kind,
            FormFieldKind::Choice {
                selected: Some(0),
                ..
            }
        ));
    }
}
