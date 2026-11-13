//! The H.264 and HEVC decoders of the video module: our pure-Rust decoders with `patent-codecs`, and the
//! WebCodecs-only fallbacks without it. `video` re-exports them.

#[cfg(feature = "patent-codecs")]
use super::*;

/// Worker threads of the H.264 decoders made for background work (thumbnails): a frame-threaded
/// 4K decoder holds `threads + 2` pictures in flight (~0.5 GB with 16 threads), and the frame
/// workers already decode different files in parallel.
#[cfg(feature = "patent-codecs")]
const BACKGROUND_H264_THREADS: usize = 2;

/// Our pure-Rust H.264 decoder (frame-threaded).
#[cfg(feature = "patent-codecs")]
pub struct H264Decoder {
    avcc: Vec<u8>,
    dec: craft_video::h264::decode::Decoder,
    length_size: usize,
    draft: bool,
    /// Worker threads (a background decoder gets fewer).
    threads: usize,
}

#[cfg(feature = "patent-codecs")]
impl H264Decoder {
    /// The worker threads for a decoder made now.
    fn threads_now() -> usize {
        if filmcraft_media::cancel::background() { BACKGROUND_H264_THREADS.min(h264_threads()) } else { h264_threads() }
    }

    fn make_inner(avcc: &[u8], threads: usize) -> Result<craft_video::h264::decode::Decoder> {
        let mut dec = craft_video::h264::decode::Decoder::with_threads(threads);
        if !avcc.is_empty() {
            dec.configure_avcc(avcc).map_err(h264_error)?;
        }
        Ok(dec)
    }

    pub fn new(avcc: Vec<u8>) -> Result<Self> {
        recycle_h264_planes();
        let threads = Self::threads_now();
        let dec = Self::make_inner(&avcc, threads)?;
        let length_size = avcc_length_size(&avcc);
        Ok(Self { avcc, dec, length_size, draft: false, threads })
    }
    /// A decoder for Annex B byte-stream samples (start codes, in-band parameter sets: MXF, TS).
    pub fn annexb() -> Self {
        recycle_h264_planes();
        let threads = Self::threads_now();
        Self { avcc: Vec::new(), dec: craft_video::h264::decode::Decoder::with_threads(threads), length_size: 0, draft: false, threads }
    }
    fn convert(p: craft_video::h264::decode::Picture) -> DecodedFrame {
        use std::sync::Arc;
        let draft = p.draft;
        let (w, h) = (p.width as usize, p.height as usize);
        let (cw, ch) = (p.chroma_width as usize, p.chroma_height as usize);
        let y = tight_plane(p.y, p.y_stride, w, h);
        let u = tight_plane(p.u, p.uv_stride, cw, ch);
        let v = tight_plane(p.v, p.uv_stride, cw, ch);
        let color = vui_color(p.width, p.height, p.color.primaries, p.color.matrix, p.color.transfer, p.color.full_range);
        let par = sar_par(p.sar);
        let frame = VideoFrame {
            width: p.width,
            height: p.height,
            data: filmcraft_frame::PixelData::Yuv8 { planes: [Arc::new(y), Arc::new(u), Arc::new(v)], chroma: filmcraft_frame::Chroma::C420, alpha: None },
            color,
            par,
            pts: filmcraft_time::Tick::ZERO,
        };
        DecodedFrame { pts: p.pts, frame, draft }
    }
}

/// An H.264 decoder error as a [`CodecError`]: a feature the decoder does not implement (High
/// 4:2:2, field pictures...) is [`CodecError::Unsupported`], so the media pool shows the
/// unreadable slate instead of a black picture (#626); anything else is a decode error.
#[cfg(feature = "patent-codecs")]
fn h264_error(e: craft_video::h264::decode::Error) -> CodecError {
    match e {
        craft_video::h264::decode::Error::Unsupported(what) => CodecError::Unsupported(format!("H.264: {what}")),
        e => CodecError::Decode(e.to_string()),
    }
}

