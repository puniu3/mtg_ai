use burn_engine::{terminal_bot::*, *};
use mtg_state_design::burn::{Combat, Execution, Phase, State, World};

fn fixture() -> Engine {
    let mut w = World::default();
    w.identity.next = 1;
    for p in 0..2 {
        w.players[p].life = 20;
        w.zones.library[p].push(MOUNTAIN).unwrap();
        w.zones.library[p].push(BOLT).unwrap();
    }
    Engine::fixture(w, Player(0), 3)
}
fn priority(e: &mut Engine, actor: Player, phase: Phase) {
    if let State::Playing {
        execution,
        progress,
        ..
    } = &mut e.state
    {
        *execution = Execution::Priority {
            priority: Priority {
                player: actor,
                consecutive_passes: 0,
            },
        };
        progress.phase = phase;
    } else {
        panic!("not playing")
    }
}
fn cast(card: Card, target: u8) -> Action {
    Action::Cast {
        card,
        targets: [
            Subject::Player {
                player: Player(target),
            },
            Subject::Absent,
        ],
        mode: 0,
        spectacle: false,
    }
}
fn is_pass(e: &Engine) -> bool {
    matches!(
        auto_step(e).unwrap(),
        AutoStep::Automatic {
            action: Action::Pass,
            ..
        }
    )
}
fn choices(e: &Engine) -> Vec<Action> {
    match auto_step(e).unwrap() {
        AutoStep::Decision { actions, .. } => (0..actions.len()).map(|i| actions.get(i)).collect(),
        x => panic!("expected choice: {x:?}"),
    }
}
fn pay(e: &mut Engine) -> Vec<Action> {
    let mut trace = Vec::new();
    for _ in 0..10 {
        if !matches!(e.execution(), Execution::Running { running } if matches!(running.choice, Choice::Mana { .. }))
        {
            return trace;
        }
        let AutoStep::Automatic { actor, action, .. } = auto_step(e).unwrap() else {
            panic!("manual payment")
        };
        trace.push(action);
        e.apply(actor, action).unwrap();
    }
    panic!("payment loop")
}

#[test]
fn empty_stack_stops_and_nonempty_stack_overrides_every_phase() {
    let mut e = fixture();
    for p in 0..2 {
        e.world_mut().zones.hand[p][BOLT.0 as usize] = 2;
        e.enter(MOUNTAIN, Player(p as u8), false).unwrap();
    }
    let mut combat = Combat::default();
    combat.header.step = CombatStep::Blockers {};
    let id = e.enter(GUIDE, Player(0), true).unwrap();
    combat
        .attackers
        .push(Attacker {
            attackers: IdRange {
                first: id,
                count: 1,
            },
            defender: Subject::Player { player: Player(1) },
            was_blocked: false,
        })
        .unwrap();
    let phases = [
        (
            Phase::Beginning {
                step: BeginningStep::Upkeep {},
            },
            [false, false],
        ),
        (
            Phase::Beginning {
                step: BeginningStep::Draw {},
            },
            [false, false],
        ),
        (Phase::Main { postcombat: false }, [true, false]),
        (Phase::Combat { combat }, [false, true]),
        (Phase::Main { postcombat: true }, [true, false]),
        (Phase::Ending { cleanup: false }, [false, true]),
    ];
    for (phase, stops) in phases {
        for (p, stop) in stops.into_iter().enumerate() {
            priority(&mut e, Player(p as u8), phase);
            assert_eq!(!is_pass(&e), stop, "{phase:?} P{p}");
        }
    }
    priority(&mut e, Player(0), Phase::Main { postcombat: false });
    e.apply(Player(0), cast(BOLT, 1)).unwrap();
    pay(&mut e);
    // Restore a source for the caster: responding to our OWN stack is allowed.
    e.enter(MOUNTAIN, Player(0), false).unwrap();
    for (phase, _) in phases {
        for p in [Player(0), Player(1)] {
            priority(&mut e, p, phase);
            assert!(choices(&e).iter().any(|a| matches!(a, Action::Cast { .. })));
        }
    }
}

#[test]
fn no_response_ignores_bare_taps_but_keeps_casts_fetches_and_canyon() {
    let mut e = fixture();
    e.enter(MOUNTAIN, Player(0), false).unwrap();
    assert!(is_pass(&e));
    e.world_mut().zones.hand[0][BOLT.0 as usize] = 1;
    assert!(choices(&e).contains(&cast(BOLT, 1)));
    assert!(!choices(&e).iter().any(|a| matches!(a, Action::Tap { .. })));
    e.world_mut().zones.hand[0][BOLT.0 as usize] = 0;
    let fetch = e.enter(MESA, Player(0), false).unwrap();
    assert!(choices(&e).contains(&Action::Fetch { object: fetch }));
    let canyon = e.enter(CANYON, Player(0), false).unwrap();
    assert!(choices(&e).contains(&Action::Canyon { object: canyon }));
}

