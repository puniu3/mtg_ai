use burn_engine::terminal_bot::*;
use burn_engine::*;
use mtg_state_design::burn::{Execution, World};
fn fixture() -> Engine {
    let mut w = World::default();
    w.identity.next = 1;
    w.players = [PlayerCore {
        life: 5,
        mana: [0; 6],
    }; 2];
    for p in 0..2 {
        w.zones.hand[p][BOLT.0 as usize] = 1;
        w.zones.hand[p][HELIX.0 as usize] = 1;
        w.zones.library[p].push(MOUNTAIN).unwrap();
        w.zones.library[p].push(BOLT).unwrap();
    }
    let mut e = Engine::fixture(w, Player(0), 3);
    for p in [Player(0), Player(1)] {
        e.enter(FOUNDRY, p, false).unwrap();
        e.enter(MOUNTAIN, p, false).unwrap();
    }
    e
}
fn all(o: &Observation) -> Vec<Action> {
    let a = o.actions().unwrap();
    assert!(a.len() < 10000);
    (0..a.len()).map(|i| a.get(i)).collect()
}
#[test]
fn hidden_worlds_produce_identical_observations_candidates_and_search() {
    let a = fixture();
    let mut b = a;
    b.world_mut().zones.hand[1][BOLT.0 as usize] -= 1;
    b.world_mut().zones.hand[1][MOUNTAIN.0 as usize] += 1;
    b.world_mut().zones.library[1].data[0] = BOLT;
    b.world_mut().zones.library[0].data[..2].reverse();
    let oa = Position::new(a).observe(Player(0));
    let ob = Position::new(b).observe(Player(0));
    assert_eq!(oa, ob);
    assert_eq!(all(&oa), all(&ob));
    let cfg = Config {
        budget: Budget::Trials(24),
        seed: 76,
        ..Default::default()
    };
    let x = search(&oa, cfg);
    let y = search(&ob, cfg);
    assert_eq!(x.error, None);
    assert_eq!(x.completed, 24);
    assert_eq!(x.selected, y.selected);
    assert_eq!(x.details, y.details);
}
#[test]
fn sampling_is_without_replacement_and_matches_hypergeometric() {
    let o = Position::new(fixture()).observe(Player(0));
    let mut rng = Random(0);
    let mut count = 0;
    for _ in 0..6000 {
        let p = o.sample(&mut rng).unwrap();
        p.audit().unwrap();
        assert_eq!(
            p.engine.world().zones.hand[0],
            fixture().world().zones.hand[0]
        );
        count += p.engine.world().zones.hand[1][BOLT.0 as usize] as usize;
    }
    // Opponent has four unknown cards, two Bolts; two-card sample has E[Bolts]=1.
    assert!((count as f64 / 6000.0 - 1.0).abs() < 0.04, "{count}");
}
#[test]
fn guide_public_nonland_top_survives_sampling_then_moves_to_known_hand() {
    let mut e = fixture();
    e.world_mut().zones.library[1].data[1] = HELIX;
    let guide = e.enter(GUIDE, Player(0), false).unwrap();
    let mut p = Position::new(e);
    let mut r = Random(7);
    let mut attacked = false;
    let mut revealed = false;
    for _ in 0..100 {
        if p.engine.finished().is_some() {
            break;
        }
        let actor = p.engine.actor().unwrap();
        let space = p.observe(actor).actions().unwrap();
        let mut a = Action::Pass;
        for i in 0..space.len() {
            let candidate = space.get(i);
            if matches!(candidate,Action::Attack{objects,len:1} if objects[0]==guide) {
                a = candidate;
                attacked = true;
                break;
            }
        }
        if !matches!(p.engine.execution(), Execution::Priority { .. })
            && !matches!(a, Action::Attack { .. })
        {
            a = space.get(0);
        }
        let events = p.apply(actor, a).unwrap();
        if !events.is_empty() {
            revealed = true;
            let o = p.observe(Player(0));
            for _ in 0..25 {
                let sample = o.sample(&mut r).unwrap();
                assert_eq!(sample.engine.world().zones.library[1].data[1], HELIX);
            }
            break;
        }
    }
    assert!(attacked && revealed);
    // Opponent draws the revealed card; it must remain known even after top leaves.
    for _ in 0..100 {
        let actor = p.engine.actor().unwrap();
        let space = p.observe(actor).actions().unwrap();
        let a = if matches!(p.engine.execution(), Execution::Priority { .. }) {
            Action::Pass
        } else {
            space.get(0)
        };
        p.apply(actor, a).unwrap();
        if p.engine.world().zones.library[1].len == 1 {
            for _ in 0..25 {
                let sample = p.observe(Player(0)).sample(&mut r).unwrap();
                assert!(sample.engine.world().zones.hand[1][HELIX.0 as usize] >= 1);
            }
            return;
        }
    }
    panic!("no draw");
}
#[test]
fn no_unpayable_cast_and_all_payment_sources_preserved() {
    let mut e = fixture();
    for b in &mut e.world_mut().battlefield.data[..2] {
        b.status |= TAPPED;
    }
    let choices = all(&Position::new(e).observe(Player(0)));
    assert!(!choices.iter().any(|a| matches!(a, Action::Cast { .. })));
    let mut e = fixture();
    e.apply(
        Player(0),
        Action::Cast {
            card: BOLT,
            targets: [Subject::Player { player: Player(1) }, Subject::Absent],
            mode: 0,
            spectacle: false,
        },
    )
    .unwrap();
    let choices = all(&Position::new(e).observe(Player(0)));
    let red = choices
        .iter()
        .filter(|a| matches!(a,Action::Tap{color,..} if *color==RED as u8))
        .count();
    assert_eq!(red, 2);
    for a in choices {
        let mut c = e;
        c.apply(Player(0), a).unwrap();
    }
}
#[test]
fn terminal_only_failures_have_no_selected_move_or_reward() {
    let o = Position::new(fixture()).observe(Player(0));
    let report = search(
        &o,
        Config {
            budget: Budget::Trials(3),
            max_trial_steps: 1,
            ..Default::default()
        },
    );
    assert_eq!(report.completed, 0);
    assert_eq!(report.incomplete, 1);
    assert!(report.selected.is_none());
    assert!(report.error.is_some());
}
#[test]
fn fixed_seed_replays_complete_sixty_card_games() {
    for seed in 1..=8 {
        let mut deck_rng = Random(seed);
        let e = Engine::new(
            [deck(&mut deck_rng), deck(&mut deck_rng)],
            Player((seed % 2) as u8),
        )
        .unwrap();
        let mut p = Position::new(e);
        let mut rng = Random(seed + 100);
        for _ in 0..20000 {
            if p.engine.finished().is_some() {
                break;
            }
            if p.engine.actor().is_none() {
                resolve_chance(&mut p, &mut rng).unwrap();
            } else {
                let actor = p.engine.actor().unwrap();
                let o = p.observe(actor);
                let space = o.actions().unwrap();
                let a = space.get(rng.below(space.len()));
                p.apply(actor, a).unwrap();
            }
            p.audit().unwrap();
        }
        assert!(p.engine.finished().is_some(), "seed {seed}");
    }
}

