# Upstream pull 20261028: filmcraft `176e6a9c585f` into `c0/main`

Status: **BLOCKED** on the UI port. The merge, the export crate and the trademark/CI cleanup are done and committed.
The tool-strip and right-dock views (`crates/ui-egui/src/c0_host/dock_modes.rs`) and `dock_ui.rs` are not ported.
Nothing below claims a test pass that was not run.

## Commits on `fix`

| commit | what |
|---|---|
| `2afca3aa` | `merge: upstream 176e6a9c585f into c0/main` (parents: `610b19c7` fix base, `176e6a9c` upstream). Checkpoint: export compiles; UI hooks applied but not compile-verified. |
| `4ea59eff` | `chore(c0): keep upstream trademarks and CI out of c0/main`. Upstream `.github/workflows` and `docs/brand` removed; the two PNGs `brand.rs` embeds are 1x1 transparent placeholders. |

The result descends from `origin/c0/main` (`610b19c7`) and from upstream `176e6a9c`.

## Conflicted files and resolution

| file | conflict | resolution |
|---|---|---|
| `.github/workflows/packaging-lint.yml`, `windows-arm64.yml` | modify/delete | Kept deleted (fork has no CI); removed in the chore commit. Not resurrected. |
| `Cargo.lock` | content | Took upstream's lock; cargo re-resolved it. `craft-media` now points at `C0craft-libs` via the root `[patch]`. |
| `crates/gpu/Cargo.toml` | content | `pollster = "1"` (upstream) kept with our `filmcraft-export` `default-features = true`. |
| `crates/ui-egui/Cargo.toml` | content | `pollster = "1"` (upstream) kept with our `filmcraft-codecs` `default-features = true`. |
| `crates/export/src/lib.rs` | restructured | Upstream split this file into `convert`, `encoders`, `format`, `registry`, `run`, `output`, `stats`. Took upstream's file; re-applied our feature-boundary lines in the new modules (`c0_codec_policy` module declaration, test-only factory imports split by feature). |
| `crates/export/src/encoders.rs`, `format.rs`, `registry.rs`, `run.rs` | new layout | Re-applied as patch debt (see `PATCH-DEBT.md`). Pending-audio guard moved from `lib.rs` to `run.rs`. |
| `crates/ui-egui/src/lib.rs` | restructured | Upstream moved `FilmcraftApp` to `lib.rs`/`app.rs`, and its update to `app/frame.rs`. Re-applied: `pub mod c0_host`, `c0` state field, `set_theme` palette hook, `ui.render` raise exemption. |
| `crates/ui-egui/src/app/frame.rs`, `app/prefs.rs`, `app/control.rs`, `app.rs` | new layout | Re-applied the header/status/start-screen gates, the file-drop hook, the palette hook and the `ui.render` exemption. Uses upstream's `header::HEIGHT`. |
| `crates/ui-egui/src/dock_ui.rs` | restructured | **Not ported.** Upstream replaced `dock::layout_with_actions` and `DockNode`/`Group` with `craft_ui::DockArea::show_content` and `craft_ui::docking::{Node, Layout}`. Our tool-mode/right-dock view is not re-applied. |

Mistake during the merge, recovered: `git add -N .` on a conflicted index dropped the conflict stages; `checkout --theirs` then wrote empty files. Recovered by `git reset --hard HEAD` and a fresh merge. `fix` was never altered by it.

## Compile status

- `filmcraft-export`: `cargo check` clean with default features (`patent-codecs`), and clean with `--no-default-features` after gating the H.264-only imports, helpers and the `Yuv420` alias.
- `filmcraft-ui-egui`: fails with 4 resolution errors, all in `c0_host/dock_modes.rs`:
  - `crate::dock::DockNode`, `crate::dock::Group` are gone upstream.
  - `SplitSize` is now `craft_ui::layout::SplitSize` (`FixedFirst`/`FixedSecond`, not `FixedA`/`FixedB`).
  - `PanelKind::compact()` no longer exists (our definition was Tools and AudioMeters; it can move into `dock_modes.rs`).
  - `FilmcraftApp::panel_body` no longer exists (it can become a helper in `dock_modes.rs`, doing what the old method did: `panels::show` into a child of the content rect).
  Rustc stops at name resolution, so type errors in the rest of `c0_host` are not yet known.

## Why the dock port is blocked

