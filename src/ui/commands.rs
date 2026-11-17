use rune::{KeyEvent, KeyPlatform, Keybind, Tx};

use super::app::App;

#[derive(Clone, Copy)]
enum Command {
    Switch,
    Copy,
    Merge,
    Toggle,
    Delete,
    Deselect,
}

const BINDINGS: [(Command, &str, &str); 6] = [
    (Command::Switch, "mod+k", "Go to section"),
    (Command::Copy, "mod+c", "Copy selected notes"),
    (Command::Merge, "mod+j", "Merge selected notes"),
    (Command::Toggle, "space", "Check or uncheck"),
    (Command::Delete, "backspace, delete", "Delete selected notes"),
    (Command::Deselect, "escape", "Clear the selection"),
];

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
        Command::Deselect => s.deselect(),
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
    .collect()
}
