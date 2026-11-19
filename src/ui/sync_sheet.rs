use rune::tok::type_;
use rune::{
    Cursor, El, Family, Icon, ScrollSpec, Shadow, Span, TextStyle, Tx, auto_grow, col, el, icon, rich, row, spacer,
    text, text_input,
};

use super::app::{App, Overlay, SyncSheet};
use super::chrome::{BODY, clear_of_caret};
use super::icons;
use super::look::Look;
use crate::inline::{self, Mark};
use crate::sync::{Offline, Refusal, Setup, SetupRefusal, Status, SyncCode};

const PROSE: TextStyle = TextStyle::new(12.5, 400.0).lh(17.0);
const HELP: TextStyle = TextStyle::new(12.0, 400.0).lh(16.0);
const CODE: TextStyle = TextStyle::new(12.0, 400.0).lh(17.0).family(Family::SystemMono);
const CHECK_DIGITS: usize = 8;

const FIELD_KEY: &str = "sync-field";
const CODE_KEY: &str = "sync-code";
const NOTE_KEY: &str = "sync-note";

enum Reading {
    Blank,
    Parsed(Setup),
    Refused { refusal: SetupRefusal, joining: bool },
}

#[derive(Clone, Copy)]
enum Dot {
    Hollow,
    Lit,
    Alarm,
}

struct Condition {
    dot: Dot,
    headline: String,
    advice: Option<String>,
}

#[derive(Clone, Copy)]
enum Ink {
    Primary,
    Plain,
}

pub fn view(s: &App, sheet: &SyncSheet, look: &Look) -> El {
    match (sheet, s.sync_code()) {
        (SyncSheet::NewCode { copied }, Some(code)) => new_code(code, *copied, look),
        (SyncSheet::Linked { copied, failure }, Some(code)) => {
            linked(&s.sync_status(), code, *copied, failure.as_deref(), look)
        }
        (SyncSheet::Entry { failure }, _) => entry(s, failure.as_deref(), look),
        (SyncSheet::NewCode { .. } | SyncSheet::Linked { .. }, None) => entry(s, None, look),
    }
}

pub fn summary(status: &Status) -> &'static str {
    match status {
        Status::Off => "\u{25CB}  Off",
        Status::Connecting => "\u{25CB}  Connecting",
        Status::Synced => "\u{25CF}  Synced",
        Status::Offline(_) | Status::Refused(_) => "\u{25CF}  Not synced",
    }
}

impl Reading {
    fn of(typed: &str) -> Reading {
        let started = typed.trim().to_ascii_lowercase();
        if ["https://", "http://"].iter().any(|scheme| scheme.starts_with(&started)) {
            return Reading::Blank;
        }
        match Setup::parse(typed) {
            Ok(setup) => Reading::Parsed(setup),
            Err(refusal) => Reading::Refused { refusal, joining: typed.contains('#') },
        }
    }

    fn action(&self) -> (&'static str, bool) {
        match self {
            Reading::Blank => ("Continue", false),
            Reading::Parsed(Setup::Start(_)) => ("Start New Vault", true),
            Reading::Parsed(Setup::Join(_)) => ("Join Vault", true),
            Reading::Refused { joining: true, .. } => ("Join Vault", false),
            Reading::Refused { joining: false, .. } => ("Start New Vault", false),
        }
    }
}

fn refusal_sentence(refusal: SetupRefusal) -> &'static str {
    match refusal {
        SetupRefusal::NotAddress => {
            "This is not a server address or a sync code. A server address starts with https://."
        }
        SetupRefusal::NotSecure => "Use https. Plain http works only for localhost.",
        SetupRefusal::KeyMalformed => {
            "This code is cut short or has extra characters. Copy the whole code again from the other machine."
        }
        SetupRefusal::KeyCheckFails => {
            "This code has a typo somewhere, so its check doesn't match. Copy it again from the other machine."
        }
    }
}

