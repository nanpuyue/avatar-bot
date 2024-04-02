use std::io::Read;
use std::slice;
use std::sync::atomic::AtomicUsize;
use std::sync::atomic::Ordering::Relaxed;
use std::sync::Arc;

use rsmpeg::avcodec::{AVCodec, AVCodecContext};
use rsmpeg::avformat::{AVFormatContextInput, AVIOContextContainer, AVIOContextCustom};
use rsmpeg::avutil::{AVFrameWithImage, AVImage, AVMem};
use rsmpeg::error::RsmpegError;
use rsmpeg::ffi;
use rsmpeg::swscale::SwsContext;

use crate::error::Error;

pub fn video_to_png(data: Vec<u8>) -> Result<Vec<u8>, Error> {
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

    let mut input_format_context =
        AVFormatContextInput::from_io_context(AVIOContextContainer::Custom(io_context))?;

    let (video_stream_index, mut decode_context) = {
        let (stream_index, mut decoder) = input_format_context
            .find_best_stream(ffi::AVMediaType_AVMEDIA_TYPE_VIDEO)?
            .ok_or("Failed to find the best stream")?;
        let stream = input_format_context.streams().get(stream_index).unwrap();

        if decoder.name() == c"vp9" {
            decoder = AVCodec::find_decoder_by_name(c"libvpx-vp9").unwrap_or_else(|| {
                eprintln!("the decoder is not found: libvpx-vp9");
                decoder
            });
        }

        let mut decode_context = AVCodecContext::new(&decoder);
        decode_context.apply_codecpar(&stream.codecpar())?;
        decode_context.open(None)?;

        (stream_index, decode_context)
    };

    let cover_frame = loop {
        let cover_packet = loop {
            match input_format_context.read_packet()? {
                Some(x) if x.stream_index != video_stream_index as i32 => {}
                x => break x,
            }
        };

        decode_context.send_packet(cover_packet.as_ref())?;
        match decode_context.receive_frame() {
            Ok(x) => break x,
            Err(RsmpegError::DecoderDrainError) => {}
            Err(e) => return Err(e.into()),
        }

        if cover_packet.is_none() {
            return Err("Can't find video cover frame".into());
        }
    };

    let mut encode_context = {
        let encoder =
            AVCodec::find_encoder(ffi::AVCodecID_AV_CODEC_ID_PNG).ok_or("Encoder not found")?;
        let mut encode_context = AVCodecContext::new(&encoder);

        encode_context.set_bit_rate(decode_context.bit_rate);
        encode_context.set_width(decode_context.width);
        encode_context.set_height(decode_context.height);
        encode_context.set_time_base(ffi::AVRational { num: 1, den: 1 });
        encode_context.set_pix_fmt(ffi::AVPixelFormat_AV_PIX_FMT_RGBA);
        encode_context.open(None)?;

        encode_context
    };

    let scaled_cover_packet = {
        let mut sws_context = SwsContext::get_context(
            decode_context.width,
            decode_context.height,
            decode_context.pix_fmt,
            encode_context.width,
            encode_context.height,
            encode_context.pix_fmt,
            ffi::SWS_FAST_BILINEAR | ffi::SWS_ACCURATE_RND,
        )
        .ok_or("Invalid sws_context parameter")?;
        let image_buffer = AVImage::new(
            encode_context.pix_fmt,
            encode_context.width,
            encode_context.height,
            1,
        )
        .ok_or("Invalid image_buffer parameter")?;

        let mut scaled_cover_frame = AVFrameWithImage::new(image_buffer);

        sws_context.scale_frame(
            &cover_frame,
            0,
            decode_context.height,
            &mut scaled_cover_frame,
        )?;

        encode_context.send_frame(Some(&scaled_cover_frame))?;
        encode_context.receive_packet()?
    };

    let data = unsafe {
        slice::from_raw_parts(scaled_cover_packet.data, scaled_cover_packet.size as usize)
    };

    Ok(data.into())
}
