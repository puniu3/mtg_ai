use super::*;
use mtg_state_design::burn::World;
fn fixture(life: i32) -> Engine {
    let mut w = World::default();
    w.identity.next = 1;
    w.players[0].life = life;
    w.players[1].life = 20;
    for p in 0..2 {
        for _ in 0..12 {
            w.zones.library[p].push(MOUNTAIN).unwrap();
        }
    }
    Engine::fixture(w, Player(0), 3)
}
fn cast(card: Card, player: u8) -> Action {
    Action::Cast {
        card,
        targets: if creature(card) {
            [Subject::Absent; 2]
        } else {
            [
                Subject::Player {
                    player: Player(player),
                },
                Subject::Absent,
            ]
        },
        mode: 0,
        spectacle: false,
    }
}
fn settle(e: &mut Engine) {
    for _ in 0..100 {
        if e.finished().is_some() {
            return;
        }
        if matches!(e.execution(), Execution::Priority { .. }) && e.world().stack.len == 0 {
            return;
        }
        match auto_step(e).unwrap() {
            AutoStep::Automatic { actor, action, .. } => e.apply(actor, action).unwrap(),
            AutoStep::Decision { actor, .. } => e.apply(actor, Action::Pass).unwrap(),
            x => panic!("unexpected {x:?}"),
        }
    }
    panic!("did not settle")
}
#[test]
fn eidolon_counts_both_sides_and_helix_cannot_rescue_cast() {
    for actor in [Player(0), Player(1)] {
        for life in 1..=7 {
            for owners in [
                vec![],
                vec![Player(0)],
                vec![Player(1)],
                vec![Player(0), Player(1)],
            ] {
                for card in [BOLT, HELIX, EIDOLON, SKEWER] {
                    let mut e = fixture(20);
                    e.world_mut().players[pid(actor)].life = life;
                    // Both sides may cast on their own main phase.
                    if let State::Playing {
                        progress,
                        execution,
                        ..
                    } = &mut e.state
                    {
                        progress.turn.active = actor;
                        *execution = Execution::Priority {
                            priority: Priority {
                                player: actor,
                                consecutive_passes: 0,
                            },
                        };
                    }
                    for owner in &owners {
                        e.enter(EIDOLON, *owner, false).unwrap();
                    }
                    e.world_mut().players[pid(actor)].mana = [1, 0, 0, 3, 0, 0];
                    e.world_mut().zones.hand[pid(actor)][card.0 as usize] = 1;
                    let a = cast(card, other(actor).0);
                    let expected = life <= owners.len() as i32 * 2;
                    assert_eq!(Context::new(&e, actor).excludes(&e, actor, a), expected);
                    e.apply(actor, a).unwrap();
                    settle(&mut e);
                    assert_eq!(
                        matches!(e.finished(),Some(Outcome::Won{winner}) if winner!=actor),
                        expected
                    );
                }
            }
        }
    }
}
#[test]
fn estimated_payment_loss_matches_actual_auto_payment() {
    let mut checked = 0;
    for life in [1, 2, 4, 20] {
        for order in [[MOUNTAIN, FOUNDRY, CANYON], [CANYON, VANTAGE, MOUNTAIN]] {
            for mask in 0..8 {
                for mana_code in 0..729u32 {
                    let mut base = fixture(life);
                    let mut code = mana_code;
                    for v in &mut base.world_mut().players[0].mana {
                        *v = code % 3;
                        code /= 3;
                    }
                    for (i, land) in order.into_iter().enumerate() {
                        base.enter(land, Player(0), mask & (1 << i) != 0).unwrap();
                    }
                    for card in [BOLT, HELIX, SKEWER, EIDOLON] {
                        let mut e = base;
                        e.world_mut().zones.hand[0][card.0 as usize] = 1;
                        if !actions::affordable(&e, Player(0), cost(card, false), None) {
                            continue;
                        }
                        let expected =
                            Context::new(&e, Player(0)).payment_loss(&e, cost(card, false), None);
                        e.apply(Player(0), cast(card, 1)).unwrap();
                        let mut loss = 0;
                        while matches!(e.execution(),Execution::Running{running} if matches!(running.choice,Choice::Mana{..}))
                        {
                            let AutoStep::Automatic { actor, action, .. } = auto_step(&e).unwrap()
                            else {
                                panic!()
                            };
                            if let Action::Tap { object, .. } = action {
                                if card_of(e.permanent(object).unwrap()) == CANYON {
                                    loss += 1;
                                }
                            }
                            e.apply(actor, action).unwrap();
                        }
                        assert_eq!(expected, loss);
                        checked += 1;
                    }
                }
            }
        }
    }
    assert!(checked > 100000);
    eprintln!("payment plans checked: {checked}");
}
#[test]
fn costs_free_suspend_and_self_damage_boundaries() {
    let mut e = fixture(1);
    let canyon = e.enter(CANYON, Player(0), false).unwrap();
    let fetch = e.enter(MESA, Player(0), false).unwrap();
    let context = Context::new(&e, Player(0));
    assert!(context.excludes(&e, Player(0), Action::Fetch { object: fetch }));
    assert!(context.excludes(&e, Player(0), Action::Suspend));
    assert!(context.excludes(&e, Player(0), cast(BOLT, 1)));
    e.world_mut().players[0].life = 2;
    assert!(Context::new(&e, Player(0)).excludes(&e, Player(0), Action::Shock { pay: true }));
    e.enter(MOUNTAIN, Player(0), false).unwrap();
    let c = Context::new(&e, Player(0));
    assert!(!c.excludes(&e, Player(0), Action::Canyon { object: canyon }));
    e.enter(EIDOLON, Player(1), false).unwrap();
    let c = Context::new(&e, Player(0));
    assert!(!c.excludes(&e, Player(0), Action::Suspend));
    assert!(c.excludes(
        &e,
        Player(0),
        Action::SuspendTarget {
            target: Subject::Player { player: Player(1) }
        }
    ));
    assert!(!c.excludes(&e, Player(0), Action::DeclineSuspend));
    let mut e = fixture(3);
    e.world_mut().players[0].mana = [1, 0, 0, 1, 0, 0];
    let c = Context::new(&e, Player(0));
    assert!(c.excludes(&e, Player(0), cast(BOLT, 0)));
    assert!(!c.excludes(&e, Player(0), cast(HELIX, 0)));
    e.world_mut().zones.hand[0][HELIX.0 as usize] = 1;
    e.apply(Player(0), cast(HELIX, 0)).unwrap();
    settle(&mut e);
    assert_eq!(e.world().players[0].life, 3);
}
#[test]
fn original_legal_indices_and_human_options_are_preserved() {
    let mut e = fixture(2);
    e.enter(EIDOLON, Player(1), false).unwrap();
    e.world_mut().zones.hand[0][BOLT.0 as usize] = 1;
    e.world_mut().players[0].mana[RED] = 1;
    let o = Position::new(e).observe(Player(0));
    let raw = o.auto_actions().unwrap();
    assert!(raw.len() > 1);
    let choices = SearchChoices::new(&e, Player(0), &raw).unwrap();
    assert_eq!(choices.len(), 1);
    assert_eq!(raw.get(choices.index(0)), Action::Pass);
    let report = search(
        &o,
        Config {
            budget: Budget::Trials(64),
            ..Default::default()
        },
    );
    assert_eq!(report.error, None);
    assert_eq!(report.selected, Some(Action::Pass));
    assert!(report.pruned_actions > 0);
    assert_eq!(report.details["search_count"], "1");
    assert_eq!(report.details["candidates"].as_array().unwrap().len(), 1);
    // The same legal suicide is still accepted by the untouched primitive engine.
    e.apply(Player(0), cast(BOLT, 1)).unwrap();
    settle(&mut e);
    assert_eq!(e.finished(), Some(Outcome::Won { winner: Player(1) }));
}
#[test]
fn filtered_random_games_finish_for_both_players() {
    let mut excluded = 0;
    for seed in 3000..3256 {
        let mut rng = Random(seed);
        let mut p =
            Position::new(Engine::new([deck(&mut rng), deck(&mut rng)], Player(0)).unwrap());
        for _ in 0..10000 {
            let (actor, action) = match auto_step(&p.engine).unwrap() {
                AutoStep::Finished(_) => break,
                AutoStep::Chance => (Player(0), chance_action(&p.engine, &mut rng).unwrap()),
                AutoStep::Automatic { actor, action, .. } => (actor, action),
                AutoStep::Decision { actor, actions } => {
                    let choices = SearchChoices::new(&p.engine, actor, &actions).unwrap();
                    excluded += actions.len() - choices.len();
                    (actor, actions.get(choices.index(rng.below(choices.len()))))
                }
            };
            p.apply(actor, action).unwrap();
            p.audit().unwrap();
        }
        assert!(p.engine.finished().is_some(), "seed {seed}");
    }
    assert!(excluded > 0);
    eprintln!("excluded candidates in 256 games: {excluded}");
}

