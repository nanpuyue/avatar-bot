use std::collections::HashMap;
use std::env;
use std::io::Cursor;
use std::str::FromStr;
use std::time::{Duration, Instant};

use grammers_client::types::media::Uploaded;
use grammers_client::types::photo_sizes::VecExt;
use grammers_client::types::{Downloadable, Media, Message, PackedChat};
use grammers_client::{Client, InputMessage, Update};
use grammers_tl_types::enums::{InputChatPhoto, MessageEntity};
use grammers_tl_types::functions::channels::EditPhoto;
use grammers_tl_types::types::{InputChatUploadedPhoto, MessageEntityCode};
use lazy_static::lazy_static;
use tokio::sync::Mutex;
use tokio::task::spawn;
use tokio::time::timeout;

use crate::error::{Error, IntoErrorMessage, Message as _};
use crate::image::{image_to_png, tgs_to_png};
use crate::opengraph::link_to_img;
use crate::video::{tgs_to_mp4, video_to_mp4};
use crate::USERNAME;

const SET_TIMEOUT: Duration = Duration::from_secs(60);
const MIN_INTERVAL: Duration = Duration::from_secs(30);
const MAX_FILESIZE: usize = 10 * 1024 * 1024;

lazy_static! {
    pub static ref LAST_UPDATE: HashMap<i64, Mutex<Instant>> = {
        let mut last_update = HashMap::new();
        let last = Instant::now() - MIN_INTERVAL;

        for i in env::var("CHAT_LIST").expect("CHAT_LIST").split(',') {
            last_update.insert(
                i64::from_str(i).expect("Parsing CHAT_LIST failed"),
                Mutex::new(last),
            );
        }

        last_update
    };
}

#[derive(Clone, Copy, Debug)]
pub enum Color {
    Rgb([i32; 3]),
    Trans,
}

#[derive(Clone, Copy, Debug)]
pub enum Align {
    Top,
    Bottom,
    Center,
}

#[derive(Clone, Copy, Debug)]
pub struct Opt {
    pub color: Color,
    pub align: Option<Align>,
    pub dry_run: bool,
    pub show_detect: bool,
}

#[derive(Debug)]
enum Command {
    Help,
    SetAvatar(Opt),
}

impl From<&str> for Opt {
    fn from(opt: &str) -> Self {
        let mut color = Color::Rgb([0xff, 0xff, 0xff]);
        let mut align = None;
        let mut dry_run = false;
        let mut show_detect = false;
        for x in opt.split_whitespace().take(3) {
            match x {
                "t" | "top" => align = Some(Align::Top),
                "b" | "bottom" => align = Some(Align::Bottom),
                "c" | "center" => align = Some(Align::Center),
                "d" | "dry" => dry_run = true,
                "s" | "show" => {
                    align = None;
                    dry_run = true;
                    show_detect = true;
                }
                "tr" | "trans" => color = Color::Trans,
                x => {
                    let [_, rgb @ ..] = u32::from_str_radix(x.trim().trim_start_matches('#'), 16)
                        .unwrap_or(0xffffff)
                        .to_be_bytes()
                        .map(|x| x as _);
                    color = Color::Rgb(rgb)
                }
            }
        }

        Self {
            color,
            align,
            dry_run,
            show_detect,
        }
    }
}

trait Entity {
    fn entity(&self, offset: i32, length: i32) -> &str;
    fn url(&self) -> Option<&str>;
    fn bot_command(&self, username: &str) -> Option<Command>;
}

impl Entity for Message {
    fn entity(&self, offset: i32, length: i32) -> &str {
        let text = self.text();
        let start = match offset {
            0 => 0,
            _ => text
                .chars()
                .take(offset as _)
                .fold(0, |acc, x| acc + x.len_utf8()),
        };
        let end = match length {
            -1 => text.len(),
            _ => text
                .chars()
                .skip(offset as _)
                .take(length as _)
                .fold(start, |acc, x| acc + x.len_utf8()),
        };

        &text[start..end]
    }

    fn url(&self) -> Option<&str> {
        match self.fmt_entities() {
            None => None,
            Some(x) => x.iter().find_map(|x| match x {
                MessageEntity::Url(x) => {
                    let url = self.entity(x.offset, x.length);
                    Some(url)
                }
                _ => None,
            }),
        }
    }