Our `dock_modes.rs` is a view layer over the old dock tree: it clones and edits `DockNode` splits, derives strip rectangles from laid-out `Group`s (`path`, `rect`, `panels`, `active`, `content`), and settles resize actions against the real tree.
Upstream's `DockArea` lays out the tree internally and returns only actions, focus and hit rectangles. Rebuilding group rectangles from `Hit` rectangles (the locked craft-ui `92d392c` exposes `hit_rects`, `Group<P>` without rectangles) is a new geometry layer, not a rename.
The tests `tests/c0_tool_modes.rs`, `c0_right_dock.rs`, `c0_host.rs` and `c0_render.rs` depend on that layer.
Each compile cycle is queuing at the shared gate for 20 to 30 minutes, so the port cannot be finished and verified in this session.

## PATCH-DEBT

- Before (`c0/main` vs `7d8dd5ae`): 41 files, +283 / -70. Ratchet 283.
- After (fix HEAD vs `176e6a9c`): 48 files, +303 / -79. The dock port is pending, so `dock_ui.rs` is not in this count.
- The table in `PATCH-DEBT.md` now lists the eight files that were missing (encoders, format, registry, run, app.rs, app/frame.rs, app/prefs.rs, app/control.rs).
- +20 lines over the ratchet are forced by upstream's moved hooks (the trait surface and factory gating for the codec-free build) and the new UI hook sites. This is not yet fully justified; it must be re-checked after the dock port.
- No patch has been dropped. Upstream's pending-audio fix made our `export_here` guard equivalent; the guard is kept (it still refuses a synchronous export while web media loads), so nothing is removed.

## Tests (not run; blocked)

- `cargo test -p filmcraft-ui-egui`, `-p filmcraft-web`, and c0 test crates: **not run**. Blocked: ui-egui does not compile.
- `filmcraft-export` tests: not run (not in the required list; can run once the gate is free).
- Upstream-red list: **not determined** (needs the pure-upstream worktree run after the port).
- wasm (`cargo check --target wasm32-unknown-unknown` for the web app), release wasm size, clippy: **not run**.

## Upstream changes (plain words, 129 non-merge commits since `7d8dd5ae`)

- Export: the export crate was split into modules (`convert`, `encoders`, `format`, `registry`, `run`, `output`, `stats`, `still_encode`). Pending-audio handling is now in `job.rs`; a `prepare_ahead` hook and `media_start` were added to the encoder trait; the H.264 encoder was sped up without changing its output.
- Dock/UI: the docking moved into `craft_ui` (`DockArea`, `Layout`, `Node` with a `Stack` variant, `Hit`). Saved workspaces load fixed split sizes as ratios; workspace layouts are saved alongside the new format. The UI crate was split into `app/` modules.
- Installer: per-user ArtCraft folder by default; a `test-install-policy.ps1`; scope policy.
- Fixes: the `ui.render`/hidden-window handling (see our control-channel hunk); pending-audio export fixes (`aac_out`, `audio_out`, `job`).
- Not enumerated: the full command and menu list. No new upstream commands or menus were verified against the Core Agent verbs (not done).

## Clashes with our UI (not yet resolved)

- Home screen and community links: `header.rs` and `links.rs` hunks are re-applied; the header height now comes from upstream's `header::HEIGHT` rather than our old 38.0 literal.
- Tool modes and right dock: the whole `dock_modes` view must be ported (see above). Until then, our default layout is upstream's and the Tool-mode switches have no effect.
- Start screen (`import_mode`): our gate is kept; upstream's `Import` mode now falls through to the dock when the start screen is off.
- Theme: `theme::host::resolve` is wired in `set_theme`. The palette override is in `theme/host.rs`; no conflict found in upstream's theme.rs.
- Agent server and file drop: `c0_host::files::show` is called from the frame; the agent-server control paths are not re-verified.

## Assumptions I could not confirm

- That `craft_ui` at the locked revision `92d392c` is the one the fork builds against (it is; the lock file was re-resolved by cargo, not edited by hand).
- That the codec-free build is the page's build configuration (the page build was not run).
- That the 1x1 PNG placeholders satisfy `brand.rs` (they are decoded only if the brand logo is shown; not verified at runtime).
- That `c0_host/dock_modes.rs` tests can be ported to `craft_ui`'s `Layout`/`Node` without weakening the assertions (not attempted).

LANE-BLOCKED filmcraft ui-egui dock port: c0_host/dock_modes.rs and dock_ui.rs depend on the removed DockNode/Group layout API; the group-geometry replacement is not built, so ui-egui does not compile and no UI tests, wasm build or clippy ran.
