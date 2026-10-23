//! C0vid guest contract. Presentation switches are session-local and default to upstream.

pub mod dock_modes;
pub mod files;
mod limits;
mod panels;
pub mod render;
pub use limits::{MAX_FILE_BYTES, MAX_NAME_BYTES, MAX_REQUEST_BYTES};

use crate::{FilmcraftApp, control::Outcome};
use filmcraft_project::{Bin, BinEntry, ItemId};
use serde_json::{Value, json};

const MAX_INSPECT_BINS: usize = 100_000;
pub const METHODS: &[&str] = &["c0.inspect", "c0.goTo", "c0.play", "c0.documentBytes", "c0.openFile", "c0.newDocument", "c0.cancelFileDialog", "c0.set"];

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Chrome {
    pub title_bar: bool,
    pub menu_bar: bool,
    pub header: bool,
    pub status_bar: bool,
    pub start_screen: bool,
    pub community_links: bool,
}

impl Default for Chrome {
    fn default() -> Self {
        Self { title_bar: true, menu_bar: true, header: true, status_bar: true, start_screen: true, community_links: true }
    }
}

#[derive(Default)]
pub struct Host {
    pub chrome: Chrome,
    pub dock: dock_modes::Modes,
    pub pending: Option<files::Pending>,
}

pub fn handle(app: &mut FilmcraftApp, ctx: &egui::Context, method: &str, params: &Value) -> Outcome {
    let result = dispatch(app, ctx, method, params);
    if result.is_ok() && !matches!(method, "c0.inspect" | "c0.documentBytes") {
        ctx.request_repaint();
    }
    Outcome::Done(match result {
        Ok(value) => json!({"ok":true,"result":value}),
        Err(error) => json!({"ok":false,"error":error}),
    })
}

fn dispatch(app: &mut FilmcraftApp, ctx: &egui::Context, method: &str, params: &Value) -> Result<Value, String> {
    if !params.is_object() {
        return Err("params must be an object".into());
    }
    if params.to_string().len() > MAX_REQUEST_BYTES {
        return Err("host request exceeds 100 MiB".into());
    }
    match method {
        "c0.inspect" => Ok(inspect(app)),
        "c0.goTo" => go_to(app, ctx, params),
        "c0.play" => {
            let action = params.get("action").and_then(Value::as_str).ok_or("action is required")?;
            match action {
                "play" => {
                    if app.session.active_sequence().is_none() {
                        return Err("no active sequence".into());
                    }
                    app.play(1.0);
                }
                "pause" => app.stop(),
                "toggle" if app.playback.playing => app.stop(),
                "toggle" => {
                    if app.session.active_sequence().is_none() {
                        return Err("no active sequence".into());
                    }
                    app.play(1.0);
                }
                _ => return Err("action must be play, pause or toggle".into()),
            }
            Ok(json!({"playing":app.playback.playing}))
        }
        "c0.documentBytes" => files::document_bytes(app, params),
        "c0.openFile" => files::open(app, params),
        "c0.newDocument" => files::new_document(app, params),
        "c0.cancelFileDialog" => Ok(json!({"cancelled":files::cancel(app)})),
        "c0.set" => panels::set(app, ctx, params),
        _ => Err(format!("unknown host method {method}")),
    }
}

