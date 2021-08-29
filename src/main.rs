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

use crate::convert::webp_to_jpg;

mod convert;

const MIN_INTERVAL: Duration = Duration::from_secs(30);
const MAX_FILESIZE: u32 = 10 * 1024 * 1024;

lazy_static! {
    static ref LAST_UPDATE: Arc<Mutex<HashMap<i64, Instant>>> =
        Arc::new(Mutex::new(<HashMap<i64, Instant>>::new()));
    static ref CHAT_LIST: Vec<i64> = {
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
enum Command {
    #[command(rename = "help", description = "显示帮助信息")]
    Help,
    #[command(rename = "set_avatar", description = "设置头像")]
    SetAvatar,
}

async fn answer(
    cx: UpdateWithCx<AutoSend<Bot>, Message>,
    command: Command,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    match command {
        Command::Help => {
            cx.answer(Command::descriptions()).await?;
        }
        Command::SetAvatar => {
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

                            if let Some(file_id) = file_id {
                                let mut buf = Vec::new();
                                let file = cx.requester.get_file(&file_id).await?;
                                cx.requester
                                    .download_file(&file.file_path, &mut buf)
                                    .await?;

                                if file.file_path.ends_with(".webp") {
                                    buf = webp_to_jpg(buf.as_ref())?;
                                }

                                cx.requester
                                    .set_chat_photo(chat_id, InputFile::memory("avatar.file", buf))
                                    .await?;
                                LAST_UPDATE.lock().unwrap().insert(chat_id, Instant::now());
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

#[tokio::main]
async fn main() {
    teloxide::enable_logging!();

    let bot_token = env::var("BOT_TOKEN").expect("Please set the environment variable BOT_TOKEN");
    let bot_name = env::var("BOT_NAME").expect("Please set the environment variable BOT_NAME");

    lazy_static::initialize(&CHAT_LIST);

    let bot = Bot::new(bot_token).auto_send();
    teloxide::commands_repl(bot, bot_name, answer).await;
}