fn condition(status: &Status, host: &str) -> Condition {
    let (dot, headline, advice) = match status {
        Status::Off | Status::Connecting => (Dot::Hollow, format!("Connecting to {host}"), None),
        Status::Synced => (Dot::Lit, format!("Synced with {host}"), None),
        Status::Offline(Offline::Unreachable(_)) => (
            Dot::Alarm,
            format!("Can't reach {host}"),
            Some(
                "New notes stay on this machine and sync once it is back. Check the address and that bronze-sync is running."
                    .to_owned(),
            ),
        ),
        Status::Offline(Offline::ServerError(code)) => (
            Dot::Alarm,
            format!("{host} had a problem ({code})"),
            Some("New notes stay on this machine and sync once the server recovers.".to_owned()),
        ),
        Status::Offline(Offline::Unexpected(code)) => (
            Dot::Alarm,
            format!("{host} gave an answer Bronze doesn't expect ({code})"),
            Some(
                "Check that the address points at bronze-sync. New notes stay on this machine until it answers."
                    .to_owned(),
            ),
        ),
        Status::Refused(Refusal::WrongKey) => (
            Dot::Alarm,
            format!("{host} doesn't accept this key"),
            Some(
                "Check that the code came from a machine that syncs with this server. To use another code, stop syncing and join again."
                    .to_owned(),
            ),
        ),
        Status::Refused(Refusal::Declined(why)) => (
            Dot::Alarm,
            format!("{host} declined"),
            Some(format!(
                "The server says: {why}. Ask whoever runs it, or stop syncing and use another server."
            )),
        ),
        Status::Refused(Refusal::TooLarge) => (
            Dot::Alarm,
            format!("Your notes are too large for {host}"),
            Some(
                "Delete notes you no longer need, or ask whoever runs the server to raise BRONZE_SYNC_MAX_BYTES."
                    .to_owned(),
            ),
        ),
        Status::Refused(Refusal::NewerFormat) => (
            Dot::Alarm,
            format!("A newer Bronze wrote the copy on {host}"),
            Some("Update Bronze on this machine to keep syncing. Until then it sends nothing.".to_owned()),
        ),
        Status::Refused(Refusal::Damaged) => (
            Dot::Alarm,
            format!("The copy on {host} is damaged"),
            Some(
                "Bronze won't overwrite it, and every note stays on this machine. Restore the server from a backup, or stop syncing and start a new vault."
                    .to_owned(),
            ),
        ),
    };
    Condition { dot, headline, advice }
}

fn host(address: impl ToString) -> String {
    let address = address.to_string();
    match address.split_once("://") {
        Some((_, rest)) => rest.to_owned(),
        None => address,
    }
}

fn entry(s: &App, failure: Option<&str>, look: &Look) -> El {
    let typed = s.overlay_field.text.as_str();
    let reading = Reading::of(typed);
    let refused = matches!(reading, Reading::Refused { .. });
    let field = text_input(&s.overlay_field, if typed.contains('#') { CODE } else { BODY })
        .key(FIELD_KEY)
        .label("Sync code or server address")
        .w_full()
        .wrap_anywhere()
        .color(look.text)
        .placeholder(clear_of_caret("Sync code or server address"), look.faint)
        .on_edit(|s: &mut App| &mut s.overlay_field)
        .on_submit(|s: &mut App, _: &mut Tx| s.submit_sync());
    let frame = col()
        .key("sync-field-box")
        .w_full()
        .min_h(32.0)
        .px(10.0)
        .py(6.0)
        .radius(9.0)
        .fill(look.field)
        .cursor(Cursor::Text)
        .on_press_tx(|_: &mut App, tx: &mut Tx| tx.focus(FIELD_KEY, false))
        .child(auto_grow(field, 1, 4));
    let frame = match (refused, s.overlay_field.focused) {
        (true, _) => frame.border(1.0, look.danger).box_shadow(ring(look.danger.alpha(0.14))),
        (false, true) => frame.border(1.0, look.accent).box_shadow(ring(look.accent.alpha(0.18))),
        (false, false) => frame.border(look.hair, look.field_border),
    };
    let mut body = vec![col().gap(5.0).child(frame).children(match &reading {
        Reading::Parsed(Setup::Start(_)) => Some(help("Use https. Plain http works only for localhost.", look.faint)),
        Reading::Refused { refusal, .. } => Some(help(refusal_sentence(*refusal), look.danger).key(NOTE_KEY)),
        Reading::Blank | Reading::Parsed(Setup::Join(_)) => None,
    })];
    match &reading {
        Reading::Blank => {
            body.push(prose(
                "**Joining?** Paste the sync code from a machine that already syncs.\n**Starting fresh?** Enter your bronze-sync server address.",
                look,
            ));
            body.push(prose(
                "Notes are encrypted on this machine before they leave. The server stores them but can't read them.",
                look,
            ));
        }
        Reading::Parsed(Setup::Start(server)) => {
            body.push(status_line(
                Dot::Hollow,
                prose(&format!("Starts a new vault on **{}**", host(server)), look),
                look,
            ));
        }
        Reading::Parsed(Setup::Join(code)) => {
            body.push(status_line(
                Dot::Lit,
                prose(&format!("Joins your vault on **{}**", host(&code.server)), look),
                look,
            ));
            body.push(prose("Notes already on this machine are added to the vault. Nothing is lost.", look));
        }
        Reading::Refused { .. } => {}
    }
    if let Some(failure) = failure {
        body.push(help(&format!("Bronze couldn't save the sync settings: {failure}."), look.danger));
    }
    let (label, ready) = reading.action();
    let go = button("sync-go", label, None, Ink::Primary, look);
    let go = if ready { go.default_button().on_press(|s: &mut App| s.submit_sync()) } else { unavailable(go) };
    frame_sheet("Sync", body, row().w_full().gap(8.0).center_y().child(spacer()).child(go), look)
}

