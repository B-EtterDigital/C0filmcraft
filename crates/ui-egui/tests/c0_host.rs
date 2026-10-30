use base64::{Engine as _, engine::general_purpose::STANDARD};
use filmcraft_ui_egui::{
    ControlRequest, FilmcraftApp,
    control::{self, Outcome},
    theme::{ThemeKind, Tokens},
};
use serde_json::{Value, json};

fn request(app: &mut FilmcraftApp, ctx: &egui::Context, method: &str, params: Value) -> Value {
    let (request, _) = ControlRequest::new(method, params);
    match control::handle(app, ctx, &request) {
        Outcome::Done(reply) => reply,
        _ => panic!("host replies must be immediate"),
    }
}
fn call(app: &mut FilmcraftApp, ctx: &egui::Context, method: &str, params: Value) -> Value {
    let reply = request(app, ctx, method, params);
    assert_eq!(reply["ok"], true, "{method}: {reply}");
    reply["result"].clone()
}
#[test]
fn defaults_preserve_upstream_and_inspect_is_read_only() {
    let mut app = FilmcraftApp::new(Default::default());
    let ctx = egui::Context::default();
    let chrome = &app.c0.chrome;
    assert!(chrome.title_bar && chrome.header && chrome.menu_bar && chrome.status_bar && chrome.start_screen && chrome.community_links);
    let journal = app.session.journal.len();
    let revision = app.session.revision;
    let result = call(&mut app, &ctx, "c0.inspect", json!({}));
    for key in [
        "revision",
        "dirty",
        "canUndo",
        "canRedo",
        "panes",
        "theme",
        "project",
        "sequences",
        "activeSequence",
        "playheadFrame",
        "playing",
        "selectionCount",
        "bins",
    ] {
        assert!(result.get(key).is_some(), "{key}");
    }
    assert_eq!(result["sequences"], json!([]));
    assert_eq!(app.session.revision, revision);
    assert_eq!(app.session.journal.len(), journal);
}
#[test]
fn every_host_reply_and_document_bytes_roundtrip() {
    let mut app = FilmcraftApp::new(Default::default());
    let ctx = egui::Context::default();
    let doc = call(&mut app, &ctx, "c0.documentBytes", json!({"format":"fcproj"}));
    assert!(STANDARD.decode(doc["bytes"].as_str().unwrap()).is_ok());
    assert_eq!(doc["referencedMedia"], json!([]));
    let opened = call(&mut app, &ctx, "c0.openFile", json!({"name":"roundtrip.fcproj","bytes":doc["bytes"]}));
    assert_eq!(opened["pending"], false);
    assert_eq!(app.session.project.name, "Untitled");
    call(&mut app, &ctx, "c0.newDocument", json!({"template":"blank"}));
    call(&mut app, &ctx, "c0.cancelFileDialog", json!({}));
    call(&mut app, &ctx, "c0.set", json!({}));
    assert_eq!(request(&mut app, &ctx, "c0.goTo", json!({}))["ok"], false);
    assert_eq!(request(&mut app, &ctx, "c0.play", json!({"action":"play"}))["ok"], false);
    call(&mut app, &ctx, "c0.play", json!({"action":"pause"}));
    assert_eq!(request(&mut app, &ctx, "c0.unknown", json!({}))["ok"], false);
}
#[test]
fn navigation_playback_and_hostile_parameters() {
    let mut app = FilmcraftApp::new(Default::default());
    let ctx = egui::Context::default();
    app.session.execute("file.newSequence", json!({"name":"One"})).unwrap();
    let id = app.session.state.active_sequence.unwrap().0;
    let revision = app.session.revision;
    let result = call(&mut app, &ctx, "c0.goTo", json!({"sequence":id,"frame":17}));
    assert_eq!(result["activeSequence"], id);
    assert_eq!(result["playheadFrame"], 17);
    assert_eq!(app.session.revision, revision);
    assert_eq!(call(&mut app, &ctx, "c0.play", json!({"action":"play"}))["playing"], true);
    assert_eq!(call(&mut app, &ctx, "c0.play", json!({"action":"toggle"}))["playing"], false);
    for params in [json!({"sequence":u64::MAX}), json!({"frame":-1}), json!({"frame":i64::MAX}), json!({"frame":1.5}), json!({"sequence":"bad"})] {
        assert_eq!(request(&mut app, &ctx, "c0.goTo", params)["ok"], false);
    }
    for (method, params) in [
        ("c0.play", json!({"action":"explode"})),
        ("c0.openFile", json!({"name":"../a.fcproj","bytes":""})),
        ("c0.openFile", json!({"name":"a.fcproj","bytes":"%%%"})),
        ("c0.openFile", json!({"name":"a.fcproj","bytes":STANDARD.encode(b"invalid")})),
        ("c0.documentBytes", json!({"format":"mp4"})),
        ("c0.newDocument", json!({"template":"demo"})),
    ] {
        assert_eq!(request(&mut app, &ctx, method, params)["ok"], false);
    }
}
#[test]
fn dirty_replacements_require_a_decision_and_can_be_cancelled() {
    let mut app = FilmcraftApp::new(Default::default());
    let ctx = egui::Context::default();
    let doc = call(&mut app, &ctx, "c0.documentBytes", json!({}));
    app.session.execute("file.newSequence", json!({"name":"Unsaved"})).unwrap();
    assert!(app.session.is_dirty());
    let revision = app.session.revision;
    assert_eq!(call(&mut app, &ctx, "c0.openFile", json!({"name":"open.fcproj","bytes":doc["bytes"]}))["pending"], true);
    assert_eq!(app.session.revision, revision);
    assert_eq!(call(&mut app, &ctx, "c0.cancelFileDialog", json!({}))["cancelled"], true);
    assert!(app.session.is_dirty());
    assert_eq!(call(&mut app, &ctx, "c0.newDocument", json!({}))["pending"], true);
    filmcraft_ui_egui::c0_host::files::resolve(&mut app, "discard").unwrap();
    assert!(app.session.project.sequences().next().is_none());
    assert!(!app.session.is_dirty());
}
#[test]
fn panels_and_all_color_overrides_report_keys_and_survive_theme_changes() {
    let mut app = FilmcraftApp::new(Default::default());
    let ctx = egui::Context::default();
    let result = call(
        &mut app,
        &ctx,
        "c0.set",
        json!({"panels":{"community_links":false,"header":false,"menu_bar":false,"status_bar":false,"start_screen":false,"unknown":true,"title_bar":3},"themeTokens":{"accent":"#12345678","radius":"#112233","text":false}}),
    );
    assert!(result["unknown"].as_array().unwrap().contains(&json!("panels.unknown")));
    assert!(result["unknown"].as_array().unwrap().contains(&json!("themeTokens.radius")));
    assert!(result["invalid"].as_array().unwrap().contains(&json!("panels.title_bar")));
    assert!(result["invalid"].as_array().unwrap().contains(&json!("themeTokens.text")));
    let accent = egui::Color32::from_rgba_unmultiplied(0x12, 0x34, 0x56, 0x78);
    for kind in [ThemeKind::Light, ThemeKind::Dark, ThemeKind::Medium] {
        app.set_theme(&ctx, kind);
        assert_eq!(app.tokens.accent, accent);
    }
    call(&mut app, &ctx, "c0.set", json!({"themeTokens":null}));
    assert_eq!(
        app.tokens,
        Tokens::for_kind(ThemeKind::Medium).with_appearance(
            filmcraft_engine::settings::parse_hex(&app.session.prefs.appearance.highlight_color),
            app.session.prefs.appearance.accessible_contrast
        )
    );
    assert!(!app.ui.show_header && !app.ui.show_menu_bar && !app.ui.show_status_bar);
}
#[test]
fn community_disabled_removes_every_link_and_keeps_about_and_commands() {
    let mut app = FilmcraftApp::new(Default::default());
    let ctx = egui::Context::default();
    filmcraft_ui_egui::theme::install(&ctx, &app.tokens);
    call(&mut app, &ctx, "c0.set", json!({"panels":{"community_links":false}}));
    for (id, _, url) in filmcraft_ui_egui::links::ALL {
        assert!(!filmcraft_ui_egui::menus::menu_items(&app).iter().any(|item| item.id == id));
        assert!(filmcraft_ui_egui::menus::invoke(&mut app, &ctx, id, json!({})).is_err());
        filmcraft_ui_egui::links::open(&mut app, &ctx, url);
    }
    for mode in [filmcraft_ui_egui::state::Mode::Import, filmcraft_ui_egui::state::Mode::Edit] {
        app.ui.mode = mode;
        app.auto.begin_frame();
        let mut out = ctx.run_ui(egui::RawInput::default(), |ui| {
            filmcraft_ui_egui::header::show(&mut app, ui, egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1400.0, 38.0)));
            if mode == filmcraft_ui_egui::state::Mode::Import {
                filmcraft_ui_egui::panels::import_mode::show(&mut app, ui, egui::Rect::from_min_size(egui::pos2(0.0, 40.0), egui::vec2(1400.0, 800.0)));
            }
            app.dialog = Some(filmcraft_ui_egui::Dialog::About);
            filmcraft_ui_egui::panels::dialogs::show(&mut app, ui.ctx());
        });
        out.textures_delta.clear();
        assert!(!app.auto.elements.iter().any(|e| e.id == "header.discord"
            || e.id.starts_with("import.link.")
            || e.id.starts_with("about.discord")
            || e.id.starts_with("about.website")
            || e.id.starts_with("about.github")));
        assert!(app.auto.elements.iter().any(|element| element.id == "header.workspaceName"));
        if mode == filmcraft_ui_egui::state::Mode::Import {
            assert!(app.auto.elements.iter().any(|element| element.id == "import.file.import"));
        }
    }
    assert!(filmcraft_ui_egui::menus::invoke(&mut app, &ctx, "app.about", json!({})).is_ok());
    assert!(filmcraft_ui_egui::menus::invoke(&mut app, &ctx, "mode.import", json!({})).is_ok());
    assert!(filmcraft_ui_egui::menus::invoke(&mut app, &ctx, "mode.export", json!({})).is_ok());
    assert!(ctx.output(|o| o.commands.iter().all(|c| !matches!(c, egui::OutputCommand::OpenUrl(_)))));
}

