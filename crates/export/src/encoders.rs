use super::convert::{Matrix, Planes444, covers, rgba_to_yuv422_10, rgbf_to_yuv422_10, to_yuv444_10};
use super::format::{ColorSignal, ExportSettings, Format};
use super::registry::{AudioEncoder, EncodedPacket, EncoderFrame, VideoEncoder};
use super::stats::{Stage, timed};
use super::{ExportError, FrameRate, Result, SampleEntry, SampleFormat};
mod c0_codecs;
pub(crate) use c0_codecs::*;

struct MjpegEncoder {
    w: u16,
    h: u16,
    quality: u8,
    rate: FrameRate,
}

impl VideoEncoder for MjpegEncoder {
    fn sample_entry(&self) -> SampleEntry {
        SampleEntry::jpeg(self.w, self.h)
    }
    fn timescale(&self) -> u32 {
        self.rate.num as u32
    }
    fn encode(&mut self, f: &EncoderFrame) -> Result<Vec<EncodedPacket>> {
        let rgb: Vec<u8> = f.rgba.as_chunks::<4>().0.iter().flat_map(|p| [p[0], p[1], p[2]]).collect();
        let mut out = Vec::new();
        let mut enc = image::codecs::jpeg::JpegEncoder::new_with_quality(&mut out, self.quality);
        enc.encode(&rgb, f.width, f.height, image::ExtendedColorType::Rgb8).map_err(|e| ExportError::Encode(e.to_string()))?;
        Ok(vec![EncodedPacket { data: out, key: true, duration: self.rate.den as u32, composition_offset: 0 }])
    }
    fn flush(&mut self) -> Result<Vec<EncodedPacket>> {
        Ok(Vec::new())
    }
}

pub(crate) fn mjpeg_factory(format: Format, w: u32, h: u32, rate: FrameRate, s: &ExportSettings) -> Option<Result<Box<dyn VideoEncoder>>> {
    (format == Format::Mjpeg).then(|| Ok(Box::new(MjpegEncoder { w: w as u16, h: h as u16, quality: s.quality.clamp(1, 100), rate }) as Box<dyn VideoEncoder>))
}

/// FLAC (our encoder) in MP4 / QuickTime: one frame of 4096 samples per access unit.
pub(crate) struct FlacAudio {
    enc: craft_audio::flac::Encoder,
    format: SampleFormat,
    /// Samples of the frame `flush` returns (shorter than a full block).
    tail: u32,
}

impl FlacAudio {
    pub(crate) fn new(sample_rate: u32, channels: u32, s: &ExportSettings) -> Result<Self> {
        let format = s.audio.sample_format();
        let ch = u8::try_from(channels).map_err(|_| ExportError::Unsupported("FLAC: too many audio channels".into()))?;
        let mut cfg = craft_audio::flac::EncoderConfig::new(sample_rate, ch, format.bits() as u8);
        cfg.level = s.audio.flac_level.min(8);
        let enc = craft_audio::flac::Encoder::new(cfg).map_err(|e| ExportError::Unsupported(format!("FLAC: {e}")))?;
        Ok(FlacAudio { enc, format, tail: 0 })
    }
}

impl AudioEncoder for FlacAudio {
    fn sample_entry(&self) -> SampleEntry {
        SampleEntry::flac(&self.enc.streaminfo())
    }
    fn encode(&mut self, planar: &[Vec<f32>]) -> Result<Vec<Vec<u8>>> {
        let ints: Vec<Vec<i32>> = planar.iter().map(|c| c.iter().map(|&s| self.format.quantise(s)).collect()).collect();
        let refs: Vec<&[i32]> = ints.iter().map(Vec::as_slice).collect();
        self.enc.encode_frames(&refs).map_err(|e| ExportError::Encode(format!("FLAC: {e}")))
    }
    fn flush(&mut self) -> Result<Vec<Vec<u8>>> {
        Ok(self.flush_timed()?.into_iter().map(|(au, _)| au).collect())
    }
    fn flush_timed(&mut self) -> Result<Vec<(Vec<u8>, u32)>> {
        self.tail = self.enc.pending_samples() as u32;
        let last = self.enc.finish();
        Ok(if last.is_empty() { Vec::new() } else { vec![(last, self.tail)] })
    }
    fn priming(&self) -> u32 {
        0
    }
    fn frame_size(&self) -> u32 {
        4096
    }
    fn final_sample_entry(&self) -> Option<SampleEntry> {
        Some(SampleEntry::flac(&self.enc.streaminfo()))
    }
}

