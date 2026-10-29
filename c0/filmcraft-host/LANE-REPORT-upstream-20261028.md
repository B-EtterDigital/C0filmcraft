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

## Debt pass

Goal: the patch debt back to the ratchet (283) without losing a feature. Measured with
`git diff --numstat --diff-filter=M 176e6a9c585f HEAD -- . ':!c0'`, the count the daily check uses.

**Total: 345 added / 85 removed lines in 47 text files before (`55979088`); 255 added / 497 removed in 47 text files after (`HEAD`). Ratchet: 255** (was 283, the check refused the candidate at 345).

Commits on `fix`:

| commit | what |
|---|---|
| `e96302fd` | `refactor(c0): move the dock and Home hooks out of dock_ui.rs and header.rs` (dock_ui 42 → 9, header 15 → 3) |
| `0d9c3d02` | `refactor(c0): move the H.264, HEVC and AAC hooks of codecs and export into c0 modules` (video 15 → 2, encoders 19 → 5, registry and lib.rs import hunks reverted) |
| `d0ad9f01` | `chore(c0): use the base64 release the lock already holds` (Cargo.lock 16 → 6) |

How the hooks moved:

- **Tool modes and right dock**: `c0_host/dock_modes.rs` has `begin` (the layout to draw and a folded dock's pane width), `fold` (a folded group's strip), `settle` (gutter actions) and `after` (palette and flyout). They share one per-frame `Frame` on the host (`c0_host.rs`). `dock_ui.rs` keeps one call per site and `FilmContent` is back to upstream's literal.
- **Home button**: `c0_host/chrome.rs::home` returns where the mode tabs start; `header.rs` has one call.
- **H.264 and HEVC decoders**: `codecs/src/video/c0_video.rs` holds them, with the no-patent fallbacks; `video.rs` has `mod c0_video;` and `pub use c0_video::*;`.
- **AAC and H.264 encoders and factories**: `export/src/encoders/c0_codecs.rs`, with `mod c0_codecs;` and `pub(crate) use c0_codecs::*;` in `encoders.rs`. The no-patent factories come from `c0_codec_policy`, so `registry.rs` and the `lib.rs` test imports are back to upstream's lines.
- **Cargo.lock**: `base64` is 0.23, the release the lock already resolves (`crates/ui-egui/Cargo.toml`). Cargo rewrote the lock; it was not edited by hand.

Left in upstream files, and why (they are the hook sites, not moved logic): `registry.rs` (26) declares the browser-encoder trait hooks, `job.rs` (21) polls readiness and final timing, `import.rs` (16) and `audio.rs` (18) gate code that stays in upstream functions, and `Cargo.lock` (6) holds the craft-media fork source and the edges of our `filmcraft-format`, `filmcraft-export` and `base64` dependencies. No c0 crate exists to move the dependencies into, so they stay.

Tests (final tree, `d0ad9f01`; logs under `/srv/smoac/build/sow-fix-filmcraft-20261028-logs/final/`):

| command | result |
|---|---|
| `cargo test -p filmcraft-ui-egui --no-fail-fast` | 855 passed, 0 failed, 5 ignored: lib 246, `c0_hidden_ui_pass` 2, `c0_host` 15, `c0_render` 6, `c0_right_dock` 7, `c0_tool_modes` 9, `it` 561 (5 ignored), `play_ahead` 6, `robustness_ui` 3. Same counts as the lane's previous run. |
| `cargo test -p filmcraft-web --no-fail-fast` | 8 passed, 0 failed (`it`); `c0_page_control` is gated on `page-control` and runs 0 here |
| `cargo test -p filmcraft-web --features page-control --no-fail-fast` | 11 passed, 0 failed: `c0_page_control` 3, `it` 8 |
| `cargo check -p filmcraft-web --target wasm32-unknown-unknown` | EXIT 0 |
| `cargo check -p filmcraft-web --no-default-features --target wasm32-unknown-unknown` | EXIT 0 |
| `cargo check -p filmcraft-web --no-default-features --features page-control --target wasm32-unknown-unknown` | EXIT 0 (the page bridge build) |
| `cargo test -p filmcraft-export --no-fail-fast` | 141 passed, 0 failed (this exercises the moved encoder code) |
| `cargo test -p filmcraft-codecs --no-fail-fast` | lib 133 passed; `it` 56 passed, 7 failed, 3 ignored; `pool_bitexact` 1 passed. The 7 failures are all the ffmpeg oracle helper (`crates/codecs/tests/it/common/mod.rs:37`): `/root/.local/bin/ffmpeg` (7.0.2 static) exits 139 (segfault) on the fixture before any decoder runs. Not re-run at `55979088` to prove they pre-date this pass. |

