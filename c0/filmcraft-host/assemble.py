"""Assemble a relative, content-addressed site from upstream's generated binary."""
import hashlib
import json
import os
import re
from pathlib import Path
import shutil
import tempfile

root = Path(__file__).resolve().parents[2]
target = Path(os.environ.get("CARGO_TARGET_DIR", root / "target"))
source = target / "web" / "dist"
dist = root / "dist" / "c0host"
digest = hashlib.sha256()
files = [source / "filmcraft_web.js", source / "filmcraft_web_bg.wasm", source / "audio-worklet.js"]
if (source / "snippets").is_dir():
    files.extend(sorted(path for path in (source / "snippets").rglob("*") if path.is_file()))
for path in files:
    digest.update(str(path.relative_to(source)).encode())
    with path.open("rb") as stream:
        for chunk in iter(lambda: stream.read(1024 * 1024), b""):
            digest.update(chunk)
build = digest.hexdigest()[:16]
dist.parent.mkdir(parents=True, exist_ok=True)
with tempfile.TemporaryDirectory(prefix="c0host-stage-", dir=dist.parent) as staging:
    stage = Path(staging)
    if (source / "snippets").is_dir():
        shutil.copytree(source / "snippets", stage / f"snippets.{build}")
    # Only the module specifiers move to the build folder. The import-table keys must keep the names compiled into
    # the wasm: eframe 0.37 brings an inline snippet the wasm imports as ./snippets/..., and renaming that key broke
    # instantiation ("Import #32 ... module is not an object or function", 2026-10-26).
    glue = re.sub(r"""(from\s*['"]|import\(\s*['"])\./snippets/""", lambda m: f"{m.group(1)}./snippets.{build}/",
                  (source / "filmcraft_web.js").read_text())
    (stage / f"filmcraft_web.{build}.js").write_text(glue)
    shutil.copy2(source / "filmcraft_web_bg.wasm", stage / f"filmcraft_web_bg.{build}.wasm")
    shutil.copy2(source / "audio-worklet.js", stage / "audio-worklet.js")
    (stage / "index.html").write_text((Path(__file__).parent / "index.html").read_text().replace("__FC_BUILD__", build))
    if dist.exists():
        shutil.rmtree(dist)
    stage.rename(dist)
print(json.dumps({"dist": str(dist), "build": build, "wasmBytes": (source / "filmcraft_web_bg.wasm").stat().st_size,
                  "totalBytes": sum(path.stat().st_size for path in dist.rglob("*") if path.is_file())}))
