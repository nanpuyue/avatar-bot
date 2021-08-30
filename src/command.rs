use std::collections::HashMap;
use std::env;
use std::error::Error;
use std::str::FromStr;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use lazy_static::lazy_static;
use teloxide::net::Download;
use teloxide::prelude::*;
use teloxide::types::{ForwardKind, InputFile, MessageKind};
use teloxide::utils::command::BotCommand;

use crate::convert::{mp4_to_png, webp_to_png};

const MIN_INTERVAL: Duration = Duration::from_secs(30);
const MAX_FILESIZE: u32 = 10 * 1024 * 1024;

lazy_static! {
    pub static ref LAST_UPDATE: Arc<Mutex<HashMap<i64, Instant>>> =
        Arc::new(Mutex::new(<HashMap<i64, Instant>>::new()));
    pub static ref CHAT_LIST: Vec<i64> = {
        let mut chat_list = Vec::new();
        for i in env::var("CHAT_LIST")
            .expect("Please set the environment variable CHAT_LIST")
            .split(',')
        {
            chat_list.push(i64::from_str(i).expect("Parsing CHAT_LIST failed"));
        }
        chat_list
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

pub async fn answer(
    cx: UpdateWithCx<AutoSend<Bot>, Message>,
    command: Command,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    match command {
        Command::Help => {
            cx.answer(Command::descriptions()).await?;
        }
        Command::SetAvatar(background) => {
            let chat_id = cx.chat_id();
            if CHAT_LIST.contains(&chat_id) {
                if LAST_UPDATE
                    .lock()
                    .unwrap()
                    .get(&chat_id)
                    .map_or(false, |x| x.elapsed() < MIN_INTERVAL)
                {
                    cx.reply_to("技能冷却中").await?;
                    return Ok(());
                }

                if let MessageKind::Common(common) = &cx.update.kind {
                    if let ForwardKind::Origin(orig) = &common.forward_kind {
                        if let Some(msg) = &orig.reply_to_message {
                            let mut file_id = msg.sticker().map(|x| x.file_id.clone());

                            if file_id.is_none() {
                                file_id = msg
                                    .photo()
                                    .map(|x| {
                                        x.iter()
                                            .max_by_key(|&x| x.file_size)
                                            .map(|x| x.file_id.clone())
                                    })
                                    .flatten();
                            }

                            if file_id.is_none() {
                                file_id = msg
                                    .document()
                                    .filter(|&x| {
                                        x.thumb.is_some()
                                            && x.file_size.map_or(false, |x| x <= MAX_FILESIZE)
                                    })
                                    .map(|x| x.file_id.clone())
                            }

                            if file_id.is_none() {
                                file_id = msg
                                    .animation()
                                    .filter(|&x| {
                                        x.thumb.is_some()
                                            && x.file_size.map_or(false, |x| x <= MAX_FILESIZE)
                                    })
                                    .map(|x| x.file_id.clone())
                            }

                            if let Some(file_id) = file_id {
                                let mut buf = Vec::new();
                                let file = cx.requester.get_file(&file_id).await?;
                                cx.requester
                                    .download_file(&file.file_path, &mut buf)
                                    .await?;

                                if file.file_path.ends_with(".webp") {
                                    buf = webp_to_png(buf.as_ref(), &background)?;
                                }
                                if file.file_path.ends_with(".mp4") {
                                    buf = mp4_to_png(buf.as_ref())?;
                                }

                                match cx
                                    .requester
                                    .set_chat_photo(chat_id, InputFile::memory("avatar.file", buf))
                                    .await
                                {
                                    Err(_) => {
                                        cx.reply_to("出现了预料外的错误").await?;
                                    }
                                    Ok(_) => {
                                        LAST_UPDATE.lock().unwrap().insert(chat_id, Instant::now());
                                    }
                                };
                            } else {
                                cx.reply_to("未检测到受支持的头像").await?;
                            }
                        }
                    }
                }
            }
        }
    };

    Ok(())
}
