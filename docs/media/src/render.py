#!/usr/bin/env python3
import json
import os
import re
import shutil
import subprocess
import sys
import tempfile
from dataclasses import dataclass, field
from functools import cache
from pathlib import Path
from typing import Literal

from PIL import Image, ImageDraw, ImageFilter, ImageFont

SRC = Path(__file__).resolve().parent
MEDIA = SRC.parent
ROOT = MEDIA.parent.parent
BIN = ROOT / "target/debug/bronze"
SCALE = 2
FPS = 20
PANEL = (360, 640)
WINDOW_RADIUS = 12
HOLD = 1.6


@dataclass(frozen=True)
class Palette:
    wall: tuple[str, str]
    glow: str
    window: str
    window_edge: str
    bar: str
    rule: str
    ink: str
    muted: str
    selection: str
    cap: str
    cap_edge: str
    cap_ink: str
    shadow: int


PALETTES = {
    "light": Palette(
        wall=("#f4efe8", "#e4d9cc"),
        glow="#f6d7b5",
        window="#ffffff",
        window_edge="#00000014",
        bar="#f7f6f4",
        rule="#e7e5e2",
        ink="#1c1c1e",
        muted="#76767c",
        selection="#f3d9bf",
        cap="#ffffff",
        cap_edge="#0000001f",
        cap_ink="#1c1c1e",
        shadow=70,
    ),
    "dark": Palette(
        wall=("#24201d", "#121110"),
        glow="#5a3a1f",
        window="#232325",
        window_edge="#ffffff1a",
        bar="#2b2b2e",
        rule="#3a3a3d",
        ink="#ececee",
        muted="#9a9aa0",
        selection="#6b4a2c",
        cap="#2e2e31",
        cap_edge="#ffffff26",
        cap_ink="#ececee",
        shadow=150,
    ),
}


Layout = Literal["desktop", "panel", "tour", "bare"]


@dataclass(frozen=True)
class Shot:
    script: str
    layout: Layout
    gif: bool = False
    selection: str | None = None
    sweep: tuple[float, float] | None = None
    panel: tuple[int, int] = PANEL
    width: int | None = None


ARTICLE_SELECTION = "Ask for a migration plan before anyone touches the schema."

SHOTS = {
    "hero": Shot("capture.json", "desktop", selection=ARTICLE_SELECTION, sweep=(0.5, 1.3)),
    "capture": Shot("capture.json", "desktop", gif=True, selection=ARTICLE_SELECTION, sweep=(0.5, 1.3), width=1200),
    "queue": Shot("queue.json", "panel", gif=True, panel=(360, 560), width=600),
    "search": Shot("search.json", "panel", gif=True, panel=(360, 560), width=600),
    "tour": Shot("", "tour", panel=(360, 560)),
    "panel": Shot("rest.json", "bare"),
}

TOUR = ("rest.json", "search-still.json", "switcher-still.json")


@cache
def inter(weight: str, size: int) -> ImageFont.FreeTypeFont:
    rev = re.search(r'rune = \{[^}]*rev = "([0-9a-f]+)"', (ROOT / "Cargo.toml").read_text())[1]
    cargo = Path(os.environ.get("CARGO_HOME", Path.home() / ".cargo"))
    found = sorted(cargo.glob(f"git/checkouts/rune-*/{rev[:7]}/crates/rune/assets/Inter-{weight}.ttf"))
    if not found:
        sys.exit(f"render: Inter-{weight}.ttf from rune {rev[:7]} not found; run cargo build first")
    return ImageFont.truetype(found[0], size * SCALE)


def px(value: float) -> int:
    return round(value * SCALE)


def rgba(hex_color: str) -> tuple[int, int, int, int]:
    h = hex_color.lstrip("#")
    return tuple(int(h[i : i + 2], 16) for i in range(0, 6, 2)) + ((int(h[6:8], 16),) if len(h) == 8 else (255,))


def bronze(theme: str, script: Path, out_args: list[str], selection: str | None, panel: tuple[int, int]) -> None:
    with tempfile.TemporaryDirectory() as data:
        shutil.copy(SRC / "notes.json", data)
        args = [str(BIN), "--data", data, "--scale", str(SCALE), "--theme", theme, "--size", f"{panel[0]}x{panel[1]}"]
        args += ["--script", str(script), *(["--selection", selection] if selection else []), *out_args]
        run = subprocess.run(args, capture_output=True, text=True)
        if run.returncode:
            sys.exit(f"render: bronze failed\n{run.stdout}{run.stderr}")