#[test]
fn palette_registry_covers_every_upstream_color_field() {
    let mut app = FilmcraftApp::new(Default::default());
    let ctx = egui::Context::default();
    let names: Vec<&str> =
        include_str!("../src/theme.rs").lines().filter_map(|line| line.trim().strip_prefix("pub ").and_then(|line| line.strip_suffix(": Color32,"))).collect();
    assert_eq!(names.len(), 82);
    let tokens: serde_json::Map<String, Value> = names.iter().map(|name| (name.to_string(), json!("#c01234"))).collect();
    let report = call(&mut app, &ctx, "c0.set", json!({"themeTokens":tokens}));
    assert_eq!(report["applied"].as_array().unwrap().len(), names.len());
    assert_eq!(report["unknown"], json!([]));
    assert_eq!(report["invalid"], json!([]));
}

#[test]
fn cancellation_drops_native_picker_and_failed_save_keeps_pending_document() {
    let mut app = FilmcraftApp::new(Default::default());
    let ctx = egui::Context::default();
    let (sender, receiver) = std::sync::mpsc::channel();
    let mut receiver = Some(receiver);
    app.hooks.run_dialog = Some(Box::new(move |_| receiver.take().unwrap()));
    app.pick(filmcraft_ui_egui::native_dialogs::FileDialog::OpenProject, |_, _| panic!("cancelled continuation ran")).unwrap();
    assert!(app.native_dialog_open());
    assert_eq!(call(&mut app, &ctx, "c0.cancelFileDialog", json!({}))["cancelled"], true);
    assert!(!app.native_dialog_open());
    assert!(sender.send(vec!["late.fcproj".into()]).is_err());
    app.session.execute("file.newSequence", json!({"name":"Unsaved"})).unwrap();
    app.session.path = Some("/proc/c0vid-missing-project.fcproj".into());
    call(&mut app, &ctx, "c0.newDocument", json!({}));
    assert!(filmcraft_ui_egui::c0_host::files::resolve(&mut app, "save").is_err());
    assert!(app.c0.pending.is_some());
    assert!(app.session.is_dirty());
}