// Independent exhaustive terminal enumerator for deliberately finite, no-chance
// fixtures. No material/heuristic score and no horizon evaluation.
fn exact_uniform(p: Position, visited: &mut u64) -> f64 {
    *visited += 1;
    assert!(*visited < 100000, "fixture too large for exhaustive check");
    if let Some(outcome) = p.engine.finished() {
        return match outcome {
            Outcome::Won { winner: Player(0) } => 1.0,
            Outcome::Draw => 0.5,
            _ => 0.0,
        };
    }
    let actor = p.engine.actor().unwrap();
    let actions = p.observe(actor).actions().unwrap();
    let mut sum = 0.0;
    for i in 0..actions.len() {
        let mut child = p.clone();
        child.apply(actor, actions.get(i)).unwrap();
        sum += exact_uniform(child, visited);
    }
    sum / actions.len() as f64
}
fn tiny(life: [i32; 2]) -> Engine {
    let mut w = World::default();
    w.identity.next = 1;
    for p in 0..2 {
        w.players[p].life = life[p];
    }
    Engine::fixture(w, Player(0), 3)
}
#[test]
fn exact_terminal_enumeration_and_search_agree_on_lethal_and_self_kill() {
    let mut e = tiny([1, 3]);
    e.world_mut().zones.hand[0][BOLT.0 as usize] = 1;
    e.world_mut().players[0].mana[RED] = 1;
    let p = Position::new(e);
    let o = p.observe(Player(0));
    let actions = o.actions().unwrap();
    let mut nodes = 0;
    for i in 0..actions.len() {
        let a = actions.get(i);
        let mut child = p.clone();
        child.apply(Player(0), a).unwrap();
        let value = exact_uniform(child, &mut nodes);
        let expected = if matches!(
            a,
            Action::Cast {
                targets: [Subject::Player { player: Player(0) }, _],
                ..
            }
        ) {
            0.0
        } else {
            1.0
        };
        assert_eq!(value, expected);
    }
    let report = search(
        &o,
        Config {
            budget: Budget::Trials(128),
            ..Default::default()
        },
    );
    assert_eq!(report.error, None);
    let auto_actions = o.auto_actions().unwrap();
    for row in report.details["candidates"].as_array().unwrap() {
        let index = row["index"].as_str().unwrap().parse::<u128>().unwrap();
        let mut child = p.clone();
        child.apply(Player(0), auto_actions.get(index)).unwrap();
        assert_eq!(row["mean"].as_f64().unwrap(), exact_uniform(child, &mut 0));
    }
}
#[test]
fn finds_helix_response_instead_of_passing_to_lethal() {
    let mut e = tiny([3, 3]);
    e.world_mut().zones.hand[0][BOLT.0 as usize] = 1;
    e.world_mut().zones.hand[1][HELIX.0 as usize] = 1;
    e.world_mut().players[0].mana[RED] = 1;
    e.world_mut().players[1].mana[RED] = 1;
    e.world_mut().players[1].mana[WHITE] = 1;
    e.apply(
        Player(0),
        Action::Cast {
            card: BOLT,
            targets: [Subject::Player { player: Player(1) }, Subject::Absent],
            mode: 0,
            spectacle: false,
        },
    )
    .unwrap();
    let mut mana = [0; 6];
    mana[RED] = 1;
    e.apply(Player(0), Action::Pay { mana }).unwrap();
    e.apply(Player(0), Action::Pass).unwrap();
    let p = Position::new(e);
    let o = p.observe(Player(1));
    let report = search(
        &o,
        Config {
            budget: Budget::Trials(256),
            seed: 90,
            ..Default::default()
        },
    );
    assert_eq!(report.error, None);
    assert!(
        matches!(
            report.selected,
            Some(Action::Cast {
                card: HELIX,
                targets: [Subject::Player { player: Player(0) }, _],
                ..
            })
        ),
        "{:?}",
        report.json()
    );
    for a in all(&o) {
        let mut child = p.clone();
        child.apply(Player(1), a).unwrap();
        let value = exact_uniform(child, &mut 0);
        let expected = if matches!(
            a,
            Action::Cast {
                targets: [Subject::Player { player: Player(0) }, _],
                ..
            }
        ) {
            0.0
        } else {
            1.0
        };
        assert_eq!(value, expected);
    }
}
#[test]
fn eidolon_self_damage_is_a_terminal_loss_and_deckout_is_terminal() {
    let mut e = tiny([2, 3]);
    e.enter(EIDOLON, Player(1), true).unwrap();
    e.world_mut().zones.hand[0][BOLT.0 as usize] = 1;
    e.world_mut().players[0].mana[RED] = 1;
    let mut p = Position::new(e);
    p.apply(
        Player(0),
        Action::Cast {
            card: BOLT,
            targets: [Subject::Player { player: Player(1) }, Subject::Absent],
            mode: 0,
            spectacle: false,
        },
    )
    .unwrap();
    let mut mana = [0; 6];
    mana[RED] = 1;
    p.apply(Player(0), Action::Pay { mana }).unwrap();
    for _ in 0..12 {
        if p.engine.finished().is_some() {
            break;
        }
        let actor = p.engine.actor().unwrap();
        let a = if matches!(p.engine.execution(), Execution::Priority { .. }) {
            Action::Pass
        } else {
            p.observe(actor).actions().unwrap().get(0)
        };
        p.apply(actor, a).unwrap();
    }
    assert_eq!(
        p.engine.finished(),
        Some(Outcome::Won { winner: Player(1) })
    );
    assert_eq!(exact_uniform(Position::new(tiny([20, 20])), &mut 0), 1.0);
}
#[test]
fn attack_block_and_all_damage_divisions_are_available() {
    let mut e = tiny([20, 20]);
    let attacker = e.enter(GUIDE, Player(0), false).unwrap();
    let b1 = e.enter(EIDOLON, Player(1), false).unwrap();
    let b2 = e.enter(EIDOLON, Player(1), false).unwrap();
    let mut p = Position::new(e);
    let mut saw_attack = false;
    let mut saw_block = false;
    for _ in 0..50 {
        let actor = p.engine.actor().unwrap();
        let space = p.observe(actor).actions().unwrap();
        let attack = (0..space.len())
            .map(|i| space.get(i))
            .find(|a| matches!(a, Action::Attack { len: 1, .. }));
        let a = if let Some(a) = attack {
            saw_attack = true;
            assert_eq!(space.len(), 2);
            a
        } else if matches!(space.get(0), Action::Block { .. }) {
            saw_block = true;
            assert_eq!(space.len(), 4);
            (0..space.len()).map(|i|space.get(i)).find(|a|matches!(a,Action::Block{pairs,len:2} if pairs[..2].contains(&(b1,attacker))&&pairs[..2].contains(&(b2,attacker)))).unwrap()
        } else if matches!(space.get(0), Action::Damage { .. }) {
            assert_eq!(space.len(), 3);
            let mut divisions = Vec::new();
            for i in 0..space.len() {
                let a = space.get(i);
                let mut child = p.clone();
                child.apply(actor, a).unwrap();
                if let Action::Damage { assignments, len } = a {
                    assert_eq!(len, 2);
                    divisions.push((assignments[0].2, assignments[1].2));
                }
            }
            assert_eq!(divisions, vec![(0, 2), (1, 1), (2, 0)]);
            assert!(saw_attack && saw_block);
            return;
        } else if matches!(p.engine.execution(), Execution::Priority { .. }) {
            Action::Pass
        } else {
            space.get(0)
        };
        p.apply(actor, a).unwrap();
    }
    panic!("did not reach damage choice");
}
#[test]
fn private_bottom_is_remembered_only_by_its_owner_and_shuffle_clears_it() {
    fn bottom(e: &Engine) -> Card {
        match &e.state {
            mtg_state_design::burn::State::Mulligan { setup, .. } => setup.zones.library[0].data[0],
            mtg_state_design::burn::State::Playing { world, .. } => world.zones.library[0].data[0],
            _ => panic!("state"),
        }
    }
    let mut rng = Random(192);
    let mut p = Position::new(Engine::new([deck(&mut rng), deck(&mut rng)], Player(0)).unwrap());
    p.apply(Player(0), Action::Mulligan { take: true }).unwrap();
    p.apply(Player(1), Action::Mulligan { take: false })
        .unwrap();
    resolve_chance(&mut p, &mut rng).unwrap();
    let actor = p.engine.actor().unwrap();
    assert_eq!(actor, Player(0));
    let bottom_action = p.observe(actor).actions().unwrap().get(0);
    let Action::Bottom { card } = bottom_action else {
        panic!("not bottom")
    };
    p.apply(actor, bottom_action).unwrap();
    let o = p.observe(actor);
    for _ in 0..30 {
        let sampled = o.sample(&mut rng).unwrap();
        assert_eq!(bottom(&sampled.engine), card);
        sampled.audit().unwrap();
    }
    let opponent = p.observe(Player(1));
    let mut differs = false;
    for _ in 0..30 {
        let sampled = opponent.sample(&mut rng).unwrap();
        differs |= bottom(&sampled.engine) != card;
    }
    assert!(differs);
    p.apply(Player(0), Action::Mulligan { take: true }).unwrap();
    resolve_chance(&mut p, &mut rng).unwrap();
    let o = p.observe(Player(0));
    let mut differs = false;
    for _ in 0..30 {
        let sampled = o.sample(&mut rng).unwrap();
        sampled.audit().unwrap();
        differs |= bottom(&sampled.engine) != card;
    }
    assert!(differs);
}

