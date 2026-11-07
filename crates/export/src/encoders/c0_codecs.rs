//! The AAC and H.264 encoders of `encoders`: our own with `patent-codecs`, and the WebCodecs-only fallbacks
//! of `c0_codec_policy` without it. `encoders` re-exports the factories.

#[cfg(feature = "patent-codecs")]
use crate::convert::{Matrix, rgba_to_yuv420_8, rgba_to_yuv420_10, rgbf_to_yuv420_8, rgbf_to_yuv420_10};
#[cfg(feature = "patent-codecs")]
use crate::format::{DEFAULT_CRF, H264Pass};
#[cfg(feature = "patent-codecs")]
use crate::registry::PrepareAhead;
#[cfg(feature = "patent-codecs")]
use super::*;
#[cfg(feature = "patent-codecs")]
use crate::{BitrateMode, H264Profile};
#[cfg(feature = "patent-codecs")]
use std::sync::{Arc, Mutex, PoisonError};

#[cfg(not(feature = "patent-codecs"))]
pub(crate) use crate::c0_codec_policy::{aac_factory, h264_factory};

/// AAC-LC (our encoder), 320 kbps stereo by default.
#[cfg(feature = "patent-codecs")]
struct AacEncoder {
    enc: filmcraft_aac::Encoder,
    rate: u32,
    channels: u32,
}

#[cfg(feature = "patent-codecs")]
impl AudioEncoder for AacEncoder {
    fn sample_entry(&self) -> SampleEntry {
        SampleEntry::aac(self.enc.audio_specific_config(), self.channels, self.rate)
    }
    fn encode(&mut self, planar: &[Vec<f32>]) -> Result<Vec<Vec<u8>>> {
        let refs: Vec<&[f32]> = if planar.len() == 6 {
            // ours: L, R, C, LFE, Ls, Rs → AAC channel configuration 6: C, L, R, Ls, Rs, LFE
            [2usize, 0, 1, 4, 5, 3].iter().map(|&c| planar[c].as_slice()).collect()
        } else {
            planar.iter().map(Vec::as_slice).collect()
        };
        Ok(self.enc.encode(&refs))
    }
    fn flush(&mut self) -> Result<Vec<Vec<u8>>> {
        Ok(self.enc.flush())
    }
    fn priming(&self) -> u32 {
        self.enc.priming_samples()
    }
    fn frame_size(&self) -> u32 {
        1024
    }
}

#[cfg(feature = "patent-codecs")]
pub(crate) fn aac_factory(_format: Format, sample_rate: u32, channels: u32, s: &ExportSettings) -> Option<Result<Box<dyn AudioEncoder>>> {
    // AAC caps a frame at 6144 bits per channel (ISO/IEC 14496-3 §4.5.3.2): 6 bits per sample, so
    // 264.6 kbps for stereo at 22.05 kHz; a higher setting is capped instead of refused
    let max = (6 * sample_rate as u64 * channels.max(1) as u64).min(u32::MAX as u64) as u32;
    let bps = (s.audio.bitrate_kbps.clamp(32, 512) * 1000).min(max);
    Some(
        filmcraft_aac::Encoder::new(filmcraft_aac::EncoderConfig::cbr(sample_rate, channels as usize, bps))
            .map(|enc| Box::new(AacEncoder { enc, rate: sample_rate, channels }) as Box<dyn AudioEncoder>)
            .map_err(|e| ExportError::Encode(e.to_string())),
    )
}

/// H.264 (our encoder): sRGB/709 RGBA8 → limited-range BT.709 4:2:0, 8-bit, or 10-bit for High 10
/// (HDR frames from their float pixels), length-prefixed samples with the `avcC` in the sample entry.
#[cfg(feature = "patent-codecs")]
struct H264Encoder {
    enc: filmcraft_h264enc::Encoder,
    w: u32,
    h: u32,
    rate: FrameRate,
    y: Vec<u8>,
    u: Vec<u8>,
    v: Vec<u8>,
    /// The 10-bit planes of High 10.
    y16: Vec<u16>,
    u16: Vec<u16>,
    v16: Vec<u16>,
    signal: ColorSignal,
    /// 8-bit 4:2:0 planes the render threads converted ahead ([`VideoEncoder::prepare_ahead`]), by
    /// picture index.
    ready: Arc<Mutex<std::collections::HashMap<u64, Yuv420>>>,
}

#[cfg(feature = "patent-codecs")]
type Yuv420 = (Vec<u8>, Vec<u8>, Vec<u8>);

/// The encoder input of `f`: 8-bit limited-range 4:2:0 in the export's colour signal.
#[cfg(feature = "patent-codecs")]
fn h264_input(f: &EncoderFrame, signal: ColorSignal, y: &mut Vec<u8>, u: &mut Vec<u8>, v: &mut Vec<u8>) {
    match f.hdr {
        Some(rgb) => {
            let (kr, kb) = signal.kr_kb();
            rgbf_to_yuv420_8(rgb, f.width as usize, f.height as usize, Matrix::new(kr, kb), y, u, v)
        }
        None => rgba_to_yuv420_8(f.rgba, f.width as usize, f.height as usize, y, u, v),
    }
}

