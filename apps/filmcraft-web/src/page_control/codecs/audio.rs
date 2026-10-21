//! AAC random-access windows, decoded solely by the browser's AudioDecoder.
use super::*;
use filmcraft_frame::{AudioBuffer, VideoFrame};
use filmcraft_isobmff::{CodecConfig, Mp4File, TrackKind};
use filmcraft_media::{FrameRequest, MediaError, MediaInfo, MediaSource, SharedReader, SharedSource};
use filmcraft_time::Tick;
use std::sync::{Arc, Mutex};

const MAX_FRAMES: usize = 1 << 20;
const MAX_PACKETS: usize = 2048;
const MAX_PACKET_BYTES: usize = 1024 * 1024;
const MAX_WINDOW_BYTES: usize = 64 * 1024 * 1024;
fn error(value: impl std::fmt::Display) -> MediaError {
    MediaError::Unsupported(format!("AAC browser WebCodecs: {value}"))
}
struct Window {
    id: u32,
    start: i64,
    end: i64,
    rate: u32,
}
impl Drop for Window {
    fn drop(&mut self) {
        close(self.id, true);
    }
}
struct AudioSource {
    inner: SharedSource,
    reader: SharedReader,
    file: Mp4File,
    tracks: Vec<usize>,
    windows: Mutex<HashMap<usize, Window>>,
}

pub fn wrap(reader: &SharedReader, file: &Mp4File, inner: SharedSource) -> SharedSource {
    let tracks: Vec<usize> = file
        .tracks
        .iter()
        .enumerate()
        .filter(|(_, track)| track.kind == TrackKind::Audio && !track.samples.is_empty() && !track.entries.is_empty() && track.timescale > 0)
        .map(|(i, _)| i)
        .take(filmcraft_media::MAX_AUDIO_STREAMS)
        .collect();
    if !tracks.iter().any(|&i| file.tracks.get(i).and_then(|t| t.entries.first()).is_some_and(|entry| matches!(entry.codec, CodecConfig::Aac(_)))) {
        return inner;
    }
    Arc::new(AudioSource { inner, reader: reader.clone(), file: file.clone(), tracks, windows: Mutex::new(HashMap::new()) })
}

