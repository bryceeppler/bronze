use rune::tok::type_;
use rune::{Cursor, El, Key, KeyEvent, ScrollSpec, TextCase, Tx, col, el, icon, row, text, text_input};
use rune_kit::{Kit, Menu, MenuItem, context_menu};

use super::app::{App, Overlay, Pick, pick_key};
use super::chrome::{BODY, clear_of_caret};
use super::commands;
use super::icons;
use super::list::SECTION_LABEL;
use super::look::Look;
use super::sync_sheet;
use crate::board::Scope;

pub fn mount(s: &App, look: &Look, kit: &Kit, layer: &mut Vec<El>) {
    match &s.overlay {
        &Overlay::NoteMenu(at) => {
            let menu = note_menu(s, context_menu("note-menu", el(), Some(at)));
            let anchor = menu.build(kit, layer);
            layer.push(anchor);
        }
        &Overlay::SectionMenu(id, at) => {
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
        &Overlay::Switcher { picked } => layer.push(sheet(switcher(s, picked, look), look)),
        Overlay::Shortcuts => layer.push(sheet(shortcuts(s, look), look)),
        Overlay::Sync(setup) => layer.push(sheet(sync_sheet::view(s, setup, look), look)),
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
    col()
        .key("sheet")
        .absolute(0.0, 0.0)
        .w_full()
        .h_full()
        .px(16.0)
        .pt(50.0)
        .pb(16.0)
        .items_stretch()
        .fill(look.scrim)
        .child(
            content
                .shrink()
                .min_h(0.0)
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
        .placeholder(clear_of_caret("Go to section"), look.faint)
        .on_edit(|s: &mut App| &mut s.overlay_field)
        .on_input_change(|s: &mut App, tx: &mut Tx| {
            s.overlay = Overlay::Switcher { picked: 0 };
            tx.scroll_into_view(pick_key(0));
        })
        .on_input_key(switcher_key);
    let current = match s.scope() {
        Scope::All => Pick::All,
        Scope::Active => s.board().active().map_or(Pick::All, |section| Pick::Section(section.id)),
    };
    let mut rows = col().key("switcher-rows").padding(6.0).gap(1.0);
    for (i, (pick, name)) in s.picks().into_iter().enumerate() {
        let on = i == picked;
        rows = rows.child(
            row()
                .key(pick_key(i))
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
        .child(col().key("switcher-list").shrink().min_h(0.0).scroll(ScrollSpec { stick_bottom: false }).child(rows))
}

fn switcher_key(s: &mut App, ev: &KeyEvent, tx: &mut Tx) -> bool {
    match ev.key {
        Key::Up => s.step_switcher(-1, tx),
        Key::Down => s.step_switcher(1, tx),
        Key::Enter => s.pick_switcher(tx),
        Key::Escape => s.overlay = Overlay::None,
        _ => return false,
    }
    true
}

fn shortcuts(s: &App, look: &Look) -> El {
    let shortcut_row = |(keys, what): (String, &'static str)| {
        row()
            .h(28.0)
            .gap(12.0)
            .center_y()
            .child(text(what).font(BODY).color(look.text).grow(1.0).truncate())
            .child(text(keys).font(type_::LABEL).color(look.muted))
    };
    let heading = |title: &str| {
        row()
            .h(28.0)
            .mt(6.0)
            .center_y()
            .child(text(title).font(SECTION_LABEL).color(look.label).text_case(TextCase::Uppercase))
    };
    let command_row = |(command, what): (String, &'static str)| {
        col()
            .py(4.0)
            .gap(2.0)
            .child(text(what).font(BODY).color(look.text))
            .child(text(command).font(type_::LABEL).color(look.muted).wrap())
    };
    let bindable = commands::bindable(s.custom_data_dir());
    let mut rows = col()
        .key("shortcut-rows")
        .px(14.0)
        .pb(10.0)
        .children(commands::cheat_sheet().into_iter().map(shortcut_row))
        .child(heading("From any app"))
        .children(commands::anywhere(s.hears_gestures()).into_iter().map(shortcut_row));
    if !bindable.is_empty() {
        rows = rows
            .child(heading("Your own keys"))
            .child(
                text("Bind a shortcut in your desktop's keyboard settings to one of these commands.")
                    .font(type_::LABEL)
                    .color(look.muted)
                    .wrap()
                    .pb(4.0),
            )
            .children(bindable.into_iter().map(command_row));
    }
    col()
        .key("shortcuts")
        .child(row().h(40.0).px(14.0).center_y().child(text("Keyboard Shortcuts").font(type_::TITLE).color(look.text)))
        .child(el().w_full().h(look.hair).fill(look.hairline).mb(4.0))
        .child(col().shrink().min_h(0.0).scroll(ScrollSpec { stick_bottom: false }).child(rows))
}
