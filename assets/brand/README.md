# Bronze brand

The mark is a capital B built from two stacked note cards, and the slot in each card is a line of text.
The wordmark is drawn by hand as monoline geometric lowercase, so it needs no font.

| File | Use |
| --- | --- |
| `logo-light.svg`, `logo-dark.svg` | Mark and wordmark, for light and dark backgrounds |
| `mark-light.svg`, `mark-dark.svg` | The mark alone |
| `wordmark-light.svg`, `wordmark-dark.svg` | The wordmark alone |
| `icon/icon-{light,dark}.svg` | App icon artwork, full bleed, 824 units square |
| `icon/icon-{light,dark}-{16,32}.svg` | Pixel-fitted icons for 16, 24 and 32 pixels |
| `icon/bronze-{light,dark}.icns` | macOS icon, every slot from 16 to 1024 |
| `icon/bronze-{light,dark}.ico` | Windows icon, 16 to 256 |
| `social-preview.svg`, `social-preview.png` | GitHub social preview, 1280 by 640 |

`-light` means for light backgrounds and `-dark` for dark ones, which matches GitHub's `prefers-color-scheme` sources.
The dark icon is the default app icon.

```sh
python3 assets/brand/build.py   # every PNG, .icns and .ico from the SVGs
```

It needs `resvg` (`cargo install resvg`) and Python with Pillow.
The macOS icons sit on Apple's 1024 grid with an 824 body and its drop shadow, and the Windows icons fill 15/16 of the canvas.

## Colors

| Name | Hex | Where |
| --- | --- | --- |
| Bronze | `#cd7f32` | The app's accent in dark mode |
| Deep bronze | `#a3612a` | The app's accent in light mode |
| Metal, for dark backgrounds | `#f4c590` to `#d78c4b` to `#9a5526` | The mark |
| Metal, for light backgrounds | `#e09d5e` to `#b8692f` to `#7a3f18` | The mark |
| Ink | `#1c1c1e` | Wordmark on light, and the app's dark background |
| Paper | `#ececee` | Wordmark on dark, and the app's dark-mode text |
