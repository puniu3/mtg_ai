use super::*;
use std::sync::atomic::{AtomicU64, Ordering};
static SERIAL: AtomicU64 = AtomicU64::new(0);
fn fresh(starting: u8) -> Game {
    let path = env::temp_dir().join(format!(
        "burn-play-{}-{}.jsonl",
        std::process::id(),
        SERIAL.fetch_add(1, Ordering::Relaxed)
    ));
    let _ = fs::remove_file(&path);
    Game::open(path, 41, starting, 2).unwrap()
}
fn human_boundary(g: &mut Game) {
    for _ in 0..20000 {
        g.tick().unwrap();
        if g.outcome.is_some()
            || matches!(
                g.controls.step(&g.position.engine),
                Ok(AutoStep::Decision {
                    actor: Player(0),
                    ..
                })
            )
        {
            return;
        }
    }
    panic!("no human boundary");
}
#[test]
fn durable_actions_deduplicate_and_reject_stale() {
    let mut g = fresh(0);
    human_boundary(&mut g);
    let r = json!({"id":"one","op":"act","sequence":g.sequence,"selection":{"index":"0"}});
    g.command(&r).unwrap();
    human_boundary(&mut g);
    let seq = g.sequence;
    g.command(&r).unwrap();
    assert_eq!(seq, g.sequence);
    let stale = json!({"id":"two","op":"act","sequence":0,"selection":{"index":"0"}});
    assert!(g.command(&stale).is_err());
    assert_eq!(seq, g.sequence);
    let mut loaded = Game::open(g.path.clone(), 99, 1, 999).unwrap();
    assert_eq!(g.state()["observation"], loaded.state()["observation"]);
    assert_eq!(g.log, loaded.log);
    loaded.command(&r).unwrap();
    assert_eq!(seq, loaded.sequence);
    fs::remove_file(&g.path).unwrap();
}
#[test]
fn both_starting_seats_complete_and_replay() {
    for starting in [0, 1] {
        let mut g = fresh(starting);
        for _ in 0..1500 {
            human_boundary(&mut g);
            if g.outcome.is_some() {
                break;
            }
            let obs = g.position.observe(Player(0));
            let report = burn_engine::terminal_bot::macro_search::search(
                &obs,
                Config {
                    budget: Budget::Trials(4),
                    seed: g.sequence + 321,
                    ..Default::default()
                },
            );
            let a = report.selected.expect("human simulation choice");
            let space = g.controls.actions(&g.position.engine).unwrap();
            let index = report.details["candidates"]
                .as_array()
                .unwrap()
                .iter()
                .find(|r| r["action"] == action_text(a))
                .unwrap()["index"]
                .as_str()
                .unwrap()
                .parse::<u128>()
                .unwrap();
            g.commit(Player(0), space.get(index), Some(index), "human", None)
                .unwrap();
        }
        assert!(g.outcome.is_some(), "complete game");
        let loaded = Game::open(g.path.clone(), 0, 0, 0).unwrap();
        assert_eq!(loaded.outcome, g.outcome);
        assert_eq!(loaded.last_view, g.last_view);
        assert_eq!(loaded.log, g.log);
        fs::remove_file(&g.path).unwrap();
    }
}
#[test]
fn concede_and_torn_tail_recover() {
    let mut g = fresh(0);
    human_boundary(&mut g);
    let r = json!({"id":"concede","op":"concede","sequence":g.sequence});
    g.command(&r).unwrap();
    OpenOptions::new()
        .append(true)
        .open(&g.path)
        .unwrap()
        .write_all(b"{\"partial\":")
        .unwrap();
    let mut loaded = Game::open(g.path.clone(), 0, 0, 0).unwrap();
    assert_eq!(g.outcome, loaded.outcome);
    loaded.command(&r).unwrap();
    assert_eq!(g.sequence, loaded.sequence);
    fs::remove_file(&g.path).unwrap();
}
#[test]
fn ponder_cache_is_used_only_at_identical_root() {
    let mut g = fresh(1);
    // Stop at the bot's opening-hand decision, without searching it.
    loop {
        match g.controls.step(&g.position.engine).unwrap() {
            AutoStep::Chance => {
                let a = chance_action(&g.position.engine, &mut g.chance).unwrap();
                g.commit(Player(0), a, None, "chance", None).unwrap();
            }
            AutoStep::Automatic { actor, action, .. } => {
                g.commit(actor, action, None, "auto", None).unwrap()
            }
            AutoStep::Decision {
                actor: Player(1), ..
            } => break,
            _ => panic!("unexpected initial boundary"),
        }
    }
    g.budget = 1.;
    let obs = g.position.observe(Player(1));
    let mut cache = SearchSession::new(&obs, Config::default()).unwrap();
    cache.run(Budget::Trials(8));
    let elapsed = cache.elapsed();
    g.candidates.push(cache);
    g.tick().unwrap();
    assert_eq!(g.reused, elapsed);
    let running_elapsed = g.active.as_ref().unwrap().elapsed();
    g.command(&json!({"id":"reconnect","op":"resume"})).unwrap();
    assert_eq!(g.active.as_ref().unwrap().elapsed(), running_elapsed);
    g.active = None;
    g.candidates.clear();
    let wrong = g
        .position
        .observe(Player(1))
        .sample(&mut Random(99))
        .unwrap();
    let mut candidate = wrong;
    let actions = candidate.observe(Player(1)).auto_actions().unwrap();
    candidate.apply(Player(1), actions.get(0)).unwrap();
    // A different root cannot be matched; preserve the same actual game.
    if let Ok(mut s) = SearchSession::new(&candidate.observe(Player(1)), Config::default()) {
        s.run(Budget::Trials(1));
        g.candidates.push(s);
    }
    g.tick().unwrap();
    assert_eq!(g.reused, 0.);
    fs::remove_file(&g.path).unwrap();
}

