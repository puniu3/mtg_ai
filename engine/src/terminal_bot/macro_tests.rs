use super::super::macro_search;
use super::*;
use mtg_state_design::burn::World;
fn fixture() -> Engine {
    let mut w = World::default();
    w.identity.next = 1;
    for p in 0..2 {
        w.players[p].life = 20;
        w.players[p].mana[RED] = 4;
        w.players[p].mana[WHITE] = 4;
        w.zones.hand[p][BOLT.0 as usize] = 2;
        for c in [MOUNTAIN, FOUNDRY, BOLT, HELIX, MOUNTAIN] {
            w.zones.library[p].push(c).unwrap();
        }
    }
    let mut e = Engine::fixture(w, Player(0), 3);
    e.enter(MESA, Player(0), false).unwrap();
    e
}
fn source(e: &Engine) -> ObjectId {
    entries(&e.world().battlefield)
        .iter()
        .find(|p| card_of(p) == MESA)
        .unwrap()
        .ids
        .first
}
fn fetch(e: &Engine) -> Action {
    Action::Fetch { object: source(e) }
}
#[test]
fn own_top_passes_but_opponent_response_is_preserved() {
    let e = fixture();
    let a = fetch(&e);
    let mut p = Position::new(e);
    let mut c = Control::new(Player(0), Mode::named("both"), None);
    c.apply(&mut p, Player(0), a).unwrap();
    assert!(matches!(
        c.step(&p.engine).unwrap(),
        AutoStep::Automatic {
            action: Action::Pass,
            reason: "own top stack object",
            ..
        }
    ));
    c.apply(&mut p, Player(0), Action::Pass).unwrap();
    assert!(matches!(
        c.step(&p.engine).unwrap(),
        AutoStep::Decision {
            actor: Player(1),
            ..
        }
    ));
}
#[test]
fn auto_fetch_plans_include_only_mountain_tapped_and_untapped_foundry() {
    let e = fixture();
    let a = ActionSpace::build_auto(&e).unwrap();
    let expanded = expand(&e, Player(0), a).unwrap();
    let plans: Vec<_> = (0..expanded.len())
        .map(|i| expanded.get(i))
        .filter(|a| matches!(a, Action::FetchPlan { .. }))
        .collect();
    for (card, pay) in [
        (Some(MOUNTAIN), false),
        (Some(FOUNDRY), false),
        (Some(FOUNDRY), true),
    ] {
        assert!(plans.contains(&Action::FetchPlan {
            object: source(&e),
            card,
            pay
        }));
    }
    assert_eq!(plans.len(), 3);
    assert!(!plans.iter().any(|a| matches!(a, Action::FetchPlan { card: None, .. })));
}
#[test]
fn plan_matches_primitive_rules_and_public_observation_at_every_step() {
    for (card, pay) in [
        (None, false),
        (Some(MOUNTAIN), false),
        (Some(FOUNDRY), false),
        (Some(FOUNDRY), true),
    ] {
        let e = fixture();
        let mut a = Position::new(e);
        let mut b = a.clone();
        let mut c = Control::new(Player(0), Mode::named("both"), None);
        let plan = Action::FetchPlan {
            object: source(&e),
            card,
            pay,
        };
        c.apply(&mut a, Player(0), plan).unwrap();
        b.apply(Player(0), primitive(plan)).unwrap();
        let mut rng = Random(42);
        let mut planned_steps = 0;
        for _ in 0..32 {
            assert_eq!(a.engine, b.engine);
            for p in [Player(0), Player(1)] {
                assert_eq!(a.observe(p), b.observe(p));
            }
            if a.engine.world().stack.len == 0 {
                break;
            }
            let (actor, action) = match c.step(&a.engine).unwrap() {
                AutoStep::Automatic {
                    actor,
                    action,
                    reason,
                } => {
                    planned_steps += usize::from(reason == "committed fetch plan");
                    (actor, action)
                }
                AutoStep::Decision { actor, .. } => {
                    assert_ne!(actor, Player(0), "plan must cover search/shock");
                    (actor, Action::Pass)
                }
                AutoStep::Chance => (Player(0), chance_action(&a.engine, &mut rng).unwrap()),
                x => panic!("{x:?}"),
            };
            c.apply(&mut a, actor, action).unwrap();
            b.apply(actor, primitive(action)).unwrap();
        }
        assert_eq!(planned_steps, if card == Some(FOUNDRY) { 2 } else { 1 });
        assert!(c.plan.is_none());
        assert_eq!(a.engine.world().stack.len, 0);
        a.audit().unwrap();
        assert_eq!(a.engine.world().players[0].life, if pay { 17 } else { 19 });
    }
}
#[test]
fn opponent_spell_invalidates_the_entire_fetch_intent() {
    let e = fixture();
    let mut p = Position::new(e);
    let mut c = Control::new(Player(0), Mode::named("both"), None);
    c.apply(
        &mut p,
        Player(0),
        Action::FetchPlan {
            object: source(&e),
            card: Some(FOUNDRY),
            pay: true,
        },
    )
    .unwrap();
    c.apply(&mut p, Player(0), Action::Pass).unwrap();
    let bolt = Action::Cast {
        card: BOLT,
        targets: [Subject::Player { player: Player(0) }, Subject::Absent],
        mode: 0,
        spectacle: false,
    };
    c.apply(&mut p, Player(1), bolt).unwrap();
    assert!(c.plan.is_none());
    assert_eq!(c.cancelled, 1);
}
#[test]
fn primitive_prior_mass_is_not_multiplied_by_fetch_variants() {
    let e = fixture();
    let base = ActionSpace::build_auto(&e).unwrap();
    let bc = SearchChoices::new(&e, Player(0), &base).unwrap();
    let bp = super::super::rollout_policy::prior(&e, Player(0), &base, &bc).unwrap();
    let expanded = expand(&e, Player(0), base.clone()).unwrap();
    let c = SearchChoices::new(&e, Player(0), &expanded).unwrap();
    let ep = prior(&e, Player(0), &expanded, &c).unwrap();
    for i in 0..bc.len() {
        let a = base.get(bc.index(i));
        let mass: f64 = (0..c.len())
            .filter(|&j| primitive(expanded.get(c.index(j))) == a)
            .map(|j| ep[j as usize])
            .sum();
        assert!(
            (mass - bp[i as usize]).abs() < 1e-12,
            "{a:?}: {mass} vs {}",
            bp[i as usize]
        );
    }
}
#[test]
fn low_life_cannot_schedule_a_lethal_shock() {
    let mut e = fixture();
    e.world_mut().players[0].life = 3;
    let a = expand(&e, Player(0), ActionSpace::build_auto(&e).unwrap()).unwrap();
    assert!(!(0..a.len()).any(|i| matches!(a.get(i), Action::FetchPlan { pay: true, .. })));
    e.world_mut().players[0].life = 1;
    let a = expand(&e, Player(0), ActionSpace::build_auto(&e).unwrap()).unwrap();
    assert!(!(0..a.len()).any(|i| matches!(a.get(i), Action::FetchPlan { .. })));
}
#[test]
fn disabled_abstraction_matches_baseline_search_exactly() {
    let o = Position::new(fixture()).observe(Player(0));
    let a = search(
        &o,
        Config {
            budget: Budget::Trials(256),
            seed: 77,
            ..Default::default()
        },
    );
    let b = macro_search::search(
        &o,
        macro_search::Config {
            control: Mode::default(),
            budget: macro_search::Budget::Trials(256),
            seed: 77,
            ..Default::default()
        },
    );
    assert!(a.error.is_none());
    assert!(b.error.is_none());
    assert_eq!(a.selected, b.selected);
    assert_eq!(a.steps, b.steps);
    assert_eq!(a.details["candidates"], b.details["candidates"]);
}
#[test]
fn choices_and_priors_do_not_depend_on_hidden_world() {
    let o = Position::new(fixture()).observe(Player(0));
    let a = o.sample(&mut Random(1)).unwrap();
    let b = o.sample(&mut Random(2)).unwrap();
    let aa = expand(
        &a.engine,
        Player(0),
        ActionSpace::build_auto(&a.engine).unwrap(),
    )
    .unwrap();
    let bb = expand(
        &b.engine,
        Player(0),
        ActionSpace::build_auto(&b.engine).unwrap(),
    )
    .unwrap();
    assert_eq!(aa.len(), bb.len());
    for i in 0..aa.len() {
        assert_eq!(aa.get(i), bb.get(i));
    }
    assert_eq!(
        prior(
            &a.engine,
            Player(0),
            &aa,
            &SearchChoices::new(&a.engine, Player(0), &aa).unwrap()
        ),
        prior(
            &b.engine,
            Player(0),
            &bb,
            &SearchChoices::new(&b.engine, Player(0), &bb).unwrap()
        )
    );
}

