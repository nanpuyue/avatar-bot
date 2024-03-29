use std::io::Read;
use std::slice;
use std::sync::atomic::Ordering::Relaxed;
use std::{
    ffi::{CStr, CString},
    io::{Cursor, Seek, SeekFrom, Write},
    sync::{atomic::AtomicUsize, Arc, Mutex},
};

use flate2::write::GzDecoder;
use rlottie::{Animation, Size, Surface};
use rsmpeg::avutil::{av_d2q, av_inv_q};
use rsmpeg::{
    avcodec::{AVCodec, AVCodecContext},
    avformat::{
        AVFormatContextInput, AVFormatContextOutput, AVIOContextContainer, AVIOContextCustom,
    },
    avutil::{AVFrame, AVMem, AVRational},
    error::RsmpegError,
    ffi,
    swscale::SwsContext,
};

use crate::error::Error;
use crate::image::alpha_composite;

struct SurfaceIter {
    surface: Surface,
    animation: Animation,
    frame_buffer: AVFrame,
    frame_index: usize,
    totalframe: usize,
    width: i32,
    height: i32,
}

struct AVFrameIter {
    frame_buffer: AVFrame,
    format_context: AVFormatContextInput,
    decode_context: AVCodecContext,
    stream_index: usize,
}

impl TryFrom<Animation> for SurfaceIter {
    type Error = Error;

    fn try_from(animation: Animation) -> Result<SurfaceIter, Self::Error> {
        let size = animation.size();
        let totalframe = animation.totalframe();
        let mut frame_buffer = AVFrame::new();
        frame_buffer.set_format(ffi::AVPixelFormat_AV_PIX_FMT_BGRA);
        frame_buffer.set_width(size.width as _);
        frame_buffer.set_height(size.height as _);
        frame_buffer.alloc_buffer()?;
        Ok(Self {
            surface: Surface::new(size),
            animation,
            frame_buffer,
            frame_index: 0,
            totalframe,
            width: size.width as _,
            height: size.height as _,
        })
    }
}

trait FrameDataIter {
    fn next_frame(&mut self) -> Result<Option<&mut AVFrame>, Error>;
    fn format(&self) -> i32;
    fn size(&self) -> (i32, i32);
    fn framerate(&self) -> AVRational;
}

impl FrameDataIter for SurfaceIter {
    fn next_frame(&mut self) -> Result<Option<&mut AVFrame>, Error> {
        if self.frame_index >= self.totalframe {
            return Ok(None);
        }
        self.animation.render(self.frame_index, &mut self.surface);
        self.frame_index += 1;

        self.frame_buffer.make_writable()?;
        unsafe {
            self.frame_buffer.fill_arrays(
                self.surface.data_as_bytes().as_ptr(),
                self.format(),
                self.width,
                self.height,
            )?;

            let ptr = self.frame_buffer.data_mut()[0] as *mut [u8; 4];
            let length = self.surface.data().len();
            let data = slice::from_raw_parts_mut(ptr, length);
            for i in data {
                // BGRA
                alpha_composite(i, [0xff, 0xff, 0xff]);
            }
        };

        Ok(Some(&mut self.frame_buffer))
    }

    fn format(&self) -> i32 {
        ffi::AVPixelFormat_AV_PIX_FMT_BGRA
    }