    fn bot_command(&self, username: &str) -> Option<Command> {
        match self.fmt_entities() {
            None => None,
            Some(x) => x.iter().find_map(|x| match x {
                MessageEntity::BotCommand(x) if x.offset == 0 => {
                    let mut command = self.entity(x.offset, x.length);
                    if let Some((a, b)) = command.split_once('@') {
                        if b != username {
                            return None;
                        }
                        command = a
                    }
                    let opt = self.entity(x.offset + x.length, -1);
                    match command {
                        "/help" => Some(Command::Help),
                        "/set_avatar" => {
                            let opt = opt.trim().into();
                            Some(Command::SetAvatar(opt))
                        }
                        _ => None,
                    }
                }
                _ => None,
            }),
        }
    }
}

trait RunCommand {
    async fn help(&mut self, message: &Message) -> Result<(), Error>;
    async fn set_avatar(&mut self, message: &Message, opt: &Opt) -> Result<(), Error>;
    async fn upload_file(&mut self, file: Vec<u8>, name: &str) -> Result<Uploaded, Error>;
    async fn edit_photo<C: Into<PackedChat>>(
        &mut self,
        chat: C,
        uploaded: Uploaded,
        is_video: bool,
    ) -> Result<(), Error>;
}

impl RunCommand for Client {
    async fn help(&mut self, message: &Message) -> Result<(), Error> {
        let text = r###"
接头霸王为你服务

/help
显示帮助信息

/set_avatar
设置群头像, 使用时需要回复包含头像的消息, 支持图片、视频、贴纸、文件、链接等, 默认自动检测人脸并截取为头像图片。

可接如下选项, 最多接受三个选项, 选项顺序不敏感:
    t/top     截取顶部
    b/bottom  截取底部
    c/center  截取中间, 默认值, 但是自动检测到人脸除外, 可以指定这个选项跳过人脸检测
    d/dry     回复处理后的头像, 不执行设置头像的操作
    s/show    回复人脸检测结果, 不执行设置头像的操作, 设置这个选项则截取选项和背景颜色都无效
    color     背景颜色, 默认为白色, 十六进制 RGB 格式或别名, 只对有透明度的头像有效
当前可用背景颜色别名:
    tr/trans  跨性别旗

示例:
    /set_avatar
    /set_avatar s
    /set_avatar tr d
    /set_avatar t ffc0cb
    /set_avatar t ffc0cb d
"###
        .trim();

        let mut input_message = InputMessage::text(text);
        input_message = input_message.fmt_entities(vec![MessageEntity::Code(MessageEntityCode {
            offset: 0,
            length: text.chars().count() as _,
        })]);

        // TODO
        let _ = self.send_message(message.chat(), input_message).await?;
        Ok(())
    }

    async fn edit_photo<C: Into<PackedChat>>(
        &mut self,
        chat: C,
        uploaded: Uploaded,
        is_video: bool,
    ) -> Result<(), Error> {
        let chat = Into::<PackedChat>::into(chat);
        let channel = chat
            .try_to_input_channel()
            .ok_or("获取群组信息失败".error())?;

        let mut photo = InputChatUploadedPhoto {
            file: None,
            video: None,
            video_start_ts: None,
            video_emoji_markup: None,
        };
        let input_file = uploaded.into();
        if is_video {
            photo.video.replace(input_file);
        } else {
            photo.file.replace(input_file);
        }
        let photo = InputChatPhoto::InputChatUploadedPhoto(photo);

        // TODO
        let _ = self.invoke(&EditPhoto { photo, channel }).await?;
        Ok(())
    }

    async fn upload_file(&mut self, file: Vec<u8>, name: &str) -> Result<Uploaded, Error> {
        let len = file.len();
        let mut file = Cursor::new(file);
        let uploaded = self.upload_stream(&mut file, len, name.into()).await?;
        Ok(uploaded)
    }

