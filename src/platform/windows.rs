use std::cell::RefCell;
use std::mem::{self, size_of};
use std::path::Path;
use std::process::Command;
use std::sync::{Mutex, PoisonError};
use std::thread;
use std::time::{Duration, Instant};
use std::{ptr, slice};

use windows::Win32::Foundation::{GlobalFree, HANDLE, HGLOBAL, HINSTANCE, LPARAM, LRESULT, WPARAM};
use windows::Win32::System::Com::{
    CLSCTX_INPROC_SERVER, COINIT_MULTITHREADED, CoCreateInstance, CoInitializeEx, CoUninitialize,
};
use windows::Win32::System::DataExchange::{
    CloseClipboard, EmptyClipboard, EnumClipboardFormats, GetClipboardData, GetClipboardSequenceNumber, OpenClipboard,
    RegisterClipboardFormatW, SetClipboardData,
};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::System::Memory::{GMEM_MOVEABLE, GlobalAlloc, GlobalLock, GlobalSize, GlobalUnlock};
use windows::Win32::System::Ole::{
    CF_BITMAP, CF_DIB, CF_DIBV5, CF_DSPENHMETAFILE, CF_ENHMETAFILE, CF_GDIOBJFIRST, CF_GDIOBJLAST, CF_METAFILEPICT,
    CF_OEMTEXT, CF_OWNERDISPLAY, CF_PALETTE, CF_PRIVATEFIRST, CF_PRIVATELAST, CF_TEXT, CF_UNICODETEXT,
    CLIPBOARD_FORMAT,
};
use windows::Win32::UI::Accessibility::{CUIAutomation8, IUIAutomation2, IUIAutomationTextPattern, UIA_TextPatternId};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    GetAsyncKeyState, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBD_EVENT_FLAGS, KEYBDINPUT, KEYEVENTF_EXTENDEDKEY,
    KEYEVENTF_KEYUP, MAPVK_VK_TO_VSC, MapVirtualKeyW, SendInput, VIRTUAL_KEY, VK_CONTROL, VK_INSERT, VK_LBUTTON,
    VK_LSHIFT, VK_LWIN, VK_MBUTTON, VK_MENU, VK_RBUTTON, VK_RSHIFT, VK_RWIN, VK_SHIFT, VK_XBUTTON1, VK_XBUTTON2,
};
use windows::Win32::UI::Shell::{
    QUERY_USER_NOTIFICATION_STATE, QUNS_BUSY, QUNS_RUNNING_D3D_FULL_SCREEN, SHQueryUserNotificationState,
};
use windows::Win32::UI::WindowsAndMessaging::{
    CallNextHookEx, GetMessageW, HC_ACTION, KBDLLHOOKSTRUCT, KillTimer, MSG, PostMessageW, SetTimer, SetWindowsHookExW,
    UnhookWindowsHookEx, WH_KEYBOARD_LL, WM_APP, WM_KEYDOWN, WM_SYSKEYDOWN, WM_TIMER,
};
use windows::core::{PCWSTR, w};

use super::Access;
use crate::taps::{Detector, Gesture, Input, Side, TAP_WINDOW};

const AUTOMATION_TIMEOUT_MS: u32 = 300;
const MODIFIER_WAIT: Duration = Duration::from_secs(1);
const COPY_WAIT: Duration = Duration::from_millis(500);
const OPEN_WAIT: Duration = Duration::from_millis(250);
const POLL: Duration = Duration::from_millis(10);
const CONVERTIBLE: [[CLIPBOARD_FORMAT; 3]; 2] = [[CF_UNICODETEXT, CF_TEXT, CF_OEMTEXT], [CF_BITMAP, CF_DIB, CF_DIBV5]];
const MODIFIERS: [VIRTUAL_KEY; 5] = [VK_CONTROL, VK_MENU, VK_SHIFT, VK_LWIN, VK_RWIN];
const BUTTONS: [VIRTUAL_KEY; 5] = [VK_LBUTTON, VK_RBUTTON, VK_MBUTTON, VK_XBUTTON1, VK_XBUTTON2];
const BUTTON_POLL_MS: u32 = 10;
const RIGHT_SHIFT_SCAN: u32 = 0x36;

pub fn access() -> Access {
    Access::Granted
}

pub fn ask_for_access() {}