#[test]
fn large_block_space_is_lazy_and_not_truncated() {
    use mtg_state_design::burn::{Combat, Phase};
    let mut e = tiny([20, 20]);
    let mut combat = Combat::default();
    for _ in 0..8 {
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
        e.enter(EIDOLON, Player(1), false).unwrap();
    }
    if let mtg_state_design::burn::State::Playing {
        progress,
        execution,
        ..
    } = &mut e.state
    {
        progress.phase = Phase::Combat { combat };
        let mut running = mtg_state_design::burn::Running::default();
        running.choice = Choice::Select {
            player: Player(1),
            rule: Behavior(301),
            min: 0,
            max: 24,
            selected: Span::default(),
        };
        *execution = Execution::Running { running };
    }
    let o = Position::new(e).observe(Player(1));
    let space = o.actions().unwrap();
    assert_eq!(space.len(), 9u128.pow(8));
    for index in [0, 4096, space.len() / 2, space.len() - 1] {
        let mut copy = e;
        copy.apply(Player(1), space.get(index)).unwrap();
    }
}
#[test]
fn rejected_action_does_not_change_observation_or_knowledge() {
    let mut p = Position::new(fixture());
    let before = p.observe(Player(0));
    assert!(p.apply(Player(1), Action::Pass).is_err());
    assert_eq!(before, p.observe(Player(0)));
}