fn new_code(code: &SyncCode, copied: bool, look: &Look) -> El {
    let warning = row()
        .key("sync-warning")
        .gap(8.0)
        .items_start()
        .px(10.0)
        .py(9.0)
        .radius(10.0)
        .fill(look.field)
        .border(look.hair, look.field_border)
        .child(icon(&icons::KEY, 14.0).color(look.accent).mt(1.0))
        .child(
            styled(
                "**Save this code now.** It is the only way into your notes. There is no account or password, so nobody can reset it.",
                HELP,
                look.muted,
                look,
            )
            .grow(1.0)
            .shrink(),
        );
    let copy = copy_button(if copied { "Copied" } else { "Copy Code" }, Ink::Primary, look);
    let body = vec![
        prose(
            "Paste this code on every other machine you want to sync. It holds the server address and your key.",
            look,
        ),
        col().px(10.0).py(8.0).radius(9.0).fill(look.code).child(revealed(code, look)),
        row().child(copy),
        warning,
    ];
    let done = button("sync-done", "Done", None, Ink::Plain, look)
        .default_button()
        .on_press(|s: &mut App| s.overlay = Overlay::None);
    frame_sheet("Your Sync Code", body, row().w_full().center_y().child(spacer()).child(done), look)
}

fn linked(status: &Status, code: &SyncCode, copied: bool, failure: Option<&str>, look: &Look) -> El {
    let Condition { dot, headline, advice } = condition(status, &host(&code.server));
    let headline = text(headline).key("sync-status").font(type_::TITLE).color(look.text).wrap().grow(1.0).shrink();
    let mut body = vec![
        col()
            .gap(6.0)
            .child(status_line(dot, headline, look))
            .children(advice.map(|advice| styled(&advice, PROSE, look.muted, look).key(NOTE_KEY))),
    ];
    let shown = row()
        .key("sync-code-box")
        .grow(1.0)
        .shrink()
        .min_w(0.0)
        .min_h(32.0)
        .px(10.0)
        .py(7.0)
        .center_y()
        .radius(9.0)
        .fill(look.field)
        .border(look.hair, look.field_border)
        .child(text(masked(code)).key(CODE_KEY).font(CODE).color(look.text).wrap_anywhere().grow(1.0).shrink());
    body.push(
        col()
            .gap(5.0)
            .child(text("Sync code for your other machines").font(type_::LABEL).color(look.muted))
            .child(row().gap(8.0).center_y().child(shown).child(copy_button(
                if copied { "Copied" } else { "Copy" },
                Ink::Plain,
                look,
            )))
            .child(help("End-to-end encrypted. The server can't read your notes.", look.faint)),
    );
    if let Some(failure) = failure {
        body.push(help(&format!("Bronze couldn't stop syncing: {failure}."), look.danger));
    }
    let stop = button("sync-stop", "Stop Syncing", None, Ink::Plain, look).on_press(|s: &mut App| s.stop_syncing());
    let foot = row()
        .w_full()
        .gap(8.0)
        .center_y()
        .child(help("Stopping keeps every note on this machine.", look.faint).grow(1.0).shrink())
        .child(stop);
    frame_sheet("Sync", body, foot, look)
}

