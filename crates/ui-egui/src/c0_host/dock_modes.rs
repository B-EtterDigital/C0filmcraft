//! Tool-strip modes and the foldable right dock of the C0vid guest.
//!
//! Both are *views* of the dock tree. The tree the user edits, saves in a workspace and restores is
//! never changed: [`view`] clones it, applies the host's switches to the clone, and the dock lays
//! out the clone. At default switch values [`view`] returns `None` and the dock is upstream's.
//!
//! * Tools panel (Premiere's tools): docked in one column (upstream: 64 pt), docked in two
//!   columns, floating in one column or two, or hidden (`toolbar: false`). Floating draws the same
//!   [`crate::panels::tools::show`] in a palette the user can move by its grip.
//! * Right dock: the rightmost column of panels that holds none of the editing monitors or the
//!   Timeline (Properties, Lumetri, Essential Graphics...). Folded, each of its groups narrows to an
//!   icon strip; an icon opens its panel in a flyout over the canvas, so nothing else moves.

use craft_ui::docking::Action;
use egui::{Align2, Rect, Sense, Stroke, StrokeKind, Vec2, pos2, vec2};
use serde_json::Value;

use crate::FilmcraftApp;
use crate::dock::{DockNode, Group, PanelKind, SplitSize};
use crate::icons::{self, Icon};

/// Side of a tool button, as upstream's Tools panel.
pub const TOOL_SIZE: f32 = 32.0;
/// Space between tool buttons, as upstream's Tools panel.
const TOOL_GAP: f32 = 4.0;
/// Upstream's Tools column (the factory workspaces' `FixedA(64.0)`).
pub const SINGLE_W: f32 = 64.0;
/// Two tool columns: two buttons, the gap and a margin of 8 pt on both sides.
pub const DOUBLE_W: f32 = 84.0;
/// A folded dock group.
pub const STRIP_W: f32 = 36.0;
/// Height of the palette's grip, as a compact docked panel's.
const GRIP_H: f32 = 8.0;
/// The palette stays this far inside the canvas area.
const EDGE: f32 = 6.0;
/// Side of a strip button.
const STRIP_BUTTON: f32 = 28.0;
/// The flyout's narrowest width.
const FLYOUT_MIN_W: f32 = 260.0;

/// The host's switches for the tool strip and the right dock. Session-local, never saved.
#[derive(Clone, Debug, PartialEq)]
pub struct Modes {
    /// The Tools panel is shown at all.
    pub toolbar: bool,
    /// Two tool columns instead of one (docked or floating).
    pub toolbar_double: bool,
    /// The tools are a palette over the canvas instead of a docked column.
    pub toolbar_floating: bool,
    /// The palette's corner in points from the canvas area's corner; `None`: its default corner.
    pub toolbar_pos: Option<[f32; 2]>,
    /// The right dock is folded to icon strips.
    pub right_dock_collapsed: bool,
    /// The panel open in the flyout of a folded dock.
    pub flyout: Option<PanelKind>,
}

impl Default for Modes {
    fn default() -> Self {
        Self { toolbar: true, toolbar_double: false, toolbar_floating: false, toolbar_pos: None, right_dock_collapsed: false, flyout: None }
    }
}

impl Modes {
    /// Whether the dock is upstream's.
    fn is_default(&self) -> bool {
        self.toolbar && !self.toolbar_double && !self.toolbar_floating && !self.right_dock_collapsed
    }
}

