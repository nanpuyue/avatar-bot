use image::error::{DecodingError, ImageFormatHint, ImageResult};
use image::{guess_format, load_from_memory_with_format};
use image::{DynamicImage, ImageError, ImageFormat, ImageOutputFormat};
use webp::Decoder;

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
        let b = u32::from_str_radix(background.trim().trim_start_matches('#'), 16)
            .unwrap_or(0xffffff)
            .to_le_bytes();
        let b = [b[0] as i32, b[1] as _, b[2] as _];

        let mut bgra = image.into_bgra8();
        bgra.pixels_mut().filter(|x| x[3] != 255).for_each(|x| {
            for i in 0..3 {
                x[i] = (b[i] + (x[i] as i32 - b[i]) * x[3] as i32 / 255) as u8;
            }
            x[3] = 255;
        });
        image = DynamicImage::ImageBgra8(bgra);
    } else if format == ImageFormat::Png {
        return Ok(());
    }

    data.clear();
    image.write_to(data, ImageOutputFormat::Png)
}
