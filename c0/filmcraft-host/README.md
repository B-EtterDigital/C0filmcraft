# C0vid: FilmCraft as a C0VIBE guest page

The page runs the existing `apps/filmcraft-web` binary with `page-control`. FilmCraft owns the
project model, timeline, commands, renderers, importers and UI. The wrapper starts with an empty
project and forces `?empty&norecover`; it neither loads demo footage nor restores a document.
Its body and canvas are transparent until FilmCraft paints. The host supplies the surrounding
title/navigation UI. FilmCraft's status bar remains visible by default in this page.

## Build

FilmCraft uses `cargo xtask web` and wasm-bindgen **0.2.129**, rather than Trunk. Run from the
repository root:

```sh
c0/filmcraft-host/build.sh
```

The script runs upstream's cargo → wasm-bindgen pipeline under
`smoac-slot run heavy 2 3600`, with `CARGO_BUILD_JOBS=2`. It tries the gate pool only after an
admission refusal. The optional upstream wasm-opt step is retained with `BINARYEN_CORES=2`.
The new xtask arguments
`--page-control` and `--no-default-features` forward those cargo switches; upstream builds
without those arguments retain their defaults. `assemble.py` copies the generated files and
browser-codec JS snippets into `dist/c0host/`, supplies this page's `index.html`, and hashes the
JS/wasm filenames. The audio worklet retains its upstream `audio-worklet.js` URL.
All URLs are relative (`./`); serve the finished directory over HTTP(S).

`CRAFT_FONTS_DIR` is optional. If it points to a missing directory, the script unsets it and
reports that optional craft-fonts glyphs will not be embedded. Bundled fonts remain available.

For the upstream software-codec comparison build:

```sh
c0/filmcraft-host/build.sh --with-patent-codecs
```

This replaces the finished site. The default page build disables the default-on
`patent-codecs` feature. Do not ship the comparison build as the C0vid guest.

## Page function and envelopes

The host calls `window.__c0hostRequest(jsonText)`, which forwards to
`window.__filmcraftControl(jsonText)`. Wait for `window.filmcraftLoad.readyMs` before calling;
`filmcraftLoad.error` records startup failures. The underlying bridge is installed by the
`page-control` feature and wraps the existing `window.filmcraft.request` API and control queue.

Input is JSON **text**, and the Promise resolves to JSON **text**:

```js
const reply = JSON.parse(await window.__c0hostRequest(JSON.stringify({
  method: 'c0.inspect', params: {}
})));
```

Requests have `{method, params?}`. Missing or null params mean `{}`; other non-object params
are rejected. Optional ids are ignored; each Promise pairs its own reply. All methods return
`{"ok":true,"result":...}` or `{"ok":false,"error":"..."}`. Malformed requests and unknown
methods use the same error envelope.

Ordinary requests are limited to **1 MiB**, host requests (`c0.*`) to **100 MiB**, decoded project
files to **72 MiB**, and file basenames to **255 UTF-8 bytes**. Paths, NUL, invalid base64 and
non-`.fcproj` names are rejected. `ui.click.count` is capped at 256 and `ui.drag.steps` at 32.
These limits are transport/allocation limits, in addition to FilmCraft's project validation.

## When a request is answered

Each call uses the existing browser API, then dispatches an event to the **same WebRunner**.
Its listener runs an egui logic pass inside the call without presenting a frame. The pass drains
FilmCraft's queue and resolves its existing API promises. Synthetic UI input receives up to
40 additional unpresented passes. `c0.*` methods answer immediately; long-running exports
return FilmCraft job ids. `ui.screenshot` still needs presented pixels and upstream timeouts.
The original `window.filmcraft` API remains available.

## Host methods

