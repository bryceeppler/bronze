use std::cell::RefCell;
use std::ffi::c_void;
use std::path::Path;
use std::process::Command;
use std::ptr::{self, NonNull};
use std::sync::Mutex;
use std::thread;
use std::time::{Duration, Instant};

use objc2::rc::Retained;
use objc2::runtime::ProtocolObject;
use objc2_app_kit::{NSPasteboard, NSPasteboardItem, NSPasteboardTypeString, NSPasteboardWriting};
use objc2_core_foundation::{
    CFBoolean, CFDictionary, CFMachPort, CFRetained, CFRunLoop, CFString, CFType, Type, kCFRunLoopDefaultMode,
};
use objc2_core_graphics::{
    CGEvent, CGEventField, CGEventFlags, CGEventTapLocation, CGEventTapOptions, CGEventTapPlacement, CGEventTapProxy,
    CGEventType,
};
use objc2_foundation::{NSArray, NSData, NSString};

use super::Access;
use crate::taps::{Detector, Gesture, Input, Side};

const AX_TIMEOUT_SECONDS: f32 = 0.3;
const COPY_WAIT: Duration = Duration::from_millis(500);
const COPY_POLL: Duration = Duration::from_millis(10);
const TAP_RETRY: Duration = Duration::from_secs(2);
const ACCESSIBILITY_SETTINGS: &str = "x-apple.systempreferences:com.apple.preference.security?Privacy_Accessibility";

const KEY_C: u16 = 8;
const KEY_LEFT_SHIFT: i64 = 56;
const KEY_RIGHT_SHIFT: i64 = 60;
// IOKit's device-dependent modifier bits (NX_DEVICELSHIFTKEYMASK and NX_DEVICERSHIFTKEYMASK), which
// Core Graphics passes through in the event flags without naming them.
const LEFT_SHIFT_DOWN: u64 = 0x02;
const RIGHT_SHIFT_DOWN: u64 = 0x04;

#[link(name = "ApplicationServices", kind = "framework")]
unsafe extern "C" {
    static kAXTrustedCheckOptionPrompt: &'static CFString;
    fn AXIsProcessTrusted() -> u8;
    fn AXIsProcessTrustedWithOptions(options: &CFDictionary<CFString, CFBoolean>) -> u8;
    fn AXUIElementCreateSystemWide() -> NonNull<CFType>;
    fn AXUIElementSetMessagingTimeout(element: &CFType, seconds: f32) -> i32;
    fn AXUIElementCopyAttributeValue(element: &CFType, attribute: &CFString, value: *mut *mut CFType) -> i32;
}

pub fn access() -> Access {
    match unsafe { AXIsProcessTrusted() } {
        0 => Access::Missing,
        _ => Access::Granted,
    }
}

pub fn ask_for_access() {
    let prompt = unsafe { kAXTrustedCheckOptionPrompt };
    let options = CFDictionary::from_slices(&[prompt], &[CFBoolean::new(true)]);
    unsafe { AXIsProcessTrustedWithOptions(&options) };
    if let Err(error) = Command::new("open").arg(ACCESSIBILITY_SETTINGS).spawn() {
        eprintln!("bronze: cannot open the Accessibility settings: {error}");
    }
}

pub fn reveal_command(path: &Path) -> Command {
    let mut command = Command::new("open");
    command.arg("-R").arg(path);
    command
}

#[derive(Debug)]
enum Source {
    Clipboard,
    FocusedApp,
    PressedCopy,
}

pub fn selected_text() -> Option<String> {
    let (source, text) = match access() {
        Access::Missing => (Source::Clipboard, pasteboard_text(&NSPasteboard::generalPasteboard())),
        Access::Granted => match focused_selection() {
            Some(text) => (Source::FocusedApp, Some(text)),
            None => (Source::PressedCopy, copied_selection()),
        },
    };
    let count = text.as_deref().map_or(0, |text| text.chars().count());
    eprintln!("bronze: captured {count} characters via {source:?}");
    text
}

fn focused_selection() -> Option<String> {
    let system = unsafe { CFRetained::from_raw(AXUIElementCreateSystemWide()) };
    unsafe { AXUIElementSetMessagingTimeout(&system, AX_TIMEOUT_SECONDS) };
    let focused = attribute(&system, "AXFocusedUIElement")?;
    let text = attribute(&focused, "AXSelectedText")?.downcast::<CFString>().ok()?.to_string();
    (!text.is_empty()).then_some(text)
}