    fn size(&self) -> (i32, i32) {
        let Size { width, height } = self.animation.size();
        (width as _, height as _)
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

            self.decode_context.send_packet(packet.as_ref())?;
            match self.decode_context.receive_frame() {
                Ok(x) => {
                    self.frame_buffer = x;
                    break Ok(Some(&mut self.frame_buffer));
                }
                Err(RsmpegError::DecoderDrainError) => {}
                Err(RsmpegError::DecoderFlushedError) => break Ok(None),
                Err(e) => break Err(e.into()),
            }
        }
    }

    fn format(&self) -> i32 {
        self.decode_context.pix_fmt
    }

    fn size(&self) -> (i32, i32) {
        (self.decode_context.width, self.decode_context.height)
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

fn decode_video(input_format_context: AVFormatContextInput) -> Result<AVFrameIter, Error> {
    let (stream_index, decode_context) = {
        let (stream_index, mut decoder) = input_format_context
            .find_best_stream(ffi::AVMediaType_AVMEDIA_TYPE_VIDEO)?
            .ok_or("Failed to find the best stream")?;
        let stream = input_format_context.streams().get(stream_index).unwrap();

        if decoder.name().to_str() == Ok("vp9") {
            decoder = AVCodec::find_decoder_by_name(unsafe {
                CStr::from_bytes_with_nul_unchecked(b"libvpx-vp9\0")
            })
            .unwrap_or_else(|| {
                eprintln!("the decoder is not found: libvpx-vp9");
                decoder
            });
        }

        let mut decode_context = AVCodecContext::new(&decoder);
        decode_context.apply_codecpar(&stream.codecpar())?;
        decode_context.open(None)?;
        decode_context.set_framerate(stream.r_frame_rate);
        decode_context.set_time_base(stream.time_base);

        (stream_index, decode_context)
    };

    let ret = AVFrameIter {
        frame_buffer: AVFrame::new(),
        format_context: input_format_context,
        decode_context,
        stream_index,
    };

    Ok(ret)
}

fn input_format_context(data: Vec<u8>) -> Result<AVFormatContextInput, Error> {
    let cur1 = Arc::new(AtomicUsize::new(0));
    let cur2 = cur1.clone();

    let io_context = AVIOContextCustom::alloc_context(
        AVMem::new(4096),
        false,
        data,
        Some(Box::new(move |data, buf| {
            let cur = cur1.load(Relaxed);
            if data.len() <= cur {
                return ffi::AVERROR_EOF;
            }
            let ret = (&data[cur..]).read(buf).unwrap();
            cur1.store(cur + ret, Relaxed);
            ret as i32
        })),
        None,
        Some(Box::new(move |data, offset, whence| {
            let cur = cur2.load(Relaxed) as i64;
            const AVSEEK_SIZE: i32 = ffi::AVSEEK_SIZE as i32;
            let new = match whence {
                0 => offset,
                1 => cur + offset,
                2 => data.len() as i64 + offset,
                AVSEEK_SIZE => return data.len() as i64,
                _ => -1,
            };

            if new >= 0 {
                cur2.store(new as usize, Relaxed);
            }
            new
        })),
    );

    let input_format_context =
        AVFormatContextInput::from_io_context(AVIOContextContainer::Custom(io_context))?;

    Ok(input_format_context)
}

fn output_format_context() -> Result<(AVFormatContextOutput, Arc<Mutex<Cursor<Vec<u8>>>>), Error> {
    let buffer = Arc::new(Mutex::new(Cursor::new(Vec::new())));
    let buffer1 = buffer.clone();
    let buffer2 = buffer.clone();

    // Custom IO Context
    let io_context = AVIOContextCustom::alloc_context(
        AVMem::new(4096),
        true,
        Vec::new(),
        None,
        Some(Box::new(move |_, buf: &[u8]| {
            let mut buffer = buffer1.lock().unwrap();
            if buffer.write_all(buf).is_err() {
                return -1;
            };
            buf.len() as _
        })),
        Some(Box::new(move |_, offset: i64, whence: i32| {
            let mut buffer = match buffer2.lock() {
                Ok(x) => x,
                Err(_) => return -1,
            };
            match whence {
                0 => buffer.seek(SeekFrom::Start(offset as _)),
                1 => buffer.seek(SeekFrom::Current(offset)),
                2 => buffer.seek(SeekFrom::End(offset)),
                _ => return -1,
            }
            .map(|x| x as _)
            .unwrap_or(-1)
        })),
    );

    let output_format_context = AVFormatContextOutput::create(
        CStr::from_bytes_with_nul(b".mp4\0").unwrap(),
        Some(AVIOContextContainer::Custom(io_context)),
    )?;

    Ok((output_format_context, buffer))
}

fn encode_mp4<S: FrameDataIter>(mut src: S) -> Result<Vec<u8>, Error> {
    let buffer = {
        let framerate = src.framerate();
        let (width, height) = src.size();

        let codec_name = &CString::new("libx264").unwrap();

        let (mut output_format_context, buffer) = output_format_context()?;

        let encoder =
            AVCodec::find_encoder_by_name(codec_name).ok_or("Failed to find encoder codec")?;
        let mut encode_context = AVCodecContext::new(&encoder);
        encode_context.set_bit_rate(1000000);
        encode_context.set_width(width);
        encode_context.set_height(height);
        encode_context.set_time_base(av_inv_q(framerate));
        encode_context.set_framerate(framerate);
        encode_context.set_gop_size(10);
        encode_context.set_max_b_frames(1);
        encode_context.set_pix_fmt(ffi::AVPixelFormat_AV_PIX_FMT_YUV420P);
        let name = CString::new("preset").unwrap();
        let val = CString::new("slow").unwrap();
        if encoder.id == ffi::AVCodecID_AV_CODEC_ID_H264 {
            unsafe {
                if ffi::av_opt_set(encode_context.priv_data, name.as_ptr(), val.as_ptr(), 0) < 0 {
                    return Err("Failed to set preset".into());
                }
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

        let mut sws_context = SwsContext::get_context(
            width,
            height,
            src.format(),
            width,
            height,
            encode_context.pix_fmt,
            ffi::SWS_FAST_BILINEAR | ffi::SWS_ACCURATE_RND,
        )
        .ok_or("Failed to get SwsContext")?;

        {
            let mut out_stream = output_format_context.new_stream();
            out_stream.set_codecpar(encode_context.extract_codecpar());
            out_stream.set_time_base(encode_context.time_base);
        }

        output_format_context.dump(0, unsafe {
            CStr::from_bytes_with_nul_unchecked(b"file.mp4\0")
        })?;
        output_format_context.write_header(&mut None)?;

        let mut pts = 0;
        while let Some(src_frame) = src.next_frame()? {
            let frame_after = if src_frame.format == dst_frame.format {
                src_frame
            } else {
                sws_context.scale_frame(src_frame, 0, height, &mut dst_frame)?;
                &mut dst_frame
            };

            frame_after.set_pts(pts);
            pts += 1;
            encode_write_frame(
                Some(frame_after),
                &mut encode_context,
                &mut output_format_context,
                0,
            )?;
        }

        flush_encoder(&mut encode_context, &mut output_format_context, 0)?;

        output_format_context.write_trailer()?;

        buffer
    };

    let ret = Arc::into_inner(buffer)
        .ok_or("Failed to get buffer")?
        .into_inner()?
        .into_inner();

    Ok(ret)
}

/// encode -> write_frame
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

/// Send an empty packet to the `encode_context` for packet flushing.
fn flush_encoder(
    encode_context: &mut AVCodecContext,
    output_format_context: &mut AVFormatContextOutput,
    out_stream_index: usize,
) -> Result<(), Error> {
    if encode_context.codec().capabilities & ffi::AV_CODEC_CAP_DELAY as i32 == 0 {
        return Ok(());
    }
    encode_write_frame(
        None,
        encode_context,
        output_format_context,
        out_stream_index,
    )
}

pub fn tgs_to_mp4(data: Vec<u8>, cache_key: &str) -> Result<Vec<u8>, Error> {
    let animation = read_animation(&data, cache_key)?;
    let surface_iter = SurfaceIter::try_from(animation)?;

    encode_mp4(surface_iter)
}

pub fn video_to_mp4(data: Vec<u8>) -> Result<Vec<u8>, Error> {
    let format_context = input_format_context(data)?;
    let frame_iter = decode_video(format_context)?;

    encode_mp4(frame_iter)
}
