//! Keep the exporter ABI stable without linking a software H.264/AAC implementation.
pub fn pending_loudness_error() -> crate::ExportError {
    crate::ExportError::Unsupported("Loudness normalization cannot measure asynchronous media here; disable loudness normalization for this export".into())
}
pub fn pending_audio_error() -> crate::ExportError {
    crate::ExportError::Unsupported("Media is still loading asynchronously; synchronous audio export is not supported here until its media is ready".into())
}
#[cfg(feature = "patent-codecs")]
pub use filmcraft_h264enc::PassStats;
#[cfg(not(feature = "patent-codecs"))]
#[derive(Clone, Debug)]
pub struct PassStats;
#[cfg(not(feature = "patent-codecs"))]
pub fn aac_factory(
    _format: crate::Format,
    _rate: u32,
    _channels: u32,
    _settings: &crate::ExportSettings,
) -> Option<crate::Result<Box<dyn crate::AudioEncoder>>> {
    Some(Err(crate::ExportError::Unsupported("AAC browser WebCodecs is not supported here".into())))
}
#[cfg(not(feature = "patent-codecs"))]
pub fn h264_factory(
    format: crate::Format,
    _width: u32,
    _height: u32,
    _rate: filmcraft_time::FrameRate,
    _settings: &crate::ExportSettings,
) -> Option<crate::Result<Box<dyn crate::VideoEncoder>>> {
    (format == crate::Format::H264).then(|| Err(crate::ExportError::Unsupported("H.264 browser WebCodecs is not supported here".into())))
}

pub fn default_description() -> &'static str {
    if cfg!(feature = "patent-codecs") {
        "H.264 at the sequence's frame size and rate; bitrate adapts to the frame size (0.2 bits per pixel)"
    } else {
        "Motion JPEG at the sequence's frame size and rate, with PCM audio"
    }
}
pub fn default_settings(mut settings: crate::ExportSettings) -> crate::ExportSettings {
    if !cfg!(feature = "patent-codecs") {
        settings.format = crate::Format::Mjpeg;
    }
    settings
}