#[test]
fn payment_uses_mountain_then_dual_then_pain_and_only_needed_colors() {
    let mut e = fixture();
    let pain = e.enter(CANYON, Player(0), false).unwrap();
    let dual = e.enter(FOUNDRY, Player(0), false).unwrap();
    let mountain = e.enter(MOUNTAIN, Player(0), false).unwrap();
    e.world_mut().zones.hand[0][HELIX.0 as usize] = 1;
    e.apply(Player(0), cast(HELIX, 1)).unwrap();
    let trace = pay(&mut e);
    assert_eq!(
        trace,
        vec![
            Action::Tap {
                object: mountain,
                color: RED as u8
            },
            Action::Tap {
                object: dual,
                color: WHITE as u8
            },
            Action::Pay {
                mana: [1, 0, 0, 1, 0, 0]
            }
        ]
    );
    assert_eq!(e.world().players[0].life, 20);
    assert_eq!(e.world().battlefield.data[0].ids.first, pain);
    assert_eq!(e.world().battlefield.data[0].status & TAPPED, 0);

    let mut e = fixture();
    e.enter(MOUNTAIN, Player(0), false).unwrap();
    let dual = e.enter(FOUNDRY, Player(0), false).unwrap();
    e.world_mut().players[0].mana[RED] = 1;
    e.world_mut().zones.hand[0][HELIX.0 as usize] = 1;
    e.apply(Player(0), cast(HELIX, 1)).unwrap();
    assert_eq!(
        pay(&mut e),
        vec![
            Action::Tap {
                object: dual,
                color: WHITE as u8
            },
            Action::Pay {
                mana: [1, 0, 0, 1, 0, 0]
            }
        ]
    );

    let mut e = fixture();
    e.enter(CANYON, Player(0), false).unwrap();
    e.world_mut().zones.hand[0][BOLT.0 as usize] = 1;
    e.apply(Player(0), cast(BOLT, 1)).unwrap();
    assert_eq!(pay(&mut e).len(), 2);
    assert_eq!(e.world().players[0].life, 19);
}

#[test]
fn floating_mana_and_activated_ability_payments_are_automatic() {
    let mut e = fixture();
    e.enter(MOUNTAIN, Player(0), false).unwrap();
    e.world_mut().zones.hand[0][BOLT.0 as usize] = 1;
    e.world_mut().players[0].mana[RED] = 1;
    e.apply(Player(0), cast(BOLT, 1)).unwrap();
    assert_eq!(
        pay(&mut e),
        vec![Action::Pay {
            mana: [0, 0, 0, 1, 0, 0]
        }]
    );
    let mut e = fixture();
    let canyon = e.enter(CANYON, Player(0), false).unwrap();
    let mountain = e.enter(MOUNTAIN, Player(0), false).unwrap();
    e.apply(Player(0), Action::Canyon { object: canyon })
        .unwrap();
    assert_eq!(
        pay(&mut e)[0],
        Action::Tap {
            object: mountain,
            color: RED as u8
        }
    );
    let mut e = fixture();
    let mountain = e.enter(MOUNTAIN, Player(0), false).unwrap();
    e.world_mut().zones.hand[0][RIFT.0 as usize] = 1;
    e.apply(Player(0), Action::Suspend).unwrap();
    assert_eq!(
        pay(&mut e)[0],
        Action::Tap {
            object: mountain,
            color: RED as u8
        }
    );
}

#[test]
fn trigger_order_is_automatic_apnap_and_each_players_order_is_stable() {
    let mut e = fixture();
    for p in [Player(0), Player(1)] {
        for _ in 0..2 {
            e.enter(EIDOLON, p, false).unwrap();
        }
    }
    e.world_mut().zones.hand[0][BOLT.0 as usize] = 1;
    e.world_mut().players[0].mana[RED] = 1;
    e.apply(Player(0), cast(BOLT, 1)).unwrap();
    pay(&mut e);
    let mut ordered = Vec::new();
    while matches!(e.execution(), Execution::Running { running } if matches!(running.choice, Choice::Order { .. }))
    {
        let AutoStep::Automatic {
            actor,
            action: Action::OrderTrigger { trigger },
            ..
        } = auto_step(&e).unwrap()
        else {
            panic!("manual ordering")
        };
        ordered.push((actor.0, trigger.0));
        e.apply(actor, Action::OrderTrigger { trigger }).unwrap();
    }
    // The engine places the last remaining trigger for each controller itself.
    assert_eq!(ordered.iter().map(|x| x.0).collect::<Vec<_>>(), vec![0, 1]);
    let stack = &e.world().stack;
    let triggers = &stack.data[1..stack.len as usize];
    assert_eq!(
        triggers.iter().map(|t| t.controller.0).collect::<Vec<_>>(),
        vec![0, 0, 1, 1]
    );
    assert!(triggers[0].ids.first.0 < triggers[1].ids.first.0);
    assert!(triggers[2].ids.first.0 < triggers[3].ids.first.0);
}

