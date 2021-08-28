use std::env;
use std::error::Error;

use image_convert::{to_jpg, ColorName, ImageResource, JPGConfig};
use teloxide::net::Download;
use teloxide::prelude::*;
use teloxide::types::{ForwardKind, InputFile, MessageKind};
use teloxide::utils::command::BotCommand;

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
            if cx.update.chat.is_supergroup() || cx.update.chat.is_group() {
                match &cx.update.kind {
                    MessageKind::Common(common) => {
                        if let ForwardKind::Origin(orig) = &common.forward_kind {
                            if let Some(msg) = &orig.reply_to_message {
                                let mut file_id;
                                file_id = msg.sticker().map(|x| x.file_id.clone());
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
                                if let Some(file_id) = file_id {
                                    let mut buf = Vec::new();
                                    let file = cx.requester.get_file(&file_id).await?;
                                    cx.requester
                                        .download_file(&file.file_path, &mut buf)
                                        .await?;
                                    if file.file_path.ends_with(".webp") {
                                        let image = ImageResource::from_reader(&*buf)?;
                                        let mut jpg = ImageResource::Data(Vec::new());
                                        let mut config = JPGConfig::new();
                                        config.background_color = Some(ColorName::White);
                                        config.quality = 100;
                                        to_jpg(&mut jpg, &image, &config)?;
                                        buf = jpg.into_vec().unwrap();
                                    }
                                    cx.requester
                                        .set_chat_photo(
                                            cx.update.chat.id,
                                            InputFile::memory("avatar.file", buf),
                                        )
                                        .await?;
                                } else {
                                    cx.reply_to("未检测到受支持的头像").await?;
                                }
                            }
                        }
                    }
                    _ => {}
                }
            }
        }
    };

    Ok(())
}

#[tokio::main]
async fn main() {
    teloxide::enable_logging!();

    let bot_token = env::var("BOT_TOKEN").expect("Please set the environment variable BOT_TOKEN.");
    let bot_name = env::var("BOT_NAME").expect("Please set the environment variable BOT_NAME.");

    let bot = Bot::new(bot_token).auto_send();
    teloxide::commands_repl(bot, bot_name, answer).await;
}
