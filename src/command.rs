use std::collections::HashMap;
use std::env;
use std::str::FromStr;
use std::time::{Duration, Instant};

use lazy_static::lazy_static;
use teloxide::net::Download;
use teloxide::prelude::*;
use teloxide::types::{
    ForwardKind, ForwardOrigin, InputFile, MessageCommon, MessageEntity, MessageEntityKind,
    MessageKind,
};
use teloxide::utils::command::BotCommand;
use tokio::sync::Mutex;

use crate::error::{Error, Message as _};
use crate::ffmpeg::video_to_png;
use crate::image::image_to_png;
use crate::opengraph::link_to_img;

type Context = UpdateWithCx<AutoSend<Bot>, Message>;

const MIN_INTERVAL: Duration = Duration::from_secs(30);
const MAX_FILESIZE: u32 = 10 * 1024 * 1024;

lazy_static! {
    pub static ref LAST_UPDATE: Mutex<HashMap<i64, Mutex<Instant>>> = {
        let mut last_update = <HashMap<i64, Mutex<Instant>>>::new();
        let last = Instant::now() - MIN_INTERVAL;

        for i in env::var("CHAT_LIST")
            .expect("Please set the environment variable CHAT_LIST")
            .split(',')
        {
            last_update.insert(
                i64::from_str(i).expect("Parsing CHAT_LIST failed"),
                Mutex::new(last),
            );
        }

        Mutex::new(last_update)
    };
}

#[derive(BotCommand)]
#[command(description = "本 bot 支持如下命令:")]
pub enum Command {
    #[command(rename = "help", description = "显示帮助信息")]
    Help,
    #[command(rename = "set_avatar", description = "设置头像")]
    SetAvatar(String),
}

macro_rules! file_id {
    ($msg:expr, $func:ident) => {
        $msg.$func()
            .filter(|&x| x.thumb.is_some() && x.file_size.map_or(false, |x| x <= MAX_FILESIZE))
            .map(|x| &x.file_id)
    };
}

impl Command {
    async fn help(cx: &Context) -> Result<(), Error> {
        cx.answer(Command::descriptions()).await?;
        Ok(())
    }

    async fn set_avatar(color: &str, cx: &Context) -> Result<(), Error> {
        let chat_id = cx.chat_id();

        let last_update = LAST_UPDATE.lock().await;
        let mut chat_last_update = if let Some(x) = last_update.get(&chat_id) {
            x.lock().await
        } else {
            cx.reply_to(format!("尚未向本群组 ({}) 提供服务", chat_id))
                .await?;
            return Ok(());
        };
        if chat_last_update.elapsed() < MIN_INTERVAL {
            cx.reply_to("技能冷却中").await?;
            return Ok(());
        }

        if let MessageKind::Common(MessageCommon {
            forward_kind:
                ForwardKind::Origin(ForwardOrigin {
                    reply_to_message: Some(msg),
                    ..
                }),
            ..
        }) = &cx.update.kind
        {
            let file_id = msg
                .sticker()
                .map(|x| &x.file_id)
                .or_else(|| {
                    msg.photo()
                        .map(|x| x.iter().max_by_key(|&x| x.file_size).map(|x| &x.file_id))
                        .flatten()
                })
                .or_else(|| file_id!(msg, document))
                .or_else(|| file_id!(msg, animation))
                .or_else(|| file_id!(msg, video));

            let image = if let Some(file_id) = file_id {
                let mut buf = Vec::new();
                let file = cx.requester.get_file(file_id).await?;
                cx.requester
                    .download_file(&file.file_path, &mut buf)
                    .await?;

                if file.file_path.ends_with(".webp") || file.file_path.ends_with(".png") {
                    image_to_png(&mut buf, color)?;
                } else if file.file_path.ends_with(".mp4") || file.file_path.ends_with(".webm") {
                    buf = video_to_png(buf)?;
                    image_to_png(&mut buf, color)?;
                }

                Some(buf)
            } else if let Some(
                [MessageEntity {
                    kind: MessageEntityKind::Url,
                    offset,
                    length,
                }, ..],
            ) = msg.entities()
            {
                let url = &msg.text().unwrap()[*offset..*offset + *length];

                link_to_img(url).await?
            } else {
                None
            };

            if let Some(buf) = image {
                cx.requester
                    .set_chat_photo(chat_id, InputFile::memory("avatar.file", buf))
                    .await?;
                *chat_last_update = Instant::now();
            } else {
                cx.reply_to("未检测到受支持的头像").await?;
            }
        } else {
            cx.reply_to("使用 set_avatar 命令时请回复包含头像的消息 (照片、视频、贴纸、文件)")
                .await?;
        }
        Ok(())
    }

    pub async fn run(cx: Context, command: Self) -> Result<(), Error> {
        match command {
            Command::Help => Self::help(&cx).await,
            Command::SetAvatar(color) => {
                let ret = Self::set_avatar(&color, &cx).await;
                if let Err(e) = &ret {
                    if let Some(x) = e.message() {
                        cx.reply_to(x).await?;
                        return Ok(());
                    }
                    cx.reply_to("出现了预料外的错误").await?;
                }
                ret
            }
        }
    }
}