#[test]
fn automatic_states_are_not_search_roots_and_rollouts_use_auto() {
    let mut e = fixture();
    e.world_mut().zones.hand[0][BOLT.0 as usize] = 1;
    e.enter(MOUNTAIN, Player(0), false).unwrap();
    let report = search(
        &Position::new(e).observe(Player(0)),
        Config {
            budget: Budget::Trials(32),
            ..Default::default()
        },
    );
    assert_eq!(report.error, None);
    assert_eq!(report.completed, 32);
    assert!(report.automatic_steps > 0 && report.rollout_decisions > 0);
    for row in report.details["candidates"].as_array().unwrap() {
        assert!(!row["action"].as_str().unwrap().starts_with("Tap"));
    }
    e.apply(Player(0), cast(BOLT, 1)).unwrap();
    let report = search(&Position::new(e).observe(Player(0)), Config::default());
    assert!(report.error.is_some());
    assert_eq!(report.completed, 0);
    assert!(report.selected.is_none());
}

#[test]
fn auto_games_finish_and_primitive_replay_preserves_knowledge_and_inventory() {
    for seed in 1..=16 {
        let mut rng = Random(seed);
        let e = Engine::new([deck(&mut rng), deck(&mut rng)], Player(0)).unwrap();
        let mut p = Position::new(e);
        let mut replay = p.clone();
        for _ in 0..10000 {
            let (actor, action) = match auto_step(&p.engine).unwrap() {
                AutoStep::Finished(_) => break,
                AutoStep::Chance => (Player(0), chance_action(&p.engine, &mut rng).unwrap()),
                AutoStep::Automatic { actor, action, .. } => (actor, action),
                AutoStep::Decision { actor, actions } => {
                    let a = actions.get(rng.below(actions.len()));
                    assert!(!matches!(
                        a,
                        Action::Tap { .. } | Action::Pay { .. } | Action::OrderTrigger { .. }
                    ));
                    assert_eq!(
                        p.observe(actor).auto_actions().unwrap().get(0),
                        actions.get(0)
                    );
                    (actor, a)
                }
            };
            assert_eq!(
                p.apply(actor, action).unwrap(),
                replay.apply(actor, action).unwrap()
            );
            p.audit().unwrap();
            assert_eq!(p.engine, replay.engine);
            if p.engine.finished().is_none() {
                for viewer in [Player(0), Player(1)] {
                    assert_eq!(p.observe(viewer), replay.observe(viewer));
                }
            }
        }
        assert!(p.engine.finished().is_some(), "seed {seed}");
    }
}

#[test]
fn auto_payment_completes_every_affordable_fixture_without_extra_mana() {
    for life in 1..=3 {
        for mask in 0..8 {
            for floating in 0..4 {
                for card in [BOLT, HELIX, SKEWER] {
                    let mut e = fixture();
                    e.world_mut().players[0].life = life;
                    e.world_mut().players[0].mana[RED] = floating % 2;
                    e.world_mut().players[0].mana[WHITE] = floating / 2;
                    for (i, land) in [MOUNTAIN, FOUNDRY, CANYON].into_iter().enumerate() {
                        e.enter(land, Player(0), mask & (1 << i) != 0).unwrap();
                    }
                    e.world_mut().zones.hand[0][card.0 as usize] = 1;
                    let a = cast(card, 1);
                    let primitive = Position::new(e).observe(Player(0)).actions().unwrap();
                    if !(0..primitive.len()).any(|i| primitive.get(i) == a) {
                        continue;
                    }
                    e.apply(Player(0), a).unwrap();
                    let trace = pay(&mut e);
                    assert!(
                        matches!(trace.last(), Some(Action::Pay { .. })),
                        "{life} {mask} {floating} {card:?}"
                    );
                    if e.finished().is_none() {
                        assert!(
                            !matches!(e.execution(), Execution::Running { running } if matches!(running.choice, Choice::Mana { .. }))
                        );
                    }
                }
            }
        }
    }
}