#[test]
fn eidolon_lock_attack_is_a_win_without_opponent_suicide() {
    let mut e = fixture(1);
    e.world_mut().players[1].life = 4;
    let first = e.enter(EIDOLON, Player(0), false).unwrap();
    let second = e.enter(EIDOLON, Player(0), false).unwrap();
    for per in &mut e.world_mut().battlefield.data[..2] {
        per.status |= CONTROLLED;
    }
    for player in [Player(0), Player(1)] {
        e.world_mut().zones.hand[pid(player)][HELIX.0 as usize] = 1;
        e.enter(FOUNDRY, player, false).unwrap();
        e.enter(MOUNTAIN, player, false).unwrap();
    }
    // Advance only the empty main/begin-combat priority windows to attackers.
    for _ in 0..10 {
        let step = auto_step(&e).unwrap();
        if matches!(&step,AutoStep::Decision{actions,..} if matches!(actions.get(0),Action::Attack{..}))
        {
            break;
        }
        e.apply(e.actor().unwrap(), Action::Pass).unwrap();
    }
    let report = search(
        &Position::new(e).observe(Player(0)),
        Config {
            budget: Budget::Trials(256),
            seed: 90,
            ..Default::default()
        },
    );
    assert_eq!(report.error, None);
    let text = format!("Attack [{first:?}, {second:?}]");
    let row = report.details["candidates"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["action"] == text)
        .unwrap();
    assert_eq!(row["wins"], row["visits"]);
    assert_eq!(row["losses"], 0);
    assert_eq!(row["draws"], 0);
}

#[test]
fn pruning_uses_no_hidden_opponent_information() {
    let mut a = fixture(2);
    a.enter(EIDOLON, Player(1), false).unwrap();
    a.enter(MOUNTAIN, Player(0), false).unwrap();
    a.world_mut().zones.hand[0][BOLT.0 as usize] = 1;
    a.world_mut().zones.hand[1][HELIX.0 as usize] = 1;
    let mut b = a;
    b.world_mut().zones.hand[1][HELIX.0 as usize] = 0;
    b.world_mut().zones.hand[1][MOUNTAIN.0 as usize] = 1;
    b.world_mut().zones.library[1].data[0] = HELIX;
    let oa = Position::new(a).observe(Player(0));
    let ob = Position::new(b).observe(Player(0));
    assert_eq!(oa, ob);
    let config = Config {
        budget: Budget::Trials(64),
        seed: 91,
        ..Default::default()
    };
    let x = search(&oa, config);
    let y = search(&ob, config);
    assert_eq!(x.error, None);
    assert_eq!(x.details, y.details);
    assert_eq!(x.selected, y.selected);
    assert!(x.pruned_actions > 0);
}
