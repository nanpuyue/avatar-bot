use std::collections::HashMap;
use std::str::FromStr;
use std::time::{Duration, Instant};
use std::{env, io};

use lazy_static::lazy_static;
use teloxide::net::Download;
use teloxide::prelude::*;
use teloxide::types::{InputFile, MessageEntityKind, StickerFormat::*};
use teloxide::utils::command::BotCommands;
use tokio::sync::Mutex;

use crate::error::{Error, Message as _};
use crate::ffmpeg::video_to_png;
use crate::image::image_to_png;
use crate::opengraph::link_to_img;

const MIN_INTERVAL: Duration = Duration::from_secs(30);
const MAX_FILESIZE: u32 = 10 * 1024 * 1024;

lazy_static! {
    pub static ref LAST_UPDATE: HashMap<ChatId, Mutex<Instant>> = {
        let mut last_update = HashMap::new();
        let last = Instant::now() - MIN_INTERVAL;

        for i in env::var("CHAT_LIST")
            .expect("Please set the environment variable CHAT_LIST")
            .split(',')
        {
            last_update.insert(
                ChatId(i64::from_str(i).expect("Parsing CHAT_LIST failed")),
                Mutex::new(last),
            );
        }

        last_update
    };
}

#[derive(BotCommands, Clone)]
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
            .filter(|&x| x.thumb.is_some() && x.file.size <= MAX_FILESIZE)
            .map(|x| (&x.file.id, stringify!($func)))
    };
}

impl Command {
    async fn set_avatar(bot: &Bot, message: &Message, args: String) -> Result<(), Error> {
        let mut align = None;
        let mut dry_run = false;
        let mut show_detect = false;
        let mut color = "ffffff";
        for x in args.split_whitespace().take(3) {
            match x {
                "t" | "top" | "b" | "bottom" | "c" | "center" => align = Some(x),
                "d" | "dry" => dry_run = true,
                "s" | "show" => {
                    align = None;
                    dry_run = true;
                    show_detect = true;
                }
                _ => color = x,
            }
        }

        let chat_id = message.chat.id;

        let mut chat_last_update = if let Some(x) = LAST_UPDATE.get(&chat_id) {
            x.lock().await
        } else {
            bot.send_message(chat_id, format!("尚未向本群组 ({}) 提供服务", chat_id))
                .await?;
            return Ok(());
        };
        if !dry_run && chat_last_update.elapsed() < MIN_INTERVAL {
            bot.send_message(chat_id, "技能冷却中").await?;
            return Ok(());
        }

        if let Some(message) = message.reply_to_message() {
            let file_id = message
                .photo()
                .and_then(|x| {
                    x.iter()
                        .max_by_key(|&x| x.file.size)
                        .map(|x| (&x.file.id, "photo"))
                })
                .or_else(|| file_id!(message, sticker))
                .or_else(|| file_id!(message, document))
                .or_else(|| file_id!(message, animation))
                .or_else(|| file_id!(message, video));

            let image = if let Some((file_id, file_type)) = file_id {
                let tgs_to_png;
                let mut download = true;
                let mut file_to_png: Option<&(dyn Fn(_) -> _ + Sync)> = None;
                match (file_type, message.sticker().map(|x| &x.format)) {
                    ("sticker", Some(Raster)) | ("photo", _) => {}
                    ("sticker", Some(Animated)) => {
                        tgs_to_png = |x| crate::image::tgs_to_png(x, file_id);
                        file_to_png.replace(&tgs_to_png);
                    }
                    ("sticker" | "video" | "animation", _) => {
                        file_to_png.replace(&video_to_png);
                    }
                    _ => {
                        let mime = message.document().and_then(|x| x.mime_type.as_ref());
                        match mime.map(|x| x.type_().as_str()) {
                            Some("image") => {}
                            Some("video") => {
                                file_to_png.replace(&video_to_png);
                            }
                            _ => download = false,
                        }
                    }
                };

                if download {
                    let mut buf = Vec::new();
                    let file = bot.get_file(file_id).await?;
                    bot.download_file(&file.path, &mut buf).await?;
                    if let Some(file_to_png) = file_to_png {
                        buf = file_to_png(buf)?;
                    }
                    Some(buf)
                } else {
                    None
                }
            } else if let Some(entities) = message.entities() {
                let mut buf = None;
                for e in entities {
                    if e.kind == MessageEntityKind::Url {
                        if let Some(url) = message
                            .text()
                            .map(|x| x.chars().skip(e.offset).take(e.length).collect::<String>())
                        {
                            buf = link_to_img(&url).await?;
                        }
                        break;
                    }
                }
                buf
            } else {
                None
            };

            if let Some(mut buf) = image {
                image_to_png(&mut buf, color, align, show_detect)?;
                let photo = InputFile::memory(buf);
                if dry_run {
                    let mut send_photo = bot.send_photo(chat_id, photo);
                    send_photo.reply_to_message_id = Some(message.id);
                    send_photo.await?;
                } else {
                    bot.set_chat_photo(chat_id, photo).await?;
                    *chat_last_update = Instant::now();
                }
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

    pub async fn run(bot: Bot, message: Message, command: Command) -> ResponseResult<()> {
        let chat_id = message.chat.id;

        match command {
            Command::Help => {
                bot.send_message(chat_id, Command::descriptions().to_string())
                    .await?;
                Ok(())
            }
            Command::SetAvatar(args) => match Self::set_avatar(&bot, &message, args).await {
                Ok(_) => Ok(()),
                Err(e) => {
                    if let Some(x) = e.message() {
                        bot.send_message(chat_id, x).await?;
                        return Ok(());
                    }
                    bot.send_message(chat_id, "出现了预料外的错误").await?;
                    Err(io::Error::new(io::ErrorKind::Other, e).into())
                }
            },
        }
    }
}
