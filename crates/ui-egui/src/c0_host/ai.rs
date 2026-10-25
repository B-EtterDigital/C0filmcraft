//! `c0.selection` and `c0.place`: what the C0VIBE AI actions read from the editor and put back.
//!
//! Not upstream's: part of the C0VIBE host shell (`c0/filmcraft-host/README.md`). Every edit goes
//! through the engine's own commands (`file.import`, `timeline.place`, `captions.*`), so history,
//! dirty state and saving behave as for a user edit, and a whole placement is ONE undo step
//! ([`filmcraft_engine::Session::edit_group`]; a failure rolls everything back).
//!
//! | Method | Params | Result |
//! |---|---|---|
//! | `c0.selection` | `{}` | `{kind:"timeline", sequenceName, playheadMs, durationMs, inMs?, outMs?, selectedClips:[{id,name,kind,startMs,endMs,sourcePath?}], frame?}` |
//! | `c0.place` | exactly one of `{media:{base64,mime,name}, toTimeline?:{track,atMs?}}`, `{captions:[{startMs,endMs,text}], name?}` | `{placed:"media",itemId,name,clipIds,...}` or `{placed:"captions",trackId,count,captionIds}` |

use crate::FilmcraftApp;
use crate::control;
use crate::frames::{FrameKey, Target};
use base64::{Engine as _, engine::general_purpose::STANDARD};
use filmcraft_engine::{EngineError, Session};
use filmcraft_project::{ItemId, ItemKind, TrackKind};
use filmcraft_time::Tick;
use serde_json::{Value, json};

/// The biggest media file a request may carry, decoded.
pub const MAX_MEDIA_BYTES: usize = 40 * 1024 * 1024;
/// The longest side of the frame `c0.selection` returns.
pub const MAX_FRAME_SIDE: f32 = 1280.0;
const MAX_CAPTIONS: usize = 2000;
const MAX_CAPTION_CHARS: usize = 2000;
const MAX_TIME_MS: u64 = 100 * 60 * 60 * 1000;
const MAX_SELECTED: usize = 500;
/// Frame-server priority, as `ui.render`.
const PRIORITY: u32 = 1;

pub fn methods() -> [&'static str; 2] {
    ["c0.selection", "c0.place"]
}

fn ms(t: Tick) -> i64 {
    t.to_units_floor(1000)
}
fn ticks(ms: u64) -> Tick {
    Tick::from_units(ms as i64, 1000)
}

// ---------- c0.selection ----------

