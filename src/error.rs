use std::any::Any;

use teloxide::{ApiError, RequestError};

pub type Error = Box<dyn std::error::Error + Send + Sync>;

pub trait Message: Any {
    fn message(&self) -> Option<&str>;
}

impl Message for Error {
    fn message(&self) -> Option<&str> {
        self.downcast_ref::<RequestError>().and_then(|x| match x {
            RequestError::ApiError {
                kind: ApiError::Unknown(x),
                ..
            } => match x.as_str() {
                "Bad Request: not enough rights to change chat photo" => {
                    Some("请给与本 bot 管理员权限中的 \"修改群组信息/Change Group Info\"")
                }
                "Bad Request: PHOTO_CROP_SIZE_SMALL" => Some("头像分辨率太小"),
                _ => None,
            },
            _ => None,
        })
    }
}
