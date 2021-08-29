use image::error::{DecodingError, ImageFormatHint, ImageResult};
use image::{Bgra, DynamicImage, ImageError, ImageOutputFormat, Pixel};
use webp::Decoder;

fn bgra_to_bgr(pixel: &mut Bgra<u8>) {
    let alpha = pixel.0[3] as u32;
    *pixel = pixel.map(|x| ((255 - alpha) + alpha * x as u32 / 255) as u8);
}

pub fn webp_to_jpg(data: &[u8]) -> ImageResult<Vec<u8>> {
    let decoder = Decoder::new(data);
    let webp = decoder.decode().map_or(
        Err(ImageError::Decoding(DecodingError::from_format_hint(
            ImageFormatHint::Name("webp".to_string()),
        ))),
        |x| Ok(x),
    )?;
    let image = webp.to_image();

    let mut bgra = image.into_bgra8();
    bgra.pixels_mut().for_each(bgra_to_bgr);
    let image = DynamicImage::ImageBgra8(bgra);

    let mut buf = Vec::new();
    image.write_to(&mut buf, ImageOutputFormat::Jpeg(100))?;
    Ok(buf)
}
