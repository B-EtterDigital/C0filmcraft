//! Software-codec-free fallbacks. Browser sources take precedence where supported.
use crate::{CodecError, DecodedFrame, Result, VideoDecoder};
use filmcraft_isobmff::{AvcConfig, CodecConfig, HevcConfig, SampleEntry};
fn unsupported() -> CodecError {
    CodecError::Unsupported("H.264, HEVC and AAC require browser WebCodecs; not supported here".into())
}
pub struct H264Decoder;
pub struct HevcDecoder;
macro_rules! unavailable_decoder {
    ($ty:ident) => {
        impl $ty {
            pub fn annexb() -> Self {
                Self
            }
        }
        impl VideoDecoder for $ty {
            fn decode(&mut self, _data: &[u8], _pts: i64) -> Result<Vec<DecodedFrame>> {
                Err(unsupported())
            }
            fn flush(&mut self) -> Vec<DecodedFrame> {
                Vec::new()
            }
            fn reset(&mut self) {}
            fn name(&self) -> &str {
                "Browser WebCodecs required"
            }
        }
    };
}
unavailable_decoder!(H264Decoder);
unavailable_decoder!(HevcDecoder);
pub fn h264_factory(entry: &SampleEntry) -> Option<Result<Box<dyn VideoDecoder>>> {
    matches!(entry.codec, CodecConfig::Avc(_)).then(|| Err(unsupported()))
}
pub fn hevc_factory(entry: &SampleEntry) -> Option<Result<Box<dyn VideoDecoder>>> {
    matches!(entry.codec, CodecConfig::Hevc(_)).then(|| Err(unsupported()))
}
pub fn h264_threads() -> usize {
    1
}
impl crate::hw::NalStreamInfo {
    pub fn from_avcc(_avcc: &[u8]) -> Result<Self> {
        Err(unsupported())
    }
    pub fn from_hvcc(_hvcc: &[u8]) -> Result<Self> {
        Err(unsupported())
    }
}
impl crate::audio::PacketDecoder {
    pub fn aac(_asc: &[u8], _rate: u32) -> Result<Self> {
        Err(unsupported())
    }
    pub fn ensure_aac(&mut self, _asc: &[u8]) -> Result<()> {
        Err(unsupported())
    }
}
pub fn aac_output_rate<'a>(asc: &[u8], _units: impl IntoIterator<Item = &'a [u8]>) -> Option<u32> {
    crate::c0_codec_policy::aac_core_rate(asc)
}
/// ADTS, LATM and `.aac` files: upstream reads their headers with craft-audio's AAC module, which
/// exists only with the AAC decoder. An ADTS frame is handed on whole, so the TS audio decoder
/// reports that AAC needs the browser ([`crate::audio::PacketDecoder::ensure_aac`]); a LATM
/// configuration is not read, so the stream is listed as unsupported.
pub fn adts_split(frame: &[u8]) -> Option<(Vec<u8>, &[u8])> {
    Some((Vec::new(), frame))
}
pub fn decode_adts(_bytes: &[u8]) -> Result<Option<(u32, Vec<Vec<f32>>)>> {
    Err(unsupported())
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LatmConfig {
    pub asc: Vec<u8>,
    pub sample_rate: u32,
    pub channels: u32,
}
impl LatmConfig {
    pub fn new(asc: Vec<u8>, sample_rate: u32, channels: u32) -> Self {
        Self { asc, sample_rate, channels }
    }
}
pub fn latm_config(_frame: &[u8]) -> Option<LatmConfig> {
    None
}
pub fn latm_split(_frame: &[u8], _last: &mut Option<LatmConfig>) -> Option<(Vec<u8>, Vec<u8>)> {
    None
}
pub fn avc_presentation_order<H: AsRef<[u8]>>(_heads: impl IntoIterator<Item = H>) -> Option<Vec<i64>> {
    None
}
pub(crate) fn avc_format(config: &AvcConfig) -> Option<(u32, u32)> {
    let &[chroma, depth, ..] = config.ext.as_slice() else {
        return None;
    };
    (chroma & 0xFC == 0xFC && depth & 0xF8 == 0xF8).then_some(((chroma & 3) as u32, (depth & 7) as u32 + 8))
}
pub(crate) fn hevc_format(config: &HevcConfig) -> (u32, u32) {
    (config.chroma_format_idc as u32, config.bit_depth_luma as u32)
}