#[test]
fn payment_feasibility_matches_exhaustive_primitive_payment_paths() {
    fn completes(e: Engine) -> bool {
        for r in 0..=3 {
            for w in 0..=3 {
                let mut mana = [0; 6];
                mana[RED] = r;
                mana[WHITE] = w;
                let mut c = e;
                if c.apply(Player(0), Action::Pay { mana }).is_ok() {
                    return true;
                }
            }
        }
        for b in &e.world().battlefield.data[..e.world().battlefield.len as usize] {
            for color in [RED, WHITE] {
                let mut c = e;
                if c.apply(
                    Player(0),
                    Action::Tap {
                        object: b.ids.first,
                        color: color as u8,
                    },
                )
                .is_ok()
                    && completes(c)
                {
                    return true;
                }
            }
        }
        false
    }
    for life in 1..=3 {
        for mask in 0..8 {
            for card in [BOLT, HELIX, SKEWER] {
                let mut e = tiny([life, 20]);
                for (i, land) in [MOUNTAIN, FOUNDRY, CANYON].into_iter().enumerate() {
                    e.enter(land, Player(0), mask & (1 << i) != 0).unwrap();
                }
                e.world_mut().zones.hand[0][card.0 as usize] = 1;
                let cast = Action::Cast {
                    card,
                    targets: [Subject::Player { player: Player(1) }, Subject::Absent],
                    mode: 0,
                    spectacle: false,
                };
                let available = all(&Position::new(e).observe(Player(0))).contains(&cast);
                let mut paying = e;
                paying.apply(Player(0), cast).unwrap();
                assert_eq!(
                    available,
                    completes(paying),
                    "life {life}, mask {mask}, card {card:?}"
                );
            }
        }
    }
}
