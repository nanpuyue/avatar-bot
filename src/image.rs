use std::cmp::min;
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

use crate::command::Align;
use crate::command::Color;
use crate::command::Opt;
use crate::error::Error;
use crate::opencv::detect_animeface;

pub fn alpha_composite(pixel: &mut [u8; 4], color: [i32; 3]) {
    for i in 0..3 {
        if pixel[3] == 0 {
            pixel[i] = color[i] as _;
        } else {
            pixel[i] = (color[i] + (pixel[i] as i32 - color[i]) * pixel[3] as i32 / 255) as _;
        }
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
                .for_each(|(_, _, Rgba(x))| alpha_composite(x, b))
        }
    });
}

fn square_image(img: &mut RgbaImage, align: &Align) -> Option<RgbaImage> {
    let height = img.height();
    let width = img.width();
    if width >= height {
        return None;
    }

    let subimage = match align {
        Align::Top => img.sub_image(0, 0, width, width),
        Align::Bottom => img.sub_image(0, height - width, width, width),
        Align::Center => {
            let y = (height - width) / 2;
            img.sub_image(0, y, width, width)
        }
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

fn face_image_rect(img: &RgbaImage, face: &Rect) -> Rect {
    assert_eq!(face.width, face.height);

    let mut offset = face.width;
    offset = min(offset, img.width() - face.width);
    offset = min(offset, img.height() - face.height);
    let width = face.width + offset;

    offset /= 2;
    let x = match face.x.checked_sub(offset) {
        None => 0,
        Some(x) if x + width > img.width() => img.width() - width,
        Some(x) => x,
    };
    let y = match face.y.checked_sub(offset + face.width * 3 / 20) {
        None => 0,
        Some(y) if y + width > img.height() => img.height() - width,
        Some(y) => y,
    };

    Rect {
        x,
        y,
        width,
        height: width,
    }
}

pub fn image_to_png(data: &mut Vec<u8>, opt: &Opt) -> Result<(), Error> {
    let image = load_from_memory(data)?;

    let mut rgba = image.into_rgba8();
    if let Some(align) = &opt.align {
        if let Some(x) = square_image(&mut rgba, align) {
            rgba = x;
        }
    } else {
        let mut select = None;
        let detect = detect_animeface(data)?;
        for i in &detect {
            match select {
                None => select = Some(i),
                Some(x) if i.width > x.width => select = Some(i),
                _ => {}
            }
        }

        let select = select.map(|x| face_image_rect(&rgba, x));
        if opt.show_detect {
            for i in &detect {
                draw_thickness_rect(&mut rgba, i, Rgba([0, 0, 0, 0xff]), i.width / 64);
            }
            if let Some(x) = select {
                draw_thickness_rect(&mut rgba, &x, Rgba([0xff, 0, 0, 0xff]), x.width / 128 + 1);
            }
        } else if let Some(x) = select {
            rgba = rgba.sub_image(x.x, x.y, x.width, x.height).to_image();
        }
    }

    if !opt.show_detect {
        match opt.color {
            Color::Trans => trans_flag(&mut rgba),
            Color::Rgb(b) => {
                rgba.pixels_mut()
                    .filter(|x| x[3] != 255)
                    .for_each(|Rgba(x)| alpha_composite(x, b));
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
