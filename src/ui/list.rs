use std::rc::Rc;

use rune::tok::type_;
use rune::{
    Cursor, Cx, Drag, El, Phase, ScrollSpec, Shadow, Span, TextCase, Tx, col, el, icon, rich, row, text, text_input,
};

use super::app::{App, Click, LIST_KEY, Overlay, Shown, card_key};
use super::chrome::BODY;
use super::icons;
use super::look::Look;
use super::strike;
use crate::board::{Note, NoteId, Owner, Place};
use crate::inline::{self, Mark};

const GAP: f32 = 8.0;
const CONTENT_KEY: &str = "list-content";
const SECTION_LABEL: rune::TextStyle = type_::SECTION.tracking(0.6);

struct Geometry {
    origin: f32,
    gaps: Vec<(f32, Place)>,
}

impl Geometry {
    fn target(&self, y: f32) -> Option<Place> {
        self.gaps.iter().min_by(|a, b| (a.0 - y).abs().total_cmp(&(b.0 - y).abs())).map(|&(_, place)| place)
    }
}

pub fn list(s: &App, cx: &mut Cx<App>, look: &Look) -> El {
    let shown = s.shown();
    let geometry = s.dragging.and_then(|d| geometry(cx, &shown, d.id)).map(Rc::new);
    let mut content = col().key(CONTENT_KEY).w_full().pt(10.0).pb(2.0).gap(GAP);
    for list in &shown {
        if let (Some(name), Owner::Section(id)) = (list.name, list.owner) {
            content = content.child(section_header(s, id, name, look));
        }
        for note in &list.notes {
            let width = cx.layout_with(&format!("{}/text", card_key(note.id)), |r| r[2]);
            content = content.child(card(s, note, look, width, geometry.clone()));
        }
    }
    if !s.search.text.trim().is_empty() && shown.is_empty() {
        content = content
            .child(text("No matching notes").key("no-matches").font(BODY).color(look.faint).self_center().mt(24.0));
    }
    if let (Some(target), Some(geometry), Some(top)) =
        (s.dragging.and_then(|d| d.target), &geometry, cx.layout(CONTENT_KEY))
        && let Some(&(y, _)) = geometry.gaps.iter().find(|(_, place)| *place == target)
    {
        content = content.child(
            el().key("drop-line").absolute(4.0, y - top[1] - 1.0).w(top[2] - 8.0).h(2.0).radius(1.0).fill(look.accent),
        );
    }
    col()
        .key(LIST_KEY)
        .w_full()
        .grow(1.0)
        .min_h(0.0)
        .scroll(ScrollSpec { stick_bottom: false })
        .on_press_tx(|s: &mut App, tx: &mut Tx| {
            s.deselect();
            tx.blur();
        })
        .child(content)
}

