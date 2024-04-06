use std::cmp::min;
use std::io::{Cursor, Read, Seek, SeekFrom, Write};
use std::slice;
use std::sync::{Arc, Mutex};

use flate2::write::GzDecoder;
use rlottie::{Animation, Surface};
use rsmpeg::avcodec::{AVCodec, AVCodecContext};
use rsmpeg::avformat::{
    AVFormatContextInput, AVFormatContextOutput, AVIOContextContainer, AVIOContextCustom,
};
use rsmpeg::avutil::{av_d2q, av_inv_q, AVFrame, AVMem, AVRational};
use rsmpeg::error::RsmpegError;
use rsmpeg::ffi;
use rsmpeg::swscale::SwsContext;

use crate::command::Color;
use crate::error::Error;
use crate::image::{set_color, trans_flag};

struct SurfaceIter {
    surface: Surface,
    animation: Animation,
    frame_buffer: AVFrame,
    frame_index: usize,
    totalframe: usize,
    width: i32,
    height: i32,
    color: Color,
}

#[allow(clippy::type_complexity)]
struct AVFrameIter {
    frame_buffer: AVFrame,
    format_context: AVFormatContextInput,
    decode_context: AVCodecContext,
    stream_index: usize,
    sws_context: Option<SwsContext>,
    crop_frame: Option<Box<dyn FnMut(&mut AVFrame) -> i32>>,
    color: Color,
}

unsafe fn frame_set_color(frame: &mut AVFrame, color: Color) {
    if frame.format != ffi::AVPixelFormat_AV_PIX_FMT_BGRA {
        return;
    }
    let len = frame.width * frame.height * 4;
    let data = unsafe { slice::from_raw_parts_mut(frame.data_mut()[0], len as _) };
    match color {
        Color::Rgb(x) => set_color(data, [x[2], x[1], x[0]]),
        Color::Trans => trans_flag(data, frame.width as _, frame.height as _, false),
    }
}

trait FrameDataIter {
    fn next_frame(&mut self) -> Result<Option<&mut AVFrame>, Error>;
    fn time_base(&self) -> AVRational;
    fn framerate(&self) -> AVRational;
}

impl FrameDataIter for SurfaceIter {
    fn next_frame(&mut self) -> Result<Option<&mut AVFrame>, Error> {
        if self.frame_index >= self.totalframe {
            return Ok(None);
        }
        self.animation.render(self.frame_index, &mut self.surface);

        self.frame_buffer.make_writable()?;
        unsafe {
            self.frame_buffer.fill_arrays(
                self.surface.data_as_bytes().as_ptr(),
                ffi::AVPixelFormat_AV_PIX_FMT_BGRA,
                self.width,
                self.height,
            )?;

            frame_set_color(&mut self.frame_buffer, self.color);
        };

        self.frame_buffer.set_pts(self.frame_index as _);
        self.frame_index += 1;

        Ok(Some(&mut self.frame_buffer))
    }

    fn time_base(&self) -> AVRational {
        av_inv_q(self.framerate())
    }

    fn framerate(&self) -> AVRational {
        av_d2q(self.animation.framerate(), 60)
    }
}

