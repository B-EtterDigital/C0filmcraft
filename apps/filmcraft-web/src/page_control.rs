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
pub use protocol::{inline_passes, parse};

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
