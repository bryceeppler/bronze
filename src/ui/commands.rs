use rune::{KeyEvent, KeyPlatform, Keybind, Tx};

use super::app::App;

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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Global {
    Capture,
    ShowOrHide,
}

impl Global {
    pub const ALL: [Global; 2] = [Global::Capture, Global::ShowOrHide];

    pub fn chord(self) -> &'static str {
        match self {
            Global::Capture => "ctrl+alt+c",
            Global::ShowOrHide => "ctrl+alt+space",
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
            Global::Capture => "Capture selected text anywhere",
            Global::ShowOrHide => "Show or hide Bronze anywhere",
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
    .chain(Global::ALL.map(|global| (label(global.chord()), global.what())))
    .collect()
}
