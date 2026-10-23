//! `ui.render {sequence?, frame?, scale?}`: a PNG of the Program monitor's frame, composited off
//! screen. The app here never runs a UI pass for it, as a hidden page never does.

use base64::{Engine as _, engine::general_purpose::STANDARD};
use filmcraft_engine::Session;
use filmcraft_ui_egui::FilmcraftApp;
use filmcraft_ui_egui::control::{self, ControlRequest, Outcome};
use serde_json::{Value, json};
use std::{
    cell::Cell,
    rc::Rc,
    time::{Duration, Instant},
};

/// A 1280 x 720 sequence with a red matte on V1 for 48 frames.
fn red_sequence() -> Session {
    let mut s = Session::default();
    s.execute("file.newSequence", json!({"name": "Look", "video": 1, "audio": 1, "width": 1280, "height": 720})).expect("new sequence");
    let matte = s.execute("file.newColorMatte", json!({"color": "#ff0000", "seconds": 3.0})).expect("a color matte");
    let item = matte["item"].as_u64().expect("the matte's item id");
    let rate = s.sequence_rate();
    s.execute("timeline.place", json!({"item": item, "frame": 0, "sourceIn": 0, "duration": rate.tick_of(48).0})).expect("place the matte");
    s.execute("edit.deselectAll", json!({})).expect("deselect");
    s
}

/// A reply, waiting for queued frames as the control loop's retries do. Panics if none arrives.
fn answer(app: &mut FilmcraftApp, method: &str, params: Value) -> Value {
    let ctx = egui::Context::default();
    let started = Instant::now();
    loop {
        let (request, _) = ControlRequest::new(method, params.clone());
        match control::handle(app, &ctx, &request) {
            Outcome::Done(reply) => return reply,
            Outcome::Retry(_) if started.elapsed() < Duration::from_secs(60) => std::thread::sleep(Duration::from_millis(10)),
            Outcome::Retry(message) => panic!("{method} still retrying: {message}"),
            _ => panic!("{method} must not ask for a presented frame"),
        }
    }
}

fn ok(reply: &Value) -> &Value {
    assert_eq!(reply["ok"], true, "{reply}");
    &reply["result"]
}

fn decode(result: &Value) -> image::RgbaImage {
    let bytes = STANDARD.decode(result["pngBase64"].as_str().expect("a base64 PNG")).expect("valid base64");
    assert_eq!(&bytes[..8], b"\x89PNG\r\n\x1a\n");
    image::load_from_memory_with_format(&bytes, image::ImageFormat::Png).expect("a decodable PNG").to_rgba8()
}

#[test]
fn renders_the_program_frame_at_the_playhead_without_any_ui_pass() {
    let mut app = FilmcraftApp::new(red_sequence());
    app.session.execute("playhead.set", json!({"frame": 5})).expect("seek");
    let reply = answer(&mut app, "ui.render", json!({}));
    let result = ok(&reply);
    assert_eq!((result["frame"].clone(), result["presented"].clone()), (json!(5), json!(false)));
    let image = decode(result);
    let scale = result["scale"].as_f64().expect("the scale used");
    assert_eq!((image.width(), image.height()), ((1280.0 * scale).round() as u32, (720.0 * scale).round() as u32));
    assert_eq!((result["width"].as_u64(), result["height"].as_u64()), (Some(u64::from(image.width())), Some(u64::from(image.height()))));
    let centre = image.get_pixel(image.width() / 2, image.height() / 2).0;
    assert!(centre[0] > 200 && centre[1] < 40 && centre[2] < 40 && centre[3] == 255, "the matte, not a stand-in: {centre:?}");
}

#[test]
fn frame_scale_and_sequence_choose_what_is_drawn_and_it_is_served_from_cache_next_time() {
    let mut app = FilmcraftApp::new(red_sequence());
    let first = app.session.state.active_sequence.expect("an active sequence").0;
    // explicit frame and scale; a frame past the matte is black over black
    let reply = answer(&mut app, "ui.render", json!({"sequence": first, "frame": 100, "scale": 0.25}));
    let image = decode(ok(&reply));
    assert_eq!((image.width(), image.height()), (320, 180));
    assert_eq!(image.get_pixel(160, 90).0, [0, 0, 0, 255], "nothing is placed at frame 100");
    let inside = answer(&mut app, "ui.render", json!({"frame": 10, "scale": 0.25}));
    assert_eq!(decode(ok(&inside)).dimensions(), (320, 180));
    // the second request for the same frame is answered at once, without a retry
    let (request, _) = ControlRequest::new("ui.render", json!({"frame": 10, "scale": 0.25}));
    assert!(matches!(control::handle(&mut app, &egui::Context::default(), &request), Outcome::Done(_)));
    // another sequence, not the active one
    app.session.execute("file.newSequence", json!({"name": "Other", "video": 1, "audio": 1, "width": 640, "height": 360})).expect("a second sequence");
    let reply = answer(&mut app, "ui.render", json!({"sequence": first, "frame": 3, "scale": 0.25}));
    assert_eq!(decode(ok(&reply)).dimensions(), (320, 180), "the first sequence, though the second is active");
    let reply = answer(&mut app, "ui.render", json!({"scale": 1.0}));
    assert_eq!(decode(ok(&reply)).dimensions(), (640, 360), "the active one by default");
}

