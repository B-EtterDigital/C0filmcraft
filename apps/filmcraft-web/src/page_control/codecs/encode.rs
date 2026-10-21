use super::*;
use filmcraft_export::{AudioEncoder, EncodedPacket, EncoderFrame, ExportError, ExportSettings, Format, Result, VideoEncoder};
use filmcraft_isobmff::{AvcConfig, SampleEntry};
use filmcraft_time::FrameRate;
fn error(message: String) -> ExportError {
    ExportError::Unsupported(format!("browser WebCodecs: {message}"))
}

struct Video {
    id: u32,
    width: u16,
    height: u16,
    num: u32,
    den: u32,
}
impl Video {
    fn entry(&self) -> Option<SampleEntry> {
        Some(SampleEntry::avc(AvcConfig::parse(&description(self.id)).ok()?, self.width, self.height))
    }
    fn drain(&self) -> Result<Vec<EncodedPacket>> {
        Ok(packets(self.id)
            .map_err(error)?
            .into_iter()
            .map(|packet| EncodedPacket {
                data: bytes(&get(&packet, "bytes")),
                key: get(&packet, "key").as_bool().unwrap_or(false),
                duration: self.den,
                composition_offset: 0,
            })
            .collect())
    }
}
impl VideoEncoder for Video {
    fn ready(&mut self) -> Result<bool> {
        with(self.id, encoder_ready).map_err(error)
    }
    fn finish_ready(&mut self) -> Result<bool> {
        let ready = with(self.id, encoder_finish_ready).map_err(error)?;
        if ready && self.entry().is_none() {
            return Err(error("encoder did not provide an AVC configuration".into()));
        }
        Ok(ready)
    }
    fn sample_entry(&self) -> SampleEntry {
        self.entry().unwrap_or_else(|| SampleEntry::avc(AvcConfig::new(Vec::new(), Vec::new(), 4), self.width, self.height))
    }
    fn final_sample_entry(&self) -> Option<SampleEntry> {
        self.entry()
    }
    fn timescale(&self) -> u32 {
        self.num
    }
    fn encode(&mut self, frame: &EncoderFrame) -> Result<Vec<EncodedPacket>> {
        if frame.hdr.is_some() {
            return Err(error("HDR H.264 export is not supported by this browser adapter; select SDR".into()));
        }
        let required =
            usize::from(self.width).checked_mul(usize::from(self.height)).and_then(|n| n.checked_mul(4)).ok_or_else(|| error("frame size overflow".into()))?;
        if frame.width != u32::from(self.width) || frame.height != u32::from(self.height) || frame.rgba.len() != required {
            return Err(error("invalid RGBA frame dimensions".into()));
        }
        let index = u32::try_from(frame.index).map_err(|_| error("video exceeds browser frame limit".into()))?;
        with(self.id, |state| encode_video(state, frame.rgba, index)).map_err(error)?;
        self.drain()
    }
    fn flush(&mut self) -> Result<Vec<EncodedPacket>> {
        self.drain()
    }
}
impl Drop for Video {
    fn drop(&mut self) {
        close(self.id, false);
    }
}

pub fn video_factory(format: Format, width: u32, height: u32, rate: FrameRate, settings: &ExportSettings) -> Option<Result<Box<dyn VideoEncoder>>> {
    if format != Format::H264 || !CAPS.with(|caps| caps.get().0) {
        return None;
    }
    Some((|| {
        let width = u16::try_from(width).map_err(|_| error("width exceeds browser limit".into()))?;
        let height = u16::try_from(height).map_err(|_| error("height exceeds browser limit".into()))?;
        let num = u32::try_from(rate.num).ok().filter(|n| *n > 0).ok_or_else(|| error("invalid frame rate".into()))?;
        let den = u32::try_from(rate.den).ok().filter(|n| *n > 0).ok_or_else(|| error("invalid frame rate".into()))?;
        let bitrate = settings.bitrate_kbps.clamp(100, 200_000).saturating_mul(1000);
        let profile = match settings.h264_profile {
            filmcraft_export::H264Profile::Baseline => 66,
            filmcraft_export::H264Profile::Main => 77,
            filmcraft_export::H264Profile::High => 100,
        };
        const DEFAULT_LEVEL: u8 = 51;
        let codec = format!("avc1.{profile:02x}00{:02x}", settings.h264_level.unwrap_or(DEFAULT_LEVEL));
        let bitrate_mode = match settings.bitrate_mode {
            filmcraft_export::BitrateMode::Cbr => "constant",
            filmcraft_export::BitrateMode::Vbr1Pass => "variable",
            _ => return Err(error("browser H.264 supports one-pass VBR or CBR; CRF and two-pass encoding are unavailable".into())),
        };
        let options = crate::api::to_js(&serde_json::json!({"codec":codec,"keyInterval":settings.keyframe_distance.unwrap_or(0),"bitrateMode":bitrate_mode}));
        let state = create_video(width.into(), height.into(), num, den, bitrate, &options).map_err(|e| error(message(e)))?;
        let id = store(state).map_err(error)?;
        Ok(Box::new(Video { id, width, height, num, den }) as Box<dyn VideoEncoder>)
    })())
}

