//! Read-only serialization and guarded project replacement through engine commands.

use super::{MAX_FILE_BYTES, MAX_NAME_BYTES};
use crate::FilmcraftApp;
use base64::{Engine as _, engine::general_purpose::STANDARD};
use filmcraft_engine::Services;
use serde_json::{Value, json};
use std::sync::Arc;

pub fn cancel(app: &mut FilmcraftApp) -> bool {
    let replacement = app.c0.pending.take().is_some();
    app.native_dialog.take().is_some() || replacement
}

pub enum Pending {
    Open { name: String, bytes: Vec<u8> },
    New,
}

pub fn document_bytes(app: &FilmcraftApp, params: &Value) -> Result<Value, String> {
    if params.get("format").is_some_and(|v| v.as_str() != Some("fcproj")) {
        return Err("supported format: fcproj".into());
    }
    let bytes = filmcraft_format::encode_with_view(&app.session.project, Some(&app.session.project_view()), true);
    if bytes.len() > MAX_FILE_BYTES {
        return Err("project exceeds 72 MiB".into());
    }
    let media: Vec<Value> = app
        .session
        .project
        .items
        .values()
        .flat_map(|item| {
            item.as_media().into_iter().flat_map(move |clip| {
                std::iter::once((&clip.media, false)).chain(clip.proxy.as_ref().map(|proxy| (proxy, true))).filter_map(move |(media, proxy)| match media {
                    filmcraft_project::MediaRef::File { path } => Some(json!({"name":item.name,"path":path,"proxy":proxy})),
                    _ => None,
                })
            })
        })
        .collect();
    Ok(
        json!({"format":"fcproj","name":format!("{}.fcproj",app.session.project.name),"revision":app.session.revision,"encoding":"base64","bytes":STANDARD.encode(&bytes),"size":bytes.len(),"referencedMedia":media}),
    )
}

pub fn open(app: &mut FilmcraftApp, params: &Value) -> Result<Value, String> {
    let name = params.get("name").and_then(Value::as_str).ok_or("name is required")?;
    validate_name(name)?;
    let encoded = params.get("bytes").and_then(Value::as_str).ok_or("bytes must be base64 text")?;
    if encoded.len() > MAX_FILE_BYTES.div_ceil(3) * 4 {
        return Err("file exceeds 72 MiB".into());
    }
    let bytes = STANDARD.decode(encoded).map_err(|e| format!("invalid base64: {e}"))?;
    if bytes.len() > MAX_FILE_BYTES {
        return Err("file exceeds 72 MiB".into());
    }
    open_bytes(app, name.into(), bytes)
}

pub fn open_bytes(app: &mut FilmcraftApp, name: String, bytes: Vec<u8>) -> Result<Value, String> {
    let path = name.clone();
    open_bytes_at(app, name, bytes, path)
}

/// Shells may retain their existing virtual project path while sharing the unsaved guard.
pub fn open_bytes_at(app: &mut FilmcraftApp, name: String, bytes: Vec<u8>, path: String) -> Result<Value, String> {
    if bytes.len() > MAX_FILE_BYTES {
        return Err("file exceeds 72 MiB".into());
    }
    validate_name(&name)?;
    // Validate before asking the user to discard their current work.
    filmcraft_format::decode(&bytes).map_err(|e| e.to_string())?;
    begin(app, Pending::Open { name: path, bytes })
}

pub fn new_document(app: &mut FilmcraftApp, params: &Value) -> Result<Value, String> {
    if params.get("template").is_some_and(|v| !v.is_null() && v.as_str() != Some("blank")) {
        return Err("supported template: blank".into());
    }
    begin(app, Pending::New)
}

fn begin(app: &mut FilmcraftApp, pending: Pending) -> Result<Value, String> {
    if app.c0.pending.is_some() {
        return Err("a document replacement is already awaiting a decision".into());
    }
    if app.session.is_dirty() {
        app.c0.pending = Some(pending);
        Ok(json!({"pending":true,"reason":"unsavedChanges"}))
    } else {
        apply(app, pending)
    }
}