#[test]
fn document_bytes_roundtrip_preserves_sequences_view_and_external_media_names() {
    use filmcraft_project::MediaRef;
    let mut app = FilmcraftApp::new(Default::default());
    let ctx = egui::Context::default();
    app.session.execute("file.newSequence", json!({"name":"One","fps":25})).unwrap();
    app.session.execute("file.newSequence", json!({"name":"Two","fps":30})).unwrap();
    app.session.execute("file.newColorMatte", json!({"name":"Reference"})).unwrap();
    let project = std::sync::Arc::make_mut(&mut app.session.project);
    let item = project.items.values_mut().find(|item| item.as_media().is_some()).unwrap();
    let media = item.as_media_mut().unwrap();
    media.media = MediaRef::File { path: "/files/reference.png".into() };
    media.proxy = Some(MediaRef::File { path: "/files/reference-proxy.png".into() });
    let before = serde_json::to_value(&app.session.project).unwrap();
    let active = app.session.state.active_sequence;
    app.session.saved_revision = app.session.revision;
    call(&mut app, &ctx, "c0.goTo", json!({"frame":9}));
    let doc = call(&mut app, &ctx, "c0.documentBytes", json!({}));
    assert_eq!(doc["referencedMedia"].as_array().unwrap().len(), 2);
    assert_eq!(doc["referencedMedia"][0]["name"], "Reference");
    assert_eq!(doc["referencedMedia"][1]["proxy"], true);
    call(&mut app, &ctx, "c0.newDocument", json!({}));
    call(&mut app, &ctx, "c0.openFile", json!({"name":"roundtrip.fcproj","bytes":doc["bytes"]}));
    assert_eq!(serde_json::to_value(&app.session.project).unwrap(), before);
    assert_eq!(app.session.state.active_sequence, active);
    assert_eq!(call(&mut app, &ctx, "c0.inspect", json!({}))["playheadFrame"], 9);
}