/// A `panels` entry of `c0.set` that belongs here; false when the key is not one of ours. The
/// result goes into `report` as `panels.<key>`.
pub fn set_key(modes: &mut Modes, key: &str, value: &Value, report: &mut crate::theme::host::Report) -> bool {
    let name = format!("panels.{key}");
    let valid = match key {
        "toolbar_float" => float_pos(value).map(|pos| modes.toolbar_pos = pos).is_some(),
        "toolbar" | "toolbar_double" | "toolbar_floating" | "right_dock_collapsed" => {
            let flag = match key {
                "toolbar" => &mut modes.toolbar,
                "toolbar_double" => &mut modes.toolbar_double,
                "toolbar_floating" => &mut modes.toolbar_floating,
                _ => &mut modes.right_dock_collapsed,
            };
            value.as_bool().map(|on| *flag = on).is_some()
        }
        _ => return false,
    };
    if !modes.right_dock_collapsed {
        modes.flyout = None;
    }
    if valid {
        report.applied.push(name)
    } else {
        report.invalid.push(name)
    }
    true
}

/// `{"pos": null | [x, y]}`: the palette's corner. Only finite, reasonable offsets are accepted.
fn float_pos(value: &Value) -> Option<Option<[f32; 2]>> {
    const LIMIT: f64 = 1.0e6;
    let object = value.as_object().filter(|object| object.len() == 1)?;
    let pos = object.get("pos")?;
    if pos.is_null() {
        return Some(None);
    }
    let [x, y] = pos.as_array()?.as_slice() else { return None };
    let (x, y) = (x.as_f64()?, y.as_f64()?);
    (x.abs() <= LIMIT && y.abs() <= LIMIT).then_some(Some([x as f32, y as f32]))
}

// ---------------------------------------------------------------- the Tools panel's columns

/// Tool columns the Tools panel draws in `rect`: two when asked for and wide enough, else one.
pub fn columns(app: &FilmcraftApp, rect: Rect) -> usize {
    if app.c0.dock.toolbar_double && rect.width() >= DOUBLE_W { 2 } else { 1 }
}

/// Left edge of the `index`th tool button. One column is upstream's: centred in `rect`.
pub fn column_x(rect: Rect, index: usize, columns: usize, size: f32) -> f32 {
    let columns = columns.max(1);
    let total = size * columns as f32 + TOOL_GAP * (columns - 1) as f32;
    rect.center().x - total / 2.0 + (index % columns) as f32 * (size + TOOL_GAP)
}

/// How far down the next button sits after the `index`th: the row pitch once a row is full.
pub fn advance(index: usize, columns: usize, pitch: f32) -> f32 {
    if (index + 1).is_multiple_of(columns.max(1)) { pitch } else { 0.0 }
}

// ---------------------------------------------------------------- the dock tree's view

/// The dock tree as the host wants it drawn, and what the dock needs to undo the difference.
pub struct View {
    /// The tree to lay out instead of the real one.
    pub tree: DockNode,
    /// Gutters (paths in `tree`) that must not resize the real tree.
    inert: Vec<Vec<bool>>,
    /// Where the Tools split was dropped from `tree`: its path, and whether the second child stayed.
    removed: Option<(Vec<bool>, bool)>,
    /// The palette is drawn: the Tools panel is floating, not hidden.
    palette: bool,
    /// Layout paths (as [`crate::dock::Group::path`]) of the folded groups.
    strips: Vec<String>,
    /// The size of the right dock before it was folded.
    unfolded: Option<SplitSize>,
}

fn lone_tools(node: &DockNode) -> bool {
    matches!(node, DockNode::Tabs { panels, .. } if panels.as_slice() == [PanelKind::Tools])
}

/// The path of the split that has the Tools panel alone in one of its two children.
fn tools_split(node: &DockNode, path: &mut Vec<bool>) -> Option<Vec<bool>> {
    let DockNode::Split { a, b, .. } = node else { return None };
    if lone_tools(a) || lone_tools(b) {
        return Some(path.clone());
    }
    for (side, child) in [(false, a), (true, b)] {
        path.push(side);
        let found = tools_split(child, path);
        path.pop();
        if found.is_some() {
            return found;
        }
    }
    None
}

fn node_at_mut<'a>(node: &'a mut DockNode, path: &[bool]) -> Option<&'a mut DockNode> {
    let Some((side, rest)) = path.split_first() else { return Some(node) };
    match node {
        DockNode::Split { a, b, .. } => node_at_mut(if *side { b } else { a }, rest),
        DockNode::Tabs { .. } => None,
    }
}

