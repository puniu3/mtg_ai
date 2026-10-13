use super::*;
use mtg_state_design::burn::World;

fn fixture() -> Engine {
    let mut w = World::default();
    w.identity.next = 1;
    for p in 0..2 {
        w.players[p].life = 20;
        w.players[p].mana[RED] = 8;
        w.players[p].mana[WHITE] = 8;
        w.zones.hand[p][BOLT.0 as usize] = 1;
        w.zones.hand[p][HELIX.0 as usize] = 1;
        w.zones.library[p].push(MOUNTAIN).unwrap();
        w.zones.library[p].push(BOLT).unwrap();
    }
    Engine::fixture(w, Player(0), 3)
}
fn row<'a>(v: &'a Value, c: Card) -> &'a Value {
    v.as_array()
        .unwrap()
        .iter()
        .find(|x| x["card_id"] == c.0)
        .unwrap()
}
fn pass(p: &mut Position) {
    p.apply(p.engine.actor().unwrap(), Action::Pass).unwrap();
}

#[test]
fn view_is_identical_for_indistinguishable_hidden_worlds() {
    let a = fixture();
    let mut b = a;
    b.world_mut().zones.hand[1][BOLT.0 as usize] = 0;
    b.world_mut().zones.hand[1][MOUNTAIN.0 as usize] = 1;
    b.world_mut().zones.library[1].data[0] = BOLT;
    b.world_mut().zones.library[0].data[..2].reverse();
    let a = Position::new(a).observe(Player(0));
    let b = Position::new(b).observe(Player(0));
    assert_eq!(a.agent_view(), b.agent_view());
    let actions = a.actions().unwrap();
    for i in 0..actions.len() {
        assert_eq!(a.action_view(actions.get(i)), b.action_view(actions.get(i)));
    }
    let v = a.agent_view();
    assert_eq!(v["players"][0]["library"]["composition_is_exact"], true);
    let remaining = &v["players"][0]["library"]["remaining"];
    assert_eq!(row(remaining, BOLT)["count"], 1);
    assert_eq!(row(remaining, MOUNTAIN)["count"], 1);
    assert_eq!(row(remaining, HELIX)["count"], 0);
    assert_eq!(v["players"][1]["hand"]["unknown_count"], 2);
    assert_eq!(
        row(&v["players"][1]["hand_and_library_remaining"], BOLT)["count"],
        2
    );
}

#[test]
fn opponent_library_bounds_match_exhaustive_hidden_allocations() {
    let v = Position::new(fixture()).observe(Player(0)).agent_view();
    let pool = [BOLT, BOLT, HELIX, MOUNTAIN];
    for c in [BOLT, HELIX, MOUNTAIN] {
        let mut possible = Vec::new();
        for i in 0..4 {
            for j in i + 1..4 {
                possible.push(u64::from(pool[i] == c) + u64::from(pool[j] == c));
            }
        }
        let r = row(&v["players"][1]["library"]["remaining"], c);
        assert_eq!(r["min"].as_u64(), possible.iter().min().copied());
        assert_eq!(r["max"].as_u64(), possible.iter().max().copied());
        assert!(r["count"].is_null());
    }
}

#[test]
fn card_accounting_can_deduce_hand_and_exact_library_without_peeking() {
    let mut p = Position::new(fixture());
    p.memories[0].hand[1][HELIX.0 as usize] = 1;
    p.memories[0].library[1][0] = Some(MOUNTAIN);
    let v = p.observe(Player(0)).agent_view();
    let opponent = &v["players"][1];
    assert_eq!(opponent["hand"]["unknown_count"], 0);
    assert_eq!(row(&opponent["hand"]["known"], BOLT)["count"], 1);
    assert_eq!(row(&opponent["hand"]["known"], HELIX)["count"], 1);
    assert_eq!(opponent["library"]["composition_is_exact"], true);
    assert_eq!(row(&opponent["library"]["remaining"], BOLT)["count"], 1);
    assert_eq!(
        opponent["library"]["known_positions"][0]["position_from_top"],
        2
    );
}

