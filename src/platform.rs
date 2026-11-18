use std::cell::{Cell, RefCell};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::thread;

#[cfg(target_os = "macos")]
mod mac;

#[cfg(target_os = "macos")]
pub use mac::listen;
#[cfg(target_os = "macos")]
use mac::{access, ask_for_access, selected_text};

pub type Reply = Box<dyn FnOnce(Option<String>) + Send>;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Access {
    Granted,
    Missing,
}

pub trait Platform {
    fn read_selection(&self, reply: Reply);
    fn reveal(&self, path: &Path);
    fn access(&self) -> Access;
    fn ask_for_access(&self);
}

pub struct Native;

impl Platform for Native {
    fn read_selection(&self, reply: Reply) {
        thread::Builder::new()
            .name("bronze-selection".into())
            .spawn(move || reply(selected_text()))
            .expect("spawn the selection reader");
    }

    fn reveal(&self, path: &Path) {
        let mut command = reveal_command(path);
        thread::spawn(move || match command.status() {
            Ok(status) if status.success() => {}
            Ok(status) => eprintln!("bronze: {command:?} exited with {status}"),
            Err(error) => eprintln!("bronze: cannot run {command:?}: {error}"),
        });
    }

    fn access(&self) -> Access {
        access()
    }

    fn ask_for_access(&self) {
        ask_for_access();
    }
}

#[cfg(not(target_os = "macos"))]
pub fn listen(_on_gesture: impl Fn(crate::taps::Gesture) + Send + 'static) {}

#[cfg(not(target_os = "macos"))]
fn access() -> Access {
    Access::Granted
}

#[cfg(not(target_os = "macos"))]
fn ask_for_access() {}

#[cfg(target_os = "linux")]
fn selected_text() -> Option<String> {
    use arboard::{GetExtLinux, LinuxClipboardKind};
    arboard::Clipboard::new().ok()?.get().clipboard(LinuxClipboardKind::Primary).text().ok()
}

#[cfg(not(any(target_os = "linux", target_os = "macos")))]
fn selected_text() -> Option<String> {
    arboard::Clipboard::new().ok()?.get_text().ok()
}

#[cfg(target_os = "macos")]
fn reveal_command(path: &Path) -> Command {
    let mut command = Command::new("open");
    command.arg("-R").arg(path);
    command
}

#[cfg(target_os = "windows")]
fn reveal_command(path: &Path) -> Command {
    let mut command = Command::new("explorer");
    command.arg(format!("/select,{}", path.display()));
    command
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
fn reveal_command(path: &Path) -> Command {
    let mut command = Command::new("xdg-open");
    command.arg(path.parent().unwrap_or(path));
    command
}

pub struct Fake {
    selection: RefCell<Option<String>>,
    revealed: RefCell<Vec<PathBuf>>,
    access: Cell<Access>,
    asked: Cell<usize>,
}

impl Default for Fake {
    fn default() -> Fake {
        Fake::selecting(None)
    }
}

impl Fake {
    pub fn selecting(text: Option<String>) -> Fake {
        Fake {
            selection: RefCell::new(text),
            revealed: RefCell::default(),
            access: Cell::new(Access::Granted),
            asked: Cell::new(0),
        }
    }

    pub fn set_access(&self, access: Access) {
        self.access.set(access);
    }

    #[cfg(test)]
    pub fn asked_for_access(&self) -> usize {
        self.asked.get()
    }

    #[cfg(test)]
    pub fn select(&self, text: Option<&str>) {
        *self.selection.borrow_mut() = text.map(str::to_owned);
    }

    #[cfg(test)]
    pub fn revealed(&self) -> Vec<PathBuf> {
        self.revealed.borrow().clone()
    }
}

impl Platform for Fake {
    fn read_selection(&self, reply: Reply) {
        reply(self.selection.borrow().clone());
    }

    fn reveal(&self, path: &Path) {
        self.revealed.borrow_mut().push(path.to_owned());
    }

    fn access(&self) -> Access {
        self.access.get()
    }

    fn ask_for_access(&self) {
        self.asked.set(self.asked.get() + 1);
    }
}