impl FrameDataIter for AVFrameIter {
    fn next_frame(&mut self) -> Result<Option<&mut AVFrame>, Error> {
        loop {
            let packet = loop {
                match self.format_context.read_packet()? {
                    Some(x) if x.stream_index != self.stream_index as i32 => {}
                    x => break x,
                }
            };

            match self.decode_context.send_packet(packet.as_ref()) {
                Ok(_) | Err(RsmpegError::DecoderFlushedError) => {}
                Err(e) => return Err(e.into()),
            };

            match self.decode_context.receive_frame() {
                Ok(mut frame) => {
                    let time_base = self.time_base();
                    if (frame.pts as i32) * time_base.num > 10 * time_base.den {
                        return Ok(None);
                    }
                    if frame.pts == self.frame_buffer.pts {
                        continue;
                    }

                    if self.sws_context.is_none()
                        && (frame.width != frame.height
                            || frame.format == ffi::AVPixelFormat_AV_PIX_FMT_YUVA420P
                            || min(frame.width, frame.height) % 2 != 0)
                    {
                        let dst_fromat = if frame.format == ffi::AVPixelFormat_AV_PIX_FMT_YUVA420P {
                            ffi::AVPixelFormat_AV_PIX_FMT_BGRA
                        } else {
                            ffi::AVPixelFormat_AV_PIX_FMT_YUV420P
                        };

                        let diff = frame.width - frame.height;
                        let (x, y, length) = if diff > 0 {
                            (diff / 2, 0, frame.height)
                        } else {
                            (0, -diff / 2, frame.width)
                        };

                        let dst_length = length + length % 2;
                        let sws_context = SwsContext::get_context(
                            length,
                            length,
                            frame.format,
                            dst_length,
                            dst_length,
                            dst_fromat,
                            0,
                        )
                        .ok_or("Failed to get sws_context")?;
                        self.sws_context = Some(sws_context);

                        if frame.width != frame.height {
                            let crop_frame = move |frame: &mut AVFrame| {
                                if frame.width == frame.height {
                                    return 0;
                                }

                                let frame: &mut ffi::AVFrame = unsafe { &mut *frame.as_mut_ptr() };
                                frame.crop_left = x as _;
                                frame.crop_right = (frame.width - x - length) as _;
                                frame.crop_top = y as _;
                                frame.crop_bottom = (frame.height - y - length) as _;

                                unsafe {
                                    ffi::av_frame_apply_cropping(
                                        frame,
                                        ffi::AV_FRAME_CROP_UNALIGNED as _,
                                    )
                                }
                            };
                            self.crop_frame = Some(Box::new(crop_frame));
                        }

                        self.frame_buffer.set_format(dst_fromat);
                        self.frame_buffer.set_width(dst_length);
                        self.frame_buffer.set_height(dst_length);
                        self.frame_buffer.alloc_buffer()?;
                    }

                    if let Some(sws_ctx) = &mut self.sws_context {
                        if let Some(crop_frame) = self.crop_frame.as_mut() {
                            if crop_frame(&mut frame) != 0 || frame.width != frame.height {
                                return Err("Failed to crop frame".into());
                            }
                        };
                        self.frame_buffer.make_writable()?;
                        sws_ctx.scale_frame(&frame, 0, frame.height, &mut self.frame_buffer)?;
                        if self.frame_buffer.format == ffi::AVPixelFormat_AV_PIX_FMT_BGRA {
                            unsafe { frame_set_color(&mut self.frame_buffer, self.color) };
                        }
                        self.frame_buffer.set_pts(frame.pts);
                    } else {
                        self.frame_buffer = frame;
                    }

                    break Ok(Some(&mut self.frame_buffer));
                }
                Err(RsmpegError::DecoderDrainError) => {}
                Err(RsmpegError::DecoderFlushedError) => break Ok(None),
                Err(e) => break Err(e.into()),
            }
        }
    }

    fn time_base(&self) -> AVRational {
        self.decode_context.time_base
    }

    fn framerate(&self) -> AVRational {
        self.decode_context.framerate
    }
}

fn read_animation(data: &[u8], cache_key: &str) -> Result<Animation, Error> {
    let mut json_data = Vec::new();
    GzDecoder::new(&mut json_data).write_all(data)?;
    let animation =
        Animation::from_data(json_data, cache_key, "/nonexistent").ok_or("Invalid lottie data")?;

    Ok(animation)
}

fn decode_lottie(animation: Animation, color: Color) -> Result<SurfaceIter, Error> {
    let size = animation.size();
    let totalframe = animation.totalframe();
    let mut frame_buffer = AVFrame::new();
    frame_buffer.set_format(ffi::AVPixelFormat_AV_PIX_FMT_BGRA);
    frame_buffer.set_width(size.width as _);
    frame_buffer.set_height(size.height as _);
    frame_buffer.alloc_buffer()?;
    Ok(SurfaceIter {
        surface: Surface::new(size),
        animation,
        frame_buffer,
        frame_index: 0,
        totalframe,
        width: size.width as _,
        height: size.height as _,
        color,
    })
}