fn attribute(element: &CFType, name: &str) -> Option<CFRetained<CFType>> {
    let mut value = ptr::null_mut();
    let error = unsafe { AXUIElementCopyAttributeValue(element, &CFString::from_str(name), &mut value) };
    let value = NonNull::new(value)?;
    let value = unsafe { CFRetained::from_raw(value) };
    (error == 0).then_some(value)
}

static PASTEBOARD: Mutex<()> = Mutex::new(());

fn copied_selection() -> Option<String> {
    let _only_copy = PASTEBOARD.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    let board = NSPasteboard::generalPasteboard();
    let saved = saved_items(&board);
    let before = board.changeCount();
    press_copy();
    let deadline = Instant::now() + COPY_WAIT;
    let text = loop {
        let text = (board.changeCount() != before).then(|| pasteboard_text(&board)).flatten();
        if text.is_some() || Instant::now() >= deadline {
            break text;
        }
        thread::sleep(COPY_POLL);
    };
    if board.changeCount() != before {
        restore(&board, &saved);
    }
    text
}

fn pasteboard_text(board: &NSPasteboard) -> Option<String> {
    board.stringForType(unsafe { NSPasteboardTypeString }).map(|text| text.to_string())
}

type SavedItem = Vec<(Retained<NSString>, Retained<NSData>)>;

fn saved_items(board: &NSPasteboard) -> Vec<SavedItem> {
    let Some(items) = board.pasteboardItems() else { return Vec::new() };
    items
        .iter()
        .map(|item| item.types().iter().filter_map(|kind| item.dataForType(&kind).map(|data| (kind, data))).collect())
        .collect()
}

fn restore(board: &NSPasteboard, saved: &[SavedItem]) {
    board.clearContents();
    if saved.is_empty() {
        return;
    }
    let items: Vec<Retained<ProtocolObject<dyn NSPasteboardWriting>>> = saved
        .iter()
        .map(|types| {
            let item = NSPasteboardItem::new();
            for (kind, data) in types {
                item.setData_forType(data, kind);
            }
            ProtocolObject::from_retained(item)
        })
        .collect();
    board.writeObjects(&NSArray::from_retained_slice(&items));
}

fn press_copy() {
    for down in [true, false] {
        let event = CGEvent::new_keyboard_event(None, KEY_C, down);
        CGEvent::set_flags(event.as_deref(), CGEventFlags::MaskCommand);
        CGEvent::post(CGEventTapLocation::HIDEventTap, event.as_deref());
    }
}

pub fn listen(on_gesture: impl Fn(Gesture) + Send + 'static) {
    thread::Builder::new()
        .name("bronze-shift-listener".into())
        .spawn(move || {
            Listener { detector: RefCell::default(), on_gesture, started: Instant::now(), tap: RefCell::default() }
                .run()
        })
        .expect("spawn the Shift listener");
}

struct Listener<F> {
    detector: RefCell<Detector>,
    on_gesture: F,
    started: Instant,
    tap: RefCell<Option<CFRetained<CFMachPort>>>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Hearing {
    Listening,
    Waiting,
}

impl<F: Fn(Gesture)> Listener<F> {
    fn run(&self) {
        let mut last = None;
        loop {
            match self.open() {
                Some(tap) => {
                    Hearing::Listening.announce(&mut last);
                    self.hear_until_it_dies(&tap);
                }
                None => {
                    Hearing::Waiting.announce(&mut last);
                    thread::sleep(TAP_RETRY);
                }
            }
        }
    }

    fn open(&self) -> Option<CFRetained<CFMachPort>> {
        if access() == Access::Missing {
            return None;
        }
        let interest = [CGEventType::FlagsChanged, CGEventType::KeyDown]
            .into_iter()
            .chain([CGEventType::LeftMouseDown, CGEventType::RightMouseDown, CGEventType::OtherMouseDown])
            .fold(0, |mask, kind| mask | 1 << kind.0);
        let tap = unsafe {
            CGEvent::tap_create(
                CGEventTapLocation::SessionEventTap,
                CGEventTapPlacement::HeadInsertEventTap,
                CGEventTapOptions::ListenOnly,
                interest,
                Some(on_event::<F>),
                ptr::from_ref(self).cast_mut().cast(),
            )
        }?;
        if CGEvent::tap_is_enabled(&tap) {
            Some(tap)
        } else {
            tap.invalidate();
            None
        }
    }

