use image::error::{DecodingError, ImageFormatHint, ImageResult};
use image::{guess_format, load_from_memory_with_format};
use image::{Bgra, DynamicImage, ImageBuffer, ImageError, ImageFormat, ImageOutputFormat};
use webp::Decoder;

fn alpha_composit(pixel: &mut Bgra<u8>, color: [i32; 3]) {
    for i in 0..3 {
        pixel[i] = (color[i] + (pixel[i] as i32 - color[i]) * pixel[3] as i32 / 255) as u8;
    }
    pixel[3] = 255;
}

fn trans_flag(img: &mut ImageBuffer<Bgra<u8>, Vec<u8>>) {
    const COLOR: [[i32; 3]; 5] = [
        [0xfa, 0xce, 0x5b],
        [0xb8, 0xa9, 0xf5],
        [0xff, 0xff, 0xff],
        [0xb8, 0xa9, 0xf5],
        [0xfa, 0xce, 0x5b],
    ];

    let height = img.height();

    let mut color_index = 0;
    let mut passed = 0;
    img.enumerate_rows_mut().for_each(|(row_index, row)| {
        if color_index < 4 && (row_index - passed) * (5 - color_index) > height - passed {
            color_index += 1;
            passed = row_index;
        }
        let b = COLOR[color_index as usize];
        row.filter(|(_, _, x)| x[3] != 255)
            .for_each(|(_, _, x)| alpha_composit(x, b))
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
        let mut bgra = image.into_bgra8();

        if background == "trans" {
            trans_flag(&mut bgra);
        } else {
            let b = u32::from_str_radix(background.trim().trim_start_matches('#'), 16)
                .unwrap_or(0xffffff)
                .to_le_bytes();
            let b = [b[0] as i32, b[1] as _, b[2] as _];

            bgra.pixels_mut()
                .filter(|x| x[3] != 255)
                .for_each(|x| alpha_composit(x, b));
        }

        image = DynamicImage::ImageBgra8(bgra);
    } else if format == ImageFormat::Png {
        return Ok(());
    }

    data.clear();
    image.write_to(data, ImageOutputFormat::Png)
}