fn geometry(cx: &mut Cx<App>, shown: &[Shown<'_>], dragged: NoteId) -> Option<Geometry> {
    let origin = cx.layout(&card_key(dragged))?[1];
    let content_top = cx.layout(CONTENT_KEY).map_or(0.0, |r| r[1]);
    let mut gaps = Vec::new();
    for list in shown {
        let mut end = match list.owner {
            Owner::Section(id) => cx.layout(&header_key(id)).map(|r| r[1] + r[3] + GAP / 2.0),
            Owner::Loose => Some(content_top + GAP / 2.0),
        };
        for note in list.notes.iter().filter(|n| n.id != dragged) {
            if let Some(rect) = cx.layout(&card_key(note.id)) {
                gaps.push((rect[1] - GAP / 2.0, Place::Before(note.id)));
                end = Some(rect[1] + rect[3] + GAP / 2.0);
            }
        }
        if let Some(y) = end {
            gaps.push((y, Place::End(list.owner)));
        }
    }
    Some(Geometry { origin, gaps })
}

fn header_key(id: crate::board::SectionId) -> String {
    format!("section-{id}")
}

fn section_header(s: &App, id: crate::board::SectionId, name: &str, look: &Look) -> El {
    let label = match &s.overlay {
        Overlay::Rename(renaming) if *renaming == id => text_input(&s.overlay_field, SECTION_LABEL)
            .key("rename")
            .label("Section name")
            .single_line()
            .w(180.0)
            .color(look.text)
            .on_edit(|s: &mut App| &mut s.overlay_field)
            .on_submit(|s: &mut App, _| s.finish_rename())
            .modal(true, |s: &mut App| s.overlay = Overlay::None),
        _ => text(name).font(SECTION_LABEL).color(look.label).text_case(TextCase::Uppercase),
    };
    row()
        .key(header_key(id))
        .w_full()
        .h(20.0)
        .mt(6.0)
        .px(6.0)
        .gap(10.0)
        .center_y()
        .on_secondary_press(move |s: &mut App, at, _| s.overlay = Overlay::SectionMenu(id, at))
        .child(label)
        .child(el().grow(1.0).h(look.hair).fill(look.hairline))
}

fn card(s: &App, note: &Note, look: &Look, width: Option<f32>, geometry: Option<Rc<Geometry>>) -> El {
    let id = note.id;
    let selected = s.is_selected(id);
    let card = row()
        .key(card_key(id))
        .w_full()
        .px(12.0)
        .py(11.0)
        .gap(10.0)
        .items_start()
        .radius(14.0)
        .fill(if selected { look.selected } else { look.card })
        .on_drag(move |s: &mut App, d: &mut Drag, tx: &mut Tx| drag(s, id, d, tx, geometry.as_deref()))
        .on_secondary_press(move |s: &mut App, at, _| s.open_note_menu(id, at))
        .child(checkbox(note, look))
        .child(body(note, look, width));
    let card = if selected {
        card.border(1.0, look.accent.alpha(0.75))
    } else {
        card.border(look.hair, look.card_border).shadow(1.0, 2.0, look.shadow)
    };
    match s.dragging {
        Some(d) if d.id == id => card.raise().offset(0.0, d.dy).box_shadow(Shadow {
            y: 6.0,
            blur: 18.0,
            color: look.shadow.alpha(0.2),
            ..Shadow::default()
        }),
        _ => card,
    }
}

fn drag(s: &mut App, id: NoteId, d: &Drag, tx: &mut Tx, geometry: Option<&Geometry>) {
    match d.phase {
        Phase::Down => {}
        Phase::Move if s.dragging.is_some() || d.dx.hypot(d.dy) > 4.0 => {
            let target = geometry.and_then(|g| g.target(g.origin + d.start_y + d.dy));
            s.drag(id, d.dy, target);
        }
        Phase::Move => {}
        Phase::Up if d.cancelled() => s.dragging = None,
        Phase::Up if s.dragging.is_some() => s.drop(),
        Phase::Up => {
            s.click(id, Click::from_mods(tx.mods()));
            tx.blur();
        }
    }
}

fn checkbox(note: &Note, look: &Look) -> El {
    let id = note.id;
    let check = el()
        .key("check")
        .role("checkbox")
        .label("Done")
        .checked(note.done)
        .size(18.0, 18.0)
        .radius(9.0)
        .center()
        .cursor(Cursor::Pointer)
        .on_press(move |s: &mut App| _ = s.toggle(&[id]));
    if note.done {
        check.fill(look.accent).child(icon(&icons::CHECK, 14.0).color(look.on_accent))
    } else {
        check.border(1.25, look.ring)
    }
}

fn body(note: &Note, look: &Look, width: Option<f32>) -> El {
    let styled = inline::parse(&note.text);
    let spans = styled
        .marks
        .iter()
        .map(|(range, mark)| {
            let span = Span::new(range.start, range.end);
            match mark {
                Mark::Bold => span.bold(),
                Mark::Italic => Span { italic: true, ..span },
                Mark::Code => Span { mono: true, size: Some(12.0), bg: Some(look.code), bg_pad: 2.0, ..span },
            }
        })
        .collect::<Vec<_>>();
    let strikes = match (note.done, width) {
        (true, Some(width)) => {
            let code: Vec<_> = styled.marks.iter().filter(|(_, m)| *m == Mark::Code).map(|(r, _)| r.clone()).collect();
            strike::struck(&styled.text, &spans, BODY, width, &code)
        }
        _ => Vec::new(),
    };
    let body = rich(styled.text, spans).key("text").font(BODY).max_lines(3).grow(1.0).shrink().min_w(0.0);
    let body = body.strikes(strikes.into_iter().map(|r| (r.start, r.end, None)));
    body.color(if note.done { look.faint } else { look.text })
}