pub fn inspect(app: &FilmcraftApp) -> Value {
    let session = &app.session;
    let sequences: Vec<Value> = session
        .project
        .sequences()
        .filter_map(|item| {
            let sequence = item.as_sequence()?;
            let rate = sequence.settings.frame_rate;
            Some(json!({"id":item.id.0,"name":item.name,"active":Some(item.id)==session.state.active_sequence,
            "durationFrames":rate.frame_at(sequence.duration()),"fps":{"num":rate.num,"den":rate.den}}))
        })
        .collect();
    let mut bins = Vec::new();
    let mut bins_truncated = false;
    // Iterative and bounded: a project is untrusted, including its bin nesting.
    let mut pending: Vec<&Bin> = vec![&session.project.root];
    while let Some(bin) = pending.pop() {
        if bins.len() >= MAX_INSPECT_BINS {
            bins_truncated = true;
            break;
        }
        bins.push(json!({"name":bin.name,"items":bin.children.iter().filter(|entry| matches!(entry, BinEntry::Item(_))).count()}));
        for child in bin.children.iter().filter_map(|entry| if let BinEntry::Bin(child) = entry { Some(child) } else { None }) {
            if pending.len().saturating_add(bins.len()) >= MAX_INSPECT_BINS {
                bins_truncated = true;
                break;
            }
            pending.push(child);
        }
    }
    json!({"revision":session.revision,"dirty":session.is_dirty(),"canUndo":!session.history.undo.is_empty(),"canRedo":!session.history.redo.is_empty(),
        "project":{"name":session.project.name,"dirty":session.is_dirty(),"revision":session.revision},
        "sequences":sequences,"activeSequence":session.state.active_sequence.map(|id|id.0),
        "playheadFrame":session.sequence_rate().frame_at(session.playhead()),"playing":app.playback.playing,
        "selectionCount":session.state.selection.len(),"bins":bins,"binsTruncated":bins_truncated,"panes":panels::inspect(app),
        "theme":app.tokens.kind.pref_name(),"fileDialogPending":app.c0.pending.is_some() || app.native_dialog_open()})
}

fn go_to(app: &mut FilmcraftApp, ctx: &egui::Context, params: &Value) -> Result<Value, String> {
    let sequence = match params.get("sequence") {
        None => app.session.state.active_sequence,
        Some(value) => Some(ItemId(value.as_u64().ok_or("sequence must be an integer id")?)),
    }
    .ok_or("no active sequence")?;
    let seq = app.session.project.sequence(sequence).ok_or("unknown sequence")?;
    let frame = match params.get("frame") {
        None => None,
        Some(value) => Some(
            value
                .as_i64()
                .filter(|n| *n >= 0 && *n <= seq.settings.frame_rate.frame_at(filmcraft_time::Tick::MAX))
                .ok_or("frame exceeds the supported time range")?,
        ),
    };
    app.stop();
    app.session.execute("sequence.open", json!({"item":sequence.0})).map_err(|e| e.to_string())?;
    if let Some(frame) = frame {
        app.session.execute("playhead.set", json!({"frame":frame})).map_err(|e| e.to_string())?;
    }
    app.ui.mode = crate::state::Mode::Edit;
    crate::panels::timeline::reveal_playhead(app, ctx);
    Ok(inspect(app))
}

pub fn community_url(url: &str) -> bool {
    ["discord.gg", "discord.com", "getartcraft.com", "github.com"]
        .iter()
        .any(|domain| url.strip_prefix("https://").or_else(|| url.strip_prefix("http://")).is_some_and(|rest| rest.split('/').next() == Some(*domain)))
}

pub(crate) fn check_command(app: &FilmcraftApp, id: &str) -> Result<(), String> {
    if !app.c0.chrome.community_links && (crate::links::url_for(id).is_some() || id == "help.filmcraftHelp") {
        return Err("community links are hidden by the host".into());
    }
    Ok(())
}

pub(crate) fn about(ui: &mut egui::Ui) {
    ui.heading("FilmCraft");
    ui.label(format!("Version {}", env!("CARGO_PKG_VERSION")));
    ui.label("A clean-room, pure-Rust non-linear video editor.");
    ui.label("FilmCraft authors and contributors (see Contributors and Models).");
    ui.label("MIT OR Apache-2.0. Fonts: Inter, Noto Serif and JetBrains Mono (SIL OFL 1.1).");
}

pub(crate) fn community_key() -> egui::Id {
    egui::Id::new("filmcraft-host-community")
}
pub(crate) fn contributor(ui: &mut egui::Ui, contributor: &crate::credits::Contributor, names: crate::credits::NameMode) {
    if ui.ctx().data(|data| data.get_temp::<bool>(community_key())).unwrap_or(true) {
        ui.hyperlink_to(contributor.name(names), format!("https://github.com/{}", contributor.login)).on_hover_text(contributor.summary());
    } else {
        ui.label(contributor.name(names)).on_hover_text(contributor.summary());
    }
}
