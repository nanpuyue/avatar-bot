use std::mem::swap;

use image::error::{DecodingError, ImageFormatHint, ImageResult};
use image::{guess_format, load_from_memory_with_format};
use image::{Bgra, DynamicImage, ImageError, ImageFormat, ImageOutputFormat};
use webp::Decoder;

fn bgra_to_bgr(pixel: &mut Bgra<u8>, background: [u8; 4]) {
    let alpha = pixel[3] as i32;
    for i in 0..3 {
        pixel[i] =
            ((255 - alpha) * background[3 - i] as i32 / 255 + alpha * pixel[i] as i32 / 255) as u8;
    }
    pixel[3] = 255;
}

fn load_webp(data: &[u8]) -> ImageResult<DynamicImage> {
    let decoder = Decoder::new(data);
    let webp = decoder.decode().ok_or_else(|| {
        ImageError::Decoding(DecodingError::from_format_hint(ImageFormatHint::Name(
            "webp".to_string(),
        )))
    })?;
    Ok(webp.to_image())
}

fn set_background(image: DynamicImage, background: [u8; 4]) -> DynamicImage {
    let mut bgra = image.into_bgra8();
    bgra.pixels_mut().for_each(|x| bgra_to_bgr(x, background));
    DynamicImage::ImageBgra8(bgra)
}

fn image_to_png(image: DynamicImage) -> ImageResult<Vec<u8>> {
    let mut buf = Vec::new();
    image.write_to(&mut buf, ImageOutputFormat::Png)?;
    Ok(buf)
}

pub fn str_to_color(str: &str) -> [u8; 4] {
    u32::from_str_radix(str.trim().trim_start_matches('#'), 16)
        .unwrap_or(0xffffff)
        .to_be_bytes()
}

pub fn img_to_png(data: &mut Vec<u8>, background: [u8; 4]) -> ImageResult<()> {
    let format = guess_format(data)?;
    let mut image = if format == ImageFormat::WebP {
        load_webp(data)
    } else {
        load_from_memory_with_format(data, format)
    }?;

    if format != ImageFormat::Png || image.color().has_alpha() {
        image = set_background(image, background);
        swap(data, &mut image_to_png(image)?);
    }
    Ok(())
}
