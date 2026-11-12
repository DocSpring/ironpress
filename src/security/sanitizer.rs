use crate::error::IronpressError;
use crate::parser::dom::{DomNode, HtmlTag};
use crate::security::resources::DocumentResources;
#[cfg(test)]
use crate::security::resources::NetworkPolicy;

/// Maximum allowed HTML input size (10 MB).
const MAX_INPUT_SIZE: usize = 10 * 1024 * 1024;

/// Maximum allowed nesting depth.
const MAX_NESTING_DEPTH: usize = 500;

/// Sanitize HTML input by removing dangerous elements and attributes.
#[cfg(test)]
pub(crate) fn sanitize_html(html: &str) -> Result<String, IronpressError> {
    sanitize_html_with_resources(
        html,
        &DocumentResources::new(None, None, NetworkPolicy::default()),
    )
}

pub(crate) fn sanitize_html_with_resources(
    html: &str,
    resources: &DocumentResources,
) -> Result<String, IronpressError> {
    // Check input size
    if html.len() > MAX_INPUT_SIZE {
        return Err(IronpressError::SecurityError(format!(
            "Input exceeds maximum size of {MAX_INPUT_SIZE} bytes"
        )));
    }

    // Check nesting depth
    if check_nesting_depth(html) > MAX_NESTING_DEPTH {
        return Err(IronpressError::SecurityError(
            "HTML nesting depth exceeds maximum".to_string(),
        ));
    }

    let mut result = html.to_string();

    // Remove script tags and content
    result = remove_tag_with_content(&result, "script");
    // Note: <style> tags are preserved for CSS support, but sanitized
    result = sanitize_style_tags(&result, resources);
    result = remove_tag_with_content(&result, "iframe");
    result = remove_tag_with_content(&result, "object");
    result = remove_tag_with_content(&result, "embed");
    result = remove_tag_with_content(&result, "form");

    // Remove event handler attributes
    result = remove_event_handlers(&result);

    // Remove javascript: URLs
    result = result.replace("javascript:", "");

    Ok(result)
}

/// Apply resource authorization to parsed attributes that can reach a loader.
///
/// Stylesheet URLs are handled separately because imported sheets have their
/// own base directory. Inline declarations and replaced-element attributes use
/// the document base.
pub(crate) fn sanitize_dom_resources(nodes: &mut [DomNode], resources: &DocumentResources) {
    for node in nodes {
        let DomNode::Element(element) = node else {
            continue;
        };

        if let Some(style) = element.attributes.get_mut("style") {
            *style = resources.rewrite_css_urls(style, resources.base_path());
        }

        if element.tag == HtmlTag::Img {
            authorize_attribute(&mut element.attributes, "src", resources);
        }
        if element.raw_tag_name.eq_ignore_ascii_case("image") {
            authorize_attribute(&mut element.attributes, "href", resources);
            authorize_attribute(&mut element.attributes, "xlink:href", resources);
        }

        sanitize_dom_resources(&mut element.children, resources);
    }
}

/// Image references that the HTML tree can request independently of layout.
#[cfg(feature = "remote")]
pub(crate) fn document_image_references(nodes: &[DomNode]) -> Vec<&str> {
    fn collect<'a>(nodes: &'a [DomNode], references: &mut Vec<&'a str>) {
        for node in nodes {
            let DomNode::Element(element) = node else {
                continue;
            };
            if element.tag == HtmlTag::Img {
                references.extend(element.attributes.get("src").map(String::as_str));
            }
            if element.raw_tag_name.eq_ignore_ascii_case("image") {
                references.extend(element.attributes.get("href").map(String::as_str));
                references.extend(element.attributes.get("xlink:href").map(String::as_str));
            }
            collect(&element.children, references);
        }
    }

    let mut references = Vec::new();
    collect(nodes, &mut references);
    references
}

fn authorize_attribute(
    attributes: &mut std::collections::HashMap<String, String>,
    name: &str,
    resources: &DocumentResources,
) {
    let Some(value) = attributes.get(name) else {
        return;
    };
    match resources.resolve(value, resources.base_path()) {
        Some(authorized) => {
            attributes.insert(name.to_string(), authorized.reference());
        }
        None => {
            attributes.remove(name);
        }
    }
}

fn remove_tag_with_content(html: &str, tag: &str) -> String {
    let mut result = html.to_string();
    let open = format!("<{tag}");
    let close = format!("</{tag}>");

    loop {
        let lower = result.to_ascii_lowercase();
        let start = lower.find(&open);
        let end = lower.find(&close);

        match (start, end) {
            (Some(s), Some(e)) => {
                let end_pos = e + close.len();
                result = format!("{}{}", &result[..s], &result[end_pos..]);
            }
            (Some(s), None) => {
                // Self-closing or unclosed — remove from start to end of tag
                if let Some(gt) = result[s..].find('>') {
                    result = format!("{}{}", &result[..s], &result[s + gt + 1..]);
                } else {
                    break;
                }
            }
            _ => break,
        }
    }

    result
}

