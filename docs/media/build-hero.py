from io import BytesIO
from pathlib import Path

import cairosvg
from PIL import Image, ImageDraw, ImageFilter, ImageFont, PngImagePlugin

ROOT = Path(__file__).resolve().parents[2]
MEDIA = ROOT / "docs/media"
BRAND = ROOT / "assets/brand"

for theme, background, ink in [
    ("light", "#f5efe6", "#29251f"),
    ("dark", "#20211f", "#f5efe6"),
]:
    canvas = Image.new("RGBA", (1600, 1000), background)
    logo = Image.open(
        BytesIO(
            cairosvg.svg2png(url=str(BRAND / f"logo-{theme}.svg"), output_width=470)
        )
    )
    canvas.alpha_composite(logo, (100, 190))
    font = ImageFont.truetype("/usr/share/fonts/truetype/lato/Lato-Regular.ttf", 55)
    label = ImageFont.truetype("/usr/share/fonts/truetype/lato/Lato-Regular.ttf", 42)
    draw = ImageDraw.Draw(canvas)
    draw.text(
        (104, 375),
        "Keep the thought.\nKeep your flow.",
        font=font,
        fill=ink,
        spacing=12,
    )
    draw.text(
        (107, 570),
        "A small capture panel\nfor notes and prompts.",
        font=label,
        fill=ink,
        spacing=9,
    )
    draw.text(
        (107, 810),
        "Rust + Rune",
        font=label,
        fill=("#9d542c" if theme == "light" else "#e6af7f"),
    )
    shot = Image.open(MEDIA / "overview-linux.png").convert("RGBA")
    shot = shot.resize((480, 840), Image.Resampling.LANCZOS)
    mask = Image.new("L", shot.size)
    ImageDraw.Draw(mask).rounded_rectangle((0, 0, 479, 839), radius=20, fill=255)
    shot.putalpha(mask)
    shadow = Image.new("RGBA", canvas.size)
    ImageDraw.Draw(shadow).rounded_rectangle(
        (956, 93, 1436, 933), radius=20, fill=(0, 0, 0, 65)
    )
    canvas = Image.alpha_composite(canvas, shadow.filter(ImageFilter.GaussianBlur(28)))
    canvas.alpha_composite(shot, (948, 80))
    metadata = PngImagePlugin.PngInfo()
    metadata.add_text(
        "Source",
        "Original SVG brand sources plus overview-linux.png, a real native Bronze window with fixture notes. Only the window perimeter is rounded for presentation.",
    )
    canvas.convert("RGB").save(
        MEDIA / f"hero-{theme}.png", pnginfo=metadata, optimize=True
    )
