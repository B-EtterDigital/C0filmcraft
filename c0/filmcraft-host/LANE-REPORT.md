# C0vid FilmCraft guest lane report

Guest implementation, release builds and Chromium smoke are complete. The required UI,
engine, export and MP4 suites pass. **The full workspace gate remains red** on two untouched
upstream assertion failures; a GPU test process also needed a successful serial retry.
**The patch-debt target below 70 is not met: Ratchet 259.**

Branch: `c0/main`. Base/main: `d643a886`. No push or upstream submission was made.

## Commits

- `d38ef1a7c0537f4d95b625e0a0178897f5af5943` — `feat(c0vid): embed FilmCraft as a host-controlled guest`.
- This report's commit (HEAD at hand-back) — `docs(c0vid): record guest lane verification`.
  Its exact hash is printed in the stdout hand-back; a commit cannot contain its own hash.

## Delivered

- `c0/filmcraft-host/build.sh` builds the existing filmcraft-web binary via upstream's
  cargo/wasm-bindgen 0.2.129 pipeline, then assembles content-addressed, relative URLs in
  `dist/c0host/`. FilmCraft does not use Trunk. The page starts with `?empty&norecover`,
  transparent until painted, without a demo or recovery document.
- The page-control feature wraps `window.filmcraft.request` and its existing queue.
  `__c0hostRequest(json) => __filmcraftControl(json)` performs an unpresented egui pass.
  All eight c0 methods, guarded project replacement, read-only fcproj bytes/media references,
  default-on chrome switches and all 82 colour tokens are documented in README.md.
- Default-on patent-codecs features preserve ordinary builds. The shipping page disables
  FilmCraft H.264/HEVC/AAC decoders, H.264/AAC encoders and Symphonia AAC. A cargo-tree audit
  found none of those codec implementations/features in the shipping wasm dependency graph.
- Browser AAC decoding and H.264/AAC encoding use WebCodecs; MP4 output uses FilmCraft's writer.
  Pending audio now preserves its cursor; synchronous exports reject pending audio instead
  of publishing truncated output. Generic fixes are classified in PATCH-DEBT.md for upstream
  offering; no external messages or submissions were sent.

## Verification

All heavy commands ran locally inside the admitted `smoac-slot run heavy 2 3600` wrapper,
with `CARGO_BUILD_JOBS=2`, `RUST_TEST_THREADS=2` and `RAYON_NUM_THREADS=2`.
The gate used `CARGO_PROFILE_RELEASE_LTO=false` to avoid ThinLTO on every integration binary.
Production wasm builds retained upstream's ThinLTO profile. wasm-opt used `BINARYEN_CORES=2`.

| Check | Result |
|---|---|
| K-41: cargo test -p filmcraft-web --features page-control | 10 passed; 2 new protocol + 8 upstream tests |
| UI suite, including c0 host tests | 675 passed, 4 ignored |
| Engine suite with newer external oracle | 653 passed, 3 ignored |
| Export suite, including pending/async audio regressions | 131 passed |
| isobmff suite, including final audio timing regression | 63 passed, 1 ignored |
| New tests across the four test files | 16 passed: 9 host, 2 transport, 4 audio, 1 mux |
| GPU serial rerun of the same compiled binary | 41 passed |
| Exact wasm cargo check, page-control with defaults | passed |
| Exact wasm cargo check, page-control without default features | passed |
| Workspace release clippy --all-targets, -D warnings | passed |
| Native codec-off codecs/export/ui libraries, clippy --no-deps -D warnings | passed |
| Wasm codec-off web package, clippy --no-deps -D warnings | passed |
| cargo fmt --all --check; git diff --check | passed |
| cargo xtask layers | passed |
| cargo xtask assets | passed; 69 attributed assets |
| cargo xtask wasm | passed |
| Both release page builds | passed |
| Headless Chromium guest smoke | passed; 8 methods, 8 unpresented requests, real files, no page errors |

The full FFmpeg 8.1 workspace run used `cargo test --workspace --release --no-fail-fast`.
Completed harnesses reported **3,527 passed, 2 failed, 45 ignored**; its GPU harness received
SIGSEGV after 22 successful cases, so it has no harness summary in that count. The same GPU
binary then passed all 41 tests with `--test-threads=1` (9.46 s). Including that completed
retry gives **3,568 passed, 2 failed, 45 ignored** across the completed harnesses. The raw
workspace run exited 101; it is not reported as a green full gate.