pub fn reveal_command(path: &Path) -> Command {
    let mut command = Command::new("explorer");
    command.arg(format!("/select,{}", path.display()));
    command
}

#[derive(Debug)]
enum Source {
    FocusedApp,
    PressedCopy,
}

pub fn selected_text() -> Option<String> {
    let (source, text) = match focused_selection() {
        Some(text) => (Source::FocusedApp, Some(text)),
        None => (Source::PressedCopy, copied_selection()),
    };
    let text = text.map(|text| unix_lines(&text));
    let count = text.as_deref().map_or(0, |text| text.chars().count());
    eprintln!("bronze: captured {count} characters via {source:?}");
    text
}

fn unix_lines(text: &str) -> String {
    text.replace("\r\n", "\n").replace('\r', "\n")
}

fn focused_selection() -> Option<String> {
    let _com = Com::init()?;
    let text = automation_selection().ok()?;
    (!text.is_empty()).then_some(text)
}

fn automation_selection() -> windows::core::Result<String> {
    unsafe {
        let automation: IUIAutomation2 = CoCreateInstance(&CUIAutomation8, None, CLSCTX_INPROC_SERVER)?;
        automation.SetConnectionTimeout(AUTOMATION_TIMEOUT_MS)?;
        automation.SetTransactionTimeout(AUTOMATION_TIMEOUT_MS)?;
        let text: IUIAutomationTextPattern = automation.GetFocusedElement()?.GetCurrentPatternAs(UIA_TextPatternId)?;
        let ranges = text.GetSelection()?;
        let parts = (0..ranges.Length()?)
            .map(|index| Ok(ranges.GetElement(index)?.GetText(-1)?.to_string()))
            .collect::<windows::core::Result<Vec<String>>>()?;
        Ok(parts.into_iter().filter(|part| !part.is_empty()).collect::<Vec<_>>().join("\n"))
    }
}

struct Com;

impl Com {
    fn init() -> Option<Com> {
        unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) }.is_ok().then_some(Com)
    }
}

impl Drop for Com {
    fn drop(&mut self) {
        unsafe { CoUninitialize() };
    }
}

static CLIPBOARD: Mutex<()> = Mutex::new(());

fn copied_selection() -> Option<String> {
    let _only_copy = CLIPBOARD.lock().unwrap_or_else(PoisonError::into_inner);
    within(MODIFIER_WAIT, || (!MODIFIERS.into_iter().any(is_down)).then_some(()))?;
    let saved = Clipboard::open()?.contents();
    let before = sequence();
    press_copy();
    let text = within(COPY_WAIT, || (sequence() != before).then(|| Clipboard::open()?.text()).flatten());
    if sequence() != before
        && let Some(clipboard) = Clipboard::open()
    {
        clipboard.replace(&saved);
    }
    text
}

fn within<T>(limit: Duration, mut ready: impl FnMut() -> Option<T>) -> Option<T> {
    let deadline = Instant::now() + limit;
    loop {
        if let Some(value) = ready() {
            return Some(value);
        }
        if Instant::now() >= deadline {
            return None;
        }
        thread::sleep(POLL);
    }
}

fn sequence() -> u32 {
    unsafe { GetClipboardSequenceNumber() }
}

fn is_down(key: VIRTUAL_KEY) -> bool {
    (unsafe { GetAsyncKeyState(key.0.into()) }) < 0
}

fn press_copy() {
    let key = |key: VIRTUAL_KEY, flags: KEYBD_EVENT_FLAGS| INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT {
                wVk: key,
                wScan: unsafe { MapVirtualKeyW(key.0.into(), MAPVK_VK_TO_VSC) } as u16,
                dwFlags: flags,
                time: 0,
                dwExtraInfo: 0,
            },
        },
    };
    let presses = [
        key(VK_CONTROL, KEYBD_EVENT_FLAGS::default()),
        key(VK_INSERT, KEYEVENTF_EXTENDEDKEY),
        key(VK_INSERT, KEYEVENTF_EXTENDEDKEY | KEYEVENTF_KEYUP),
        key(VK_CONTROL, KEYEVENTF_KEYUP),
    ];
    unsafe { SendInput(&presses, size_of::<INPUT>() as i32) };
}

type Contents = Vec<(u32, Vec<u8>)>;

