//! A pending browser pull must not consume audio that has not arrived.
use filmcraft_export::{ExportSettings, Exporter, Format, OutputSink, Progress, Step};
use filmcraft_frame::{AudioBuffer, VideoFrame};
use filmcraft_media::{FrameRequest, Generator, MediaError, MediaInfo, MediaSource, SharedSource, generators::GeneratorSource};
use filmcraft_project::{ItemId, ItemKind, Label, MediaClip, MediaRef, Project, SequenceSettings, TrackKind};
use filmcraft_time::{FrameRate, TICKS_PER_SECOND, Tick, TimeRange};
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
};

struct Source {
    tone: GeneratorSource,
    pending: AtomicBool,
}
impl MediaSource for Source {
    fn info(&self) -> &MediaInfo {
        self.tone.info()
    }
    fn video_frame(&self, request: FrameRequest) -> Result<Arc<VideoFrame>, MediaError> {
        self.tone.video_frame(request)
    }
    fn audio(&self, start: i64, frames: usize, rate: u32) -> Result<AudioBuffer, MediaError> {
        if self.pending.swap(false, Ordering::SeqCst) {
            filmcraft_media::pending::mark();
            return Err(MediaError::Decode("pending test audio".into()));
        }
        self.tone.audio(start, frames, rate)
    }
}
fn fixture(pending: bool) -> (Arc<Project>, ItemId, ItemId, SharedSource) {
    let mut project = Project::new("Async audio");
    let rate = FrameRate::FPS_24;
    let duration = Tick(TICKS_PER_SECOND);
    let generator = GeneratorSource::new(Generator::Tone { hz: 440.0, db: -6.0 }, 8, 8, rate, duration);
    let id = project.add_item(
        "Tone",
        Label::Iris,
        ItemKind::Media(MediaClip {
            media: MediaRef::Generator(generator.generator.clone()),
            info: generator.info().clone(),
            interpret: Default::default(),
            mark_in: None,
            mark_out: None,
            markers: Vec::new(),
            offline: false,
            proxy: None,
            identity: None,
        }),
        None,
    );
    let sequence = project.new_sequence("Sequence", SequenceSettings { width: 8, height: 8, frame_rate: rate, ..Default::default() }, 1, 1, None);
    let clip = project.make_track_item(id, TrackKind::Audio, Tick::ZERO, TimeRange::new(Tick::ZERO, duration), rate).unwrap();
    project.sequence_mut(sequence).unwrap().audio_tracks[0].items.push(clip);
    (Arc::new(project), sequence, id, Arc::new(Source { tone: generator, pending: AtomicBool::new(pending) }))
}
fn run(pending: bool) -> (Vec<u8>, usize) {
    let _ = filmcraft_media::pending::take();
    let (project, sequence, id, source) = fixture(pending);
    let output = Arc::new(Mutex::new(Vec::new()));
    let store = output.clone();
    let settings = ExportSettings {
        format: Format::Mjpeg,
        path: "async.mov".into(),
        sink: Some(OutputSink(Arc::new(move |_, bytes| {
            *store.lock().unwrap() = bytes;
            Ok(())
        }))),
        ..Default::default()
    };
    let progress = Progress::default();
    let mut exporter = Exporter::new(project, sequence, &settings, &progress).unwrap();
    exporter.set_batch(16);
    let provider = move |item| if item == id { Some(source.clone()) } else { None };
    let mut waits = 0;
    let mut done = false;
    for _ in 0..100 {
        match exporter.step(&provider, &progress).unwrap() {
            Step::Done(_) => {
                done = true;
                break;
            }
            Step::Pending => waits += 1,
            Step::Progress => {}
        }
    }
    assert!(done, "export never completed");
    let bytes = output.lock().unwrap().clone();
    (bytes, waits)
}
#[test]
fn a_pending_audio_pull_retries_without_losing_the_start_of_the_mix() {
    let (expected, _) = run(false);
    let (actual, waits) = run(true);
    assert_eq!(waits, 1);
    assert!(!actual.is_empty());
    assert_eq!(actual, expected);
}
#[test]
fn pending_loudness_measurement_fails_clearly_instead_of_looping() {
    let _ = filmcraft_media::pending::take();
    let (project, sequence, id, source) = fixture(true);
    let mut settings = ExportSettings { format: Format::Mjpeg, path: "unused.mov".into(), ..Default::default() };
    settings.effects.loudness.enabled = true;
    let progress = Progress::default();
    let mut exporter = Exporter::new(project, sequence, &settings, &progress).unwrap();
    let provider = move |item| if item == id { Some(source.clone()) } else { None };
    let error = match exporter.step(&provider, &progress) {
        Err(error) => error.to_string(),
        Ok(_) => panic!("pending loudness must return an actionable error"),
    };
    assert!(error.contains("disable loudness normalization"));
    let _ = filmcraft_media::pending::take();
}