/// Replaces the split at `path` by its child that is not the Tools panel; returns whether that was
/// the second child.
fn drop_tools(tree: &mut DockNode, path: &[bool]) -> Option<bool> {
    let node = node_at_mut(tree, path)?;
    let DockNode::Split { a, b, .. } = node else { return None };
    let keep_second = lone_tools(a);
    let empty = || DockNode::Tabs { panels: vec![], active: 0 };
    let kept = std::mem::replace(if keep_second { &mut **b } else { &mut **a }, empty());
    *node = kept;
    Some(keep_second)
}

/// Widens the Tools column to two buttons; a column the user made wider stays as it is.
fn widen_tools(tree: &mut DockNode, path: &[bool]) -> bool {
    let Some(DockNode::Split { vertical: false, size, a, b }) = node_at_mut(tree, path) else { return false };
    match (*size, lone_tools(a), lone_tools(b)) {
        (SplitSize::FixedA(width), true, _) => *size = SplitSize::FixedA(width.max(DOUBLE_W)),
        (SplitSize::FixedB(width), _, true) => *size = SplitSize::FixedB(width.max(DOUBLE_W)),
        _ => return false,
    }
    true
}

/// Panels that are the editing canvas: the right dock holds none of them.
fn canvas_panel(panel: PanelKind) -> bool {
    matches!(panel, PanelKind::Program | PanelKind::Timeline | PanelKind::Source | PanelKind::ReferenceMonitor)
}

/// Whether `node` is made of dock panels: some panel, none of them canvas, not only narrow strips.
fn dock_like(node: &DockNode) -> bool {
    let mut panels = Vec::new();
    node.panels(&mut panels);
    !panels.is_empty() && panels.iter().all(|panel| !canvas_panel(*panel)) && panels.iter().any(|panel| !panel.compact())
}

/// The path of the first side-by-side split whose second child is the right dock.
fn right_dock_split(node: &DockNode, path: &mut Vec<bool>) -> Option<Vec<bool>> {
    let DockNode::Split { vertical, a, b, .. } = node else { return None };
    if !*vertical && dock_like(b) {
        return Some(path.clone());
    }
    for (side, child) in [(false, a), (true, b)] {
        path.push(side);
        let found = right_dock_split(child, path);
        path.pop();
        if found.is_some() {
            return found;
        }
    }
    None
}

/// Whether the layout has a right dock to fold.
pub fn has_right_dock(tree: &DockNode) -> bool {
    right_dock_split(tree, &mut Vec::new()).is_some()
}

fn tab_paths(node: &DockNode, at: String, out: &mut Vec<String>) {
    match node {
        DockNode::Tabs { .. } => out.push(at),
        DockNode::Split { a, b, .. } => {
            tab_paths(a, format!("{at}a"), out);
            tab_paths(b, format!("{at}b"), out);
        }
    }
}

fn path_label(path: &[bool]) -> String {
    path.iter().map(|side| if *side { 'b' } else { 'a' }).collect()
}

/// `dock` as the host's switches want it drawn, or `None` at default values (upstream's dock).
pub fn view(app: &FilmcraftApp, dock: &DockNode) -> Option<View> {
    let modes = &app.c0.dock;
    if modes.is_default() {
        return None;
    }
    let mut view = View { tree: dock.clone(), inert: Vec::new(), removed: None, palette: false, strips: Vec::new(), unfolded: None };
    if let Some(path) = tools_split(&view.tree, &mut Vec::new()) {
        if !modes.toolbar || modes.toolbar_floating {
            view.removed = drop_tools(&mut view.tree, &path).map(|keep_second| (path, keep_second));
            view.palette = view.removed.is_some() && modes.toolbar;
        } else if modes.toolbar_double && widen_tools(&mut view.tree, &path) {
            view.inert.push(path);
        }
    }
    if modes.right_dock_collapsed
        && let Some(path) = right_dock_split(&view.tree, &mut Vec::new())
        && let Some(DockNode::Split { size, b, .. }) = node_at_mut(&mut view.tree, &path)
    {
        view.unfolded = Some(*size);
        *size = SplitSize::FixedB(STRIP_W);
        let label = path_label(&path);
        tab_paths(b, format!("{label}b"), &mut view.strips);
        view.inert.push(path);
    }
    Some(view)
}

