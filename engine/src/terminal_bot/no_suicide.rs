//! Search policy pruning for the fixed Burn card pool, not rules legality.
//! Uses public state only. Original action indices remain stable for runners.
use super::*;

pub(super) struct SearchChoices {
    indices: Option<Vec<u128>>,
    len: u128,
}
impl SearchChoices {
    pub(super) fn new(e: &Engine, actor: Player, actions: &ActionSpace) -> Result<Self> {
        let mut out = Self {
            indices: None,
            len: actions.len(),
        };
        let Some(small) = actions.small() else {
            return Ok(out);
        };
        if !matches!(e.state, State::Playing { .. }) {
            return Ok(out);
        }
        let context = Context::new(e, actor);
        let mut kept = Vec::new();
        // Allocate only when the first excluded candidate is encountered.
        for (i, &action) in small.iter().enumerate() {
            if context.excludes(e, actor, action) {
                if out.indices.is_none() {
                    kept.extend(0..i as u128);
                    out.indices = Some(Vec::new());
                }
            } else if out.indices.is_some() {
                kept.push(i as u128);
            }
        }
        if out.indices.is_some() {
            if kept.is_empty() {
                return Err(Error::Illegal("search policy removed every action"));
            }
            out.len = kept.len() as u128;
            out.indices = Some(kept);
        }
        Ok(out)
    }
    pub(super) fn len(&self) -> u128 {
        self.len
    }
    pub(super) fn index(&self, rank: u128) -> u128 {
        self.indices.as_ref().map_or(rank, |v| v[rank as usize])
    }
}

struct Context {
    life: i32,
    eidolon_damage: u32,
    mana: [u32; 6],
    red: u32,
    dual: u32,
}
impl Context {
    fn new(e: &Engine, actor: Player) -> Self {
        let player = e.world().players[pid(actor)];
        let mut c = Self {
            life: player.life,
            mana: player.mana,
            eidolon_damage: 0,
            red: 0,
            dual: 0,
        };
        for per in entries(&e.world().battlefield) {
            // Each battlefield entry produces one trigger in the current rules engine.
            if card_of(per) == EIDOLON {
                c.eidolon_damage += 2;
            }
            if per.base_controller != actor || per.status & TAPPED != 0 {
                continue;
            }
            match card_of(per) {
                MOUNTAIN => c.red += 1,
                FOUNDRY | VANTAGE => c.dual += 1,
                _ => (),
            }
        }
        c
    }
    fn payment_loss(&self, e: &Engine, cost: Cost, excluded: Option<ObjectId>) -> u32 {
        let (mut red, mut dual) = (self.red, self.dual);
        if let Some(per) = excluded.and_then(|id| e.permanent(id)) {
            match card_of(per) {
                MOUNTAIN => red -= 1,
                FOUNDRY | VANTAGE => dual -= 1,
                _ => (),
            }
        }
        let r = cost.red.saturating_sub(self.mana[RED]);
        let w = cost.white.saturating_sub(self.mana[WHITE]);
        let surplus = self.mana.iter().sum::<u32>()
            - self.mana[RED].min(cost.red)
            - self.mana[WHITE].min(cost.white);
        let generic = cost.generic.saturating_sub(surplus);
        // Auto consumes floating mana, then Mountain/painless dual before Canyon,
        // retaining a completing payment. This is its exact minimum pain cost.
        (r + w + generic)
            .saturating_sub(red + dual)
            .max(w.saturating_sub(dual))
    }
    fn excludes(&self, e: &Engine, actor: Player, action: Action) -> bool {
        let lethal = |damage: u32| i64::from(damage) >= i64::from(self.life);
        match action {
            Action::Cast {
                card,
                targets,
                mode,
                spectacle,
            } => {
                let pain = self.payment_loss(e, cost(card, spectacle), None);
                let triggers = if mana_value(card) <= 3 {
                    self.eidolon_damage
                } else {
                    0
                };
                // Helix heals only after all newly created Eidolon triggers resolve.
                if lethal(pain + triggers) {
                    return true;
                }
                let self_damage = if targets[0] == (Subject::Player { player: actor }) {
                    match card {
                        BOLT | SPIKE | RIFT | SKEWER => 3,
                        CHARM if mode == 0 => 4,
                        BLAZE => {
                            if e.world().turn_history.land_entered_mask & (1 << actor.0) != 0 {
                                3
                            } else {
                                1
                            }
                        }
                        // Helix damage and healing occur within one resolution;
                        // state-based loss is checked afterwards. Net damage is zero.
                        _ => 0,
                    }
                } else {
                    0
                };
                lethal(pain + triggers + self_damage)
            }
            Action::SuspendTarget { target } => lethal(
                self.eidolon_damage
                    + if target == (Subject::Player { player: actor }) {
                        3
                    } else {
                        0
                    },
            ),
            // Suspending is not casting and causes no Eidolon trigger.
            Action::Suspend => lethal(self.payment_loss(
                e,
                Cost {
                    red: 1,
                    ..Default::default()
                },
                None,
            )),
            Action::Fetch { .. } => lethal(1),
            Action::Shock { pay: true } => lethal(2),
            Action::Canyon { object } => {
                lethal(self.payment_loss(
                    e,
                    Cost {
                        generic: 1,
                        ..Default::default()
                    },
                    Some(object),
                )) || e.world().zones.library[pid(actor)].len == 0
            }
            // Passing, combat and mandatory choices are never pruned as tactical losses.
            _ => false,
        }
    }
}

#[cfg(test)]
#[path = "../../tests/support/no_suicide.rs"]
mod tests;
