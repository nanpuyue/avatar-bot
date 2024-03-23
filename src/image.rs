use std::io::Cursor;
use std::io::Write;

use flate2::write::GzDecoder;
use image::load_from_memory;
use image::math::Rect;
use image::GenericImage;
use image::{DynamicImage, ImageFormat::Png, Rgba, RgbaImage};
use imageproc::drawing::draw_hollow_rect_mut;
use imageproc::rect;
use rlottie::{Animation, Surface};

use crate::error::Error;
use crate::opencv::detect_animeface;

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
        "c" | "center" => {
            let y = (height - width) / 2;
            img.sub_image(0, y, width, width)
        }
        _ => return None,
    };

    Some(subimage.to_image())
}

fn draw_thickness_rect(img: &mut RgbaImage, rect: &Rect, color: Rgba<u8>, thickness: u32) {
    for i in 0..thickness {
        draw_hollow_rect_mut(
            img,
            rect::Rect::at((rect.x + i) as _, (rect.y + i) as _)
                .of_size(rect.width - i * 2, rect.height - i * 2),
            color,
        );
    }
}

fn face_image(img: &mut RgbaImage, rect: Rect) -> RgbaImage {
    assert_eq!(rect.width, rect.height);

    let mut offset = rect.width / 2;
    let mut update_offset = |x: u32| {
        if x < offset {
            offset = x;
        }
    };
    update_offset((img.width() - rect.width) / 2);
    update_offset((img.height() - rect.height) / 2);

    let width = rect.width + offset * 2;
    let x = match rect.x.checked_sub(offset) {
        None => 0,
        Some(x) if x + width > img.width() => img.width() - width,
        Some(x) => x,
    };
    let y = match rect.y.checked_sub(offset + rect.width * 3 / 20) {
        None => 0,
        Some(y) if y + width > img.height() => img.height() - width,
        Some(y) => y,
    };

    img.sub_image(x, y, width, width).to_image()
}

pub fn image_to_png(
    data: &mut Vec<u8>,
    background: &str,
    align: Option<&str>,
    show_detect: bool,
) -> Result<(), Error> {
    let image = load_from_memory(data)?;

    let mut rgba = image.into_rgba8();
    if let Some(align) = align {
        if let Some(x) = square_image(&mut rgba, align) {
            rgba = x;
        }
    } else {
        let mut select = None;
        let detect = detect_animeface(data)?;
        for i in 0..detect.len() {
            match select {
                None => select = Some(i),
                Some(x) if detect[i].width > detect[x].width => select = Some(i),
                _ => {}
            }
        }

        if show_detect {
            for (i, rect) in detect.iter().enumerate() {
                assert!(select.is_some());
                let select = select.unwrap();
                let color = if i == select {
                    Rgba([0xff, 0, 0, 0xff])
                } else {
                    Rgba([0, 0, 0, 0xff])
                };
                draw_thickness_rect(&mut rgba, rect, color, rect.width / 64);
            }
        } else if let Some(x) = select {
            rgba = face_image(&mut rgba, detect[x]);
        }
    }

    if !show_detect {
        match background {
            "tr" | "trans" => trans_flag(&mut rgba),
            _ => {
                let [_, b @ ..] =
                    u32::from_str_radix(background.trim().trim_start_matches('#'), 16)
                        .unwrap_or(0xffffff)
                        .to_be_bytes()
                        .map(|x| x as _);

                rgba.pixels_mut()
                    .filter(|x| x[3] != 255)
                    .for_each(|x| alpha_composit(x, b));
            }
        }
    }

    DynamicImage::ImageRgba8(rgba).write_to(&mut Cursor::new(data), Png)?;
    Ok(())
}

pub fn tgs_to_png(data: Vec<u8>, cache_key: &str) -> Result<Vec<u8>, Error> {
    let mut json_data = Vec::new();
    GzDecoder::new(&mut json_data).write_all(&data)?;
    let mut animation =
        Animation::from_data(json_data, cache_key, "/nonexistent").ok_or("Invalid lottie data")?;
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