/// The High 10 encoder input of `f`: 10-bit limited-range 4:2:0 in the export's colour signal
/// (HDR frames from their float pixels).
#[cfg(feature = "patent-codecs")]
fn h264_input10(f: &EncoderFrame, signal: ColorSignal, y: &mut Vec<u16>, u: &mut Vec<u16>, v: &mut Vec<u16>) -> Result<()> {
    let (w, h) = (f.width as usize, f.height as usize);
    match f.hdr {
        Some(rgb) => {
            let (kr, kb) = signal.kr_kb();
            rgbf_to_yuv420_10(rgb, w, h, Matrix::new(kr, kb), y, u, v)
        }
        None => rgba_to_yuv420_10(f.rgba, w, h, y, u, v),
    }
}

#[cfg(feature = "patent-codecs")]
impl H264Encoder {
    fn deep(&self) -> bool {
        self.enc.config().bit_depth > 8
    }

    /// Encode `f` at 10 bits (High 10), converting it on the encoder's threads.
    fn encode10(&mut self, f: &EncoderFrame) -> Result<Vec<filmcraft_h264enc::Packet>> {
        let (enc, y, u, v, signal) = (&self.enc, &mut self.y16, &mut self.u16, &mut self.v16, self.signal);
        timed(Stage::Convert, || enc.install(|| h264_input10(f, signal, y, u, v)))?;
        let cw = (f.width as usize).div_ceil(2);
        let frame = filmcraft_h264enc::YuvFrame16 { y: &self.y16, u: &self.u16, v: &self.v16, y_stride: f.width as usize, uv_stride: cw };
        self.enc.encode16(&frame, f.index as i64 * self.rate.den).map_err(|e| ExportError::Encode(e.to_string()))
    }

    fn packets(&self, ps: Vec<filmcraft_h264enc::Packet>) -> Vec<EncodedPacket> {
        ps.into_iter()
            .map(|p| EncodedPacket { data: p.data, key: p.keyframe, duration: self.rate.den as u32, composition_offset: (p.pts - p.dts) as i32 })
            .collect()
    }
}

#[cfg(feature = "patent-codecs")]
impl VideoEncoder for H264Encoder {
    fn sample_entry(&self) -> SampleEntry {
        let cfg = filmcraft_isobmff::AvcConfig::parse(&self.enc.avcc()).unwrap_or_else(|_| {
            let (sps, pps) = self.enc.sps_pps();
            filmcraft_isobmff::AvcConfig::new(vec![sps], vec![pps], 4)
        });
        let mut e = SampleEntry::avc(cfg, self.w as u16, self.h as u16);
        self.signal.apply_to(&mut e, false);
        e
    }
    fn timescale(&self) -> u32 {
        self.rate.num as u32
    }
    fn encode(&mut self, f: &EncoderFrame) -> Result<Vec<EncodedPacket>> {
        if self.deep() {
            let ps = self.encode10(f)?;
            return Ok(self.packets(ps));
        }
        let ready = {
            let mut ready = self.ready.lock().unwrap_or_else(PoisonError::into_inner);
            let mine = ready.remove(&f.index);
            // anything older belongs to a batch that was rendered and then not encoded
            ready.retain(|&i, _| i > f.index);
            mine
        };
        let (w, h) = (f.width as usize, f.height as usize);
        let chroma = w.div_ceil(2) * h.div_ceil(2);
        match ready.filter(|(y, u, v)| y.len() == w * h && u.len() == chroma && v.len() == chroma) {
            Some((y, u, v)) => (self.y, self.u, self.v) = (y, u, v),
            None => {
                let (enc, y, u, v, signal) = (&self.enc, &mut self.y, &mut self.u, &mut self.v, self.signal);
                // on the encoder's own threads: the global pool is busy rendering the next batch
                timed(Stage::Convert, || enc.install(|| h264_input(f, signal, y, u, v)));
            }
        }
        let cw = (f.width as usize).div_ceil(2);
        let frame = filmcraft_h264enc::YuvFrame { y: &self.y, u: &self.u, v: &self.v, y_stride: f.width as usize, uv_stride: cw };
        let ps = self.enc.try_encode(&frame, f.index as i64 * self.rate.den).map_err(|e| ExportError::Encode(e.to_string()))?;
        Ok(self.packets(ps))
    }
    fn flush(&mut self) -> Result<Vec<EncodedPacket>> {
        let ps = self.enc.flush();
        Ok(self.packets(ps))
    }
    fn media_start(&self) -> Option<i64> {
        // With B-frames the first DTS is one frame before the first PTS.
        (self.enc.delay() > 0).then_some(self.rate.den)
    }
    fn pass_stats(&self) -> Option<filmcraft_h264enc::PassStats> {
        self.enc.pass_stats()
    }
    fn prepare_ahead(&self) -> Option<PrepareAhead> {
        // High 10 converts in `encode`: the planes prepared ahead are 8-bit
        if self.deep() {
            return None;
        }
        let (ready, signal) = (self.ready.clone(), self.signal);
        Some(Arc::new(move |f: &EncoderFrame| {
            let (mut y, mut u, mut v) = (Vec::new(), Vec::new(), Vec::new());
            timed(Stage::Convert, || h264_input(f, signal, &mut y, &mut u, &mut v));
            ready.lock().unwrap_or_else(PoisonError::into_inner).insert(f.index, (y, u, v));
        }))
    }
}

