use std::io::Cursor;

use image::error::{DecodingError, ImageFormatHint, ImageResult};
use image::{guess_format, load_from_memory_with_format};
use image::{DynamicImage, ImageBuffer, ImageError, ImageFormat, ImageOutputFormat, Rgba};
use webp::Decoder;

fn alpha_composit(pixel: &mut Rgba<u8>, color: [i32; 3]) {
    for i in 0..3 {
        pixel[i] = (color[i] + (pixel[i] as i32 - color[i]) * pixel[3] as i32 / 255) as u8;
    }
    pixel[3] = 255;
}

fn trans_flag(img: &mut ImageBuffer<Rgba<u8>, Vec<u8>>) {
    const COLOR: [[i32; 3]; 5] = [
        [0x5b, 0xce, 0xfa],
        [0xf5, 0xa9, 0xb8],
        [0xff, 0xff, 0xff],
        [0xf5, 0xa9, 0xb8],
        [0x5b, 0xce, 0xfa],
    ];

    let mut height = img.height();
    let weight = img.width();

    let mut color_index = 0;
    let mut passed = height.saturating_sub(weight) / 2;
    height -= passed;
    img.enumerate_rows_mut().for_each(|(row_index, row)| {
        if row_index >= passed && row_index <= height {
            if color_index < 4 && (row_index - passed) * (5 - color_index) > height - passed {
                color_index += 1;
                passed = row_index;
            }
            let b = COLOR[color_index as usize];
            row.filter(|(_, _, x)| x[3] != 255)
                .for_each(|(_, _, x)| alpha_composit(x, b))
        }
    });
}

pub fn image_to_png(data: &mut Vec<u8>, background: &str) -> ImageResult<()> {
    let format = guess_format(data)?;
    let mut image = if format == ImageFormat::WebP {
        let webp = Decoder::new(data).decode().ok_or_else(|| {
            ImageError::Decoding(DecodingError::from_format_hint(ImageFormatHint::Exact(
                ImageFormat::WebP,
            )))
        })?;
        webp.to_image()
    } else {
        load_from_memory_with_format(data, format)?
    };

    if image.color().has_alpha() {
        let mut rgba = image.into_rgba8();

        if background == "trans" {
            trans_flag(&mut rgba);
        } else {
            let [_, b @ ..] = u32::from_str_radix(background.trim().trim_start_matches('#'), 16)
                .unwrap_or(0xffffff)
                .to_be_bytes()
                .map(|x| x as _);

            rgba.pixels_mut()
                .filter(|x| x[3] != 255)
                .for_each(|x| alpha_composit(x, b));
        }

        image = DynamicImage::ImageRgba8(rgba);
    } else if format == ImageFormat::Png {
        return Ok(());
    }

    image.write_to(&mut Cursor::new(data), ImageOutputFormat::Png)
}
