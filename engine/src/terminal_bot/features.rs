//! Frozen inputs for the adopted ordered-stack rollout policy.
use super::*;
pub const STATE_DIM: usize = 64;
pub const ACTION_DIM: usize = 48;
pub fn state_features(e: &Engine, actor: Player) -> [f32; STATE_DIM] {
    let mut x = [0.; STATE_DIM];
    let Some(z) = zones(e) else {
        return x;
    };
    let us = pid(actor);
    let them = pid(other(actor));
    x[2] = z.hand[us].iter().map(|&n| n as f32).sum::<f32>() / 7.;
    x[3] = z.hand[them].iter().map(|&n| n as f32).sum::<f32>() / 7.;
    x[4] = z.library[us].len as f32 / 60.;
    x[5] = z.library[them].len as f32 / 60.;
    for c in 0..16 {
        x[16 + c] = z.hand[us][c] as f32 / 4.;
    }
    if let State::Mulligan { process, .. } = e.state {
        x[58] = process.taken[us] as f32 / 3.;
        x[59] = process.taken[them] as f32 / 3.;
        return x;
    }
    if !matches!(e.state, State::Playing { .. }) {
        return x;
    }
    let w = e.world();
    let progress = e.progress();
    x[0] = w.players[us].life as f32 / 20.;
    x[1] = w.players[them].life as f32 / 20.;
    match progress.phase {
        Phase::Main { postcombat: false } => x[6] = 1.,
        Phase::Main { postcombat: true } => x[7] = 1.,
        Phase::Combat { combat } => {
            x[8] = 1.;
            for a in entries(&combat.attackers) {
                if let Some(p) = e.permanent(a.attackers.first) {
                    x[60 + usize::from(p.base_controller != actor)] += 0.25;
                }
            }
        }
        Phase::Ending { .. } => x[9] = 1.,
        _ => (),
    }
    x[10] = f32::from(progress.turn.active == actor);
    x[11] = w.stack.len as f32 / 8.;
    x[12] = w.players[us].mana[RED] as f32 / 4.;
    x[13] = w.players[us].mana[WHITE] as f32 / 4.;
    x[14] = w.players[them].mana[RED] as f32 / 4.;
    x[15] = w.players[them].mana[WHITE] as f32 / 4.;
    for p in entries(&w.battlefield) {
        let offset = 32 + 8 * usize::from(p.base_controller != actor);
        let c = card_of(p);
        let untapped = p.status & TAPPED == 0;
        if creature(c) {
            x[offset + c.0 as usize] += 0.25;
            x[offset + 3] += if untapped { 0.25 } else { 0. };
            x[offset + 4] += e.power_toughness(p.ids.first).unwrap().0 as f32 / 8.;
        }
        if untapped && land(c) {
            x[offset + 5] += 0.25;
            if matches!(c, FOUNDRY | VANTAGE | CANYON) {
                x[offset + 6] += 0.25;
            }
            if c == CANYON {
                x[offset + 7] += 0.25;
            }
        }
    }
    x[48] = f32::from(w.players[us].life <= 2);
    x[49] = f32::from(w.players[them].life <= 2);
    x[50] = f32::from(w.players[us].life <= 3);
    x[51] = f32::from(w.players[them].life <= 3);
    x[52] = x[34] + x[42];
    x[53] = f32::from(w.turn_history.land_entered_mask & (1 << us) != 0);
    x[54] = f32::from(w.turn_history.land_entered_mask & (1 << them) != 0);
    x[55] = w.turn_history.land_plays[us] as f32;
    x[56] = w.turn_history.land_plays[them] as f32;
    x[62] = (w.players[us].life - w.players[them].life) as f32 / 20.;
    x[63] = 1.;
    x
}
pub fn action_features(e: &Engine, actor: Player, a: Action) -> [f32; ACTION_DIM] {
    let mut x = [0.; ACTION_DIM];
    let mut card = None;
    let mut target = None;
    let kind = match a {
        Action::Pass => 0,
        Action::Cast {
            card: c,
            targets,
            mode,
            spectacle,
        } => {
            card = Some(c);
            target = Some(targets[0]);
            if c == CHARM {
                x[43 + mode as usize] = 1.;
            }
            x[46] = f32::from(spectacle);
            x[47] = match c {
                BOLT | SPIKE | RIFT | SKEWER | HELIX => 0.75,
                CHARM if mode == 0 => 1.,
                BLAZE => {
                    if matches!(e.state, State::Playing { .. })
                        && e.world().turn_history.land_entered_mask & (1 << actor.0) != 0
                    {
                        0.75
                    } else {
                        0.25
                    }
                }
                _ => 0.,
            };
            1
        }
        Action::Land { card: c } => {
            card = Some(c);
            2
        }
        Action::Attack { objects, len } => {
            x[40] = len as f32 / 4.;
            x[41] = objects[..len as usize]
                .iter()
                .map(|&id| e.power_toughness(id).unwrap().0 as f32)
                .sum::<f32>()
                / 8.;
            3
        }
        Action::Block { len, .. } => {
            x[42] = len as f32 / 4.;
            4
        }
        Action::Damage { .. } => 5,
        Action::Suspend => {
            card = Some(RIFT);
            6
        }
        Action::Fetch { .. } => 7,
        Action::Canyon { .. } => 8,
        Action::Shock { pay: true } => 9,
        Action::Shock { pay: false } => 10,
        Action::Search { card: Some(c) } => {
            card = Some(c);
            11
        }
        Action::Search { card: None } => 12,
        Action::Mulligan { take: true } => 13,
        Action::Mulligan { take: false } => 14,
        Action::Bottom { card: c } => {
            card = Some(c);
            15
        }
        Action::Discard { card: c } => {
            card = Some(c);
            16
        }
        Action::SuspendTarget { target: t } => {
            target = Some(t);
            17
        }
        Action::DeclineSuspend => 18,
        _ => 19,
    };
    x[kind] = 1.;
    if let Some(c) = card {
        x[20 + c.0 as usize] = 1.;
    }
    if let Some(t) = target {
        match t {
            Subject::Player { player } => x[36 + usize::from(player != actor)] = 1.,
            Subject::Object { id } => {
                if let Some(p) = e.permanent(id) {
                    x[38 + usize::from(p.base_controller != actor)] = 1.;
                }
            }
            _ => (),
        }
    }
    x
}

// Additive v2 representation. V1 features and action encoding stay frozen.
pub const STACK_SLOTS: usize = 16;
pub const STACK_STATE_DIM: usize = STATE_DIM + 2 * STACK_SLOTS;

pub fn stack_state_features(e: &Engine, actor: Player) -> [f32; STACK_STATE_DIM] {
    let mut x = [0.; STACK_STATE_DIM];
    x[..STATE_DIM].copy_from_slice(&state_features(e, actor));
    if let State::Playing { world, .. } = &e.state {
        // Engine stack entries are individual objects, stored bottom to top.
        // Include abilities as well as spells; never truncate to the top object.
        assert!(world.stack.len as usize <= STACK_SLOTS);
        for (i, item) in entries(&world.stack).iter().enumerate() {
            x[STATE_DIM + 2 * i + usize::from(item.controller != actor)] = 1.;
        }
    }
    x
}
