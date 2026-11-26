//! A flex line taller than the space left on its page breaks inside its items: each item's content breaks at the
//! page end, between its lines or block children, and its continuation starts the next page at the same inline
//! position (CSS Fragmentation 3 §5.2, CSS Flexbox 1 §12.1 "breaks inside flex items", as Chrome prints a row of
//! columns). An item whose content is all placed ends on this page; the others continue with their bottom padding and
//! border. Lines wrapped after it continue below what's left of it.
//!
//! Each item keeps the place and size the unfragmented row gave it (its margins, `align-items` or `align-self`
//! offset, and its `min-height` or stretched size), and each fragment draws the part of it on its page: re-aligning a
//! fragment in its shorter line, or clamping it to the item's whole minimum size again, would move its content off
//! the page.

use super::{FragmentBreakRule, LayoutNode, simulate_block_flow, split_container_node};
use crate::layout::elements::Container;
use crate::layout::engine::{FlexCell, FlexLineId, FlexRow, TextLine};
use crate::layout::roundoff::exceeds_with_roundoff;
use crate::style::computed::AlignSelf;

/// Where an item sits in the unfragmented row: the top of its border box below the row's content-box top, and its
/// used height
#[derive(Clone, Copy)]
struct ItemPlace {
    top: f32,
    size: f32,
}

/// What of one item goes on this page, and what continues on the next
struct ItemBreak {
    first: Option<FlexCell>,
    rest: Option<FlexCell>,
    /// Whether any of the item's content (or the whole item) was placed above the page end
    placed: bool,
}

/// Split a flex row whose first line, `line`, is taller than `avail_inner`, the space below the row's content-box top
pub(super) fn split_flex_line(
    element: &FlexRow,
    line: FlexLineId,
    avail_inner: f32,
    rule: FragmentBreakRule,
) -> Option<(LayoutNode, LayoutNode)> {
    let row_height = element.content.row_height;
    let places: Vec<ItemPlace> = element
        .content
        .cells
        .iter()
        .map(|cell| {
            let geometry = cell.cross_geometry(row_height, element.content.alignment, 0.0);
            ItemPlace {
                top: geometry.offset,
                size: geometry.size,
            }
        })
        .collect();
    let in_line = |index: usize| element.content.cells[index].line_id == line;

    let mut breaks: Vec<Option<ItemBreak>> = element
        .content
        .cells
        .iter()
        .zip(&places)
        .enumerate()
        .map(|(index, (cell, &place))| {
            in_line(index).then(|| break_item(cell, place, avail_inner, rule))
        })
        .collect();
    if breaks.iter().flatten().all(|item| item.rest.is_none()) {
        return None;
    }
    if !breaks.iter().flatten().any(|item| item.placed) {
        // Nothing fits above the page end: the row moves to the next page, unless it already starts one, where the
        // emergency rule places the first line or child of its first unfinished item to make progress
        if rule != FragmentBreakRule::Emergency {
            return None;
        }
        let index = (0..breaks.len()).find(|&index| {
            breaks[index]
                .as_ref()
                .is_some_and(|item| item.rest.is_some() && places[index].top < avail_inner)
        })?;
        breaks[index] = Some(force_item_progress(
            &element.content.cells[index],
            places[index],
            avail_inner,
        )?);
    }

    // What's left of the line on the next page, and how far the lines wrapped after it move up: as far as the page
    // took, less any growth of the line's remainder
    let line_bottom = (0..places.len())
        .filter(|&index| in_line(index))
        .map(|index| places[index].top + places[index].size)
        .fold(0.0_f32, f32::max);
    let line_rest = breaks
        .iter()
        .flatten()
        .filter_map(|item| item.rest.as_ref())
        .map(|cell| cell.y_offset + cell.natural_height)
        .fold(0.0_f32, f32::max);
    let later_lines_shift = avail_inner - (line_rest - (line_bottom - avail_inner)).max(0.0);

    let mut first_cells = Vec::new();
    let mut rest_cells = Vec::new();
    for (cell, item) in element.content.cells.iter().zip(breaks) {
        match item {
            Some(item) => {
                first_cells.extend(item.first);
                rest_cells.extend(item.rest);
            }
            None => {
                let mut later = cell.clone();
                later.y_offset = (later.y_offset - later_lines_shift).max(0.0);
                rest_cells.push(later);
            }
        }
    }

    let mut first = element.clone();
    first.content.cells = first_cells;
    first.content.row_height = avail_inner;
    first.content.forced_line_breaks.clear();
    first.box_model.margins.end = 0.0;
    first.box_model.padding.bottom = 0.0;
    first.box_model.border.bottom.width = 0.0;

    let mut rest = element.clone();
    let rest_height = rest_cells
        .iter()
        .map(|cell| cell.y_offset + cell.natural_height)
        .fold(0.0_f32, f32::max);
    rest.content.row_height = (row_height - later_lines_shift).max(rest_height);
    rest.content.forced_line_breaks.retain(|marker| {
        marker.before != line && rest_cells.iter().any(|cell| cell.line_id == marker.before)
    });
    rest.content.cells = rest_cells;
    rest.box_model.margins.start = 0.0;
    rest.box_model.padding.top = 0.0;
    rest.box_model.border.top.width = 0.0;

    Some((Box::new(first), Box::new(rest)))
}