fn frame_sheet(title: &str, body: Vec<El>, foot: El, look: &Look) -> El {
    let rule = || el().w_full().h(look.hair).fill(look.hairline);
    let content = col().key("sync-body").px(14.0).pt(12.0).pb(14.0).gap(12.0).children(body);
    col()
        .key("sync")
        .child(row().h(40.0).px(14.0).center_y().child(text(title).font(type_::TITLE).color(look.text)))
        .child(rule())
        .child(col().shrink().min_h(0.0).scroll(ScrollSpec { stick_bottom: false }).child(content))
        .child(rule())
        .child(row().px(14.0).py(10.0).child(foot))
}

fn revealed(code: &SyncCode, look: &Look) -> El {
    let shown = code.reveal();
    let server = shown.find('#').map_or(0, |at| at + 1);
    let check = shown.len().saturating_sub(CHECK_DIGITS);
    let spans = vec![
        Span { color: Some(look.muted), ..Span::new(0, server) },
        Span { color: Some(look.accent), ..Span::new(check, shown.len()) },
    ];
    rich(shown, spans).key(CODE_KEY).font(CODE).color(look.text).wrap_anywhere()
}

fn masked(code: &SyncCode) -> String {
    let key = code.key.reveal();
    let (head, tail) = (key.split_once('-').map_or("", |(head, _)| head), &key[key.len().saturating_sub(4)..]);
    format!("{}#{head}-\u{2026}{tail}", host(&code.server))
}

fn status_line(dot: Dot, words: El, look: &Look) -> El {
    let mark = el().size(7.0, 7.0).radius(3.5);
    let mark = match dot {
        Dot::Hollow => mark.border(1.25, look.ring),
        Dot::Lit => mark.fill(look.accent),
        Dot::Alarm => mark.fill(look.danger),
    };
    row().gap(8.0).center_y().child(mark).child(words)
}

fn prose(source: &str, look: &Look) -> El {
    styled(source, PROSE, look.muted, look)
}

fn styled(source: &str, font: TextStyle, color: rune::Color, look: &Look) -> El {
    let styled = inline::parse(source);
    let spans = styled
        .marks
        .iter()
        .filter(|(_, mark)| *mark == Mark::Bold)
        .map(|(range, _)| Span { weight: Some(600.0), color: Some(look.text), ..Span::new(range.start, range.end) })
        .collect();
    rich(styled.text, spans).font(font).color(color)
}

fn help(words: &str, color: rune::Color) -> El {
    text(words.to_owned()).font(HELP).color(color).wrap()
}

fn copy_button(label: &str, ink: Ink, look: &Look) -> El {
    button("sync-copy", label, Some(&icons::COPY), ink, look)
        .on_press_tx(|s: &mut App, tx: &mut Tx| s.copy_sync_code(tx))
}

fn button(key: &str, label: &str, glyph: Option<&'static Icon>, ink: Ink, look: &Look) -> El {
    let (fill, edge, words, mark) = match ink {
        Ink::Primary => (look.accent, look.accent, look.on_accent, look.on_accent),
        Ink::Plain => (look.field, look.field_border, look.text, look.muted),
    };
    row()
        .key(key)
        .role("button")
        .label(label)
        .h(26.0)
        .px(12.0)
        .gap(6.0)
        .center_y()
        .radius(13.0)
        .fill(fill)
        .border(look.hair, edge)
        .cursor(Cursor::Pointer)
        .children(glyph.map(|glyph| icon(glyph, 13.0).color(mark)))
        .child(text(label.to_owned()).font(type_::LABEL).color(words))
}

fn unavailable(button: El) -> El {
    button.disabled(true).cursor(Cursor::Default).layer_opacity(0.45)
}

fn ring(color: rune::Color) -> Shadow {
    Shadow { spread: 3.0, color, ..Shadow::default() }
}
