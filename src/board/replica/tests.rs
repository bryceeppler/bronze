use super::*;

fn stamp(rng: &mut fastrand::Rng) -> Stamp {
    Stamp { time: rng.u64(..3), device: DeviceId(rng.u64(..2)) }
}

fn random_replica(rng: &mut fastrand::Rng, ranks: &[Rank]) -> Replica {
    let mut replica = Replica::default();
    for _ in 0..rng.usize(..6) {
        let record = match rng.u8(..4) {
            0 => NoteRecord::Removed,
            _ => NoteRecord::Live {
                text: ["a", "b", "c"][rng.usize(..3)].to_owned(),
                done: Reg { value: rng.bool(), at: stamp(rng) },
                place: Reg {
                    value: Position {
                        section: rng.bool().then(|| SectionId(rng.u128(..3))),
                        rank: ranks[rng.usize(..ranks.len())].clone(),
                    },
                    at: stamp(rng),
                },
            },
        };
        replica.notes.insert(NoteId(rng.u128(..6)), record);
    }
    for _ in 0..rng.usize(..4) {
        let record = match rng.u8(..4) {
            0 => SectionRecord::Removed,
            _ => SectionRecord::Live {
                name: Reg { value: ["Work", "work", "Home"][rng.usize(..3)].to_owned(), at: stamp(rng) },
                created: stamp(rng),
            },
        };
        replica.sections.insert(SectionId(rng.u128(..3)), record);
    }
    replica
}

fn joined(a: &Replica, b: &Replica) -> Replica {
    let mut out = a.clone();
    out.join(b);
    out
}

#[test]
fn join_is_commutative_associative_and_idempotent_over_random_replicas() {
    for seed in 0..20_000 {
        let mut rng = fastrand::Rng::with_seed(seed);
        let ranks: Vec<Rank> = std::iter::successors(Some(Rank::between(None, None, &mut rng)), |r| {
            Some(Rank::between(Some(r), None, &mut rng))
        })
        .take(3)
        .collect();
        let [a, b, c] = std::array::from_fn(|_| random_replica(&mut rng, &ranks));
        assert_eq!(joined(&a, &b), joined(&b, &a), "commutative, seed {seed}");
        assert_eq!(joined(&joined(&a, &b), &c), joined(&a, &joined(&b, &c)), "associative, seed {seed}");
        assert_eq!(joined(&a, &a), a, "idempotent, seed {seed}");
    }
}
