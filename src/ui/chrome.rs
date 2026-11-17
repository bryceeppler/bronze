use rune::tok::type_;
use rune::{Cursor, El, Shadow, Tx, auto_grow, el, icon, row, text_input};
use rune_kit::{Kit, Menu, MenuItem, Placement};

use super::app::{App, Overlay, Pick};
use super::icons;
use super::look::Look;
use crate::board::Scope;

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
    row().key("top").w_full().gap(8.0).center_y().child(search).child(overflow_menu(s, more, kit, layer))
}

fn overflow_menu(s: &App, trigger: El, kit: &Kit, layer: &mut Vec<El>) -> El {
    let active = s.board().active().map(|section| section.id);
    let mut menu = Menu::new("overflow")
        .trigger(trigger)
        .placement(Placement::BottomEnd)
        .min_width(200.0)
        .open(matches!(s.overlay, Overlay::Menu))
        .on_close(|s: &mut App| s.overlay = Overlay::None)
        .item(pick_item("Show All", Pick::All, s.scope == Scope::All));
    if !s.board().sections().is_empty() {
        menu = menu.separator();
    }
    for section in s.board().sections() {
        menu = menu.item(pick_item(&section.name, Pick::Section(section.id), active == Some(section.id)));
    }
    menu.separator()
        .item(MenuItem::new("Clear Done").on_select(|s: &mut App| s.clear_done()))
        .item(MenuItem::new("Keyboard Shortcuts").on_select(|s: &mut App| s.overlay = Overlay::Shortcuts))
        .build(kit, layer)
}

fn pick_item(label: &str, pick: Pick, checked: bool) -> MenuItem {
    MenuItem::new(label).checked(checked).on_select_tx(move |s: &mut App, tx: &mut Tx| s.pick(pick, tx))
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
