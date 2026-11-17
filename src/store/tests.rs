use super::*;
use crate::board::Owner;

type Outline = Vec<(Option<String>, Vec<(String, bool)>)>;

fn outline(board: &Board) -> (Outline, Option<String>) {
    let lists = board
        .lists(crate::board::Scope::All)
        .iter()
        .map(|l| (l.name.map(str::to_owned), l.notes.iter().map(|n| (n.text.clone(), n.done)).collect()))
        .collect();
    (lists, board.active().map(|s| s.name.clone()))
}

fn sample() -> Board {
    let mut board = Board::default();
    board.submit("loose one");
    board.submit("# Research");
    board.submit("multi\nline `code`");
    board.submit("# Errands");
    board.submit("buy milk");
    let done: Vec<_> = board.notes().filter(|n| n.text == "loose one").map(|n| n.id).collect();
    board.toggle(&done);
    let research = board.sections()[0].id;
    board.activate(research);
    board
}

#[test]
fn a_saved_board_reloads_identically() {
    let dir = tempfile::tempdir().unwrap();
    let (empty, store) = open(dir.path());
    assert!(empty.notes().next().is_none());
    let board = sample();
    store.save(&board);
    store.flush();

    let (reloaded, _) = open(dir.path());
    assert_eq!(outline(&reloaded), outline(&board));
    assert_eq!(reloaded.lists(crate::board::Scope::All)[1].owner, Owner::Section(reloaded.sections()[0].id));
}

#[test]
fn an_unreadable_or_newer_file_is_moved_aside_not_overwritten() {
    for contents in ["{ not json", r#"{"version": 2, "loose": [], "sections": [], "notes": "future"}"#] {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("notes.json");
        fs::write(&path, contents).unwrap();

        let (board, store) = open(dir.path());
        assert!(board.notes().next().is_none());
        store.save(&sample());
        store.flush();

        let aside: Vec<_> = fs::read_dir(dir.path())
            .unwrap()
            .map(|e| e.unwrap().path())
            .filter(|p| p.file_name().is_some_and(|n| n.to_string_lossy().contains("unreadable")))
            .collect();
        assert_eq!(aside.len(), 1, "one file moved aside for {contents:?}");
        assert_eq!(fs::read_to_string(&aside[0]).unwrap(), contents);
        let (reloaded, _) = open(dir.path());
        assert_eq!(outline(&reloaded), outline(&sample()));
    }
}

#[test]
fn the_file_is_pretty_versioned_json() {
    let dir = tempfile::tempdir().unwrap();
    let (_, store) = open(dir.path());
    store.save(&sample());
    store.flush();
    let json: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(dir.path().join("notes.json")).unwrap()).unwrap();
    assert_eq!(json["version"], 1);
    assert_eq!(json["active"], "Research");
    assert_eq!(json["loose"][0], serde_json::json!({"text": "loose one", "done": true}));
    assert_eq!(json["sections"][1]["name"], "Errands");
}