#[test]
fn shared_control_replans_after_response_and_automates_both_sides() {
    let e = fixture();
    let mut p = Position::new(e);
    let mut c = Controls::new(Player(0), Mode::named("both"), None);
    c.apply(
        &mut p,
        Player(0),
        Action::FetchPlan {
            object: source(&e),
            card: Some(FOUNDRY),
            pay: true,
        },
    )
    .unwrap();
    c.apply(&mut p, Player(0), Action::Pass).unwrap();
    c.apply(
        &mut p,
        Player(1),
        Action::Cast {
            card: BOLT,
            targets: [Subject::Player { player: Player(0) }, Subject::Absent],
            mode: 0,
            spectacle: false,
        },
    )
    .unwrap();
    assert!(c.plan(Player(0)).is_none());
    // Casting still performs primitive automatic mana payment before priority.
    loop {
        match c.step(&p.engine).unwrap() {
            AutoStep::Automatic {
                actor: Player(1),
                action: Action::Pass,
                reason: "own top stack object",
            } => break,
            AutoStep::Automatic { actor, action, .. } => {
                c.apply(&mut p, actor, action).unwrap();
            }
            other => panic!("expected automatic payment then own-top pass: {other:?}"),
        }
    }
    let mut replanned = false;
    for _ in 0..40 {
        let (actor, a) = match c.step(&p.engine).unwrap() {
            AutoStep::Automatic { actor, action, .. } => (actor, action),
            AutoStep::Decision { actor, actions } => {
                if let Some(a) = (0..actions.len()).map(|i| actions.get(i)).find(|a| {
                    matches!(
                        a,
                        Action::SearchPlan {
                            card: Some(FOUNDRY),
                            pay: false
                        }
                    )
                }) {
                    assert_eq!(actor, Player(0));
                    replanned = true;
                    (actor, a)
                } else {
                    (actor, Action::Pass)
                }
            }
            AutoStep::Chance => break,
            other => panic!("{other:?}"),
        };
        c.apply(&mut p, actor, a).unwrap();
    }
    assert!(replanned);
    assert!(c.plan(Player(0)).is_none());
    assert_eq!(p.engine.world().players[0].life, 16); // fetch + opponent bolt, no shock
    assert!(
        entries(&p.engine.world().battlefield)
            .iter()
            .any(|x| card_of(x) == FOUNDRY)
    );
}

