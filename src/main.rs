use std::env;

use rsmpeg::ffi;
use teloxide::prelude::*;

use crate::command::{Command, LAST_UPDATE};

mod command;
mod error;
mod ffmpeg;
mod image;

#[tokio::main]
async fn main() {
    teloxide::enable_logging!();
    unsafe { ffi::av_log_set_level(ffi::AV_LOG_ERROR as i32) };

    let bot_token = env::var("BOT_TOKEN").expect("Please set the environment variable BOT_TOKEN");
    let bot_name = env::var("BOT_NAME").expect("Please set the environment variable BOT_NAME");

    lazy_static::initialize(&LAST_UPDATE);

    let bot = Bot::new(bot_token).auto_send();
    teloxide::commands_repl(bot, bot_name, Command::run).await;
}
