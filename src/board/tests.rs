use super::*;

fn texts(notes: &[Note]) -> Vec<&str> {
    notes.iter().map(|n| n.text.as_str()).collect()
}

fn added(submitted: Submitted) -> NoteId {
    match submitted {
        Submitted::Added(id) => id,
        other => panic!("expected a note, got {other:?}"),
    }
}

#[test]
fn a_heading_opens_a_section_and_later_notes_land_in_it() {
    let mut board = Board::default();
    added(board.submit("loose thought"));
    let Submitted::Activated(research) = board.submit("# Research") else { panic!("expected a section") };
    added(board.submit("  read the paper  "));

    assert_eq!(texts(board.loose()), ["loose thought"]);
    assert_eq!(board.section(research).map(|s| texts(&s.notes)), Some(vec!["read the paper"]));
    assert_eq!(board.active().map(|s| s.name.as_str()), Some("Research"));
}

#[test]
fn a_heading_finds_an_existing_section_ignoring_case() {
    let mut board = Board::default();
    let first = board.submit("# Research");
    board.submit("# Errands");
    assert_eq!(board.submit("# research"), first);
    assert_eq!(board.sections().len(), 2);
    assert_eq!(board.active().map(|s| s.name.as_str()), Some("Research"));
}

#[test]
fn blank_input_and_non_headings_are_not_section_commands() {
    let mut board = Board::default();
    assert_eq!(board.submit("  \n "), Submitted::Empty);
    added(board.submit("#hashtag"));
    added(board.submit("# two\nlines"));
    assert!(board.sections().is_empty());
    assert_eq!(texts(board.loose()), ["#hashtag", "# two\nlines"]);
}

#[test]
fn a_capture_is_kept_verbatim_even_when_it_looks_like_a_heading() {
    let mut board = Board::default();
    board.capture("# Not a section\n");
    assert!(board.sections().is_empty());
    assert_eq!(texts(board.loose()), ["# Not a section\n"]);
}

#[test]
fn toggling_a_mixed_set_marks_all_done_then_all_undone() {
    let mut board = Board::default();
    let a = added(board.submit("a"));
    let b = added(board.submit("b"));
    board.toggle(&[a]);
    board.toggle(&[a, b]);
    assert!(board.notes().all(|n| n.done));
    board.toggle(&[a, b]);
    assert!(board.notes().all(|n| !n.done));
}

#[test]
fn merging_folds_notes_into_the_first_in_board_order() {
    let mut board = Board::default();
    let a = added(board.submit("first"));
    board.submit("# Later");
    let b = added(board.submit("second"));
    let c = added(board.submit("third"));

    board.toggle(&[a, c]);
    let merged = board.merge(&[c, a, b]);
    assert_eq!(merged, board.loose().first().map(|n| n.id));
    assert_eq!(texts(board.loose()), ["first\n\nsecond\n\nthird"]);
    assert!(!board.loose()[0].done, "an open note keeps the merge open");
    assert!(board.active().is_some_and(|s| s.notes.is_empty()));
    assert_eq!(board.merge(merged.as_slice()), None, "one note has nothing to merge with");
}

#[test]
fn copy_text_follows_board_order_not_selection_order() {
    let mut board = Board::default();
    let a = added(board.submit("alpha"));
    let b = added(board.submit("beta"));
    assert_eq!(board.copy_text(&[b, a]), "alpha\n\nbeta");
}

#[test]
fn moving_a_note_reorders_within_and_across_lists() {
    let mut board = Board::default();
    let a = added(board.submit("a"));
    let b = added(board.submit("b"));
    let c = added(board.submit("c"));
    board.move_note(c, Place::Before(a));
    assert_eq!(texts(board.loose()), ["c", "a", "b"]);
    board.move_note(c, Place::Before(b));
    assert_eq!(texts(board.loose()), ["a", "c", "b"]);

    let Submitted::Activated(s) = board.submit("# S") else { panic!() };
    board.move_note(a, Place::End(Owner::Section(s)));
    assert_eq!(texts(board.loose()), ["c", "b"]);
    assert_eq!(board.section(s).map(|s| texts(&s.notes)), Some(vec!["a"]));
}

