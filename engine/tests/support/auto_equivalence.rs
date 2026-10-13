use super::*;
#[path = "auto_reference.rs"]
mod reference;

fn compare(e: &Engine) -> AutoStep {
    let old = reference::reference_step(e).unwrap();
    // Apply the approved target and fetch policies to the frozen oracle.
    let old = match old {
        AutoStep::Decision { actor, actions } if actions.small().is_some() => {
            let kept: Vec<_> = (0..actions.len()).map(|i| actions.get(i)).filter(|a| {
                let has_land = matches!(e.state, State::Playing { .. }) && entries(&e.world().zones.library[pid(actor)])
                    .iter().any(|c| matches!(*c, MOUNTAIN | FOUNDRY));
                if matches!(a, Action::Fetch { .. }) && !has_land { return false; }
                if matches!(a, Action::Search { card: None }) && has_land { return false; }
                let targets = match a {
                    Action::Cast { card: CHARM, mode: 1 | 2, .. } => return true,
                    Action::Cast { targets, .. } => targets.to_vec(),
                    Action::SuspendTarget { target } => vec![*target],
                    _ => return true,
                };
                !targets.iter().any(|t| match t {
                    Subject::Player { player } => *player == actor,
                    Subject::Object { id } => e.permanent(*id).is_some_and(|p| p.base_controller == actor)
                        && !entries(&e.world().stack).iter().any(|s| s.controller != actor
                            && (s.targets.start..s.targets.start+s.targets.len).any(|i| e.world().targets.data[i as usize].subject == *t)),
                    _ => false,
                })
            }).collect();
            if kept.len() == 1 {
                AutoStep::Automatic { actor, action: kept[0], reason: "only auto action" }
            } else {
                AutoStep::Decision { actor, actions: ActionSpace::from_macro_actions(kept) }
            }
        }
        other => other,
    };
    let new = auto_step(e).unwrap();
    match (&old, &new) {
        (
            AutoStep::Decision {
                actor: a,
                actions: x,
            },
            AutoStep::Decision {
                actor: b,
                actions: y,
            },
        ) => {
            assert_eq!(a, b);
            assert_eq!(x.len(), y.len());
            // Exhaust all ordinary spaces; lazy combat spaces share unchanged code.
            let n = x.len();
            let indices: Vec<u128> = if n <= 10000 {
                (0..n).collect()
            } else {
                vec![0, 1, n / 4, n / 2, n - 2, n - 1]
            };
            for i in indices {
                assert_eq!(x.get(i), y.get(i), "candidate {i}");
            }
        }
        _ => assert_eq!(format!("{old:?}"), format!("{new:?}")),
    }
    new
}

#[test]
fn only_approved_action_pruning_across_256_complete_games() {
    let mut boundaries = 0;
    for seed in 1000..1256 {
        let mut rng = Random(seed);
        let mut p =
            Position::new(Engine::new([deck(&mut rng), deck(&mut rng)], Player(0)).unwrap());
        for _ in 0..10000 {
            boundaries += 1;
            let (actor, action) = match compare(&p.engine) {
                AutoStep::Finished(_) => break,
                AutoStep::Chance => (Player(0), chance_action(&p.engine, &mut rng).unwrap()),
                AutoStep::Automatic { actor, action, .. } => (actor, action),
                AutoStep::Decision { actor, actions } => {
                    (actor, actions.get(rng.below(actions.len())))
                }
            };
            p.apply(actor, action).unwrap();
            p.audit().unwrap();
        }
        assert!(p.engine.finished().is_some(), "seed {seed}");
    }
    eprintln!("equivalent auto boundaries: {boundaries}");
}

#[test]
fn unchanged_payment_with_surplus_colors_life_land_order_and_taps() {
    let mut checked = 0;
    for life in [1, 2, 4, 20] {
        for order in [[MOUNTAIN, FOUNDRY, CANYON], [CANYON, VANTAGE, MOUNTAIN]] {
            for mask in 0..8 {
                for mana_code in 0..729u32 {
                    let mut w = mtg_state_design::burn::World::default();
                    w.identity.next = 1;
                    w.players[0].life = life;
                    w.players[1].life = 20;
                    let mut code = mana_code;
                    for v in &mut w.players[0].mana {
                        *v = code % 3;
                        code /= 3;
                    }
                    let base = Engine::fixture(w, Player(0), 3);
                    for card in [BOLT, HELIX, SKEWER, EIDOLON] {
                        let mut e = base;
                        for (i, land) in order.iter().enumerate() {
                            e.enter(*land, Player(0), mask & (1 << i) != 0).unwrap();
                        }
                        e.world_mut().zones.hand[0][card.0 as usize] = 1;
                        let a = Action::Cast {
                            card,
                            targets: if card == EIDOLON {
                                [Subject::Absent; 2]
                            } else {
                                [Subject::Player { player: Player(1) }, Subject::Absent]
                            },
                            mode: 0,
                            spectacle: false,
                        };
                        let space = ActionSpace::build(&e).unwrap();
                        if !(0..space.len()).any(|i| space.get(i) == a) {
                            continue;
                        }
                        e.apply(Player(0), a).unwrap();
                        while matches!(e.execution(),Execution::Running{running} if matches!(running.choice,Choice::Mana{..}))
                        {
                            let AutoStep::Automatic { actor, action, .. } = compare(&e) else {
                                panic!("payment decision")
                            };
                            e.apply(actor, action).unwrap();
                            checked += 1;
                        }
                    }
                }
            }
        }
    }
    eprintln!("equivalent payment boundaries: {checked}");
}

#[path = "redundancy_targets.rs"]
mod redundancy_targets;
