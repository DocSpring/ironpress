//! A flex item's own text, wrapped at the item's resolved main size.
//!
//! An item that holds text directly (no block children) is measured before its flexible length is resolved, at its
//! flex base width: zero for `flex: 1`, which wraps every word onto a line of its own. Its lines and height are
//! measured again at the resolved width, before the flex lines take their cross sizes from their items' heights
//! (css-flexbox-1 §9.7 comes before §9.4), and again where the items are positioned.

use super::{
    AncestorInfo, ComputedStyle, ElementNode, ElementSiblingPosition, FlexItem,
    GeneratedContentStyles, InlineContentSequence, InlineRunCollector, LayoutContext, LayoutEnv,
    TextWrapOptions, compute_style_with_context_with_font_metrics, parent_line_strut,
    resolve_padding_box_height, text_run_line_height_factor, update_text_block_layout,
    used_font_size, wrap_text_runs,
};

/// Wrap `item`'s text at `final_w`, its resolved border-box main size, and give it the height of those lines.
#[allow(clippy::too_many_arguments)]
pub(super) fn relayout_direct_text_item(
    item: &mut FlexItem,
    child_el: &ElementNode,
    position: &ElementSiblingPosition,
    ancestors: &[AncestorInfo],
    parent_for_children: &ComputedStyle,
    final_w: f32,
    layout: &LayoutContext,
    env: &mut LayoutEnv,
) {
    let relayout_classes = child_el.class_list();
    let relayout_selector_ctx = position
        .as_context()
        .selector_context(ancestors, child_el.children.is_empty());
    let relayout_child_style = compute_style_with_context_with_font_metrics(
        child_el.tag,
        child_el.style_attr(),
        parent_for_children,
        env.rules,
        child_el.tag_name(),
        &relayout_classes,
        child_el.id(),
        &child_el.attributes,
        &relayout_selector_ctx,
        env.font_metrics(),
    );
    let relayout_generated_styles = GeneratedContentStyles::resolve(
        child_el,
        &relayout_child_style,
        env.rules,
        &relayout_selector_ctx,
        env.fonts,
    );
    let content_w = (final_w
        - relayout_child_style.padding.horizontal()
        - relayout_child_style.border.horizontal_width())
    .max(0.0);
    // The item's lines, where atomic inline boxes are laid out
    let line_layout = layout.with_parent(
        content_w,
        layout.parent.content_height,
        relayout_child_style.font_size,
    );
    let mut runs = Vec::new();
    let mut relayout_ancestors = ancestors.to_vec();
    relayout_ancestors.push(position.ancestor(child_el, child_el.children.is_empty()));
    let counter_scope = env.counter_state.enter_element(&relayout_child_style);
    InlineRunCollector::for_layout(env, &line_layout).collect(
        InlineContentSequence::with_generated(
            &child_el.children,
            relayout_generated_styles.boxes(child_el),
        ),
        &relayout_child_style,
        &mut runs,
        None,
        &relayout_ancestors,
    );
    env.counter_state.leave_element(counter_scope);
    let lines = if runs.is_empty() {
        Vec::new()
    } else {
        wrap_text_runs(
            runs,
            TextWrapOptions::new(
                content_w,
                used_font_size(&relayout_child_style, env.fonts),
                text_run_line_height_factor(&relayout_child_style, env.fonts),
                relayout_child_style.overflow_wrap,
            )
            .with_white_space(relayout_child_style.white_space)
            .with_parent_strut(parent_line_strut(&relayout_child_style, env.fonts))
            .with_rtl(relayout_child_style.direction_rtl)
            .with_bidi_override(relayout_child_style.unicode_bidi.overrides()),
            env.fonts,
        )
    };
    let text_h: f32 = lines.iter().map(|l| l.height).sum();
    let mut border_box_h = resolve_padding_box_height(
        text_h,
        relayout_child_style.height,
        relayout_child_style.padding,
        relayout_child_style.border.widths(),
        relayout_child_style.box_sizing,
    ) + relayout_child_style.border.vertical_width();
    if relayout_child_style.height.is_none() {
        if let Some(ratio) = relayout_child_style.aspect_ratio {
            if ratio > 0.0 {
                border_box_h = border_box_h.max(final_w / ratio);
            }
        }
    }
    if let Some(element) = item.elements.first_mut() {
        let height = (relayout_child_style.height.is_some()
            || relayout_child_style.aspect_ratio.is_some())
        .then(|| (border_box_h - relayout_child_style.border.vertical_width()).max(0.0));
        let clip_height = relayout_child_style
            .overflow
            .clips()
            .then_some(border_box_h);
        update_text_block_layout(element.as_mut(), Some(lines), final_w, height, clip_height);
    }
    item.height = border_box_h + relayout_child_style.margin.vertical();
    item.natural_height = item.height;
}
