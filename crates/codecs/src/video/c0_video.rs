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
    dec: filmcraft_h264::Decoder,
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

    fn make_inner(avcc: &[u8], threads: usize) -> Result<filmcraft_h264::Decoder> {
        let mut dec = filmcraft_h264::Decoder::with_threads(threads);
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
        Self { avcc: Vec::new(), dec: filmcraft_h264::Decoder::with_threads(threads), length_size: 0, draft: false, threads }
    }
    fn convert(p: filmcraft_h264::Picture) -> DecodedFrame {
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
fn h264_error(e: filmcraft_h264::Error) -> CodecError {
    match e {
        filmcraft_h264::Error::Unsupported(what) => CodecError::Unsupported(format!("H.264: {what}")),
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
        if let Ok(d) = Self::make_inner(&self.avcc, self.threads) {
            self.dec = d;
        }
        self.dec.set_draft(self.draft);
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
        // Annex B samples carry their parameter sets: an IDR access unit is a starting point.
        (self.length_size == 0).then(|| filmcraft_bitstream::annexb_nals(sample).iter().any(|n| n.first().is_some_and(|h| h & 0x1f == 5)))
    }
}

/// The H.264 decoder's output planes come from the frames the caches evicted
/// (`filmcraft_frame::pool`) instead of the allocator.
#[cfg(feature = "patent-codecs")]
fn recycle_h264_planes() {
    filmcraft_h264::set_plane_allocator(filmcraft_frame::pool::take_u8);
}

/// Worker threads each H.264 decoder uses (frame threading; 1 on wasm).
#[cfg(feature = "patent-codecs")]
pub fn h264_threads() -> usize {
    filmcraft_h264::default_threads()
}

#[cfg(feature = "patent-codecs")]
pub fn h264_factory(e: &SampleEntry) -> Option<Result<Box<dyn VideoDecoder>>> {
    match &e.codec {
        CodecConfig::Avc(a) => Some(H264Decoder::new(a.to_bytes()).map(|d| Box::new(d) as Box<dyn VideoDecoder>)),
        _ => None,
    }
}

/// Our pure-Rust HEVC decoder (Main / Main 10, frame-threaded).
#[cfg(feature = "patent-codecs")]
pub struct HevcDecoder {
    hvcc: Vec<u8>,
    dec: filmcraft_hevc::Decoder,
    length_size: usize,
    highest_tid: Option<u8>,
}

#[cfg(feature = "patent-codecs")]
impl HevcDecoder {
    pub fn new(hvcc: Vec<u8>) -> Result<Self> {
        let dec = filmcraft_hevc::Decoder::from_hvcc(&hvcc).map_err(hevc_error)?;
        let (length_size, highest_tid) = hvcc_length_size_and_tid(&hvcc);
        Ok(Self { hvcc, dec, length_size, highest_tid })
    }
    /// A decoder for Annex B byte-stream samples (start codes, in-band parameter sets: TS).
    pub fn annexb() -> Self {
        Self { hvcc: Vec::new(), dec: filmcraft_hevc::Decoder::new(), length_size: 0, highest_tid: None }
    }
    fn convert(p: filmcraft_hevc::Picture) -> DecodedFrame {
        use filmcraft_hevc::Plane;
        use std::sync::Arc;
        let (w, h) = (p.width as usize, p.height as usize);
        let (cw, ch) = (p.chroma_width as usize, p.chroma_height as usize);
        let (ys, uvs) = (p.y_stride, p.uv_stride);
        let wide = |pl: Plane| -> Vec<u16> {
            match pl {
                Plane::U16(v) => v,
                Plane::U8(v) => v.into_iter().map(u16::from).collect(),
            }
        };
        let data = match (p.y, p.u, p.v) {
            (Plane::U8(y), Plane::U8(u), Plane::U8(v)) => filmcraft_frame::PixelData::Yuv8 {
                planes: [Arc::new(tight_plane(y, ys, w, h)), Arc::new(tight_plane(u, uvs, cw, ch)), Arc::new(tight_plane(v, uvs, cw, ch))],
                chroma: filmcraft_frame::Chroma::C420,
                alpha: None,
            },
            (y, u, v) => filmcraft_frame::PixelData::Yuv16 {
                planes: [Arc::new(tight_plane(wide(y), ys, w, h)), Arc::new(tight_plane(wide(u), uvs, cw, ch)), Arc::new(tight_plane(wide(v), uvs, cw, ch))],
                chroma: filmcraft_frame::Chroma::C420,
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
fn hevc_error(e: filmcraft_hevc::Error) -> CodecError {
    match e {
        filmcraft_hevc::Error::Unsupported(what) => CodecError::Unsupported(format!("HEVC: {what}")),
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
            self.dec = filmcraft_hevc::Decoder::new();
        } else if let Ok(d) = filmcraft_hevc::Decoder::from_hvcc(&self.hvcc) {
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

#[cfg(not(feature = "patent-codecs"))]
pub use crate::c0_no_patent::{H264Decoder, HevcDecoder, h264_factory, h264_threads, hevc_factory};
