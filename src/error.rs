use std::any::Any;

use grammers_client::client::bots::InvocationError;

pub type Error = Box<dyn std::error::Error + Send + Sync>;

pub trait Message: Any {
    fn message(&self) -> Option<&str>;
}

impl Message for Error {
    fn message(&self) -> Option<&str> {
        self.downcast_ref::<InvocationError>()
            .and_then(|x| match x {
                InvocationError::Rpc(x) => match x.name.as_str() {
                    "PHOTO_CROP_SIZE_SMALL" => Some("头像分辨率太小"),
                    "CHAT_ADMIN_REQUIRED" => Some(
                        "权限不足, 请给与本 bot 管理员权限中的 \"修改群组信息/Change Group Info\"",
                    ),
                    "IMAGE_PROCESS_FAILED" => Some("Telegram 处理图片出错"),
                    x => Some(x),
                },
                _ => None,
            })
    }
}