/// `Ok(None)`: the program frame is queued and not ready yet (the caller retries, as for `ui.render`).
pub fn selection(app: &mut FilmcraftApp) -> Result<Option<Value>, String> {
    let s = &app.session;
    let id = s.state.active_sequence.ok_or("no sequence open")?;
    let name = s.project.item(id).map(|i| i.name.clone()).unwrap_or_default();
    let seq = s.project.sequence(id).ok_or("no sequence open")?;
    let mut clips = Vec::new();
    for clip in s.state.selection.iter().take(MAX_SELECTED) {
        let Some((track, item)) = seq.all_tracks().find_map(|t| t.item(*clip).map(|i| (t, i))) else { continue };
        let source = s.project.item(item.item);
        let kind = match (track.kind, source.map(|i| &i.kind)) {
            (TrackKind::Audio, _) => "audio",
            (_, Some(ItemKind::Graphic { .. })) => "title",
            (_, Some(ItemKind::Media(m))) if m.info.kind == filmcraft_media::MediaKind::Still => "image",
            _ => "video",
        };
        let mut entry = json!({"id": clip.0, "name": item.name, "kind": kind, "startMs": ms(item.start), "endMs": ms(item.end())});
        if let Some(ItemKind::Media(m)) = source.map(|i| &i.kind)
            && let filmcraft_project::MediaRef::File { path } = &m.media
        {
            entry["sourcePath"] = json!(path);
        }
        clips.push(entry);
    }
    for caption in s.state.caption_selection.iter().take(MAX_SELECTED.saturating_sub(clips.len())) {
        if let Some((_, c)) = seq.find_caption(*caption) {
            clips.push(json!({"id": caption.0, "name": c.text, "kind": "title", "startMs": ms(c.start), "endMs": ms(c.start + c.duration)}));
        }
    }
    let rate = seq.settings.frame_rate;
    let longest = seq.settings.width.max(seq.settings.height).max(1) as f32;
    let scale = (MAX_FRAME_SIDE / longest).clamp(0.001, 1.0);
    let mut out = json!({"kind": "timeline", "sequenceName": name, "playheadMs": ms(s.playhead()), "durationMs": ms(seq.duration()), "selectedClips": clips});
    if let Some(t) = seq.mark_in {
        out["inMs"] = json!(ms(t));
    }
    if let Some(t) = seq.mark_out {
        out["outMs"] = json!(ms(t));
    }
    let key = FrameKey { target: Target::Sequence(id), frame: rate.frame_at(s.playhead()), size: (scale * 1000.0) as u32, revision: s.revision, draft: false };
    let Some(image) = app.frames.get(&key) else {
        app.frames.request(key, s.playhead(), scale, &s.project, PRIORITY);
        return Ok(None);
    };
    let (w, h) = (u32::try_from(image.w).map_err(|e| e.to_string())?, u32::try_from(image.h).map_err(|e| e.to_string())?);
    let png = control::encode_png(&image.px, w, h)?;
    out["frame"] = json!(control::base64(&png));
    Ok(Some(out))
}

// ---------- c0.place ----------

pub fn place(app: &mut FilmcraftApp, p: &Value) -> Result<Value, String> {
    let kinds: Vec<&str> = ["media", "captions"].into_iter().filter(|k| p.get(*k).is_some_and(|v| !v.is_null())).collect();
    match kinds.as_slice() {
        ["media"] => place_media(app, p),
        ["captions"] => place_captions(app, p),
        [] => Err("nothing to place: give `media` or `captions`".into()),
        _ => Err("place one thing at a time: give only one of `media` or `captions`".into()),
    }
}

/// Next to a saved project, else in the scratch folder.
fn media_dir(s: &Session) -> String {
    let base = s.path.as_deref().and_then(|x| std::path::Path::new(x).parent()).filter(|d| !d.as_os_str().is_empty()).map(std::path::Path::to_path_buf);
    base.unwrap_or_else(|| filmcraft_engine::temp_dir().join("FilmCraft")).join("AI Media").to_string_lossy().into_owned()
}

fn run_group<T>(app: &mut FilmcraftApp, label: &str, f: impl FnOnce(&mut Session) -> Result<T, String>) -> Result<T, String> {
    let was_dirty = app.session.is_dirty();
    let result = app.session.edit_group(label, |s| f(s).map_err(EngineError::Other)).map_err(|e| e.to_string());
    if result.is_err() && !was_dirty {
        // The rollback restored the project exactly; it must not leave the document marked changed.
        app.session.saved_revision = app.session.revision;
    }
    result
}

fn sniff(mime: &str, b: &[u8]) -> bool {
    match mime {
        "audio/wav" => b.len() > 12 && &b[..4] == b"RIFF" && &b[8..12] == b"WAVE",
        "audio/mpeg" => b.starts_with(b"ID3") || (b.len() > 2 && b[0] == 0xff && b[1] & 0xe0 == 0xe0),
        "video/mp4" => b.len() > 12 && &b[4..8] == b"ftyp",
        "image/png" => b.starts_with(&[0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a]),
        "image/jpeg" => b.starts_with(&[0xff, 0xd8, 0xff]),
        _ => false,
    }
}

fn extension(mime: &str) -> Option<&'static [&'static str]> {
    Some(match mime {
        "audio/wav" => &[".wav"],
        "audio/mpeg" => &[".mp3"],
        "video/mp4" => &[".mp4"],
        "image/png" => &[".png"],
        "image/jpeg" => &[".jpg", ".jpeg"],
        _ => return None,
    })
}