#[test]
fn unknown_ids_change_nothing() {
    let mut board = Board::default();
    let a = added(board.submit("a"));
    let mut other = Board::default();
    other.submit("x");
    other.submit("y");
    let stranger = added(other.submit("z"));
    let Submitted::Activated(ghost) = other.submit("# Ghost") else { panic!() };

    board.toggle(&[stranger]);
    board.remove(&[stranger]);
    board.move_note(a, Place::Before(stranger));
    board.move_note(a, Place::End(Owner::Section(ghost)));
    board.move_note(stranger, Place::End(Owner::Loose));
    board.activate(ghost);
    board.remove_section(ghost);
    assert!(!board.rename_section(ghost, "Boo"));

    assert_eq!(texts(board.loose()), ["a"]);
    assert!(!board.loose()[0].done);
    assert!(board.active().is_none());
}

#[test]
fn clear_done_respects_the_scope() {
    let mut board = Board::default();
    let loose = added(board.submit("loose"));
    board.submit("# S");
    let inside = added(board.submit("inside"));
    board.toggle(&[loose, inside]);

    board.clear_done(Scope::Active);
    assert_eq!(texts(board.loose()), ["loose"]);
    assert!(board.active().is_some_and(|s| s.notes.is_empty()));

    board.clear_done(Scope::All);
    assert!(board.notes().next().is_none());
}

#[test]
fn active_scope_lists_only_the_active_section() {
    let mut board = Board::default();
    board.submit("loose");
    board.submit("# A");
    board.submit("# B");
    let names = |scope| board.lists(scope).iter().map(|l| l.name).collect::<Vec<_>>();
    assert_eq!(names(Scope::All), [None, Some("A"), Some("B")]);
    assert_eq!(names(Scope::Active), [Some("B")]);
}

#[test]
fn renaming_refuses_blank_and_taken_names() {
    let mut board = Board::default();
    let Submitted::Activated(a) = board.submit("# A") else { panic!() };
    board.submit("# B");
    assert!(!board.rename_section(a, "  "));
    assert!(!board.rename_section(a, "b"));
    assert!(board.rename_section(a, "  Alpha "));
    assert!(board.rename_section(a, "ALPHA"));
    assert_eq!(board.section(a).map(|s| s.name.as_str()), Some("ALPHA"));
}

#[test]
fn removing_a_section_moves_its_notes_to_the_loose_list() {
    let mut board = Board::default();
    board.submit("loose");
    let Submitted::Activated(s) = board.submit("# S") else { panic!() };
    board.submit("kept");
    board.remove_section(s);
    assert!(board.sections().is_empty());
    assert!(board.active().is_none());
    assert_eq!(texts(board.loose()), ["loose", "kept"]);
    added(board.submit("after"));
    assert_eq!(texts(board.loose()), ["loose", "kept", "after"]);
}

#[test]
fn ids_are_never_reused() {
    let mut board = Board::default();
    let a = added(board.submit("a"));
    board.remove(&[a]);
    let b = added(board.submit("b"));
    assert_ne!(a, b);
}

type Outline = Vec<(Option<String>, Vec<(String, bool)>)>;

fn outline(board: &Board) -> Outline {
    board
        .lists(Scope::All)
        .iter()
        .map(|l| (l.name.map(str::to_owned), l.notes.iter().map(|n| (n.text.clone(), n.done)).collect()))
        .collect()
}

fn named(name: &str, notes: &[(&str, bool)]) -> (Option<String>, Vec<(String, bool)>) {
    (Some(name.to_owned()), notes.iter().map(|&(text, done)| (text.to_owned(), done)).collect())
}

fn sync(a: &mut Board, b: &mut Board) {
    a.join(b.replica());
    b.join(a.replica());
}

fn id_of(board: &Board, text: &str) -> NoteId {
    board.notes().find(|n| n.text == text).map(|n| n.id).unwrap_or_else(|| panic!("no note {text:?}"))
}

