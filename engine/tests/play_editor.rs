use burn_engine::{harness::*, *};
use mtg_state_design::burn::{Combat, Execution, Phase, Running, State, World};
use serde_json::{Value, json};
fn base() -> Engine {
    let mut w = World::default();
    w.identity.next = 1;
    for p in 0..2 {
        w.players[p].life = 20;
        for _ in 0..8 {
            w.zones.library[p].push(MOUNTAIN).unwrap();
        }
    }
    Engine::fixture(w, Player(0), 3)
}
fn input(action: Action, editor: &Value, index: u128) -> Value {
    match action {
        Action::Attack { objects, len } => {
            json!({"attackers":objects[..len as usize].iter().map(|x|x.0).collect::<Vec<_>>()})
        }
        Action::Block { pairs, len } => {
            json!({"blocks":pairs[..len as usize].iter().map(|(b,a)|[b.0,a.0]).collect::<Vec<_>>()})
        }
        Action::Damage { assignments, len } => {
            let groups = editor["groups"].as_array().unwrap();
            json!({"damage":groups.iter().map(|g|g["targets"].as_array().unwrap().iter().map(|t|assignments[..len as usize].iter().find(|(s,target,_)|Some(s.0 as u64)==g["source"].as_u64()&&matches!(target,Subject::Object{id}if Some(id.0 as u64)==t.as_u64())).unwrap().2).collect::<Vec<_>>()).collect::<Vec<_>>()})
        }
        _ => json!({"index":index.to_string()}),
    }
}
fn save_fixture(o: &Observation, space: &ActionSpace, name: &str) {
    if let Ok(path) = std::env::var("BURN_PLAY_FIXTURE_OUT") {
        let path = std::path::Path::new(&path).with_file_name(format!("{name}-fixture.json"));
        std::fs::write(path,json!({"sequence":10,"status":"human","starting":0,"observation":o.agent_view(),"editor":space.editor(o),"outcome":null,"log":[],"ponder_enabled":true,"pondering":false,"budget_seconds":10,"thinking_seconds":0,"reused_seconds":0}).to_string()).unwrap();
    }
}
fn verify(space: &ActionSpace, o: &Observation, indices: &[u128]) {
    let editor = space.editor(o);
    for &index in indices {
        let a = space.get(index);
        assert_eq!(
            space.editor_index(&input(a, &editor, index)).unwrap(),
            index
        );
    }
}
#[test]
fn combat_editor_roundtrips_large_spaces_and_rejects_duplicates() {
    let mut e = base();
    let mut combat = Combat::default();
    for _ in 0..8 {
        let id = e.enter(GUIDE, Player(1), true).unwrap();
        combat
            .attackers
            .push(Attacker {
                attackers: IdRange {
                    first: id,
                    count: 1,
                },
                defender: Subject::Player { player: Player(0) },
                was_blocked: false,
            })
            .unwrap();
        e.enter(EIDOLON, Player(0), false).unwrap();
    }
    if let State::Playing {
        progress,
        execution,
        ..
    } = &mut e.state
    {
        progress.phase = Phase::Combat { combat };
        let mut r = Running::default();
        r.choice = Choice::Select {
            player: Player(0),
            rule: Behavior(301),
            min: 0,
            max: 24,
            selected: Span::default(),
        };
        *execution = Execution::Running { running: r };
    }
    let o = Position::new(e).observe(Player(0));
    let space = o.auto_actions().unwrap();
    assert_eq!(space.len(), 9u128.pow(8));
    verify(&space, &o, &[0, 4096, space.len() / 2, space.len() - 1]);
    let editor = space.editor(&o);
    let b = editor["blockers"][0].clone();
    let a = editor["attackers"][0].clone();
    assert!(
        space
            .editor_index(&json!({"blocks":[[b,a],[b,a]]}))
            .is_err()
    );
    assert!(space.editor_index(&json!({"blocks":[[999999,1]]})).is_err());
    if let Ok(path) = std::env::var("BURN_PLAY_FIXTURE_OUT") {
        std::fs::write(path,json!({"sequence":10,"status":"human","starting":0,"observation":o.agent_view(),"editor":editor,"outcome":null,"log":[],"ponder_enabled":true,"pondering":false,"budget_seconds":10,"thinking_seconds":0,"reused_seconds":0}).to_string()).unwrap();
    }
}
#[test]
fn attack_and_damage_editors_match_engine() {
    let mut e = base();
    e.enter(GUIDE, Player(0), false).unwrap();
    e.enter(EIDOLON, Player(1), false).unwrap();
    e.enter(EIDOLON, Player(1), false).unwrap();
    let mut p = Position::new(e);
    let mut saw_damage = false;
    for _ in 0..50 {
        match auto_step(&p.engine).unwrap() {
            AutoStep::Automatic { actor, action, .. } => {
                p.apply(actor, action).unwrap();
            }
            AutoStep::Decision { actor, actions } => {
                let o = p.observe(actor);
                let editor = actions.editor(&o);
                let k = editor["kind"].as_str().unwrap();
                if ["attack", "block", "damage"].contains(&k) {
                    verify(&actions, &o, &(0..actions.len()).collect::<Vec<_>>());
                }
                if actor == Player(0) && ["attack", "damage"].contains(&k) {
                    save_fixture(&o, &actions, k);
                }
                if k == "attack" {
                    let id = editor["attackers"][0].clone();
                    assert!(actions.editor_index(&json!({"attackers":[id,id]})).is_err());
                }
                if k == "damage" {
                    assert!(actions.editor_index(&json!({"damage":[[99,0]]})).is_err());
                    saw_damage = true;
                    break;
                }
                let a = if k == "attack" || k == "block" {
                    actions.get(actions.len() - 1)
                } else {
                    (0..actions.len())
                        .map(|i| actions.get(i))
                        .find(|a| *a == Action::Pass)
                        .unwrap_or(actions.get(0))
                };
                p.apply(actor, a).unwrap();
            }
            _ => break,
        }
    }
    assert!(saw_damage);
}

