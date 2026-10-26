//! The guest page's start chain, as C0VIBE sends it while the guest view is still hidden under its
//! start cover: `c0.inspect` -> `c0.set {panels}` -> `ui.set {theme}` -> `c0.set {themeTokens}`.
//!
//! Since the egui 0.37 port the app answers `ui.*` requests only at the end of a UI pass, and a
//! hidden page's runner runs logic-only passes, so the page bridge lends the request one UI pass
//! (`page_control::lend_ui_pass`). The contract here: that one pass, laid out and never painted
//! (`Harness::step` does not render; nothing here calls `render`), answers a `ui.set` theme request.

mod c0_support;

use c0_support::{Page, settle};
use egui_kittest::Harness;
use filmcraft_engine::Session;
use filmcraft_ui_egui::FilmcraftApp;
use filmcraft_ui_egui::control::ControlRequest;
use filmcraft_ui_egui::theme::ThemeKind;
use serde_json::{Value, json};
use std::sync::mpsc::{Receiver, Sender, channel};

fn page() -> (Page, Sender<ControlRequest>) {
    let (tx, rx) = channel();
    let app = FilmcraftApp::new(Session::default()).with_control(rx);
    let mut h = Harness::builder().with_size(egui::vec2(1600.0, 980.0)).with_max_steps(10_000).build_eframe(move |_| app);
    settle(&mut h);
    (h, tx)
}

/// Queues a request the way the page's `window.filmcraft.request` does and returns its reply channel.
fn send(tx: &Sender<ControlRequest>, method: &str, params: Value) -> Receiver<Value> {
    let (request, reply) = ControlRequest::new(method, params);
    tx.send(request).expect("the app holds the control channel");
    reply
}

/// One unpainted pass (logic, then UI), then the reply that pass produced.
fn one_pass(h: &mut Page, reply: &Receiver<Value>, method: &str) -> Value {
    c0_support::step(h);
    let value = reply.try_recv().unwrap_or_else(|_| panic!("{method} was not answered by one unpainted UI pass"));
    assert_eq!(value["ok"], true, "{method}: {value}");
    value
}

#[test]
fn ui_set_theme_is_answered_by_one_unpainted_ui_pass() {
    let (mut h, tx) = page();
    let reply = send(&tx, "ui.set", json!({"theme":"light"}));
    one_pass(&mut h, &reply, "ui.set light");
    let ctx = h.ctx.clone();
    assert_eq!(h.state().appearance_kind(&ctx), ThemeKind::Light);
    let reply = send(&tx, "ui.set", json!({"theme":"dark"}));
    one_pass(&mut h, &reply, "ui.set dark");
    assert_eq!(h.state().appearance_kind(&ctx), ThemeKind::Dark);
}

#[test]
fn host_start_chain_needs_one_pass_per_request() {
    let (mut h, tx) = page();
    let chain = [
        ("c0.inspect", json!({})),
        ("c0.set", json!({"panels":{"title_bar":false,"header":false,"menu_bar":false,"start_screen":false,"community_links":false}})),
        ("ui.set", json!({"theme":"light"})),
        ("c0.set", json!({"themeTokens":{"accent":"#3366ffff"}})),
    ];
    for (method, params) in chain {
        let reply = send(&tx, method, params);
        one_pass(&mut h, &reply, method);
    }
}