#[test]
fn boards_that_join_each_other_show_the_same_outline_and_keep_their_own_active_section() {
    let (mut mac, mut linux) = (Board::seeded(1), Board::seeded(2));
    mac.submit("from the mac");
    mac.submit("# Errands");
    linux.submit("from linux");

    assert!(mac.join(linux.replica()), "a new note changes the view");
    assert!(!mac.join(linux.replica()), "joining the same copy again changes nothing");
    linux.join(mac.replica());

    assert_eq!(outline(&mac), outline(&linux));
    assert_eq!(texts(mac.loose()).len(), 2);
    assert_eq!(mac.active().map(|s| s.name.as_str()), Some("Errands"));
    assert!(linux.active().is_none());
}

#[test]
fn a_delete_beats_a_check_or_move_made_at_the_same_time_elsewhere() {
    let (mut mac, mut linux) = (Board::seeded(1), Board::seeded(2));
    for text in ["a", "b", "c"] {
        mac.submit(text);
    }
    sync(&mut mac, &mut linux);
    let b = id_of(&mac, "b");

    mac.remove(&[b]);
    linux.toggle(&[b]);
    linux.move_note(b, Place::Before(id_of(&linux, "a")));
    sync(&mut mac, &mut linux);

    assert_eq!(texts(mac.loose()), ["a", "c"]);
    assert_eq!(outline(&mac), outline(&linux));
}

#[test]
fn merging_keeps_the_text_of_a_picked_note_deleted_elsewhere_at_the_same_time() {
    let (mut mac, mut linux) = (Board::seeded(1), Board::seeded(2));
    mac.submit("keep");
    mac.submit("deleted on the mac");
    sync(&mut mac, &mut linux);

    mac.remove(&[id_of(&mac, "deleted on the mac")]);
    let picked = [id_of(&linux, "keep"), id_of(&linux, "deleted on the mac")];
    linux.merge(&picked);
    sync(&mut mac, &mut linux);

    assert_eq!(texts(mac.loose()), ["keep\n\ndeleted on the mac"]);
    assert_eq!(outline(&mac), outline(&linux));
}

#[test]
fn sections_named_alike_on_two_machines_show_as_one_that_renames_and_removes_as_one() {
    let (mut mac, mut linux) = (Board::seeded(1), Board::seeded(2));
    mac.submit("# Research");
    mac.submit("from the mac");
    linux.submit("first on linux");
    linux.submit("# research");
    linux.submit("from linux");
    sync(&mut mac, &mut linux);

    let expected = named("Research", &[("from the mac", false), ("from linux", false)]);
    assert_eq!(outline(&linux)[1..], [expected]);
    assert_eq!(linux.active().map(|s| s.name.as_str()), Some("Research"));
    linux.submit("after the join");
    mac.join(linux.replica());
    assert_eq!(mac.sections().len(), 1);
    assert_eq!(texts(&mac.sections()[0].notes), ["from the mac", "from linux", "after the join"]);

    let shown = mac.sections()[0].id;
    assert!(mac.rename_section(shown, "Reading"));
    linux.join(mac.replica());
    assert_eq!(linux.sections().iter().map(|s| s.name.as_str()).collect::<Vec<_>>(), ["Reading"]);

    linux.remove_section(linux.sections()[0].id);
    mac.join(linux.replica());
    assert!(mac.sections().is_empty());
    assert_eq!(texts(mac.loose()), ["first on linux", "from the mac", "from linux", "after the join"]);
}

#[test]
fn a_note_added_to_a_section_removed_elsewhere_shows_as_loose() {
    let (mut mac, mut linux) = (Board::seeded(1), Board::seeded(2));
    mac.submit("loose");
    mac.submit("# Drafts");
    sync(&mut mac, &mut linux);

    mac.remove_section(mac.sections()[0].id);
    let drafts = linux.sections()[0].id;
    linux.activate(drafts);
    linux.submit("written into drafts");
    sync(&mut mac, &mut linux);

    assert!(mac.sections().is_empty());
    let mut loose = texts(mac.loose());
    loose.sort_unstable();
    assert_eq!(loose, ["loose", "written into drafts"]);
    assert_eq!(outline(&mac), outline(&linux));
    assert!(linux.active().is_none());
}
