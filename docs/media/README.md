# README media, set B

These images and GIFs show the real native Bronze application, not an HTML reconstruction or an AI-generated screenshot.
GPT-6.1 Sol on high authored this independent set.

## Capture provenance

The app was built locally on odin with `cargo build --locked` using the repository's shared target cache.
The source was `3ad610a844c7c384b5c112e5006521c2cd63e1a4`, the current `main` when captured on November 18, 2026.
The app ran in an isolated Xvfb display at 1440 by 1000 with a 400 by 700 native window.
The Vulkan software renderer was used.
These are Linux development-host screenshots, not captures of macOS or Windows chrome.

The app launched with an explicit temporary `--data` directory populated from `fixture-notes.json`.
Every note is invented for this asset set.
No default notes directory, Bryce's real notes, Nokto vault, Mac container data, or backups were used.

The four raw PNGs preserve the app pixels.
The README heroes place `overview-linux.png` on a branded background with an external shadow and rounded perimeter.
They do not redraw the app or add controls.

## Recordings

| File | Action | Duration | Dimensions |
| --- | --- | --- | --- |
| `quick-capture.gif` | Ctrl+Alt+C captures a fixture PRIMARY selection into Field notes | 3.5 seconds | 400 × 700 |
| `search.gif` | Type sensor into the real search input | 4 seconds | 400 × 700 |
| `sections.gif` | Ctrl+K, type Weekend, then Enter | 5.5 seconds | 400 × 700 |

The selected-text capture uses the repository's `scripts/own-primary.py` as its selection source.
The source process is outside the recorded crop.
The capture script checks that the actual notes file contains the captured fixture text.
All GIFs use 10 frames per second with a 128-color palette.
Reduced-motion visitors to the preview page see still images.
The README uses standard GIF embeds.

## Reproduce

Build the app with the repository's normal local build command.
Run `python docs/media/capture.py --binary /absolute/path/to/bronze` on a Linux host with Xvfb, xdotool, xdpyinfo, FFmpeg, and Python Tk installed.
The script makes a temporary data directory and an isolated display, then cleans up its app, display, and selection owner.
It overwrites only this set's raw PNGs and GIFs.

Run `python docs/media/build-hero.py` with the brand Python dependencies and Lato Regular installed to regenerate the two hero PNGs.
The preview page is `preview.html`.
It uses only local assets.

## Re-take after N-226

Record a real bronze-sync transfer between two separately launched fixture clients after sync lands.
Use a disposable sync server and disposable sync key.
Keep the key and any server credentials out of captures and logs.
Do not substitute a simulated sync animation.

Re-take the overview and rebuild the heroes if N-226 changes app chrome or settings.
Re-take search, section navigation, or global capture only if their visible UI changes.
A future Mac or Windows capture should replace the development-host shots before describing the images as platform-specific.

The proposed README defers its MIT badge until the actual license file lands.
It makes no sync availability claim.

## Verification and preview hosting

`validation.json` records the decoded asset dimensions, GIF frames, durations, and sizes.
`verification.txt` records final macOS decoding, HTTP checks from loki, text-palette contrast ratios, and preview service status.
The optional design detector used a regex fallback, so it was supplemented with explicit contrast calculations.

An independent Opus 5.5 technical review found no blockers.
Its minor findings led to a Linux-inclusive platform badge, larger hero captions, pointer-free still captures, local relative preview paths, a persistent preview service, and recorded contrast evidence.
The creative artwork remains independently authored by GPT-6.1 Sol on high.

The requested preview is staged outside the repository at `~/.local/share/bronze-set-b-preview` on odin.
The enabled user service is `bronze-set-b-preview.service`, with an installed unit in `~/.config/systemd/user`.
The user manager has lingering enabled, and the service restarts on failure.
It serves only the staged static assets and accepts no uploads or application mutations.
The page is available at `http://odin:8798/` on the tailnet.
