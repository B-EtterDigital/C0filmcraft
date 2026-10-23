use crate::FilmcraftApp;
use serde_json::{Value, json};

pub fn inspect(app: &FilmcraftApp) -> Value {
    let modes = &app.c0.dock;
    json!({"toolbar":modes.toolbar,"toolbar_double":modes.toolbar_double,"toolbar_floating":modes.toolbar_floating,
        "toolbar_float":{"pos":modes.toolbar_pos},"right_dock_collapsed":modes.right_dock_collapsed,
        "right_dock":super::dock_modes::has_right_dock(&app.ui.dock),"title_bar":app.c0.chrome.title_bar,"menu_bar":app.ui.show_menu_bar && app.c0.chrome.menu_bar,
        "header":app.ui.show_header && app.c0.chrome.header && app.c0.chrome.title_bar,
        "status_bar":app.ui.show_status_bar && app.c0.chrome.status_bar,"start_screen":app.c0.chrome.start_screen,"community_links":app.c0.chrome.community_links})
}

pub fn set(app: &mut FilmcraftApp, ctx: &egui::Context, params: &Value) -> Result<Value, String> {
    if params.get("panels").is_some_and(|value| !value.is_object()) {
        return Err("panels must be an object".into());
    }
    if params.get("themeTokens").is_some_and(|value| !value.is_object() && !value.is_null()) {
        return Err("themeTokens must be an object or null".into());
    }
    let mut report = crate::theme::host::Report::default();
    for key in params.as_object().into_iter().flat_map(|map| map.keys()) {
        if key != "themeTokens" && key != "panels" {
            report.unknown.push(key.clone());
        }
    }
    if let Some(value) = params.get("themeTokens") {
        report = crate::theme::host::set(ctx, value, report)?;
        app.set_theme(ctx, app.tokens.kind);
    }
    if let Some(value) = params.get("panels") {
        let entries = value.as_object().ok_or("panels must be an object")?;
        for (key, value) in entries {
            if super::dock_modes::set_key(&mut app.c0.dock, key, value, &mut report) {
                continue;
            }
            let target = match key.as_str() {
                "title_bar" => &mut app.c0.chrome.title_bar,
                "menu_bar" => &mut app.c0.chrome.menu_bar,
                "header" => &mut app.c0.chrome.header,
                "status_bar" => &mut app.c0.chrome.status_bar,
                "start_screen" => &mut app.c0.chrome.start_screen,
                "community_links" => &mut app.c0.chrome.community_links,
                _ => {
                    report.unknown.push(format!("panels.{key}"));
                    continue;
                }
            };
            if let Some(enabled) = value.as_bool() {
                *target = enabled;
                match key.as_str() {
                    "menu_bar" => app.ui.show_menu_bar = enabled,
                    "header" => app.ui.show_header = enabled,
                    "status_bar" => app.ui.show_status_bar = enabled,
                    _ => {}
                }
                report.applied.push(format!("panels.{key}"));
            } else {
                report.invalid.push(format!("panels.{key}"));
            }
        }
        if !app.c0.chrome.start_screen && app.ui.mode == crate::state::Mode::Import {
            app.ui.mode = crate::state::Mode::Edit;
        }
        if !app.c0.chrome.community_links {
            app.ui.failed_link = None;
        }
    }
    Ok(json!({"applied":report.applied,"unknown":report.unknown,"invalid":report.invalid}))
}