fn decode_video(
    input_format_context: AVFormatContextInput,
    color: Color,
) -> Result<AVFrameIter, Error> {
    let (stream_index, decode_context) = {
        let (stream_index, mut decoder) = input_format_context
            .find_best_stream(ffi::AVMediaType_AVMEDIA_TYPE_VIDEO)?
            .ok_or("Failed to find the best stream")?;
        let stream = input_format_context.streams().get(stream_index).unwrap();

        if decoder.name() == c"vp9" {
            decoder = match AVCodec::find_decoder_by_name(c"libvpx-vp9") {
                Some(x) => x,
                None => {
                    println!("the decoder is not found: libvpx-vp9");
                    decoder
                }
            };
        }

        let mut decode_context = AVCodecContext::new(&decoder);
        decode_context.apply_codecpar(&stream.codecpar())?;
        decode_context.open(None)?;
        decode_context.set_framerate(stream.avg_frame_rate);
        decode_context.set_time_base(stream.time_base);

        (stream_index, decode_context)
    };

    let mut frame_buffer = AVFrame::new();
    frame_buffer.set_pts(-1);

    let ret = AVFrameIter {
        frame_buffer,
        format_context: input_format_context,
        decode_context,
        stream_index,
        sws_context: None,
        crop_frame: None,
        color,
    };

    Ok(ret)
}

#[allow(clippy::type_complexity)]
fn io_context_custom(
    data: Vec<u8>,
    write: bool,
) -> Result<(AVIOContextCustom, Arc<Mutex<Cursor<Vec<u8>>>>), Error> {
    let data = Arc::new(Mutex::new(Cursor::new(data)));

    let seek = {
        let data = data.clone();
        Box::new(move |_: &mut Vec<u8>, offset: i64, whence: i32| {
            let mut data = data.lock().unwrap();
            const AVSEEK_SIZE: i32 = ffi::AVSEEK_SIZE as i32;
            match whence {
                0 => data.seek(SeekFrom::Start(offset as _)),
                1 => data.seek(SeekFrom::Current(offset)),
                2 => data.seek(SeekFrom::End(offset)),
                AVSEEK_SIZE => return data.get_ref().len() as _,
                _ => return -1,
            }
            .map(|x| x as _)
            .unwrap_or(-1)
        })
    };

    let io_context = if write {
        let write_packet = {
            let data = data.clone();
            Box::new(
                move |_: &mut Vec<u8>, buf: &[u8]| match data.lock().unwrap().write_all(buf) {
                    Ok(_) => buf.len() as _,
                    Err(_) => -1,
                },
            )
        };

        AVIOContextCustom::alloc_context(
            AVMem::new(4096),
            true,
            Vec::new(),
            None,
            Some(write_packet),
            Some(seek),
        )
    } else {
        let read_packet = {
            let data = data.clone();
            Box::new(move |_: &mut Vec<u8>, buf: &mut [u8]| {
                let mut data = data.lock().unwrap();
                match data.read(buf) {
                    Ok(n) if n == 0 => ffi::AVERROR_EOF,
                    Ok(n) => n as _,
                    Err(_) => -1,
                }
            })
        };

        AVIOContextCustom::alloc_context(
            AVMem::new(4096),
            false,
            Vec::new(),
            Some(read_packet),
            None,
            Some(seek),
        )
    };

    Ok((io_context, data))
}

fn input_format_context(data: Vec<u8>) -> Result<AVFormatContextInput, Error> {
    let (io_context, _) = io_context_custom(data, false)?;
    let input_format_context =
        AVFormatContextInput::from_io_context(AVIOContextContainer::Custom(io_context))?;

    Ok(input_format_context)
}

#[allow(clippy::type_complexity)]
fn output_format_context() -> Result<(AVFormatContextOutput, Arc<Mutex<Cursor<Vec<u8>>>>), Error> {
    let (io_context, data) = io_context_custom(Vec::new(), true)?;
    let output_format_context =
        AVFormatContextOutput::create(c".mp4", Some(AVIOContextContainer::Custom(io_context)))?;

    Ok((output_format_context, data))
}

