# FilmCraft guest patch debt

Measured against `main`, counting only modified upstream files. New guest modules, page files
and c0 tests are additions rather than upstream patch debt.

| File | Lines | What | Why | Offered upstream |
|---|---:|---|---|---|
| `Cargo.lock` | +6 / -3 | Lock host serialization/browser export dependency wiring; craft-media from our C0craft-libs fork | Keep builds reproducible | no |
| `Cargo.toml` | +7 / -4 | Disable inherited codec defaults at dependency seams; `[patch]` craft-media to B-EtterDigital/C0craft-libs (the presentation-window hook moved there with the muxer, 2026-10-25) | Allow the page to select a codec-free graph | no |
| `apps/filmcraft-cli/Cargo.toml` | +3 / -3 | Explicit native dependency defaults | Keep CLI behaviour unchanged | no |
| `apps/filmcraft-web/Cargo.toml` | +9 / -0 | Page feature, codec forwarding, native transport test deps, export interface | Reuse the existing web binary and pass K-41 | no |
| `apps/filmcraft-web/src/import.rs` | +16 / -0 | Guard project replacement and invalidate late picker results | Protect unsaved work and implement cancellation in the guest | no |
| `apps/filmcraft-web/src/lib.rs` | +8 / -0 | Declare page module; configure chrome; attach bridge; `page_control::lend_ui_pass` in `WebApp::raw_input_hook` | Opt-in runner hook and first-paint guest configuration; since egui 0.37 the app answers `ui.*` only in a UI pass and a hidden page runs logic-only passes, so the bridge lends a page-control request one unpainted UI pass | no |
| `apps/filmcraft-web/src/webcodecs.rs` | +2 / -0 | Wrap the existing MP4 source for browser AAC | Keep video decoding in the existing browser opener | no |
| `apps/filmcraft/Cargo.toml` | +3 / -3 | Explicit native dependency defaults | Keep desktop behaviour unchanged | no |
| `crates/automation/Cargo.toml` | +1 / -1 | Explicit engine defaults | Keep automation behaviour unchanged | no |
| `crates/codecs/Cargo.toml` | +8 / -4 | Default-on patent-codecs feature and optional codec dependencies | Actually exclude software H.264, HEVC and AAC including Symphonia AAC | no |
| `crates/codecs/src/audio.rs` | +18 / -3 | Gate AAC implementations and preserve metadata helpers | Codec-free browser errors without losing other audio codecs | no |
| `crates/codecs/src/hw.rs` | +9 / -2 | Gate software SPS-based NAL descriptions | Remove transitive H.264/HEVC references | no |
| `crates/codecs/src/lib.rs` | +3 / -0 | Declare policy/fallback modules | Keep registry API stable in both configurations | no |
| `crates/codecs/src/mp4.rs` | +4 / -0 | Gate SPS helpers and select container metadata fallbacks | MP4 remains available without software decoders | no |
| `crates/codecs/src/mxf.rs` | +3 / -0 | Gate AVC POC parsing and select unsupported fallback | Avoid linking software H.264 merely for MXF metadata | no |
| `crates/codecs/src/stream_color.rs` | +7 / -1 | Gate AVC/HEVC bitstream colour inference | Use container colour metadata without software SPS parsers | no |
| `crates/codecs/src/video.rs` | +2 / -225 | Declare and re-export `video/c0_video.rs`, which holds the H.264 and HEVC decoders and their codec-free fallbacks | Gated decoders and fallbacks move into a c0 child module; upstream keeps the decoder trait and MJPEG | no |
| `crates/engine/Cargo.toml` | +2 / -1 | Forward default-on codec feature | Allow the page to disable codecs without changing normal engine defaults | no |
| `crates/export/Cargo.toml` | +7 / -3 | Default-on codec feature and optional encoder dependencies | Exclude software H.264/AAC from the page | no |
| `crates/export/src/aac_out.rs` | +3 / -0 | Reject pending synchronous audio before finishing output | Avoid silently truncated audio files | yes, as is |
| `crates/export/src/audio_out.rs` | +10 / -0 | Preserve cursors on pending audio; reject incomplete loudness passes | Avoid lost samples and non-progressing measurement on wasm | yes, as is |
| `crates/export/src/job.rs` | +21 / -7 | Poll browser readiness and final metadata/timing; correct pending-audio docs | Drain asynchronous output without losing audio or changing default packet ordering | mixed: pending-audio fix yes, as is; host encoder hooks no |
| `crates/export/src/encoders.rs` | +6 / -199 | Declare and re-export `encoders/c0_codecs.rs` (AAC and H.264 encoders, factories, `h264_slices`); four import lines without the patent-only names (the fourth since 20261030, when upstream replaced `use super::*` with an explicit list naming `BitrateMode`, `H264Profile` and `std::sync` types only the moved encoders use) | Build without software AAC/H.264; codec-free factories come from `c0_codec_policy` | no |
| `crates/export/src/format.rs` | +5 / -4 | Codec-free default format (Motion JPEG); `has_builtin_encoder` gates H.264/AAC; `H264Pass` takes the `c0_codec_policy` stat type | Quick Export defaults to a format that exists | no |
| `crates/export/src/registry.rs` | +26 / -1 | Host hooks on the traits (`ready`, `finish_ready`, `final_sample_entry`, `final_timing`, `asynchronous`); `PassStats` from `c0_codec_policy` | Browser encoders need readiness; upstream has no such hook | no |
| `crates/export/src/run.rs` | +3 / -0 | Synchronous pending-audio guard in `export_here` (moved here upstream) | Refuse a truncated synchronous export while media loads | yes, as is |
| `crates/ui-egui/src/app.rs` | +1 / -0 | Host state initialised in `FilmcraftApp::new` | Host chrome state | no |
| `crates/ui-egui/src/app/control.rs` | +1 / -1 | `ui.render` does not raise the window | Frameless render must not present a window | no |
| `crates/ui-egui/src/app/frame.rs` | +6 / -4 | Host gates on header/title bar, status bar and start screen; c0 file-drop hook | Hide host-owned chrome; tool modes | no |
| `crates/ui-egui/src/app/prefs.rs` | +1 / -1 | `set_theme` passes the host palette through `theme::host::resolve` | Runtime host palette | no |
| `crates/export/src/lib.rs` | +1 / -0 | Declare the codec policy module | Codec-free fallbacks and PassStats | mixed: pending-audio guard yes, as is; host feature/hooks no |
| `crates/export/src/presets.rs` | +2 / -2 | Choose a codec-free default preset when feature is off | Quick Export defaults to a format that exists | no |
| `crates/gpu/Cargo.toml` | +1 / -1 | Explicit test dependency defaults | Keep isolated GPU tests unchanged | no |
| `crates/platform/Cargo.toml` | +2 / -2 | Explicit native codec defaults | Keep OS codec integrations unchanged | no |
| `crates/ui-egui/Cargo.toml` | +7 / -1 | Forward codec feature; project file format (`filmcraft-format`) and base64 0.23 (the locked release) for the host byte contract | Use the app's own file format and host byte contract | no |
| `crates/ui-egui/src/control.rs` | +2 / -0 | One c0.* dispatch arm; one `ui.render` arm | Reuse the existing control queue | no |
| `crates/ui-egui/src/credits.rs` | +2 / -2 | Host-aware contributor labels | Keep author credit while removing GitHub links | no |
| `crates/ui-egui/src/dock_ui.rs` | +9 / -3 | `dock_modes::begin` lays out the frame (the tool-mode and right-dock views, and a folded dock's pane width), `fold` draws a folded group as its strip, `settle` maps the gutter actions and `after` draws the palette and flyout | The tool modes and the foldable right dock are views of the dock; at default values the layout is the real tree as before | no |
| `crates/ui-egui/src/header.rs` | +3 / -14 | Home is `c0_host::chrome::home`; read Home, community and menu switches | Hide host-owned and promotional chrome | no |
| `crates/ui-egui/src/lib.rs` | +2 / -0 | Declare the host module and its state field; the runtime palette lives in `theme::host` | Host state; the frameless render must not present a window | no |
| `crates/ui-egui/src/links.rs` | +3 / -0 | Suppress host-hidden community URLs | Cover direct opener calls as well as visible widgets | no |
| `crates/ui-egui/src/menus.rs` | +2 / -0 | Host command guard and menu filtering | Remove all promotional link entries and refuse direct invocation | no |
| `crates/ui-egui/src/panels/dialogs.rs` | +5 / -0 | Community state for credits and hosted About branch | Keep authors/licence, remove promotions and trademark mark | no |
| `crates/ui-egui/src/panels/import_mode.rs` | +3 / -0 | Community draw guard | Hide Home promotions and ArtCraft mark | no |
| `crates/ui-egui/src/panels/tools.rs` | +4 / -3 | Button column and row advance come from `c0_host::dock_modes` (`columns`, `column_x`, `advance`) | Two tool columns; one column is bit-for-bit upstream's centred column | yes, as a general column count |
| `crates/ui-egui/src/theme.rs` | +1 / -0 | Declare host palette module | Expose the context-local colour override | no |
| `xtask/src/main.rs` | +6 / -0 | Forward opt-in page-control/no-default-features arguments | Build via upstream's own cargo/wasm-bindgen pipeline | no |

Total: 256 added lines in 47 upstream text files (measured 2026-10-30: 255 after the 2026-10-29 debt pass, plus 1 forced by upstream's explicit imports in `encoders.rs`).
Ratchet: 256
Re-measured 2026-10-26 after the upstream merge (egui 0.37 port, update prompt): 281, unchanged file for file. Our header Home gate, status-bar gate, tool columns and the craft-media [patch] were kept; upstream dropped its own no-status-bar job poll (it polls every frame now), so that line left with it, not with us.
Re-measured 2026-10-26 after the hidden-page fix: 283 added lines in 41 modified upstream files (`git diff --numstat --diff-filter=M origin/main -- . ':!c0'`). +2 in `apps/filmcraft-web/src/lib.rs`: the `raw_input_hook` seam that lets the page bridge run a `ui.*` request's UI pass while the guest view is hidden under the host's start cover (`ui.set {theme}` timed out with "no UI pass ran before the control request deadline"). Everything else is in `page_control*` and a c0 test.

Measurement: `git diff --numstat --diff-filter=M main c0/main -- . ':!c0'`.
The 70-line target is exceeded (it already was, at 259, before the tool-mode lane; that lane added 19 lines in `dock_ui.rs`, `tools.rs`, `control.rs` and `lib.rs`, all else being in new files: `c0_host/dock_modes.rs`, `c0_host/render.rs`, `c0_host/panels.rs`). The codec feature must propagate through the existing dependency
graph; browser encoding also needs cooperative completion and final mux timing. The pending-audio
fixes are generic wasm fixes and are marked for offering upstream as is. No upstream code is
removed from the default feature configuration.

Re-measured 2026-10-28 on upstream `176e6a9c585f` (`git diff --numstat --diff-filter=M 176e6a9c585f HEAD -- . ':!c0'`):
303 added / 79 removed lines in 48 files. The UI port is not finished: `crates/ui-egui/src/dock_ui.rs` (13 lines in the ratchet)
is not re-applied, and `c0_host/dock_modes.rs` does not compile against the new dock API. The +20 added lines over the ratchet of 283 are forced by upstream's
moved hooks (the export trait surface and factory gating) and the new UI hook sites; they must be re-justified once the dock port lands.

Re-measured 2026-10-29 on upstream `176e6a9c585f` after the tool-strip and right-dock port (`git diff --numstat --diff-filter=M 176e6a9c585f HEAD -- . ':!c0'`):
49 files, +345 / -85. Ratchet 283 is exceeded by +62. The growth is in `crates/ui-egui/src/dock_ui.rs` (42 / 6 lines: the view, the settled gutter actions, the folded-group body, the palette and flyout hooks, and the `min_pane` lift while folded) and in the test-adaptation hunks. The port is not yet re-justified against the ratchet; it must be, at the next upstream pull that touches `dock_ui.rs`.

Re-measured 2026-10-29, debt pass on upstream `176e6a9c585f` (`git diff --numstat --diff-filter=M 176e6a9c585f HEAD -- . ':!c0'`):
255 added / 497 removed lines in 47 text files (the two brand PNGs are binary and not counted). Ratchet 283 is met with 28 lines to spare; the
ratchet is now 255. The growth that was not justified is paid back by moving code out of upstream files, with one hook line per site:

- `crates/ui-egui/src/dock_ui.rs` 42 → 9 added: the view, fold, settle and flyout logic is in `c0_host/dock_modes.rs` behind a per-frame `Frame` on the host.
- `crates/ui-egui/src/header.rs` 15 → 3 added: the Home button is `c0_host/chrome.rs`; the community and menu gates are one-line changes.
- `crates/codecs/src/video.rs` 15 → 2 added: the H.264 and HEVC decoders are in `video/c0_video.rs`.
- `crates/export/src/encoders.rs` 19 → 5 added: the AAC and H.264 encoders and factories are in `encoders/c0_codecs.rs`.
- `crates/ui-egui/Cargo.toml` and `Cargo.lock`: `base64` is 0.23, the release upstream's lock already resolves, which removed a second package (Cargo.lock 16 → 6 added).

Left, and why: `registry.rs` (26) is the export trait surface the browser encoders need, and upstream's traits must declare it; `job.rs` (21) is the
readiness and final-timing hook sites; `import.rs` (16) and `audio.rs` (18) are gates on code that stays in upstream's functions; `Cargo.lock` (6) is the
craft-media fork's source and the edges of our `filmcraft-format`, `filmcraft-export` and `base64` dependencies. No c0 crate exists to move
`filmcraft-format` or `base64` into, so they stay.
