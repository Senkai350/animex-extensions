"""Embed the reviewed provider artwork from docs/icons into module manifests.

Run from any directory: python scripts/update-icons.py. Changed manifests receive
a patch version bump because published package bytes are immutable.
"""
import base64
import hashlib
import json
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
ALIASES = {"player-hls": "anilibria", "player-animego": "animego"}
sources = json.loads((ROOT / "docs/icons/sources.json").read_text(encoding="utf-8"))
for manifest_path in sorted((ROOT / "modules").glob("*/module.json")):
    manifest = json.loads(manifest_path.read_text(encoding="utf-8"))
    provider = ALIASES.get(manifest_path.parent.name, manifest_path.parent.name.split("-", 1)[1])
    source = sources[provider]
    icon = None
    if source:
        artwork = (ROOT / source["file"]).read_bytes()
        assert hashlib.sha256(artwork).hexdigest() == source["sha256"], provider
        assert 0 < len(artwork) <= 256 * 1024, provider
        icon = f'data:{source["mime"]};base64,{base64.b64encode(artwork).decode("ascii")}'
    if "icon" in manifest and manifest["icon"] == icon:
        continue
    manifest["icon"] = icon
    major, minor, patch = map(int, manifest["version"].split("."))
    manifest["version"] = f"{major}.{minor}.{patch + 1}"
    manifest_path.write_text(json.dumps(manifest, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
    print(f'{manifest["id"]} {manifest["version"]}: {provider if icon else "no verified artwork"}')
