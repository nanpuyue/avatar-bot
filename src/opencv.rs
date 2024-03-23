use std::cell::RefCell;
use std::sync::Once;

use image::math::Rect;
use opencv::core::{
    FileStorage, FileStorageTraitConst, FileStorage_MEMORY, FileStorage_READ, Size, Vector,
};
use opencv::imgcodecs::{imdecode, IMREAD_COLOR};
use opencv::objdetect::{CascadeClassifier, CascadeClassifierTrait};

use crate::error::Error;

pub fn detect_animeface(img: &[u8]) -> Result<Vec<Rect>, Error> {
    thread_local! {
        static INIT: Once = const { Once::new() };
        static CLASSIFIER: RefCell<CascadeClassifier> =
            RefCell::new(CascadeClassifier::default().unwrap());
    }
    INIT.with(|x| {
        x.call_once(|| {
            CLASSIFIER.with_borrow_mut(|x| {
                let model: FileStorage = FileStorage::new(
                    include_str!("../data/lbpcascade_animeface.xml"),
                    FileStorage_READ | FileStorage_MEMORY,
                    "UTF-8",
                )
                .unwrap();
                x.read(&model.get_first_top_level_node().unwrap()).unwrap();
            })
        })
    });

    let mut ret = Vector::new();
    CLASSIFIER.with_borrow_mut(|x| {
        x.detect_multi_scale(
            &imdecode(&Vector::from_slice(img), IMREAD_COLOR)?,
            &mut ret,
            1.1,
            5,
            0,
            Size::new(64, 64),
            Size::default(),
        )
    })?;

    let ret = ret
        .iter()
        .map(|x| Rect {
            x: x.x as _,
            y: x.y as _,
            width: x.width as _,
            height: x.height as _,
        })
        .collect();

    Ok(ret)
}
