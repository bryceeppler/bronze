use std::path::Path;
use std::process::Command;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::thread;
use std::time::Instant;

use arboard::{GetExtLinux, LinuxClipboardKind};

use super::{Access, OnShortcut};
use crate::taps::{Detector, Gesture, Input, Side};

const KEY_LEFT_SHIFT: u16 = 42;
const KEY_RIGHT_SHIFT: u16 = 54;
const BTN_LEFT: u16 = 0x110;

static HEARS_KEYBOARD: AtomicBool = AtomicBool::new(false);

fn wayland() -> bool {
    ["WAYLAND_DISPLAY", "WAYLAND_SOCKET"]
        .into_iter()
        .any(|name| std::env::var_os(name).is_some_and(|value| !value.is_empty()))
}

pub fn bind_shortcuts(on: OnShortcut) -> bool {
    let wayland = wayland();
    if wayland {
        super::portal::bind(on);
    }
    wayland
}

pub fn listen(on_gesture: impl Fn(Gesture) + Send + 'static) {
    let (send, inputs) = mpsc::channel();
    let heard = if wayland() { devices::listen(send) } else { x11::listen(send) };
    HEARS_KEYBOARD.store(heard, Ordering::Relaxed);
    announce(heard);
    thread::Builder::new()
        .name("bronze-shift-gestures".into())
        .spawn(move || {
            let (mut detector, started) = (Detector::default(), Instant::now());
            for input in inputs {
                if let Some(gesture) = detector.feed(input, started.elapsed()) {
                    on_gesture(gesture);
                }
            }
        })
        .expect("spawn the Shift gesture detector");
}

fn hears_keyboard(heard: bool) {
    if HEARS_KEYBOARD.swap(heard, Ordering::Relaxed) != heard {
        announce(heard);
    }
}

fn announce(heard: bool) {
    if heard {
        eprintln!("bronze: listening for Shift gestures");
    } else {
        eprintln!("bronze: cannot read the keyboard, so Shift gestures are off");
    }
}

pub fn access() -> Access {
    if HEARS_KEYBOARD.load(Ordering::Relaxed) { Access::Granted } else { Access::NoKeyboard }
}

pub fn ask_for_access() {}

pub fn selected_text() -> Option<String> {
    arboard::Clipboard::new().ok()?.get().clipboard(LinuxClipboardKind::Primary).text().ok()
}

pub fn reveal_command(path: &Path) -> Command {
    let mut command = Command::new("xdg-open");
    command.arg(path.parent().unwrap_or(path));
    command
}

fn input(code: u16, pressed: bool) -> Option<Input> {
    match code {
        KEY_LEFT_SHIFT => Some(Input::Shift { side: Side::Left, pressed }),
        KEY_RIGHT_SHIFT => Some(Input::Shift { side: Side::Right, pressed }),
        _ => pressed.then_some(Input::Other),
    }
}

mod x11 {
    use std::sync::mpsc::Sender;
    use std::thread;

    use x11rb::connection::Connection;
    use x11rb::protocol::Event;
    use x11rb::protocol::xinput::{ConnectionExt as _, Device, EventMask, XIEventMask};
    use x11rb::rust_connection::RustConnection;

    use super::input;
    use crate::taps::Input;

    const EVDEV_TO_X11_KEYCODE: u32 = 8;

    pub fn listen(send: Sender<Input>) -> bool {
        match select_raw_input() {
            Ok(connection) => {
                thread::Builder::new()
                    .name("bronze-x11-keys".into())
                    .spawn(move || hear(&connection, &send))
                    .expect("spawn the X11 key listener");
                true
            }
            Err(error) => {
                eprintln!("bronze: cannot listen to X11 input: {error}");
                false
            }
        }
    }

    fn select_raw_input() -> Result<RustConnection, Box<dyn std::error::Error>> {
        let (connection, screen) = x11rb::connect(None)?;
        connection.xinput_xi_query_version(2, 0)?.reply()?;
        let root = connection.setup().roots[screen].root;
        let mask = XIEventMask::RAW_KEY_PRESS | XIEventMask::RAW_KEY_RELEASE | XIEventMask::RAW_BUTTON_PRESS;
        let interest = EventMask { deviceid: Device::ALL_MASTER.into(), mask: vec![mask] };
        connection.xinput_xi_select_events(root, &[interest])?.check()?;
        Ok(connection)
    }

    fn hear(connection: &RustConnection, send: &Sender<Input>) {
        while let Ok(event) = connection.wait_for_event() {
            let heard = match event {
                Event::XinputRawKeyPress(key) => keycode(key.detail).and_then(|code| input(code, true)),
                Event::XinputRawKeyRelease(key) => keycode(key.detail).and_then(|code| input(code, false)),
                Event::XinputRawButtonPress(_) => Some(Input::Other),
                _ => None,
            };
            if heard.is_some_and(|input| send.send(input).is_err()) {
                return;
            }
        }
        super::hears_keyboard(false);
    }

    fn keycode(detail: u32) -> Option<u16> {
        detail.checked_sub(EVDEV_TO_X11_KEYCODE).and_then(|code| code.try_into().ok())
    }
}

mod devices {
    use std::collections::HashMap;
    use std::fs::{self, File};
    use std::io::Read;
    use std::path::PathBuf;
    use std::sync::mpsc::Sender;
    use std::sync::{Arc, Mutex};
    use std::thread;
    use std::time::Duration;

    use super::{BTN_LEFT, KEY_LEFT_SHIFT, input};
    use crate::taps::Input;