def still(theme: str, script: str, shot: Shot) -> Image.Image:
    with tempfile.TemporaryDirectory() as tmp:
        snap = Path(tmp) / "snap.png"
        bronze(theme, SRC / "scripts" / script, ["--at", "mark:end", "--snap", str(snap)], shot.selection, shot.panel)
        return Image.open(snap).convert("RGBA")


def frames(theme: str, shot: Shot, tmp: Path) -> list[Path]:
    out = tmp / "frames"
    out.mkdir()
    args = ["--out", str(out), "--fps", str(FPS), "--duration", "auto", "--pointer"]
    bronze(theme, SRC / "scripts" / shot.script, args, shot.selection, shot.panel)
    return sorted(out.glob("frame_*.png"))


@cache
def wallpaper(theme: str, size: tuple[int, int]) -> Image.Image:
    p = PALETTES[theme]
    top, bottom = rgba(p.wall[0]), rgba(p.wall[1])
    column = Image.new("RGBA", (1, size[1]))
    for y in range(size[1]):
        f = y / (size[1] - 1)
        column.putpixel((0, y), tuple(round(a + (b - a) * f) for a, b in zip(top, bottom)))
    wall = column.resize(size)
    glow = Image.new("RGBA", size, (0, 0, 0, 0))
    r = max(size) * 0.42
    cx, cy = size[0] * 0.78, size[1] * 0.92
    ImageDraw.Draw(glow).ellipse((cx - r, cy - r * 0.7, cx + r, cy + r * 0.7), fill=rgba(p.glow)[:3] + (150,))
    return Image.alpha_composite(wall, glow.filter(ImageFilter.GaussianBlur(max(size) * 0.12)))


def rounded(image: Image.Image, radius: int) -> Image.Image:
    mask = Image.new("L", image.size, 0)
    ImageDraw.Draw(mask).rounded_rectangle((0, 0, image.size[0] - 1, image.size[1] - 1), radius, fill=255)
    out = image.copy()
    out.putalpha(mask)
    return out


def place(canvas: Image.Image, window: Image.Image, at: tuple[int, int], theme: str) -> None:
    p = PALETTES[theme]
    radius = px(WINDOW_RADIUS)
    pad = px(60)
    shadow = Image.new("RGBA", (window.size[0] + 2 * pad, window.size[1] + 2 * pad), (0, 0, 0, 0))
    box = (pad, pad + px(14), pad + window.size[0], pad + window.size[1] + px(10))
    ImageDraw.Draw(shadow).rounded_rectangle(box, radius, fill=(0, 0, 0, p.shadow))
    canvas.alpha_composite(shadow.filter(ImageFilter.GaussianBlur(px(22))), (at[0] - pad, at[1] - pad))
    canvas.alpha_composite(rounded(window, radius), at)
    ring = Image.new("RGBA", window.size, (0, 0, 0, 0))
    ImageDraw.Draw(ring).rounded_rectangle((0, 0, window.size[0] - 1, window.size[1] - 1), radius, outline=rgba(p.window_edge), width=SCALE)
    canvas.alpha_composite(ring, at)