#[cfg(feature = "patent-codecs")]
impl VideoDecoder for H264Decoder {
    fn decode(&mut self, sample: &[u8], pts: i64) -> Result<Vec<DecodedFrame>> {
        let pics = self.dec.decode(sample, pts).map_err(h264_error)?;
        Ok(pics.into_iter().map(Self::convert).collect())
    }
    fn flush(&mut self) -> Vec<DecodedFrame> {
        self.dec.flush().into_iter().map(Self::convert).collect()
    }
    fn reset(&mut self) {
        self.dec.reset();
        if !self.avcc.is_empty() {
            let _ = self.dec.configure_avcc(&self.avcc);
        }
    }
    fn name(&self) -> &str {
        "FilmCraft H.264"
    }
    fn thread_limited(&self) -> bool {
        self.threads < h264_threads()
    }
    fn set_draft(&mut self, on: bool) {
        self.draft = on;
        self.dec.set_draft(on);
    }
    fn is_disposable(&self, sample: &[u8]) -> bool {
        if self.length_size == 0 {
            return h264_disposable_annexb(sample);
        }
        h264_disposable(sample, self.length_size)
    }
    fn is_random_access(&self, sample: &[u8]) -> Option<bool> {
        // Annex B samples carry their parameter sets: an IDR access unit is a starting point, and
        // so is a non-IDR one with a recovery point SEI of recovery_frame_cnt 0 (open GOP), the
        // same rule as the transport stream splitter's `key`.
        (self.length_size == 0).then(|| {
            filmcraft_bitstream::annexb_nals(sample).iter().any(|n| match n.first().map(|h| h & 0x1f) {
                Some(5) => true,
                Some(6) => craft_media::mpegts::sei_has_recovery_point(n),
                _ => false,
            })
        })
    }
}

/// The H.264 decoder's output planes come from the frames the caches evicted
/// (`filmcraft_frame::pool`) instead of the allocator.
#[cfg(feature = "patent-codecs")]
fn recycle_h264_planes() {
    craft_video::h264::decode::set_plane_allocator(filmcraft_frame::pool::take_u8);
}

/// Worker threads each H.264 decoder uses (frame threading; 1 on wasm).
#[cfg(feature = "patent-codecs")]
pub fn h264_threads() -> usize {
    craft_video::h264::decode::default_threads()
}

#[cfg(feature = "patent-codecs")]
pub fn h264_factory(e: &SampleEntry) -> Option<Result<Box<dyn VideoDecoder>>> {
    match &e.codec {
        CodecConfig::Avc(a) => Some(H264Decoder::new(a.to_bytes()).map(|d| Box::new(d) as Box<dyn VideoDecoder>)),
        _ => None,
    }
}

/// Our pure-Rust HEVC decoder (Main / Main 10 and 4:2:2, frame-threaded).
#[cfg(feature = "patent-codecs")]
pub struct HevcDecoder {
    hvcc: Vec<u8>,
    dec: craft_video::hevc::decode::Decoder,
    length_size: usize,
    highest_tid: Option<u8>,
}

#[cfg(feature = "patent-codecs")]
impl HevcDecoder {
    pub fn new(hvcc: Vec<u8>) -> Result<Self> {
        let dec = craft_video::hevc::decode::Decoder::from_hvcc(&hvcc).map_err(hevc_error)?;
        let (length_size, highest_tid) = hvcc_length_size_and_tid(&hvcc);
        Ok(Self { hvcc, dec, length_size, highest_tid })
    }
    /// A decoder for Annex B byte-stream samples (start codes, in-band parameter sets: TS).
    pub fn annexb() -> Self {
        Self { hvcc: Vec::new(), dec: craft_video::hevc::decode::Decoder::new(), length_size: 0, highest_tid: None }
    }
    fn convert(p: craft_video::hevc::decode::Picture) -> DecodedFrame {
        use craft_video::hevc::decode::{ChromaFormat, Plane};
        use std::sync::Arc;
        let (w, h) = (p.width as usize, p.height as usize);
        let (cw, ch) = (p.chroma_width as usize, p.chroma_height as usize);
        let (ys, uvs) = (p.y_stride, p.uv_stride);
        let chroma = match p.chroma {
            ChromaFormat::C420 => filmcraft_frame::Chroma::C420,
            ChromaFormat::C422 => filmcraft_frame::Chroma::C422,
        };
        let wide = |pl: Plane| -> Vec<u16> {
            match pl {
                Plane::U16(v) => v,
                Plane::U8(v) => v.into_iter().map(u16::from).collect(),
            }
        };
        let data = match (p.y, p.u, p.v) {
            (Plane::U8(y), Plane::U8(u), Plane::U8(v)) => filmcraft_frame::PixelData::Yuv8 {
                planes: [Arc::new(tight_plane(y, ys, w, h)), Arc::new(tight_plane(u, uvs, cw, ch)), Arc::new(tight_plane(v, uvs, cw, ch))],
                chroma,
                alpha: None,
            },
            (y, u, v) => filmcraft_frame::PixelData::Yuv16 {
                planes: [Arc::new(tight_plane(wide(y), ys, w, h)), Arc::new(tight_plane(wide(u), uvs, cw, ch)), Arc::new(tight_plane(wide(v), uvs, cw, ch))],
                chroma,
                bits: p.bit_depth,
                alpha: None,
            },
        };
        let color = vui_color(p.width, p.height, p.color.primaries, p.color.matrix, p.color.transfer, p.color.full_range);
        let par = sar_par(p.sar);
        let frame = VideoFrame { width: p.width, height: p.height, data, color, par, pts: filmcraft_time::Tick::ZERO };
        DecodedFrame { pts: p.pts, frame, draft: p.draft }
    }
}

