//! Opt-in JSON-text facade over window.filmcraft, with an unpresented runner pass.
#![cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
use serde_json::json;
#[cfg(target_arch = "wasm32")]
mod bridge;
#[cfg(all(target_arch = "wasm32", not(feature = "patent-codecs")))]
pub mod codecs;
#[cfg(target_arch = "wasm32")]
pub mod files;
#[cfg(target_arch = "wasm32")]
pub use bridge::attach;

mod protocol;
pub use protocol::{inline_passes, needs_ui_pass, parse};

#[cfg(target_arch = "wasm32")]
thread_local! {
    /// Set while the bridge runs a pass for a request that needs a UI pass (see [`lend_ui_pass`]).
    static LEND_UI_PASS: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// Runs `pass` (one runner pass, dispatched synchronously) with a UI pass lent when `lend` is set.
#[cfg(target_arch = "wasm32")]
pub(crate) fn lending_ui_pass<R>(lend: bool, pass: impl FnOnce() -> R) -> R {
    let before = LEND_UI_PASS.with(|slot| slot.replace(lend));
    let out = pass();
    LEND_UI_PASS.with(|slot| slot.set(before));
    out
}

/// `WebApp::raw_input_hook` seam. eframe's web runner runs only `App::logic` while the document is
/// hidden (C0VIBE keeps the guest view hidden under its start cover), but the app answers `ui.*`
/// requests at the end of a UI pass. While the bridge lends a pass, the root viewport counts as
/// not occluded, so that one pass is a full UI pass. It is laid out, never painted: eframe paints
/// only from its own frame callback.
#[cfg(target_arch = "wasm32")]
pub fn lend_ui_pass(raw_input: &mut egui::RawInput) {
    if LEND_UI_PASS.with(std::cell::Cell::get)
        && let Some(viewport) = raw_input.viewports.get_mut(&egui::ViewportId::ROOT)
    {
        viewport.occluded = Some(false);
    }
}

#[cfg(target_arch = "wasm32")]
pub fn configure(app: &mut filmcraft_ui_egui::FilmcraftApp, ctx: &egui::Context) {
    use wasm_bindgen::JsCast as _;
    crate::set_info("softwarePatentCodecs", json!(cfg!(feature = "patent-codecs")));
    if let Some(window) = web_sys::window() {
        let reduced = js_sys::Reflect::get(&window, &"matchMedia".into())
            .ok()
            .and_then(|function| function.dyn_into::<js_sys::Function>().ok())
            .and_then(|function| function.call1(&window, &"(prefers-reduced-motion: reduce)".into()).ok())
            .and_then(|query| js_sys::Reflect::get(&query, &"matches".into()).ok())
            .and_then(|value| value.as_bool())
            .unwrap_or(false);
        if reduced {
            app.session.prefs.appearance.animations = false;
        }
    }
    let _ = filmcraft_ui_egui::c0_host::handle(
        app,
        ctx,
        "c0.set",
        &json!({"panels":{"title_bar":false,"header":false,"menu_bar":false,"start_screen":false,"community_links":false}}),
    );
}