    async fn set_avatar(&mut self, message: &Message, opt: &Opt) -> Result<(), Error> {
        let chat = &message.chat();
        let chat_id = chat.id();

        let mut chat_last_update = if let Some(x) = LAST_UPDATE.get(&chat_id) {
            x.try_lock().or("正在处理之前的请求, 请稍后...".result())?
        } else {
            return format!("尚未向本群组 ({chat_id}) 提供服务").result();
        };
        if !opt.dry_run && chat_last_update.elapsed() < MIN_INTERVAL {
            return "技能冷却中".result();
        }

        if let Some(ref x) = message.reply_to_message_id() {
            let message = self
                .get_messages_by_id(chat, &[*x])
                .await?
                .swap_remove(0)
                .ok_or("读取引用的消息失败".error())?;

            let mut is_square = false;
            let mut is_video = false;
            let image = if let Some(media) = message.media() {
                let mut download = false;
                let mut mime = None;
                let mut sticker_id = 0;
                match &media {
                    Media::Photo(x) => {
                        if let Some(x) = x.thumbs().largest() {
                            download = x.size() <= MAX_FILESIZE;
                        }
                    }
                    Media::Document(x) => {
                        download = x.size() <= MAX_FILESIZE as _;
                        mime = x.mime_type();
                        if let Some((width, height)) = x.resolution() {
                            is_square = width == height;
                        }
                    }
                    Media::Sticker(x) => {
                        download = x.document.size() <= MAX_FILESIZE as _;
                        mime = x.document.mime_type();
                        if let Some((width, height)) = x.document.resolution() {
                            is_square = width == height;
                        }
                        sticker_id = x.document.id();
                    }
                    _ => {}
                }

                let mime = mime.map(str::to_string);
                if download {
                    let mut buf = Vec::new();
                    let mut downloader = self.iter_download(&Downloadable::Media(media));
                    while let Some(x) = downloader.next().await? {
                        buf.extend(x);
                    }

                    if let Some(x) = mime {
                        if x.starts_with("video/") {
                            is_video = true;
                            if !is_square || !x.starts_with("video/mp4") {
                                buf = video_to_mp4(buf, opt.color)?;
                                is_square = true
                            }
                        } else if x == "application/x-tgsticker" {
                            is_video = true;
                            if is_square {
                                buf = tgs_to_mp4(buf, &format!("{sticker_id}"), opt.color)?;
                            } else {
                                buf = tgs_to_png(buf, &format!("{sticker_id}"))?;
                            }
                        }
                    }
                    Some(buf)
                } else if let Some(x) = message.url() {
                    link_to_img(x).await?
                } else {
                    None
                }
            } else {
                None
            };

            if let Some(mut buf) = image {
                is_video = is_video && is_square;
                let file_name = if is_video {
                    "file.mp4"
                } else {
                    image_to_png(&mut buf, opt)?;
                    "file.png"
                };
                let uploaded = self.upload_file(buf, file_name).await?;
                if opt.dry_run {
                    let mut input_message = InputMessage::text("").reply_to(Some(message.id()));
                    if is_video {
                        input_message = input_message.document(uploaded).mime_type("video/mp4");
                    } else {
                        input_message = input_message.photo(uploaded);
                    }
                    self.send_message(chat, input_message).await?;
                } else {
                    self.edit_photo(chat, uploaded, is_video).await?;
                    *chat_last_update = Instant::now();
                }
            } else {
                return "未检测到受支持的头像".result();
            }
        } else {
            return "使用 set_avatar 命令时请回复包含头像的消息 (照片、视频、贴纸、文件)".result();
        }
        Ok(())
    }
}

pub fn handle_update(client: &Client, update: Update) {
    let username = USERNAME.get().unwrap();
    match update {
        Update::NewMessage(message) if !message.outgoing() => {
            if let Some(command) = message.bot_command(username) {
                let mut bot = client.clone();
                spawn(async move {
                    let ret = match command {
                        Command::Help => bot.help(&message).await,
                        Command::SetAvatar(opt) => {
                            timeout(SET_TIMEOUT, bot.set_avatar(&message, &opt))
                                .await
                                .unwrap_or("请求处理超时".result())
                        }
                    };
                    if let Err(e) = ret {
                        match e.message() {
                            Some(x) => {
                                let error_message =
                                    InputMessage::text(x).reply_to(Some(message.id()));
                                if let Err(e) =
                                    bot.send_message(&message.chat(), error_message).await
                                {
                                    println!("Failed to send error message \"{x}\": {e}");
                                }
                            }
                            None => println!("Failed to handle update: {e}"),
                        };
                    };
                });
            }
        }
        _ => {}
    }
}
