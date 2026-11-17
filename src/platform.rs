use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::thread;

pub type Reply = Box<dyn FnOnce(Option<String>) + Send>;

pub trait Platform {
    fn read_selection(&self, reply: Reply);
    fn reveal(&self, path: &Path);
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
}

#[cfg(target_os = "linux")]
fn selected_text() -> Option<String> {
    use arboard::{GetExtLinux, LinuxClipboardKind};
    arboard::Clipboard::new().ok()?.get().clipboard(LinuxClipboardKind::Primary).text().ok()
}

#[cfg(not(target_os = "linux"))]
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

#[derive(Default)]
pub struct Fake {
    selection: RefCell<Option<String>>,
    revealed: RefCell<Vec<PathBuf>>,
}

impl Fake {
    pub fn selecting(text: Option<String>) -> Fake {
        Fake { selection: RefCell::new(text), revealed: RefCell::default() }
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
}
