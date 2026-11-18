#[cfg(test)]
mod tests;

use serde::{Deserialize, Serialize};

const OPEN: u128 = 1 << 64;
const MIDDLE: u128 = 1 << 63;
const STEP: u128 = 1 << 32;

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(into = "String", try_from = "String")]
pub struct Rank(Vec<u64>);

impl Rank {
    pub fn between(lo: Option<&Rank>, hi: Option<&Rank>, rng: &mut fastrand::Rng) -> Rank {
        let mut hi = hi.filter(|hi| lo.is_none_or(|lo| lo < *hi));
        let mut digits = Vec::new();
        loop {
            let at = digits.len();
            let a = lo.map_or(0, |lo| lo.digit(at));
            let b = hi.map_or(OPEN, |hi| hi.digit(at));
            if b - a > 1 {
                digits.push(pick(a, b, rng));
                return Rank(digits);
            }
            digits.push(a as u64);
            if b > a {
                hi = None;
            }
        }
    }

    fn digit(&self, at: usize) -> u128 {
        self.0.get(at).map_or(0, |&d| u128::from(d))
    }
}

fn pick(a: u128, b: u128, rng: &mut fastrand::Rng) -> u64 {
    let gap = b - a - 1;
    let digit = match (a, b) {
        (0, OPEN) => MIDDLE + rng.u128(..STEP),
        (_, OPEN) => a + rng.u128(1..=gap.min(STEP)),
        (0, _) => b - rng.u128(1..=gap.min(STEP)),
        _ => a + 1 + gap / 4 + rng.u128(..=gap / 2),
    };
    digit as u64
}

impl From<Rank> for String {
    fn from(rank: Rank) -> String {
        rank.0.iter().map(|d| format!("{d:016x}")).collect()
    }
}

impl TryFrom<String> for Rank {
    type Error = &'static str;

    fn try_from(text: String) -> Result<Rank, &'static str> {
        const GROUPS: &str = "a rank is groups of 16 hex digits";
        if !text.is_ascii() || text.len() % 16 != 0 {
            return Err(GROUPS);
        }
        let digits = (0..text.len())
            .step_by(16)
            .map(|at| u64::from_str_radix(&text[at..at + 16], 16))
            .collect::<Result<Vec<u64>, _>>()
            .map_err(|_| GROUPS)?;
        match digits.last() {
            Some(&last) if last != 0 => Ok(Rank(digits)),
            _ => Err("a rank ends in a nonzero group"),
        }
    }
}