#[test]
fn guide_reveal_moves_to_hand_and_playing_it_removes_known_copy() {
    let mut e = fixture();
    e.world_mut().zones.library[1].data[1] = HELIX;
    let guide = e.enter(GUIDE, Player(0), false).unwrap();
    let mut p = Position::new(e);
    let mut revealed = false;
    for _ in 0..100 {
        let actor = p.engine.actor().unwrap();
        let actions = p.observe(actor).actions().unwrap();
        let attack = (0..actions.len())
            .map(|i| actions.get(i))
            .find(|a| matches!(a,Action::Attack{objects,len:1} if objects[0]==guide));
        let a = attack.unwrap_or_else(|| {
            if matches!(p.engine.execution(), Execution::Priority { .. }) {
                Action::Pass
            } else {
                actions.get(0)
            }
        });
        let events = p.apply(actor, a).unwrap();
        if let Some(event) = events.first() {
            assert_eq!(event.agent_view()["card"], "Lightning Helix");
            assert_eq!(event.agent_view()["destination"], "library_top");
            let v = p.observe(Player(0)).agent_view();
            assert_eq!(
                v["players"][1]["library"]["known_positions"][0]["card"],
                "Lightning Helix"
            );
            revealed = true;
            break;
        }
    }
    assert!(revealed);
    for _ in 0..100 {
        if p.engine.world().zones.library[1].len == 1 {
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
    assert_eq!(p.engine.world().zones.library[1].len, 1);
    let v = p.observe(Player(0)).agent_view();
    assert_eq!(row(&v["players"][1]["hand"]["known"], HELIX)["count"], 1);
    assert!(
        v["players"][1]["library"]["known_positions"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    // A player holding two indistinguishable copies can spend either; retain
    // only the guaranteed remaining known count, not physical-copy identity.
    p.engine.world_mut().players[1].mana[RED] = 1;
    p.engine.world_mut().players[1].mana[WHITE] = 1;
    p.apply(
        Player(1),
        Action::Cast {
            card: HELIX,
            targets: [Subject::Player { player: Player(0) }, Subject::Absent],
            mode: 0,
            spectacle: false,
        },
    )
    .unwrap();
    let v = p.observe(Player(0)).agent_view();
    assert!(
        !v["players"][1]["hand"]["known"]
            .as_array()
            .unwrap()
            .iter()
            .any(|x| x["card_id"] == HELIX.0)
    );
}

#[test]
fn fetch_shuffle_forgets_library_positions_but_preserves_known_hand() {
    let mut e = fixture();
    let mesa = e.enter(MESA, Player(0), false).unwrap();
    let mut p = Position::new(e);
    p.memories[1].hand[0][HELIX.0 as usize] = 1;
    p.public.hand[0][HELIX.0 as usize] = 1;
    p.memories[1].library[0][1] = Some(BOLT);
    p.apply(Player(0), Action::Fetch { object: mesa }).unwrap();
    pass(&mut p);
    pass(&mut p);
    p.apply(
        Player(0),
        Action::Search {
            card: Some(MOUNTAIN),
        },
    )
    .unwrap();
    resolve_chance(&mut p, &mut Random(7)).unwrap();
    p.audit().unwrap();
    let o = p.observe(Player(1));
    let v = o.agent_view();
    assert_eq!(row(&v["players"][0]["hand"]["known"], HELIX)["count"], 1);
    assert!(
        v["players"][0]["library"]["known_positions"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    assert_eq!(p.public.hand[0][HELIX.0 as usize], 1);
    for seed in 0..20 {
        assert!(
            o.sample(&mut Random(seed))
                .unwrap()
                .engine
                .world()
                .zones
                .hand[0][HELIX.0 as usize]
                >= 1
        );
    }
}

#[test]
fn private_bottom_is_visible_only_to_owner_and_mulligan_invalidates_it() {
    let mut rng = Random(192);
    let mut p = Position::new(Engine::new([deck(&mut rng), deck(&mut rng)], Player(0)).unwrap());
    p.apply(Player(0), Action::Mulligan { take: true }).unwrap();
    p.apply(Player(1), Action::Mulligan { take: false })
        .unwrap();
    resolve_chance(&mut p, &mut rng).unwrap();
    let a = p.observe(Player(0)).actions().unwrap().get(0);
    assert!(matches!(a, Action::Bottom { .. }));
    p.apply(Player(0), a).unwrap();
    let own = p.observe(Player(0)).agent_view();
    assert_eq!(
        own["players"][0]["library"]["known_positions"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    assert!(
        p.observe(Player(1)).agent_view()["players"][0]["library"]["known_positions"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    p.apply(Player(0), Action::Mulligan { take: true }).unwrap();
    resolve_chance(&mut p, &mut rng).unwrap();
    assert!(
        p.observe(Player(0)).agent_view()["players"][0]["library"]["known_positions"]
            .as_array()
            .unwrap()
            .is_empty()
    );
}

#[test]
fn stack_shows_targets_resolution_order_and_current_prowess_stats() {
    let mut e = fixture();
    let swift = e.enter(SWIFT, Player(0), false).unwrap();
    let mut p = Position::new(e);
    let a = Action::Cast {
        card: BOLT,
        targets: [Subject::Player { player: Player(1) }, Subject::Absent],
        mode: 0,
        spectacle: false,
    };
    p.apply(Player(0), a).unwrap();
    let pending = p.observe(Player(0)).agent_view();
    assert_eq!(
        row(&pending["players"][0]["library"]["remaining"], BOLT)["count"],
        1
    );
    p.apply(
        Player(0),
        Action::Pay {
            mana: [0, 0, 0, 1, 0, 0],
        },
    )
    .unwrap();
    let v = p.observe(Player(0)).agent_view();
    assert_eq!(v["public"]["stack"][0]["ability"], "prowess");
    assert_eq!(v["public"]["stack"][1]["card"], "Lightning Bolt");
    assert_eq!(v["public"]["stack"][1]["targets"][0]["player"], 1);
    pass(&mut p);
    pass(&mut p);
    let v = p.observe(Player(0)).agent_view();
    let per = v["public"]["battlefield"]
        .as_array()
        .unwrap()
        .iter()
        .find(|b| b["object_id"] == swift.0)
        .unwrap();
    assert_eq!(per["power"], 2);
    assert_eq!(per["toughness"], 3);
    assert_eq!(per["summoning_sick"], false);
    assert!(!v.to_string().contains("Pool"));
    assert!(!v.to_string().contains("Card("));
}

#[test]
fn suspended_card_and_pending_land_are_counted_outside_library() {
    let mut e = fixture();
    e.world_mut().zones.hand[0][RIFT.0 as usize] = 1;
    e.world_mut().zones.hand[0][FOUNDRY.0 as usize] = 1;
    let mut p = Position::new(e);
    p.apply(Player(0), Action::Suspend).unwrap();
    p.apply(
        Player(0),
        Action::Pay {
            mana: [0, 0, 0, 1, 0, 0],
        },
    )
    .unwrap();
    let v = p.observe(Player(0)).agent_view();
    assert_eq!(v["public"]["suspended"][0]["card"], "Rift Bolt");
    assert_eq!(v["public"]["suspended"][0]["time_counters"], 1);
    assert_eq!(
        row(&v["players"][0]["library"]["remaining"], RIFT)["count"],
        0
    );
    p.apply(Player(0), Action::Land { card: FOUNDRY }).unwrap();
    let v = p.observe(Player(0)).agent_view();
    assert_eq!(
        v["public"]["execution"]["entering"][0]["card"],
        "Sacred Foundry"
    );
    assert_eq!(
        row(&v["players"][0]["library"]["remaining"], FOUNDRY)["count"],
        0
    );
    p.apply(Player(0), Action::Shock { pay: false }).unwrap();
    assert_eq!(
        p.observe(Player(0)).agent_view()["public"]["this_turn"]["land_entered"][0],
        true
    );
}

#[test]
fn full_game_views_bound_actual_library_counts_at_every_boundary() {
    for seed in 1..=12 {
        let mut rng = Random(seed);
        let mut p =
            Position::new(Engine::new([deck(&mut rng), deck(&mut rng)], Player(0)).unwrap());
        for _ in 0..10000 {
            if p.engine.finished().is_some() {
                break;
            }
            for viewer in [Player(0), Player(1)] {
                let v = p.observe(viewer).agent_view();
                let z = zones(&p.engine).unwrap();
                for owner in 0..2 {
                    let mut actual = [0u32; 16];
                    for c in entries(&z.library[owner]) {
                        actual[c.0 as usize] += 1;
                    }
                    for c in 0..16 {
                        let r = row(&v["players"][owner]["library"]["remaining"], Card(c as u16));
                        let min = r["count"].as_u64().or(r["min"].as_u64()).unwrap();
                        let max = r["count"].as_u64().or(r["max"].as_u64()).unwrap();
                        assert!(
                            min <= actual[c] as u64 && actual[c] as u64 <= max,
                            "seed {seed}, owner {owner}, card {c}: {r}, actual {}",
                            actual[c]
                        );
                        if owner == viewer.0 as usize {
                            assert_eq!(min, max);
                        }
                    }
                }
            }
            let (actor, action) = match auto_step(&p.engine).unwrap() {
                AutoStep::Finished(_) => break,
                AutoStep::Chance => (Player(0), chance_action(&p.engine, &mut rng).unwrap()),
                AutoStep::Automatic { actor, action, .. } => (actor, action),
                AutoStep::Decision { actor, actions } => {
                    (actor, actions.get(rng.below(actions.len())))
                }
            };
            p.apply(actor, action).unwrap();
        }
        assert!(p.engine.finished().is_some());
    }
}