struct Clipboard;

impl Clipboard {
    fn open() -> Option<Clipboard> {
        within(OPEN_WAIT, || unsafe { OpenClipboard(None) }.ok().map(|()| Clipboard))
    }

    fn formats(&self) -> impl Iterator<Item = u32> + '_ {
        let mut format = 0;
        std::iter::from_fn(move || {
            format = unsafe { EnumClipboardFormats(format) };
            (format != 0).then_some(format)
        })
    }

    fn contents(&self) -> Contents {
        restorable(self.formats()).into_iter().filter_map(|format| Some((format, self.bytes(format)?))).collect()
    }

    fn bytes(&self, format: u32) -> Option<Vec<u8>> {
        let memory = HGLOBAL(unsafe { GetClipboardData(format) }.ok()?.0);
        let size = unsafe { GlobalSize(memory) };
        let data = unsafe { GlobalLock(memory) };
        if data.is_null() {
            return None;
        }
        let bytes = unsafe { slice::from_raw_parts(data.cast::<u8>(), size) }.to_vec();
        let _ = unsafe { GlobalUnlock(memory) };
        (!bytes.is_empty()).then_some(bytes)
    }

    fn text(&self) -> Option<String> {
        self.bytes(CF_UNICODETEXT.0.into()).map(|bytes| utf16_text(&bytes))
    }

    fn replace(self, contents: &Contents) {
        let _ = unsafe { EmptyClipboard() };
        for (format, bytes) in contents {
            put(*format, bytes);
        }
        if !contents.is_empty() {
            put(unsafe { RegisterClipboardFormatW(w!("ExcludeClipboardContentFromMonitorProcessing")) }, &[0]);
        }
    }
}

impl Drop for Clipboard {
    fn drop(&mut self) {
        let _ = unsafe { CloseClipboard() };
    }
}

fn put(format: u32, bytes: &[u8]) {
    let Ok(memory) = (unsafe { GlobalAlloc(GMEM_MOVEABLE, bytes.len()) }) else { return };
    let data = unsafe { GlobalLock(memory) };
    if !data.is_null() {
        unsafe { ptr::copy_nonoverlapping(bytes.as_ptr(), data.cast::<u8>(), bytes.len()) };
        let _ = unsafe { GlobalUnlock(memory) };
        if unsafe { SetClipboardData(format, Some(HANDLE(memory.0))) }.is_ok() {
            return;
        }
    }
    let _ = unsafe { GlobalFree(Some(memory)) };
}

fn restorable(formats: impl Iterator<Item = u32>) -> Vec<u32> {
    let mut kept: Vec<u32> = Vec::new();
    for format in formats.filter(|&format| is_memory_block(format)) {
        let derived = family(format).is_some_and(|own| kept.iter().any(|&earlier| family(earlier) == Some(own)));
        if !derived {
            kept.push(format);
        }
    }
    kept
}

fn family(format: u32) -> Option<usize> {
    CONVERTIBLE.iter().position(|family| family.iter().any(|member| u32::from(member.0) == format))
}

fn is_memory_block(format: u32) -> bool {
    let handles = [CF_BITMAP, CF_METAFILEPICT, CF_PALETTE, CF_ENHMETAFILE].map(|format| u32::from(format.0));
    let ranges =
        [(CF_OWNERDISPLAY, CF_DSPENHMETAFILE), (CF_PRIVATEFIRST, CF_PRIVATELAST), (CF_GDIOBJFIRST, CF_GDIOBJLAST)]
            .map(|(first, last)| u32::from(first.0)..=u32::from(last.0));
    !handles.contains(&format) && !ranges.iter().any(|range| range.contains(&format))
}

fn utf16_text(bytes: &[u8]) -> String {
    let units: Vec<u16> =
        bytes.as_chunks::<2>().0.iter().map(|&pair| u16::from_le_bytes(pair)).take_while(|&unit| unit != 0).collect();
    String::from_utf16_lossy(&units)
}

pub fn listen(on_gesture: impl Fn(Gesture) + Send + 'static) {
    thread::Builder::new()
        .name("bronze-shift-listener".into())
        .spawn(move || hear(on_gesture))
        .expect("spawn the Shift listener");
}

