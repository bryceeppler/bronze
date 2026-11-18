use super::*;

fn seeded() -> fastrand::Rng {
    fastrand::Rng::with_seed(7)
}

#[test]
fn a_rank_made_anywhere_in_a_list_keeps_the_list_in_order() {
    let mut rng = seeded();
    let mut list: Vec<Rank> = Vec::new();
    for _ in 0..5_000 {
        let at = rng.usize(..=list.len());
        let rank = Rank::between(at.checked_sub(1).map(|i| &list[i]), list.get(at), &mut rng);
        list.insert(at, rank);
    }
    assert!(list.windows(2).all(|pair| pair[0] < pair[1]));
}

#[test]
fn appending_or_prepending_ten_thousand_notes_keeps_every_rank_one_group_long() {
    let mut rng = seeded();
    let (mut first, mut last) = (Rank::between(None, None, &mut rng), None::<Rank>);
    for _ in 0..10_000 {
        first = Rank::between(None, Some(&first), &mut rng);
        let next = Rank::between(last.as_ref().or(Some(&first)), None, &mut rng);
        last = Some(next);
    }
    assert_eq!(String::from(first).len(), 16);
    assert_eq!(last.map(|rank| String::from(rank).len()), Some(16));
}

#[test]
fn neighbors_out_of_order_place_the_rank_after_the_lower_one() {
    let mut rng = seeded();
    let lo = Rank::between(None, None, &mut rng);
    let tie = Rank::between(Some(&lo), Some(&lo), &mut rng);
    assert!(tie > lo);
    let below = Rank::between(None, Some(&lo), &mut rng);
    assert!(Rank::between(Some(&lo), Some(&below), &mut rng) > lo);
}

#[test]
fn a_rank_reads_back_only_from_whole_groups_ending_in_a_nonzero_one() {
    let mut rng = seeded();
    let rank = Rank::between(Some(&Rank::between(None, None, &mut rng)), None, &mut rng);
    assert_eq!(Rank::try_from(String::from(rank.clone())), Ok(rank));
    for bad in ["", "8000", "800000000000000g", "80000000000000000000000000000000", "8000000000000000é"] {
        assert!(Rank::try_from(bad.to_owned()).is_err(), "{bad:?} is not a rank");
    }
}
