use filmcraft_isobmff::{Brand, Mp4Writer, PcmConfig, SampleEntry, TrackConfig, WriteSample, WriterOptions};
use std::io::Cursor;
#[test]
fn final_encoder_priming_and_padding_are_trimmed_and_hostile_windows_fail() {
    let mut writer = Mp4Writer::new(Cursor::new(Vec::new()), WriterOptions::new(Brand::Mov)).unwrap();
    let track = writer
        .add_track(TrackConfig::new(
            SampleEntry::pcm(PcmConfig { bits: 16, float: false, big_endian: false, signed: true, channels: 1, sample_rate: 48000.0 }),
            48000,
        ))
        .unwrap();
    writer.write_sample(track, WriteSample { data: &[0; 128], duration: 64, composition_offset: 0, is_sync: true }).unwrap();
    assert!(writer.set_presentation_window(usize::MAX, 0, 1).is_err());
    assert!(writer.set_presentation_window(track, u64::MAX, 1).is_err());
    assert!(writer.set_presentation_window(track, 10, 100).is_err());
    writer.set_presentation_window(track, 8, 48).unwrap();
    let bytes = writer.finish().unwrap().into_inner();
    let file = filmcraft_isobmff::open(bytes.as_slice()).unwrap();
    let track = &file.tracks[0];
    assert_eq!(track.edits[0].media_time, 8);
    assert_eq!(track.edits[0].segment_duration, 1);
}