thread_local! {
    static LISTENER: RefCell<Option<Listener>> = const { RefCell::new(None) };
}

struct Listener {
    ear: Ear,
    started: Instant,
    timer: Option<usize>,
    fired: Vec<Gesture>,
}

fn hear(on_gesture: impl Fn(Gesture)) {
    LISTENER.set(Some(Listener { ear: Ear::default(), started: Instant::now(), timer: None, fired: Vec::new() }));
    let module = unsafe { GetModuleHandleW(PCWSTR::null()) }.ok().map(HINSTANCE::from);
    let hook = match unsafe { SetWindowsHookExW(WH_KEYBOARD_LL, Some(on_key), module, 0) } {
        Ok(hook) => hook,
        Err(error) => return eprintln!("bronze: cannot listen for Shift gestures: {error}"),
    };
    eprintln!("bronze: listening for Shift gestures");
    let mut message = MSG::default();
    while unsafe { GetMessageW(&mut message, None, 0, 0) }.0 > 0 {
        match message.message {
            WM_TIMER => notice(Event::Tick { button: BUTTONS.into_iter().any(is_down) }),
            WM_APP => {
                for gesture in fired() {
                    if !full_screen_in_front() {
                        on_gesture(gesture);
                    }
                }
            }
            _ => {}
        }
    }
    let _ = unsafe { UnhookWindowsHookEx(hook) };
}

unsafe extern "system" fn on_key(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if code == HC_ACTION as i32 {
        let key = unsafe { &*(lparam.0 as *const KBDLLHOOKSTRUCT) };
        let down = matches!(wparam.0 as u32, WM_KEYDOWN | WM_SYSKEYDOWN);
        notice(Event::Key(Stroke { key: Key::of(key.vkCode, key.scanCode), down }));
    }
    unsafe { CallNextHookEx(None, code, wparam, lparam) }
}

fn notice(event: Event) {
    LISTENER.with_borrow_mut(|listener| {
        if let Some(listener) = listener {
            listener.notice(event);
        }
    });
}

fn fired() -> Vec<Gesture> {
    LISTENER.with_borrow_mut(|listener| {
        listener.as_mut().map(|listener| mem::take(&mut listener.fired)).unwrap_or_default()
    })
}

impl Listener {
    fn notice(&mut self, event: Event) {
        let at = self.started.elapsed();
        let before = Shifts { left: is_down(VK_LSHIFT), right: is_down(VK_RSHIFT) };
        if let Some(gesture) = self.ear.hear(before, event, at) {
            self.fired.push(gesture);
            let _ = unsafe { PostMessageW(None, WM_APP, WPARAM(0), LPARAM(0)) };
        }
        match (self.ear.watching_buttons(at), self.timer) {
            (true, None) => self.timer = Some(unsafe { SetTimer(None, 0, BUTTON_POLL_MS, None) }).filter(|&id| id != 0),
            (false, Some(id)) => {
                let _ = unsafe { KillTimer(None, id) };
                self.timer = None;
            }
            _ => {}
        }
    }
}

fn full_screen_in_front() -> bool {
    unsafe { SHQueryUserNotificationState() }.is_ok_and(silences_gestures)
}

fn silences_gestures(state: QUERY_USER_NOTIFICATION_STATE) -> bool {
    matches!(state, QUNS_BUSY | QUNS_RUNNING_D3D_FULL_SCREEN)
}

#[derive(Clone, Copy, Default, PartialEq, Eq)]
struct Shifts {
    left: bool,
    right: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Key {
    Shift(Side),
    Other,
}

#[derive(Clone, Copy)]
struct Stroke {
    key: Key,
    down: bool,
}

#[derive(Clone, Copy)]
enum Event {
    Key(Stroke),
    Tick { button: bool },
}

#[derive(Default)]
struct Ear {
    detector: Detector,
    fed: Shifts,
    released: Option<Duration>,
}

impl Shifts {
    fn down(self, side: Side) -> bool {
        match side {
            Side::Left => self.left,
            Side::Right => self.right,
        }
    }

