use std::io::Write;

use image::error::{DecodingError, ImageFormatHint, ImageResult};
use image::{Bgra, DynamicImage, ImageError, ImageOutputFormat};
use opencv::core::{Mat, Vector};
use opencv::imgcodecs::imencode;
use opencv::videoio::{VideoCapture, VideoCaptureTrait, CAP_ANY};
use tempfile::NamedTempFile;
use webp::Decoder;

use crate::Error;

fn bgra_to_bgr(pixel: &mut Bgra<u8>, background: &str) {
    let background = u32::from_str_radix(background.trim_start_matches('#'), 16)
        .unwrap_or(0xffffff)
        .to_be_bytes();

    let alpha = pixel[3] as i32;
    for i in 0..3 {
        pixel[i] =
            ((255 - alpha) * background[3 - i] as i32 / 255 + alpha * pixel[i] as i32 / 255) as u8;
    }
    pixel[3] = 255;
}

pub fn webp_to_png(data: &[u8], background: &str) -> ImageResult<Vec<u8>> {
    let decoder = Decoder::new(data);
    let webp = decoder.decode().ok_or_else(|| {
        ImageError::Decoding(DecodingError::from_format_hint(ImageFormatHint::Name(
            "webp".to_string(),
        )))
    })?;
    let image = webp.to_image();

    let mut bgra = image.into_bgra8();
    bgra.pixels_mut().for_each(|x| bgra_to_bgr(x, background));
    let image = DynamicImage::ImageBgra8(bgra);

    let mut buf = Vec::new();
    image.write_to(&mut buf, ImageOutputFormat::Png)?;
    Ok(buf)
}

pub fn mp4_to_png(data: &[u8]) -> Result<Vec<u8>, Error> {
    let mut temp = NamedTempFile::new()?;
    temp.write_all(data)?;
    let mut video_capture = VideoCapture::from_file(temp.as_ref().to_str().unwrap(), CAP_ANY)?;

    let mut frame = Mat::default();
    video_capture.read(&mut frame)?;

    let mut buf = Vector::new();
    imencode(".png", &frame, &mut buf, &Vector::new())?;

    Ok(buf.into())
}
