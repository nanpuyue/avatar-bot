use std::env;

use rsmpeg::ffi;
use teloxide::prelude::*;

use crate::command::{Command, LAST_UPDATE};

mod command;
mod error;
mod ffmpeg;
mod image;
mod opencv;
mod opengraph;

#[tokio::main]
async fn main() {
    pretty_env_logger::init();

    unsafe { ffi::av_log_set_level(ffi::AV_LOG_ERROR as i32) };

    let bot_token = env::var("BOT_TOKEN").expect("Please set the environment variable BOT_TOKEN");
    lazy_static::initialize(&LAST_UPDATE);

    let channel_post = Update::filter_channel_post()
        .filter_command::<Command>()
        .endpoint(Command::run);
    let message = Update::filter_message()
        .filter_command::<Command>()
        .endpoint(Command::run);
    let handler = dptree::entry().branch(message).branch(channel_post);

    let bot = Bot::new(bot_token);
    Dispatcher::builder(bot, handler)
        .enable_ctrlc_handler()
        .build()
        .dispatch()
        .await;
}