// Only the project read is overlaid. Media continues to use the platform's readers/loaders;
// no project is written to disk or offered as a browser download merely by opening it.
struct ProjectRead {
    inner: Arc<dyn Services>,
    name: String,
    bytes: Vec<u8>,
}
impl Services for ProjectRead {
    fn read_file(&self, path: &str) -> std::io::Result<Vec<u8>> {
        if path == self.name { Ok(self.bytes.clone()) } else { self.inner.read_file(path) }
    }
    fn write_file(&self, path: &str, bytes: &[u8]) -> std::io::Result<()> {
        self.inner.write_file(path, bytes)
    }
    fn reader(&self, path: &str) -> Option<std::io::Result<filmcraft_media::SharedReader>> {
        self.inner.reader(path)
    }
    fn file_size(&self, path: &str) -> std::io::Result<u64> {
        if path == self.name { Ok(self.bytes.len() as u64) } else { self.inner.file_size(path) }
    }
    fn read_range(&self, path: &str, offset: u64, len: usize) -> std::io::Result<Vec<u8>> {
        self.inner.read_range(path, offset, len)
    }
    fn list_dir(&self, path: &str) -> Option<std::io::Result<Vec<String>>> {
        self.inner.list_dir(path)
    }
    fn file_loader(&self) -> Option<filmcraft_media::sequence::FrameLoader> {
        self.inner.file_loader()
    }
    fn export_in_memory(&self) -> bool {
        self.inner.export_in_memory()
    }
    fn list_entries(&self, dir: &str) -> Option<std::io::Result<Vec<filmcraft_engine::media_browser::DirEntry>>> {
        self.inner.list_entries(dir)
    }
    fn volumes(&self) -> Vec<filmcraft_engine::media_browser::Volume> {
        self.inner.volumes()
    }
    fn home_dir(&self) -> Option<String> {
        self.inner.home_dir()
    }
}

fn apply(app: &mut FilmcraftApp, pending: Pending) -> Result<Value, String> {
    app.stop();
    app.stop_source();
    let result = match pending {
        Pending::New => app.session.execute("file.newProject", json!({"name":"Untitled"})).map_err(|e| e.to_string()),
        Pending::Open { name, bytes } => {
            let original = app.session.services.clone();
            app.session.services = Arc::new(ProjectRead { inner: original.clone(), name: name.clone(), bytes });
            let result = app.session.execute("file.open", json!({"path":name})).map_err(|e| e.to_string());
            app.session.services = original;
            result
        }
    }?;
    app.ui.mode = crate::state::Mode::Edit;
    Ok(json!({"pending":false,"document":super::inspect(app),"opened":result}))
}

/// Resolve a pending replacement; a host can drive the same buttons with ui.click.
pub fn resolve(app: &mut FilmcraftApp, action: &str) -> Result<Value, String> {
    if action == "cancel" {
        return Ok(json!({"cancelled":app.c0.pending.take().is_some()}));
    }
    if app.c0.pending.is_none() {
        return Err("no pending document replacement".into());
    }
    match action {
        "save" => {
            // Save through the normal picker/download path. An asynchronous picker must finish
            // before replacement; the unchanged prompt stays available meanwhile.
            crate::menus::invoke(app, &egui::Context::default(), "file.save", json!({}))?;
            if app.session.is_dirty() {
                return Ok(json!({"pending":true}));
            }
        }
        "discard" => {}
        _ => return Err("action must be save, discard or cancel".into()),
    }
    let pending = app.c0.pending.take().ok_or("no pending document replacement")?;
    apply(app, pending)
}

pub fn show(app: &mut FilmcraftApp, ctx: &egui::Context) {
    if app.c0.pending.is_none() {
        return;
    }
    let mut action = None;
    egui::Window::new("Save changes to the current project?")
        .id(egui::Id::unique("c0-unsaved"))
        .collapsible(false)
        .resizable(false)
        .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
        .show(ctx, |ui| {
            ui.label("Save your changes before replacing this project.");
            ui.horizontal(|ui| {
                for (id, label) in [("save", "Save"), ("discard", "Discard"), ("cancel", "Cancel")] {
                    let response = ui.button(label);
                    app.auto.add(&format!("c0.unsaved.{id}"), response.rect, label);
                    if response.clicked() {
                        action = Some(id);
                    }
                }
            });
        });
    if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
        action = Some("cancel");
    }
    if let Some(action) = action
        && let Err(error) = resolve(app, action)
    {
        app.status(error);
    }
}

fn validate_name(name: &str) -> Result<(), String> {
    if name.is_empty() || name.len() > MAX_NAME_BYTES || name.contains(['/', '\\', '\0']) || !name.to_ascii_lowercase().ends_with(".fcproj") {
        return Err("name must be a .fcproj basename".into());
    }
    Ok(())
}