    fn with(self, side: Side, down: bool) -> Shifts {
        match side {
            Side::Left => Shifts { left: down, ..self },
            Side::Right => Shifts { right: down, ..self },
        }
    }
}

impl Key {
    fn of(code: u32, scan: u32) -> Key {
        match VIRTUAL_KEY(code as u16) {
            VK_LSHIFT => Key::Shift(Side::Left),
            VK_RSHIFT => Key::Shift(Side::Right),
            VK_SHIFT if scan == RIGHT_SHIFT_SCAN => Key::Shift(Side::Right),
            VK_SHIFT => Key::Shift(Side::Left),
            _ => Key::Other,
        }
    }
}

impl Ear {
    fn hear(&mut self, before: Shifts, event: Event, at: Duration) -> Option<Gesture> {
        let mut gesture = None;
        for input in self.inputs(before, event) {
            if let Input::Shift { pressed: false, .. } = input {
                self.released = Some(at);
            }
            gesture = gesture.or(self.detector.feed(input, at));
        }
        gesture
    }

    fn inputs(&mut self, before: Shifts, event: Event) -> Vec<Input> {
        let mut inputs = Vec::new();
        for side in [Side::Left, Side::Right] {
            if self.fed.down(side) && !before.down(side) {
                self.fed = self.fed.with(side, false);
                inputs.push(Input::Shift { side, pressed: false });
            }
        }
        match event {
            Event::Key(Stroke { key: Key::Shift(side), down }) if self.fed.down(side) != down => {
                self.fed = self.fed.with(side, down);
                inputs.push(Input::Shift { side, pressed: down });
            }
            Event::Key(Stroke { key: Key::Other, down: true }) | Event::Tick { button: true } => {
                inputs.push(Input::Other)
            }
            Event::Key(_) | Event::Tick { button: false } => {}
        }
        inputs
    }