@dataclass
class Article:
    theme: str
    size: tuple[int, int] = (800, 624)
    lines: list[tuple[str, tuple[float, float], ImageFont.FreeTypeFont, str]] = field(default_factory=list)
    selection: list[tuple[float, float, float, float]] = field(default_factory=list)
    renders: dict[float, Image.Image] = field(default_factory=dict)

    def __post_init__(self) -> None:
        p = PALETTES[self.theme]
        left, width, y = 64, 672, 92
        self.lines.append(("Moving the orders table without downtime", (left, y), inter("SemiBold", 28), p.ink))
        y += 46
        self.lines.append(("Engineering notes  ·  6 min read", (left, y), inter("Regular", 14), p.muted))
        y += 44
        body = inter("Regular", 17)
        paragraphs = [
            "We split the work into small steps that could each be rolled back on their own. The hard part was not the copy itself but knowing when every reader had moved to the new table.",
            ARTICLE_SELECTION + " A one-page plan with a rollback step for each stage would have saved us two late nights.",
        ]
        for i, text in enumerate(paragraphs):
            y = self.paragraph(text, left, y, width, body, p.ink, len(ARTICLE_SELECTION) if i == 1 else 0) + 18
        y += 8
        self.lines.append(("What we would keep", (left, y), inter("SemiBold", 20), p.ink))
        y += 40
        self.paragraph(
            "Dual writes behind a flag, a nightly diff between the two tables, and a dashboard that showed how many reads each table served.",
            left, y, width, body, p.ink, 0,
        )

    def paragraph(self, text: str, left: float, y: float, width: float, font: ImageFont.FreeTypeFont, color: str, selected: int) -> float:
        words, line, start = text.split(" "), "", 0
        rows: list[tuple[str, int]] = []
        for word in words:
            candidate = f"{line} {word}" if line else word
            if font.getlength(candidate) / SCALE > width and line:
                rows.append((line, start))
                start += len(line) + 1
                line = word
            else:
                line = candidate
        rows.append((line, start))
        for row, offset in rows:
            self.lines.append((row, (left, y), font, color))
            end = min(len(row), selected - offset)
            if end > 0:
                x1 = left + font.getlength(row[:end]) / SCALE
                self.selection.append((left - 1, y - 3, x1 + 1, y + 25))
            y += 30
        return y

    def at(self, progress: float) -> Image.Image:
        if progress not in self.renders:
            self.renders[progress] = self.render(progress)
        return self.renders[progress]

    def render(self, progress: float) -> Image.Image:
        p = PALETTES[self.theme]
        image = Image.new("RGBA", (px(self.size[0]), px(self.size[1])), rgba(p.window))
        draw = ImageDraw.Draw(image)
        draw.rectangle((0, 0, image.size[0], px(44)), fill=rgba(p.bar))
        draw.line((0, px(44), image.size[0], px(44)), fill=rgba(p.rule), width=SCALE)
        for i, dot in enumerate(("#ff5f57", "#febc2e", "#28c840")):
            cx, cy = px(22 + i * 20), px(22)
            draw.ellipse((cx - px(6), cy - px(6), cx + px(6), cy + px(6)), fill=rgba(dot))
        title = inter("Medium", 13)
        label = "orders-migration.md"
        draw.text(((image.size[0] - title.getlength(label)) / 2, px(14)), label, font=title, fill=rgba(p.muted))
        total = sum(x1 - x0 for x0, _, x1, _ in self.selection)
        left = total * progress
        for x0, y0, x1, y1 in self.selection:
            shown = min(x1 - x0, left)
            left -= shown
            if shown > 0:
                draw.rectangle((px(x0), px(y0), px(x0 + shown), px(y1)), fill=rgba(p.selection))
        for text, (x, y), font, color in self.lines:
            draw.text((px(x), px(y)), text, font=font, fill=rgba(color))
        return image


KEY_LABELS = {"ctrl": "⌃", "control": "⌃", "alt": "⌥", "option": "⌥", "shift": "⇧", "cmd": "⌘", "space": "Space", "escape": "esc", "backspace": "⌫"}
SHOWN_KEYS = {"escape", "space", "backspace"}


def keycaps(script: Path) -> list[tuple[float, list[str]]]:
    shown = []
    for event in json.loads(script.read_text())["events"]:
        if event.get("type") == "shortcut":
            shown.append((event["t"], [KEY_LABELS.get(k, k.upper()) for k in event["chord"].split("+")]))
        elif event.get("type") == "key":
            name = event["name"].lower()
            mods = [KEY_LABELS[m] for m in ("ctrl", "alt", "shift", "cmd") if event.get(m)]
            if mods or name in SHOWN_KEYS:
                shown.append((event["t"], mods + [KEY_LABELS.get(name, name.upper())]))
    return shown


def draw_keycaps(canvas: Image.Image, caps: list[tuple[float, list[str]]], t: float, center: tuple[int, int], theme: str) -> None:
    p = PALETTES[theme]
    for start, keys in caps:
        age = t - start + 0.05
        if not 0 <= age <= 0.9:
            continue
        alpha = min(1.0, age / 0.08, (0.9 - age) / 0.15)
        font = inter("SemiBold", 17)
        widths = [max(px(40), round(font.getlength(k)) + px(24)) for k in keys]
        gap, height = px(8), px(40)
        total = sum(widths) + gap * (len(keys) - 1)
        layer = Image.new("RGBA", canvas.size, (0, 0, 0, 0))
        draw = ImageDraw.Draw(layer)
        x, y = center[0] - total // 2, center[1] - height // 2
        for key, w in zip(keys, widths):
            draw.rounded_rectangle((x, y + px(3), x + w, y + height + px(3)), px(9), fill=(0, 0, 0, 40))
            draw.rounded_rectangle((x, y, x + w, y + height), px(9), fill=rgba(p.cap), outline=rgba(p.cap_edge), width=SCALE)
            draw.text((x + w / 2, y + height / 2), key, font=font, fill=rgba(p.cap_ink), anchor="mm")
            x += w + gap
        faded = layer.copy()
        faded.putalpha(layer.getchannel("A").point(lambda a: round(a * alpha)))
        canvas.alpha_composite(faded)


