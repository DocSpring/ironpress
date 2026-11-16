//! Scrollable overflow that a box's own content adds for print fitting.
//!
//! Chromium shrinks a printed document to the width of its scrollable
//! overflow, which includes line boxes wider than their block and flex items
//! wider than their container (css-overflow-3 §2.2). A box whose `overflow`
//! clips on the inline axis keeps that overflow inside itself.

use std::collections::HashMap;

use crate::layout::elements::LayoutElement;
use crate::layout::engine::{FlexCell, FlexNestedOrigin, TextLine};
use crate::parser::ttf::TtfFont;

/// What a box contributes to print fitting beyond its own border box.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PrintFitContent {
    /// Its in-flow children contribute recursively, in its content box.
    Descendants,
    /// The box measures its own content, children included, through
    /// `content_overflow_right_edge`, since they aren't positioned in its
    /// content box (a flex row's items).
    Measured,
    /// The box clips its inline overflow, so nothing inside it widens the
    /// document.
    Clipped,
}

/// Inline-end edge of laid-out lines whose start edge is `content_left`.
///
/// A line too long for its box starts at the box's start edge whatever its
/// `text-align` (css-text-3 §6.1), as the renderer draws it, so only its
/// indent and offset move it.
pub(crate) fn text_lines_right_edge(
    lines: &[TextLine],
    content_left: f32,
    first_line_indent: f32,
    fonts: &HashMap<String, TtfFont>,
) -> Option<f32> {
    lines
        .iter()
        .enumerate()
        .filter(|(_, line)| !line.runs.is_empty())
        .map(|(index, line)| {
            let indent = if index == 0 { first_line_indent } else { 0.0 };
            content_left + indent + line.x_offset + inked_line_width(line, fonts)
        })
        .filter(|right| right.is_finite())
        .reduce(f32::max)
}

/// A line's width without the white space at its end, which hangs past the
/// line box instead of overflowing it (css-text-3 §4.1.3).
fn inked_line_width(line: &TextLine, fonts: &HashMap<String, TtfFont>) -> f32 {
    let Some(last) = line.runs.iter().rposition(|run| !run.text.is_empty()) else {
        return crate::layout::helpers::measure_runs_width(&line.runs, fonts);
    };
    let trimmed = line.runs[last].text.trim_end();
    if trimmed.len() == line.runs[last].text.len() {
        return crate::layout::helpers::measure_runs_width(&line.runs, fonts);
    }
    let mut runs = line.runs.clone();
    runs[last] = line.runs[last].text_fragment(trimmed.to_owned(), true);
    crate::layout::helpers::measure_runs_width(&runs, fonts)
}

/// Inline-end edge of a flex row's items, their lines of text and their
/// nested boxes, where `content_left` is the row's content-box start edge.
pub(crate) fn flex_row_content_right_edge(
    cells: &[FlexCell],
    content_left: f32,
    fonts: &HashMap<String, TtfFont>,
) -> Option<f32> {
    cells
        .iter()
        .filter(|cell| cell.positioning.is_in_normal_flow())
        .filter_map(|cell| flex_cell_right_edge(cell, content_left, fonts))
        .filter(|right| right.is_finite())
        .reduce(f32::max)
}

fn flex_cell_right_edge(
    cell: &FlexCell,
    row_content_left: f32,
    fonts: &HashMap<String, TtfFont>,
) -> Option<f32> {
    let border_left = row_content_left + cell.x_offset;
    let content_left = border_left + cell.border.left.width + cell.padding.left;
    let nested_left = match cell.nested_origin {
        FlexNestedOrigin::ContentBox => content_left,
        FlexNestedOrigin::TableBorderBox => border_left,
    };
    let mut right = border_left + cell.width;
    if let Some(lines_right) = text_lines_right_edge(&cell.lines, content_left, 0.0, fonts) {
        right = right.max(lines_right);
    }
    for nested in &cell.nested_elements {
        if let Some(nested_right) = nested.print_fit_right_edge(fonts) {
            right = right.max(nested_left + nested_right);
        }
    }
    right.is_finite().then_some(right)
}