/// ProRes 422 encoder (HQ unless the settings pick another flavour): sRGB/709 RGBA8 → 10-bit limited-range BT.709 4:2:2.
struct ProResEncoder {
    enc: craft_video::prores::Encoder,
    profile: craft_video::prores::Profile,
    /// Code the frames' alpha channel (4444 profiles with Include Alpha Channel).
    alpha: bool,
    w: u32,
    h: u32,
    rate: FrameRate,
    signal: ColorSignal,
}

impl VideoEncoder for ProResEncoder {
    fn sample_entry(&self) -> SampleEntry {
        let mut e = SampleEntry::prores(filmcraft_isobmff::FourCc(self.profile.fourcc()), self.w as u16, self.h as u16);
        self.signal.apply_to(&mut e, true);
        e
    }
    fn timescale(&self) -> u32 {
        self.rate.num as u32
    }
    fn encode(&mut self, f: &EncoderFrame) -> Result<Vec<EncodedPacket>> {
        let chroma = self.profile.chroma();
        let mut fr = craft_video::prores::Frame::new(f.width, f.height, chroma, 10, self.alpha).map_err(|e| ExportError::Encode(e.to_string()))?;
        let (w, h) = (f.width as usize, f.height as usize);
        timed(Stage::Convert, || match (chroma, f.hdr) {
            (craft_video::prores::ChromaFormat::Yuv444, hdr) => {
                let (kr, kb) = if hdr.is_some() { self.signal.kr_kb() } else { (0.2126, 0.0722) };
                to_yuv444_10(f.rgba, hdr, w, h, Matrix::new(kr, kb), Planes444 { y: &mut fr.y, cb: &mut fr.cb, cr: &mut fr.cr, alpha: fr.alpha.as_deref_mut() })
            }
            (_, Some(rgb)) => {
                let (kr, kb) = self.signal.kr_kb();
                rgbf_to_yuv422_10(rgb, w, h, Matrix::new(kr, kb), &mut fr.y, &mut fr.cb, &mut fr.cr)
            }
            (_, None) => rgba_to_yuv422_10(f.rgba, w, h, &mut fr.y, &mut fr.cb, &mut fr.cr),
        });
        let data = self.enc.encode(&fr).map_err(|e| ExportError::Encode(e.to_string()))?;
        Ok(vec![EncodedPacket { data, key: true, duration: self.rate.den as u32, composition_offset: 0 }])
    }
    fn flush(&mut self) -> Result<Vec<EncodedPacket>> {
        Ok(Vec::new())
    }
}

/// The ProRes profile named by [`ExportSettings::prores_profile`].
pub fn prores_profile(name: &str) -> craft_video::prores::Profile {
    use craft_video::prores::Profile;
    match name.to_ascii_lowercase().as_str() {
        "proxy" => Profile::Proxy,
        "lt" => Profile::Lt,
        "standard" | "422" => Profile::Standard,
        "4444" => Profile::P4444,
        "4444xq" | "4444 xq" | "4444-xq" | "xq" => Profile::P4444Xq,
        _ => Profile::Hq,
    }
}

pub(crate) fn prores_factory(format: Format, w: u32, h: u32, rate: FrameRate, s: &ExportSettings) -> Option<Result<Box<dyn VideoEncoder>>> {
    (format == Format::ProRes).then(|| {
        let profile = prores_profile(&s.prores_profile);
        let mut cfg = craft_video::prores::EncoderConfig::new(profile, w, h);
        if s.signal.is_hdr() {
            cfg.color = craft_video::prores::ColorInfo { primaries: s.signal.primaries, transfer: s.signal.transfer, matrix: s.signal.matrix };
        }
        let alpha = s.keeps_alpha();
        cfg.encode_alpha = alpha;
        let enc = craft_video::prores::Encoder::with_config(cfg);
        Ok(Box::new(ProResEncoder { enc, profile, alpha, w, h, rate, signal: s.signal }) as Box<dyn VideoEncoder>)
    })
}

