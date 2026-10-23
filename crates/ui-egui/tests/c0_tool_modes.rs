//! The guest page's tool-strip modes: `c0.set {panels: {toolbar, toolbar_double, toolbar_floating,
//! toolbar_float}}` and the same keys in `c0.inspect().panes`. The Tools panel of the editing
//! workspaces is docked in one column (upstream), docked in two, floating in one or in two.

mod c0_support;

use c0_support::*;
use egui::{Rect, pos2, vec2};
use filmcraft_ui_egui::c0_host::dock_modes::{self, DOUBLE_W, SINGLE_W};
use filmcraft_ui_egui::dock;
use serde_json::json;

const TOOLS: [&str; 8] = ["tools.Selection", "tools.TrackSelectForward", "tools.Ripple", "tools.Razor", "tools.Slip", "tools.Pen", "tools.Hand", "tools.Type"];

/// A reply's key list, sorted: the order of a JSON object's keys is not part of the contract.
fn sorted(list: &serde_json::Value) -> Vec<String> {
    let mut names: Vec<String> = list.as_array().into_iter().flatten().filter_map(|name| name.as_str().map(str::to_string)).collect();
    names.sort();
    names
}

fn all_tools_drawn(h: &Page) -> Vec<Rect> {
    TOOLS.iter().map(|id| must(h, id)).collect()
}

fn tools_column(h: &Page) -> Rect {
    must(h, "panel.Tools")
}

#[test]
fn defaults_are_upstreams_and_inspect_reports_all_three_keys() {
    let mut h = page();
    assert_eq!(h.state().c0.dock, dock_modes::Modes::default());
    let panes = panes(&mut h);
    assert_eq!((panes["toolbar"].clone(), panes["toolbar_double"].clone(), panes["toolbar_floating"].clone()), (json!(true), json!(false), json!(false)));
    assert_eq!(panes["toolbar_float"], json!({"pos": null}));
    // the other panes keep their defaults
    for key in ["title_bar", "menu_bar", "header", "status_bar", "start_screen", "community_links"] {
        assert_eq!(panes[key], json!(true), "{key}");
    }
    // upstream's column: 64 wide, one button per row, centred
    assert!(near(tools_column(&h).width(), SINGLE_W));
    let buttons = all_tools_drawn(&h);
    assert!(buttons.windows(2).all(|pair| pair[1].min.y > pair[0].min.y && pair[1].min.x == pair[0].min.x));
    assert!((buttons[0].center().x - tools_column(&h).center().x).abs() < 0.01);
    assert!(rect(&h, "c0.tools.palette").is_none());
    assert!(dock_modes::view(h.state(), &h.state().ui.dock).is_none(), "default switches leave the dock tree alone");
}

#[test]
fn every_key_sets_inspects_and_reports_unknown_and_invalid() {
    let mut h = page();
    let reply = call(
        &mut h,
        "c0.set",
        json!({"panels": {"toolbar": false, "toolbar_double": true, "toolbar_floating": true, "toolbar_float": {"pos": [20, 30]}, "nope": true, "toolbar_pinned": 1}}),
    );
    assert_eq!(sorted(&reply["applied"]), ["panels.toolbar", "panels.toolbar_double", "panels.toolbar_float", "panels.toolbar_floating"]);
    assert_eq!(sorted(&reply["unknown"]), ["panels.nope", "panels.toolbar_pinned"]);
    assert_eq!(reply["invalid"], json!([]));
    let panes = panes(&mut h);
    assert_eq!((panes["toolbar"].clone(), panes["toolbar_double"].clone(), panes["toolbar_floating"].clone()), (json!(false), json!(true), json!(true)));
    assert_eq!(panes["toolbar_float"], json!({"pos": [20.0, 30.0]}));
    // wrong types change nothing and are reported as invalid
    let reply = call(
        &mut h,
        "c0.set",
        json!({"panels": {"toolbar": "yes", "toolbar_double": 1, "toolbar_floating": null, "toolbar_float": {"pos": [1, "x"]}, "toolbar_float_": {}}}),
    );
    assert_eq!(reply["applied"], json!([]));
    assert_eq!(sorted(&reply["invalid"]), ["panels.toolbar", "panels.toolbar_double", "panels.toolbar_float", "panels.toolbar_floating"]);
    assert_eq!(reply["unknown"], json!(["panels.toolbar_float_"]));
    let after = self::panes(&mut h);
    assert_eq!((after["toolbar"].clone(), after["toolbar_double"].clone(), after["toolbar_floating"].clone()), (json!(false), json!(true), json!(true)));
    for hostile in [json!({"pos": [1.0e12, 0]}), json!({"pos": [1]}), json!({"pos": "a"}), json!({}), json!({"pos": [1, 2], "x": 1}), json!(3)] {
        let reply = call(&mut h, "c0.set", json!({"panels": {"toolbar_float": hostile}}));
        assert_eq!(reply["invalid"], json!(["panels.toolbar_float"]), "{hostile}");
    }
    assert_eq!(self::panes(&mut h)["toolbar_float"], json!({"pos": [20.0, 30.0]}), "invalid positions are ignored");
    // pos null returns the palette to its default corner
    call(&mut h, "c0.set", json!({"panels": {"toolbar_float": {"pos": null}}}));
    assert_eq!(self::panes(&mut h)["toolbar_float"], json!({"pos": null}));
    // a non-object panels map is an error envelope, as for the other switches
    assert_eq!(raw(&mut h, "c0.set", json!({"panels": []}))["ok"], false);
}