fn sanitize_style_tags(html: &str, resources: &DocumentResources) -> String {
    let mut result = String::new();
    let mut remaining = html;

    loop {
        let lower = remaining.to_ascii_lowercase();
        let start = lower.find("<style");
        let end = lower.find("</style>");

        match (start, end) {
            (Some(s), Some(e)) => {
                // Add everything before the <style> tag
                result.push_str(&remaining[..s]);

                // Find end of opening tag
                if let Some(gt) = remaining[s..].find('>') {
                    let css_start = s + gt + 1;
                    if css_start > e {
                        // Malformed: </style> appears before the opening tag closes.
                        // Skip past the </style> and continue scanning.
                        remaining = &remaining[e + 8..];
                        continue;
                    }
                    let css = &remaining[css_start..e];
                    // An import is retained only when conversion has an
                    // explicit local authorization root; the import resolver
                    // applies the same canonical descendant boundary.
                    let import_safe_css = if resources.has_authorized_root() {
                        css.to_string()
                    } else {
                        remove_ascii_case_insensitive(css, "@import")
                    };
                    let expression_safe_css =
                        remove_ascii_case_insensitive(&import_safe_css, "expression(");
                    let expression_safe_css =
                        remove_ascii_case_insensitive(&expression_safe_css, "expression (");
                    let safe_css =
                        resources.rewrite_css_urls(&expression_safe_css, resources.base_path());
                    result.push_str("<style>");
                    result.push_str(&safe_css);
                    result.push_str("</style>");
                    remaining = &remaining[e + 8..];
                } else {
                    result.push_str(remaining);
                    break;
                }
            }
            _ => {
                result.push_str(remaining);
                break;
            }
        }
    }

    result
}

fn remove_ascii_case_insensitive(value: &str, needle: &str) -> String {
    let mut result = String::with_capacity(value.len());
    let mut remaining = value;
    while let Some(position) = remaining.to_ascii_lowercase().find(needle) {
        result.push_str(&remaining[..position]);
        remaining = &remaining[position + needle.len()..];
    }
    result.push_str(remaining);
    result
}

fn remove_event_handlers(html: &str) -> String {
    // Only remove onXXX attributes inside HTML tags
    let mut result = String::with_capacity(html.len());
    let mut in_tag = false;
    // Quote char of the attribute value currently being scanned (0 = none).
    // The onXXX heuristic must NOT fire inside a quoted value, or an ordinary
    // attribute value token that happens to start with "on" (e.g. the class
    // name `one` in `class="chip one"`) would be mistaken for an event handler
    // and deleted, breaking the quote and corrupting the rest of the tag.
    let mut in_quote: u8 = 0;

    let bytes = html.as_bytes();
    let mut i = 0;

    while i < bytes.len() {
        // Skip multi-byte UTF-8 sequences — they are never HTML syntax
        if bytes[i] & 0x80 != 0 {
            // Determine UTF-8 sequence length and copy all bytes
            let seq_len = if bytes[i] & 0xE0 == 0xC0 {
                2
            } else if bytes[i] & 0xF0 == 0xE0 {
                3
            } else if bytes[i] & 0xF8 == 0xF0 {
                4
            } else {
                1 // invalid, copy single byte
            };
            let end = (i + seq_len).min(bytes.len());
            if let Ok(s) = std::str::from_utf8(&bytes[i..end]) {
                result.push_str(s);
            }
            i = end;
            continue;
        }

        let c = bytes[i] as char;

        // Track quoted attribute values so tag-structure characters and the
        // onXXX heuristic below are only interpreted outside of quotes.
        if in_tag && in_quote != 0 {
            if bytes[i] == in_quote {
                in_quote = 0;
            }
            result.push(c);
            i += 1;
            continue;
        }

        if in_tag && (c == '"' || c == '\'') {
            in_quote = bytes[i];
            result.push(c);
            i += 1;
            continue;
        }

        if c == '<' {
            in_tag = true;
            result.push(c);
            i += 1;
            continue;
        }

        if c == '>' {
            in_tag = false;
            result.push(c);
            i += 1;
            continue;
        }

        if in_tag && (c == 'o' || c == 'O') && i + 2 < bytes.len() {
            let next = bytes[i + 1] as char;
            if (next == 'n' || next == 'N') && (bytes[i + 2] as char).is_ascii_alphabetic() {
                // Check there's a space or start of tag before this
                let prev = if i > 0 { bytes[i - 1] as char } else { ' ' };
                if prev == ' ' || prev == '\t' || prev == '\n' {
                    // This looks like an event handler attribute — skip it
                    // Skip attribute name
                    let mut j = i;
                    while j < bytes.len()
                        && bytes[j] != b'='
                        && bytes[j] != b' '
                        && bytes[j] != b'>'
                    {
                        j += 1;
                    }
                    // Skip = and quoted value
                    if j < bytes.len() && bytes[j] == b'=' {
                        j += 1;
                        // Skip whitespace
                        while j < bytes.len() && (bytes[j] as char).is_whitespace() {
                            j += 1;
                        }
                        if j < bytes.len() && (bytes[j] == b'"' || bytes[j] == b'\'') {
                            let quote = bytes[j];
                            j += 1;
                            while j < bytes.len() && bytes[j] != quote {
                                j += 1;
                            }
                            if j < bytes.len() {
                                j += 1; // skip closing quote
                            }
                        } else {
                            // Unquoted — skip to space or >
                            while j < bytes.len() && bytes[j] != b' ' && bytes[j] != b'>' {
                                j += 1;
                            }
                        }
                    }
                    i = j;
                    continue;
                }
            }
        }

        result.push(c);
        i += 1;
    }

    result
}