    fn watching_buttons(&self, at: Duration) -> bool {
        self.fed != Shifts::default() || self.released.is_some_and(|released| at.saturating_sub(released) < TAP_WINDOW)
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use windows::Win32::UI::Shell::{
        QUNS_ACCEPTS_NOTIFICATIONS, QUNS_APP, QUNS_BUSY, QUNS_NOT_PRESENT, QUNS_PRESENTATION_MODE, QUNS_QUIET_TIME,
        QUNS_RUNNING_D3D_FULL_SCREEN,
    };

    use super::{Ear, Event, Key, Shifts, Stroke, restorable, silences_gestures, unix_lines, utf16_text};
    use crate::taps::{Gesture, Side, TAP_WINDOW};

    const NONE: Shifts = Shifts { left: false, right: false };
    const LEFT: Shifts = Shifts { left: true, right: false };
    const RIGHT: Shifts = Shifts { left: false, right: true };

    fn shift(side: Side, down: bool) -> Event {
        Event::Key(Stroke { key: Key::Shift(side), down })
    }

    fn letter(down: bool) -> Event {
        Event::Key(Stroke { key: Key::Other, down })
    }

    fn hear(timeline: &[(u64, Shifts, Event)]) -> Vec<(u64, Gesture)> {
        let mut ear = Ear::default();
        timeline
            .iter()
            .filter_map(|&(ms, before, event)| {
                ear.hear(before, event, Duration::from_millis(ms)).map(|gesture| (ms, gesture))
            })
            .collect()
    }

    #[test]
    fn a_double_tap_heard_by_the_hook_fires_on_the_second_release() {
        let left = |down| shift(Side::Left, down);
        let fired =
            hear(&[(0, NONE, left(true)), (80, LEFT, left(false)), (200, NONE, left(true)), (260, LEFT, left(false))]);
        assert_eq!(fired, [(260, Gesture::DoubleLeftShift)]);
    }

    #[test]
    fn a_release_the_hook_never_heard_does_not_leave_shift_held() {
        let fired = hear(&[
            (0, NONE, shift(Side::Left, true)),
            (5000, NONE, shift(Side::Right, true)),
            (5080, RIGHT, shift(Side::Right, false)),
        ]);
        assert_eq!(fired, []);
    }

    #[test]
    fn the_first_double_tap_after_a_release_the_hook_never_heard_fires() {
        let left = |down| shift(Side::Left, down);
        let fired = hear(&[
            (0, NONE, left(true)),
            (5000, NONE, left(true)),
            (5080, LEFT, left(false)),
            (5200, NONE, left(true)),
            (5260, LEFT, left(false)),
        ]);
        assert_eq!(fired, [(5260, Gesture::DoubleLeftShift)]);
    }

    #[test]
    fn a_mouse_button_down_during_the_taps_cancels_them() {
        let left = |down| shift(Side::Left, down);
        for (button, expected) in [(true, vec![]), (false, vec![(240, Gesture::DoubleLeftShift)])] {
            let tick = Event::Tick { button };
            let fired = hear(&[
                (0, NONE, left(true)),
                (40, LEFT, tick),
                (90, LEFT, left(false)),
                (160, NONE, left(true)),
                (200, LEFT, tick),
                (240, LEFT, left(false)),
            ]);
            assert_eq!(fired, expected, "button down: {button}");
        }
    }

    #[test]
    fn only_pressing_another_key_cancels_a_tap() {
        let left = |down| shift(Side::Left, down);
        let tap_with = |other| {
            hear(&[
                (0, NONE, left(true)),
                (30, LEFT, other),
                (80, LEFT, left(false)),
                (200, NONE, left(true)),
                (260, LEFT, left(false)),
            ])
        };
        assert_eq!(tap_with(letter(false)), [(260, Gesture::DoubleLeftShift)]);
        assert_eq!(tap_with(letter(true)), []);
    }

    #[test]
    fn the_mouse_is_watched_only_while_a_shift_is_down_or_just_released() {
        let mut ear = Ear::default();
        let at = Duration::from_millis;
        assert!(!ear.watching_buttons(at(0)));
        ear.hear(NONE, shift(Side::Left, true), at(0));
        assert!(ear.watching_buttons(at(5000)));
        ear.hear(NONE, Event::Tick { button: false }, at(5000));
        assert!(ear.watching_buttons(at(5000) + TAP_WINDOW - at(1)));
        assert!(!ear.watching_buttons(at(5000) + TAP_WINDOW));
    }

    #[test]
    fn only_a_full_screen_app_in_front_silences_the_gestures() {
        for state in [QUNS_BUSY, QUNS_RUNNING_D3D_FULL_SCREEN] {
            assert!(silences_gestures(state), "{state:?}");
        }
        for state in [QUNS_ACCEPTS_NOTIFICATIONS, QUNS_NOT_PRESENT, QUNS_PRESENTATION_MODE, QUNS_QUIET_TIME, QUNS_APP] {
            assert!(!silences_gestures(state), "{state:?}");
        }
    }

    #[test]
    fn a_generic_shift_code_takes_its_side_from_the_scan_code() {
        assert_eq!(Key::of(0x10, 0x2A), Key::Shift(Side::Left));
        assert_eq!(Key::of(0x10, 0x36), Key::Shift(Side::Right));
        assert_eq!(Key::of(0xA1, 0x36), Key::Shift(Side::Right));
        assert_eq!(Key::of(0x41, 0x1E), Key::Other);
    }

    #[test]
    fn clipboard_text_ends_at_its_terminator_and_keeps_every_character() {
        let bytes: Vec<u8> = "Café 🦀\0garbage".encode_utf16().flat_map(u16::to_le_bytes).collect();
        assert_eq!(utf16_text(&bytes), "Café 🦀");
        assert_eq!(utf16_text(&[]), "");
    }

    #[test]
    fn captured_text_uses_unix_line_breaks() {
        assert_eq!(unix_lines("one\r\ntwo\rthree\nfour"), "one\ntwo\nthree\nfour");
    }

    #[test]
    fn only_plain_memory_is_saved_and_only_the_placed_format_of_each_convertible_family() {
        let saved = |formats: &[u32]| restorable(formats.iter().copied());
        assert_eq!(saved(&[13, 0xC0F1, 16, 1, 7]), [13, 0xC0F1, 16]);
        assert_eq!(saved(&[1, 13, 7]), [1]);
        assert_eq!(saved(&[0xC0F0, 2, 0xC0F2, 8, 17]), [0xC0F0, 0xC0F2, 8]);
        assert_eq!(saved(&[17, 8, 2]), [17]);
        assert_eq!(saved(&[15, 0xC2F1, 0xC00D]), [15, 0xC2F1, 0xC00D]);
        assert_eq!(saved(&[3, 9, 14, 0x80, 0x82, 0x83, 0x8E, 0x200, 0x2FF, 0x300, 0x3FF]), Vec::<u32>::new());
    }
}
