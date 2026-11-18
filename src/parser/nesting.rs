//! How deeply a document's elements may nest. Converting html5ever's tree, laying it out, measuring it and painting
//! it each recurse a frame or a few per level of element nesting, so one check of the parsed tree, before any of them
//! runs, bounds the stack all of them use.

use markup5ever_rcdom::{Handle, NodeData};

use super::dom::HtmlTag;

/// How many elements nested in each other a document may have, counting the element itself but not `html` and
/// `body`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct NestingLimit {
    max: usize,
}

/// A document whose elements nest deeper than its [`NestingLimit`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct NestedTooDeep {
    max: usize,
}

impl std::fmt::Display for NestedTooDeep {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "elements are nested more than {} deep", self.max)
    }
}

impl NestingLimit {
    /// Each level of a flex container takes about 60 KB of stack in a release build (four times that in a debug
    /// one), so on the 2 MiB stack of a thread from `std::thread::spawn` this is about as deep as a release build can
    /// go.
    pub(crate) const DEFAULT_MAX: usize = 31;

    pub(crate) const fn new(max: usize) -> Self {
        Self { max }
    }

    /// Whether every element under `document` is within the limit. Walks the tree without recursing, so a document
    /// of any depth is checked on any stack.
    pub(crate) fn check(self, document: &Handle) -> Result<(), NestedTooDeep> {
        let mut pending: Vec<(Handle, usize)> = vec![(document.clone(), 0)];
        while let Some((node, depth)) = pending.pop() {
            for child in node.children.borrow().iter() {
                let NodeData::Element { name, .. } = &child.data else {
                    continue;
                };
                let depth = match HtmlTag::from_tag_name(name.local.as_ref()) {
                    HtmlTag::Html | HtmlTag::Body => depth,
                    _ => depth + 1,
                };
                if depth > self.max {
                    return Err(NestedTooDeep { max: self.max });
                }
                pending.push((child.clone(), depth));
            }
        }
        Ok(())
    }
}

impl Default for NestingLimit {
    fn default() -> Self {
        Self::new(Self::DEFAULT_MAX)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::IronpressError;
    use crate::parser::html::parse_html_within;

    fn check(markup: &str, max: usize) -> Result<(), String> {
        match parse_html_within(markup, NestingLimit::new(max)) {
            Ok(_) => Ok(()),
            Err(IronpressError::LayoutError(message)) => Err(message),
            Err(error) => panic!("{error}"),
        }
    }

    fn nested(open: &str, close: &str, levels: usize) -> String {
        format!(
            "<html><body>{}x{}</body></html>",
            open.repeat(levels),
            close.repeat(levels)
        )
    }

    #[test]
    fn counts_elements_but_not_html_and_body() {
        assert_eq!(check(&nested("<span>", "</span>", 3), 3), Ok(()));
        assert_eq!(
            check(&nested("<span>", "</span>", 4), 3),
            Err("elements are nested more than 3 deep".to_string())
        );
    }

    #[test]
    fn counts_every_kind_of_element_and_only_the_deepest_branch_matters() {
        // Inline, grid and SVG nesting recurse in layout and paint as blocks do
        let markup = "<p>shallow</p><div style=\"display:grid\"><b><svg><g><rect/></g></svg></b></div><i>x</i>";
        assert_eq!(check(markup, 5), Ok(()));
        assert!(check(markup, 4).is_err());
    }

    #[test]
    fn counts_the_elements_the_parser_builds_not_the_tags_written() {
        // A `/` doesn't close a `<div>`, and the parser adds a `<tbody>` to each table
        assert!(check(&nested("<div/>", "", 4), 3).is_err());
        assert!(check(&nested("<table><tr><td>", "</td></tr></table>", 1), 3).is_err());
        assert_eq!(
            check(&nested("<table><tr><td>", "</td></tr></table>", 1), 4),
            Ok(())
        );
    }

    #[test]
    fn checks_a_very_deep_document_without_recursing() {
        // Run on a small stack: neither the check nor html5ever's tree may recurse
        std::thread::Builder::new()
            .stack_size(256 * 1024)
            .spawn(|| {
                let markup = "<div/>".repeat(4_000);
                let result = check(&markup, 500);
                assert_eq!(
                    result,
                    Err("elements are nested more than 500 deep".to_string())
                );
            })
            .unwrap()
            .join()
            .unwrap();
    }
}