| Method | Params | Result |
|---|---|---|
| `c0.inspect` | `{}` | Project/editor/host state below |
| `c0.goTo` | `{sequence?: integer id, frame?: nonnegative integer}` | Updated inspect state; activates the sequence and seeks in exact frame time |
| `c0.play` | `{action:"play"\|"pause"\|"toggle"}` | `{playing:boolean}`; play needs an active sequence |
| `c0.documentBytes` | `{format?:"fcproj"}` | `{format,name,revision,encoding:"base64",bytes,size,referencedMedia}` |
| `c0.openFile` | `{name,bytes}`; standard padded base64 | `{pending:false,document,opened}` or `{pending:true,reason:"unsavedChanges"}` |
| `c0.newDocument` | `{template?:"blank"}`; null also means blank | Same replacement result; creates an empty Untitled project |
| `c0.cancelFileDialog` | `{}` | `{cancelled:boolean}`; cancels pending replacement/native continuation and invalidates late browser-picker answers |
| `c0.set` | `{themeTokens?,panels?}` | `{applied:string[],unknown:string[],invalid:string[]}` |

`c0.inspect` returns:

- `revision`, `dirty`, `canUndo`, `canRedo`.
- `project:{name,dirty,revision}`.
- `sequences:[{id,name,active,durationFrames,fps:{num,den}}]`, `activeSequence` (id or null).
- `playheadFrame`, `playing`, `selectionCount` (selected timeline clips).
- `bins:[{name,items}]` (direct item count in each bin, including the root).
- `panes:{title_bar,menu_bar,header,status_bar,start_screen,community_links}`.
- `theme` (FilmCraft preference name: `darkest`, `dark`, or `light`), `fileDialogPending`.

Frame navigation rejects negative, fractional and unrepresentable offsets before changing the
active sequence. Inspect and document serialization do not change project revision or dirty
state. `.fcproj` files contain project JSON and references to media, not embedded media.
`referencedMedia` lists `{name,path,proxy}` for media files and proxies referenced by the project;
procedural generators do not need external files. Hosts must keep referenced media available
through FilmCraft's existing import/relink API when reopening a project.

Open and New validate before replacing anything. When the current project is dirty they keep
it intact and show FilmCraft-style Save/Discard/Cancel controls. The corresponding automation
ids are `c0.unsaved.save`, `c0.unsaved.discard`, and `c0.unsaved.cancel`. A successful save permits
replacement; cancellation or a failed save preserves the document. `c0.cancelFileDialog` also
dismisses the pending replacement. Browsers do not expose an imperative way to close every
native picker; cancellation prevents its later answer from importing a file.

## Chrome and theme

All six `Chrome` fields default to `true` and are never serialized into project/preferences.
This page explicitly disables `title_bar`, `header`, `menu_bar`, `start_screen`, and
`community_links`. Hosts can restore any of them with `c0.set`. Header/title hiding also hides
the header's embedded menu, while its own `menu_bar` switch is retained for restoration.
Disabling the start screen routes Home/Import presentation to the editor; importing media and
export settings remain available through commands:

```js
await filmcraft.execute('file.import');
await filmcraft.execute('mode.export');
// file.exportMedia, export.resolve, export.presets.list, export.formats and jobs.list
// retain their normal FilmCraft contracts; see docs/control-protocol.md.
```

`community_links:false` removes the Discord header button, Home community cards/ArtCraft mark,
Help promotions, About promotions/mark, and contributor-profile hyperlinks. Direct Help link
commands are refused, and direct community URL openers are suppressed. About retains FilmCraft,
its version, authors/contributors/models and licence. Import/Export commands remain available.

`themeTokens` replaces the context-local override map; `{}` or null clears it. Values are
`#RRGGBB` or `#RRGGBBAA`. Every one of FilmCraft's **82 scalar Color32 field names** in
`crates/ui-egui/src/theme.rs::Tokens` is accepted (including `app_bg`, `panel_bg`, `text`,
`accent`, `playhead`, `monitor_bg`). `kind`, `radius`, `radius_sm`, `gap`, and `tab_h` are not
colour tokens. Overrides survive theme changes. `panels` patches the boolean switches.
Valid siblings apply even when another key is unknown/invalid; returned keys are prefixed
`themeTokens.<name>` or `panels.<name>`. Invalid container types return an error envelope.

## Codec policy and browser limits

The `patent-codecs` feature is **on by default** throughout the ordinary native/web dependency
chain. Turning it off removes FilmCraft's H.264/HEVC decoders, H.264 encoder, AAC decoder/encoder
and Symphonia's AAC feature from the page's build graph. Software-codec-free stubs report a
normal unsupported error. FilmCraft's MP4 parser and writer remain.

