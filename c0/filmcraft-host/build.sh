#!/usr/bin/env bash
# Same cargo -> wasm-bindgen 0.2.129 pipeline as upstream, via cargo xtask web.
set -euo pipefail
cd "$(dirname "$0")/../.."
export CARGO_BUILD_JOBS=2
export BINARYEN_CORES=2
if [[ -n "${CRAFT_FONTS_DIR:-}" && ! -d "$CRAFT_FONTS_DIR" ]]; then
  echo "CRAFT_FONTS_DIR is missing; using bundled/system fonts (no optional craft-fonts glyphs)." >&2
  unset CRAFT_FONTS_DIR
fi
args=(web --page-control)
if [[ "${1:-}" != "--with-patent-codecs" ]]; then args+=(--no-default-features); fi
cat /proc/loadavg
set +e
smoac-slot run heavy 2 3600 -- cargo xtask "${args[@]}"
code=$?
set -e
if [[ "$code" == 13 || "$code" == 14 || "$code" == 73 || "$code" == 75 ]]; then
  smoac-slot run gate 3 3600 -- cargo xtask "${args[@]}"
elif [[ "$code" != 0 ]]; then exit "$code"; fi
python3 c0/filmcraft-host/assemble.py
cat /proc/loadavg
