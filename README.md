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

Linux needs the packages listed under [Linux](#linux).
`--data <dir>` keeps notes and preferences in a folder of your choice.

## Mac

```sh
./scripts/mac-app.sh
```

This builds Bronze and installs it as `~/Applications/Bronze.app`, signed with your Apple Development identity (or the one named in `BRONZE_SIGN`).
Open it, choose Open Settings in the notice, and turn Bronze on under Accessibility.
You grant this once; the notice goes away by itself.
Then double-tap Left Shift to capture the selected text, and press Right Shift with Left Shift to show or hide Bronze.

## Windows

```powershell
cargo build --release
.\target\release\bronze.exe
```

Bronze needs no permission on Windows.
Double-tap Left Shift to capture the selected text, and press Right Shift with Left Shift to show or hide Bronze.
Both Shift gestures are ignored while a full-screen app such as a game is in front, and `Ctrl+Alt+C` and `Ctrl+Alt+Space` still work there.
While an app running as administrator has focus, Windows keeps its keys and its text from Bronze, so the gestures do nothing there and capture finds nothing.
The release build has no console window; run `.\target\release\bronze.exe 2> bronze.log` to keep its log.

## Linux

```sh
sudo apt install build-essential pkg-config libfontconfig-dev libxkbcommon-dev
./scripts/linux-app.sh
```

Those are the build packages on Ubuntu and Debian.
Bronze draws with Vulkan, so it also needs a Vulkan driver, such as `mesa-vulkan-drivers`.
The script builds Bronze and installs it for you alone: the binary in `~/.local/bin`, its icon, and an entry that puts Bronze in your app launcher.
Run it again to update.
Pass `--autostart` to also start Bronze at login, and `--uninstall` to remove what it installed.
Uninstalling keeps your notes in `~/.local/share/bronze`.

On X11, everything works except the Shift gestures, which Linux does not have yet, so use `Ctrl+Alt+C` and `Ctrl+Alt+Space`.
On Wayland, Bronze starts, and the rest is untested.
Its global shortcuts need the desktop's global shortcuts portal, and capture needs a compositor that shares the selected text with other apps.

## Shortcuts

| Action | Shortcut |
| --- | --- |
| Capture the selected text, from any app | `Ctrl+Alt+C`, or double-tap Left Shift on the Mac and Windows |
| Show or hide Bronze, from any app | `Ctrl+Alt+Space`, or Right Shift + Left Shift on the Mac and Windows |
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
On Windows it asks the focused app for its selection through UI Automation, and when the app does not say, presses Ctrl+Insert for you and puts your clipboard back afterwards.
Ctrl+Insert copies like Ctrl+C in standard Windows controls, browsers, Electron apps and terminals, but with nothing selected it never interrupts a program running in a terminal.

## Check

```sh
./scripts/check.sh        # format, lint, tests, every scene rendered headless, scripted runs
./scripts/real-window.sh  # a real window under Xvfb: capture, show and hide, remembered position
./scripts/linux-app-check.sh  # install into a temp home, launch from the launcher entry under Xvfb, capture, uninstall
```

On Windows, `scripts\real-window.ps1` drives real windows with injected keys: both capture paths, the chord held like a person holds it, show and hide, the timelines that must not capture, and the gestures over a full-screen app.
It sends real key presses to the desktop, so it refuses to start while the session is locked or was used in the last 3 minutes unless you pass `-Force`.

Renders and element trees land in `out/`.
