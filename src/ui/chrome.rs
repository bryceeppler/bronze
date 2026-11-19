use rune::tok::type_;
use rune::{Cursor, El, Shadow, Tx, auto_grow, col, el, icon, row, text, text_input};
use rune_kit::{Kit, Menu, MenuItem, Placement};

use super::app::{App, Notice, Overlay};
use super::commands;
use super::icons;
use super::look::Look;
use super::sync_sheet;
use crate::board::{Scope, SectionId};

pub const BODY: rune::TextStyle = type_::BODY.lh(18.0);

pub fn clear_of_caret(placeholder: &str) -> String {
    format!("\u{2005}{placeholder}")
}

pub fn top_bar(s: &App, look: &Look, kit: &Kit, layer: &mut Vec<El>) -> El {
    let search = row()
        .key("search-box")
        .grow(1.0)
        .h(30.0)
        .px(10.0)
        .gap(7.0)
        .center_y()
        .radius(15.0)
        .fill(look.field)
        .border(look.hair, look.field_border)
        .cursor(Cursor::Text)
        .on_press_tx(|_: &mut App, tx: &mut Tx| tx.focus("search", false))
        .child(icon(&icons::SEARCH, 14.0).color(look.faint))
        .child(
            text_input(&s.search, BODY)
                .key("search")
                .label("Search notes")
                .single_line()
                .grow(1.0)
                .color(look.text)
                .placeholder(clear_of_caret("Search"), look.faint)
                .on_edit(|s: &mut App| &mut s.search)
                .on_input_change(|s: &mut App, _| s.keep_shown_selected()),
        );
    let more = el()
        .key("more")
        .role("button")
        .label("More")
        .size(30.0, 30.0)
        .radius(15.0)
        .center()
        .fill(look.field)
        .border(look.hair, look.field_border)
        .cursor(Cursor::Pointer)
        .on_press(|s: &mut App| s.overlay = Overlay::Menu)
        .child(icon(&icons::MORE, 16.0).color(look.muted));
    row()
        .key("top")
        .w_full()
        .pt(12.0)
        .pb(2.0)
        .gap(8.0)
        .center_y()
        .window_drag()
        .child(search)
        .child(overflow_menu(s, more, kit, layer))
}

fn overflow_menu(s: &App, trigger: El, kit: &Kit, layer: &mut Vec<El>) -> El {
    let board = s.board();
    let mut menu = Menu::new("overflow")
        .trigger(trigger)
        .placement(Placement::BottomEnd)
        .min_width(220.0)
        .open(matches!(s.overlay, Overlay::Menu))
        .on_close(|s: &mut App| s.overlay = Overlay::None);
    if let Some(active) = board.active() {
        let (label, scope) = match s.scope() {
            Scope::All => (format!("Show Only {}", active.name), Scope::Active),
            Scope::Active => ("Show All Sections".to_owned(), Scope::All),
        };
        menu = menu
            .item(MenuItem::new(label).on_select_tx(move |s: &mut App, tx: &mut Tx| s.set_scope(scope, tx)))
            .separator();
    }
    if !board.sections().is_empty() {
        let target = board.active().map(|section| section.id);
        menu = menu.group_label("New Notes Go To").item(target_item("No Section", None, target));
        for section in board.sections() {
            menu = menu.item(target_item(&section.name, Some(section.id), target));
        }
        menu = menu.separator();
    }
    menu.item(MenuItem::new("Clear Done").on_select(|s: &mut App| s.clear_done()))
        .item(MenuItem::new("Keyboard Shortcuts").on_select(|s: &mut App| s.overlay = Overlay::Shortcuts))
        .item(
            MenuItem::new("Sync\u{2026}")
                .shortcut(sync_sheet::summary(&s.sync_status()))
                .on_select(|s: &mut App| s.open_sync()),
        )
        .separator()
        .group_label("Window")
        .item(MenuItem::new("Keep on Top").checked(s.keeps_on_top()).on_select(|s: &mut App| s.toggle_keep_on_top()))
        .item(MenuItem::new("Hide").shortcut(commands::label("escape")).on_select(|s: &mut App| s.hide_panel()))
        .separator()
        .item(
            MenuItem::new("Reveal Notes File")
                .disabled(s.notes_path().is_none())
                .on_select(|s: &mut App| s.reveal_notes()),
        )
        .item(MenuItem::new("Quit Bronze").on_select_tx(|_: &mut App, tx: &mut Tx| tx.quit()))
        .build(kit, layer)
}