#[test]
fn export_special_choice_fixtures() {
    // Reach genuine rule-engine choice boundaries; no handwritten action data.
    let mut e = base();
    e.world_mut().zones.hand[0][FOUNDRY.0 as usize] = 1;
    let mut p = Position::new(e);
    p.apply(Player(0), Action::Land { card: FOUNDRY }).unwrap();
    let o = p.observe(Player(0));
    let space = o.auto_actions().unwrap();
    save_fixture(&o, &space, "shock");
    let mut e = base();
    // Two actual targets are needed for a choice now that fail-to-find is pruned.
    e.world_mut().zones.library[0].push(FOUNDRY).unwrap();
    let mesa = e.enter(MESA, Player(0), false).unwrap();
    let mut p = Position::new(e);
    p.apply(Player(0), Action::Fetch { object: mesa }).unwrap();
    for _ in 0..100 {
        match auto_step(&p.engine).unwrap() {
            AutoStep::Automatic { actor, action, .. } => {
                p.apply(actor, action).unwrap();
            }
            AutoStep::Decision { actor, actions } => {
                if matches!(actions.get(0), Action::Search { .. }) {
                    let o = p.observe(actor);
                    save_fixture(&o, &actions, "fetch");
                    break;
                }
                p.apply(actor, Action::Pass).unwrap();
            }
            _ => panic!("fetch choice"),
        }
    }
    let mut rng = Random(2);
    let mut p = Position::new(Engine::new([deck(&mut rng), deck(&mut rng)], Player(0)).unwrap());
    let mut mull = false;
    for _ in 0..200 {
        match auto_step(&p.engine).unwrap() {
            AutoStep::Automatic { actor, action, .. } => {
                p.apply(actor, action).unwrap();
            }
            AutoStep::Chance => {
                resolve_chance(&mut p, &mut rng).unwrap();
            }
            AutoStep::Decision { actor, actions } => {
                if actor == Player(0) && matches!(actions.get(0), Action::Bottom { .. }) {
                    let o = p.observe(actor);
                    save_fixture(&o, &actions, "bottom");
                    break;
                }
                let a = if actor == Player(0) && !mull {
                    mull = true;
                    Action::Mulligan { take: true }
                } else {
                    actions.get(0)
                };
                p.apply(actor, a).unwrap();
            }
            _ => panic!("bottom choice"),
        }
    }
    let mut e = base();
    e.world_mut().zones.hand[0][RIFT.0 as usize] = 1;
    e.enter(MOUNTAIN, Player(0), false).unwrap();
    let mut p = Position::new(e);
    p.apply(Player(0), Action::Suspend).unwrap();
    let mut found = false;
    for _ in 0..500 {
        match auto_step(&p.engine).unwrap() {
            AutoStep::Automatic { actor, action, .. } => {
                p.apply(actor, action).unwrap();
            }
            AutoStep::Chance => {
                resolve_chance(&mut p, &mut rng).unwrap();
            }
            AutoStep::Decision { actor, actions } => {
                if actor == Player(0)
                    && (0..actions.len())
                        .any(|i| matches!(actions.get(i), Action::SuspendTarget { .. }))
                {
                    save_fixture(&p.observe(actor), &actions, "suspend");
                    found = true;
                    break;
                }
                let a = (0..actions.len())
                    .map(|i| actions.get(i))
                    .find(|a| *a == Action::Pass)
                    .unwrap_or(actions.get(0));
                p.apply(actor, a).unwrap();
            }
            _ => break,
        }
    }
    assert!(found);
}

