//! Shared by the `c0_*` UI tests: a real `FilmcraftApp` under `egui_kittest`, driven through the
//! same control handler the guest page's `c0.set` / `c0.inspect` / `ui.*` requests reach.
#![allow(dead_code)]

use egui::{Event, Modifiers, PointerButton, Pos2, Rect, Vec2, pos2, vec2};
use egui_kittest::Harness;
use filmcraft_engine::Session;
use filmcraft_ui_egui::FilmcraftApp;
use filmcraft_ui_egui::control::{self, ControlRequest, Outcome};
use serde_json::{Value, json};

pub type Page = Harness<'static, FilmcraftApp>;

/// The page: a 1600 x 980 window on a fresh session, in the factory Editing workspace.
pub fn page() -> Page {
    page_with(Session::default())
}

pub fn page_with(session: Session) -> Page {
    let app = FilmcraftApp::new(session);
    let mut h = Harness::builder().with_size(vec2(1600.0, 980.0)).with_max_steps(10_000).build_eframe(move |_| app);
    settle(&mut h);
    h
}

pub fn step(h: &mut Page) {
    let ctx = h.ctx.clone();
    let mut input = std::mem::take(h.input_mut());
    eframe::App::raw_input_hook(h.state_mut(), &ctx, &mut input);
    *h.input_mut() = input;
    h.step();
}

pub fn settle(h: &mut Page) {
    for _ in 0..5 {
        step(h);
    }
}

/// A control request answered at once (`c0.*`); the whole reply envelope.
pub fn raw(h: &mut Page, method: &str, params: Value) -> Value {
    let ctx = h.ctx.clone();
    let (request, _) = ControlRequest::new(method, params);
    match control::handle(h.state_mut(), &ctx, &request) {
        Outcome::Done(reply) => reply,
        _ => panic!("{method} was not answered at once"),
    }
}

/// [`raw`], asserting `ok`, then the frames the page needs to show the change.
pub fn call(h: &mut Page, method: &str, params: Value) -> Value {
    let reply = raw(h, method, params);
    assert_eq!(reply["ok"], true, "{method}: {reply}");
    settle(h);
    reply["result"].clone()
}

pub fn set_panels(h: &mut Page, panels: Value) -> Value {
    call(h, "c0.set", json!({"panels": panels}))
}

pub fn panes(h: &mut Page) -> Value {
    call(h, "c0.inspect", json!({}))["panes"].clone()
}

/// An element of the frame just drawn (`auto.find` would also return one only the frame before had).
pub fn rect(h: &Page, id: &str) -> Option<Rect> {
    let element = h.state().auto.elements.iter().find(|e| e.id == id)?;
    Some(Rect::from_min_size(pos2(element.rect[0], element.rect[1]), vec2(element.rect[2], element.rect[3])))
}

/// Widths are the sum of layout fractions: equal to within a hundredth of a point.
pub fn near(a: f32, b: f32) -> bool {
    (a - b).abs() < 0.01
}

pub fn must(h: &Page, id: &str) -> Rect {
    rect(h, id).unwrap_or_else(|| panic!("no element {id}"))
}

pub fn click_at(h: &mut Page, pos: Pos2) {
    h.event(Event::PointerMoved(pos));
    step(h);
    for pressed in [true, false] {
        h.event(Event::PointerButton { pos, button: PointerButton::Primary, pressed, modifiers: Modifiers::NONE });
        step(h);
    }
    settle(h);
}

pub fn click(h: &mut Page, id: &str) {
    let at = must(h, id).center();
    click_at(h, at);
}

pub fn drag_by(h: &mut Page, from: Pos2, by: Vec2) {
    h.event(Event::PointerMoved(from));
    step(h);
    h.event(Event::PointerButton { pos: from, button: PointerButton::Primary, pressed: true, modifiers: Modifiers::NONE });
    step(h);
    for fraction in [0.2, 0.5, 1.0] {
        h.event(Event::PointerMoved(from + by * fraction));
        step(h);
    }
    h.event(Event::PointerButton { pos: from + by, button: PointerButton::Primary, pressed: false, modifiers: Modifiers::NONE });
    step(h);
    settle(h);
}

/// Every id in the layout that names a docked panel (`panel.<Kind>`), with its rectangle.
pub fn panel_rects(h: &Page) -> Vec<(String, Rect)> {
    let mut rects: Vec<_> = h
        .state()
        .auto
        .elements
        .iter()
        .filter(|e| {
            e.id.starts_with("panel.")
                && !e.id.starts_with("panel.tab.")
                && !e.id.starts_with("panel.menu.")
                && !e.id.starts_with("panel.drag.")
                && !e.id.starts_with("panel.float.")
        })
        .map(|e| (e.id.clone(), Rect::from_min_size(pos2(e.rect[0], e.rect[1]), vec2(e.rect[2], e.rect[3]))))
        .collect();
    rects.sort_by(|a, b| a.0.cmp(&b.0));
    rects
}