#[test]
fn suspend_progress_is_automatic_but_target_and_spell_response_remain() {
    for owner in [Player(0), Player(1)] {
        for count in [1, 2] {
            let other = Player(1 - owner.0);
            let mut e = Engine::fixture(*fixture().world(), owner, 3);
            for actor in [owner, other] {
                e.enter(MOUNTAIN, actor, false).unwrap();
            }
            let own_creature = e.enter(GUIDE, owner, false).unwrap();
            let enemy_creature = e.enter(GUIDE, other, false).unwrap();
            e.world_mut().zones.hand[pid(owner)][RIFT.0 as usize] = 1;
            let mut p = Position::new(e);
            let mut reference = p.clone();
            let mut control = Controls::new(owner, Mode::named("both"), None);
            control.apply(&mut p, owner, Action::Suspend).unwrap();
            reference.apply(owner, Action::Suspend).unwrap();
            let mut initialized = false;
            let mut time_passes = 0;
            let mut cast_trigger_passes = 0;
            let mut targets = 0;
            let mut responded = false;
            for _ in 0..500 {
                // Exercise both last-counter and non-last-counter progression.
                if !initialized && p.engine.world().counters.len > 0 {
                    p.engine.world_mut().counters.data[0].amount = count;
                    reference.engine.world_mut().counters.data[0].amount = count;
                    initialized = true;
                }
                let top = entries(&p.engine.world().stack).last().copied();
                let step = control.step(&p.engine).unwrap();
                if matches!(p.engine.execution(), Execution::Priority { .. }) {
                    if let Some(top) = top {
                        match top.body {
                            StackBody::Ability {
                                definition: Behavior(TIME | SUSPEND_CAST),
                                ..
                            } => {
                                assert!(matches!(
                                    step,
                                    AutoStep::Automatic {
                                        action: Action::Pass,
                                        reason: "suspend progression",
                                        ..
                                    }
                                ));
                                if matches!(
                                    top.body,
                                    StackBody::Ability {
                                        definition: Behavior(TIME),
                                        ..
                                    }
                                ) {
                                    time_passes += 1;
                                } else {
                                    cast_trigger_passes += 1;
                                }
                            }
                            StackBody::Spell {
                                origin: Origin::Card { card: RIFT, .. },
                                ..
                            } if p.engine.actor() == Some(other) => {
                                let AutoStep::Decision { actor, ref actions } = step else {
                                    panic!("spell response skipped")
                                };
                                assert_eq!(actor, other);
                                let reply = (0..actions.len())
                                    .map(|i| actions.get(i))
                                    .find(|a| matches!(a, Action::Cast { card: BOLT, .. }))
                                    .expect("opponent can respond with Bolt");
                                control.apply(&mut p, actor, reply).unwrap();
                                reference.apply(actor, reply).unwrap();
                                assert_eq!(p.engine, reference.engine);
                                responded = true;
                                break;
                            }
                            _ => (),
                        }
                    }
                }
                let (actor,action)=match step {
                    AutoStep::Automatic{actor,action,..}=>(actor,action),
                    AutoStep::Decision{actor,actions}=>{
                        if let Some(a)=(0..actions.len()).map(|i|actions.get(i)).find(|a|matches!(a,Action::SuspendTarget{target:Subject::Player{player}} if *player==other)) {
                            assert_eq!(actor,owner);
                            let choices: Vec<_> = (0..actions.len()).map(|i|actions.get(i)).collect();
                            assert!(choices.contains(&Action::DeclineSuspend));
                            assert!(choices.contains(&Action::SuspendTarget {target: Subject::Object {id: enemy_creature}}));
                            for target in [Subject::Player {player: owner}, Subject::Object {id: own_creature}] {
                                assert!(!choices.contains(&Action::SuspendTarget {target}));
                                let raw = p.observe(actor).actions().unwrap();
                                assert!((0..raw.len()).any(|i|raw.get(i)==Action::SuspendTarget {target}));
                            }
                            targets+=1;(actor,a)
                        } else {(actor,(0..actions.len()).map(|i|actions.get(i)).find(|a|*a==Action::Pass).unwrap_or(actions.get(0)))}
                    },
                    x=>panic!("unexpected suspend boundary {x:?}"),
                };
                control.apply(&mut p, actor, action).unwrap();
                reference.apply(actor, action).unwrap();
                assert_eq!(p.engine, reference.engine);
                for viewer in [owner, other] {
                    assert_eq!(p.observe(viewer), reference.observe(viewer));
                }
            }
            assert!(responded);
            assert_eq!(time_passes, 2 * count);
            assert_eq!(cast_trigger_passes, 2);
            assert_eq!(targets, 1);
        }
    }
}