impl AudioSource {
    fn decode(&self, stream: usize, start: i64, frames: usize, rate: u32) -> Result<AudioBuffer, MediaError> {
        let track = self.tracks.get(stream).and_then(|&index| self.file.tracks.get(index)).ok_or(MediaError::NoStream("audio"))?;
        let Some(CodecConfig::Aac(config)) = track.entries.first().map(|entry| &entry.codec) else {
            return self.inner.audio_stream(stream, start, frames, rate);
        };
        if !CAPS.with(|caps| caps.get().2) {
            return Err(error("not supported here (AudioDecoder is unavailable)"));
        }
        if rate == 0 || frames > MAX_FRAMES {
            return Err(error("invalid sample rate or oversized audio request"));
        }
        let channels = config.channels();
        if channels == 0 || channels > 32 {
            return Err(error("unsupported channel layout"));
        }
        if frames.checked_mul(channels as usize).and_then(|size| size.checked_mul(4)).is_none_or(|bytes| bytes > MAX_WINDOW_BYTES) {
            return Err(error("audio output window exceeds 64 MiB"));
        }
        if frames == 0 {
            return Ok(AudioBuffer::silence(rate, channels as usize, 0));
        }
        let end = start.checked_add(i64::try_from(frames).map_err(error)?).ok_or_else(|| error("sample offset overflow"))?;
        let timescale = i64::from(track.timescale.max(1));
        if track.samples.last().is_some_and(|sample| {
            Tick::from_units(start, rate.into()).to_rational_floor(1, timescale)
                >= sample.pts.saturating_add(i64::from(sample.duration)).saturating_add(track.edit_offset)
        }) {
            return Ok(AudioBuffer::silence(rate, channels as usize, frames));
        }
        let mut windows = self.windows.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        if !windows.get(&stream).is_some_and(|window| window.rate == rate && window.start <= start && window.end >= end) {
            let timescale = i64::from(track.timescale.max(1));
            let begin_tick = Tick::from_units(start, rate.into()).to_rational_floor(1, timescale).saturating_sub(track.edit_offset);
            let end_tick = Tick::from_units(end, rate.into()).to_rational_round(1, timescale).saturating_sub(track.edit_offset);
            let first = track.sample_at_pts(begin_tick).unwrap_or(0).saturating_sub(4);
            let last = track.sample_at_pts(end_tick).unwrap_or(track.samples.len().saturating_sub(1)).saturating_add(3).min(track.samples.len());
            if last.saturating_sub(first) > MAX_PACKETS {
                return Err(error("audio window exceeds packet limit"));
            }
            let mut packets = Vec::new();
            let mut total = 0usize;
            for sample in track.samples.get(first..last).ok_or_else(|| error("invalid sample table"))? {
                let size = sample.size as usize;
                total = total.checked_add(size).filter(|sum| *sum <= MAX_WINDOW_BYTES).ok_or_else(|| error("audio window exceeds 64 MiB"))?;
                if size > MAX_PACKET_BYTES {
                    return Err(error("AAC packet exceeds 1 MiB"));
                }
                let mut bytes = vec![0; size];
                self.reader.read_at(sample.offset, &mut bytes).map_err(error)?;
                let timestamp = Tick::from_rational(sample.pts.saturating_add(track.edit_offset), 1, timescale).seconds() * 1e6;
                let duration = f64::from(sample.duration) * 1e6 / timescale as f64;
                packets.push((bytes, timestamp, duration));
            }
            let state = create_audio_decoder(&config.asc, config.sample_rate, channels).map_err(|e| error(message(e)))?;
            for (bytes, timestamp, duration) in packets {
                if let Err(failure) = decode_audio(&state, &bytes, timestamp, duration) {
                    let _ = close_audio_decoder(&state);
                    return Err(error(message(failure)));
                }
            }
            if let Err(failure) = flush_audio_decoder(&state) {
                let _ = close_audio_decoder(&state);
                return Err(error(message(failure)));
            }
            let id = store(state).map_err(error)?;
            windows.insert(stream, Window { id, start, end, rate });
        }
        let id = windows.get(&stream).ok_or_else(|| error("decode window disappeared"))?.id;
        let value = with(id, |state| read_audio(state, start as f64, frames as u32, rate)).map_err(error)?;
        if value.is_null() {
            filmcraft_media::pending::mark();
            return Err(MediaError::Decode("AAC WebCodecs: decoding".into()));
        }
        let pcm = js_sys::Float32Array::new(&value).to_vec();
        let expected = frames.checked_mul(channels as usize).ok_or_else(|| error("audio buffer size overflow"))?;
        if pcm.len() != expected {
            return Err(error("browser returned an invalid audio buffer"));
        }
        let channels = if frames == 0 { vec![Vec::new(); channels as usize] } else { pcm.chunks_exact(frames).map(|channel| channel.to_vec()).collect() };
        Ok(AudioBuffer { sample_rate: rate, channels })
    }
}
impl MediaSource for AudioSource {
    fn info(&self) -> &MediaInfo {
        self.inner.info()
    }
    fn video_frame(&self, request: FrameRequest) -> Result<Arc<VideoFrame>, MediaError> {
        self.inner.video_frame(request)
    }
    fn audio(&self, start: i64, frames: usize, rate: u32) -> Result<AudioBuffer, MediaError> {
        self.decode(0, start, frames, rate)
    }
    fn audio_stream(&self, stream: usize, start: i64, frames: usize, rate: u32) -> Result<AudioBuffer, MediaError> {
        self.decode(stream, start, frames, rate)
    }
}
