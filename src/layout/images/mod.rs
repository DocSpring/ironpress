mod loader;
mod placement;
mod raster;
mod source;
mod svg;

pub(crate) use loader::{
    ElementImage, decode_element_image, load_image_bytes, load_image_from_element, looks_like_svg,
    raster_image_size, svg_image_box, try_parse_svg_bytes,
};
pub(crate) use placement::{
    ImagePlacement, add_inline_replaced_baseline_gap, compute_image_placement,
    compute_replaced_content_placement, float_box_around_replaced, svg_intrinsic_size,
};
pub(crate) use raster::{
    RasterCrop, crop_raster_asset, decode_asset_to_rgba, png_bytes_for_decoding,
};
pub(crate) use source::build_raster_background_tree;
#[cfg(test)]
pub(crate) use source::{base64_encode, load_image_data};
pub(crate) use svg::{resolve_svg_image_size, svg_element_box, svg_element_node};

#[cfg(test)]
mod tests;
