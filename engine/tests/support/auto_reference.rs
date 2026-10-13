// Frozen pre-optimization auto policy, used only as a differential oracle.
use super::*;
pub fn reference_step(e: &Engine) -> Result<AutoStep> {
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
            return Ok(automatic(Action::Pass, "empty stack outside stops"));
        }
    }
    let mut actions = ActionSpace::build(e)?;
    if priority {
        // Any nonempty stack (including our own spells/triggers) offers a response.
        // Standalone mana production is not an auto-mode decision. Cast/activate
        // feasibility already includes untapped sources, not just floating mana.
        actions = actions.without_taps();
    }
    if let Execution::Running { running } = e.execution() {
        match running.choice {
            Choice::Mana { .. } => {
                return Ok(automatic(payment(e, actor, &actions)?, "auto payment"));
            }
            Choice::Order { .. } => {
                let action = (0..actions.len())
                    .map(|i| actions.get(i))
                    .min_by_key(|a| match a {
                        Action::OrderTrigger { trigger } => trigger.0,
                        _ => u32::MAX,
                    })
                    .ok_or(Error::Illegal("auto: no trigger order"))?;
                return Ok(automatic(action, "auto trigger order"));
            }
            _ => (),
        }
    }
    if actions.len() == 1 {
        return Ok(automatic(actions.get(0), "only auto action"));
    }
    Ok(AutoStep::Decision { actor, actions })
}

fn payment(e: &Engine, actor: Player, actions: &ActionSpace) -> Result<Action> {
    // Use floating mana before activating any more lands.
    for i in 0..actions.len() {
        if let a @ Action::Pay { .. } = actions.get(i) {
            return Ok(a);
        }
    }
    let (cost, _) = actions::pending_cost(e)?;
    let mana = e.world().players[pid(actor)].mana;
    let red = cost.red.saturating_sub(mana[RED]);
    let white = cost.white.saturating_sub(mana[WHITE]);
    let surplus = mana.iter().sum::<u32>() - mana[RED].min(cost.red) - mana[WHITE].min(cost.white);
    let generic = cost.generic.saturating_sub(surplus);
    // Primitive candidates already ensure the remaining cost can still be paid.
    // Do not float unnecessary red when only white is missing. Ties are stable.
    (0..actions.len())
        .map(|i| actions.get(i))
        .filter(|a| match a {
            Action::Tap { color, .. } => {
                generic > 0
                    || (*color as usize == RED && red > 0)
                    || (*color as usize == WHITE && white > 0)
            }
            _ => false,
        })
        .min_by_key(|a| match a {
            Action::Tap { object, color } => {
                let rank = match card_of(e.permanent(*object).unwrap()) {
                    MOUNTAIN => 0,
                    FOUNDRY | VANTAGE => 1,
                    CANYON => 2,
                    _ => 3,
                };
                (rank, object.0, *color)
            }
            _ => unreachable!(),
        })
        .ok_or(Error::Illegal("auto: no completing payment"))
}
