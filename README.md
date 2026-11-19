<img src="assets/icon.svg" width="64" height="64" alt="">

# bronze

A capture panel for AI-assisted work: part to-do list, part clipboard, part scratchpad.
Select text in any app, press a shortcut, and it lands in Bronze.
Queue the prompts you want to try next, copy them back out, and check them off.

Bronze is written in Rust on [Rune](https://github.com/anomalyco/rune) and is inspired by shadcn's Copper, which is Mac only.
It is an independent project and is not affiliated with shadcn.

Notes stay in one local file on your machine.
To have them on your other machines too, Bronze can sync them through a server you run yourself, end-to-end encrypted.
There is no account and no telemetry.

## Run

```sh
cargo run
```

Linux needs the packages listed under [Linux](#linux).
Type in the box at the bottom and press Enter to add a note.
`--data <dir>` keeps notes and preferences in a folder of your choice, as in `cargo run -- --data /path/to/folder`.

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

## Sync

Bronze syncs through `bronze-sync`, a small server you host, so the same notes show on your Mac, Windows and Linux machines.
The server runs on Linux or macOS, from a binary or a Docker image; [docs/self-hosting.md](docs/self-hosting.md) shows how.

To start, open the menu, choose **Sync…**, enter your server's address, and choose **Start New Vault**.
Bronze then shows your sync code: choose **Copy Code** and keep it somewhere safe.
On each other machine, choose **Sync…**, paste the code, and choose **Join Vault**.
Notes already on that machine are added, and nothing is lost.

Notes are encrypted on each machine before they leave it, and the server stores them without being able to read them.
The sync code is the only way in, so there is no account, and nobody can reset a lost code.
**Stop Syncing** in the same sheet turns sync off and keeps every note on that machine.
With sync off, Bronze opens no network connection.
[docs/sync.md](docs/sync.md) describes the encryption, the protocol and its limits.

## Check

```sh
./scripts/check.sh        # format, lint, tests, every scene rendered headless, scripted runs
./scripts/real-window.sh  # a real window under Xvfb: capture, show and hide, remembered position
./scripts/sync-check.sh   # two real windows and a real bronze-sync: setup, live edits, offline edits, a wiped server
./scripts/server-check.sh binary  # bronze-sync built and driven with curl; `docker <image>` runs the same cases on an image
./scripts/linux-app-check.sh  # install into a temp home, launch from the launcher entry under Xvfb, capture, uninstall
```

On Windows, `scripts\real-window.ps1` drives real windows with injected keys: both capture paths, the chord held like a person holds it, show and hide, the timelines that must not capture, and the gestures over a full-screen app.
It sends real key presses to the desktop, so it refuses to start while the session is locked or was used in the last 3 minutes unless you pass `-Force`.

Renders and element trees land in `out/`.

## License

MIT, in [LICENSE](LICENSE).