Remaining assertion failures:
- `dev_icon::tests::desktop_file_matches_the_packaged_fields`: the development desktop entry
  lacks the packaged entry's MimeType line. Final `cargo xtask ci` stops here (74 passed,
  1 failed in that binary).
- `ffmpeg_ycbcr_tiffs_match_ffmpeg_decode`: yuv420p TIFF worst channel difference 17.

Both failing source areas have no diff from main. Their fixes are outside this guest lane's
default-behaviour constraint. Dependency-inclusive wasm clippy also finds an unchanged
`clippy::drop_non_drop` at `crates/engine/src/proxies.rs:417`; the package checks above
use --no-deps, while native workspace clippy checks all touched crates with -D warnings.

The installed FFmpeg/ffprobe 7.0.2 initially produced 14 assertion failures. Its TS decode
also reproduced SIGSEGV in an isolated subprocess (return -11, empty stderr), while MP4
returned 0. An external [BtbN FFmpeg 8.1 build](https://github.com/BtbN/FFmpeg-Builds/releases/tag/latest)
in `/tmp/c0vid-oracles81/` reduced the assertion failures to the two above. Actual version:
`n8.1.3-20-gc8de997cb1-20261021`. Archive:
`ffmpeg-n8.1-latest-linux64-gpl-8.1.tar.xz`, publisher-verified SHA-256
`56df3ea697554a4e50ae4aec1737fa4e71fa201fd8b768f35b736d8c36ada138`.
It was used only through FILMCRAFT_FFMPEG/FILMCRAFT_FFPROBE as an external test oracle;
its binaries and libraries are outside the repo and finished site.

## Browser proof and codec-off formats

Chromium used WebGL and the CPU compositor. Startup to ready was 992.9 ms in the final smoke.
The empty project was clean, with no sequences, header, menu, Home or community links.
A real fcproj byte round trip and browser File open succeeded. A generated H.264/AAC MP4
imported without errors; 9 frames decoded with zero software fallbacks. Audio meters showed
the imported audio. The inspected screenshot retains the timeline, media and status bar.

H.264 browser export produced a **3,474-byte MP4**, verified by ffprobe: **3 frames,
64 × 64, 0.100000 s**. Chromium advertised H.264 encoding and AAC decoding, but no AAC encoding;
the tested export is video-only. The standalone adapter also encoded three High-profile
frames and decoded an AAC oracle to 19,200 nonzero channel samples. With `?nowebcodecs`,
export returned `H.264 export is not available (no encoder registered)` and inspection
still answered.

| Codec-off operation | Available path / result |
|---|---|
| H.264/HEVC MP4/MOV import | Browser VideoDecoder only, conditional on actual stream support |
| AAC MP4/MOV/M4A import | Browser AudioDecoder only |
| H.264/HEVC in other containers; raw ADTS AAC | Unsupported; no software fallback |
| Other retained imports | Images/sequences, Y4M, MPEG-1/2, VP8/VP9/AV1, ProRes, DNx, APV, MJPEG/raw; PCM/WAV/AIFF, MP3/MP1/MP2, AC-3/E-AC-3, ALAC, FLAC, Vorbis, Opus in upstream containers |
| Default export | Motion JPEG MOV + PCM |
| Other retained exports | ProRes/DNxHR/APV MOV, supported MXF, PNG/TIFF/BMP/JPEG/TGA/DPX/OpenEXR sequences, GIF, WAV/AIFF/FLAC |
| H.264 MP4 export | Browser VideoEncoder; video-only works without AAC encoding |
| H.264 + AAC MP4 export | Requires browser VideoEncoder and AAC AudioEncoder; unavailable in tested Chromium |
| HEVC/AV1 export; standalone AAC export | No browser export adapter |

Only fcproj and H.264/AAC MP4 were exercised end-to-end in this guest; the other retained
formats are upstream paths, not a claim that every browser/profile was tested.
Non-stepped image/GIF/WAV/FLAC/AAC exports remain synchronous on wasm and can block the page.
Loudness measurement and synchronous audio exports reject a pending source with an actionable
error; there is no worker/thread rewrite.

Proof files remain outside git: `/tmp/c0vid-smoke.png`, `/tmp/c0vid-export.mp4`,
`/tmp/c0vid-export-proof.json`, `/tmp/c0vid-page-smoke-final.log`.
Native logs: `/tmp/c0vid-native-81.log`, `/tmp/c0vid-gpu-serial.log`,
`/tmp/c0vid-ci-final.log`; the earlier-oracle full run is `/tmp/c0vid-workspace-all.log`.

## Release sizes and load

| Build | wasm bytes | Total finished-site bytes | Content hash |
|---|---:|---:|---|
| Comparison, patent-codecs on | 33,153,879 | 33,340,556 | 2949dbd34f5a93db |
| Shipping page, patent-codecs off | 31,818,179 | 32,016,080 | 88d9b3c320a7b60c |

Shipping site: `dist/c0host/`. Wasm reduction: 1,335,700 bytes.
CRAFT_FONTS_DIR was unset: the optional craft-fonts table is empty and extra glyphs are not
embedded; bundled fonts remain. build.sh also handles a configured but missing font directory.

Exact `cat /proc/loadavg` output around each page build:

| Build | Before | After |
|---|---|---|
| Comparison | `11.74 10.81 12.09 18/2013 1697666` | `13.79 13.59 14.61 12/2011 3310799` |
| Shipping | `13.28 12.92 14.04 11/1997 3496719` | `30.95 22.96 17.88 25/2183 3694177` |

The first heavy admission waited 3,600 s and refused with exit 13; its gate fallback waited
3,600 s and refused with exit 75 (landing mode). Neither ran cargo. A later heavy retry
acquired slot-2 and performed the work reported here.

## Every modified upstream file

| File | Added lines |
|---|---:|
| `Cargo.lock` | 3 |
| `Cargo.toml` | 4 |
| `apps/filmcraft-cli/Cargo.toml` | 3 |
| `apps/filmcraft-web/Cargo.toml` | 9 |
| `apps/filmcraft-web/src/import.rs` | 16 |
| `apps/filmcraft-web/src/lib.rs` | 6 |
| `apps/filmcraft-web/src/webcodecs.rs` | 2 |
| `apps/filmcraft/Cargo.toml` | 3 |
| `crates/automation/Cargo.toml` | 1 |
| `crates/codecs/Cargo.toml` | 8 |
| `crates/codecs/src/audio.rs` | 18 |
| `crates/codecs/src/hw.rs` | 9 |
| `crates/codecs/src/lib.rs` | 3 |
| `crates/codecs/src/mp4.rs` | 4 |
| `crates/codecs/src/mxf.rs` | 3 |
| `crates/codecs/src/stream_color.rs` | 7 |
| `crates/codecs/src/video.rs` | 15 |
| `crates/engine/Cargo.toml` | 2 |
| `crates/export/Cargo.toml` | 7 |
| `crates/export/src/aac_out.rs` | 3 |
| `crates/export/src/audio_out.rs` | 10 |
| `crates/export/src/job.rs` | 21 |
| `crates/export/src/lib.rs` | 41 |
| `crates/export/src/presets.rs` | 2 |
| `crates/gpu/Cargo.toml` | 1 |
| `crates/isobmff/src/mux.rs` | 1 |
| `crates/platform/Cargo.toml` | 2 |
| `crates/ui-egui/Cargo.toml` | 7 |
| `crates/ui-egui/src/control.rs` | 1 |
| `crates/ui-egui/src/credits.rs` | 2 |
| `crates/ui-egui/src/header.rs` | 15 |
| `crates/ui-egui/src/lib.rs` | 10 |
| `crates/ui-egui/src/links.rs` | 3 |
| `crates/ui-egui/src/menus.rs` | 2 |
| `crates/ui-egui/src/panels/dialogs.rs` | 5 |
| `crates/ui-egui/src/panels/import_mode.rs` | 3 |
| `crates/ui-egui/src/theme.rs` | 1 |
| `xtask/src/main.rs` | 6 |

Total: **259 added lines, 63 removed lines in 38 upstream files.**
Ratchet: **259**. Measured with
`git diff --numstat --diff-filter=M main c0/main -- . ':!c0'`.
The target below 70 is exceeded by codec-feature propagation through the existing dependency
graph, browser encoder completion/timing hooks, and the chrome/theme/guard seams.
PATCH-DEBT.md lists each rationale and upstream-offer classification.

## Assumptions I could not confirm

- Actual AAC browser encoding, priming/padding behaviour and H.264 + AAC export on a browser
  that provides an AAC AudioEncoder; the synthetic async/mux tests pass.
- HEVC browser decoding (not advertised by this Chromium), HE-AAC, and every production
  profile/container combination.
- The cause of the one parallel native GPU SIGSEGV; the serial retry passes, and the earlier
  full run also passed all 41 GPU tests.
- A clean-main rebuild of the two untouched failing tests was not performed. Their failing
  source paths and media implementation have no lane edits.
- C0VIBE-side embedding/navigation integration was not run; the guest was served over localhost.
- Optional craft-fonts glyph coverage was not tested because that input was unset.