// ---------- c0.selection and c0.place (the AI actions) ----------

mod ai {
    use super::*;
    use filmcraft_engine::Session;
    use filmcraft_engine::project::{ItemKind, Label, MediaClip, MediaRef, Project, SequenceSettings, TrackKind};
    use filmcraft_media::generators::GeneratorSource;
    use filmcraft_media::{DemoScene, Generator, MediaSource};
    use filmcraft_time::{FrameRate, Tick, TimeRange};
    use std::time::{Duration, Instant};

    /// A reply, asking again while the program frame renders, as the control loop does.
    fn answer(app: &mut FilmcraftApp, method: &str, params: Value) -> Value {
        let ctx = egui::Context::default();
        let started = Instant::now();
        loop {
            let (request, _) = ControlRequest::new(method, params.clone());
            match control::handle(app, &ctx, &request) {
                Outcome::Done(reply) => return reply,
                Outcome::Retry(_) if started.elapsed() < Duration::from_secs(60) => std::thread::sleep(Duration::from_millis(10)),
                other => panic!("{method} did not finish: {}", matches!(other, Outcome::Retry(_))),
            }
        }
    }
    fn ok(app: &mut FilmcraftApp, method: &str, params: Value) -> Value {
        let reply = answer(app, method, params);
        assert_eq!(reply["ok"], true, "{method}: {reply}");
        reply["result"].clone()
    }
    fn fails(app: &mut FilmcraftApp, params: Value) -> String {
        let (items, steps, dirty) = (app.session.project.items.len(), app.session.history.undo.len(), app.session.is_dirty());
        let reply = answer(app, "c0.place", params.clone());
        assert_eq!(reply["ok"], false, "{params}: {reply}");
        let error = reply["error"].as_str().expect("a plain message").to_owned();
        assert!(!error.is_empty());
        assert_eq!((app.session.project.items.len(), app.session.history.undo.len(), app.session.is_dirty()), (items, steps, dirty), "{error}: nothing changed");
        error
    }

