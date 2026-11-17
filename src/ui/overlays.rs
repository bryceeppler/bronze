use rune::tok::type_;
use rune::{Cursor, El, Key, KeyEvent, Tx, col, el, icon, row, text, text_input};
use rune_kit::{Kit, Menu, MenuItem, context_menu};

use super::app::{App, Overlay, Pick};
use super::chrome::BODY;
use super::commands;
use super::icons;
use super::look::Look;
use crate::board::Scope;

pub fn mount(s: &App, look: &Look, kit: &Kit, layer: &mut Vec<El>) {
    match s.overlay {
        Overlay::NoteMenu(at) => {
            let menu = note_menu(s, context_menu("note-menu", el(), Some(at)));
            let anchor = menu.build(kit, layer);
            layer.push(anchor);
        }
        Overlay::SectionMenu(id, at) => {
            let menu = context_menu("section-menu", el(), Some(at))
                .min_width(180.0)
                .on_close(|s: &mut App| s.overlay = Overlay::None)
                .item(MenuItem::new("Rename Section").on_select(move |s: &mut App| s.start_rename(id)))
                .item(
                    MenuItem::new("Delete Section")
                        .description("Its notes move to the top")
                        .on_select(move |s: &mut App| s.remove_section(id)),
                );
            let anchor = menu.build(kit, layer);
            layer.push(anchor);
        }
        Overlay::Switcher { picked } => layer.push(sheet(switcher(s, picked, look), look)),
        Overlay::Shortcuts => layer.push(sheet(shortcuts(look), look)),
        Overlay::None | Overlay::Menu | Overlay::Rename(_) => {}
    }
}

fn note_menu(s: &App, menu: Menu) -> Menu {
    let count = s.selected().len();
    let all_done = s.board().notes().filter(|n| s.is_selected(n.id)).all(|n| n.done);
    let close = |s: &mut App| s.overlay = Overlay::None;
    menu.min_width(200.0)
        .on_close(close)
        .item(
            MenuItem::new(if count > 1 { "Copy Notes" } else { "Copy" })
                .shortcut(commands::label("mod+c"))
                .on_select_tx(|s: &mut App, tx: &mut Tx| {
                    s.copy(tx);
                    s.overlay = Overlay::None;
                }),
        )
        .item(MenuItem::new("Merge").shortcut(commands::label("mod+j")).disabled(count < 2).on_select(|s: &mut App| {
            s.merge();
            s.overlay = Overlay::None;
        }))
        .item(
            MenuItem::new(if all_done { "Mark as Not Done" } else { "Mark as Done" })
                .shortcut(commands::label("space"))
                .on_select(|s: &mut App| {
                    s.toggle_selected();
                    s.overlay = Overlay::None;
                }),
        )
        .separator()
        .item(MenuItem::new("Delete").danger(true).shortcut(commands::label("backspace")).on_select(|s: &mut App| {
            s.delete_selected();
            s.overlay = Overlay::None;
        }))
}

fn sheet(content: El, look: &Look) -> El {
    col().key("sheet").absolute(0.0, 0.0).w_full().h_full().px(16.0).pt(50.0).items_stretch().fill(look.scrim).child(
        content
            .radius(14.0)
            .fill(look.raised)
            .border(look.hair, look.card_border)
            .box_shadow(look.popover_shadow())
            .modal(true, |s: &mut App| s.overlay = Overlay::None),
    )
}

fn switcher(s: &App, picked: usize, look: &Look) -> El {
    let field = text_input(&s.overlay_field, BODY)
        .key("switcher-query")
        .label("Section name")
        .single_line()
        .grow(1.0)
        .color(look.text)
        .placeholder("Go to section", look.faint)
        .on_edit(|s: &mut App| &mut s.overlay_field)
        .on_input_change(|s: &mut App, _| s.overlay = Overlay::Switcher { picked: 0 })
        .on_input_key(switcher_key);
    let current = match s.scope {
        Scope::All => Pick::All,
        Scope::Active => s.board().active().map_or(Pick::All, |section| Pick::Section(section.id)),
    };
    let mut rows = col().key("switcher-rows").padding(6.0).gap(1.0);
    for (i, (pick, name)) in s.picks().into_iter().enumerate() {
        let on = i == picked;
        rows = rows.child(
            row()
                .key(format!("pick-{i}"))
                .role("option")
                .h(30.0)
                .px(10.0)
                .gap(8.0)
                .center_y()
                .radius(8.0)
                .fill(if on { look.selected } else { look.raised })
                .cursor(Cursor::Pointer)
                .on_press_tx(move |s: &mut App, tx: &mut Tx| s.pick(pick, tx))
                .child(text(name).font(BODY).color(look.text).grow(1.0).truncate())
                .children((pick == current).then(|| icon(&icons::CHECK, 14.0).color(look.accent))),
        );
    }
    col()
        .key("switcher")
        .child(
            row().h(40.0).px(12.0).gap(8.0).center_y().child(icon(&icons::SEARCH, 14.0).color(look.faint)).child(field),
        )
        .child(el().w_full().h(look.hair).fill(look.hairline))
        .child(rows)
}

fn switcher_key(s: &mut App, ev: &KeyEvent, tx: &mut Tx) -> bool {
    match ev.key {
        Key::Up => s.step_switcher(-1),
        Key::Down => s.step_switcher(1),
        Key::Enter => s.pick_switcher(tx),
        Key::Escape => s.overlay = Overlay::None,
        _ => return false,
    }
    true
}

fn shortcuts(look: &Look) -> El {
    let mut rows = col().key("shortcut-rows").px(14.0).pb(10.0);
    for (keys, what) in commands::cheat_sheet() {
        rows = rows.child(
            row()
                .h(28.0)
                .gap(12.0)
                .center_y()
                .child(text(what).font(BODY).color(look.text).grow(1.0).truncate())
                .child(text(keys).font(type_::LABEL).color(look.muted)),
        );
    }
    col()
        .key("shortcuts")
        .child(row().h(40.0).px(14.0).center_y().child(text("Keyboard Shortcuts").font(type_::TITLE).color(look.text)))
        .child(el().w_full().h(look.hair).fill(look.hairline).mb(4.0))
        .child(rows)
}
