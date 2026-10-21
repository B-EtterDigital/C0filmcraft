//! Final presentation timing for encoders that reveal priming only in asynchronous output.
use super::*;

impl<W: Write + Seek> Mp4Writer<W> {
    /// Trim encoder priming and padding. Inputs use the track's media timescale. Call after
    /// writing its samples, before finish; no media outside the written interval is exposed.
    pub fn set_presentation_window(&mut self, track: usize, start: u64, duration: u64) -> Result<()> {
        let track = self.tracks.get_mut(track).ok_or_else(|| Error::Invalid("no such track".into()))?;
        let end = start.checked_add(duration).ok_or_else(|| Error::Invalid("presentation window overflows".into()))?;
        let media_time = i64::try_from(start).map_err(|_| Error::Invalid("presentation start exceeds i64".into()))?;
        if end > track.t.media_duration || track.cfg.timescale == 0 || self.opts.movie_timescale == 0 {
            return Err(Error::Invalid("presentation window exceeds written media or has a zero timescale".into()));
        }
        track.cfg.edits = vec![Edit { segment_duration: rescale(duration, track.cfg.timescale, self.opts.movie_timescale), media_time, media_rate: 0x10000 }];
        Ok(())
    }
}
