#!/usr/bin/env python3
"""Empaqueta Parkour para distribuir, en dist/:

  * parkour.hnmod: la parte nativa (código Rust). Es lo que instala "Add from GitHub" del launcher.
  * parkour.hmod:  la parte de contenido (sonidos, partículas), sólo si content/ tiene algo. Horizon aún
                   no instala .hmod desde un enlace, así que va a mano en la carpeta horizon-mods/.

Uso: python3 tools/package.py
"""
import json
import re
import sys
import zipfile
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
DIST = ROOT / "dist"
NATIVE = ROOT / "parkour"
CONTENT = ROOT / "content"
MIXINS_REV = "5a64af938ec86b00d53d16a408aabc0038fb3e57"  # horizon-mixins de la Nightly fijada

# Fecha fija: el mismo código produce el mismo archivo (y el mismo SHA-256).
STAMP = (2026, 1, 1, 0, 0, 0)


def add(zf: zipfile.ZipFile, arcname: str, data: bytes) -> None:
    info = zipfile.ZipInfo(arcname, STAMP)
    info.compress_type = zipfile.ZIP_DEFLATED
    info.external_attr = 0o644 << 16
    zf.writestr(info, data)


def toml_value(text: str, key: str) -> str:
    m = re.search(rf'^{key}\s*=\s*"([^"]+)"', text, re.M)
    if not m:
        sys.exit(f"falta {key} en horizon-mixin.toml")
    return m.group(1)


def pack_native() -> Path:
    manifest = (NATIVE / "horizon-mixin.toml").read_text(encoding="utf-8")
    meta = {
        "schema": 1,
        "id": toml_value(manifest, "id"),
        "version": toml_value(manifest, "version"),
        "game": "26.3",
        "horizon": toml_value(manifest, "horizon_revision"),
        "mixins": MIXINS_REV,
    }
    out = DIST / f"{meta['id']}.hnmod"
    files = sorted(p for p in NATIVE.rglob("*")
                   if p.is_file() and "target" not in p.parts
                   and (p.suffix in {".rs", ".toml"} or p.name.startswith("LICENSE")))
    with zipfile.ZipFile(out, "w") as zf:
        add(zf, "horizon-native.json", (json.dumps(meta, indent=2) + "\n").encode())
        for p in files:
            add(zf, f"mixins/{meta['id']}/{p.relative_to(NATIVE).as_posix()}", p.read_bytes())
    return out


def pack_content() -> Path:
    manifest = (CONTENT / "horizon-mod.toml").read_text(encoding="utf-8")
    out = DIST / f"{toml_value(manifest, 'id')}.hmod"
    files = [CONTENT / "horizon-mod.toml"] + sorted(
        p for d in ("assets", "data") for p in (CONTENT / d).rglob("*") if p.is_file())
    with zipfile.ZipFile(out, "w") as zf:
        for p in files:
            add(zf, p.relative_to(CONTENT).as_posix(), p.read_bytes())
    return out


def has_content() -> bool:
    return any(p.is_file() for d in ("assets", "data") for p in (CONTENT / d).rglob("*"))


def main() -> None:
    native = toml_value((NATIVE / "horizon-mixin.toml").read_text(), "version")
    content = toml_value((CONTENT / "horizon-mod.toml").read_text(), "version")
    if native != content:
        sys.exit(f"versiones distintas: nativo {native}, contenido {content}")
    DIST.mkdir(exist_ok=True)
    # El .hmod sólo existe cuando hay contenido (sonidos, partículas...): aún no en la fase 0.
    paths = [pack_native()] + ([pack_content()] if has_content() else [])
    for path in paths:
        print(f"{path.relative_to(ROOT)}  ({path.stat().st_size} bytes)")


if __name__ == "__main__":
    main()