#[test]
fn leaves_the_project_and_the_window_alone() {
    let mut app = FilmcraftApp::new(red_sequence());
    let (revision, journal, dirty) = (app.session.revision, app.session.journal.len(), app.session.is_dirty());
    let playhead = app.session.playhead();
    ok(&answer(&mut app, "ui.render", json!({"frame": 7, "scale": 0.125})));
    assert_eq!((app.session.revision, app.session.journal.len(), app.session.is_dirty()), (revision, journal, dirty));
    assert_eq!(app.session.playhead(), playhead, "rendering another frame does not move the playhead");
    assert!(!app.playback.playing);
}

#[test]
fn hostile_parameters_are_errors_and_never_retries() {
    let mut app = FilmcraftApp::new(red_sequence());
    let first = app.session.state.active_sequence.expect("an active sequence").0;
    let bad = [
        json!({"sequence": "one"}),
        json!({"sequence": -1}),
        json!({"sequence": 1.5}),
        json!({"sequence": 987654321}),
        json!({"sequence": first + 1_000_000}),
        json!({"frame": -1}),
        json!({"frame": 1.5}),
        json!({"frame": "7"}),
        json!({"frame": i64::MAX}),
        json!({"frame": 1.0e30}),
        json!({"scale": 0}),
        json!({"scale": -0.5}),
        json!({"scale": 1.5}),
        json!({"scale": 0.0001}),
        json!({"scale": "half"}),
        json!({"scale": [1]}),
        json!({"path": "/etc/passwd"}),
        json!({"frame": 1, "unexpected": true}),
    ];
    for params in bad {
        let (request, _) = ControlRequest::new("ui.render", params.clone());
        match control::handle(&mut app, &egui::Context::default(), &request) {
            Outcome::Done(reply) => assert_eq!(reply["ok"], false, "{params}: {reply}"),
            _ => panic!("{params} must be an immediate error"),
        }
    }
    for params in [json!([]), json!(3), json!("frame")] {
        let (request, _) = ControlRequest::new("ui.render", params.clone());
        assert!(matches!(control::handle(&mut app, &egui::Context::default(), &request), Outcome::Done(reply) if reply["ok"] == false), "{params}");
    }
    // null params are the empty object
    ok(&answer(&mut app, "ui.render", Value::Null));
    // nothing to render
    let mut empty = FilmcraftApp::new(Session::default());
    let reply = answer(&mut empty, "ui.render", json!({}));
    assert_eq!(reply["error"], "no active sequence");
}

#[test]
fn a_frame_too_large_to_hold_is_refused_not_allocated() {
    let mut s = Session::default();
    s.execute("file.newSequence", json!({"name": "Huge", "video": 1, "audio": 1, "width": 8192, "height": 8192})).expect("a large sequence");
    let mut app = FilmcraftApp::new(s);
    let reply = answer(&mut app, "ui.render", json!({"scale": 1.0}));
    assert_eq!(reply["ok"], false);
    assert!(reply["error"].as_str().is_some_and(|e| e.contains("too large") && e.contains("lower scale")), "{reply}");
    // the same sequence at a scale that fits is fine
    assert_eq!(decode(ok(&answer(&mut app, "ui.render", json!({"scale": 0.0625})))).dimensions(), (512, 512));
}

#[test]
fn the_control_loop_answers_without_raising_the_window_where_a_screenshot_does() {
    let (tx, rx) = std::sync::mpsc::channel();
    let mut app = FilmcraftApp::new(red_sequence()).with_control(rx);
    let raised = Rc::new(Cell::new(0usize));
    let counter = raised.clone();
    app.hooks.raise_without_focus = Some(Box::new(move || counter.set(counter.get() + 1)));
    let mut h = egui_kittest::Harness::builder().with_size(egui::vec2(800.0, 600.0)).build_eframe(move |_| app);
    // let the app go "idle" long enough that any ui.* request would normally raise the window
    for _ in 0..20 {
        h.step();
        std::thread::sleep(Duration::from_millis(20));
    }
    let (request, reply) = ControlRequest::new("ui.render", json!({"frame": 2, "scale": 0.25}));
    tx.send(request).expect("send the request");
    let started = Instant::now();
    let answer = loop {
        h.step();
        if let Ok(answer) = reply.try_recv() {
            break answer;
        }
        assert!(started.elapsed() < Duration::from_secs(60), "no reply");
        std::thread::sleep(Duration::from_millis(10));
    };
    assert_eq!(answer["ok"], true, "{answer}");
    assert_eq!(raised.get(), 0, "ui.render must not raise or present the window");
    // the same loop does raise for a request that needs a presented frame
    let (request, _reply) = ControlRequest::new("ui.screenshot", json!({}));
    tx.send(request).expect("send the screenshot request");
    for _ in 0..5 {
        h.step();
    }
    assert!(raised.get() > 0, "the control: a screenshot raises the window");
}
