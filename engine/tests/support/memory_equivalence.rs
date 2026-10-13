// Differential oracle: pre-optimization update, with the same knowledge fixes.
use super::*;

impl Position {
    fn reference_apply(&mut self, p: Player, a: Action) -> Result<Vec<PublicEvent>> {
        let before = self.engine;
        let events = self.engine.apply_with_events(p, a)?;
        let Some(after) = zones(&self.engine) else {
            return Ok(events);
        };
        let old = zones(&before).unwrap();
        let shuffle = chance_owner(&before);
        for viewer in 0..3 {
            let m = if viewer == 2 {
                &mut self.public
            } else {
                &mut self.memories[viewer]
            };
            for &PublicEvent::RevealTop { player, card } in &events {
                if let Some(top) = m.library[pid(player)].last_mut() {
                    *top = Some(card);
                }
            }
            for owner in 0..2 {
                let n = after.library[owner].len as usize;
                let old_n = old.library[owner].len as usize;
                let mulligan_return = matches!(self.engine.state, State::Mulligan { .. })
                    && n > old_n
                    && !matches!(a, Action::Bottom { .. });
                if mulligan_return {
                    m.library[owner] = vec![None; n];
                    m.hand[owner] = [0; 16];
                } else if shuffle == Some(Player(owner as u8)) {
                    m.library[owner] = vec![None; n];
                } else if matches!(a, Action::Search { .. }) && owner == pid(p) {
                    // A search immediately leads to a shuffle; physical identical-copy identity is not observed.
                    m.library[owner] = vec![None; n];
                } else if let Action::Bottom { card } = a {
                    if owner == pid(p) {
                        m.library[owner].insert(0, if viewer == owner { Some(card) } else { None });
                        // The opponent cannot tell whether a previously known card was put back.
                        if viewer != owner {
                            for x in &mut m.hand[owner] {
                                *x = x.saturating_sub(1);
                            }
                        }
                    }
                } else if n < old_n {
                    while m.library[owner].len() > n {
                        if let Some(Some(c)) = m.library[owner].pop() {
                            m.hand[owner][c.0 as usize] += 1;
                        }
                    }
                }
                m.library[owner].resize(n, None);
                let played = match a {
                    Action::Land { card }
                    | Action::Cast { card, .. }
                    | Action::Discard { card } => Some(card),
                    Action::Suspend => Some(RIFT),
                    _ => None,
                };
                if owner == pid(p) {
                    if let Some(c) = played {
                        m.hand[owner][c.0 as usize] = m.hand[owner][c.0 as usize].saturating_sub(1);
                    }
                }
                if viewer == owner {
                    m.hand[owner] = after.hand[owner];
                }
            }
        }
        Ok(events)
    }
}


fn compare_position(a: &Position, b: &Position) {
    assert_eq!(a.engine, b.engine);
    assert_eq!(a.memories, b.memories);
    assert_eq!(a.public, b.public);
    assert_eq!(a.totals, b.totals);
}
fn next(p: &Position, rng: &mut Random) -> Option<(Player, Action)> {
    match auto_step(&p.engine).unwrap() {
        AutoStep::Finished(_) => None,
        AutoStep::Chance => Some((Player(0), chance_action(&p.engine, rng).unwrap())),
        AutoStep::Automatic { actor, action, .. } => Some((actor, action)),
        AutoStep::Decision { actor, actions } => Some((actor, actions.get(rng.below(actions.len())))),
    }
}
#[test]
fn memory_matches_original_across_games_and_sampled_worlds() {
    let mut boundaries = 0;
    let mut sampled = 0;
    for seed in 1000..1256 {
        let mut rng = Random(seed);
        let mut actual = Position::new(Engine::new([deck(&mut rng), deck(&mut rng)], Player(0)).unwrap());
        let mut reference = actual.clone();
        for step in 0..10000 {
            let Some((actor, action)) = next(&reference, &mut rng) else { break; };
            // Check the observer-specific memories rebuilt by determinization, too.
            // Search samples only at an auto decision, from its actor's view.
            // Sampling another player's hand halfway through automatic payment
            // can remove the reserved card, which is not a supported search root.
            if step % 7 == 0 {
                if let AutoStep::Decision { actor: viewer, .. } = auto_step(&reference.engine).unwrap() {
                    let mut a = actual.observe(viewer).sample(&mut Random(seed + step)).unwrap();
                    let mut b = reference.observe(viewer).sample(&mut Random(seed + step)).unwrap();
                    if let Some((p, action)) = next(&b, &mut Random(seed ^ step)) {
                        assert_eq!(a.apply(p, action), b.reference_apply(p, action));
                        compare_position(&a, &b);
                        sampled += 1;
                    }
                }
            }
            assert_eq!(actual.apply(actor, action), reference.reference_apply(actor, action));
            compare_position(&actual, &reference);
            actual.audit().unwrap();
            boundaries += 1;
        }
        assert!(actual.engine.finished().is_some(), "seed {seed}");
    }
    eprintln!("memory-equivalent primitive boundaries: {boundaries}; sampled worlds: {sampled}");
}
