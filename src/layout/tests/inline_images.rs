//! An `<img>` or `<svg>` inside an inline element (`<a>`, `<span>`) is a replaced box in its line, sized against the
//! block its line is in and fitted as `object-fit` says, as one directly in the block is (CSS 2.1 §10.3.2, CSS Images
//! 3 §5.5).

use crate::layout::elements::{LayoutVisitor, TextBlock};
use crate::layout::engine::visit_layout_tree;

use super::support::layout_pages;

/// The (width, height) of each replaced box drawn in a line.
fn inline_replaced_sizes(markup: &str) -> Vec<(f32, f32)> {
    #[derive(Default)]
    struct Sizes(Vec<(f32, f32)>);
    impl Sizes {
        // An inline-block's own lines hold what's inside it
        fn visit_lines(&mut self, lines: &[crate::layout::engine::TextLine]) {
            for run in lines.iter().flat_map(|line| &line.runs) {
                let Some(inline) = run.inline_box.as_deref() else {
                    continue;
                };
                // A replaced box is drawn from its image or SVG, or from the replaced node it was laid out as
                let replaced_node = matches!(inline.content.as_slice(), [node] if node.replaced_element().is_some());
                if inline.image.is_some() || inline.svg.is_some() || replaced_node {
                    self.0.push((inline.width, inline.height));
                }
                self.visit_lines(&inline.lines);
            }
        }
    }
    impl LayoutVisitor for Sizes {
        fn visit_text_block(&mut self, block: &TextBlock) {
            self.visit_lines(&block.lines);
        }
    }
    let mut sizes = Sizes::default();
    for page in layout_pages(markup) {
        for (_, element) in &page.elements {
            visit_layout_tree(element.as_ref(), &mut sizes);
        }
    }
    sizes.0
}

/// A PNG `width` x `height` px with four stripes across it, red, green, blue and yellow.
fn striped_png(width: u32, height: u32) -> String {
    let stripes = [[255, 0, 0], [0, 255, 0], [0, 0, 255], [255, 255, 0]];
    let image = image::RgbImage::from_fn(width, height, |x, _| {
        image::Rgb(stripes[(x * 4 / width) as usize])
    });
    let mut png = Vec::new();
    image::DynamicImage::ImageRgb8(image)
        .write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png)
        .expect("a PNG");
    format!(
        "data:image/png;base64,{}",
        crate::layout::images::base64_encode(&png)
    )
}

fn pdf_content(markup: &str) -> String {
    let pdf = crate::HtmlConverter::new()
        .compress(false)
        .convert(markup)
        .expect("the document converts");
    String::from_utf8_lossy(&pdf).into_owned()
}

#[test]
fn a_wide_image_in_a_link_is_no_wider_than_its_block() {
    // A 2000px logo in a 150pt column scales down to the column, keeping its
    // ratio, as it does outside the link
    let logo = striped_png(2000, 100);
    let inside = inline_replaced_sizes(&format!(
        r#"<div style="width:150pt"><a href="https://example.com"><img src="{logo}" style="max-width:100%"></a></div>"#
    ));
    assert_eq!(inside, vec![(150.0, 7.5)]);
}

#[test]
fn a_percentage_svg_in_a_span_takes_a_share_of_its_block() {
    let sizes = inline_replaced_sizes(
        r#"<div style="width:200pt"><span><svg width="50%" height="20" viewBox="0 0 10 10"><rect width="10" height="10"/></svg></span></div>"#,
    );
    assert_eq!(sizes, vec![(100.0, 15.0)]);
}

#[test]
fn a_percentage_svg_in_a_link_writes_only_finite_numbers() {
    // Sized against an unknown width, a percentage became infinite: the PDF
    // got `inf` and `NaN` operands and viewers stopped drawing the page there
    let content = pdf_content(
        r#"<p><a href="https://example.com"><svg width="100%" viewBox="0 0 10 10"><rect width="10" height="10"/></svg></a> after</p>"#,
    );
    for word in ["inf", "NaN"] {
        assert!(
            !content
                .split_whitespace()
                .any(|token| token.eq_ignore_ascii_case(word)),
            "{word} in the content"
        );
    }
}

#[test]
fn an_image_in_a_span_is_fitted_as_object_fit_says() {
    // A 40x20px image in a 30pt (40px) square: `contain` draws it 30pt wide
    // and 15pt tall, centred; `cover` 60pt wide, clipped to the square
    let image = striped_png(40, 20);
    let draw = |fit: &str| {
        let content = pdf_content(&format!(
            r#"<p>x <span><img src="{image}" style="width:30pt;height:30pt;object-fit:{fit}"></span> y</p>"#
        ));
        let matrix = content
            .lines()
            .take_while(|line| !line.ends_with(" Do"))
            .filter(|line| line.ends_with(" cm"))
            .last()
            .map(|line| {
                line.split_whitespace()
                    .take(6)
                    .map(|number| number.parse::<f32>().expect("a number"))
                    .collect::<Vec<_>>()
            })
            .expect("the image's matrix");
        (matrix[0], matrix[3], content.matches("W n\n").count())
    };
    // Only `cover` overflows the square, so only it adds a clip
    let (fill_width, fill_height, page_clips) = draw("fill");
    assert_eq!((fill_width, fill_height), (30.0, 30.0));
    assert_eq!(draw("contain"), (30.0, 15.0, page_clips));
    assert_eq!(draw("cover"), (60.0, 30.0, page_clips + 1));
}

#[test]
fn an_image_in_an_inline_block_span_is_drawn() {
    let image = striped_png(40, 20);
    let sizes = inline_replaced_sizes(&format!(
        r#"<p>x <span style="display:inline-block"><img src="{image}" style="width:30pt;height:30pt;object-fit:contain"></span> y</p>"#
    ));
    assert_eq!(sizes, vec![(30.0, 30.0)]);
}
