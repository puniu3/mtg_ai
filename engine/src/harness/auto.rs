//! Fixed interaction policy, outside the primitive rules engine.
//! Search runners use fixed stops; external players retain every meaningful
//! non-mana priority window. Payment and trigger ordering remain automatic.
use super::*;

#[derive(Clone, Debug)]
pub enum AutoStep {
    Finished(Outcome),
    Chance,
    Automatic {
        actor: Player,
        action: Action,
        reason: &'static str,
    },
    Decision {
        actor: Player,
        actions: ActionSpace,
    },
}

/// Fixed search boundary. Friendly damage is limited to creatures targeted by the opponent.
pub fn auto_step(e: &Engine) -> Result<AutoStep> {
    step(e, false)
}

/// External-player boundary: preserve instant/ability windows for both players.
/// This still fixes mana payment and trigger order; it is not full manual play.
pub fn external_step(e: &Engine) -> Result<AutoStep> {
    step(e, true)
}

fn step(e: &Engine, all_priority: bool) -> Result<AutoStep> {
    if let Some(outcome) = e.finished() {
        return Ok(AutoStep::Finished(outcome));
    }
    if chance_owner(e).is_some() {
        return Ok(AutoStep::Chance);
    }
    let actor = e.actor().ok_or(Error::Illegal("auto: missing actor"))?;
    let automatic = |action, reason| AutoStep::Automatic {
        actor,
        action,
        reason,
    };
    let priority = matches!(e.execution(), Execution::Priority { .. });
    if priority && e.world().stack.len == 0 {
        let own_turn = e.progress().turn.active == actor;
        let stop = match e.progress().phase {
            Phase::Main { .. } => own_turn,
            Phase::Combat { combat } if matches!(combat.header.step, CombatStep::Blockers {}) => {
                !own_turn
                    && entries(&combat.attackers)
                        .iter()
                        .any(|a| e.permanent(a.attackers.first).is_some())
            }
            Phase::Ending { cleanup: false } => !own_turn,
            _ => false,
        };
        if !stop {
            if all_priority {
                let actions = ActionSpace::build(e)?.without_taps();
                if actions.len() > 1 {
                    return Ok(AutoStep::Decision { actor, actions });
                }
            }
            return Ok(automatic(Action::Pass, "empty stack outside stops"));
        }
    }
    if let Execution::Running { running } = e.execution() {
        match running.choice {
            Choice::Mana { .. } => return Ok(automatic(payment_fast(e, actor)?, "auto payment")),
            Choice::Order { .. } => {
                let trigger = entries(&running.triggers)
                    .iter()
                    .filter(|t| t.controller == actor)
                    .map(|t| t.ids.first)
                    .min_by_key(|id| id.0)
                    .ok_or(Error::Illegal("auto: no trigger order"))?;
                return Ok(automatic(
                    Action::OrderTrigger { trigger },
                    "auto trigger order",
                ));
            }
            _ => (),
        }
    }
    // Build only decisions the fixed auto policy exposes.
    let actions = ActionSpace::build_auto(e)?;
    if actions.len() == 1 {
        return Ok(automatic(actions.get(0), "only auto action"));
    }
    Ok(AutoStep::Decision { actor, actions })
}

fn payment_fast(e: &Engine, actor: Player) -> Result<Action> {
    let (cost, excluded) = actions::pending_cost(e)?;
    let mana = e.world().players[pid(actor)].mana;
    let total = cost.red + cost.white + cost.generic;
    if mana[RED] >= cost.red && mana[WHITE] >= cost.white && mana.iter().sum::<u32>() >= total {
        // Same lexicographically first legal payment as the original enumerator.
        let mut left = total;
        let mut paid = [0; 6];
        for i in 0..6 {
            let required = if i == RED {
                cost.red
            } else if i == WHITE {
                cost.white
            } else {
                0
            };
            paid[i] = required.max(left.saturating_sub(mana[i + 1..].iter().sum()));
            left -= paid[i];
        }
        return Ok(Action::Pay { mana: paid });
    }
    let red = cost.red.saturating_sub(mana[RED]);
    let white = cost.white.saturating_sub(mana[WHITE]);
    let surplus = mana.iter().sum::<u32>() - mana[RED].min(cost.red) - mana[WHITE].min(cost.white);
    let generic = cost.generic.saturating_sub(surplus);
    let mut candidates = Vec::new();
    for per in entries(&e.world().battlefield) {
        if per.base_controller != actor
            || per.status & TAPPED != 0
            || Some(per.ids.first) == excluded
        {
            continue;
        }
        let rank = match card_of(per) {
            MOUNTAIN => 0,
            FOUNDRY | VANTAGE => 1,
            CANYON => 2,
            _ => continue,
        };
        for color in [WHITE, RED] {
            if rank == 0 && color != RED {
                continue;
            }
            if generic == 0 && (color == RED && red == 0 || color == WHITE && white == 0) {
                continue;
            }
            candidates.push((rank, per.ids.first.0, color));
        }
    }
    candidates.sort_unstable();
    for (_, object, color) in candidates {
        let action = Action::Tap {
            object: ObjectId(object),
            color: color as u8,
        };
        let mut copy = *e;
        if copy.apply(actor, action).is_ok() && actions::affordable(&copy, actor, cost, excluded) {
            return Ok(action);
        }
    }
    Err(Error::Illegal("auto: no completing payment"))
}

#[cfg(test)]
#[path = "../../tests/support/auto_equivalence.rs"]
mod equivalence;