/// Elements that never have content (HTML's void elements), so they add no depth.
const VOID_ELEMENTS: &[&str] = &[
    "area", "base", "br", "col", "embed", "hr", "img", "input", "link", "meta", "param", "source",
    "track", "wbr",
];

/// Elements whose content is text, up to their end tag, whatever `<` it contains.
const RAW_TEXT_ELEMENTS: &[&str] = &["script", "style", "textarea", "title", "xmp", "noscript"];

/// How deeply elements nest: each start tag of an element that can hold content opens a level and its end tag closes
/// it. Comments, the doctype, processing instructions, void elements and self-closing tags hold nothing, so they don't
/// count, and neither does a `/` or `>` inside a quoted attribute value, or a tag in the text of a raw text element
/// (a `<style>`, a `<script>`).
fn check_nesting_depth(html: &str) -> usize {
    let bytes = html.as_bytes();
    let mut depth: usize = 0;
    let mut max_depth: usize = 0;
    let mut i = 0;

    while let Some(offset) = bytes[i..].iter().position(|&b| b == b'<') {
        let start = i + offset;
        let rest = &bytes[start + 1..];
        match rest.first() {
            Some(b'!') if rest.starts_with(b"!--") => {
                i = find(bytes, start + 4, b"-->").map_or(bytes.len(), |end| end + 3);
            }
            Some(b'!') | Some(b'?') => {
                i = find(bytes, start + 1, b">").map_or(bytes.len(), |end| end + 1);
            }
            Some(b'/') => {
                depth = depth.saturating_sub(1);
                i = find(bytes, start + 1, b">").map_or(bytes.len(), |end| end + 1);
            }
            Some(c) if c.is_ascii_alphabetic() => {
                let name_end = start
                    + 1
                    + rest
                        .iter()
                        .position(|b| b.is_ascii_whitespace() || *b == b'/' || *b == b'>')
                        .unwrap_or(rest.len());
                let name = html[start + 1..name_end].to_ascii_lowercase();
                let (tag_end, self_closing) = tag_end(bytes, name_end);
                i = tag_end;
                if self_closing || VOID_ELEMENTS.contains(&name.as_str()) {
                    continue;
                }
                if RAW_TEXT_ELEMENTS.contains(&name.as_str()) {
                    // Its text runs to its end tag, which closes it again
                    let close = format!("</{name}");
                    i = find_ignoring_case(bytes, i, close.as_bytes())
                        .and_then(|end| find(bytes, end, b">"))
                        .map_or(bytes.len(), |end| end + 1);
                    max_depth = max_depth.max(depth + 1);
                    continue;
                }
                depth += 1;
                max_depth = max_depth.max(depth);
            }
            // A `<` that starts no tag is text
            _ => i = start + 1,
        }
    }

    max_depth
}

/// Where the start tag whose name ends at `from` ends (just after its `>`), and whether it ends with `/>`. Quoted
/// attribute values may hold `>` and `/`.
fn tag_end(bytes: &[u8], from: usize) -> (usize, bool) {
    let mut quote: Option<u8> = None;
    let mut last = 0u8;
    let mut i = from;
    while i < bytes.len() {
        let b = bytes[i];
        match quote {
            Some(q) if b == q => quote = None,
            Some(_) => {}
            None if b == b'"' || b == b'\'' => quote = Some(b),
            None if b == b'>' => return (i + 1, last == b'/'),
            None if !b.is_ascii_whitespace() => last = b,
            None => {}
        }
        i += 1;
    }
    (bytes.len(), false)
}

fn find(bytes: &[u8], from: usize, needle: &[u8]) -> Option<usize> {
    bytes
        .get(from..)?
        .windows(needle.len())
        .position(|w| w == needle)
        .map(|p| from + p)
}

fn find_ignoring_case(bytes: &[u8], from: usize, needle: &[u8]) -> Option<usize> {
    bytes
        .get(from..)?
        .windows(needle.len())
        .position(|w| w.eq_ignore_ascii_case(needle))
        .map(|p| from + p)
}

#[cfg(test)]
mod tests;
