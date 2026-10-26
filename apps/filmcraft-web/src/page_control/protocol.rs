//! Transport validation shared by the wasm facade and native K-41 tests.
#![cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
use serde_json::{Value, json};
#[allow(dead_code)]
#[path = "../../../../crates/ui-egui/src/c0_host/limits.rs"]
mod limits;
pub const MAX_REQUEST_BYTES: usize = 1024 * 1024;
pub const MAX_CLICK_COUNT: u64 = 256;
pub const MAX_DRAG_STEPS: u64 = 32;

pub fn parse(text: &str) -> Result<(String, Value), String> {
    if text.len() > limits::MAX_REQUEST_BYTES {
        return Err("request exceeds 100 MiB".into());
    }
    let value: Value = serde_json::from_str(text).map_err(|e| format!("request is not JSON: {e}"))?;
    let method = value.get("method").and_then(Value::as_str).ok_or("request has no method")?;
    if !method.starts_with("c0.") && text.len() > MAX_REQUEST_BYTES {
        return Err("request exceeds 1 MiB".into());
    }
    let params = value.get("params").filter(|p| !p.is_null()).cloned().unwrap_or(json!({}));
    if !params.is_object() {
        return Err("params must be an object".into());
    }
    let bound = match method {
        "ui.click" => Some(("count", MAX_CLICK_COUNT)),
        "ui.drag" => Some(("steps", MAX_DRAG_STEPS)),
        _ => None,
    };
    if let Some((key, limit)) = bound
        && let Some(value) = params.get(key)
        && !value.as_u64().is_some_and(|v| v <= limit)
    {
        return Err(format!("{key} must be an integer from 0 to {limit}"));
    }
    Ok((method.into(), params))
}

/// Whether the app answers `method` only at the end of a UI pass. Since the egui 0.37 port the app
/// queues every `ui.*` request except `ui.screenshot` for the next UI pass, and a hidden page's
/// runner runs logic-only passes, so the bridge lends these requests a UI pass (`lend_ui_pass`).
pub fn needs_ui_pass(method: &str) -> bool {
    method.starts_with("ui.") && method != "ui.screenshot"
}

/// Extra passes run inside the call after the first one. Synthetic input plays one step a pass.
/// Any other UI-pass request is answered in the first pass and its page promise is settled by the
/// app's reply poll in the next one; the third covers a pass egui discarded.
pub fn inline_passes(method: &str) -> usize {
    if matches!(method, "ui.click" | "ui.move" | "ui.drag" | "ui.key" | "ui.type") {
        40
    } else if needs_ui_pass(method) {
        2
    } else {
        0
    }
}
