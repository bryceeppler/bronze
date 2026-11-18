use std::path::Path;
use std::process::Command;

use arboard::{GetExtLinux, LinuxClipboardKind};

use super::Access;
use crate::taps::Gesture;

pub fn listen(_on_gesture: impl Fn(Gesture) + Send + 'static) {}

pub fn access() -> Access {
    Access::Granted
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