#[test]
fn synchronous_audio_export_rejects_pending_media_instead_of_writing_a_truncated_file() {
    let _ = filmcraft_media::pending::take();
    let (project, sequence, id, source) = fixture(true);
    let output = Arc::new(AtomicBool::new(false));
    let store = output.clone();
    let settings = ExportSettings {
        format: Format::Wav,
        path: "unused.wav".into(),
        sink: Some(OutputSink(Arc::new(move |_, _| {
            store.store(true, Ordering::SeqCst);
            Ok(())
        }))),
        ..Default::default()
    };
    let provider = move |item| if item == id { Some(source.clone()) } else { None };
    let error = filmcraft_export::export(&project, sequence, &settings, &provider, &Progress::default()).unwrap_err();
    assert!(error.to_string().contains("synchronous audio export"));
    assert!(!output.load(Ordering::SeqCst));
    assert!(!filmcraft_media::pending::is_set());
}

/// Opaque AAC test packets exercise container timing, not AAC decoding. The encoder reveals
/// priming after its async flush, as browser encoders do; its tail contains padding.
struct DelayedAudio {
    samples: u64,
    pending: usize,
    started: bool,
    drained: bool,
}
impl filmcraft_export::AudioEncoder for DelayedAudio {
    fn asynchronous(&self) -> bool {
        true
    }
    fn finish_ready(&mut self) -> filmcraft_export::Result<bool> {
        let ready = self.drained;
        self.drained = true;
        Ok(ready)
    }
    fn sample_entry(&self) -> filmcraft_isobmff::SampleEntry {
        filmcraft_isobmff::SampleEntry::aac(vec![0x11, 0x90], 2, 48000)
    }
    fn encode(&mut self, planar: &[Vec<f32>]) -> filmcraft_export::Result<Vec<Vec<u8>>> {
        let count = planar.first().map_or(0, Vec::len);
        self.samples += count as u64;
        self.pending += count;
        let mut packets = Vec::new();
        if !self.started {
            self.started = true;
            packets.push(vec![0]);
        }
        while self.pending >= 1024 {
            self.pending -= 1024;
            packets.push(vec![0]);
        }
        Ok(packets)
    }
    fn flush(&mut self) -> filmcraft_export::Result<Vec<Vec<u8>>> {
        if self.pending == 0 {
            Ok(Vec::new())
        } else {
            self.pending = 0;
            Ok(vec![vec![0]])
        }
    }
    fn priming(&self) -> u32 {
        0
    }
    fn final_timing(&self) -> Option<(u32, u64)> {
        Some((1024, self.samples))
    }
    fn frame_size(&self) -> u32 {
        1024
    }
}
#[test]
fn async_flush_waits_and_final_priming_and_padding_are_trimmed() {
    let _ = filmcraft_media::pending::take();
    filmcraft_export::register_audio_encoder(|format, _, _, _| {
        (format == Format::Mjpeg)
            .then(|| Ok(Box::new(DelayedAudio { samples: 0, pending: 0, started: false, drained: false }) as Box<dyn filmcraft_export::AudioEncoder>))
    });
    let (project, sequence, id, source) = fixture(false);
    let output = Arc::new(Mutex::new(Vec::new()));
    let store = output.clone();
    let mut settings = ExportSettings {
        format: Format::Mjpeg,
        path: "async-encoder.mov".into(),
        sink: Some(OutputSink(Arc::new(move |_, bytes| {
            *store.lock().unwrap() = bytes;
            Ok(())
        }))),
        ..Default::default()
    };
    settings.audio.codec = filmcraft_export::AudioCodec::Aac;
    let progress = Progress::default();
    let mut exporter = Exporter::new(project, sequence, &settings, &progress).unwrap();
    exporter.set_batch(16);
    let provider = move |item| if item == id { Some(source.clone()) } else { None };
    let mut waits = 0;
    let mut done = false;
    for _ in 0..100 {
        match exporter.step(&provider, &progress).unwrap() {
            Step::Done(_) => {
                done = true;
                break;
            }
            Step::Pending => waits += 1,
            Step::Progress => {}
        }
    }
    assert!(done);
    assert_eq!(waits, 1);
    let bytes = output.lock().unwrap().clone();
    let file = filmcraft_isobmff::open(bytes.as_slice()).unwrap();
    let audio = file.tracks.iter().find(|track| track.kind == filmcraft_isobmff::TrackKind::Audio).unwrap();
    assert_eq!(audio.edits[0].media_time, 1024);
    assert_eq!(audio.edits[0].segment_duration, 1000);
    assert_eq!(audio.samples.iter().map(|sample| u64::from(sample.duration)).sum::<u64>(), 49152);
}
