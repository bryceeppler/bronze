# Bronze brand, set B

An independent proposal by GPT-6.1 Sol on high.
The existing app icon and product code are untouched.

The angular B uses two folded-note shapes and one solid spine.
Warm bronze pairs with paper and charcoal backgrounds.
The outlined lowercase wordmark keeps the result readable across renderers.

## Files

`logo-{light,dark}.svg` combines the mark and wordmark.
`wordmark-{light,dark}.svg` contains the lettering alone.
`mark-{light,dark}.svg` contains the transparent, one-color mark.
Light variants suit light backgrounds, and dark variants suit dark backgrounds.

`app-icon-{light,dark}.svg` is the source for the complete icon families.
The SVGs have transparent outer margins and a rounded tile.
`app-icon-{light,dark}-1024.png` is the large raster master.

Each `Bronze-{light,dark}.iconset` contains the standard Mac 16, 32, 128, 256, and 512 point files at 1x and 2x.
The matching ICNS file includes those raster sizes and the retina chunk identifiers.
Each `windows-{light,dark}` folder contains PNGs at 16, 20, 24, 32, 40, 48, 64, 96, 128, and 256 pixels.
The matching ICO embeds those same ten sizes as 32-bit images.

`social-preview-{light,dark}.png` is 1280 by 640 pixels, with matching SVG sources.
The four local badges make no network requests and do not claim a release or a license that this checkout lacks.

## Regenerate

Install Python dependencies in a temporary virtual environment.
Run `python assets/brand/generate.py` from any directory to rebuild the raster exports and platform containers from the committed SVG sources.
The required versions are in `requirements.txt`.

The wordmark lettering is outlined Lato Semibold by Łukasz Dziedzic.
Lato is licensed under SIL OFL 1.1, which does not require that license for documents made with the font.
No font software is bundled or needed to render the SVGs.
The mark and icon geometry are original vector artwork.
The hero builder uses the installed Lato Regular font for its supporting copy.

These files are candidate assets, not an installed application identity.
Choose the desired family before wiring it into the Mac bundle, Windows executable, or repository social settings.