#[test]
fn confirmed_concession_survives_intervening_bot_action() {
    let mut g = fresh(1);
    let seen = g.sequence;
    human_boundary(&mut g);
    assert!(g.sequence > seen);
    let request = json!({"id":"confirmed-concession","op":"concede","sequence":seen});
    g.command(&request).unwrap();
    assert_eq!(g.outcome.as_ref().unwrap()["reason"], "concede");
    let mut loaded = Game::open(g.path.clone(), 0, 0, 0).unwrap();
    loaded.command(&request).unwrap();
    assert_eq!(loaded.sequence, g.sequence);
    fs::remove_file(&g.path).unwrap();
}

#[test]
fn pending_fetch_survives_restart_and_bot_intent_stays_private() {
    let mut g = fresh(0);
    let mut plans = 0;
    let mut bot_plans = 0;
    for _ in 0..1800 {
        let (actor, action, index, kind) = match g.controls.step(&g.position.engine).unwrap() {
            AutoStep::Finished(_) => break,
            AutoStep::Chance => (
                Player(0),
                chance_action(&g.position.engine, &mut g.chance).unwrap(),
                None,
                "chance",
            ),
            AutoStep::Automatic { actor, action, .. } => (actor, action, None, "auto"),
            AutoStep::Decision { actor, actions } => {
                let selected = (0..actions.len()).find(|&i| {
                    matches!(
                        actions.get(i),
                        Action::FetchPlan {
                            card: Some(FOUNDRY),
                            pay: true,
                            ..
                        }
                    )
                });
                // Exercise persistence deliberately; search preferences may change.
                let selected = selected
                    .or_else(|| (0..actions.len()).find(|&i| matches!(actions.get(i), Action::Land { card: MESA | MIRE })))
                    .or_else(|| (0..actions.len()).find(|&i| actions.get(i) == Action::Mulligan { take: false }));
                let i = selected.unwrap_or_else(|| {
                    let r = burn_engine::terminal_bot::macro_search::search(
                        &g.position.observe(actor),
                        Config {
                            budget: Budget::Trials(4),
                            seed: g.sequence + 91,
                            plan: g.controls.plan(actor),
                            ..Default::default()
                        },
                    );
                    let a = r.selected.unwrap();
                    (0..actions.len()).find(|&i| actions.get(i) == a).unwrap()
                });
                (
                    actor,
                    actions.get(i),
                    Some(i),
                    if actor == Player(0) { "human" } else { "bot" },
                )
            }
        };
        g.commit(actor, action, index, kind, None).unwrap();
        if matches!(action, Action::FetchPlan { .. }) {
            plans += 1;
            assert!(g.controls.plan(actor).is_some());
            let loaded = Game::open(g.path.clone(), 0, 0, 0).unwrap();
            assert_eq!(loaded.controls.plan(actor), g.controls.plan(actor));
            assert_eq!(loaded.state(), g.state());
            if actor == Player(1) {
                bot_plans += 1;
                let a = g.log.iter().rev().find_map(|r| r.get("action")).unwrap();
                assert_eq!(a["kind"], "fetch");
                assert!(a.get("pay_2_life").is_none());
                assert!(a.get("card").is_none());
                assert!(g.state()["human_plan"].is_null());
            }
        }
        if g.outcome.is_some() {
            break;
        }
    }
    assert!(
        plans >= 2 && bot_plans > 0,
        "must exercise live pending plans on both sides"
    );
    fs::remove_file(&g.path).unwrap();
}