/// Slices per H.264 picture: one per four macroblock rows. The encoder's default follows the core
/// count, which would make the stream (and every decoded picture) depend on the machine; this is
/// what it picks on a machine with enough cores.
#[cfg(feature = "patent-codecs")]
pub(crate) fn h264_slices(height: u32) -> usize {
    (height.div_ceil(16) as usize).div_ceil(4).max(1)
}

#[cfg(feature = "patent-codecs")]
pub(crate) fn h264_factory(format: Format, w: u32, h: u32, rate: FrameRate, s: &ExportSettings) -> Option<Result<Box<dyn VideoEncoder>>> {
    if format != Format::H264 {
        return None;
    }
    let mut cfg = filmcraft_h264enc::EncoderConfig::new(w, h, rate.num as u32, rate.den as u32);
    // MXF carries the Annex B byte stream (ST 381-3) with in-band parameter sets
    cfg.format = if s.format.is_mxf() { filmcraft_h264enc::PacketFormat::AnnexB } else { filmcraft_h264enc::PacketFormat::LengthPrefixed };
    cfg.aud = s.format.is_mxf();
    cfg.keyint = s.keyframe_distance.filter(|k| *k > 0).unwrap_or_else(|| (rate.num as f64 / rate.den as f64 * 2.0).round().max(1.0) as u32);
    cfg.slices = h264_slices(h);
    let kbps = s.bitrate_kbps.max(100);
    let max = s.max_bitrate_kbps.filter(|m| *m >= kbps).unwrap_or_else(|| (u64::from(kbps) * 3 / 2).min(u64::from(u32::MAX)) as u32);
    cfg.rate = match s.bitrate_mode {
        BitrateMode::Cbr => filmcraft_h264enc::RateControl::Cbr { kbps },
        // the encoder clamps to 0..=51; a NaN from a hand-edited preset becomes the default
        BitrateMode::Crf => filmcraft_h264enc::RateControl::Crf(if s.crf.is_finite() { s.crf } else { DEFAULT_CRF }),
        _ => filmcraft_h264enc::RateControl::Vbr { target_kbps: kbps, max_kbps: max },
    };
    cfg.pass = match &s.h264_pass {
        H264Pass::Single => filmcraft_h264enc::Pass::Single,
        H264Pass::First => filmcraft_h264enc::Pass::First,
        H264Pass::Second(st) => filmcraft_h264enc::Pass::Second(st.clone()),
    };
    cfg.profile = match s.h264_profile {
        H264Profile::Baseline => filmcraft_h264enc::Profile::Baseline,
        H264Profile::Main => filmcraft_h264enc::Profile::Main,
        H264Profile::High => filmcraft_h264enc::Profile::High,
        H264Profile::High10 => filmcraft_h264enc::Profile::High10,
    };
    cfg.bit_depth = s.h264_profile.bit_depth();
    cfg.intra_only = s.h264_intra_only;
    cfg.level = s.h264_level;
    if let Some((n, d)) = s.pixel_aspect {
        cfg.sar = (n.clamp(1, 65535) as u16, d.clamp(1, 65535) as u16);
    }
    if s.signal.is_hdr() {
        cfg.color = filmcraft_h264enc::ColorConfig { primaries: s.signal.primaries, transfer: s.signal.transfer, matrix: s.signal.matrix, full_range: false };
        if let Some((md, cll)) = s.signal.static_metadata() {
            let mut b = [0u8; 24];
            b.copy_from_slice(&md.to_bytes());
            cfg.mastering_display = Some(b);
            cfg.content_light = Some(cll);
        }
    }
    Some(
        filmcraft_h264enc::Encoder::new(cfg)
            .map(|enc| {
                let (y, u, v, y16, u16, v16) = Default::default();
                Box::new(H264Encoder { enc, w, h, rate, y, u, v, y16, u16, v16, signal: s.signal, ready: Default::default() }) as Box<dyn VideoEncoder>
            })
            .map_err(|e| ExportError::Encode(e.to_string())),
    )
}