impl View {
    /// The floating palette is wanted: the Tools panel is in the layout, shown and floating.
    pub fn palette(&self) -> bool {
        self.palette
    }

    /// Makes the gutter actions the laid-out view produced fit the real tree: a gutter the host
    /// pinned does nothing, and a path below the dropped Tools split gets its step back.
    pub fn settle(&self, actions: &mut Vec<Action<PanelKind>>) {
        actions.retain_mut(|action| {
            let Action::ResizeSplit { path, .. } = action else { return true };
            if self.inert.contains(path) {
                return false;
            }
            if let Some((at, keep_second)) = &self.removed
                && path.starts_with(at)
            {
                path.insert(at.len(), *keep_second);
            }
            true
        });
    }

    /// Takes the folded groups out of the laid-out `groups`; the dock draws the rest.
    pub fn take_strips(&self, groups: &mut Vec<Group>) -> Vec<Group> {
        let (strips, rest): (Vec<Group>, Vec<Group>) = std::mem::take(groups).into_iter().partition(|group| self.strips.contains(&group.path));
        *groups = rest;
        strips
    }
}

// ---------------------------------------------------------------- the floating tool palette

/// The palette's size for `columns` tool columns and the Tools panel's own button count.
fn palette_size(columns: usize) -> Vec2 {
    let rows = crate::state::Tool::groups().len().div_ceil(columns.max(1));
    let width = if columns > 1 { DOUBLE_W } else { SINGLE_W };
    vec2(width, GRIP_H + 4.0 + rows as f32 * (TOOL_SIZE + TOOL_GAP) + 4.0)
}

/// `pos` moved to lie inside `body`, `EDGE` away from its sides where it fits.
pub fn clamp_to(body: Rect, size: Vec2, pos: egui::Pos2) -> egui::Pos2 {
    if !body.is_finite() || !size.is_finite() || !pos.is_finite() {
        return pos;
    }
    let low = body.min + Vec2::splat(EDGE);
    let high = pos2((body.max.x - size.x - EDGE).max(low.x), (body.max.y - size.y - EDGE).max(low.y));
    pos2(pos.x.clamp(low.x, high.x), pos.y.clamp(low.y, high.y))
}

/// The Tools panel as a palette over the canvas area `body`, movable by its grip. It starts in the
/// corner of the Program monitor, `program`, and stays inside `body`.
pub fn palette(app: &mut FilmcraftApp, ui: &mut egui::Ui, body: Rect, program: Option<Rect>) {
    let ctx = ui.ctx().clone();
    let t = app.tokens;
    let size = palette_size(if app.c0.dock.toolbar_double { 2 } else { 1 });
    let wanted = match app.c0.dock.toolbar_pos {
        Some([x, y]) => body.min + vec2(x, y),
        None => program.map_or(body.min + vec2(12.0, 12.0), |monitor| monitor.min + vec2(10.0, 10.0)),
    };
    let pos = clamp_to(body, size, wanted);
    let id = egui::Id::unique("c0-tools-palette");
    let mut moved = None;
    egui::Area::new(id).order(egui::Order::Middle).fixed_pos(pos).constrain(false).interactable(true).show(&ctx, |ui| {
        let (rect, _) = ui.allocate_exact_size(size, Sense::hover());
        app.auto.add("c0.tools.palette", rect, "Tools");
        ui.painter().rect_filled(rect, t.radius, t.panel_bg);
        let grip = Rect::from_min_size(rect.min, vec2(rect.width(), GRIP_H));
        let response = ui.interact(grip, id.with("move"), Sense::drag());
        response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, response.enabled(), "Move tools"));
        app.auto.add_response("c0.tools.palette.move", &response, "Move tools");
        let dots = pos2(rect.center().x, rect.min.y + GRIP_H / 2.0);
        for dx in [-4.0, 0.0, 4.0] {
            ui.painter().circle_filled(dots + vec2(dx, 0.0), 1.0, t.text_faint);
        }
        if response.dragged() {
            moved = Some(pos + response.drag_delta());
        }
        crate::panels::tools::show(app, ui, Rect::from_min_max(pos2(rect.min.x, rect.min.y + GRIP_H), rect.max));
        ui.painter().rect_stroke(rect, t.radius, Stroke::new(1.0, t.focus), StrokeKind::Outside);
    });
    if let Some(next) = moved {
        let next = clamp_to(body, size, next) - body.min;
        app.c0.dock.toolbar_pos = Some([next.x, next.y]);
    }
}