/// The DNxHR profile named by [`ExportSettings::dnx_profile`].
pub fn dnx_profile(name: &str) -> filmcraft_dnx::Profile {
    use filmcraft_dnx::Profile;
    match name.to_ascii_lowercase().as_str() {
        "lb" => Profile::Lb,
        "sq" => Profile::Sq,
        "hqx" => Profile::Hqx,
        _ => Profile::Hq,
    }
}

/// DNxHR encoder (RGBA8 or HDR floats → BT.709 / BT.2020 limited-range 4:2:2; 8-bit for
/// LB/SQ/HQ, 10-bit for HQX).
struct DnxEncoder {
    enc: filmcraft_dnx::Encoder,
    w: u32,
    h: u32,
    rate: FrameRate,
    signal: ColorSignal,
}

impl VideoEncoder for DnxEncoder {
    fn sample_entry(&self) -> SampleEntry {
        let mut e = SampleEntry::dnx(filmcraft_isobmff::FourCc(*b"AVdh"), self.w as u16, self.h as u16);
        self.signal.apply_to(&mut e, true);
        e
    }
    fn timescale(&self) -> u32 {
        self.rate.num as u32
    }
    fn encode(&mut self, f: &EncoderFrame) -> Result<Vec<EncodedPacket>> {
        let mut fr = filmcraft_dnx::Frame::new(f.width, f.height, filmcraft_dnx::ChromaFormat::Yuv422, 10, false);
        match f.hdr {
            Some(rgb) => {
                let (kr, kb) = self.signal.kr_kb();
                rgbf_to_yuv422_10(rgb, f.width as usize, f.height as usize, Matrix::new(kr, kb), &mut fr.y, &mut fr.cb, &mut fr.cr)
            }
            None => rgba_to_yuv422_10(f.rgba, f.width as usize, f.height as usize, &mut fr.y, &mut fr.cb, &mut fr.cr),
        }
        // the encoder rescales 10-bit input to its coded depth
        let data = self.enc.encode(&fr).map_err(|e| ExportError::Encode(e.to_string()))?;
        Ok(vec![EncodedPacket { data, key: true, duration: self.rate.den as u32, composition_offset: 0 }])
    }
    fn flush(&mut self) -> Result<Vec<EncodedPacket>> {
        Ok(Vec::new())
    }
}

pub(crate) fn dnx_factory(format: Format, w: u32, h: u32, rate: FrameRate, s: &ExportSettings) -> Option<Result<Box<dyn VideoEncoder>>> {
    (format == Format::DnxHr).then(|| {
        let mut cfg = filmcraft_dnx::EncoderConfig::new(dnx_profile(&s.dnx_profile), w, h);
        if s.signal.primaries == 9 {
            cfg.color_volume = filmcraft_dnx::ColorVolume::Bt2020Ncl;
        }
        let enc = filmcraft_dnx::Encoder::with_config(cfg).map_err(|e| ExportError::Encode(e.to_string()))?;
        Ok(Box::new(DnxEncoder { enc, w, h, rate, signal: s.signal }) as Box<dyn VideoEncoder>)
    })
}

/// The APV profile named by [`ExportSettings::apv_profile`].
pub fn apv_profile(name: &str) -> craft_video::apv::Profile {
    use craft_video::apv::Profile;
    match name.to_ascii_lowercase().replace([' ', '_'], "-").trim_start_matches("apv-").trim_start_matches("apv") {
        "422-12" | "42212" => Profile::P422_12,
        "444-10" | "44410" => Profile::P444_10,
        "444-12" | "44412" => Profile::P444_12,
        "4444-10" | "444410" => Profile::P4444_10,
        "4444-12" | "444412" => Profile::P4444_12,
        "400-10" | "40010" => Profile::P400_10,
        _ => Profile::P422_10,
    }
}

/// APV encoder (RFC 9924: 10/12-bit 4:2:2 or 4:4:4 intra).
struct ApvEncoder {
    enc: craft_video::apv::Encoder,
    chroma: craft_video::apv::ChromaFormat,
    w: u32,
    h: u32,
    rate: FrameRate,
    signal: ColorSignal,
    /// Scratch chroma planes for the 4:0:0 luma conversion, reused across frames.
    cb422: Vec<u16>,
    cr422: Vec<u16>,
}

