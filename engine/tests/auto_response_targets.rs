use burn_engine::{
    harness::*,
    terminal_bot::{
        macro_control::{Controls, Mode},
        macro_search,
    },
    *,
};
use mtg_state_design::burn::{Execution, World};

fn cast(card: Card, target: Subject) -> Action {
    Action::Cast {
        card,
        targets: [target, Subject::Absent],
        mode: 0,
        spectacle: false,
    }
}
fn fixture(
    owner: Player,
    caster: Player,
    face: bool,
    cover: bool,
) -> (Position, ObjectId, ObjectId) {
    let mut w = World::default();
    w.identity.next = 1;
    for p in 0..2 {
        w.players[p].life = 20;
        w.players[p].mana[RED] = 8;
        w.players[p].mana[WHITE] = 4;
        w.zones.hand[p][BOLT.0 as usize] = 2;
        w.zones.hand[p][HELIX.0 as usize] = 2;
        w.zones.hand[p][CHARM.0 as usize] = 1;
        for _ in 0..5 {
            w.zones.library[p].push(MOUNTAIN).unwrap();
        }
    }
    let mut e = Engine::fixture(w, caster, 3);
    let threatened = e.enter(GUIDE, owner, false).unwrap();
    let other = e.enter(GUIDE, owner, false).unwrap();
    let target = if face {
        Subject::Player { player: owner }
    } else {
        Subject::Object { id: threatened }
    };
    e.apply(caster, cast(HELIX, target)).unwrap();
    let mut mana = [0; 6];
    mana[RED] = 1;
    mana[WHITE] = 1;
    e.apply(caster, Action::Pay { mana }).unwrap();
    if cover {
        e.apply(caster, cast(BOLT, Subject::Player { player: owner }))
            .unwrap();
        mana[WHITE] = 0;
        e.apply(caster, Action::Pay { mana }).unwrap();
    }
    if caster != owner {
        e.apply(caster, Action::Pass).unwrap();
    }
    assert_eq!(e.actor(), Some(owner));
    (Position::new(e), threatened, other)
}
fn candidates(p: &Position, owner: Player) -> Vec<Action> {
    let s = p.observe(owner).auto_actions().unwrap();
    (0..s.len()).map(|i| s.get(i)).collect()
}
fn settle(p: &mut Position, c: &mut Controls) {
    for _ in 0..100 {
        if p.engine.world().stack.len == 0
            && matches!(p.engine.execution(), Execution::Priority { .. })
        {
            return;
        }
        let (actor, action) = match c.step(&p.engine).unwrap() {
            AutoStep::Automatic { actor, action, .. } => (actor, action),
            AutoStep::Decision { actor, .. } => (actor, Action::Pass),
            x => panic!("unexpected resolution {x:?}"),
        };
        c.apply(p, actor, action).unwrap();
    }
    panic!("stack did not empty");
}

#[test]
fn exception_is_only_the_opponent_targeted_creature_anywhere_on_stack() {
    for owner in [Player(0), Player(1)] {
        let opponent = Player(1 - owner.0);
        for caster in [owner, opponent] {
            for face in [false, true] {
                for cover in [false, true] {
                    let (p, threatened, other) = fixture(owner, caster, face, cover);
                    let actions = candidates(&p, owner);
                    for card in [BOLT, HELIX] {
                        assert_eq!(
                            actions.contains(&cast(card, Subject::Object { id: threatened })),
                            caster == opponent && !face
                        );
                        assert!(!actions.contains(&cast(card, Subject::Object { id: other })));
                        assert!(!actions.contains(&cast(card, Subject::Player { player: owner })));
                        assert!(
                            actions.contains(&cast(card, Subject::Player { player: opponent }))
                        );
                    }
                }
            }
        }
    }
}

#[test]
fn bolt_own_target_denies_helix_heal_and_is_available_to_ui_and_search() {
    for owner in [Player(0), Player(1)] {
        let opponent = Player(1 - owner.0);
        let (p, target, _) = fixture(owner, opponent, false, false);
        let response = cast(BOLT, Subject::Object { id: target });
        let mut control = Controls::new(owner, Mode::named("both"), None);
        let actions = control.actions(&p.engine).unwrap();
        let index = (0..actions.len())
            .find(|&i| actions.get(i) == response)
            .unwrap();
        let o = p.observe(owner);
        let editor = actions.editor(&o);
        assert_eq!(
            editor["choices"][index as usize]["action"],
            o.action_view(response)
        );
        if owner == Player(0) {
            if let Ok(path) = std::env::var("BURN_PLAY_FIXTURE_OUT") {
                let path =
                    std::path::Path::new(&path).with_file_name("helix-response-fixture.json");
                std::fs::write(path, serde_json::json!({"control":"auto-macro-v4","sequence":10,"status":"human","starting":0,"observation":o.agent_view(),"editor":editor,"outcome":null,"log":[],"ponder_enabled":false,"pondering":false,"budget_kind":"steps","budget_steps":800000,"thinking_steps":0,"reused_steps":0}).to_string()).unwrap();
            }
        }
        let report = macro_search::search(
            &o,
            macro_search::Config {
                budget: macro_search::Budget::Trials(128),
                ..Default::default()
            },
        );
        assert!(report.error.is_none(), "{:?}", report.error);
        assert!(
            report.details["candidates"]
                .as_array()
                .unwrap()
                .iter()
                .any(|r| r["action"] == action_text(response)),
            "bot searches the response"
        );
        let mut passed = p.clone();
        settle(&mut passed, &mut control.clone());
        assert_eq!(passed.engine.world().players[opponent.0 as usize].life, 23);
        let mut denied = p;
        control.apply(&mut denied, owner, response).unwrap();
        settle(&mut denied, &mut control);
        assert!(denied.engine.permanent(target).is_none());
        assert_eq!(denied.engine.world().players[opponent.0 as usize].life, 20);
    }
}

#[test]
fn exception_expires_after_the_opponent_spell_resolves() {
    for owner in [Player(0), Player(1)] {
        let opponent = Player(1 - owner.0);
        let (mut p, target, _) = fixture(owner, opponent, false, false);
        let bolt = cast(BOLT, Subject::Object { id: target });
        assert!(candidates(&p, owner).contains(&bolt));
        let mut c = Controls::new(owner, Mode::named("both"), None);
        c.apply(
            &mut p,
            owner,
            Action::Cast {
                card: CHARM,
                targets: [Subject::Absent; 2],
                mode: 1,
                spectacle: false,
            },
        )
        .unwrap();
        settle(&mut p, &mut c);
        assert!(
            p.engine.permanent(target).is_some(),
            "indestructible creature survives Helix"
        );
        // Open a new response window without targeting that creature again.
        assert_eq!(p.engine.actor(), Some(opponent));
        c.apply(
            &mut p,
            opponent,
            cast(BOLT, Subject::Player { player: owner }),
        )
        .unwrap();
        let mut mana = [0; 6];
        mana[RED] = 1;
        c.apply(&mut p, opponent, Action::Pay { mana }).unwrap();
        c.apply(&mut p, opponent, Action::Pass).unwrap();
        assert_eq!(p.engine.actor(), Some(owner));
        assert!(
            !candidates(&p, owner).contains(&bolt),
            "resolved targets no longer grant an exception"
        );
    }
}
