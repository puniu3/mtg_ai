use super::*;

#[test]
fn auto_damage_targets_exclude_friendlies_preserve_buffs_order_and_capacity_errors() {
    let mut checked = 0;
    for actor in [Player(0), Player(1)] {
        for creatures in 0..=4 {
            let mut w = mtg_state_design::burn::World::default();
            w.identity.next = 1;
            for p in 0..2 {
                w.players[p].life = 20;
                w.players[p].mana[WHITE] = 4;
                w.players[p].mana[RED] = 4;
            }
            for card in [BOLT, SPIKE, BLAZE, CHARM, HELIX, RIFT, SKEWER] {
                w.zones.hand[pid(actor)][card.0 as usize] = 1;
            }
            let mut e = Engine::fixture(w, actor, 3);
            for i in 0..creatures {
                e.enter(if i < 2 { GUIDE } else { SWIFT }, Player(i % 2), false).unwrap();
            }
            let full = ActionSpace::build(&e).unwrap().without_taps();
            let auto = ActionSpace::build_auto(&e).unwrap();
            let ours: Vec<_> = entries(&e.world().battlefield).iter()
                .filter(|b| b.base_controller == actor).map(|b| b.ids.first).collect();
            let mut expected = Vec::new();
            let mut removed = 0;
            for i in 0..full.len() {
                let a = full.get(i);
                if let Action::Cast { card, mode, targets, .. } = a {
                    if !(card == CHARM && mode != 0) && targets.iter().any(|t| match t {
                        Subject::Player { player } => *player == actor,
                        Subject::Object { id } => ours.contains(id),
                        _ => false,
                    }) { removed += 1; continue; }
                }
                expected.push(a);
            }
            assert!(removed > 0, "primitive rules still allow self damage");
            assert_eq!(expected, (0..auto.len()).map(|i|auto.get(i)).collect::<Vec<_>>());
            let enemy_count = creatures as usize - ours.len();
            assert_eq!((0..auto.len()).filter(|&i| matches!(auto.get(i), Action::Cast {card: BLAZE,..})).count(), enemy_count);
            for card in [BOLT, SPIKE, BLAZE, CHARM, HELIX, RIFT, SKEWER] {
                assert!((0..auto.len()).any(|i| matches!(auto.get(i), Action::Cast {card: c, targets: [Subject::Player { player }, _], mode: 0, ..} if c == card && player != actor)) || (card == BLAZE && enemy_count == 0));
            }
            for id in ours {
                assert!((0..auto.len()).any(|i| matches!(auto.get(i), Action::Cast {card: CHARM, mode: 2, targets: [Subject::Object {id: target}, _], ..} if target == id)), "own double strike retained");
            }
            for exhaust_ids in [false, true] {
                let mut limit = e;
                if exhaust_ids {
                    limit.world_mut().identity.next = u32::MAX;
                } else {
                    let pool = &mut limit.world_mut().cast_records;
                    pool.len = pool.data.len() as u16;
                }
                let old = ActionSpace::build(&limit).unwrap_err();
                let new = ActionSpace::build_auto(&limit).unwrap_err();
                assert_eq!(old, new);
                assert_eq!(new, Error::Capacity);
            }
            checked += 1;
        }
    }
    eprintln!("target-order and capacity fixtures: {checked}");
}
