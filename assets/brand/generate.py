import struct
from io import BytesIO
from pathlib import Path

import cairosvg
from PIL import Image, PngImagePlugin

ROOT = Path(__file__).resolve().parent
MAC_SIZES = (
    (16, 1),
    (16, 2),
    (32, 1),
    (32, 2),
    (128, 1),
    (128, 2),
    (256, 1),
    (256, 2),
    (512, 1),
    (512, 2),
)
WINDOWS_SIZES = (16, 20, 24, 32, 40, 48, 64, 96, 128, 256)
ICNS_TYPES = (
    (16, b"icp4"),
    (32, b"icp5"),
    (128, b"ic07"),
    (256, b"ic08"),
    (512, b"ic09"),
    (1024, b"ic10"),
    (32, b"ic11"),
    (64, b"ic12"),
    (256, b"ic13"),
    (512, b"ic14"),
)


def raster(source, size):
    return Image.open(
        BytesIO(
            cairosvg.svg2png(url=str(source), output_width=size, output_height=size)
        )
    ).convert("RGBA")


def png(image, destination, source):
    metadata = PngImagePlugin.PngInfo()
    metadata.add_text("Source", source)
    image.save(destination, pnginfo=metadata, optimize=True)


for theme in ("light", "dark"):
    source = ROOT / f"app-icon-{theme}.svg"
    master = raster(source, 1024)
    iconset = ROOT / f"Bronze-{theme}.iconset"
    iconset.mkdir(exist_ok=True)
    for logical, scale in MAC_SIZES:
        suffix = "@2x" if scale == 2 else ""
        png(
            raster(source, logical * scale),
            iconset / f"icon_{logical}x{logical}{suffix}.png",
            source.name,
        )
    windows = ROOT / f"windows-{theme}"
    windows.mkdir(exist_ok=True)
    for size in WINDOWS_SIZES:
        png(raster(source, size), windows / f"icon-{size}.png", source.name)
    entries = []
    for size, kind in ICNS_TYPES:
        data = BytesIO()
        raster(source, size).save(data, format="PNG")
        entries.append(
            kind + struct.pack(">I", len(data.getvalue()) + 8) + data.getvalue()
        )
    payload = b"".join(entries)
    (ROOT / f"Bronze-{theme}.icns").write_bytes(
        b"icns" + struct.pack(">I", len(payload) + 8) + payload
    )
    frames = [raster(source, size) for size in WINDOWS_SIZES]
    master.save(
        ROOT / f"Bronze-{theme}.ico",
        sizes=[(size, size) for size in WINDOWS_SIZES],
        append_images=frames,
        bitmap_format="bmp",
    )
    png(master, ROOT / f"app-icon-{theme}-1024.png", source.name)
    for name, width in [("logo", 840), ("mark", 256), ("social-preview", 1280)]:
        source = ROOT / f"{name}-{theme}.svg"
        result = Image.open(
            BytesIO(cairosvg.svg2png(url=str(source), output_width=width))
        )
        png(result, ROOT / f"{name}-{theme}.png", source.name)
