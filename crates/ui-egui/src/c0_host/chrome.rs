//! The host's chrome in the header bar: the Home button that returns to the start screen.

use egui::{Rect, Sense, pos2, vec2};

use crate::FilmcraftApp;
use crate::icons::{self, Icon};
use crate::state::Mode;

/// Draws the Home button at `x` when the start screen is shown, and returns where the mode tabs start.
/// Without the start screen the header is upstream's own, starting at `x`.
pub fn home(app: &mut FilmcraftApp, ui: &mut egui::Ui, rect: Rect, x: f32) -> f32 {
    if !app.c0.chrome.start_screen {
        return x;
    }
    let t = app.tokens;
    let p = ui.painter().clone();
    let home = Rect::from_center_size(pos2(x + 10.0, rect.center().y), vec2(26.0, 26.0));
    let hresp = ui.interact(home, egui::Id::unique("hdr-home"), Sense::click()).on_hover_text(tl!("Home"));
    app.auto.add("header.home", home, "Home");
    if hresp.hovered() {
        p.rect_filled(home, 4.0, t.hover);
    }
    icons::paint(&p, home.shrink(5.0), Icon::Home, if hresp.hovered() { t.text } else { t.text_dim });
    if hresp.clicked() {
        app.ui.mode = Mode::Import;
    }
    home.max.x + 16.0
}