def desktop_frame(theme: str, shot: Shot, panel: Image.Image, t: float, article: Article, caps: list) -> Image.Image:
    canvas = wallpaper(theme, (px(1280), px(800))).copy()
    progress = 0.0 if shot.sweep is None else min(1.0, max(0.0, (t - shot.sweep[0]) / (shot.sweep[1] - shot.sweep[0])))
    place(canvas, article.at(round(progress * 60) / 60), (px(56), px(88)), theme)
    place(canvas, panel, (px(1280 - 16 - shot.panel[0]), px((800 - shot.panel[1]) / 2)), theme)
    draw_keycaps(canvas, caps, t, (px(56 + 400), px(756)), theme)
    return canvas


def panel_frame(theme: str, shot: Shot, panel: Image.Image, t: float, caps: list) -> Image.Image:
    pad, foot = 40, 84
    canvas = wallpaper(theme, (px(shot.panel[0] + 2 * pad), px(shot.panel[1] + pad + foot))).copy()
    place(canvas, panel, (px(pad), px(pad)), theme)
    draw_keycaps(canvas, caps, t, (canvas.size[0] // 2, px(shot.panel[1] + pad + foot / 2)), theme)
    return canvas


def tour(theme: str, shot: Shot) -> Image.Image:
    gap = 48
    w, h = shot.panel
    canvas = wallpaper(theme, (px(3 * w + 4 * gap), px(h + 2 * gap))).copy()
    for i, script in enumerate(TOUR):
        place(canvas, still(theme, script, shot), (px(gap + i * (w + gap)), px(gap)), theme)
    return canvas


def encode_gif(pngs: Path, width: int, out: Path) -> None:
    graph = (
        f"[0:v]tpad=stop_mode=clone:stop_duration={HOLD},scale={width}:-1:flags=lanczos,split[a][b];"
        "[a]palettegen=max_colors=192:stats_mode=diff[p];[b][p]paletteuse=dither=bayer:bayer_scale=4:diff_mode=rectangle"
    )
    cmd = ["ffmpeg", "-loglevel", "error", "-y", "-framerate", str(FPS), "-i", str(pngs / "c_%04d.png"), "-filter_complex", graph, str(out)]
    subprocess.run(cmd, check=True)


def picture(theme: str, shot: Shot) -> Image.Image:
    match shot.layout:
        case "tour":
            return tour(theme, shot).convert("RGB")
        case "bare":
            return rounded(still(theme, shot.script, shot), px(WINDOW_RADIUS))
        case "desktop":
            return desktop_frame(theme, shot, still(theme, shot.script, shot), float("inf"), Article(theme), []).convert("RGB")
        case "panel":
            return panel_frame(theme, shot, still(theme, shot.script, shot), float("inf"), []).convert("RGB")


def animate(theme: str, shot: Shot, out: Path) -> None:
    caps = keycaps(SRC / "scripts" / shot.script)
    article = Article(theme)
    with tempfile.TemporaryDirectory() as tmp:
        tmp = Path(tmp)
        for i, frame in enumerate(frames(theme, shot, tmp)):
            panel, t = Image.open(frame).convert("RGBA"), i / FPS
            composed = desktop_frame(theme, shot, panel, t, article, caps) if shot.layout == "desktop" else panel_frame(theme, shot, panel, t, caps)
            composed.convert("RGB").save(tmp / f"c_{i:04d}.png")
        encode_gif(tmp, shot.width or 960, out)


def render(name: str, shot: Shot) -> list[Path]:
    written = []
    for theme in PALETTES:
        out = MEDIA / f"{name}-{theme}.{'gif' if shot.gif else 'png'}"
        if shot.gif:
            animate(theme, shot, out)
        else:
            picture(theme, shot).save(out, optimize=True)
        written.append(out)
    return written


def main() -> None:
    names = sys.argv[1:] or list(SHOTS)
    unknown = [n for n in names if n not in SHOTS]
    if unknown:
        sys.exit(f"render: unknown shots {unknown}; choose from {list(SHOTS)}")
    if not BIN.exists():
        sys.exit("render: build the app first with `cargo build`; only debug builds carry the headless tooling")
    for name in names:
        for out in render(name, SHOTS[name]):
            print(f"{out.relative_to(ROOT)}  {out.stat().st_size // 1024} KB")


if __name__ == "__main__":
    main()