#[test]
fn export_compound_fetch_and_reselection_fixtures() {
    use burn_engine::terminal_bot::macro_control::{Controls, Mode};
    let mut e = base();
    e.world_mut().zones.library[0].push(FOUNDRY).unwrap();
    e.world_mut().players[1].mana[3] = 4;
    e.world_mut().zones.hand[1][BOLT.0 as usize] = 1;
    let mesa = e.enter(MESA, Player(0), false).unwrap();
    let mut p = Position::new(e);
    let mut c = Controls::new(Player(0), Mode::named("both"), None);
    let actions = c.actions(&p.engine).unwrap();
    save_fixture(&p.observe(Player(0)), &actions, "fetch-plan");
    verify(
        &actions,
        &p.observe(Player(0)),
        &(0..actions.len()).collect::<Vec<_>>(),
    );
    c.apply(&mut p, Player(0), Action::Fetch { object: mesa })
        .unwrap();
    for _ in 0..30 {
        match c.step(&p.engine).unwrap() {
            AutoStep::Automatic { actor, action, .. } => {
                c.apply(&mut p, actor, action).unwrap();
            }
            AutoStep::Decision { actor, actions } => {
                if matches!(actions.get(0), Action::SearchPlan { .. }) {
                    save_fixture(&p.observe(actor), &actions, "search-plan");
                    return;
                }
                c.apply(&mut p, actor, Action::Pass).unwrap();
            }
            other => panic!("{other:?}"),
        }
    }
    panic!("missing compound search");
}

#[test]
fn friendly_damage_removed_from_shared_editor_but_double_strike_remains() {
    use burn_engine::terminal_bot::macro_control::{Controls, Mode};
    for actor in [Player(0), Player(1)] {
        let mut e = Engine::fixture(*base().world(), actor, 3);
        e.world_mut().players[actor.0 as usize].mana = [4, 0, 0, 4, 0, 0];
        for c in [BOLT, HELIX, BLAZE, CHARM] {
            e.world_mut().zones.hand[actor.0 as usize][c.0 as usize] = 1;
        }
        let own = e.enter(GUIDE, actor, false).unwrap();
        let enemy = e.enter(EIDOLON, Player(1-actor.0), false).unwrap();
        let p = Position::new(e);
        let c = Controls::new(actor, Mode::named("both"), None);
        let actions = c.actions(&p.engine).unwrap();
        let o = p.observe(actor);
        let editor = actions.editor(&o);
        for card in [BOLT, HELIX, BLAZE] {
            let rows: Vec<_> = editor["choices"].as_array().unwrap().iter().filter(|r|r["action"]["card"] == o.action_view(Action::Cast {card, targets:[Subject::Absent;2], mode:0,spectacle:false})["card"]).collect();
            assert!(!rows.is_empty());
            for row in rows {
                for target in row["action"]["targets"].as_array().unwrap() {
                    assert_ne!(target["player"].as_u64(), Some(actor.0 as u64));
                    assert_ne!(target["object_id"].as_u64(), Some(own.0 as u64));
                    assert!(target["player"] == 1-actor.0 || target["object_id"] == enemy.0);
                }
            }
        }
        assert!((0..actions.len()).any(|i|matches!(actions.get(i),Action::Cast{card:CHARM,mode:2,targets:[Subject::Object{id},_],..} if id==own)));
        if actor == Player(0) { save_fixture(&o, &actions, "friendly-targets"); }
    }
}
