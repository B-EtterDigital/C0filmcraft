//! `c0.set {panels: {right_dock_collapsed}}`: the right dock folds to a narrow icon strip, each
//! panel's icon opens it over the canvas, and unfolding restores the layout exactly. Nothing in the
//! canvas area is re-laid out by opening a panel, and the saved layout is never changed.

mod c0_support;

use c0_support::*;
use egui::{Event, Key, Modifiers};
use filmcraft_ui_egui::c0_host::dock_modes::{self, STRIP_W};
use filmcraft_ui_egui::dock::{self, PanelKind};
use serde_json::json;

fn collapsed(h: &mut Page) -> bool {
    panes(h)["right_dock_collapsed"].as_bool().expect("a boolean pane")
}

#[test]
fn default_is_upstreams_unfolded_dock_and_inspect_reports_the_key() {
    let mut h = page();
    assert!(!h.state().c0.dock.right_dock_collapsed);
    let panes = panes(&mut h);
    assert_eq!(panes["right_dock_collapsed"], json!(false));
    assert_eq!(panes["right_dock"], json!(true), "the Editing workspace has a right dock");
    assert!(rect(&h, "panel.Properties").is_some());
    assert!(rect(&h, "c0.dock.right.strip.0").is_none() && rect(&h, "c0.dock.right.unfold").is_none());
    assert!(dock_modes::view(h.state(), &h.state().ui.dock).is_none());
}

#[test]
fn set_reports_applied_invalid_and_unknown_and_inspect_follows() {
    let mut h = page();
    let reply = call(&mut h, "c0.set", json!({"panels": {"right_dock_collapsed": true}}));
    assert_eq!(reply, json!({"applied": ["panels.right_dock_collapsed"], "unknown": [], "invalid": []}));
    assert!(collapsed(&mut h));
    let reply = call(&mut h, "c0.set", json!({"panels": {"right_dock_collapsed": "yes", "right_dock_collapse": true, "right_dock": false}}));
    assert_eq!(reply["applied"], json!([]));
    assert_eq!(reply["invalid"], json!(["panels.right_dock_collapsed"]));
    let mut unknown: Vec<_> = reply["unknown"].as_array().expect("a list").iter().filter_map(|v| v.as_str()).collect();
    unknown.sort();
    assert_eq!(unknown, ["panels.right_dock", "panels.right_dock_collapse"], "`right_dock` is read-only: it only reports");
    assert!(collapsed(&mut h), "an invalid value changes nothing");
    call(&mut h, "c0.set", json!({"panels": {"right_dock_collapsed": false}}));
    assert!(!collapsed(&mut h));
}

#[test]
fn folding_narrows_the_dock_to_a_strip_and_moves_nothing_below_or_above_it() {
    let mut h = page();
    let before = panel_rects(&h);
    let dock_before = must(&h, "panel.Properties");
    let program_before = must(&h, "panel.Program");
    let source_before = must(&h, "panel.Source");
    set_panels(&mut h, json!({"right_dock_collapsed": true}));
    // the next frame already shows it: the switch is immediate, with no intermediate layout
    let strip = must(&h, "c0.dock.right.strip.0");
    assert!(near(strip.width(), STRIP_W));
    assert!(strip.min.y < dock_before.min.y && near(strip.max.y, dock_before.max.y), "the strip spans the dock's height, tab strip included");
    assert!(near(strip.max.x, dock_before.max.x), "and sits at the same right edge");
    assert!(rect(&h, "panel.Properties").is_none(), "no panel body is squeezed into the strip");
    // every panel of the dock has an icon, in tab order, inside the strip; so does the unfold button
    let icons: Vec<_> =
        [PanelKind::Properties, PanelKind::EssentialGraphics, PanelKind::Text].iter().map(|p| must(&h, &format!("c0.dock.right.panel.{}", p.id()))).collect();
    assert!(icons.iter().all(|icon| strip.contains_rect(*icon)) && strip.contains_rect(must(&h, "c0.dock.right.unfold")));
    assert!(icons.windows(2).all(|pair| pair[1].min.y > pair[0].max.y), "stacked, not overlapping");
    // the canvas content did not jump: the monitors keep their top and bottom and the Source its
    // left edge; the Program gains the width the dock gave up; the rows below do not move at all
    let program = must(&h, "panel.Program");
    assert_eq!((program.min.y, program.max.y), (program_before.min.y, program_before.max.y));
    assert_eq!(must(&h, "panel.Source").min.x, source_before.min.x);
    assert!((program.max.x - program_before.max.x - (dock_before.width() - STRIP_W)).abs() < 0.5, "Program grew by the freed width");
    for (id, rect_before) in &before {
        if matches!(id.as_str(), "panel.Properties" | "panel.Program" | "panel.Source") {
            continue;
        }
        assert_eq!(rect(&h, id), Some(*rect_before), "{id} did not move");
    }
    // unfolding is exact
    set_panels(&mut h, json!({"right_dock_collapsed": false}));
    assert_eq!(panel_rects(&h), before);
}

