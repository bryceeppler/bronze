use std::sync::LazyLock;

use rune::{KeyEvent, KeyPlatform, Keybind, Tx};

use super::app::App;
use crate::taps::Gesture;

#[derive(Clone, Copy)]
enum Command {
    Switch,
    Copy,
    Merge,
    Toggle,
    Delete,
    Escape,
}

const BINDINGS: [(Command, &str, &str); 6] = [
    (Command::Switch, "mod+k", "Go to section"),
    (Command::Copy, "mod+c", "Copy selected notes"),
    (Command::Merge, "mod+j", "Merge selected notes"),
    (Command::Toggle, "space", "Check or uncheck"),
    (Command::Delete, "backspace, delete", "Delete selected notes"),
    (Command::Escape, "escape", "Clear the selection, or hide Bronze"),
];

const LINK_SCHEME: &str = "bronze://";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Global {
    Capture,
    ShowOrHide,
}

impl Global {
    pub const ALL: [Global; 2] = [Global::Capture, Global::ShowOrHide];

    pub fn id(self) -> &'static str {
        match self {
            Global::Capture => "capture",
            Global::ShowOrHide => "show-or-hide",
        }
    }

    pub fn from_id(id: &str) -> Option<Global> {
        Global::ALL.into_iter().find(|global| global.id() == id)
    }

    pub fn link(self) -> String {
        format!("{LINK_SCHEME}{}", self.id())
    }

    pub fn from_link(link: &str) -> Option<Global> {
        Global::from_id(link.strip_prefix(LINK_SCHEME)?.trim_end_matches('/'))
    }

    pub fn chord(self) -> &'static str {
        match self {
            Global::Capture => "ctrl+alt+c",
            Global::ShowOrHide => "ctrl+alt+space",
        }
    }

    pub fn gesture(self) -> Gesture {
        match self {
            Global::Capture => Gesture::DoubleLeftShift,
            Global::ShowOrHide => Gesture::BothShifts,
        }
    }

    pub fn of(gesture: Gesture) -> Global {
        match gesture {
            Gesture::DoubleLeftShift => Global::Capture,
            Gesture::BothShifts => Global::ShowOrHide,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Global::Capture => "The capture shortcut",
            Global::ShowOrHide => "The show and hide shortcut",
        }
    }

    pub fn what(self) -> &'static str {
        match self {
            Global::Capture => "Capture selected text",
            Global::ShowOrHide => "Show or hide Bronze",
        }
    }
}

pub fn on_key(s: &mut App, ev: &KeyEvent, tx: &mut Tx) -> bool {
    let Some(&(command, ..)) = BINDINGS.iter().find(|(_, keys, _)| Keybind::new(keys).matches_tx(ev, tx)) else {
        return false;
    };
    let list_has_keys = tx.focused().is_none();
    match command {
        Command::Switch => {
            s.open_switcher();
            true
        }
        Command::Copy => list_has_keys && s.copy(tx),
        Command::Merge => s.merge(),
        Command::Toggle => list_has_keys && s.toggle_selected(),
        Command::Delete => list_has_keys && s.delete_selected(),
        Command::Escape => {
            s.escape();
            true
        }
    }
}

pub fn label(keys: &str) -> String {
    Keybind::new(keys).label()
}

fn gesture_label(gesture: Gesture) -> &'static str {
    match gesture {
        Gesture::DoubleLeftShift => "Double-tap Left Shift",
        Gesture::BothShifts => "Right Shift + Left Shift",
    }
}

pub fn cheat_sheet() -> Vec<(String, &'static str)> {
    let (toggle, extend) = match KeyPlatform::current() {
        KeyPlatform::Mac => ("⌘-click", "⇧-click"),
        KeyPlatform::Windows | KeyPlatform::Linux => ("Ctrl+Click", "Shift+Click"),
    };
    [
        (label("enter"), "Add the note"),
        (label("shift+enter"), "New line"),
        ("# Name".to_owned(), "Create or open a section"),
        (toggle.to_owned(), "Add to the selection"),
        (extend.to_owned(), "Select a range"),
    ]
    .into_iter()
    .chain(BINDINGS.iter().map(|&(_, keys, what)| (label(keys), what)))
    .collect()
}

pub fn anywhere(hears_gestures: bool) -> Vec<(String, &'static str)> {
    let gestures = if hears_gestures { Global::ALL.as_slice() } else { &[] };
    gestures
        .iter()
        .map(|global| (gesture_label(global.gesture()).to_owned(), global.what()))
        .chain(Global::ALL.map(|global| (label(global.chord()), global.what())))
        .collect()
}

static BRONZE: LazyLock<String> = LazyLock::new(|| {
    let Ok(running) = std::env::current_exe() else { return "bronze".to_owned() };
    let on_path = std::env::var_os("PATH").is_some_and(|path| {
        std::env::split_paths(&path).any(|dir| dir.join("bronze").canonicalize().is_ok_and(|found| found == running))
    });
    if on_path { "bronze".to_owned() } else { running.display().to_string() }
});

pub fn bindable() -> Vec<(String, &'static str)> {
    if KeyPlatform::current() != KeyPlatform::Linux {
        return Vec::new();
    }
    Global::ALL.map(|global| (format!("{} {}", *BRONZE, global.link()), global.what())).into()
}