// ---------------------------------------------------------------- the folded right dock

/// The code-drawn icon that stands for a panel in a folded dock.
fn panel_icon(panel: PanelKind) -> Icon {
    use PanelKind::*;
    match panel {
        Project => Icon::Folder,
        MediaBrowser => Icon::Drive,
        Libraries => Icon::Image,
        Info => Icon::Info,
        Effects | EffectControls => Icon::Fx,
        Markers => Icon::Marker,
        History => Icon::Undo,
        Source | Program | ReferenceMonitor => Icon::Film,
        AudioClipMixer | AudioTrackMixer | AudioMeters => Icon::Audio,
        Metadata => Icon::ListView,
        Timeline => Icon::Sequence,
        Tools => Icon::Wrench,
        LumetriColor | LumetriScopes | Properties => Icon::Adjust,
        EssentialGraphics => Icon::Sparkle,
        EssentialSound => Icon::Speaker,
        TextToSpeech => Icon::Mic,
        Text => Icon::Type,
        Events | Progress => Icon::Bell,
        Timecode => Icon::Clock,
        ProjectNotes => Icon::Captions,
    }
}

/// The folded groups: a column of icons each, an unfold button on the first. An icon opens its
/// panel in the flyout. Nothing else in the layout moves.
pub fn strips(app: &mut FilmcraftApp, ui: &mut egui::Ui, strips: &[Group], body: Rect) {
    let ctx = ui.ctx().clone();
    let t = app.tokens;
    for (index, group) in strips.iter().enumerate() {
        let rect = group.rect;
        ui.painter().rect_filled(rect, t.radius, t.panel_bg);
        app.auto.add(&format!("c0.dock.right.strip.{index}"), rect, "Right dock");
        let mut y = rect.min.y + 4.0;
        if index == 0 {
            let button = Rect::from_min_size(pos2(rect.center().x - STRIP_BUTTON / 2.0, y), vec2(STRIP_BUTTON, STRIP_BUTTON));
            let response = ui.interact(button, egui::Id::unique("c0-dock-unfold"), Sense::click()).on_hover_text(crate::i18n::t("Unfold"));
            response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, response.enabled(), "Unfold"));
            app.auto.add_response("c0.dock.right.unfold", &response, "Unfold");
            if response.hovered() {
                ui.painter().rect_filled(button, 4.0, t.hover);
            }
            icons::paint(ui.painter(), Rect::from_center_size(button.center(), vec2(15.0, 15.0)), Icon::ChevronLeft, t.icon);
            if response.clicked() {
                app.c0.dock.right_dock_collapsed = false;
                app.c0.dock.flyout = None;
            }
            y += STRIP_BUTTON + 6.0;
        }
        for panel in &group.panels {
            if y + STRIP_BUTTON > rect.max.y {
                break;
            }
            let button = Rect::from_min_size(pos2(rect.center().x - STRIP_BUTTON / 2.0, y), vec2(STRIP_BUTTON, STRIP_BUTTON));
            let response = ui.interact(button, egui::Id::unique(("c0-dock-strip", panel.id())), Sense::click()).on_hover_text(crate::i18n::t(panel.title()));
            let open = app.c0.dock.flyout == Some(*panel);
            response.widget_info(|| egui::WidgetInfo::selected(egui::Role::Button, response.enabled(), open, panel.title()));
            app.auto.add_response(&format!("c0.dock.right.panel.{}", panel.id()), &response, panel.title());
            if open {
                ui.painter().rect_filled(button, 4.0, t.accent);
            } else if response.hovered() {
                ui.painter().rect_filled(button, 4.0, t.hover);
            }
            let color = if open { egui::Color32::WHITE } else { t.icon };
            icons::paint(ui.painter(), Rect::from_center_size(button.center(), vec2(16.0, 16.0)), panel_icon(*panel), color);
            if response.clicked() {
                app.c0.dock.flyout = if open { None } else { Some(*panel) };
                if !open {
                    app.ui.focused = *panel;
                    // The panel is also the one the unfolded dock shows.
                    let _ = app.apply_dock(Action::Activate { panel: *panel });
                }
            }
            y += STRIP_BUTTON + 4.0;
        }
    }
    flyout(app, &ctx, strips, body);
}

