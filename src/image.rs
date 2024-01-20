use std::io::Cursor;
use std::io::Write;

use flate2::write::GzDecoder;
use image::error::ImageResult;
use image::load_from_memory;
use image::GenericImage;
use image::{DynamicImage, ImageOutputFormat::Png, Rgba, RgbaImage};
use rlottie::{Animation, Surface};

use crate::error::Error;

fn alpha_composit(pixel: &mut Rgba<u8>, color: [i32; 3]) {
    for i in 0..3 {
        pixel[i] = (color[i] + (pixel[i] as i32 - color[i]) * pixel[3] as i32 / 255) as u8;
    }
    pixel[3] = 255;
}

fn trans_flag(img: &mut RgbaImage) {
    const COLOR: [[i32; 3]; 5] = [
        [0x5b, 0xce, 0xfa],
        [0xf5, 0xa9, 0xb8],
        [0xff, 0xff, 0xff],
        [0xf5, 0xa9, 0xb8],
        [0x5b, 0xce, 0xfa],
    ];

    let mut height = img.height();
    let width = img.width();

    let mut color_index = 0;
    let mut passed = height.saturating_sub(width) / 2;
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

fn square_image(img: &mut RgbaImage, align: &str) -> Option<RgbaImage> {
    let height = img.height();
    let width = img.width();
    if width >= height {
        return None;
    }

    let subimage = match align {
        "t" | "top" => img.sub_image(0, 0, width, width),
        "b" | "bottom" => img.sub_image(0, height - width, width, width),
        _ => return None,
    };

    Some(subimage.to_image())
}

pub fn image_to_png(data: &mut Vec<u8>, background: &str, align: Option<&str>) -> ImageResult<()> {
    let image = load_from_memory(data)?;

    let mut rgba = image.into_rgba8();
    if let Some(align) = align {
        if let Some(x) = square_image(&mut rgba, align) {
            rgba = x;
        }
    }

    match background {
        "tr" | "trans" => trans_flag(&mut rgba),
        _ => {
            let [_, b @ ..] = u32::from_str_radix(background.trim().trim_start_matches('#'), 16)
                .unwrap_or(0xffffff)
                .to_be_bytes()
                .map(|x| x as _);

            rgba.pixels_mut()
                .filter(|x| x[3] != 255)
                .for_each(|x| alpha_composit(x, b));
        }
    }

    DynamicImage::ImageRgba8(rgba).write_to(&mut Cursor::new(data), Png)
}

pub fn tgs_to_png(data: Vec<u8>) -> Result<Vec<u8>, Error> {
    let mut json_data = Vec::new();
    GzDecoder::new(&mut json_data).write_all(&data)?;
    let mut animation =
        Animation::from_data(json_data, Vec::new(), "/nonexistent").ok_or("Invalid lottie data")?;
    let mut surface = Surface::new(animation.size());
    animation.render(0, &mut surface);

    let mut rgba = RgbaImage::new(surface.width() as _, surface.height() as _);
    for (x, y) in rgba.pixels_mut().zip(surface.data()) {
        (x[0], x[1], x[2], x[3]) = (y.r, y.g, y.b, y.a);
    }

    let mut png_data = Vec::new();
    DynamicImage::ImageRgba8(rgba).write_to(&mut Cursor::new(&mut png_data), Png)?;
    Ok(png_data)
}