#[test]
fn fixed_compute_reuses_ponder_within_total_budget_and_persists() {
    for prefetched in [0, 40_000, 80_000] {
        let mut g = fresh(1);
        fs::remove_file(&g.path).unwrap();
        g = Game::open_budget(g.path.clone(), 41, 1, 0, Some(80_000)).unwrap();
        loop {
            match g.controls.step(&g.position.engine).unwrap() {
                AutoStep::Chance => {
                    let a = chance_action(&g.position.engine, &mut g.chance).unwrap();
                    g.commit(Player(0), a, None, "chance", None).unwrap();
                }
                AutoStep::Automatic { actor, action, .. } => {
                    g.commit(actor, action, None, "auto", None).unwrap()
                }
                AutoStep::Decision {
                    actor: Player(1), ..
                } => break,
                _ => panic!("expected bot opening decision"),
            }
        }
        let obs = g.position.observe(Player(1));
        let config = Config {
            seed: 71,
            ..Default::default()
        };
        let mut reference = SearchSession::new(&obs, config).unwrap();
        reference.memory_limit = 7 * 1024 * 1024;
        let expected = reference.run(Budget::Steps(80_000));
        let mut cache = SearchSession::new(&obs, config).unwrap();
        cache.memory_limit = reference.memory_limit;
        if prefetched > 0 {
            cache.run(Budget::Steps(prefetched));
        }
        let reused = cache.steps();
        g.candidates.push(cache);
        // A zero wall-time limit must not stop a fixed-compute search.
        g.budget = 0.;
        let seq = g.sequence;
        for _ in 0..10 {
            g.tick().unwrap();
            if g.sequence != seq {
                break;
            }
        }
        assert!(g.sequence > seq);
        assert_eq!(g.last_search["steps"], expected.steps);
        assert_eq!(g.last_search["trials"], expected.completed);
        assert_eq!(g.last_search["reused_steps"], reused);
        assert_eq!(g.last_search["incomplete"], 0);
        let rows = fs::read_to_string(&g.path).unwrap();
        let last: V = serde_json::from_str(rows.lines().last().unwrap()).unwrap();
        assert_eq!(last["action"], action_text(expected.selected.unwrap()));
        let loaded = Game::open(g.path.clone(), 0, 0, 10_000).unwrap();
        assert_eq!(loaded.budget_steps, Some(80_000));
        for key in [
            "steps",
            "trials",
            "reused_steps",
            "budget_steps",
            "incomplete",
        ] {
            assert_eq!(loaded.last_search[key], g.last_search[key]);
        }
        assert!(
            (loaded.last_search["seconds"].as_f64().unwrap()
                - g.last_search["seconds"].as_f64().unwrap())
            .abs()
                < 1e-9
        );
        assert_eq!(loaded.last_view, g.last_view);
        assert!(loaded.state()["budget_seconds"].is_null());
        fs::remove_file(&g.path).unwrap();
    }
}
