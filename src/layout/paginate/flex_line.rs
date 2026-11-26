//! A single flex line taller than the space left on its page breaks inside its items: each item's content breaks
//! at the page end, between its lines or block children, and its continuation starts the next page at the same
//! inline position (CSS Fragmentation 3 §5.2, CSS Flexbox 1 §12.1 "breaks inside flex items", as Chrome prints a
//! row of columns). An item whose content is all placed ends on this page; the others continue with their bottom
//! padding and border.

use super::{FragmentBreakRule, LayoutNode, simulate_block_flow, split_container_node};
use crate::layout::elements::Container;
use crate::layout::engine::{FlexCell, FlexRow, TextLine};
use crate::layout::roundoff::exceeds_with_roundoff;

/// What of one item's content fits above the page end, and what continues
struct ItemBreak {
    first: FlexCell,
    rest: Option<FlexCell>,
    /// Whether any of the item's content was placed above the page end
    placed: bool,
}

/// Split a one-line flex row whose items are taller than `avail_inner`, the space below its content-box top
pub(super) fn split_flex_line(
    element: &FlexRow,
    avail_inner: f32,
    rule: FragmentBreakRule,
) -> Option<(LayoutNode, LayoutNode)> {
    let mut breaks: Vec<ItemBreak> = element
        .content
        .cells
        .iter()
        .map(|cell| break_item(cell, avail_inner, rule))
        .collect();
    if breaks.iter().all(|item| item.rest.is_none()) {
        return None;
    }
    if !breaks.iter().any(|item| item.placed) {
        // Nothing fits above the page end: the row moves to the next page, unless it already starts one, where the
        // emergency rule places the first line or child of its first unfinished item to make progress
        if rule != FragmentBreakRule::Emergency {
            return None;
        }
        let index = breaks.iter().position(|item| item.rest.is_some())?;
        breaks[index] = force_item_progress(&element.content.cells[index])?;
    }

    let mut first = element.clone();
    first.content.cells = breaks.iter().map(|item| item.first.clone()).collect();
    first.content.row_height = avail_inner;
    for cell in &mut first.content.cells {
        if cell.line_cross_size > 0.0 {
            cell.line_cross_size = avail_inner;
        }
    }
    first.box_model.margins.end = 0.0;
    first.box_model.padding.bottom = 0.0;
    first.box_model.border.bottom.width = 0.0;

    let mut rest = element.clone();
    rest.content.cells = breaks.into_iter().filter_map(|item| item.rest).collect();
    let rest_height = rest
        .content
        .cells
        .iter()
        .map(|cell| cell.y_offset + cell.natural_height)
        .fold(0.0_f32, f32::max);
    rest.content.row_height = rest_height;
    for cell in &mut rest.content.cells {
        if cell.line_cross_size > 0.0 {
            cell.line_cross_size = rest_height;
        }
    }
    rest.content.forced_line_breaks.clear();
    rest.box_model.margins.start = 0.0;
    rest.box_model.padding.top = 0.0;
    rest.box_model.border.top.width = 0.0;

    Some((Box::new(first), Box::new(rest)))
}

/// Break one item at `avail_inner` below the row's content-box top
fn break_item(cell: &FlexCell, avail_inner: f32, rule: FragmentBreakRule) -> ItemBreak {
    let content_top = cell.y_offset + cell.border.top.width + cell.padding.top;
    let space = avail_inner - content_top;
    let content_height = item_content_height(cell);
    if !exceeds_with_roundoff(
        content_top + content_height + end_decoration(cell),
        avail_inner,
    ) {
        return ItemBreak {
            first: cell.clone(),
            rest: None,
            placed: true,
        };
    }

    let mut lines = cell.lines.clone();
    let lines_first = take_fitting_lines(&mut lines, space);
    let lines_height: f32 = lines_first.iter().map(|line| line.height).sum();
    let (nested_first, nested_rest) = if lines.is_empty() {
        split_nested(&cell.nested_elements, space - lines_height, rule)
    } else {
        (Vec::new(), cell.nested_elements.clone())
    };
    let placed = !lines_first.is_empty() || !nested_first.is_empty();
    item_break(cell, lines_first, nested_first, lines, nested_rest, placed)
}

/// The first line or block child of an item that nothing of fits, placed anyway for forward progress
fn force_item_progress(cell: &FlexCell) -> Option<ItemBreak> {
    if let Some((first_line, other_lines)) = cell.lines.split_first() {
        return Some(item_break(
            cell,
            vec![first_line.clone()],
            Vec::new(),
            other_lines.to_vec(),
            cell.nested_elements.clone(),
            true,
        ));
    }
    let (first_child, other_children) = cell.nested_elements.split_first()?;
    Some(item_break(
        cell,
        Vec::new(),
        vec![first_child.clone()],
        Vec::new(),
        other_children.to_vec(),
        true,
    ))
}

fn item_break(
    cell: &FlexCell,
    lines_first: Vec<TextLine>,
    nested_first: Vec<LayoutNode>,
    lines_rest: Vec<TextLine>,
    nested_rest: Vec<LayoutNode>,
    placed: bool,
) -> ItemBreak {
    let mut first = cell.clone();
    first.lines = lines_first;
    first.nested_elements = nested_first;
    first.padding.bottom = 0.0;
    first.border.bottom.width = 0.0;
    first.natural_height = cell.border.top.width + cell.padding.top + item_content_height(&first);

    let mut rest = cell.clone();
    rest.lines = lines_rest;
    rest.nested_elements = nested_rest;
    rest.y_offset = 0.0;
    rest.padding.top = 0.0;
    rest.border.top.width = 0.0;
    rest.natural_height = item_content_height(&rest) + end_decoration(cell);
    ItemBreak {
        first,
        rest: Some(rest),
        placed,
    }
}

/// The lines at the start of `lines` that fit in `space`, taken out of it
fn take_fitting_lines(lines: &mut Vec<TextLine>, space: f32) -> Vec<TextLine> {
    let mut used = 0.0;
    let fitting = lines
        .iter()
        .take_while(|line| {
            used += line.height;
            !exceeds_with_roundoff(used, space)
        })
        .count();
    lines.drain(..fitting).collect()
}

/// An item's block children split at `space`, as a container of them would split
fn split_nested(
    children: &[LayoutNode],
    space: f32,
    rule: FragmentBreakRule,
) -> (Vec<LayoutNode>, Vec<LayoutNode>) {
    if children.is_empty() {
        return (Vec::new(), Vec::new());
    }
    if !exceeds_with_roundoff(simulate_block_flow(children).height, space) {
        return (children.to_vec(), Vec::new());
    }
    let flow = Container {
        children: children.to_vec(),
        ..Default::default()
    };
    match split_container_node(&flow, space, rule) {
        Some((first, rest)) if !first.children.is_empty() => (first.children, rest.children),
        _ => (Vec::new(), children.to_vec()),
    }
}

fn item_content_height(cell: &FlexCell) -> f32 {
    let lines: f32 = cell.lines.iter().map(|line| line.height).sum();
    lines + simulate_block_flow(&cell.nested_elements).height
}

fn end_decoration(cell: &FlexCell) -> f32 {
    cell.padding.bottom + cell.border.bottom.width
}