    /// A 1920 x 1080 sequence "Look" with a red matte on V1 for 48 frames (2 s at 24 fps).
    fn app_with_sequence() -> (FilmcraftApp, u64) {
        let mut s = Session::default();
        s.execute("file.newSequence", json!({"name": "Look", "video": 2, "audio": 2, "width": 1920, "height": 1080})).unwrap();
        let matte = s.execute("file.newColorMatte", json!({"color": "#ff0000", "seconds": 3.0})).unwrap()["item"].as_u64().unwrap();
        let rate = s.sequence_rate();
        let clip = s.execute("timeline.place", json!({"item": matte, "frame": 0, "sourceIn": 0, "duration": rate.tick_of(48).0})).unwrap()["clips"][0].as_u64().unwrap();
        s.execute("edit.deselectAll", json!({})).unwrap();
        (FilmcraftApp::new(s), clip)
    }

    fn png(w: u32, h: u32) -> Vec<u8> {
        control::encode_png(&[200u8, 30, 30, 255].repeat((w * h) as usize), w, h).unwrap()
    }
    fn jpeg() -> Vec<u8> {
        let image = filmcraft_render::Image::filled(32, 18, [0.8, 0.1, 0.1, 1.0]);
        filmcraft_export::still::encode(&image, filmcraft_export::still::StillFormat::Jpeg, 8).unwrap()
    }
    fn wav() -> Vec<u8> {
        let samples: Vec<i16> = (0..4000).map(|i| ((i as f32 * 0.1).sin() * 8000.0) as i16).collect();
        let mut b = Vec::new();
        b.extend_from_slice(b"RIFF");
        b.extend_from_slice(&(36 + samples.len() as u32 * 2).to_le_bytes());
        b.extend_from_slice(b"WAVEfmt ");
        for v in [16u32, 0x0001_0001, 8000, 16000, 0x0010_0002] {
            b.extend_from_slice(&v.to_le_bytes());
        }
        b.extend_from_slice(b"data");
        b.extend_from_slice(&(samples.len() as u32 * 2).to_le_bytes());
        samples.iter().for_each(|s| b.extend_from_slice(&s.to_le_bytes()));
        b
    }
    /// A short H.264 movie (no sound) made with our own encoder.
    fn mp4() -> Vec<u8> {
        let rate = FrameRate::FPS_24;
        let dur = rate.tick_of(12);
        let src = GeneratorSource::new(Generator::Demo(DemoScene::OceanSunset), 128, 72, rate, dur);
        let clip = MediaClip {
            media: MediaRef::Generator(Generator::Demo(DemoScene::OceanSunset)),
            info: src.info().clone(),
            interpret: Default::default(),
            mark_in: None,
            mark_out: None,
            markers: vec![],
            offline: false,
            proxy: None,
            identity: None,
        };
        let mut p = Project::new("fixture");
        let item = p.add_item("scene", Label::Iris, ItemKind::Media(Box::new(clip)), None);
        let seq = p.new_sequence("s", SequenceSettings { width: 128, height: 72, frame_rate: rate, ..Default::default() }, 1, 0, None);
        let v = p.make_track_item(item, TrackKind::Video, Tick::ZERO, TimeRange::new(Tick::ZERO, dur), rate).unwrap();
        p.sequence_mut(seq).unwrap().video_tracks[0].items.push(v);
        let path = std::env::temp_dir().join(format!("filmcraft-c0-ai-{}.mp4", std::process::id()));
        let settings = filmcraft_engine::export::ExportSettings {
            format: filmcraft_engine::export::Format::H264,
            path: path.to_string_lossy().into_owned(),
            include_audio: false,
            ..Default::default()
        };
        let shared: filmcraft_media::SharedSource = std::sync::Arc::new(src);
        let provider = move |id| (id == item).then(|| shared.clone());
        filmcraft_engine::export::export(&std::sync::Arc::new(p), seq, &settings, &provider, &Default::default()).unwrap();
        let bytes = std::fs::read(&path).unwrap();
        let _ = std::fs::remove_file(path);
        bytes
    }
    fn media(bytes: &[u8], mime: &str, name: &str) -> Value {
        json!({"base64": STANDARD.encode(bytes), "mime": mime, "name": name})
    }
    fn clip_exists(app: &FilmcraftApp, id: u64) -> bool {
        app.session.active_sequence().is_some_and(|q| q.find_item(filmcraft_project::ClipId(id)).is_some())
    }

