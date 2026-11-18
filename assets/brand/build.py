#!/usr/bin/env python3
import os
import re
import shutil
import subprocess
import sys
import struct
import tempfile
from io import BytesIO
from pathlib import Path

from PIL import Image, ImageFilter

BRAND = Path(__file__).resolve().parent
ICON = BRAND / "icon"
ROOT = BRAND.parent.parent
MAC_SIZES = (16, 32, 64, 128, 256, 512, 1024)
WINDOWS_SIZES = (16, 24, 32, 48, 64, 128, 256)
ICNS_SLOTS = {"ic04": 16, "ic05": 32, "ic11": 32, "ic12": 64, "ic07": 128, "ic13": 256, "ic08": 256, "ic14": 512, "ic09": 512, "ic10": 1024}


def resvg() -> str:
    found = shutil.which("resvg") or str(Path.home() / ".cargo/bin/resvg")
    if not Path(found).exists():
        sys.exit("build: resvg not found; install it with `cargo install resvg`")
    return found


def inter_fonts() -> list[str]:
    rev = re.search(r'rune = \{[^}]*rev = "([0-9a-f]+)"', (ROOT / "Cargo.toml").read_text())[1]
    cargo = Path(os.environ.get("CARGO_HOME", Path.home() / ".cargo"))
    fonts = [sorted(cargo.glob(f"git/checkouts/rune-*/{rev[:7]}/crates/rune/assets/Inter-{w}.ttf")) for w in ("Regular", "Medium", "SemiBold")]
    if not all(fonts):
        sys.exit(f"build: Inter from rune {rev[:7]} not found; run cargo build first")
    return [arg for found in fonts for arg in ("--use-font-file", str(found[0]))]


def svg(source: Path, width: int, out: Path, fonts: list[str] | None = None) -> Image.Image:
    subprocess.run([resvg(), *(fonts or []), "--skip-system-fonts", "-w", str(width), str(source), str(out)], check=True)
    return Image.open(out).convert("RGBA")


def mac(tile: Image.Image, size: int) -> Image.Image:
    body = round(size * 824 / 1024)
    offset = (size - body) // 2
    canvas = Image.new("RGBA", (size, size), (0, 0, 0, 0))
    shadow = Image.new("RGBA", (size, size), (0, 0, 0, 0))
    alpha = tile.resize((body, body), Image.LANCZOS).getchannel("A").point(lambda a: a * 0.3)
    shadow.paste((0, 0, 0, 255), (offset, offset + round(size * 10 / 1024)), alpha)
    canvas.alpha_composite(shadow.filter(ImageFilter.GaussianBlur(size * 10 / 1024)))
    canvas.alpha_composite(tile.resize((body, body), Image.LANCZOS), (offset, offset))
    return canvas


def windows(tile: Image.Image, size: int) -> Image.Image:
    body = round(size * 15 / 16)
    canvas = Image.new("RGBA", (size, size), (0, 0, 0, 0))
    canvas.alpha_composite(tile.resize((body, body), Image.LANCZOS), ((size - body) // 2, (size - body) // 2))
    return canvas


def write_icns(images: dict[int, Image.Image], out: Path) -> None:
    chunks = []
    for slot, size in ICNS_SLOTS.items():
        data = BytesIO()
        images[size].save(data, "PNG", optimize=True)
        chunks.append(slot.encode() + struct.pack(">I", 8 + len(data.getvalue())) + data.getvalue())
    body = b"".join(chunks)
    out.write_bytes(b"icns" + struct.pack(">I", 8 + len(body)) + body)


def icons(variant: str, tmp: Path) -> None:
    tile = svg(ICON / f"icon-{variant}.svg", 2048, tmp / f"{variant}-tile.png")
    small = {16: svg(ICON / f"icon-{variant}-16.svg", 16, tmp / f"{variant}-16.png")}
    small[24] = svg(ICON / f"icon-{variant}-32.svg", 24, tmp / f"{variant}-24.png")
    small[32] = svg(ICON / f"icon-{variant}-32.svg", 32, tmp / f"{variant}-32.png")

    png = ICON / "png" / variant
    png.mkdir(parents=True, exist_ok=True)
    mac_images = {size: mac(tile, size) for size in MAC_SIZES}
    for size, image in mac_images.items():
        image.save(png / f"mac-{size}.png", optimize=True)
    windows_images = {size: small.get(size) or windows(tile, size) for size in WINDOWS_SIZES}
    for size, image in windows_images.items():
        image.save(png / f"windows-{size}.png", optimize=True)

    write_icns(mac_images, ICON / f"bronze-{variant}.icns")
    windows_images[256].save(ICON / f"bronze-{variant}.ico", sizes=[(s, s) for s in WINDOWS_SIZES], append_images=[windows_images[s] for s in WINDOWS_SIZES[:-1]])


def main() -> None:
    with tempfile.TemporaryDirectory() as tmp:
        tmp = Path(tmp)
        for variant in ("dark", "light"):
            icons(variant, tmp)
            svg(BRAND / f"logo-{variant}.svg", 1600, BRAND / f"logo-{variant}.png")
            svg(BRAND / f"mark-{variant}.svg", 512, BRAND / f"mark-{variant}.png")
        if not (ROOT / "docs/media/panel-dark.png").exists():
            sys.exit("build: run docs/media/src/render.py panel first; the social preview embeds its panel")
        svg(BRAND / "social-preview.svg", 1280, tmp / "social.png", inter_fonts()).convert("RGB").save(BRAND / "social-preview.png", optimize=True)
    for out in sorted(BRAND.rglob("*")):
        if out.suffix in {".png", ".icns", ".ico"}:
            print(f"{out.relative_to(ROOT)}  {out.stat().st_size // 1024} KB")


if __name__ == "__main__":
    main()