/// An HEVC decoder error as a [`CodecError`] (see [`h264_error`]).
#[cfg(feature = "patent-codecs")]
fn hevc_error(e: craft_video::hevc::decode::Error) -> CodecError {
    match e {
        craft_video::hevc::decode::Error::Unsupported(what) => CodecError::Unsupported(format!("HEVC: {what}")),
        e => CodecError::Decode(e.to_string()),
    }
}

#[cfg(feature = "patent-codecs")]
impl VideoDecoder for HevcDecoder {
    fn decode(&mut self, sample: &[u8], pts: i64) -> Result<Vec<DecodedFrame>> {
        let pics = self.dec.decode(sample, pts).map_err(hevc_error)?;
        Ok(pics.into_iter().map(Self::convert).collect())
    }
    fn flush(&mut self) -> Vec<DecodedFrame> {
        self.dec.flush().into_iter().map(Self::convert).collect()
    }
    fn reset(&mut self) {
        if self.hvcc.is_empty() {
            self.dec = craft_video::hevc::decode::Decoder::new();
        } else if let Ok(d) = craft_video::hevc::decode::Decoder::from_hvcc(&self.hvcc) {
            self.dec = d;
        }
    }
    fn name(&self) -> &str {
        "FilmCraft HEVC"
    }
    fn is_disposable(&self, sample: &[u8]) -> bool {
        hevc_disposable(sample, self.length_size, self.highest_tid)
    }
    fn set_draft(&mut self, on: bool) {
        self.dec.set_draft(on);
    }
    fn is_random_access(&self, sample: &[u8]) -> Option<bool> {
        // Annex B samples carry their parameter sets: an IDR / CRA / BLA access unit is a
        // starting point.
        (self.length_size == 0)
            .then(|| filmcraft_bitstream::annexb_nals(sample).iter().any(|n| n.first().is_some_and(|h| (16..=21).contains(&((h >> 1) & 0x3F)))))
    }
}

#[cfg(feature = "patent-codecs")]
pub fn hevc_factory(e: &SampleEntry) -> Option<Result<Box<dyn VideoDecoder>>> {
    match &e.codec {
        CodecConfig::Hevc(c) => Some(HevcDecoder::new(c.to_bytes()).map(|d| Box::new(d) as Box<dyn VideoDecoder>)),
        _ => None,
    }
}

#[cfg(all(test, feature = "patent-codecs"))]
mod hevc_convert_tests {
    use super::HevcDecoder;
    use craft_video::hevc::decode::{ChromaFormat, ColorInfo, Picture, Plane};
    use filmcraft_frame::{Chroma, PixelData};

    /// A 4x2 picture whose chroma planes are 2 samples wide and `chroma_height` rows high.
    fn picture(chroma: ChromaFormat, chroma_height: u32, wide: bool) -> Picture {
        let plane = |n: usize, v: u16| if wide { Plane::U16(vec![v; n]) } else { Plane::U8(vec![v as u8; n]) };
        let uv = 2 * chroma_height as usize;
        Picture {
            width: 4,
            height: 2,
            chroma_width: 2,
            chroma_height,
            chroma,
            bit_depth: if wide { 10 } else { 8 },
            y: plane(8, 64),
            u: plane(uv, 128),
            v: plane(uv, 128),
            y_stride: 4,
            uv_stride: 2,
            pts: 0,
            poc: 0,
            key: true,
            color: ColorInfo { full_range: false, primaries: 1, transfer: 1, matrix: 1 },
            sar: (1, 1),
            draft: false,
        }
    }

    fn chroma_of(data: &PixelData) -> Option<(Chroma, usize)> {
        match data {
            PixelData::Yuv8 { planes, chroma, .. } => Some((*chroma, planes[1].len())),
            PixelData::Yuv16 { planes, chroma, .. } => Some((*chroma, planes[1].len())),
            _ => None,
        }
    }

    #[test]
    fn frames_keep_the_decoded_chroma_sampling() {
        for wide in [false, true] {
            let f = HevcDecoder::convert(picture(ChromaFormat::C420, 1, wide));
            assert_eq!(chroma_of(&f.frame.data), Some((Chroma::C420, 2)), "4:2:0, 16-bit {wide}");
            let f = HevcDecoder::convert(picture(ChromaFormat::C422, 2, wide));
            assert_eq!(chroma_of(&f.frame.data), Some((Chroma::C422, 4)), "4:2:2, 16-bit {wide}");
        }
    }
}

#[cfg(not(feature = "patent-codecs"))]
pub use crate::c0_no_patent::{H264Decoder, HevcDecoder, h264_factory, h264_threads, hevc_factory};
