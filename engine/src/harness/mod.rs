//! Shared Burn interaction layer for external players and search policies.
//! Owns legal actions, observer knowledge, chance and explicit stop policies.
//! This layer does not import the search policy or its learned model.
pub(crate) mod actions;
mod auto;
mod observation;
mod text;
use crate::*;
pub use actions::ActionSpace;
pub use auto::{AutoStep, auto_step, external_step};
use mtg_state_design::burn::{State, Zones};
pub use observation::{Observation, Position, PublicEvent};
pub use text::action_text;

#[derive(Clone, Debug)]
pub struct Random(pub u64);
impl Random {
    pub fn next_u64(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9e3779b97f4a7c15);
        let mut x = self.0;
        x = (x ^ (x >> 30)).wrapping_mul(0xbf58476d1ce4e5b9);
        x = (x ^ (x >> 27)).wrapping_mul(0x94d049bb133111eb);
        x ^ (x >> 31)
    }
    pub fn below(&mut self, n: u128) -> u128 {
        assert!(n > 0);
        let threshold = n.wrapping_neg() % n;
        loop {
            let x = ((self.next_u64() as u128) << 64) | self.next_u64() as u128;
            if x >= threshold {
                return x % n;
            }
        }
    }
    pub fn shuffle<T>(&mut self, a: &mut [T]) {
        for i in (1..a.len()).rev() {
            let j = self.below((i + 1) as u128) as usize;
            a.swap(i, j);
        }
    }
}
pub(crate) fn zones(e: &Engine) -> Option<&Zones> {
    match &e.state {
        State::Mulligan { setup, .. } => Some(&setup.zones),
        State::Playing { world, .. } | State::Opening { world, .. } => Some(&world.zones),
        _ => None,
    }
}
pub(crate) fn zones_mut(e: &mut Engine) -> Option<&mut Zones> {
    match &mut e.state {
        State::Mulligan { setup, .. } => Some(&mut setup.zones),
        State::Playing { world, .. } | State::Opening { world, .. } => Some(&mut world.zones),
        _ => None,
    }
}
pub fn deck(rng: &mut Random) -> [Card; 60] {
    let mut out = [Card(0); 60];
    let mut i = 0;
    for (c, n) in DECK_COUNTS.iter().enumerate() {
        for _ in 0..*n {
            out[i] = Card(c as u16);
            i += 1;
        }
    }
    rng.shuffle(&mut out);
    out
}
/// Inventory includes announced spells and pending land entry. Finished states contain only an outcome.
pub fn inventory(e: &Engine) -> Option<[[u32; 16]; 2]> {
    let z = zones(e)?;
    let mut out = [[0; 16]; 2];
    for p in 0..2 {
        for c in 0..16 {
            out[p][c] =
                z.hand[p][c] as u32 + z.graveyard[p][c] as u32 + z.ordinary_exile[p][c] as u32;
        }
        for c in entries(&z.library[p]) {
            if c.0 < 16 {
                out[p][c.0 as usize] += 1;
            }
        }
    }
    for t in entries(&z.tracked) {
        if !matches!(t.zone, OwnedZone::Library { .. }) {
            out[pid(t.owner)][t.card.0 as usize] += 1;
        }
    }
    let mut add = |origin: Origin, n: u32| {
        if let Origin::Card { card, owner } = origin {
            out[pid(owner)][card.0 as usize] += n;
        }
    };
    if let State::Playing { world, .. } | State::Opening { world, .. } = &e.state {
        for b in entries(&world.battlefield) {
            add(b.origin, b.ids.count);
        }
        for s in entries(&world.stack) {
            if let StackBody::Spell { origin, .. } = s.body {
                add(origin, s.ids.count);
            }
        }
    }
    if let Execution::Running { running } = e.execution() {
        for x in entries(&running.entering) {
            add(x.origin, 1);
        }
    }
    Some(out)
}
pub(crate) fn chance_owner(e: &Engine) -> Option<Player> {
    if let Execution::Running { running } = e.execution() {
        if matches!(running.choice, Choice::Chance { .. }) {
            return match running.frames.data[0] {
                Frame::MulliganAction { player, .. } => Some(player),
                Frame::Resolve { controller, .. } => Some(controller),
                _ => None,
            };
        }
    }
    None
}
pub fn chance_action(e: &Engine, rng: &mut Random) -> Result<Action> {
    let p = chance_owner(e).ok_or(Error::Illegal("not chance"))?;
    let len = zones(e).unwrap().library[pid(p)].len as usize;
    let mut order = [0; 60];
    for (i, x) in order[..len].iter_mut().enumerate() {
        *x = i as u8;
    }
    rng.shuffle(&mut order[..len]);
    Ok(Action::Shuffle {
        order,
        len: len as u8,
    })
}
pub fn resolve_chance(position: &mut Position, rng: &mut Random) -> Result {
    let action = chance_action(&position.engine, rng)?;
    position.apply(Player(0), action).map(|_| ())
}
