use std::collections::HashMap;
use std::env;
use std::str::FromStr;
use std::time::{Duration, Instant};

use lazy_static::lazy_static;
use teloxide::net::Download;
use teloxide::prelude::*;
use teloxide::types::{InputFile, MessageCommon, MessageEntity, MessageEntityKind, MessageKind};
use teloxide::utils::command::BotCommands;
use tokio::sync::Mutex;

use crate::error::{Error, Message as _};
use crate::ffmpeg::video_to_png;
use crate::image::image_to_png;
use crate::opengraph::link_to_img;

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

#[derive(BotCommands, Clone)]
#[command(rename = "snake_case", description = "本 bot 支持如下命令:")]
pub enum Command {
    #[command(description = "显示帮助信息")]
    Help,
    #[command(description = "设置头像")]
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
    async fn set_avatar(
        color: &str,
        align: Option<&str>,
        bot: &AutoSend<Bot>,
        message: &Message,
    ) -> Result<(), Error> {
        let chat_id = message.chat.id;

        let last_update = LAST_UPDATE.lock().await;
        let mut chat_last_update = if let Some(x) = last_update.get(&chat_id.0) {
            x.lock().await
        } else {
            bot.send_message(chat_id, format!("尚未向本群组 ({}) 提供服务", chat_id))
                .await?;
            return Ok(());
        };
        if chat_last_update.elapsed() < MIN_INTERVAL {
            bot.send_message(chat_id, "技能冷却中").await?;
            return Ok(());
        }

        if let MessageKind::Common(MessageCommon {
            reply_to_message: Some(msg),
            ..
        }) = &message.kind
        {
            let file_id = msg
                .sticker()
                .map(|x| &x.file_id)
                .or_else(|| {
                    msg.photo()
                        .and_then(|x| x.iter().max_by_key(|&x| x.file_size).map(|x| &x.file_id))
                })
                .or_else(|| file_id!(msg, document))
                .or_else(|| file_id!(msg, animation))
                .or_else(|| file_id!(msg, video));

            let image = if let Some(file_id) = file_id {
                let mut buf = Vec::new();
                let file = bot.get_file(file_id).await?;
                bot.download_file(&file.file_path, &mut buf).await?;

                if file.file_path.ends_with(".mp4") || file.file_path.ends_with(".webm") {
                    buf = video_to_png(buf)?;
                }

                Some(buf)
            } else if let Some(
                &[MessageEntity {
                    kind: MessageEntityKind::Url,
                    offset,
                    length,
                }, ..],
            ) = msg.entities()
            {
                let url = &msg.text().unwrap_or_default()[offset..offset + length];

                link_to_img(url).await?
            } else {
                None
            };

            if let Some(mut buf) = image {
                image_to_png(&mut buf, color, align)?;
                bot.set_chat_photo(chat_id, InputFile::memory(buf)).await?;
                *chat_last_update = Instant::now();
            } else {
                bot.send_message(chat_id, "未检测到受支持的头像").await?;
            }
        } else {
            bot.send_message(
                chat_id,
                "使用 set_avatar 命令时请回复包含头像的消息 (照片、视频、贴纸、文件)",
            )
            .await?;
        }
        Ok(())
    }

    pub async fn run(bot: AutoSend<Bot>, message: Message, command: Self) -> Result<(), Error> {
        let chat_id = message.chat.id;
        match command {
            Command::Help => {
                bot.send_message(chat_id, Command::descriptions().to_string())
                    .await?;
                Ok(())
            }
            Command::SetAvatar(args) => {
                let mut args = args.split(' ');
                let mut align = None;
                let mut color = "";
                let next = args.next().unwrap_or_default();
                match next {
                    "" => {}
                    "t" | "top" | "b" | "bottom" => align = Some(next),
                    x => {
                        color = x;
                        let next = args.next().unwrap_or_default();
                        match next {
                            "t" | "top" | "b" | "bottom" => align = Some(next),
                            _ => {}
                        }
                    }
                }

                let ret = Self::set_avatar(color, align, &bot, &message).await;
                if let Err(e) = &ret {
                    if let Some(x) = e.message() {
                        bot.send_message(chat_id, x).await?;
                        return Ok(());
                    }
                    bot.send_message(chat_id, "出现了预料外的错误").await?;
                }
                ret
            }
        }
    }
}
