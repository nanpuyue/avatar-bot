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

use crate::error::{Error, Message as _};
use crate::ffmpeg::video_to_png;
use crate::image::{image_to_png, tgs_to_png};
use crate::opengraph::link_to_img;
use crate::USERNAME;

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

trait Entity {
    fn url(&self) -> Option<String>;
    fn bot_command(&self, username: &str) -> Option<(String, String)>;
}

impl Entity for Message {
    fn url(&self) -> Option<String> {
        match self.fmt_entities() {
            None => None,
            Some(x) => x.iter().find_map(|x| match x {
                MessageEntity::Url(x) => {
                    let url = self
                        .text()
                        .chars()
                        .skip(x.offset as _)
                        .take(x.length as _)
                        .collect::<String>();
                    Some(url)
                }
                _ => None,
            }),
        }
    }

    fn bot_command(&self, username: &str) -> Option<(String, String)> {
        match self.fmt_entities() {
            None => None,
            Some(x) => x.iter().find_map(|x| match x {
                MessageEntity::BotCommand(x) => {
                    let mut command: String = self
                        .text()
                        .chars()
                        .skip(x.offset as _)
                        .take(x.length as _)
                        .collect();
                    if let Some((a, b)) = command.split_once('@') {
                        if b != username {
                            return None;
                        }
                        command = a.into()
                    }
                    let args: String = self.text().chars().skip(x.length as _).collect();
                    Some((command, args.trim().into()))
                }
                _ => None,
            }),
        }
    }
}

struct Opt<'a> {
    color: &'a str,
    align: Option<&'a str>,
    dry_run: bool,
    show_detect: bool,
}

impl<'a> Opt<'a> {
    fn new(args: &'a str) -> Self {
        let mut color = "ffffff";
        let mut align = None;
        let mut dry_run = false;
        let mut show_detect = false;
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

        Self {
            color,
            align,
            dry_run,
            show_detect,
        }
    }
}

trait RunCommand {
    async fn help(&mut self, message: &Message) -> Result<(), Error>;
    async fn set_avatar(&mut self, message: &Message, opt: Opt) -> Result<(), Error>;
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
    d/dry     回复处理的到的头像, 不执行设置头像的操作
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
            .ok_or("Failed to get input_channel")?;

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

    async fn set_avatar(&mut self, message: &Message, opt: Opt<'_>) -> Result<(), Error> {
        let chat = &message.chat();
        let chat_id = chat.id();

        let mut chat_last_update = if let Some(x) = LAST_UPDATE.get(&chat_id) {
            x.lock().await
        } else {
            self.send_message(
                chat,
                InputMessage::text(format!("尚未向本群组 ({chat_id}) 提供服务")),
            )
            .await?;
            return Ok(());
        };
        if !opt.dry_run && chat_last_update.elapsed() < MIN_INTERVAL {
            self.send_message(chat, InputMessage::text("技能冷却中"))
                .await?;
            return Ok(());
        }

        if let Some(ref x) = message.reply_to_message_id() {
            let message = self
                .get_messages_by_id(chat, &[*x])
                .await?
                .swap_remove(0)
                .ok_or("Failed to get reply")?;

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
                            is_video = width == height
                                && mime.filter(|&x| x.starts_with("video/mp4")).is_some()
                        }
                    }
                    Media::Sticker(x) => {
                        download = x.document.size() <= MAX_FILESIZE as _;
                        mime = x.document.mime_type();
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
                        if x.starts_with("video/") && !is_video {
                            buf = video_to_png(buf)?;
                        } else if x == "application/x-tgsticker" {
                            buf = tgs_to_png(buf, &format!("{sticker_id}"))?;
                        }
                    }
                    Some(buf)
                } else if let Some(x) = message.url() {
                    link_to_img(&x).await?
                } else {
                    None
                }
            } else {
                None
            };

            if let Some(mut buf) = image {
                let file_name = if is_video {
                    "file.mp4"
                } else {
                    image_to_png(&mut buf, opt.color, opt.align, opt.show_detect)?;
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
                self.send_message(chat, "未检测到受支持的头像").await?;
            }
        } else {
            self.send_message(
                chat,
                "使用 set_avatar 命令时请回复包含头像的消息 (照片、视频、贴纸、文件)",
            )
            .await?;
        }
        Ok(())
    }
}

pub async fn handle_update(mut client: Client, update: Update) -> Result<(), Error> {
    let username = USERNAME.get().unwrap();
    match update {
        Update::NewMessage(message) if !message.outgoing() => {
            if let Some((command, args)) = message.bot_command(username) {
                return match command.as_str() {
                    "/help" => client.help(&message).await,
                    "/set_avatar" => {
                        let opt = Opt::new(&args);
                        match client.set_avatar(&message, opt).await {
                            Err(e) => match e.message() {
                                Some(x) => {
                                    let _ = client
                                        .send_message(&message.chat(), InputMessage::text(x))
                                        .await?;
                                    Ok(())
                                }
                                None => Err(e),
                            },
                            x => x,
                        }
                    }
                    _ => Ok(()),
                };
            }
        }
        _ => {}
    }

    Ok(())
}