fn encode_mp4<S: FrameDataIter>(mut src: S) -> Result<Vec<u8>, Error> {
    let buffer = {
        let time_base = src.time_base();
        let framerate = src.framerate();
        let first_frame = src.next_frame()?.ok_or("Failed to get first frame")?;
        let width = first_frame.width;
        let height = first_frame.height;

        let (mut output_format_context, buffer) = output_format_context()?;

        let encoder =
            AVCodec::find_encoder_by_name(c"libx264").ok_or("Failed to find encoder codec")?;
        let mut encode_context = AVCodecContext::new(&encoder);
        encode_context.set_width(width);
        encode_context.set_height(height);
        encode_context.set_time_base(time_base);
        encode_context.set_framerate(framerate);
        encode_context.set_pix_fmt(ffi::AVPixelFormat_AV_PIX_FMT_YUV420P);
        unsafe {
            if ffi::av_opt_set(
                encode_context.priv_data,
                c"preset".as_ptr(),
                c"slow".as_ptr(),
                0,
            ) < 0
            {
                return Err("Failed to set preset".into());
            }
        }
        if output_format_context.oformat().flags & ffi::AVFMT_GLOBALHEADER as i32 != 0 {
            encode_context
                .set_flags(encode_context.flags | ffi::AV_CODEC_FLAG_GLOBAL_HEADER as i32);
        }
        encode_context.open(None)?;

        let mut dst_frame = AVFrame::new();
        dst_frame.set_format(encode_context.pix_fmt);
        dst_frame.set_width(encode_context.width);
        dst_frame.set_height(encode_context.height);
        dst_frame.alloc_buffer()?;

        {
            let mut out_stream = output_format_context.new_stream();
            out_stream.set_codecpar(encode_context.extract_codecpar());
        }

        output_format_context.write_header(&mut None)?;

        let mut sws_context = SwsContext::get_context(
            width,
            height,
            first_frame.format,
            width,
            height,
            encode_context.pix_fmt,
            ffi::SWS_FAST_BILINEAR | ffi::SWS_ACCURATE_RND,
        )
        .ok_or("Failed to get sws_context")?;
        let mut encode_frame = |src_frame: &mut AVFrame| -> Result<(), Error> {
            let frame_after = if src_frame.format == dst_frame.format {
                src_frame
            } else {
                sws_context.scale_frame(src_frame, 0, height, &mut dst_frame)?;
                dst_frame.set_pts(src_frame.pts);
                &mut dst_frame
            };

            encode_write_frame(
                Some(frame_after),
                &mut encode_context,
                &mut output_format_context,
                0,
            )
        };
        encode_frame(first_frame)?;
        while let Some(src_frame) = src.next_frame()? {
            encode_frame(src_frame)?;
        }

        encode_write_frame(None, &mut encode_context, &mut output_format_context, 0)?;
        output_format_context.write_trailer()?;

        buffer
    };

    let ret = Arc::into_inner(buffer)
        .ok_or("Failed to get encoding output")?
        .into_inner()?
        .into_inner();

    Ok(ret)
}

fn encode_write_frame(
    frame_after: Option<&AVFrame>,
    encode_context: &mut AVCodecContext,
    output_format_context: &mut AVFormatContextOutput,
    out_stream_index: usize,
) -> Result<(), Error> {
    encode_context.send_frame(frame_after)?;

    loop {
        let mut packet = match encode_context.receive_packet() {
            Ok(packet) => packet,
            Err(RsmpegError::EncoderDrainError) | Err(RsmpegError::EncoderFlushedError) => break,
            Err(e) => return Err(e.into()),
        };

        packet.set_stream_index(out_stream_index as i32);
        packet.rescale_ts(
            encode_context.time_base,
            output_format_context
                .streams()
                .get(out_stream_index)
                .ok_or("Failed to get stream")?
                .time_base,
        );

        match output_format_context.interleaved_write_frame(&mut packet) {
            Ok(()) => Ok(()),
            Err(RsmpegError::InterleavedWriteFrameError(-22)) => Ok(()),
            Err(e) => Err(e),
        }?;
    }

    Ok(())
}

pub fn tgs_to_mp4(data: Vec<u8>, cache_key: &str, color: Color) -> Result<Vec<u8>, Error> {
    let animation = read_animation(&data, cache_key)?;
    let surface_iter = decode_lottie(animation, color)?;

    encode_mp4(surface_iter)
}

pub fn video_to_mp4(data: Vec<u8>, color: Color) -> Result<Vec<u8>, Error> {
    let format_context = input_format_context(data)?;
    let frame_iter = decode_video(format_context, color)?;

    encode_mp4(frame_iter)
}
