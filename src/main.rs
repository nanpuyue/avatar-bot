use std::env;

use rsmpeg::ffi;
use teloxide::prelude::*;

use crate::command::{Command, LAST_UPDATE};

mod command;
mod error;
mod ffmpeg;
mod image;
mod opengraph;

#[tokio::main]
async fn main() {
    pretty_env_logger::init();

    unsafe { ffi::av_log_set_level(ffi::AV_LOG_ERROR as i32) };

    let bot_token = env::var("BOT_TOKEN").expect("Please set the environment variable BOT_TOKEN");
    lazy_static::initialize(&LAST_UPDATE);

    let bot = Bot::new(bot_token);
    Command::repl(bot, Command::run).await;
}