    fn hear_until_it_dies(&self, tap: &CFMachPort) {
        let Some(source) = CFMachPort::new_run_loop_source(None, Some(tap), 0) else { return };
        let Some(run_loop) = CFRunLoop::current() else { return };
        let mode = unsafe { kCFRunLoopDefaultMode };
        *self.tap.borrow_mut() = Some(tap.retain());
        run_loop.add_source(Some(&source), mode);
        while access() == Access::Granted && alive(tap) {
            CFRunLoop::run_in_mode(mode, TAP_RETRY.as_secs_f64(), false);
        }
        run_loop.remove_source(Some(&source), mode);
        *self.tap.borrow_mut() = None;
        tap.invalidate();
    }

    fn revive(&self) {
        if let Some(tap) = &*self.tap.borrow() {
            CGEvent::tap_enable(tap, true);
        }
    }

    fn hear(&self, kind: CGEventType, event: &CGEvent) {
        let gesture = self.detector.borrow_mut().feed(input(kind, event), self.started.elapsed());
        if let Some(gesture) = gesture {
            (self.on_gesture)(gesture);
        }
    }
}

impl Hearing {
    fn announce(self, last: &mut Option<Hearing>) {
        if last.replace(self) == Some(self) {
            return;
        }
        let line = match self {
            Hearing::Listening => "bronze: listening for Shift gestures",
            Hearing::Waiting => "bronze: waiting for Accessibility access",
        };
        eprintln!("{line}");
    }
}

fn alive(tap: &CFMachPort) -> bool {
    if !CGEvent::tap_is_enabled(tap) {
        CGEvent::tap_enable(tap, true);
    }
    CGEvent::tap_is_enabled(tap)
}

fn input(kind: CGEventType, event: &CGEvent) -> Input {
    if kind != CGEventType::FlagsChanged {
        return Input::Other;
    }
    let flags = CGEvent::flags(Some(event)).0;
    match CGEvent::integer_value_field(Some(event), CGEventField::KeyboardEventKeycode) {
        KEY_LEFT_SHIFT => Input::Shift { side: Side::Left, pressed: flags & LEFT_SHIFT_DOWN != 0 },
        KEY_RIGHT_SHIFT => Input::Shift { side: Side::Right, pressed: flags & RIGHT_SHIFT_DOWN != 0 },
        _ => Input::Other,
    }
}

unsafe extern "C-unwind" fn on_event<F: Fn(Gesture)>(
    _proxy: CGEventTapProxy,
    kind: CGEventType,
    event: NonNull<CGEvent>,
    listener: *mut c_void,
) -> *mut CGEvent {
    let listener = unsafe { &*listener.cast::<Listener<F>>() };
    match kind {
        CGEventType::TapDisabledByTimeout | CGEventType::TapDisabledByUserInput => listener.revive(),
        _ => listener.hear(kind, unsafe { event.as_ref() }),
    }
    event.as_ptr()
}

#[cfg(test)]
mod tests {
    use objc2_core_foundation::CFRetained;
    use objc2_core_graphics::{CGEvent, CGEventFlags, CGEventType};

    use super::input;
    use crate::taps::{Input, Side};

    fn flags_changed(key: u16, flags: u64) -> CFRetained<CGEvent> {
        let event = CGEvent::new_keyboard_event(None, key, true).expect("a keyboard event");
        CGEvent::set_type(Some(&event), CGEventType::FlagsChanged);
        CGEvent::set_flags(Some(&event), CGEventFlags(flags));
        event
    }

    #[test]
    fn shift_events_read_their_side_and_direction_past_the_companion_bits() {
        let read = |key, flags| input(CGEventType::FlagsChanged, &flags_changed(key, flags));
        assert_eq!(read(56, 0x2002_0002), Input::Shift { side: Side::Left, pressed: true });
        assert_eq!(read(56, 0x2000_0000), Input::Shift { side: Side::Left, pressed: false });
        assert_eq!(read(60, 0x2002_0004), Input::Shift { side: Side::Right, pressed: true });
        assert_eq!(read(60, 0x2002_0002), Input::Shift { side: Side::Right, pressed: false });
        assert_eq!(read(55, 0x0010_0108), Input::Other);
        assert_eq!(input(CGEventType::KeyDown, &flags_changed(56, 0x2002_0002)), Input::Other);
    }
}
