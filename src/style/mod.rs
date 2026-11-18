pub mod computed;
pub mod defaults;
pub(crate) mod font_family;
pub(crate) mod font_metrics;
pub(crate) mod font_size;
pub(crate) mod html_cascade;
pub(crate) mod raster_quality;
pub mod resolve;

#[cfg(test)]
mod border_width_tests;
#[cfg(test)]
mod transform_tests;
