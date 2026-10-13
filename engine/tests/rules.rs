#[path = "../examples/support/mod.rs"]
mod audit_support;
use audit_support::*;
use burn_engine::*;
use mtg_state_design::burn::{Execution, Phase, World};
#[global_allocator]
static ALLOC: Allocator = Allocator;
fn pass(e: &mut Engine) {
    e.apply(e.actor().unwrap(), Action::Pass).unwrap();
}
fn keep() -> Engine {
    let mut e = Engine::new([[MOUNTAIN; 60]; 2], Player(0)).unwrap();
    e.apply(Player(0), Action::Mulligan { take: false })
        .unwrap();
    e.apply(Player(1), Action::Mulligan { take: false })
        .unwrap();
    e
}
fn main_phase() -> Engine {
    let mut e = keep();
    pass(&mut e);
    pass(&mut e);
    e
}
#[test]
fn draw_skip_priority_and_mana_expiry() {
    let mut e = keep();
    assert_eq!(e.actor(), Some(Player(0)));
    pass(&mut e);
    assert_eq!(e.actor(), Some(Player(1)));
    pass(&mut e);
    assert!(matches!(
        e.progress().phase,
        Phase::Main { postcombat: false }
    ));
    assert_eq!(e.world().zones.hand[0][15], 7);
    e.apply(Player(0), Action::Land { card: MOUNTAIN }).unwrap();
    let id = e.world().battlefield.data[0].ids.first;
    e.apply(
        Player(0),
        Action::Tap {
            object: id,
            color: RED as u8,
        },
    )
    .unwrap();
    assert_eq!(e.world().players[0].mana[RED], 1);
    pass(&mut e);
    pass(&mut e);
    assert_eq!(e.world().players[0].mana, [0; 6]);
    let mut rng = verification::Rng(1);
    for _ in 0..60 {
        if e.progress().turn.serial.0 == 2
            && matches!(
                e.progress().phase,
                Phase::Beginning {
                    step: BeginningStep::Draw {}
                }
            )
        {
            break;
        }
        if matches!(e.execution(), Execution::Priority { .. }) {
            pass(&mut e)
        } else {
            verification::drive_step(&mut e, &mut rng).unwrap();
        }
    }
    assert_eq!(e.progress().turn.serial.0, 2);
    assert_eq!(e.world().zones.hand[1][15], 8);
    assert_eq!(e.world().turn_history.land_plays, [0; 2]);
}
#[test]
fn reject_wrong_actor_bad_mana_and_sorcery_response_atomically() {
    let mut e = main_phase();
    let saved = e;
    assert!(e.apply(Player(1), Action::Land { card: MOUNTAIN }).is_err());
    assert_eq!(e, saved);
    e.world_mut().zones.hand[0][3] = 1;
    e.world_mut().zones.hand[0][4] = 1;
    e.world_mut().players[0].mana[RED] = 2;
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
    let saved = e;
    assert!(e.apply(Player(0), Action::Pay { mana: [0; 6] }).is_err());
    assert_eq!(e, saved);
    let mut mana = [0; 6];
    mana[RED] = 1;
    e.apply(Player(0), Action::Pay { mana }).unwrap();
    let saved = e;
    assert!(
        e.apply(
            Player(0),
            Action::Cast {
                card: SPIKE,
                targets: [Subject::Player { player: Player(1) }, Subject::Absent],
                mode: 0,
                spectacle: false
            }
        )
        .is_err()
    );
    assert_eq!(e, saved);
    pass(&mut e);
    pass(&mut e);
    assert_eq!(e.world().players[1].life, 17);
    assert_eq!(e.actor(), Some(Player(0)));
}
#[test]
fn tentative_cast_and_shock_conserve_cards() {
    let mut e = main_phase();
    e.world_mut().zones.hand[0][14] = 1;
    e.world_mut().zones.hand[0][3] = 1;
    let initial = inventory(&e);
    e.apply(Player(0), Action::Land { card: FOUNDRY }).unwrap();
    assert_eq!(inventory(&e), initial);
    e.apply(Player(0), Action::Shock { pay: true }).unwrap();
    assert_eq!(inventory(&e), initial);
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
    assert_eq!(inventory(&e), initial);
    let id = e.world().battlefield.data[0].ids.first;
    e.apply(
        Player(0),
        Action::Tap {
            object: id,
            color: RED as u8,
        },
    )
    .unwrap();
    assert_eq!(inventory(&e), initial);
    let mut mana = [0; 6];
    mana[RED] = 1;
    e.apply(Player(0), Action::Pay { mana }).unwrap();
    pass(&mut e);
    pass(&mut e);
    assert_eq!(inventory(&e), initial);
}
#[test]
fn branches_are_independent_and_replay_deterministic() {
    let mut rng = verification::Rng(98472);
    let mut e = Engine::new([verification::deck(&mut rng), verification::deck(&mut rng)], Player(1)).unwrap();
    for _ in 0..50 {
        verification::drive_step(&mut e, &mut rng).unwrap();
    }
    let saved = e;
    let mut a = e;
    let mut b = e;
    let mut ra = rng;
    let mut rb = rng;
    for _ in 0..100 {
        if a.finished().is_some() {
            break;
        }
        verification::drive_step(&mut a, &mut ra).unwrap();
        verification::drive_step(&mut b, &mut rb).unwrap();
        assert_eq!(a, b);
        assert_eq!(e, saved);
    }
}
#[test]
fn summoning_sickness_and_duplicate_attack_rejected() {
    let mut w = World::default();
    w.identity = Identity {
        next: 1,
        next_stamp: 1,
    };
    w.players = [PlayerCore {
        life: 20,
        ..Default::default()
    }; 2];
    let mut e = Engine::fixture(w, Player(0), 3);
    let id = e.enter(EIDOLON, Player(0), false).unwrap();
    for _ in 0..4 {
        pass(&mut e);
    }
    let mut ids = [ObjectId(0); 24];
    ids[0] = id;
    let saved = e;
    assert!(
        e.apply(
            Player(0),
            Action::Attack {
                objects: ids,
                len: 1
            }
        )
        .is_err()
    );
    assert_eq!(e, saved);
    e.world_mut().battlefield.data[0].status |= CONTROLLED;
    ids[1] = id;
    let saved = e;
    assert!(
        e.apply(
            Player(0),
            Action::Attack {
                objects: ids,
                len: 2
            }
        )
        .is_err()
    );
    assert_eq!(e, saved);
}
#[test]
fn games_conserve_cards_without_allocating() {
    counting(true);
    for seed in 1..=128 {
        let mut rng = verification::Rng(seed);
        let mut e = Engine::new(
            [verification::deck(&mut rng), verification::deck(&mut rng)],
            Player((seed % 2) as u8),
        )
        .unwrap();
        for step in 0..20000 {
            if e.finished().is_some() {
                break;
            }
            assert!(step < 19999);
            assert_deck(&e);
            verification::drive_step(&mut e, &mut rng).unwrap();
        }
    }
    assert_eq!(counting(false), 0);
}

#[test]
fn capacity_overflow_rolls_back_entire_transition() {
    let mut e = main_phase();
    e.world_mut().zones.hand[0][BOLT.0 as usize] = 1;
    e.world_mut().stack.len = 16;
    let before = e;
    let result = e.apply(
        Player(0),
        Action::Cast {
            card: BOLT,
            targets: [Subject::Player { player: Player(1) }, Subject::Absent],
            mode: 0,
            spectacle: false,
        },
    );
    assert_eq!(result, Err(Error::Capacity));
    assert_eq!(e, before);
}

#[test]
fn invalid_shuffle_is_atomic() {
    let mut e = Engine::new([[MOUNTAIN; 60]; 2], Player(0)).unwrap();
    e.apply(Player(0), Action::Mulligan { take: true }).unwrap();
    e.apply(Player(1), Action::Mulligan { take: false })
        .unwrap();
    let before = e;
    assert!(
        e.apply(
            Player(0),
            Action::Shuffle {
                order: [0; 60],
                len: 60
            }
        )
        .is_err()
    );
    assert_eq!(e, before);
}
