use std::{
    ffi::{CStr, CString},
    io::{Cursor, Seek, SeekFrom, Write},
    sync::{Arc, Mutex},
};

use flate2::write::GzDecoder;
use rlottie::{Animation, Size, Surface};
use rsmpeg::{
    avcodec::{AVCodec, AVCodecContext},
    avformat::{AVFormatContextOutput, AVIOContextContainer, AVIOContextCustom},
    avutil::{ra, AVFrame, AVMem},
    error::RsmpegError,
    ffi,
    swscale::SwsContext,
};

use crate::error::Error;

trait FrameData {
    fn data(&self) -> &[u8];
}

impl FrameData for Surface {
    fn data(&self) -> &[u8] {
        self.data_as_bytes()
    }
}

struct SurfaceIter {
    surface: Surface,
    animation: Animation,
    frame_index: usize,
    totalframe: usize,
}

impl From<Animation> for SurfaceIter {
    fn from(animation: Animation) -> Self {
        let totalframe = animation.totalframe();
        Self {
            surface: Surface::new(animation.size()),
            animation,
            frame_index: 0,
            totalframe,
        }
    }
}

trait FrameDataIter {
    type Item<'a>
    where
        Self: 'a;

    fn next_frame<'a>(self: &'a mut Self) -> Option<Self::Item<'a>>;
    fn format(&self) -> i32;
    fn size(&self) -> (i32, i32);
    fn framerate(&self) -> i32;
}

impl FrameDataIter for SurfaceIter {
    type Item<'a> = &'a Surface;

    fn next_frame<'a>(self: &'a mut Self) -> Option<Self::Item<'a>> {
        if self.frame_index >= self.totalframe {
            return None;
        }
        self.animation.render(self.frame_index, &mut self.surface);
        self.frame_index += 1;
        Some(&self.surface)
    }

    fn format(&self) -> i32 {
        ffi::AVPixelFormat_AV_PIX_FMT_BGRA
    }

    fn size(&self) -> (i32, i32) {
        let Size { width, height } = self.animation.size();
        (width as _, height as _)
    }

    fn framerate(&self) -> i32 {
        self.animation.framerate() as _
    }
}

fn read_animation(data: &[u8], cache_key: &str) -> Result<Animation, Error> {
    let mut json_data = Vec::new();
    GzDecoder::new(&mut json_data).write_all(data)?;
    let animation =
        Animation::from_data(json_data, cache_key, "/nonexistent").ok_or("Invalid lottie data")?;

    Ok(animation)
}

pub fn tgs_to_mp4(data: Vec<u8>, cache_key: &str) -> Result<Vec<u8>, Error> {
    let animation = read_animation(&data, cache_key)?;
    let surface_iter = SurfaceIter::from(animation);

    encode_mp4(surface_iter)
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

fn encode_mp4<F, S>(mut src: S) -> Result<Vec<u8>, Error>
where
    F: FrameData,
    for<'a> S: FrameDataIter<Item<'a> = &'a F>,
{
    let buffer = {
        let framerate = src.framerate();
        let (width, height) = src.size();

        let codec_name = &CString::new("libx264").unwrap();

        let (mut output_format_context, buffer) = output_format_context()?;

        let encoder =
            AVCodec::find_encoder_by_name(codec_name).ok_or("Failed to find encoder codec")?;
        let mut encode_context = AVCodecContext::new(&encoder);
        encode_context.set_bit_rate(400000);
        encode_context.set_width(width);
        encode_context.set_height(height);
        encode_context.set_time_base(ra(1, framerate));
        encode_context.set_framerate(ra(framerate, 1));
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

        let mut frame = AVFrame::new();
        frame.set_format(encode_context.pix_fmt);
        frame.set_width(encode_context.width);
        frame.set_height(encode_context.height);
        frame.alloc_buffer()?;

        let mut frame2 = AVFrame::new();
        frame2.set_format(src.format());
        frame2.set_width(encode_context.width);
        frame2.set_height(encode_context.height);
        frame2.alloc_buffer()?;

        let mut sws_context = SwsContext::get_context(
            width,
            height,
            frame2.format,
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
        while let Some(f) = src.next_frame() {
            frame2.make_writable()?;
            unsafe { frame2.fill_arrays(f.data().as_ptr(), src.format(), width, height)? };

            sws_context.scale_frame(&frame2, 0, height, &mut frame)?;

            frame.set_pts(pts);
            pts += 1;
            encode_write_frame(
                Some(&frame),
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
