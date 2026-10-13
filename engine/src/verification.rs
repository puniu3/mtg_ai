//! Verification-only input generation and payment helpers for parity/stress tests.
//! This driver sees the complete state; it is not a playing-strength baseline.
//! No search, leaf evaluation, action vocabulary, or rule transitions live here.
use crate::*;
use mtg_state_design::burn::{Execution, State};
#[derive(Clone, Copy, Debug)]
pub struct Rng(pub u64);
impl Rng {
    pub fn next(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        x
    }
    pub fn index(&mut self, n: usize) -> usize {
        (self.next() as usize) % n
    }
}
#[derive(Clone, Copy, Debug)]
pub struct PaymentPlan {
    pub taps: [(ObjectId, u8); 48],
    pub len: usize,
    pub mana: [u32; 6],
}
pub fn payment(
    e: &Engine,
    p: Player,
    cost: Cost,
    excluded: Option<ObjectId>,
) -> Option<PaymentPlan> {
    let mut result = PaymentPlan {
        taps: [(ObjectId(0), 0); 48],
        len: 0,
        mana: [0; 6],
    };
    let mut available = e.world().players[pid(p)].mana;
    let mut used = [false; 48];
    let mut life = e.world().players[pid(p)].life;
    for (color, required) in [(WHITE, cost.white), (RED, cost.red)] {
        while available[color] < required {
            let mut found = None;
            for (i, per) in entries(&e.world().battlefield).iter().enumerate() {
                let c = card_of(per);
                if !used[i]
                    && Some(per.ids.first) != excluded
                    && per.base_controller == p
                    && per.status & TAPPED == 0
                    && (c != CANYON || life > 0)
                    && ((color == WHITE && matches!(c, FOUNDRY | VANTAGE | CANYON))
                        || (color == RED && matches!(c, MOUNTAIN | FOUNDRY | VANTAGE | CANYON)))
                {
                    let rank = |card: Card| (card == CANYON, color == RED && card != MOUNTAIN);
                    if found
                        .as_ref()
                        .is_none_or(|(_, old): &(usize, Permanent)| rank(c) < rank(card_of(old)))
                    {
                        found = Some((i, *per));
                    }
                    if (color == RED && c == MOUNTAIN) || (color == WHITE && c != CANYON) {
                        break;
                    }
                }
            }
            let (i, per) = found?;
            used[i] = true;
            if card_of(&per) == CANYON {
                life -= 1;
            }
            available[color] += 1;
            result.taps[result.len] = (per.ids.first, color as u8);
            result.len += 1;
        }
        available[color] -= required;
        result.mana[color] += required;
    }
    let mut generic = cost.generic;
    for color in [5, 4, 2, 1, RED, WHITE] {
        let n = available[color].min(generic);
        generic -= n;
        result.mana[color] += n;
    }
    // Generic costs preserve flexible lands and avoid unnecessary pain payments.
    for wanted in [MOUNTAIN, VANTAGE, FOUNDRY, CANYON] {
        for (i, per) in entries(&e.world().battlefield).iter().enumerate() {
            if generic == 0 {
                break;
            }
            let c = card_of(per);
            if c == wanted
                && !used[i]
                && Some(per.ids.first) != excluded
                && per.base_controller == p
                && per.status & TAPPED == 0
                && (c != CANYON || life > 0)
            {
                used[i] = true;
                if c == CANYON {
                    life -= 1;
                }
                result.taps[result.len] = (per.ids.first, RED as u8);
                result.len += 1;
                result.mana[RED] += 1;
                generic -= 1;
            }
        }
    }
    if generic == 0 { Some(result) } else { None }
}
pub fn apply_paid(e: &mut Engine, p: Player, a: Action) -> Result {
    let saved = *e;
    let (cost, excluded) = match a {
        Action::Cast {
            card, spectacle, ..
        } => (crate::cost(card, spectacle), None),
        Action::Canyon { object } => (
            Cost {
                generic: 1,
                ..Default::default()
            },
            Some(object),
        ),
        Action::Suspend => (
            Cost {
                red: 1,
                ..Default::default()
            },
            None,
        ),
        _ => return e.apply(p, a),
    };
    let plan = payment(e, p, cost, excluded).ok_or(Error::Illegal("no payment plan"))?;
    let result = (|| {
        e.apply(p, a)?;
        for &(id, color) in &plan.taps[..plan.len] {
            e.apply(p, Action::Tap { object: id, color })?;
        }
        e.apply(p, Action::Pay { mana: plan.mana })
    })();
    if result.is_err() {
        *e = saved;
    }
    result
}
pub fn library_shuffle(e: &Engine, rng: &mut Rng) -> Action {
    let len = match e.state {
        State::Mulligan {
            setup,
            execution: Execution::Running { running },
            ..
        } => {
            let p = if let Frame::MulliganAction { player, .. } = running.frames.data[0] {
                player
            } else {
                unreachable!()
            };
            setup.zones.library[pid(p)].len
        }
        State::Playing {
            world,
            execution: Execution::Running { running },
            ..
        } => {
            let p = if let Frame::Resolve { controller, .. } = running.frames.data[0] {
                controller
            } else {
                unreachable!()
            };
            world.zones.library[pid(p)].len
        }
        _ => panic!("shuffle boundary"),
    };
    let mut order = [0u8; 60];
    for i in 0..len as usize {
        order[i] = i as u8;
    }
    for i in (1..len as usize).rev() {
        let j = rng.index(i + 1);
        order.swap(i, j);
    }
    Action::Shuffle {
        order,
        len: len as u8,
    }
}
pub fn drive_step(e: &mut Engine, rng: &mut Rng) -> Result {
    if let State::Mulligan {
        setup, execution, ..
    } = e.state
    {
        if let Execution::Running { running } = execution {
            match running.choice {
                Choice::Chance { .. } => return e.apply(Player(0), library_shuffle(e, rng)),
                Choice::Select { player, .. } => {
                    let start = rng.index(16);
                    let c = (0..16)
                        .map(|i| (i + start) % 16)
                        .find(|i| setup.zones.hand[pid(player)][*i] > 0)
                        .unwrap();
                    return e.apply(
                        player,
                        Action::Bottom {
                            card: Card(c as u16),
                        },
                    );
                }
                _ => return Err(Error::Illegal("mulligan policy")),
            }
        } else {
            return e.apply(e.actor().unwrap(), Action::Mulligan { take: false });
        }
    }
    if let Execution::Running { running } = e.execution() {
        let p = e.actor().unwrap_or(Player(0));
        match running.choice {
            Choice::Order { .. } => {
                let t = entries(&running.triggers)
                    .iter()
                    .filter(|x| x.controller == p)
                    .min_by_key(|x| {
                        if let AbilitySource::Fixed { source } = x.source {
                            source.0
                        } else {
                            0
                        }
                    })
                    .unwrap();
                e.apply(
                    p,
                    Action::OrderTrigger {
                        trigger: t.ids.first,
                    },
                )
            }
            Choice::YesNo { .. } => e.apply(
                p,
                Action::Shock {
                    pay: e.world().players[pid(p)].life > 4,
                },
            ),
            Choice::Chance { .. } => e.apply(p, library_shuffle(e, rng)),
            Choice::Select {
                rule: Behavior(FETCH_CHOICE),
                ..
            } => {
                let c = entries(&e.world().zones.library[pid(p)])
                    .iter()
                    .copied()
                    .filter(|c| matches!(*c, MOUNTAIN | FOUNDRY))
                    .min_by_key(|c| if *c == MOUNTAIN { 0 } else { 1 });
                e.apply(p, Action::Search { card: c })
            }
            Choice::Select {
                rule: Behavior(SUSPEND_CHOICE),
                ..
            } => e.apply(
                p,
                Action::SuspendTarget {
                    target: Subject::Player { player: other(p) },
                },
            ),
            Choice::Select {
                rule: Behavior(DISCARD_CHOICE),
                ..
            } => {
                let c = (0..16)
                    .rev()
                    .find(|i| e.world().zones.hand[pid(p)][*i] > 0)
                    .unwrap();
                e.apply(
                    p,
                    Action::Discard {
                        card: Card(c as u16),
                    },
                )
            }
            Choice::Select {
                rule: Behavior(ATTACK_CHOICE),
                ..
            } => {
                let mut objects = [ObjectId(0); 24];
                let mut len = 0;
                for per in entries(&e.world().battlefield) {
                    let c = card_of(per);
                    if per.base_controller == p
                        && creature(c)
                        && per.status & TAPPED == 0
                        && (per.status & CONTROLLED != 0 || matches!(c, GUIDE | SWIFT))
                        && rng.index(4) != 0
                    {
                        objects[len] = per.ids.first;
                        len += 1;
                    }
                }
                e.apply(
                    p,
                    Action::Attack {
                        objects,
                        len: len as u8,
                    },
                )
            }
            Choice::Select {
                rule: Behavior(BLOCK_CHOICE),
                ..
            } => {
                let combat = e.combat();
                let living: Pool<Attacker, 24> = combat.attackers;
                let mut pairs = [(ObjectId(0), ObjectId(0)); 24];
                let mut len = 0;
                for per in entries(&e.world().battlefield) {
                    if per.base_controller == p
                        && creature(card_of(per))
                        && per.status & TAPPED == 0
                        && rng.index(3) != 0
                    {
                        let attackers = entries(&living);
                        if !attackers.is_empty() {
                            let a = attackers[rng.index(attackers.len())].attackers.first;
                            if e.permanent(a).is_some() {
                                pairs[len] = (per.ids.first, a);
                                len += 1;
                            }
                        }
                    }
                }
                e.apply(
                    p,
                    Action::Block {
                        pairs,
                        len: len as u8,
                    },
                )
            }
            Choice::Select {
                rule: Behavior(DAMAGE_CHOICE),
                ..
            } => {
                let combat = e.combat();
                let mut assignments = [(ObjectId(0), Subject::Absent, 0); 48];
                let mut len = 0;
                for a in entries(&combat.attackers) {
                    let id = a.attackers.first;
                    if e.permanent(id).is_none() || !e.damage_eligible(id, &combat) {
                        continue;
                    }
                    let mut n = 0;
                    let mut first = ObjectId(0);
                    for b in entries(&combat.blocks) {
                        if b.attacker == id && e.permanent(b.blocker).is_some() {
                            n += 1;
                            first = b.blocker;
                        }
                    }
                    if n > 1 {
                        assignments[len] = (
                            id,
                            Subject::Object { id: first },
                            e.power_toughness(id).unwrap().0.max(0) as u32,
                        );
                        len += 1;
                    }
                }
                e.apply(
                    p,
                    Action::Damage {
                        assignments,
                        len: len as u8,
                    },
                )
            }
            _ => Err(Error::Illegal("unhandled external policy choice")),
        }
    } else {
        let p = e.actor().ok_or(Error::Illegal("policy actor"))?;
        // Deliberately weak randomized policy; serves only to exercise rule transitions.
        let start = rng.index(18);
        for i in 0..18 {
            let k = (i + start) % 18;
            if k == 17 {
                return e.apply(p, Action::Pass);
            }
            if k < 16 && e.world().zones.hand[pid(p)][k] > 0 {
                let c = Card(k as u16);
                let mut targets = [Subject::Absent; 2];
                if !land(c) && !creature(c) {
                    targets[0] = Subject::Player { player: other(p) };
                    if c == BLAZE {
                        if let Some(per) = entries(&e.world().battlefield)
                            .iter()
                            .find(|x| x.base_controller == other(p) && creature(card_of(x)))
                        {
                            targets[1] = Subject::Object { id: per.ids.first };
                        } else {
                            continue;
                        }
                    }
                }
                let a = if land(c) {
                    Action::Land { card: c }
                } else if c == RIFT && rng.index(2) == 0 {
                    Action::Suspend
                } else {
                    Action::Cast {
                        card: c,
                        targets,
                        mode: 0,
                        spectacle: c == SKEWER
                            && e.world().turn_history.life_lost_mask & (1 << other(p).0) != 0,
                    }
                };
                match apply_paid(e, p, a) {
                    Ok(()) => return Ok(()),
                    Err(Error::Illegal(_)) => {}
                    Err(err) => return Err(err),
                }
            } else if k == 16 {
                let board = e.world().battlefield;
                for per in entries(&board) {
                    if per.base_controller != p || per.status & TAPPED != 0 {
                        continue;
                    }
                    let a = match card_of(per) {
                        MESA | MIRE => Action::Fetch {
                            object: per.ids.first,
                        },
                        CANYON if rng.index(4) == 0 => Action::Canyon {
                            object: per.ids.first,
                        },
                        _ => continue,
                    };
                    match apply_paid(e, p, a) {
                        Ok(()) => return Ok(()),
                        Err(Error::Illegal(_)) => {}
                        Err(err) => return Err(err),
                    }
                }
            }
        }
        e.apply(p, Action::Pass)
    }
}
pub fn deck(rng: &mut Rng) -> [Card; 60] {
    let mut d = [Card(0); 60];
    let mut i = 0;
    for (c, n) in DECK_COUNTS.iter().enumerate() {
        for _ in 0..*n {
            d[i] = Card(c as u16);
            i += 1;
        }
    }
    for i in (1..60).rev() {
        let j = rng.index(i + 1);
        d.swap(i, j);
    }
    d
}
