use std::collections::{BTreeMap, BTreeSet};

use super::*;

const MACHINES: usize = 4;
const STEPS: usize = 400;
const SEEDS: u64 = 300;
const NAMES: [&str; 5] = ["Research", "research", "Errands", "ERRANDS", "Later"];

type Shown = Vec<(Option<(SectionId, String)>, Vec<Note>)>;

fn shown(board: &Board) -> Shown {
    board
        .lists(Scope::All)
        .iter()
        .map(|l| {
            let section = match l.owner {
                Owner::Loose => None,
                Owner::Section(id) => Some((id, l.name.unwrap_or_default().to_owned())),
            };
            (section, l.notes.to_vec())
        })
        .collect()
}

fn texts(board: &Board) -> BTreeMap<NoteId, String> {
    board.notes().map(|n| (n.id, n.text.clone())).collect()
}

enum Outcome {
    Keeps,
    Removes,
    Merges,
}

fn lands(board: &Board, id: NoteId, to: Place) -> bool {
    let lists = board.lists(Scope::All);
    match to {
        Place::Before(other) if other == id => true,
        Place::Before(other) => lists.iter().any(|l| l.notes.windows(2).any(|w| (w[0].id, w[1].id) == (id, other))),
        Place::End(owner) => lists.iter().any(|l| l.owner == owner && l.notes.last().is_some_and(|n| n.id == id)),
    }
}

struct World {
    seed: u64,
    rng: fastrand::Rng,
    boards: Vec<Board>,
    typed: Vec<String>,
    made: BTreeMap<NoteId, String>,
    removed: BTreeMap<NoteId, String>,
    merged: BTreeSet<NoteId>,
}

impl World {
    fn new(seed: u64) -> World {
        let mut rng = fastrand::Rng::with_seed(seed);
        let boards = (0..MACHINES).map(|_| Board::seeded(rng.u64(..))).collect();
        World {
            seed,
            rng,
            boards,
            typed: Vec::new(),
            made: BTreeMap::new(),
            removed: BTreeMap::new(),
            merged: BTreeSet::new(),
        }
    }

    fn step(&mut self, step: usize) {
        let (seed, at) = (self.seed, self.rng.usize(..MACHINES));
        if self.rng.u8(..4) == 0 {
            let from = self.boards[self.rng.usize(..MACHINES)].replica().clone();
            let before = shown(&self.boards[at]);
            let changed = self.boards[at].join(&from);
            assert_eq!(changed, shown(&self.boards[at]) != before, "join reports a change, seed {seed} step {step}");
            return;
        }
        let before = texts(&self.boards[at]);
        let outcome = self.edit(at, step);
        let after = texts(&self.boards[at]);
        for (id, text) in before.into_iter().filter(|(id, _)| !after.contains_key(id)) {
            match outcome {
                Outcome::Removes => _ = self.removed.insert(id, text),
                Outcome::Merges => _ = self.merged.insert(id),
                Outcome::Keeps => panic!("{text:?} vanished on an edit that keeps notes, seed {seed} step {step}"),
            }
        }
        for (id, text) in after {
            self.made.entry(id).or_insert(text);
        }
    }

    fn edit(&mut self, at: usize, step: usize) -> Outcome {
        let rng = &mut self.rng;
        let board = &mut self.boards[at];
        let notes: Vec<NoteId> = board.notes().map(|n| n.id).collect();
        let sections: Vec<SectionId> = board.sections().iter().map(|s| s.id).collect();
        let pick = |rng: &mut fastrand::Rng, count: std::ops::RangeInclusive<usize>| -> Vec<NoteId> {
            (0..rng.usize(count)).filter_map(|_| rng.choice(&notes).copied()).collect()
        };
        match rng.u8(..10) {
            0 => {
                self.typed.push(format!("note <{step}>"));
                board.submit(&format!("note <{step}>"));
            }
            1 => {
                self.typed.push(format!("<{step}>"));
                board.capture(format!("  captured <{step}>\nsecond line\n"));
            }
            2 => {
                board.submit(&format!("# {}", NAMES[rng.usize(..NAMES.len())]));
            }
            3 => board.toggle(&pick(rng, 1..=3)),
            4 => {
                board.remove(&pick(rng, 1..=2));
                return Outcome::Removes;
            }
            5 => {
                board.merge(&pick(rng, 2..=3));
                return Outcome::Merges;
            }
            6 => {
                let Some(&id) = rng.choice(&notes) else { return Outcome::Keeps };
                let to = match (rng.bool(), rng.choice(&notes)) {
                    (true, Some(&other)) => Place::Before(other),
                    _ => Place::End(rng.choice(&sections).map_or(Owner::Loose, |&s| Owner::Section(s))),
                };
                board.move_note(id, to);
                assert!(
                    lands(board, id, to),
                    "a moved note lands where it was dropped, seed {} step {step}",
                    self.seed
                );
            }
            7 => {
                board.clear_done(if rng.bool() { Scope::All } else { Scope::Active });
                return Outcome::Removes;
            }
            8 => match rng.choice(&sections) {
                Some(&id) if rng.bool() => _ = board.rename_section(id, NAMES[rng.usize(..NAMES.len())]),
                Some(&id) => board.remove_section(id),
                None => {}
            },
            _ => match rng.choice(&sections) {
                Some(&id) => board.activate(id),
                None => board.deactivate(),
            },
        }
        Outcome::Keeps
    }

    fn join_all(&mut self) {
        let copies: Vec<Replica> = self.boards.iter().map(|b| b.replica().clone()).collect();
        for board in &mut self.boards {
            for copy in &copies {
                board.join(copy);
            }
        }
    }

    fn check(&mut self) {
        let seed = self.seed;
        let outline = shown(&self.boards[0]);
        for board in &self.boards[1..] {
            assert_eq!(shown(board), outline, "every machine shows the same outline, seed {seed}");
        }
        let everything = self.boards[0].replica().clone();
        assert!(self.boards.iter_mut().all(|b| !b.join(&everything)), "joining again changes nothing, seed {seed}");

        let present = texts(&self.boards[0]);
        for id in self.removed.keys().chain(&self.merged) {
            assert!(!present.contains_key(id), "a removed or merged note came back, seed {seed}");
        }
        for (id, text) in self.made.iter().filter(|(id, _)| !self.removed.contains_key(id) && !self.merged.contains(id))
        {
            assert_eq!(present.get(id), Some(text), "a note nobody removed is shown with its text, seed {seed}");
        }
        for typed in &self.typed {
            let kept = present.values().chain(self.removed.values()).any(|text| text.contains(typed.as_str()));
            assert!(kept, "{typed:?} is shown, or was in a note someone removed, seed {seed}");
        }
    }
}

#[test]
fn machines_editing_and_joining_at_random_converge_without_losing_or_reviving_notes() {
    let seeds = match std::env::var("BRONZE_SIM_SEED") {
        Ok(seed) => {
            let seed = seed.parse().expect("BRONZE_SIM_SEED is a number");
            seed..seed + 1
        }
        Err(_) => 0..SEEDS,
    };
    for seed in seeds {
        let mut world = World::new(seed);
        for step in 0..STEPS {
            world.step(step);
        }
        world.join_all();
        world.check();
    }
}