#[test]
fn land_plan_matches_both_primitive_entries_and_preserves_atomic_rejection() {
    for owner in [Player(0), Player(1)] {
        for pay in [false, true] {
            let mut e = fixture();
            e.world_mut().zones.hand[owner.0 as usize][FOUNDRY.0 as usize] = 1;
            // Fixture seat is active; swap all actor/turn fields by making another fixture.
            let e = Engine::fixture(*e.world(), owner, 3);
            let mut expected = Position::new(e);
            let mut actual = expected.clone();
            expected.apply(owner, Action::Land { card: FOUNDRY }).unwrap();
            expected.apply(owner, Action::Shock { pay }).unwrap();
            let mut controls = Controls::new(owner, Mode::named("both"), None);
            controls.apply(&mut actual, owner, Action::LandPlan { card: FOUNDRY, pay }).unwrap();
            assert_eq!(actual.engine, expected.engine);
            actual.audit().unwrap();
            for viewer in [Player(0), Player(1)] { assert_eq!(actual.observe(viewer), expected.observe(viewer)); }
            assert!(!matches!(actual.engine.execution(), Execution::Running { running } if matches!(running.choice, Choice::YesNo { .. })));
        }
    }
    let mut e = fixture();
    e.world_mut().players[0].life = 1;
    e.world_mut().zones.hand[0][FOUNDRY.0 as usize] = 1;
    let saved = e;
    assert!(e.apply(Player(0), Action::LandPlan { card: FOUNDRY, pay: true }).is_err());
    assert_eq!(e, saved);
    assert!(e.apply(Player(0), Action::LandPlan { card: MOUNTAIN, pay: false }).is_err());
    assert_eq!(e, saved);
}