    #[test]
    fn selection_reads_the_timeline_and_the_program_frame() {
        let mut app = FilmcraftApp::new(Default::default());
        let reply = answer(&mut app, "c0.selection", json!({}));
        assert_eq!(reply["ok"], false, "no sequence open: {reply}");
        assert_eq!(answer(&mut app, "c0.selection", json!([]))["ok"], false);

        let (mut app, clip) = app_with_sequence();
        let (revision, journal) = (app.session.revision, app.session.journal.len());
        let empty = ok(&mut app, "c0.selection", json!({}));
        assert_eq!((empty["kind"].clone(), empty["sequenceName"].clone(), empty["selectedClips"].clone()), (json!("timeline"), json!("Look"), json!([])));
        assert_eq!(empty["playheadMs"], 0);
        assert!(empty["durationMs"].as_i64().is_some_and(|d| (1990..=2010).contains(&d)), "48 frames: {empty}");
        assert!(empty.get("inMs").is_none() && empty.get("outMs").is_none());
        let frame = STANDARD.decode(empty["frame"].as_str().expect("a base64 PNG frame")).unwrap();
        let image = image::load_from_memory_with_format(&frame, image::ImageFormat::Png).unwrap().to_rgba8();
        assert_eq!(image.dimensions(), (1280, 720), "longest side 1280");
        let c = image.get_pixel(640, 360).0;
        assert!(c[0] > 200 && c[1] < 40 && c[2] < 40, "the matte at the playhead: {c:?}");

        app.session.execute("timeline.select", json!({"clips": [clip]})).unwrap();
        app.session.execute("playhead.set", json!({"frame": 12})).unwrap();
        app.session.execute("markers.markIn", json!({"seconds": 0.5})).unwrap();
        app.session.execute("markers.markOut", json!({"seconds": 1.5})).unwrap();
        let (steps, rev) = (app.session.history.undo.len(), app.session.revision);
        let read = ok(&mut app, "c0.selection", json!({}));
        assert!(read["playheadMs"].as_i64().is_some_and(|t| (495..=505).contains(&t)), "{read}");
        assert!(read["inMs"].as_i64().is_some_and(|t| (495..=505).contains(&t)), "{read}");
        assert!(read["outMs"].as_i64().is_some_and(|t| (1450..=1550).contains(&t)), "{read}");
        let clips = read["selectedClips"].as_array().unwrap();
        assert_eq!(clips.len(), 1);
        assert_eq!((clips[0]["id"].clone(), clips[0]["kind"].clone(), clips[0]["startMs"].clone()), (json!(clip), json!("video"), json!(0)));
        assert_eq!(clips[0]["endMs"], read["durationMs"]);
        assert!(clips[0]["name"].as_str().is_some_and(|n| !n.is_empty()));
        assert!(read["frame"].is_string());
        assert_eq!((app.session.history.undo.len(), app.session.revision), (steps, rev), "reading changes nothing");
        assert!(revision <= rev && journal <= app.session.journal.len());
    }

    #[test]
    fn selection_names_every_kind_of_clip() {
        let (mut app, matte) = app_with_sequence();
        let sound = ok(&mut app, "c0.place", json!({"media": media(&wav(), "audio/wav", "tone.wav"), "toTimeline": {"track": "audio", "atMs": 0}}));
        let still = ok(&mut app, "c0.place", json!({"media": media(&png(64, 36), "image/png", "still.png"), "toTimeline": {"track": "video", "atMs": 2500}}));
        let title = app.session.execute("graphics.newText", json!({"text": "Hello", "time": 0})).unwrap()["clip"].as_u64().unwrap();
        let ids = [matte, sound["clipIds"][0].as_u64().unwrap(), still["clipIds"][0].as_u64().unwrap(), title];
        app.session.execute("timeline.select", json!({"clips": ids, "linked": false})).unwrap();
        let read = ok(&mut app, "c0.selection", json!({}));
        let kind = |id: u64| read["selectedClips"].as_array().unwrap().iter().find(|c| c["id"] == id).map(|c| c["kind"].clone());
        assert_eq!([kind(ids[0]), kind(ids[1]), kind(ids[2]), kind(ids[3])], [Some(json!("video")), Some(json!("audio")), Some(json!("image")), Some(json!("title"))], "{read}");
        let with_path = read["selectedClips"].as_array().unwrap().iter().find(|c| c["id"] == ids[1]).unwrap();
        assert!(with_path["sourcePath"].as_str().is_some_and(|p| p.ends_with(".wav")), "{with_path}");
    }