/// The open panel of a folded dock, over the canvas beside its strip.
fn flyout(app: &mut FilmcraftApp, ctx: &egui::Context, strips: &[Group], body: Rect) {
    let Some(panel) = app.c0.dock.flyout else { return };
    let Some(group) = strips.iter().find(|group| group.panels.contains(&panel)) else {
        app.c0.dock.flyout = None;
        return;
    };
    let t = app.tokens;
    let width = FLYOUT_MIN_W.max(body.width() * 0.3).min(body.width() * 0.6).min(group.rect.min.x - body.min.x).max(0.0);
    let rect = Rect::from_min_max(pos2(group.rect.min.x - width, group.rect.min.y), pos2(group.rect.min.x - 2.0, group.rect.max.y));
    let id = egui::Id::unique("c0-dock-flyout");
    egui::Area::new(id).order(egui::Order::Middle).fixed_pos(rect.min).constrain(false).interactable(true).show(ctx, |ui| {
        let (rect, _) = ui.allocate_exact_size(rect.size(), Sense::hover());
        app.auto.add("c0.dock.right.flyout", rect, panel.title());
        ui.painter().rect_filled(rect, t.radius, t.panel_bg);
        ui.painter().text(rect.min + vec2(10.0, t.tab_h / 2.0), Align2::LEFT_CENTER, crate::i18n::t(panel.title()), crate::theme::Tokens::ui(12.0), t.text);
        let content = Rect::from_min_max(pos2(rect.min.x, rect.min.y + t.tab_h), rect.max);
        app.panel_body(ui, panel, content);
        ui.painter().rect_stroke(rect, t.radius, Stroke::new(1.0, t.focus), StrokeKind::Outside);
    });
    // Escape, or a press on the canvas outside the flyout and the strip, folds it away again.
    let flyout_layer = egui::LayerId::new(egui::Order::Middle, id);
    let strip_area = strips.iter().fold(Rect::NOTHING, |all, group| all.union(group.rect));
    // (the layer lookup takes its own lock, so it must not run inside `ctx.input`)
    let pressed = ctx.input(|input| input.pointer.any_pressed().then(|| input.pointer.interact_pos()).flatten());
    let outside = pressed
        .is_some_and(|pos| !strip_area.contains(pos) && ctx.layer_id_at(pos).is_some_and(|layer| layer != flyout_layer && layer.order <= egui::Order::Middle));
    if outside || ctx.input(|input| input.key_pressed(egui::Key::Escape)) {
        app.c0.dock.flyout = None;
    }
}