#[test]
fn docked_two_columns_widens_the_panel_and_keeps_every_tool_reachable() {
    let mut h = page();
    let before_timeline = must(&h, "panel.Timeline");
    set_panels(&mut h, json!({"toolbar_double": true}));
    assert!(near(tools_column(&h).width(), DOUBLE_W));
    let buttons = all_tools_drawn(&h);
    // two per row, left then right, rows going down
    for (index, button) in buttons.iter().enumerate() {
        assert_eq!(button.min.y, buttons[index - index % 2].min.y, "tool {index} shares its row");
        assert_eq!(index % 2 == 1, button.min.x > buttons[0].min.x, "tool {index} column");
    }
    assert!(buttons[2].min.y > buttons[0].min.y);
    let column = tools_column(&h);
    assert!(buttons.iter().all(|button| column.contains_rect(*button)), "every button lies inside the panel");
    assert!(buttons[7].max.y <= column.max.y, "all eight groups fit");
    // the Timeline gives the extra width and keeps its top, bottom and right edges
    let timeline = must(&h, "panel.Timeline");
    assert_eq!((timeline.min.y, timeline.max.y, timeline.max.x), (before_timeline.min.y, before_timeline.max.y, before_timeline.max.x));
    assert!((before_timeline.min.x + (DOUBLE_W - SINGLE_W) - timeline.min.x).abs() < 0.01);
    // back to one column: upstream's rectangles exactly
    set_panels(&mut h, json!({"toolbar_double": false}));
    assert_eq!(must(&h, "panel.Timeline"), before_timeline);
    assert!(near(tools_column(&h).width(), SINGLE_W));
}

#[test]
fn floating_draws_the_same_tools_in_a_palette_and_frees_the_dock_column() {
    let mut h = page();
    let before = h.state().ui.dock.clone();
    let docked_timeline = must(&h, "panel.Timeline");
    set_panels(&mut h, json!({"toolbar_floating": true}));
    assert!(rect(&h, "panel.Tools").is_none(), "the docked column is gone");
    let palette = must(&h, "c0.tools.palette");
    assert!(near(palette.width(), SINGLE_W));
    let buttons = all_tools_drawn(&h);
    assert!(buttons.iter().all(|button| palette.contains_rect(*button)));
    assert!(buttons.windows(2).all(|pair| pair[1].min.x == pair[0].min.x && pair[1].min.y > pair[0].min.y));
    let timeline = must(&h, "panel.Timeline");
    assert!(timeline.min.x <= docked_timeline.min.x - SINGLE_W + 0.01, "the Timeline took the column's width and its gutter");
    // two columns floating
    set_panels(&mut h, json!({"toolbar_double": true}));
    let palette = must(&h, "c0.tools.palette");
    assert!(near(palette.width(), DOUBLE_W));
    let buttons = all_tools_drawn(&h);
    assert_eq!(buttons[0].min.y, buttons[1].min.y);
    assert!(buttons[1].min.x > buttons[0].min.x);
    assert!(buttons.iter().all(|button| palette.contains_rect(*button)));
    // the app's layout was never touched: nothing in it, in the floating list or in a saved workspace changed
    assert_eq!(h.state().ui.dock, before);
    assert!(h.state().ui.floating.is_empty());
    // docked again: everything as at the start
    set_panels(&mut h, json!({"toolbar_floating": false, "toolbar_double": false}));
    assert!(rect(&h, "c0.tools.palette").is_none());
    assert_eq!(must(&h, "panel.Timeline"), docked_timeline);
    assert!(near(tools_column(&h).width(), SINGLE_W));
}

#[test]
fn the_palette_moves_by_its_grip_and_stays_inside_the_canvas_area() {
    let mut h = page();
    set_panels(&mut h, json!({"toolbar_floating": true}));
    let start = must(&h, "c0.tools.palette");
    let grip = must(&h, "c0.tools.palette.move");
    drag_by(&mut h, grip.center(), vec2(120.0, 90.0));
    let moved = must(&h, "c0.tools.palette");
    assert!((moved.min.x - start.min.x - 120.0).abs() < 2.0 && (moved.min.y - start.min.y - 90.0).abs() < 2.0, "{start:?} -> {moved:?}");
    let panes = panes(&mut h);
    assert!(panes["toolbar_float"]["pos"].is_array(), "the host can read where it sits");
    // dragged far outside the window it stays inside the body
    let grip = must(&h, "c0.tools.palette.move");
    drag_by(&mut h, grip.center(), vec2(9000.0, 9000.0));
    let edge = must(&h, "c0.tools.palette");
    let window = Rect::from_min_size(pos2(0.0, 0.0), vec2(1600.0, 980.0));
    assert!(window.contains_rect(edge), "{edge:?}");
    // a position set by the host is applied (offset from the canvas area's corner) and clamped
    set_panels(&mut h, json!({"toolbar_float": {"pos": [300, 200]}}));
    let placed = must(&h, "c0.tools.palette");
    set_panels(&mut h, json!({"toolbar_float": {"pos": [310, 200]}}));
    assert!((must(&h, "c0.tools.palette").min.x - placed.min.x - 10.0).abs() < 0.01);
    set_panels(&mut h, json!({"toolbar_float": {"pos": [-5000, -5000]}}));
    assert!(window.contains_rect(must(&h, "c0.tools.palette")));
}