    #[test]
    fn place_media_of_each_kind_is_one_undo_step() {
        let (mut app, _) = app_with_sequence();
        let cases: Vec<(&str, Vec<u8>, &str, Option<(&str, Option<u64>)>)> = vec![
            ("audio/wav", wav(), "voice.wav", Some(("audio", Some(500)))),
            ("image/png", png(32, 18), "card.png", Some(("video", Some(1000)))),
            ("image/jpeg", jpeg(), "photo", Some(("video", Some(2000)))),
            ("video/mp4", mp4(), "shot.mp4", Some(("video", None))),
            ("audio/wav", wav(), "bin-only.wav", None),
            ("image/png", png(16, 16), "bin-only.png", None),
        ];
        for (mime, bytes, name, to) in cases {
            let (items, steps) = (app.session.project.items.len(), app.session.history.undo.len());
            app.session.saved_revision = app.session.revision;
            let mut params = json!({"media": media(&bytes, mime, name)});
            if let Some((track, at)) = to {
                params["toTimeline"] = json!({"track": track, "atMs": at});
            }
            let result = ok(&mut app, "c0.place", params);
            assert_eq!(result["placed"], "media", "{name}: {result}");
            let item = result["itemId"].as_u64().expect("the bin item id");
            assert!(app.session.project.item(filmcraft_project::ItemId(item)).is_some(), "{name}");
            let clips: Vec<u64> = result["clipIds"].as_array().unwrap().iter().filter_map(Value::as_u64).collect();
            assert_eq!(clips.is_empty(), to.is_none(), "{name}: {result}");
            assert!(clips.iter().all(|c| clip_exists(&app, *c)), "{name}: {result}");
            if let Some((_, Some(at))) = to {
                let first = app.session.active_sequence().unwrap().all_tracks().flat_map(|t| &t.items).find(|i| i.id.0 == clips[0]).unwrap().start;
                assert!((first.to_units_floor(1000) - at as i64).abs() <= 45, "{name}: placed at atMs (to the frame)");
                assert_eq!(result["atMs"], first.to_units_floor(1000), "{name}: the reply names where it landed");
            }
            assert!(app.session.is_dirty(), "{name}");
            assert_eq!(app.session.history.undo.len(), steps + 1, "{name}: one undo step");
            app.session.undo().unwrap();
            assert_eq!(app.session.project.items.len(), items, "{name}: one undo removes the item");
            assert!(clips.iter().all(|c| !clip_exists(&app, *c)), "{name}: and every clip");
            assert_eq!(app.session.history.undo.len(), steps);
        }
    }

    #[test]
    fn place_captions_adds_a_track_in_one_undo_step() {
        let (mut app, _) = app_with_sequence();
        let steps = app.session.history.undo.len();
        let result = ok(
            &mut app,
            "c0.place",
            json!({"captions": [{"startMs": 0, "endMs": 1000, "text": "One"}, {"startMs": 1000, "endMs": 1800, "text": "Two"}, {"startMs": 2000, "endMs": 3000, "text": "Three"}], "name": "Spoken"}),
        );
        assert_eq!((result["placed"].clone(), result["count"].clone()), (json!("captions"), json!(3)));
        let tracks = &app.session.active_sequence().unwrap().caption_tracks;
        assert_eq!((tracks.len(), tracks[0].name.as_str(), tracks[0].captions.len()), (1, "Spoken", 3));
        let mut texts: Vec<_> = tracks[0].captions.iter().collect();
        texts.sort_by_key(|c| c.start);
        assert_eq!(texts.iter().map(|c| c.text.as_str()).collect::<Vec<_>>(), ["One", "Two", "Three"]);
        assert_eq!(result["captionIds"].as_array().unwrap().len(), 3);
        assert!(app.session.is_dirty());
        assert_eq!(app.session.history.undo.len(), steps + 1);
        app.session.undo().unwrap();
        assert!(app.session.active_sequence().unwrap().caption_tracks.is_empty(), "one undo removes the track and every caption");
    }

