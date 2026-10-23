# C0vid FilmCraft consistency lane report

FilmCraft's guest page now has PhotoCraft's tool-mode keys, a foldable right dock and a frameless
`ui.render`. Every suite below passed in the **release** profile, wasm checks, clippy, the page build
and the headless smoke included. **The patch-debt target below 70 is not met: Ratchet 278** (259 before
this lane, +19).

Branch `c0/main`, base `d643a886`. No push, no upstream submission.

## Tool-mode decision

FilmCraft's "tool strip" is the **Tools panel** of the editing workspaces (Selection, Track Select,
Ripple, Razor, Slip, Pen, Hand, Type and their grouped flyouts), drawn by
`crates/ui-egui/src/panels/tools.rs` in every mode. The four modes are `toolbar_double` x
`toolbar_floating`:

- docked, one column: upstream's 64 pt column (the default; bit-for-bit upstream);
- docked, two columns: an 84 pt column;
- floating, one column / two columns: FilmCraft has no floating palette of its own, so the panel is a
  movable palette (`c0.tools.palette`, grip `c0.tools.palette.move`) inside the canvas area, positioned
  by `toolbar_float {pos}`.

The modes are a *view* of the dock tree: the layout the user edits and saves is never changed, nothing is
written to preferences or a workspace, and at default values the dock is laid out as upstream does.
`right_dock_collapsed` folds the rightmost column without a monitor or Timeline (Properties, Essential
Graphics, Text; Lumetri, Effects; ...) to a 36 pt icon strip. An icon opens its panel in a flyout over the
canvas; an unfold button reverses it. The Program monitor keeps its top and bottom, and the rows below the
dock do not move. Assembly has no right dock (`panes.right_dock` is false there).

## Commits

1. `5960b01` — `feat(c0vid): tool modes, foldable right dock and frameless ui.render` (code and tests).
2. `docs(c0vid): record the consistency lane` (README, PATCH-DEBT.md, smoke.cjs, this report). Its hash is
   in the hand-back; a commit cannot contain its own hash.

## Tests (release profile, `CARGO_BUILD_JOBS=2`, `RUST_TEST_THREADS=2`, under `smoac-slot run heavy`)

| Suite | Result |
|---|---|
| `tests/c0_tool_modes.rs` (set, inspect, defaults = upstream, unknown/invalid, four modes, hide/show, palette move, not saved, every factory workspace) | 9 passed |
| `tests/c0_right_dock.rs` (default, set/inspect/unknown, fold geometry, flyout, unfold, layout never saved, every workspace, Assembly) | 7 passed |
| `tests/c0_render.rs` (PNG of the Program frame, no UI pass, no window raise, scale/sequence/cache, hostile params, 64 Mpx refusal) | 6 passed |
| `c0_host::dock_modes` unit tests | 3 passed |
| `tests/c0_host.rs` (existing c0 host contract) | 9 passed |
| Full `cargo test --release -p filmcraft-ui-egui` | 700 passed, 0 failed, 4 ignored (the earlier lane's 675 + the 25 new tests) |

The full-suite run came before two last edits to this lane's own new files: `rustfmt`, and one
`% == 0` -> `is_multiple_of` change that clippy asked for in `dock_modes.rs::advance`. After them
the four c0 test files, the unit tests and clippy ran again and passed (34 tests). The full suite was not run again.
`community_disabled_removes_every_link_and_keeps_about_and_commands` passes in the release profile; it
fails in the debug profile on egui's "Dropped TexturesDelta" assertion, as the first run noted. It
is upstream code, and I did not check it on clean `main`.

## Other gates

| Check | Result |
|---|---|
| `cargo fmt --all --check` | passed (after formatting this lane's six new files) |
| `cargo clippy --release -p filmcraft-ui-egui -p filmcraft-web --all-targets --features filmcraft-web/page-control -- -D warnings` | passed |
| `cargo clippy --release -p filmcraft-ui-egui --all-targets -- -D warnings` | passed |
| wasm `cargo check`, page-control with default features | passed |
| wasm `cargo check`, page-control `--no-default-features` | passed |
| `cargo xtask wasm`, `cargo xtask layers`, `cargo xtask assets` | passed |
| Page build (`c0/filmcraft-host/build.sh`) | passed |
| Headless smoke (`smoke.cjs`, Chromium 152, swiftshader) | passed; 0 page errors; ready in 683 ms |

The smoke sets each of the four tool modes with animation frames blocked and reads them back
(widths 64/84, palette present or absent), hides and shows the Tools panel, moves the palette, sends bad
values, folds and unfolds the right dock (Program keeps its top and bottom; Properties and Program return to
their exact rects), and calls `ui.render {scale:1}` (valid 64 x 64 PNG, `presented:false`) plus two refused
calls. The smoke project is empty, so its render is a black frame; the native `c0_render` tests cover frame
content at a playhead. Not run: the whole-workspace `cargo test`/`clippy`; no crate outside
`filmcraft-ui-egui` and the web app changed.

## Release size and load

| Build | wasm bytes | Finished-site bytes | Content hash |
|---|---:|---:|---|
| Shipping page, previous lane | 31,818,179 | 32,016,080 | 88d9b3c320a7b60c |
| Shipping page, this lane | 31,846,360 | 32,044,261 | bdd4cdadf318beff |

+28,181 bytes (+0.09 %). The **pin cap value is not stated anywhere I can read**, so I cannot say how much
headroom is left (see assumptions). `build.sh` uses `--no-default-features` (patent codecs off), upstream's
ThinLTO release profile, `BINARYEN_CORES=2`; `CRAFT_FONTS_DIR` unset.

Exact `cat /proc/loadavg`:

| Moment | Output |
|---|---|
| Before the first compile of this resume (other lanes running) | `20.56 21.47 17.49 8/1635 3986709` |
| Before the page build | `14.63 15.88 16.15 22/2036 389952` |
| After the page build | `15.34 16.44 16.33 6/1866 937555` |

The gate waited behind landing mode for a long stretch before the first admission; the later runs were
admitted at once.

## Patch debt

`git diff --numstat --diff-filter=M d643a88 -- . ':!c0'` (no local `main` ref in this clone; `d643a88` is
upstream `main`): **278 added, 69 removed, 40 files; Ratchet 278**, 259 before. The lane adds +19/-6 in four upstream files: `dock_ui.rs` (+13/-2), `tools.rs` (+4/-3), `control.rs` (+1) and
`lib.rs` (+1/-1). Its edits to our own `c0_host.rs` and `c0_host/panels.rs`, and every new file, are not
upstream debt. Older debt is not rewritten. PATCH-DEBT.md has the per-file rows.

## Assumptions I could not confirm

- The wasm pin cap's value: not in the repo or the packets, so the size is stated against the previous
  lane's build only.
- One `404` ("Failed to load resource") appears in the smoke's browser console. I did not identify the
  URL or check whether the earlier lane's smoke printed it too; it did not fail the smoke.
- The full ui-egui suite was not re-run after the last formatting and `is_multiple_of` edit (behaviour is
  identical for `columns.max(1) >= 1`; the c0 suites and clippy did re-run).
- The tree used here is the orchestrator's replay of the first run's file writes. It is byte-identical to
  the backup in `/srv/smoac/backups/sow-consistency-20261022` (tracked diff and all six new files compared);
  I could not check it against the first run's original worktree, which was deleted.
- `community_disabled_removes_every_link_and_keeps_about_and_commands` in the debug profile on clean `main`.
- Real-browser look at the flyout and the floating palette: the smoke checks rects and ids, and I
  did not take or inspect a screenshot of the docked/floating layouts.