/// Where the Program monitor's picture area begins, for the palette's default corner.
pub fn program_content(groups: &[Group]) -> Option<Rect> {
    groups.iter().find(|group| group.panels.get(group.active) == Some(&PanelKind::Program)).map(|group| group.content)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn node_at<'a>(node: &'a DockNode, path: &[bool]) -> Option<&'a DockNode> {
        let Some((side, rest)) = path.split_first() else { return Some(node) };
        match node {
            DockNode::Split { a, b, .. } => node_at(if *side { b } else { a }, rest),
            DockNode::Tabs { .. } => None,
        }
    }

    fn tabs(panel: PanelKind) -> DockNode {
        DockNode::Tabs { panels: vec![panel], active: 0 }
    }

    #[test]
    fn one_column_is_upstreams_centre_and_two_columns_share_the_width() {
        let rect = Rect::from_min_size(pos2(0.0, 0.0), vec2(64.0, 400.0));
        assert_eq!(column_x(rect, 0, 1, TOOL_SIZE), rect.center().x - TOOL_SIZE / 2.0);
        assert_eq!(column_x(rect, 5, 1, TOOL_SIZE), column_x(rect, 0, 1, TOOL_SIZE));
        let wide = Rect::from_min_size(pos2(0.0, 0.0), vec2(DOUBLE_W, 400.0));
        let (left, right) = (column_x(wide, 0, 2, TOOL_SIZE), column_x(wide, 1, 2, TOOL_SIZE));
        assert_eq!(right - left, TOOL_SIZE + 4.0);
        assert_eq!((left, right + TOOL_SIZE), (8.0, DOUBLE_W - 8.0));
        assert_eq!((advance(0, 1, 36.0), advance(0, 2, 36.0), advance(1, 2, 36.0)), (36.0, 0.0, 36.0));
    }

    #[test]
    fn dropping_tools_keeps_the_sibling_and_gives_resizes_their_path_back() {
        let inner = DockNode::Split {
            vertical: false,
            size: SplitSize::FixedB(116.0),
            a: Box::new(tabs(PanelKind::Timeline)),
            b: Box::new(tabs(PanelKind::AudioMeters)),
        };
        let split = DockNode::Split { vertical: false, size: SplitSize::FixedA(64.0), a: Box::new(tabs(PanelKind::Tools)), b: Box::new(inner.clone()) };
        let root = DockNode::Split { vertical: true, size: SplitSize::Ratio(0.5), a: Box::new(tabs(PanelKind::Program)), b: Box::new(split) };
        let path = tools_split(&root, &mut Vec::new()).expect("a split holds the Tools panel");
        assert_eq!(path, vec![true]);
        let mut tree = root.clone();
        assert_eq!(drop_tools(&mut tree, &path), Some(true));
        assert_eq!(node_at(&tree, &path), Some(&inner));
        let view = View { tree, inert: vec![vec![]], removed: Some((path, true)), palette: false, strips: vec![], unfolded: None };
        let resize = |path: Vec<bool>| Action::ResizeSplit { path, size: craft_ui::layout::SplitSize::Ratio(0.3) };
        let mut actions = vec![resize(vec![]), resize(vec![true]), resize(vec![true, false])];
        view.settle(&mut actions);
        // the pinned root gutter is gone; the inner split moves one step down; deeper paths follow
        assert_eq!(actions, vec![resize(vec![true, true]), resize(vec![true, true, false])]);
    }

    #[test]
    fn the_right_dock_is_found_only_beside_canvas_panels() {
        for name in ["Editing", "Color", "Effects", "Audio", "Captions and Graphics", "All Panels"] {
            assert!(has_right_dock(&crate::dock::workspace(name)), "{name} has a right dock");
        }
        assert!(!has_right_dock(&crate::dock::workspace("Assembly")), "Assembly has none");
    }
}
