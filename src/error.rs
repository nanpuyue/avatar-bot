use std::error;
use std::fmt::{self, Debug, Display, Formatter};

use grammers_client::client::bots::InvocationError;

pub type Error = Box<dyn error::Error + Send + Sync>;

#[derive(Debug)]
pub struct ErrorMessage<T>(T);

impl<T: Display> Display for ErrorMessage<T> {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

impl<T: Display + Debug> error::Error for ErrorMessage<T> {}

pub trait IntoErrorMessage
where
    Self: Sized + Display + Debug + Send + Sync + 'static,
{
    fn error(self) -> Error {
        Box::new(ErrorMessage(self))
    }

    fn result<T>(self) -> Result<T, Error> {
        Err(self.error())
    }
}

impl IntoErrorMessage for String {}
impl IntoErrorMessage for &'static str {}

pub trait Message {
    fn message(&self) -> Option<&str>;
}

impl Message for Error {
    fn message(&self) -> Option<&str> {
        if let Some(InvocationError::Rpc(x)) = self.downcast_ref::<InvocationError>() {
            return match x.name.as_str() {
                "PHOTO_CROP_SIZE_SMALL" => Some("头像分辨率太小"),
                "CHAT_ADMIN_REQUIRED" => {
                    Some("权限不足, 请给与本 bot 管理员权限中的 \"修改群组信息/Change Group Info\"")
                }
                "IMAGE_PROCESS_FAILED" => Some("Telegram 处理图片出错"),
                x => Some(x),
            };
        };

        if let Some(x) = self.downcast_ref::<ErrorMessage<&str>>() {
            return Some(x.0);
        };
        if let Some(x) = self.downcast_ref::<ErrorMessage<String>>() {
            return Some(&x.0);
        };

        None
    }
}
