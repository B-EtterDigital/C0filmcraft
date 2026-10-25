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