/// Break one item of the line at `avail_inner` below the row's content-box top
fn break_item(
    cell: &FlexCell,
    place: ItemPlace,
    avail_inner: f32,
    rule: FragmentBreakRule,
) -> ItemBreak {
    if !exceeds_with_roundoff(avail_inner, place.top) {
        // It starts at or below the page end: all of it continues, as far below the next page's top
        return ItemBreak {
            first: None,
            rest: Some(placed_at(cell.clone(), place.top - avail_inner, place.size)),
            placed: false,
        };
    }
    if !exceeds_with_roundoff(place.top + place.size, avail_inner) {
        return ItemBreak {
            first: Some(placed_at(cell.clone(), place.top, place.size)),
            rest: None,
            placed: true,
        };
    }

    let space = avail_inner - content_top(cell, place);
    let mut lines = cell.lines.clone();
    let lines_first = take_fitting_lines(&mut lines, space);
    let lines_height: f32 = lines_first.iter().map(|line| line.height).sum();
    let (nested_first, nested_rest) = if lines.is_empty() {
        split_nested(&cell.nested_elements, space - lines_height, rule)
    } else {
        (Vec::new(), cell.nested_elements.clone())
    };
    let placed = !lines_first.is_empty() || !nested_first.is_empty();
    item_break(
        cell,
        place,
        avail_inner,
        (lines_first, nested_first),
        (lines, nested_rest),
        placed,
    )
}

/// The first line or block child of an item that nothing of fits, placed anyway for forward progress
fn force_item_progress(cell: &FlexCell, place: ItemPlace, avail_inner: f32) -> Option<ItemBreak> {
    if let Some((first_line, other_lines)) = cell.lines.split_first() {
        return Some(item_break(
            cell,
            place,
            avail_inner,
            (vec![first_line.clone()], Vec::new()),
            (other_lines.to_vec(), cell.nested_elements.clone()),
            true,
        ));
    }
    let (first_child, other_children) = cell.nested_elements.split_first()?;
    Some(item_break(
        cell,
        place,
        avail_inner,
        (Vec::new(), vec![first_child.clone()]),
        (Vec::new(), other_children.to_vec()),
        true,
    ))
}

/// An item's fragment on this page, down to the page end, and the rest of it from the next page's top
fn item_break(
    cell: &FlexCell,
    place: ItemPlace,
    avail_inner: f32,
    (lines_first, nested_first): (Vec<TextLine>, Vec<LayoutNode>),
    (lines_rest, nested_rest): (Vec<TextLine>, Vec<LayoutNode>),
    placed: bool,
) -> ItemBreak {
    let on_this_page = avail_inner - place.top;
    let mut first = cell.clone();
    first.lines = lines_first;
    first.nested_elements = nested_first;
    first.padding.bottom = 0.0;
    first.border.bottom.width = 0.0;
    let first_extent =
        on_this_page.max(content_top(cell, place) - place.top + item_content_height(&first));
    let first = placed_at(first, place.top, first_extent);

    let mut rest = cell.clone();
    rest.lines = lines_rest;
    rest.nested_elements = nested_rest;
    rest.padding.top = 0.0;
    rest.border.top.width = 0.0;
    let rest_extent =
        (place.size - on_this_page).max(item_content_height(&rest) + end_decoration(cell));
    ItemBreak {
        first: Some(first),
        rest: Some(placed_at(rest, 0.0, rest_extent)),
        placed,
    }
}

/// An item at `top` below its row's content-box top, `extent` tall, kept there whatever height its line or row has.
/// Its line's size goes with the unbroken line: the fragment is its own extent, which is what the next page's break
/// measures the line by
fn placed_at(mut cell: FlexCell, top: f32, extent: f32) -> FlexCell {
    cell.y_offset = top;
    cell.line_cross_size = 0.0;
    cell.align_self = AlignSelf::FlexStart;
    cell.cross_min = 0.0;
    cell.cross_max = f32::INFINITY;
    cell.natural_height = extent;
    cell.fragmentation.fragment_block_extent = Some(extent);
    cell
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

/// The top of an item's content, below its row's content-box top
fn content_top(cell: &FlexCell, place: ItemPlace) -> f32 {
    place.top + cell.border.top.width + cell.padding.top
}

fn item_content_height(cell: &FlexCell) -> f32 {
    let lines: f32 = cell.lines.iter().map(|line| line.height).sum();
    lines + simulate_block_flow(&cell.nested_elements).height
}

fn end_decoration(cell: &FlexCell) -> f32 {
    cell.padding.bottom + cell.border.bottom.width
}
