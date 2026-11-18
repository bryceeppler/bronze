use std::time::Duration;

pub const TAP_WINDOW: Duration = Duration::from_millis(300);
pub const HOLD_LIMIT: Duration = Duration::from_millis(250);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Side {
    Left,
    Right,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Input {
    Shift { side: Side, pressed: bool },
    Other,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Gesture {
    DoubleLeftShift,
    BothShifts,
}

#[derive(Default)]
pub struct Detector {
    state: State,
}

#[derive(Clone, Copy, Default)]
enum State {
    #[default]
    Idle,
    FirstDown(Duration),
    FirstUp(Duration),
    SecondDown(Duration),
    RightAlone,
    Spent(Held),
}

#[derive(Clone, Copy)]
enum Held {
    One(Side),
    Both,
}

impl Detector {
    pub fn feed(&mut self, input: Input, at: Duration) -> Option<Gesture> {
        let (state, gesture) = step(self.state, input, at);
        self.state = state;
        gesture
    }
}

fn step(state: State, input: Input, at: Duration) -> (State, Option<Gesture>) {
    use Side::{Left, Right};
    use State::*;
    let both = (Spent(Held::Both), Some(Gesture::BothShifts));
    let Input::Shift { side, pressed } = input else {
        return match state {
            FirstDown(_) | SecondDown(_) => (Spent(Held::One(Left)), None),
            RightAlone => (Spent(Held::One(Right)), None),
            Spent(_) => (state, None),
            Idle | FirstUp(_) => (Idle, None),
        };
    };
    match (state, side, pressed) {
        (Spent(held), side, true) => (Spent(held.press(side)), None),
        (Spent(held), side, false) => (held.release(side).map_or(Idle, Spent), None),
        (FirstDown(_) | SecondDown(_), Right, true) | (RightAlone, Left, true) => both,
        (FirstDown(down), Left, false) if at.saturating_sub(down) <= HOLD_LIMIT => (FirstUp(at), None),
        (FirstUp(up), Left, true) if at.saturating_sub(up) <= TAP_WINDOW => (SecondDown(at), None),
        (SecondDown(down), Left, false) if at.saturating_sub(down) <= HOLD_LIMIT => {
            (Idle, Some(Gesture::DoubleLeftShift))
        }
        (FirstDown(_) | SecondDown(_), Left, true) => (state, None),
        (FirstDown(_) | SecondDown(_), Right, false) => (Spent(Held::One(Left)), None),
        (RightAlone, Right, true) => (state, None),
        (_, Left, true) => (FirstDown(at), None),
        (_, Right, true) => (RightAlone, None),
        (_, _, false) => (Idle, None),
    }
}

impl Held {
    fn press(self, side: Side) -> Held {
        match self {
            Held::One(held) if held == side => self,
            _ => Held::Both,
        }
    }

    fn release(self, side: Side) -> Option<Held> {
        match (self, side) {
            (Held::Both, Side::Left) => Some(Held::One(Side::Right)),
            (Held::Both, Side::Right) => Some(Held::One(Side::Left)),
            (Held::One(held), side) if held == side => None,
            (Held::One(_), _) => Some(self),
        }
    }
}

#[cfg(test)]
mod tests;