#[test]
fn land_macro_priors_and_rollouts_preserve_primitive_policy() {
    for life in [1, 2, 3, 20] {
        let mut e = fixture();
        e.world_mut().players[0].life = life;
        e.world_mut().zones.hand[0][FOUNDRY.0 as usize] = 1;
        e.world_mut().zones.hand[0][MOUNTAIN.0 as usize] = 1;
        let legacy = expand_mode(&e, Player(0), ActionSpace::build_auto(&e).unwrap(), Mode::named("legacy")).unwrap();
        let combined = expand(&e, Player(0), ActionSpace::build_auto(&e).unwrap()).unwrap();
        assert!(!(0..combined.len()).any(|i| combined.get(i)==Action::Land { card: FOUNDRY }));
        assert_eq!((0..combined.len()).filter(|&i| matches!(combined.get(i), Action::LandPlan { .. })).count(), if life>2 {2} else {1});
        let lc = SearchChoices::new(&e, Player(0), &legacy).unwrap();
        let cc = SearchChoices::new(&e, Player(0), &combined).unwrap();
        let lp = prior(&e, Player(0), &legacy, &lc).unwrap();
        let cp = prior(&e, Player(0), &combined, &cc).unwrap();
        let old = (0..lc.len()).find(|&i|legacy.get(lc.index(i))==Action::Land {card:FOUNDRY}).unwrap();
        let mass:f64=(0..cc.len()).filter(|&i|matches!(combined.get(cc.index(i)),Action::LandPlan {..})).map(|i|cp[i as usize]).sum();
        assert!((lp[old as usize]-mass).abs()<1e-10);
        for seed in 1..101 {
            let mut r1=Random(seed);let mut r2=Random(seed);
            let a=legacy.get(choose(&e,Player(0),&legacy,&lc,&mut r1));
            let b=combined.get(choose(&e,Player(0),&combined,&cc,&mut r2));
            assert_eq!(primitive(a),primitive(b));
            if a== (Action::Land {card:FOUNDRY}) {
                let mut next=e;next.apply(Player(0),a).unwrap();
                let sp=ActionSpace::build_auto(&next).unwrap();let ch=SearchChoices::new(&next,Player(0),&sp).unwrap();
                let Action::Shock{pay}=sp.get(super::super::rollout_policy::choose(&next,Player(0),&sp,&ch,&mut r1)) else {panic!()};
                assert_eq!(b,Action::LandPlan{card:FOUNDRY,pay});
            }
        }
    }
}

#[test]
fn auto_fetch_policy_handles_both_seats_sources_and_exhausted_libraries() {
    for actor in [Player(0), Player(1)] {
        for fetch_land in [MESA, MIRE] {
            for lands in [vec![], vec![MOUNTAIN], vec![FOUNDRY], vec![MOUNTAIN, FOUNDRY]] {
                let mut w = World::default();
                w.identity.next = 1;
                for p in 0..2 { w.players[p].life = 20; }
                w.zones.library[pid(actor)].push(BOLT).unwrap();
                for &card in &lands { w.zones.library[pid(actor)].push(card).unwrap(); }
                let mut e = Engine::fixture(w, actor, 3);
                e.enter(fetch_land, actor, false).unwrap();
                let object = entries(&e.world().battlefield)[0].ids.first;
                let action = Action::Fetch { object };
                let mut primitive = e;
                primitive.apply(actor, action).unwrap();
                let auto = ActionSpace::build_auto(&e).unwrap();
                assert_eq!((0..auto.len()).any(|i| auto.get(i) == action), !lands.is_empty());
                let plans = expand(&e, actor, auto).unwrap();
                assert!(!(0..plans.len()).any(|i| matches!(plans.get(i), Action::FetchPlan { card: None, .. })));

                // Primitive rules still allow the activation, including an empty search.
                let state = search_state(&e, actor, action).unwrap();
                let mut primitive = state;
                primitive.apply(actor, Action::Search { card: None }).unwrap();
                let auto = ActionSpace::build_auto(&state).unwrap();
                assert_eq!((0..auto.len()).any(|i| auto.get(i) == Action::Search { card: None }), lands.is_empty());
                if lands.is_empty() {
                    assert!(matches!(auto_step(&state).unwrap(), AutoStep::Automatic {
                        action: Action::Search { card: None }, ..
                    }));
                } else {
                    let plans = expand(&state, actor, auto).unwrap();
                    assert!(!(0..plans.len()).any(|i| matches!(plans.get(i), Action::SearchPlan { card: None, .. })));
                }
            }
        }
    }
}