struct Audio {
    id: u32,
    rate: u32,
    channels: u32,
    asc: Vec<u8>,
}
impl Audio {
    fn drain(&self) -> Result<Vec<(Vec<u8>, u32)>> {
        Ok(packets(self.id)
            .map_err(error)?
            .into_iter()
            .map(|packet| {
                let micros = get(&packet, "duration").as_f64().filter(|n| n.is_finite() && *n > 0.0).unwrap_or(1024.0 * 1e6 / f64::from(self.rate));
                (bytes(&get(&packet, "bytes")), (micros * f64::from(self.rate) / 1e6).round().max(1.0).min(f64::from(u32::MAX)) as u32)
            })
            .collect())
    }
}
impl AudioEncoder for Audio {
    fn asynchronous(&self) -> bool {
        true
    }
    fn ready(&mut self) -> Result<bool> {
        with(self.id, encoder_ready).map_err(error)
    }
    fn finish_ready(&mut self) -> Result<bool> {
        with(self.id, encoder_finish_ready).map_err(error)
    }
    fn sample_entry(&self) -> SampleEntry {
        SampleEntry::aac(self.asc.clone(), self.channels, self.rate)
    }
    fn final_sample_entry(&self) -> Option<SampleEntry> {
        let asc = description(self.id);
        Some(SampleEntry::aac(if asc.is_empty() { self.asc.clone() } else { asc }, self.channels, self.rate))
    }
    fn encode(&mut self, planar: &[Vec<f32>]) -> Result<Vec<Vec<u8>>> {
        let frames = planar.first().map_or(0, Vec::len);
        if frames == 0 {
            return Ok(Vec::new());
        }
        if planar.len() != self.channels as usize || frames > 1 << 20 || planar.iter().any(|channel| channel.len() != frames) {
            return Err(error("invalid audio block dimensions".into()));
        }
        let data: Vec<f32> = planar.iter().flatten().copied().collect();
        with(self.id, |state| encode_audio(state, &data, frames as u32)).map_err(error)?;
        Ok(self.drain()?.into_iter().map(|(bytes, _)| bytes).collect())
    }
    fn flush(&mut self) -> Result<Vec<Vec<u8>>> {
        Ok(self.drain()?.into_iter().map(|(bytes, _)| bytes).collect())
    }
    fn flush_timed(&mut self) -> Result<Vec<(Vec<u8>, u32)>> {
        self.drain()
    }
    fn priming(&self) -> u32 {
        with(self.id, |state| Ok(get(state, "firstTimestamp").as_f64().filter(|value| value.is_finite()).unwrap_or(0.0)))
            .map(|timestamp| (-timestamp * f64::from(self.rate) / 1e6).round().max(0.0).min(f64::from(u32::MAX)) as u32)
            .unwrap_or(0)
    }
    fn final_timing(&self) -> Option<(u32, u64)> {
        let samples = with(self.id, |state| Ok(get(state, "samples").as_f64())).ok()??;
        if !samples.is_finite() || samples < 0.0 || samples > u64::MAX as f64 {
            return None;
        }
        Some((self.priming(), samples as u64))
    }
    fn frame_size(&self) -> u32 {
        1024
    }
}
impl Drop for Audio {
    fn drop(&mut self) {
        close(self.id, false);
    }
}
pub fn audio_factory(format: Format, rate: u32, channels: u32, settings: &ExportSettings) -> Option<Result<Box<dyn AudioEncoder>>> {
    if !format.is_mp4_video() || !CAPS.with(|caps| caps.get().1) {
        return None;
    }
    Some((|| {
        const RATES: [u32; 13] = [96000, 88200, 64000, 48000, 44100, 32000, 24000, 22050, 16000, 12000, 11025, 8000, 7350];
        let index = RATES.iter().position(|sample_rate| *sample_rate == rate).ok_or_else(|| error("AAC sample rate is unsupported".into()))? as u8;
        if !(1..=6).contains(&channels) {
            return Err(error("AAC export supports 1 to 6 channels".into()));
        }
        let asc = vec![(2 << 3) | (index >> 1), ((index & 1) << 7) | ((channels as u8) << 3)];
        let state = create_audio(rate, channels, settings.audio.bitrate_kbps.clamp(32, 512).saturating_mul(1000)).map_err(|e| error(message(e)))?;
        let id = store(state).map_err(error)?;
        Ok(Box::new(Audio { id, rate, channels, asc }) as Box<dyn AudioEncoder>)
    })())
}