/// A file name for the media: the given one with the extension its type needs, numbered if taken.
fn file_name(s: &Session, dir: &str, name: &str, exts: &[&str]) -> String {
    let lower = name.to_lowercase();
    let name = if exts.iter().any(|e| lower.ends_with(e)) { name.to_owned() } else { format!("{name}{}", exts[0]) };
    let dot = name.rfind('.').unwrap_or(name.len());
    let (stem, ext) = name.split_at(dot);
    // A trailing number would let the importer read "shot2.png" as the start of an image sequence.
    let stem = if stem.ends_with(|c: char| c.is_ascii_digit()) { format!("{stem}_") } else { stem.to_owned() };
    let taken = |file: &str| {
        s.project.items.values().any(|i| i.name == file) || s.services.file_size(&std::path::Path::new(dir).join(file).to_string_lossy()).is_ok()
    };
    let first = format!("{stem}{ext}");
    if !taken(&first) {
        return first;
    }
    (2..100_000).map(|n| format!("{stem} ({n}){ext}")).find(|f| !taken(f)).unwrap_or(first)
}

/// The first unlocked track of `kind` with nothing in `start..end`; a track is added when all are busy.
fn free_track(s: &mut Session, kind: TrackKind, start: Tick, end: Tick) -> Result<filmcraft_project::TrackId, String> {
    let seq = s.active_sequence().ok_or("no sequence open")?;
    let tracks = match kind {
        TrackKind::Video => &seq.video_tracks,
        TrackKind::Audio => &seq.audio_tracks,
    };
    if let Some(t) = tracks.iter().find(|t| !t.locked && !t.items.iter().any(|i| i.start < end && i.end() > start)) {
        return Ok(t.id);
    }
    let (params, key) = match kind {
        TrackKind::Video => (json!({"video": 1}), "video"),
        TrackKind::Audio => (json!({"video": 0, "audio": 1}), "audio"),
    };
    let added = s.execute("sequence.addTracks", params).map_err(|e| e.to_string())?;
    added[key][0].as_u64().map(filmcraft_project::TrackId).ok_or_else(|| "no track could be added".to_string())
}

struct Media {
    bytes: Vec<u8>,
    mime: String,
    name: String,
    track: Option<TrackKind>,
    at: Option<Tick>,
}

fn parse_media(p: &Value) -> Result<Media, String> {
    let m = p["media"].as_object().ok_or("`media` must be an object {base64, mime, name}")?;
    let mime = m.get("mime").and_then(Value::as_str).ok_or("`media.mime` is required")?;
    extension(mime).ok_or("`media.mime` must be audio/wav, audio/mpeg, video/mp4, image/png or image/jpeg")?;
    let data = m.get("base64").and_then(Value::as_str).ok_or("`media.base64` must be base64 text")?;
    if data.len() > MAX_MEDIA_BYTES / 3 * 4 + 1024 {
        return Err("the media is larger than 40 MB".into());
    }
    let bytes = STANDARD.decode(data).ok().filter(|b| !b.is_empty()).ok_or("`media.base64` is not valid base64")?;
    if bytes.len() > MAX_MEDIA_BYTES {
        return Err("the media is larger than 40 MB".into());
    }
    if !sniff(mime, &bytes) {
        return Err(format!("the data is not a readable {mime} file"));
    }
    let name = match m.get("name") {
        Some(Value::String(n)) if !n.trim().is_empty() && n.len() <= super::MAX_NAME_BYTES && !n.contains(['/', '\\', '\0']) && n.trim() != "." && n.trim() != ".." => {
            n.trim().to_owned()
        }
        _ => return Err("`media.name` must be a short file name".into()),
    };
    let (mut track, mut at) = (None, None);
    match p.get("toTimeline") {
        None | Some(Value::Null) => {}
        Some(Value::Object(t)) => {
            track = Some(match t.get("track").and_then(Value::as_str) {
                Some("video") => TrackKind::Video,
                Some("audio") => TrackKind::Audio,
                _ => return Err("`toTimeline.track` must be \"video\" or \"audio\"".into()),
            });
            at = match t.get("atMs") {
                None | Some(Value::Null) => None,
                Some(v) => Some(ticks(v.as_u64().filter(|n| *n <= MAX_TIME_MS).ok_or("`toTimeline.atMs` must be a whole number of milliseconds, 0 or more")?)),
            };
            match (track, mime.split('/').next()) {
                (Some(TrackKind::Video), Some("audio")) => return Err("audio has no picture: use track \"audio\"".into()),
                (Some(TrackKind::Audio), Some("image")) => return Err("a picture has no sound: use track \"video\"".into()),
                _ => {}
            }
        }
        Some(_) => return Err("`toTimeline` must be an object {track, atMs?}".into()),
    }
    Ok(Media { bytes, mime: mime.to_owned(), name, track, at })
}