Codec-free build: `cargo check -p filmcraft-codecs --no-default-features`, `-p filmcraft-export --no-default-features` and the web `--no-default-features` wasm check pass with no warnings. Their **test** targets do not compile without patent codecs (`filmcraft_aac` is referenced by upstream test modules in codecs and export, and `Upsample2x` by codecs' audio tests). `filmcraft_aac` is optional at `176e6a9c585f` too, so this is not new; it was not fixed here because the fix means editing upstream test modules.

Load (`/proc/loadavg`, 1-minute, 5-minute, 15-minute): before the proof run `7.57 9.64 11.09`; after it `36.01 23.70 16.23`.

Not re-run in this pass: the page build (`c0/filmcraft-host/build.sh`, wasm size). The size gate was 1254304 bytes over the cap at `7174ea35` (per the lane's earlier section); `55979088` changed the build profile for it, and this pass moved code without changing what is compiled, but the size was not re-measured.

Behaviour kept: the tool-strip modes and right-dock fold (`c0_tool_modes`, `c0_right_dock`, `c0_host`, `c0_render`), the codec-free build, and the page control bridge (`c0_page_control`, `c0_hidden_ui_pass`) all pass. No test was changed or weakened.

Per-file numstat (added / removed, text files):

| file | before (`55979088`) | after (`HEAD`) |
|---|---:|---|
| `crates/ui-egui/src/dock_ui.rs` | +42 / -6 | +9 / -3 |
| `crates/export/src/encoders.rs` | +19 / -3 | +5 / -198 |
| `crates/codecs/src/video.rs` | +15 / -0 | +2 / -225 |
| `crates/ui-egui/src/header.rs` | +15 / -13 | +3 / -14 |
| `Cargo.lock` | +16 / -7 | +6 / -3 |
| `crates/export/src/registry.rs` | +31 / -2 | +26 / -1 |
| `crates/export/src/lib.rs` | +4 / -1 | +1 / -0 |
| `Cargo.toml` | +7 / -4 | unchanged |
| `apps/filmcraft-cli/Cargo.toml` | +3 / -3 | unchanged |
| `apps/filmcraft-web/Cargo.toml` | +9 / -0 | unchanged |
| `apps/filmcraft-web/src/import.rs` | +16 / -0 | unchanged |
| `apps/filmcraft-web/src/lib.rs` | +8 / -0 | unchanged |
| `apps/filmcraft-web/src/webcodecs.rs` | +2 / -0 | unchanged |
| `apps/filmcraft/Cargo.toml` | +3 / -3 | unchanged |
| `crates/automation/Cargo.toml` | +1 / -1 | unchanged |
| `crates/codecs/Cargo.toml` | +8 / -4 | unchanged |
| `crates/codecs/src/audio.rs` | +18 / -3 | unchanged |
| `crates/codecs/src/hw.rs` | +9 / -2 | unchanged |
| `crates/codecs/src/lib.rs` | +3 / -0 | unchanged |
| `crates/codecs/src/mp4.rs` | +4 / -0 | unchanged |
| `crates/codecs/src/mxf.rs` | +3 / -0 | unchanged |
| `crates/codecs/src/stream_color.rs` | +7 / -1 | unchanged |
| `crates/engine/Cargo.toml` | +2 / -1 | unchanged |
| `crates/export/Cargo.toml` | +7 / -3 | unchanged |
| `crates/export/src/aac_out.rs` | +3 / -0 | unchanged |
| `crates/export/src/audio_out.rs` | +10 / -0 | unchanged |
| `crates/export/src/format.rs` | +5 / -4 | unchanged |
| `crates/export/src/job.rs` | +21 / -7 | unchanged |
| `crates/export/src/presets.rs` | +2 / -2 | unchanged |
| `crates/export/src/run.rs` | +3 / -0 | unchanged |
| `crates/gpu/Cargo.toml` | +1 / -1 | unchanged |
| `crates/platform/Cargo.toml` | +2 / -2 | unchanged |
| `crates/ui-egui/Cargo.toml` | +7 / -1 | unchanged |
| `crates/ui-egui/src/app.rs` | +1 / -0 | unchanged |
| `crates/ui-egui/src/app/control.rs` | +1 / -1 | unchanged |
| `crates/ui-egui/src/app/frame.rs` | +6 / -4 | unchanged |
| `crates/ui-egui/src/app/prefs.rs` | +1 / -1 | unchanged |
| `crates/ui-egui/src/control.rs` | +2 / -0 | unchanged |
| `crates/ui-egui/src/credits.rs` | +2 / -2 | unchanged |
| `crates/ui-egui/src/lib.rs` | +2 / -0 | unchanged |
| `crates/ui-egui/src/links.rs` | +3 / -0 | unchanged |
| `crates/ui-egui/src/menus.rs` | +2 / -0 | unchanged |
| `crates/ui-egui/src/panels/dialogs.rs` | +5 / -0 | unchanged |
| `crates/ui-egui/src/panels/import_mode.rs` | +3 / -0 | unchanged |
| `crates/ui-egui/src/panels/tools.rs` | +4 / -3 | unchanged |
| `crates/ui-egui/src/theme.rs` | +1 / -0 | unchanged |
| `xtask/src/main.rs` | +6 / -0 | unchanged |
