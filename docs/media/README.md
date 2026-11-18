# README media

Every screenshot and GIF here is rendered by the real Bronze binary, headless, from a scripted run over invented fixture notes.
Nothing comes from anyone's real notes.

```sh
cargo build
python3 docs/media/src/render.py            # every shot, light and dark
python3 docs/media/src/render.py capture    # one shot
```

Only debug builds carry Rune's headless tooling, so the script runs `target/debug/bronze`.
It needs Python with Pillow, and `ffmpeg` for the GIFs.

| Shot | Files | Script |
| --- | --- | --- |
| `hero` | `hero-{light,dark}.png` | `src/scripts/capture.json` at its end |
| `capture` | `capture-{light,dark}.gif` | `src/scripts/capture.json` |
| `queue` | `queue-{light,dark}.gif` | `src/scripts/queue.json` |
| `search` | `search-{light,dark}.gif` | `src/scripts/search.json` |
| `tour` | `tour-{light,dark}.png` | `rest`, `search-still` and `switcher-still` |
| `panel` | `panel-{light,dark}.png` | `src/scripts/rest.json`, used by the social preview |

`src/notes.json` is the fixture every run starts from.
The document behind the panel in `hero` and `capture` is drawn by `render.py`, and its highlighted sentence is the text the capture receives.
Keycaps are drawn from the script's shortcuts, so they always match what the run pressed.

## Re-takes

- A `sync` GIF of two machines sharing one vault, once the sync work merges.
- `hero`, `tour` and `panel` again if the sync work adds anything to the panel, such as a sync status in the overflow menu.
- `social-preview.png` in `assets/brand` after `panel` changes, with `python3 assets/brand/build.py`.