fn place_media(app: &mut FilmcraftApp, p: &Value) -> Result<Value, String> {
    let media = parse_media(p)?;
    if media.track.is_some() && app.session.active_sequence().is_none() {
        return Err("no sequence open: open or create one to place media on the timeline".into());
    }
    let bin = app.import_bin().0;
    let exts = extension(&media.mime).unwrap_or(&[".bin"]);
    let (dir, file, path) = {
        let s = &app.session;
        // The browser build keeps picked files in its virtual /files folder; elsewhere the scratch folder.
        let dir = if cfg!(target_arch = "wasm32") { "/files".to_owned() } else { media_dir(s) };
        let file = file_name(s, &dir, &media.name, exts);
        let path = std::path::Path::new(&dir).join(&file).to_string_lossy().into_owned();
        (dir, file, path)
    };
    let label = if media.track.is_some() { "Place AI Media" } else { "Import AI Media" };
    run_group(app, label, |s| {
        if !cfg!(target_arch = "wasm32") {
            std::fs::create_dir_all(&dir).map_err(|e| format!("{dir}: {e}"))?;
        }
        s.services.write_file(&path, &media.bytes).map_err(|e| format!("{path}: {e}"))?;
        // The same command a user's import runs, so the page build's own decoders and limits apply.
        let imported = s.execute("file.import", json!({"paths": [path], "bin": bin})).map_err(|e| e.to_string())?;
        let item = imported["items"].as_array().and_then(|a| a.first()).and_then(Value::as_u64).map(ItemId);
        let Some(item) = item else {
            let errors: Vec<&str> = imported["errors"].as_array().map(|a| a.iter().filter_map(Value::as_str).collect()).unwrap_or_default();
            return Err(if errors.is_empty() { "the media could not be imported".into() } else { errors.join("; ") });
        };
        let mut out = json!({"placed": "media", "itemId": item.0, "name": file, "mime": media.mime, "clipIds": []});
        let Some(kind) = media.track else { return Ok(out) };
        let at = media.at.unwrap_or(s.playhead());
        let pi = s.project.item(item).ok_or("the imported item is gone")?;
        let (has_video, has_audio, dur) = (pi.has_video(), pi.has_audio(), pi.duration());
        let mut place = json!({"item": item.0, "time": at.0});
        match kind {
            TrackKind::Video => {
                if !has_video {
                    return Err("this media has no picture: use track \"audio\"".into());
                }
                place["track"] = json!(free_track(s, TrackKind::Video, at, at + dur)?.0);
                if has_audio {
                    place["audioTrack"] = json!(free_track(s, TrackKind::Audio, at, at + dur)?.0);
                }
            }
            TrackKind::Audio => {
                if !has_audio {
                    return Err("this media has no sound: use track \"video\"".into());
                }
                place["track"] = json!(free_track(s, TrackKind::Audio, at, at + dur)?.0);
            }
        }
        let placed = s.execute("timeline.place", place).map_err(|e| e.to_string())?;
        let clips: Vec<filmcraft_project::ClipId> = placed["clips"].as_array().map(|a| a.iter().filter_map(Value::as_u64).map(filmcraft_project::ClipId).collect()).unwrap_or_default();
        s.state.selection = clips.clone();
        out["clipIds"] = json!(clips.iter().map(|c| c.0).collect::<Vec<_>>());
        out["track"] = json!(if kind == TrackKind::Video { "video" } else { "audio" });
        let start = s.active_sequence().and_then(|q| q.all_tracks().flat_map(|t| &t.items).find(|i| clips.first() == Some(&i.id)).map(|i| i.start));
        out["atMs"] = json!(ms(start.unwrap_or(at)));
        Ok(out)
    })
}

