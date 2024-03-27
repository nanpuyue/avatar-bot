use std::env;
use std::sync::OnceLock;

use grammers_client::{Client, Config, InitParams};
use grammers_session::Session;
use tokio::select;
use tokio::signal::ctrl_c;
use tokio::task;

use crate::command::{handle_update, LAST_UPDATE};
use crate::error::Error;

mod command;
mod error;
mod ffmpeg;
mod image;
mod opencv;
mod opengraph;

pub static USERNAME: OnceLock<String> = OnceLock::new();

#[tokio::main]
async fn main() -> Result<(), Error> {
    let api_id = env::var("API_ID")
        .expect("API_ID")
        .parse()
        .expect("API_ID invalid");
    let api_hash = env::var("API_HASH").expect("API_HASH");
    let token = env::var("BOT_TOKEN").expect("BOT_TOKEN");
    let session_file = env::var("SESSION_FILE").expect("SESSION_FILE");

    lazy_static::initialize(&LAST_UPDATE);

    println!("Connecting to Telegram...");
    let client = Client::connect(Config {
        session: Session::load_file_or_create(&session_file)?,
        api_id,
        api_hash,
        params: InitParams::default(),
    })
    .await?;
    println!("Connected!");

    if !client.is_authorized().await? {
        println!("Signing in...");
        client.bot_sign_in(&token).await?;
        client.session().save_to_file(&session_file)?;
        println!("Signed in!");
    }

    let username = client.get_me().await?.username().unwrap_or_default().into();
    USERNAME.set(username)?;

    println!("Handling messages...");
    loop {
        let update = match select! {
            _ = ctrl_c() => break,
            x = client.next_update() => x,
        }? {
            Some(x) => x,
            None => break,
        };

        let client = client.clone();
        task::spawn(async move {
            match handle_update(client, update).await {
                Ok(_) => {}
                Err(e) => eprintln!("Failed to handle update: {}", e),
            }
        });
    }

    println!("Saving session file and exiting...");
    client.session().save_to_file(&session_file)?;
    Ok(())
}
