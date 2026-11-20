# Changelog

Every release of Bronze, newest first.
Versions follow [Semantic Versioning](https://semver.org).

## 0.1.2 - 2026-11-20

### Changed

- The macOS download is for Apple silicon only, as `Bronze-0.1.2-macos-arm64.zip`.
  It no longer carries the Intel code that made macOS 27 warn that Bronze will not open in macOS 28.

## 0.1.1 - 2026-11-20

### Added

- Linux: the Shift gestures work on X11 and on Wayland.
  Double-tap Left Shift to capture, and press Right Shift with Left Shift to show or hide Bronze.
  On Wayland they need Bronze to read the keyboard, which takes the `input` group; without it Bronze says the gestures are off and points to Keyboard Shortcuts.
- Linux: on Wayland, `Ctrl+Alt+C` and `Ctrl+Alt+Space` go through the desktop's global shortcuts portal, which asks you once.
- Linux: `bronze bronze://capture` and `bronze bronze://show-or-hide` drive the running app, so any desktop shortcut can be bound to them.
- The macOS app and the Windows program carry the Bronze ingot icon.
- The Linux download installs itself: run `./linux-app.sh` in the unpacked folder for the launcher entry, the icons and an optional start at login.
- The macOS download runs on Intel Macs as well as Apple silicon.

### Changed

- Linux: the window's app id is now `ca.bryce.bronze`.
  If you installed 0.1.0 with `scripts/linux-app.sh`, run it again so the launcher entry matches.
- Linux: the download runs on glibc 2.35 or later, such as Ubuntu 22.04 and Debian 12, instead of 2.43.
- Release binaries no longer contain the paths of the machine that built them.

### Fixed

- A relative `--data` folder is made absolute, so bound shortcuts find it from any folder.

## 0.1.0 - 2026-11-18

The first release.

- Capture selected text from any app with a shortcut, queue it in sections, copy it back out and check it off.
- Notes stay in one local file, with no account and no telemetry.
- Optional sync through `bronze-sync`, a server you host yourself, end-to-end encrypted.
- macOS, Windows and Linux.
