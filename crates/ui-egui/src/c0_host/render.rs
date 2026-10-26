//! `ui.render {sequence?, frame?, scale?}`: the Program monitor's frame as a PNG, composited off
//! screen. No window frame is presented and no window is raised, so an embedding page that is
//! hidden (no animation frames) still answers.

use crate::FilmcraftApp;
use crate::control::{self, Outcome};
use crate::frames::{FrameKey, Target};
use crate::panels::monitor::Which;
use filmcraft_project::ItemId;
use serde_json::{Value, json};

/// Largest frame answered, in pixels. 8K UHD is 33 Mpx; a project can name any size.
const MAX_PIXELS: usize = 64_000_000;
/// Frame-server priority: ahead of thumbnails and prefetch, behind the monitors' own frame (0), which
/// would otherwise replace this job in the queue.
const PRIORITY: u32 = 1;
const KEYS: [&str; 3] = ["sequence", "frame", "scale"];

pub fn handle(app: &mut FilmcraftApp, params: &Value) -> Outcome {
    match render(app, params) {
        Ok(Some(result)) => Outcome::Done(json!({"ok":true,"result":result})),
        Ok(None) => Outcome::Retry("the frame is still rendering; retry ui.render".into()),
        Err(error) => Outcome::Done(json!({"ok":false,"error":error})),
    }
}

/// `Ok(None)`: the frame is queued and not ready yet.
fn render(app: &mut FilmcraftApp, params: &Value) -> Result<Option<Value>, String> {
    let empty = serde_json::Map::new();
    let params = if params.is_null() { &empty } else { params.as_object().ok_or("params must be an object")? };
    if let Some(key) = params.keys().find(|key| !KEYS.contains(&key.as_str())) {
        return Err(format!("unknown parameter {key}"));
    }
    let active = app.session.state.active_sequence;
    let id = match params.get("sequence").filter(|value| !value.is_null()) {
        None => active,
        Some(value) => Some(ItemId(value.as_u64().ok_or("sequence must be an integer id")?)),
    }
    .ok_or("no active sequence")?;
    let rate = app.session.project.sequence(id).ok_or("unknown sequence")?.settings.frame_rate;
    let frame = match params.get("frame").filter(|value| !value.is_null()) {
        None if Some(id) == active => rate.frame_at(app.session.playhead()),
        None => 0,
        Some(value) => value
            .as_i64()
            .filter(|frame| *frame >= 0 && *frame <= rate.frame_at(filmcraft_time::Tick::MAX))
            .ok_or("frame must be a nonnegative integer inside the supported time range")?,
    };
    let scale = match params.get("scale").filter(|value| !value.is_null()) {
        None => crate::panels::monitor_view::view(app, Which::Program).effective_res(false).scale(),
        Some(value) => value
            .as_f64()
            .filter(|scale| scale.is_finite() && *scale >= 0.001 && *scale <= 1.0)
            .map(|scale| scale as f32)
            .ok_or("scale must be a number from 0.001 to 1")?,
    };
    let (width, height) = filmcraft_render::output_size(app.session.project.sequence(id).ok_or("unknown sequence")?, scale);
    if width.checked_mul(height).is_none_or(|pixels| pixels > MAX_PIXELS) {
        return Err(format!("a {width}x{height} frame is too large; lower scale"));
    }
    let key = FrameKey { target: Target::Sequence(id), frame, size: (scale * 1000.0) as u32, revision: app.session.revision, draft: false };
    let Some(image) = app.frames.get(&key) else {
        app.frames.request(key, rate.tick_of(frame), scale, &app.session.project, PRIORITY);
        return Ok(None);
    };
    let (w, h) = (u32::try_from(image.w).map_err(|e| e.to_string())?, u32::try_from(image.h).map_err(|e| e.to_string())?);
    // The cache keeps each pixels alpha since upstreams Transparency Grid (the colour bytes are
    // already over black); the Program monitor uploads its frame opaque by default, and so does this.
    let mut px = image.px.clone();
    px.chunks_exact_mut(4).for_each(|p| p[3] = 255);
    let png = control::encode_png(&px, w, h)?;
    Ok(Some(json!({"pngBase64":control::base64(&png),"width":w,"height":h,"sequence":id.0,"frame":frame,"scale":scale,"presented":false})))
}
