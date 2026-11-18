import hashlib
import json
import struct
import xml.etree.ElementTree as ET
from pathlib import Path

from PIL import Image, ImageSequence

root = Path(__file__).resolve().parents[2]
rows = []
for p in sorted((root / "assets/brand").rglob("*.svg")):
    ET.parse(p)
for theme in ("light", "dark"):
    for size, scale in (
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
    ):
        p = (
            root
            / "assets/brand"
            / f"Bronze-{theme}.iconset"
            / f"icon_{size}x{size}{'@2x' if scale == 2 else ''}.png"
        )
        assert Image.open(p).size == (size * scale, size * scale)
    p = root / "assets/brand" / f"Bronze-{theme}.ico"
    ico = Image.open(p)
    assert ico.ico.sizes() == {
        (s, s) for s in (16, 20, 24, 32, 40, 48, 64, 96, 128, 256)
    }, ico.ico.sizes()
    for size in ico.ico.sizes():
        im = ico.ico.getimage(size)
        assert im.size == size
    p = root / "assets/brand" / f"Bronze-{theme}.icns"
    content = p.read_bytes()
    assert content[:4] == b"icns" and struct.unpack(">I", content[4:8])[0] == len(
        content
    )
    tags = []
    offset = 8
    while offset < len(content):
        tag = content[offset : offset + 4].decode()
        size = struct.unpack(">I", content[offset + 4 : offset + 8])[0]
        tags.append(tag)
        offset += size
    assert set(tags) == {
        "icp4",
        "icp5",
        "ic07",
        "ic08",
        "ic09",
        "ic10",
        "ic11",
        "ic12",
        "ic13",
        "ic14",
    }, tags
    assert Image.open(root / "assets/brand" / f"social-preview-{theme}.png").size == (
        1280,
        640,
    )
for p in sorted((root / "docs/media").glob("*.gif")):
    im = Image.open(p)
    frames = [f.copy() for f in ImageSequence.Iterator(im)]
    assert im.size == (400, 700) and len(frames) > 1
    assert (
        len({hashlib.sha256(f.convert("RGB").tobytes()).hexdigest() for f in frames})
        >= 2
    )
    seconds = sum(f.info.get("duration", 0) for f in frames) / 1000
    assert seconds < 8 and p.stat().st_size < 400000
    rows.append(
        {
            "file": p.name,
            "frames": len(frames),
            "seconds": seconds,
            "bytes": p.stat().st_size,
        }
    )
print(
    json.dumps(
        {
            "status": "verified",
            "icons": "both ICNS + ICO families, all declared sizes",
            "gif_metadata": rows,
        },
        indent=2,
    )
)