With the feature off:

| Operation | Available path |
|---|---|
| H.264/HEVC MP4/MOV import | Existing browser VideoDecoder path, when the browser supports the actual stream |
| AAC MP4/MOV/M4A import | Browser AudioDecoder, with bounded random-access decode windows; all advertised audio streams are forwarded |
| H.264/HEVC in other containers; raw ADTS AAC | No software fallback; reports unsupported here |
| Other imports | Upstream image/still sequences, Y4M, MPEG-1/2, VP8/VP9/AV1, ProRes, DNx, APV, MJPEG/raw; PCM/WAV/AIFF, MP3/MP1/MP2, AC-3/E-AC-3, ALAC, FLAC, Vorbis and Opus, in their upstream containers |
| Default export | Motion JPEG MOV with PCM; this also replaces the default Quick Export preset's settings |
| Other software exports | ProRes/DNxHR/APV MOV, supported MXF modes, PNG/TIFF/BMP/JPEG/TGA/DPX/OpenEXR sequences, GIF, WAV/AIFF/FLAC |
| H.264 MP4 export | Browser VideoEncoder, muxed by FilmCraft's own MP4 writer; video-only is available without an AAC encoder |
| H.264 + AAC MP4 export | Browser VideoEncoder **and** AudioEncoder; an unavailable AAC encoder returns an unsupported error |
| HEVC/AV1 export and standalone AAC export | No browser export adapter; unavailable without another registered encoder |

`filmcraft.info().browserCodecs` reports `h264Encoder`, `aacEncoder`, `aacDecoder`, and
`softwarePatentCodecs:false`. The actual browser configuration can still reject a profile, size,
rate or channel layout; errors remain normal FilmCraft export/preview errors. Browser encoding
uses realtime AVC, the requested profile/level/keyframe interval, and one-pass VBR or CBR.
CRF, two-pass encoding and HDR H.264 return clear unsupported errors; select SDR for this adapter.
Without software SPS readers, H.264/HEVC metadata uses the container's descriptions rather than
the software decoder's VUI/crop inference. AAC-LC is the tested audio baseline; HE-AAC depends
on the browser's support and stream metadata.

Upstream non-stepped exports (image sequences, GIF, WAV, FLAC, standalone AAC) execute
synchronously on wasm and can block the page; no worker/thread rewrite is introduced. A pending
source now preserves audio cursors in stepped exports. Synchronous audio exports reject pending
media rather than downloading truncated audio, and loudness normalization rejects a pending
measurement with an instruction to disable normalization. All web rendering remains
cooperative; no worker threads are introduced.

## Verification

Native contract tests are in `crates/ui-egui/tests/c0_host.rs`; native browser-feature transport
tests are in `apps/filmcraft-web/tests/c0_page_control.rs` (K-41). Build with and without
`patent-codecs` and check the wasm dependency graph as well as artifact sizes.

```sh
CARGO_BUILD_JOBS=2 smoac-slot run heavy 2 3600 -- cargo test -p filmcraft-web --features page-control
CARGO_BUILD_JOBS=2 smoac-slot run heavy 2 3600 -- cargo check --target wasm32-unknown-unknown -p filmcraft-web --no-default-features --features page-control
NODE_PATH=/path/to/node_modules CHROMIUM_PATH=/path/to/chromium node c0/filmcraft-host/smoke.cjs
```

`smoke.cjs --adapter-only` verifies browser codecs without building wasm. The full smoke serves
the finished page on a temporary localhost port, exercises all host methods and unpresented
passes, opens actual `.fcproj` file bytes and a browser `File`, and exports MP4 where supported.
It also reloads with WebCodecs disabled and checks that unsupported export leaves the app responsive.
Screenshots and media proofs stay in `/tmp` (override `C0VID_SCREENSHOT`/`C0VID_EXPORT_PROOF`).
Measured results, sizes, patch debt and unconfirmed assumptions are in `LANE-REPORT.md` and
`PATCH-DEBT.md`.
