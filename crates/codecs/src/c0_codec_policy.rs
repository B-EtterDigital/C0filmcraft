//! Feature boundary for metadata that otherwise instantiated an AAC decoder.
#[cfg(feature = "patent-codecs")]
pub fn aac_core_rate(asc: &[u8]) -> Option<u32> {
    Some(filmcraft_aac::Decoder::new(asc).ok()?.sample_rate())
}
#[cfg(not(feature = "patent-codecs"))]
pub fn aac_core_rate(asc: &[u8]) -> Option<u32> {
    let config = filmcraft_isobmff::AacConfig::from_asc(asc.to_vec());
    (config.sample_rate > 0).then_some(config.sample_rate)
}
