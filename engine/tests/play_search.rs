use burn_engine::{harness::*, terminal_bot::*, *};
fn initial() -> Observation {
    let mut rng = Random(1);
    let p = Position::new(Engine::new([deck(&mut rng), deck(&mut rng)], Player(0)).unwrap());
    p.observe(Player(0))
}
#[test]
fn split_trials_equal_one_shot() {
    let obs = initial();
    let config = Config {
        budget: Budget::Trials(128),
        seed: 51,
        ..Default::default()
    };
    let whole = search(&obs, config);
    let mut s = SearchSession::new(&obs, config).unwrap();
    s.run(Budget::Trials(31));
    s.run(Budget::Trials(17));
    let split = s.run(Budget::Trials(80));
    assert_eq!(whole.selected, split.selected);
    assert_eq!(whole.completed, split.completed);
    assert_eq!(whole.steps, split.steps);
    assert_eq!(whole.nodes, split.nodes);
    assert_eq!(whole.details["candidates"], split.details["candidates"]);
    assert_eq!(whole.details["root_priors"], split.details["root_priors"]);
}
#[test]
fn bounded_search_retains_valid_choice() {
    let obs = initial();
    let mut s = SearchSession::new(&obs, Config::default()).unwrap();
    s.memory_limit = 7 * 1024 * 1024;
    let r = s.run(Budget::Trials(1024));
    assert!(r.error.is_none(), "{:?}", r.error);
    assert!(r.selected.is_some());
    assert!(s.memory_bytes() < 8 * 1024 * 1024);
}
#[test]
fn forecast_uses_only_bot_observation() {
    let obs = initial();
    let p = obs.sample(&mut Random(1)).unwrap();
    let bot = p.observe(Player(1));
    let a = bot.sample(&mut Random(8)).unwrap();
    let b = bot.sample(&mut Random(9)).unwrap();
    assert_eq!(a.observe(Player(1)), b.observe(Player(1)));
    assert_eq!(
        ponder_candidate(&a.observe(Player(1)), 12),
        ponder_candidate(&b.observe(Player(1)), 12)
    );
}
#[test]
fn unfinished_slices_receive_no_reward() {
    let obs = initial();
    let mut s = SearchSession::new(&obs, Config::default()).unwrap();
    s.interruptible = true;
    let r = s.run(Budget::Time(std::time::Duration::from_nanos(1)));
    assert_eq!(r.completed, 0);
    assert!(r.selected.is_none());
    assert!(r.error.is_none());
    let r = s.run(Budget::Trials(4));
    assert_eq!(r.completed, 4);
    assert!(r.selected.is_some());
    let visits: u64 = r.details["candidates"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c["visits"].as_u64().unwrap())
        .sum();
    assert_eq!(visits, 4);
}

#[test]
fn macro_step_budget_is_independent_of_slices_and_elapsed_time() {
    use burn_engine::terminal_bot::macro_search as m;
    let obs = initial();
    let mut whole = m::SearchSession::new(&obs, m::Config::default()).unwrap();
    whole.memory_limit = 7 * 1024 * 1024;
    let expected = whole.run(m::Budget::Steps(80_000));
    assert!(expected.error.is_none());
    assert!(
        expected.steps >= 80_000 && expected.steps < 80_000 + m::Config::default().max_trial_steps
    );
    for chunk in [7_000, 32_000] {
        let mut split = m::SearchSession::new(&obs, m::Config::default()).unwrap();
        split.memory_limit = whole.memory_limit;
        while split.steps() < 80_000 {
            split.run(m::Budget::Steps((80_000 - split.steps()).min(chunk)));
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
        let actual = split.run(m::Budget::Steps(0));
        assert!(actual.error.is_none());
        assert_eq!(actual.incomplete, 0);
        assert_eq!(actual.steps, expected.steps);
        assert_eq!(actual.completed, expected.completed);
        assert_eq!(actual.selected, expected.selected);
        assert_eq!(actual.details["candidates"], expected.details["candidates"]);
    }
}