#[test]
fn toolbar_false_hides_the_tools_and_true_brings_them_back() {
    let mut h = page();
    let timeline = must(&h, "panel.Timeline");
    set_panels(&mut h, json!({"toolbar": false}));
    assert!(rect(&h, "panel.Tools").is_none() && rect(&h, "c0.tools.palette").is_none() && rect(&h, "tools.Selection").is_none());
    set_panels(&mut h, json!({"toolbar_floating": true}));
    assert!(rect(&h, "c0.tools.palette").is_none(), "hidden stays hidden whatever the mode");
    set_panels(&mut h, json!({"toolbar": true}));
    assert!(rect(&h, "c0.tools.palette").is_some());
    set_panels(&mut h, json!({"toolbar_floating": false}));
    assert_eq!(must(&h, "panel.Timeline"), timeline);
}

#[test]
fn tools_are_clickable_in_all_four_modes_and_the_choice_survives_a_mode_change() {
    let mut h = page();
    let modes = [(false, false), (true, false), (false, true), (true, true)];
    for (index, (double, floating)) in modes.into_iter().enumerate() {
        set_panels(&mut h, json!({"toolbar_double": double, "toolbar_floating": floating}));
        let (id, tool) = if index % 2 == 0 { ("tools.Razor", "Razor") } else { ("tools.Hand", "Hand") };
        click(&mut h, id);
        assert_eq!(format!("{:?}", h.state().ui.tool), tool, "double {double}, floating {floating}");
        click(&mut h, "tools.Selection");
        assert_eq!(format!("{:?}", h.state().ui.tool), "Selection");
    }
}

#[test]
fn every_factory_workspace_with_tools_supports_all_four_modes() {
    for name in dock::WORKSPACES {
        let mut h = page();
        h.state_mut().ui.dock = dock::workspace(name);
        settle(&mut h);
        let had = panel_rects(&h);
        for (double, floating) in [(true, false), (false, true), (true, true)] {
            set_panels(&mut h, json!({"toolbar_double": double, "toolbar_floating": floating}));
            let rect_tools = if floating { must(&h, "c0.tools.palette") } else { tools_column(&h) };
            assert!(near(rect_tools.width(), if double { DOUBLE_W } else { SINGLE_W }), "{name}");
            assert!(all_tools_drawn(&h).iter().all(|button| rect_tools.contains_rect(*button)), "{name} {double} {floating}");
        }
        set_panels(&mut h, json!({"toolbar_double": false, "toolbar_floating": false}));
        assert_eq!(panel_rects(&h), had, "{name}: back to upstream's layout");
    }
}

#[test]
fn modes_are_not_saved_with_a_workspace() {
    let dir = std::env::temp_dir().join(format!("c0-tool-modes-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("create the scratch directory");
    let mut session = filmcraft_engine::Session::default();
    session.prefs_path = Some(dir.join("preferences.json"));
    let mut h = page_with(session);
    let before = h.state().ui.dock.clone();
    set_panels(&mut h, json!({"toolbar_floating": true, "toolbar_double": true, "toolbar_float": {"pos": [40, 40]}}));
    let ctx = h.ctx.clone();
    filmcraft_ui_egui::menus::invoke(h.state_mut(), &ctx, "window.workspace.saveAs", json!({"name": "Modes"})).expect("save the workspace");
    let prefs = dock::WorkspacePrefs::load(&dir.join(dock::WORKSPACES_FILE));
    let saved = dock::saved_docking(&prefs, "Modes");
    assert_eq!(saved.root, before, "the saved layout still has the docked Tools column");
    assert!(saved.floating.is_empty());
    assert!(saved.root.contains(dock::PanelKind::Tools));
    let text = std::fs::read_to_string(dir.join(dock::WORKSPACES_FILE)).expect("read the workspaces file");
    assert!(!text.contains("toolbar"), "no mode in the file");
    let preferences = std::fs::read_to_string(dir.join("preferences.json")).unwrap_or_default();
    assert!(!preferences.contains("toolbar_floating") && !preferences.contains("toolbar_double"));
    std::fs::remove_dir_all(dir).expect("remove the scratch directory");
}