fn place_captions(app: &mut FilmcraftApp, p: &Value) -> Result<Value, String> {
    let items = p["captions"].as_array().ok_or("`captions` must be a list")?;
    if items.is_empty() || items.len() > MAX_CAPTIONS {
        return Err(format!("`captions` must hold 1 to {MAX_CAPTIONS} captions"));
    }
    let mut parsed = Vec::new();
    for (i, c) in items.iter().enumerate() {
        let time = |key: &str| c.get(key).and_then(Value::as_u64).filter(|n| *n <= MAX_TIME_MS).ok_or(format!("captions[{i}].{key} must be a whole number of milliseconds, 0 or more"));
        let (start, end) = (time("startMs")?, time("endMs")?);
        if end <= start {
            return Err(format!("captions[{i}].endMs must be after startMs"));
        }
        let text = match c.get("text") {
            Some(Value::String(t)) if !t.trim().is_empty() && t.chars().count() <= MAX_CAPTION_CHARS => t.clone(),
            _ => return Err(format!("captions[{i}].text must be text of 1 to {MAX_CAPTION_CHARS} characters")),
        };
        parsed.push((start, end, text));
    }
    let name = match p.get("name") {
        None | Some(Value::Null) => "AI Captions".to_owned(),
        Some(Value::String(n)) if !n.trim().is_empty() && n.chars().count() <= 100 => n.trim().to_owned(),
        _ => return Err("`name` must be a short text".into()),
    };
    if app.session.active_sequence().is_none() {
        return Err("no sequence open: open or create one to add captions".into());
    }
    run_group(app, "Add AI Captions", |s| {
        // Whole frames, as the caption editor places them; a caption ends where the next one starts, never past it.
        let rate = s.sequence_rate();
        let half = rate.frame_duration().0 / 2;
        let mut spans: Vec<(usize, i64, i64)> = parsed
            .iter()
            .enumerate()
            .map(|(i, (a, b, _))| {
                let start = rate.frame_at(ticks(*a));
                (i, start, rate.frame_at(Tick(ticks(*b).0 + half)).max(start + 1))
            })
            .collect();
        spans.sort_by_key(|x| (x.1, x.0));
        for k in 1..spans.len() {
            if spans[k].1 > spans[k - 1].1 && spans[k - 1].2 > spans[k].1 {
                spans[k - 1].2 = spans[k].1;
            }
        }
        let track = s.execute("captions.newTrack", json!({"name": name})).map_err(|e| e.to_string())?["track"].as_u64().ok_or("no caption track was made")?;
        let mut ids = vec![Value::Null; parsed.len()];
        for (i, start, end) in spans {
            let seconds = (rate.tick_of(end) - rate.tick_of(start)).seconds();
            let r = s
                .execute("captions.add", json!({"track": track, "text": parsed[i].2, "time": rate.tick_of(start).0, "durationSeconds": seconds}))
                .map_err(|e| e.to_string())?;
            ids[i] = r["caption"].clone();
        }
        Ok(json!({"placed": "captions", "trackId": track, "name": name, "count": ids.len(), "captionIds": ids}))
    })
}
