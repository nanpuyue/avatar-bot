use std::env;

use teloxide::prelude::*;

use crate::command::{answer, CHAT_LIST};

mod command;
mod convert;

#[tokio::main]
async fn main() {
    teloxide::enable_logging!();

    let bot_token = env::var("BOT_TOKEN").expect("Please set the environment variable BOT_TOKEN");
    let bot_name = env::var("BOT_NAME").expect("Please set the environment variable BOT_NAME");

    lazy_static::initialize(&CHAT_LIST);

    let bot = Bot::new(bot_token).auto_send();
    teloxide::commands_repl(bot, bot_name, answer).await;
}