fn target_item(label: &str, target: Option<SectionId>, current: Option<SectionId>) -> MenuItem {
    MenuItem::new(label).checked(target == current).on_select(move |s: &mut App| s.send_to(target))
}

pub fn notice(s: &App, look: &Look) -> Option<El> {
    let notice = s.notice()?;
    let words = text(notice.text()).font(type_::LABEL).color(look.muted);
    let frame = |el: El| el.key("notice").role("status").mt(8.0).fill(look.raised).border(look.hair, look.card_border);
    Some(match notice {
        Notice::NeedsAccess => {
            frame(col()).w_full().radius(16.0).px(12.0).py(10.0).gap(8.0).child(words.wrap()).child(notice_button(
                "open-settings",
                "Open Settings",
                look,
                |s| s.ask_for_access(),
            ))
        }
        Notice::GesturesOff => frame(col()).w_full().radius(16.0).px(12.0).py(10.0).gap(8.0).child(words.wrap()).child(
            row()
                .gap(8.0)
                .child(notice_button("show-shortcuts", "Keyboard Shortcuts", look, |s| {
                    s.dismiss_gestures_notice();
                    s.overlay = Overlay::Shortcuts;
                }))
                .child(notice_button("dismiss-notice", "Dismiss", look, App::dismiss_gestures_notice)),
        ),
        Notice::NothingSelected | Notice::Unavailable(..) => {
            frame(row()).self_center().h(26.0).radius(13.0).px(12.0).center_y().child(words)
        }
    })
}

fn notice_button(key: &str, label: &str, look: &Look, on_press: fn(&mut App)) -> El {
    row()
        .key(key)
        .role("button")
        .label(label)
        .self_start()
        .h(26.0)
        .px(12.0)
        .center_y()
        .radius(13.0)
        .fill(look.field)
        .border(look.hair, look.field_border)
        .cursor(Cursor::Pointer)
        .on_press(on_press)
        .child(text(label).font(type_::LABEL).color(look.text))
}

pub fn composer(s: &App, look: &Look) -> El {
    let focused = s.composer.focused;
    let placeholder = match s.board().active() {
        Some(section) => format!("Add a note or a prompt ({})", section.name),
        None => "Add a note or a prompt".to_owned(),
    };
    let field = text_input(&s.composer, BODY)
        .key("composer")
        .label("New note")
        .w_full()
        .color(look.text)
        .placeholder(clear_of_caret(&placeholder), look.faint)
        .on_edit(|s: &mut App| &mut s.composer)
        .on_submit(|s: &mut App, tx: &mut Tx| s.submit(tx));
    let card = row()
        .key("composer-card")
        .w_full()
        .mt(10.0)
        .min_h(64.0)
        .px(12.0)
        .py(11.0)
        .gap(10.0)
        .items_start()
        .radius(16.0)
        .fill(look.card)
        .cursor(Cursor::Text)
        .on_press_tx(|_: &mut App, tx: &mut Tx| tx.focus("composer", false))
        .child(icon(&icons::ADD, 18.0).color(look.faint))
        .child(auto_grow(field, 2, 8).grow(1.0));
    if focused {
        card.border(1.0, look.accent).box_shadow(Shadow {
            spread: 3.0,
            color: look.accent.alpha(0.18),
            ..Shadow::default()
        })
    } else {
        card.border(look.hair, look.card_border).shadow(1.0, 2.0, look.shadow)
    }
}