    #[test]
    fn place_with_bad_params_is_an_error_and_changes_nothing() {
        let (mut app, _) = app_with_sequence();
        let (wav, png) = (wav(), png(8, 8));
        let big = "A".repeat(56_000_000);
        let cases = vec![
            json!({}),
            json!({"media": media(&wav, "audio/wav", "a.wav"), "captions": [{"startMs": 0, "endMs": 1, "text": "x"}]}),
            json!({"media": "nope"}),
            json!({"media": media(&wav, "audio/ogg", "a.ogg")}),
            json!({"media": {"base64": "!!not base64!!", "mime": "audio/wav", "name": "a.wav"}}),
            json!({"media": {"base64": "", "mime": "audio/wav", "name": "a.wav"}}),
            json!({"media": media(&png, "audio/wav", "a.wav")}),
            json!({"media": media(&wav, "audio/wav", "a/b.wav")}),
            json!({"media": {"base64": STANDARD.encode(&wav), "mime": "audio/wav"}}),
            json!({"media": media(&wav, "audio/wav", "a.wav"), "toTimeline": {"track": "captions"}}),
            json!({"media": media(&wav, "audio/wav", "a.wav"), "toTimeline": {"track": "video"}}),
            json!({"media": media(&png, "image/png", "a.png"), "toTimeline": {"track": "audio"}}),
            json!({"media": media(&wav, "audio/wav", "a.wav"), "toTimeline": {"track": "audio", "atMs": -5}}),
            json!({"media": media(&wav, "audio/wav", "a.wav"), "toTimeline": {"track": "audio", "atMs": "soon"}}),
            json!({"media": media(&wav, "audio/wav", "a.wav"), "toTimeline": "audio"}),
            json!({"media": {"base64": big, "mime": "video/mp4", "name": "a.mp4"}}),
            // a PNG header with nothing behind it: the import fails and leaves nothing behind
            json!({"media": media(&[&png[..8], &[0u8; 40][..]].concat(), "image/png", "broken.png"), "toTimeline": {"track": "video"}}),
            json!({"captions": []}),
            json!({"captions": "text"}),
            json!({"captions": [{"startMs": 1000, "endMs": 1000, "text": "x"}]}),
            json!({"captions": [{"startMs": 0, "endMs": 10, "text": "  "}]}),
            json!({"captions": [{"startMs": 0, "endMs": "ten", "text": "x"}]}),
            json!({"captions": [{"startMs": 0, "endMs": 10, "text": "x"}], "name": 7}),
            json!({"captions": (0..2001).map(|i| json!({"startMs": i * 10, "endMs": i * 10 + 5, "text": "x"})).collect::<Vec<_>>()}),
        ];
        for params in cases {
            fails(&mut app, params);
        }
        // the movie has no sound: the import works, the sound placement fails, and the import is taken back
        let message = fails(&mut app, json!({"media": media(&mp4(), "video/mp4", "silent.mp4"), "toTimeline": {"track": "audio"}}));
        assert!(message.contains("sound"), "{message}");
        // nothing to place on
        let mut empty = FilmcraftApp::new(Default::default());
        fails(&mut empty, json!({"media": media(&wav, "audio/wav", "a.wav"), "toTimeline": {"track": "audio"}}));
        fails(&mut empty, json!({"captions": [{"startMs": 0, "endMs": 10, "text": "x"}]}));
        let reply = answer(&mut empty, "c0.place", json!({"media": media(&wav, "audio/wav", "a.wav")}));
        assert_eq!(reply["ok"], true, "a bin import needs no sequence: {reply}");
    }

    #[test]
    fn both_methods_are_registered() {
        for method in ["c0.selection", "c0.place"] {
            assert!(filmcraft_ui_egui::c0_host::METHODS.contains(&method), "{method}");
        }
    }
}
