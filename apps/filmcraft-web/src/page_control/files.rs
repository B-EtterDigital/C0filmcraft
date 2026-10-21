//! Invalidate late browser-picker answers when the host cancels. Browsers do not expose an
//! imperative close operation for the OS picker; cancellation guarantees no late import.
use std::cell::Cell;
thread_local! {
    static GENERATION:Cell<u64>=const{Cell::new(0)};
    static PENDING:Cell<bool>=const{Cell::new(false)};
}
pub struct Picker {
    generation: u64,
}
pub fn begin() -> Picker {
    let generation = GENERATION.with(|slot| {
        let next = slot.get().saturating_add(1);
        slot.set(next);
        next
    });
    PENDING.with(|slot| slot.set(true));
    Picker { generation }
}
impl Picker {
    pub fn accept(&self) -> bool {
        let current = GENERATION.with(|slot| slot.get() == self.generation);
        current && PENDING.with(|slot| slot.replace(false))
    }
}
impl Drop for Picker {
    fn drop(&mut self) {
        self.accept();
    }
}
pub fn cancel() -> bool {
    let pending = PENDING.with(|slot| slot.replace(false));
    if pending {
        GENERATION.with(|slot| slot.set(slot.get().saturating_add(1)));
    }
    pending
}
pub async fn open_project(file: web_sys::File) -> Result<serde_json::Value, String> {
    use wasm_bindgen_futures::JsFuture;
    let limit = filmcraft_ui_egui::c0_host::MAX_FILE_BYTES;
    if !file.size().is_finite() || file.size() < 0.0 || file.size() > limit as f64 {
        return Err("project exceeds 72 MiB".into());
    }
    let name = file.name();
    let path = format!("/projects/{name}");
    let buffer = JsFuture::from(file.array_buffer()).await.map_err(|error| format!("{error:?}"))?;
    let bytes = js_sys::Uint8Array::new(&buffer).to_vec();
    let incoming = bytes.clone();
    let project_path = path.clone();
    let result = crate::import::on_ui(move |web, _| filmcraft_ui_egui::c0_host::files::open_bytes_at(&mut web.app, name, incoming, project_path)).await?;
    crate::fs::put(&path, bytes);
    if result.get("pending").and_then(serde_json::Value::as_bool) == Some(true) {
        Ok(result)
    } else {
        result.get("opened").cloned().ok_or_else(|| "project open returned no result".into())
    }
}
