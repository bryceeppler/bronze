<img src="assets/icon.svg" width="64" height="64" alt="">

# bronze

A capture panel for AI-assisted work: part to-do list, part clipboard, part scratchpad.
Select text in any app, press a shortcut, and it lands in Bronze.
Queue the prompts you want to try next, copy them back out, and check them off.

Bronze is written in Rust on [Rune](https://github.com/anomalyco/rune) and is inspired by shadcn's Copper, which is Mac only.
It is an independent project and is not affiliated with shadcn.

Notes stay in one local file.
There is no account, no sync and no telemetry.

## Run

```sh
cargo run
```

Linux needs the packages Rune lists under "Linux" in its README.
`--data <dir>` keeps notes and preferences in a folder of your choice.

## Mac

```sh
./scripts/mac-app.sh
```

This builds Bronze and installs it as `~/Applications/Bronze.app`, signed with your Apple Development identity (or the one named in `BRONZE_SIGN`).
Open it, choose Open Settings in the notice, and turn Bronze on under Accessibility.
You grant this once; the notice goes away by itself.
Then double-tap Left Shift to capture the selected text, and press Right Shift with Left Shift to show or hide Bronze.

## Shortcuts

| Action | Shortcut |
| --- | --- |
| Capture the selected text, from any app | `Ctrl+Alt+C`, or double-tap Left Shift on the Mac |
| Show or hide Bronze, from any app | `Ctrl+Alt+Space`, or Right Shift + Left Shift on the Mac |
| Create or open a section | type `# Name` |
| Go to a section | `Ctrl/Cmd+K` |
| Copy the selected notes | `Ctrl/Cmd+C` |
| Merge the selected notes | `Ctrl/Cmd+J` |
| Check or uncheck | `Space` |
| Delete the selected notes | `Backspace` |
| Clear the selection, then hide | `Esc` |

On Linux, capture reads the text you have selected.
On macOS it asks the focused app for its selection, and when the app does not say, presses Copy for you and puts your clipboard back afterwards.
Without Accessibility access, macOS capture reads the clipboard, so copy first.
On Windows it reads the clipboard for now, so copy first.

## Check

```sh
./scripts/check.sh        # format, lint, tests, every scene rendered headless, scripted runs
./scripts/real-window.sh  # a real window under Xvfb: capture, show and hide, remembered position
```

Renders and element trees land in `out/`.
