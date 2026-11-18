use std::mem::size_of;
use std::path::Path;
use std::process::Command;
use std::sync::{Mutex, PoisonError};
use std::thread;
use std::time::{Duration, Instant};
use std::{ptr, slice};

use windows::Win32::Foundation::{GlobalFree, HANDLE, HGLOBAL};
use windows::Win32::System::Com::{
    CLSCTX_INPROC_SERVER, COINIT_MULTITHREADED, CoCreateInstance, CoInitializeEx, CoUninitialize,
};
use windows::Win32::System::DataExchange::{
    CloseClipboard, EmptyClipboard, EnumClipboardFormats, GetClipboardData, GetClipboardSequenceNumber, OpenClipboard,
    RegisterClipboardFormatW, SetClipboardData,
};
use windows::Win32::System::Memory::{GMEM_MOVEABLE, GlobalAlloc, GlobalLock, GlobalSize, GlobalUnlock};
use windows::Win32::System::Ole::{
    CF_BITMAP, CF_DIB, CF_DIBV5, CF_DSPENHMETAFILE, CF_ENHMETAFILE, CF_GDIOBJFIRST, CF_GDIOBJLAST, CF_METAFILEPICT,
    CF_OEMTEXT, CF_OWNERDISPLAY, CF_PALETTE, CF_PRIVATEFIRST, CF_PRIVATELAST, CF_TEXT, CF_UNICODETEXT,
    CLIPBOARD_FORMAT,
};
use windows::Win32::UI::Accessibility::{CUIAutomation8, IUIAutomation2, IUIAutomationTextPattern, UIA_TextPatternId};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    GetAsyncKeyState, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBD_EVENT_FLAGS, KEYBDINPUT, KEYEVENTF_EXTENDEDKEY,
    KEYEVENTF_KEYUP, MAPVK_VK_TO_VSC, MapVirtualKeyW, SendInput, VIRTUAL_KEY, VK_CONTROL, VK_INSERT, VK_LWIN, VK_MENU,
    VK_RWIN, VK_SHIFT,
};
use windows::core::w;

use super::Access;
use crate::taps::Gesture;

const AUTOMATION_TIMEOUT_MS: u32 = 300;
const MODIFIER_WAIT: Duration = Duration::from_secs(1);
const COPY_WAIT: Duration = Duration::from_millis(500);
const OPEN_WAIT: Duration = Duration::from_millis(250);
const TAIL_WAIT: Duration = Duration::from_millis(250);
const POLL: Duration = Duration::from_millis(10);
const CONVERTIBLE: [[CLIPBOARD_FORMAT; 3]; 2] = [[CF_UNICODETEXT, CF_TEXT, CF_OEMTEXT], [CF_BITMAP, CF_DIB, CF_DIBV5]];
const MODIFIERS: [VIRTUAL_KEY; 5] = [VK_CONTROL, VK_MENU, VK_SHIFT, VK_LWIN, VK_RWIN];

pub fn listen(_on_gesture: impl Fn(Gesture) + Send + 'static) {}

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
    if sequence() != before {
        put_back(&saved);
    }
    text
}

fn put_back(saved: &Contents) {
    let deadline = Instant::now() + TAIL_WAIT;
    let mut ours = restore(saved);
    while let Some(seen) = ours {
        let left = deadline.saturating_duration_since(Instant::now());
        ours = within(left, || (sequence() != seen).then_some(())).and_then(|()| restore(saved));
    }
}

fn restore(saved: &Contents) -> Option<u32> {
    Clipboard::open()?.replace(saved);
    Some(sequence())
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

#[cfg(test)]
mod tests {
    use super::{restorable, unix_lines, utf16_text};

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
