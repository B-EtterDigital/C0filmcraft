# Upstream pull 20261028: filmcraft `176e6a9c585f` into `c0/main`

Status: the tool-strip modes and the right-dock fold are ported onto craft_ui docking. `filmcraft-ui-egui`
compiles and its full test run is green. The web tests, the wasm check and the page build were queued at the
shared gate behind a whole-host hold when this section was written; their results are recorded below as they land.

## Commits on `fix`

| commit | what |
|---|---|
| `2afca3aa` | `merge: upstream 176e6a9c585f into c0/main` |
| `4ea59eff` | `chore(c0): keep upstream trademarks and CI out of c0/main` |
| `7174ea35` | `feat(c0): port tool-strip modes and right-dock fold onto craft_ui docking` |

## Port

Upstream replaced `dock::layout_with_actions`, `DockNode` and `Group` with `craft_ui::docking::{Layout, Node}` and
`DockArea::show_content`. The port keeps the contract, and both switches stay views of the layout.

- `c0_host/dock_modes.rs` works on `craft_ui::docking::Layout<PanelKind>` and `Node`. The Tools split is dropped
  (hidden or floating) or its `FixedFirst`/`FixedSecond` width is raised to two columns; the right-dock split is set
  to `FixedSecond(STRIP_W)`. Gutter actions from the view go through `View::settle`, which drops pinned gutters and
  maps paths below a dropped Tools split. `PanelKind::compact()` is now a local `compact` (Tools, Audio Meters).
- `dock_ui.rs` calls `dock_modes::view`, lays out the view, settles the actions, lowers `DockStyle::min_pane` to
  `STRIP_W` only while the right dock is folded (craft-ui's 40 pt minimum otherwise clamps the strip), draws each
  folded group as a strip from its `body` callback, then draws the palette and the flyout.
- The real `ui.dock`, saved workspaces and preferences are never changed by the switches, as the tests require.

Why not craft-ui's own iconic columns: `Column.iconic` only works in a window-edge `Node::Dock`. The factory
workspaces are plain `Node::Split` trees, so an iconic fold would reshape the layout users save. The strip and
flyout are therefore ours, drawn over the group's rectangle.

Test adaptations (no assertion removed or weakened):
- `c0_tool_modes.rs`: `saved.root` becomes `saved`, `saved.root.contains` becomes `saved.contains(&..)`,
  `ui.floating` becomes `ui.dock.floating`.
- `c0_right_dock.rs`: `Layout::contains` takes a reference; `activate` becomes
  `apply(Action::Activate { .. })`.

## Tests

`cargo test -p filmcraft-ui-egui --no-fail-fast` (second run, after the fixes), `EXIT=0`:

| target | passed | failed | ignored |
|---|---:|---:|---:|
| lib | 246 | 0 | 0 |
| c0_hidden_ui_pass | 2 | 0 | 0 |
| c0_host | 15 | 0 | 0 |
| c0_render | 6 | 0 | 0 |
| c0_right_dock | 7 | 0 | 0 |
| c0_tool_modes | 9 | 0 | 0 |
| it (`tests/it`) | 561 | 0 | 5 |
| play_ahead | 6 | 0 | 0 |
| robustness_ui | 3 | 0 | 0 |

Upstream-red: none, since nothing failed. The pure-upstream worktree run was not needed.

First run, before the fixes: 3 right-dock tests failed. The strip was 40 pt wide, from craft-ui's `min_pane`, and
the flyout lacked its `panel.<Kind>` id. Both are fixed in the second run above.

`cargo test -p filmcraft-web --no-fail-fast`, `EXIT=0`: `it` 8 passed, 0 failed; the other targets have 0 tests.

`cargo check -p filmcraft-web --target wasm32-unknown-unknown`, `EXIT=0`, no errors.

`c0/filmcraft-host/build.sh` (`cargo xtask web --page-control --no-default-features`), `EXIT=0`, output
`wasmBytes: 34808736`, `totalBytes: 35011300`, build `3848ada0c946b15c`.

**Wasm size: FAILS the cap.** 34808736 bytes against 33554432: **1254304 bytes over** (3.7%). The page build itself
succeeds; the size proof does not pass. No earlier page build size is recorded in this tree, so I cannot say how much
of this overrun the upstream merge already had and how much the dock port adds. Measuring that needs a pure-upstream
`176e6a9c585f` page build for comparison, which was not run.

Load (`/proc/loadavg`): before the page build 28.14 35.07 31.25; after 11.68 16.34 14.08.

## PATCH-DEBT (after)

Measured `git diff --numstat --diff-filter=M 176e6a9c585f HEAD -- . ':!c0'`: **49 files, +345 / -85**.
Before the port (303 / 79 in 48 files, dock port pending): **+42 lines over that measure**, almost all in
`crates/ui-egui/src/dock_ui.rs` (42 / 6) and the test-adaptation hunks.
Ratchet 283. The excess over the ratchet is now **+62**. It is not justified yet: the dock hooks in `dock_ui.rs`
are the cost of the port, and they must be re-checked when the next upstream pull moves `dock_ui.rs` again.

## Clashes with our UI

- Home screen and community links: `header.rs` and `links.rs` hunks are re-applied; the header height comes from
  upstream's `header::HEIGHT`.
- Tool modes and right dock: ported as above. The flyout and strip are ours; their geometry is the group rectangle
  upstream lays out, with the tab header covered.
- Start screen (`import_mode`): our gate is kept; upstream's `Import` mode falls through to the dock.
- Theme: `theme::host::resolve` is wired in `set_theme`.

## Assumptions

- The folded strip covers the tab header by painting over it from the group's `body` callback; upstream draws the
  squeezed header first. The tests check geometry and pass; a visual check of the folded header is not done.
- The 1x1 PNG placeholders satisfy `brand.rs` (not verified at runtime).
- `craft_ui` at `92d392c` is the one the fork builds against (cargo re-resolved the lock, not hand-edited).