    const RESCAN: Duration = Duration::from_secs(2);
    const EV_KEY: u16 = 1;
    pub(super) const TIME_SIZE: usize = 2 * size_of::<usize>();
    pub(super) const EVENT_SIZE: usize = TIME_SIZE + 8;

    #[derive(Clone, Copy, PartialEq, Eq)]
    enum Kind {
        Keyboard,
        Pointer,
    }

    type Open = Arc<Mutex<HashMap<PathBuf, Kind>>>;

    pub fn listen(send: Sender<Input>) -> bool {
        let open = Open::default();
        let heard = scan(&open, &send);
        thread::Builder::new()
            .name("bronze-input-devices".into())
            .spawn(move || {
                loop {
                    thread::sleep(RESCAN);
                    super::hears_keyboard(scan(&open, &send));
                }
            })
            .expect("spawn the input device scanner");
        heard
    }

    fn scan(open: &Open, send: &Sender<Input>) -> bool {
        let nodes = fs::read_dir("/dev/input").into_iter().flatten().flatten();
        for node in nodes.filter(|node| node.file_name().to_string_lossy().starts_with("event")) {
            let path = node.path();
            if open.lock().expect("the open devices").contains_key(&path) {
                continue;
            }
            let Some(kind) = kind(&node.file_name().to_string_lossy()) else { continue };
            let Ok(device) = File::open(&path) else { continue };
            open.lock().expect("the open devices").insert(path.clone(), kind);
            let (open, send) = (open.clone(), send.clone());
            thread::Builder::new()
                .name("bronze-input-device".into())
                .spawn(move || {
                    read(device, &send);
                    open.lock().expect("the open devices").remove(&path);
                })
                .expect("spawn an input device reader");
        }
        open.lock().expect("the open devices").values().any(|kind| *kind == Kind::Keyboard)
    }

    fn kind(node: &str) -> Option<Kind> {
        let keys = fs::read_to_string(format!("/sys/class/input/{node}/device/capabilities/key")).ok()?;
        match (has_key(&keys, KEY_LEFT_SHIFT), has_key(&keys, BTN_LEFT)) {
            (true, _) => Some(Kind::Keyboard),
            (false, true) => Some(Kind::Pointer),
            (false, false) => None,
        }
    }

    pub(super) fn has_key(bitmap: &str, code: u16) -> bool {
        let (word, bit) = (usize::from(code) / usize::BITS as usize, u32::from(code) % usize::BITS);
        bitmap
            .split_whitespace()
            .rev()
            .nth(word)
            .and_then(|word| usize::from_str_radix(word, 16).ok())
            .is_some_and(|word| word >> bit & 1 == 1)
    }

    fn read(mut device: File, send: &Sender<Input>) {
        let mut events = [0; EVENT_SIZE * 64];
        while let Ok(length @ 1..) = device.read(&mut events) {
            for event in events[..length].as_chunks::<EVENT_SIZE>().0 {
                if let Some(heard) = key_input(event)
                    && send.send(heard).is_err()
                {
                    return;
                }
            }
        }
    }

    pub(super) fn key_input(event: &[u8; EVENT_SIZE]) -> Option<Input> {
        let [k0, k1, c0, c1, v0, v1, v2, v3] = *event[TIME_SIZE..].as_array()?;
        let (kind, code, value) =
            (u16::from_ne_bytes([k0, k1]), u16::from_ne_bytes([c0, c1]), i32::from_ne_bytes([v0, v1, v2, v3]));
        match (kind, value) {
            (EV_KEY, 0 | 1) => input(code, value == 1),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::devices::{EVENT_SIZE, TIME_SIZE, has_key, key_input};
    use crate::taps::{Input, Side};

    fn event(kind: u16, code: u16, value: i32) -> [u8; EVENT_SIZE] {
        let mut event = [0; EVENT_SIZE];
        event[TIME_SIZE..]
            .copy_from_slice(&[&kind.to_ne_bytes()[..], &code.to_ne_bytes(), &value.to_ne_bytes()].concat());
        event
    }

    #[test]
    fn key_events_read_shift_sides_and_ignore_repeats_and_other_releases() {
        assert_eq!(key_input(&event(1, 42, 1)), Some(Input::Shift { side: Side::Left, pressed: true }));
        assert_eq!(key_input(&event(1, 42, 0)), Some(Input::Shift { side: Side::Left, pressed: false }));
        assert_eq!(key_input(&event(1, 54, 1)), Some(Input::Shift { side: Side::Right, pressed: true }));
        assert_eq!(key_input(&event(1, 42, 2)), None);
        assert_eq!(key_input(&event(1, 30, 1)), Some(Input::Other));
        assert_eq!(key_input(&event(1, 0x110, 1)), Some(Input::Other));
        assert_eq!(key_input(&event(1, 30, 0)), None);
        assert_eq!(key_input(&event(4, 4, 458_977)), None);
    }

    #[test]
    fn a_capability_bitmap_names_its_keys_from_the_last_word() {
        let keyboard = match usize::BITS {
            64 => "1000000000007 ff9f207ac14057ff febeffdfffefffff fffffffffffffffe",
            _ => "10000 7 ff9f207a c14057ff febeffdf ffefffff ffffffff fffffffe",
        };
        assert!(has_key(keyboard, 42));
        assert!(has_key(keyboard, 54));
        assert!(!has_key(keyboard, 0x110));
        let mouse = match usize::BITS {
            64 => "1f0000 0 0 0 0",
            _ => "1f0000 0 0 0 0 0 0 0 0",
        };
        assert!(has_key(mouse, 0x110));
        assert!(!has_key(mouse, 42));
        assert!(!has_key("", 42));
    }
}