#[test]
fn an_icon_opens_its_panel_in_a_flyout_without_moving_the_canvas_and_closes_again() {
    let mut h = page();
    set_panels(&mut h, json!({"right_dock_collapsed": true}));
    let folded = panel_rects(&h);
    let strip = must(&h, "c0.dock.right.strip.0");
    click(&mut h, "c0.dock.right.panel.Properties");
    let flyout = must(&h, "c0.dock.right.flyout");
    assert!(flyout.max.x <= strip.min.x, "the flyout opens beside the strip, over the canvas");
    assert!((flyout.min.y - strip.min.y).abs() < 0.5 && (flyout.max.y - strip.max.y).abs() < 0.5);
    assert!(flyout.width() >= 260.0);
    let body = must(&h, "panel.Properties");
    assert!(flyout.contains_rect(body), "the panel itself is drawn in the flyout");
    assert_eq!(h.state().c0.dock.flyout, Some(PanelKind::Properties));
    // nothing else in the layout moved to make room
    let with_flyout: Vec<_> = panel_rects(&h).into_iter().filter(|(id, _)| id != "panel.Properties").collect();
    let without: Vec<_> = folded.iter().filter(|(id, _)| id != "panel.Properties").cloned().collect();
    assert_eq!(with_flyout, without);
    // another icon switches panels; the same icon again closes
    click(&mut h, "c0.dock.right.panel.Text");
    assert_eq!(h.state().c0.dock.flyout, Some(PanelKind::Text));
    assert!(rect(&h, "panel.Properties").is_none() && rect(&h, "panel.Text").is_some());
    click(&mut h, "c0.dock.right.panel.Text");
    assert!(rect(&h, "c0.dock.right.flyout").is_none() && h.state().c0.dock.flyout.is_none());
    assert_eq!(panel_rects(&h), folded);
    // Escape closes it, and so does a press on the canvas
    click(&mut h, "c0.dock.right.panel.EssentialGraphics");
    assert!(rect(&h, "c0.dock.right.flyout").is_some());
    h.event(Event::Key { key: Key::Escape, physical_key: Some(Key::Escape), pressed: true, repeat: false, modifiers: Modifiers::NONE });
    settle(&mut h);
    assert!(rect(&h, "c0.dock.right.flyout").is_none());
    click(&mut h, "c0.dock.right.panel.Properties");
    let canvas = must(&h, "panel.Source").center();
    click_at(&mut h, canvas);
    assert!(rect(&h, "c0.dock.right.flyout").is_none(), "a press elsewhere folds the flyout away");
}

#[test]
fn the_unfold_button_and_the_host_switch_unfold_and_the_icon_chose_the_visible_tab() {
    let mut h = page();
    let dock = h.state().ui.dock.clone();
    set_panels(&mut h, json!({"right_dock_collapsed": true}));
    click(&mut h, "c0.dock.right.panel.Text");
    click(&mut h, "c0.dock.right.unfold");
    assert!(!collapsed(&mut h), "the strip's own button unfolds, and the host reads it back");
    assert!(rect(&h, "c0.dock.right.strip.0").is_none() && rect(&h, "c0.dock.right.flyout").is_none());
    assert!(rect(&h, "panel.Text").is_some(), "the panel whose icon was used is the one the dock shows");
    assert!(h.state().ui.dock.contains(PanelKind::Properties) && h.state().ui.dock.contains(PanelKind::Text));
    assert_ne!(h.state().ui.dock, dock, "only the active tab differs");
    set_panels(&mut h, json!({"right_dock_collapsed": true}));
    click(&mut h, "c0.dock.right.panel.Properties");
    set_panels(&mut h, json!({"right_dock_collapsed": false}));
    assert!(h.state().c0.dock.flyout.is_none(), "unfolding closes the flyout");
}

#[test]
fn every_workspace_with_a_right_dock_folds_and_unfolds_exactly_and_assembly_has_none() {
    for name in dock::WORKSPACES {
        let mut h = page();
        h.state_mut().ui.dock = dock::workspace(name);
        settle(&mut h);
        let before = panel_rects(&h);
        let has = panes(&mut h)["right_dock"] == json!(true);
        assert_eq!(has, name != "Assembly", "{name}");
        set_panels(&mut h, json!({"right_dock_collapsed": true}));
        if has {
            assert!(rect(&h, "c0.dock.right.strip.0").is_some(), "{name} shows a strip");
            assert!(rect(&h, "c0.dock.right.strip.0").is_some_and(|strip| near(strip.width(), STRIP_W)), "{name}");
        } else {
            assert_eq!(panel_rects(&h), before, "{name}: with no right dock the flag changes nothing in the layout");
            assert!(collapsed(&mut h), "but it is still reported as set");
        }
        set_panels(&mut h, json!({"right_dock_collapsed": false}));
        assert_eq!(panel_rects(&h), before, "{name}: unfolded again");
    }
}

#[test]
fn folding_never_changes_the_saved_layout_even_with_the_tools_floating() {
    let mut h = page();
    let before = h.state().ui.dock.clone();
    set_panels(&mut h, json!({"right_dock_collapsed": true, "toolbar_floating": true}));
    click(&mut h, "c0.dock.right.panel.Properties");
    // dragging the gutter beside the strip must not write the strip's width into the real layout
    let strip = must(&h, "c0.dock.right.strip.0");
    let gutter = egui::pos2(strip.min.x - 1.0, strip.center().y);
    drag_by(&mut h, gutter, egui::vec2(-80.0, 0.0));
    set_panels(&mut h, json!({"right_dock_collapsed": false, "toolbar_floating": false}));
    let mut after = h.state().ui.dock.clone();
    // the click on the icon made Properties the active tab; that is the only difference allowed
    assert!(after.activate(PanelKind::Properties));
    let mut expected = before;
    assert!(expected.activate(PanelKind::Properties));
    assert_eq!(after, expected);
}