impl VideoEncoder for ApvEncoder {
    fn sample_entry(&self) -> SampleEntry {
        let apvc = filmcraft_isobmff::ApvConfig::parse(&self.enc.decoder_config_record()).unwrap_or_default();
        let mut e = SampleEntry::apv(apvc, self.w as u16, self.h as u16);
        self.signal.apply_to(&mut e, true);
        e
    }
    fn timescale(&self) -> u32 {
        self.rate.num as u32
    }
    /// Converts `f` into the profile's planes (4:4:4 at full chroma resolution, 4:2:2, or 4:0:0
    /// luma only), BT.709 for SDR and the signal's matrix for HDR, and codes one access unit.
    /// A picture buffer shorter than `width` x `height` is an error.
    fn encode(&mut self, f: &EncoderFrame) -> Result<Vec<EncodedPacket>> {
        use craft_video::apv::ChromaFormat as Cf;
        let (w, h) = (f.width as usize, f.height as usize);
        let short = |what: &str, got: usize| ExportError::Encode(format!("a {w}x{h} picture needs {what}, got {got}"));
        match f.hdr {
            Some(rgb) if !covers(rgb.len(), w, h, 3) => return Err(short("w*h*3 floats", rgb.len())),
            None if !covers(f.rgba.len(), w, h, 4) => return Err(short("w*h*4 bytes", f.rgba.len())),
            _ => {}
        }
        let mut fr = craft_video::apv::Frame::new(f.width, f.height, self.chroma, 10, self.chroma == Cf::Yuv4444);
        let (kr, kb) = if f.hdr.is_some() { self.signal.kr_kb() } else { (0.2126, 0.0722) };
        let chroma = self.chroma;
        let (cb422, cr422) = (&mut self.cb422, &mut self.cr422);
        timed(Stage::Convert, || match chroma {
            Cf::Yuv444 | Cf::Yuv4444 => to_yuv444_10(
                f.rgba,
                f.hdr,
                w,
                h,
                Matrix::new(kr, kb),
                Planes444 { y: &mut fr.y, cb: &mut fr.cb, cr: &mut fr.cr, alpha: fr.alpha.as_deref_mut() },
            ),
            Cf::Yuv422 => match f.hdr {
                Some(rgb) => rgbf_to_yuv422_10(rgb, w, h, Matrix::new(kr, kb), &mut fr.y, &mut fr.cb, &mut fr.cr),
                None => rgba_to_yuv422_10(f.rgba, w, h, &mut fr.y, &mut fr.cb, &mut fr.cr),
            },
            Cf::Monochrome => {
                // the 4:2:2 converters want chroma planes; only the luma is coded
                let n = w.div_ceil(2).saturating_mul(h);
                cb422.resize(n, 0);
                cr422.resize(n, 0);
                match f.hdr {
                    Some(rgb) => rgbf_to_yuv422_10(rgb, w, h, Matrix::new(kr, kb), &mut fr.y, cb422, cr422),
                    None => rgba_to_yuv422_10(f.rgba, w, h, &mut fr.y, cb422, cr422),
                }
            }
        });
        let data = self.enc.encode_raw_au(&fr).map_err(|e| ExportError::Encode(e.to_string()))?;
        Ok(vec![EncodedPacket { data, key: true, duration: self.rate.den as u32, composition_offset: 0 }])
    }
    fn flush(&mut self) -> Result<Vec<EncodedPacket>> {
        Ok(Vec::new())
    }
}

pub(crate) fn apv_factory(format: Format, w: u32, h: u32, rate: FrameRate, s: &ExportSettings) -> Option<Result<Box<dyn VideoEncoder>>> {
    (format == Format::Apv).then(|| {
        let profile = apv_profile(&s.apv_profile);
        let mut cfg = craft_video::apv::EncoderConfig::new(profile, w, h);
        if s.signal.is_hdr() {
            cfg.color = craft_video::apv::ColorInfo { primaries: s.signal.primaries, transfer: s.signal.transfer, matrix: s.signal.matrix, full_range: false };
        }
        let chroma = cfg.chroma;
        let enc = craft_video::apv::Encoder::with_config(cfg).map_err(|e| ExportError::Encode(e.to_string()))?;
        Ok(Box::new(ApvEncoder { enc, chroma, w, h, rate, signal: s.signal, cb422: Vec::new(), cr422: Vec::new() }) as Box<dyn VideoEncoder>)
    })
}
