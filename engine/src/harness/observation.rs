use super::*;

#[path = "view.rs"]
mod view;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PublicEvent {
    RevealTop { player: Player, card: Card },
}
impl Engine {
    /// Transactional public notifications. Guide reveals even a nonland that remains on top.
    /// All other public changes are available in the resulting public state.
    pub fn apply_with_events(&mut self, p: Player, a: Action) -> Result<Vec<PublicEvent>> {
        let mut events = Vec::new();
        if a == Action::Pass
            && self.actor() == Some(p)
            && matches!(self.execution(),Execution::Priority{priority} if priority.consecutive_passes==1)
        {
            if let State::Playing { world, .. } = &self.state {
                if let Some(top) = entries(&world.stack).last() {
                    if let StackBody::Ability {
                        definition: Behavior(GUIDE_TRIGGER),
                        capture,
                        ..
                    } = top.body
                    {
                        let owner = Player(
                            self.capture_value(capture)
                                .ok_or(Error::Illegal("guide capture"))?
                                as u8,
                        );
                        if let Some(&card) = entries(&world.zones.library[pid(owner)]).last() {
                            events.push(PublicEvent::RevealTop {
                                player: owner,
                                card,
                            });
                        }
                    }
                }
            }
        }
        self.apply(p, a)?;
        Ok(events)
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
struct Memory {
    library: [Vec<Option<Card>>; 2],
    hand: [[u8; 16]; 2],
}
impl Memory {
    fn new(e: &Engine) -> Self {
        let z = zones(e).unwrap();
        Self {
            library: std::array::from_fn(|p| vec![None; z.library[p].len as usize]),
            hand: [[0; 16]; 2],
        }
    }
}
#[derive(Clone, Debug)]
pub struct Position {
    pub engine: Engine,
    memories: [Memory; 2],
    public: Memory,
    totals: [[u32; 16]; 2],
}
impl Position {
    /// Actual runner/test fixture boundary. Only Observation may be passed to search.
    pub fn new(engine: Engine) -> Self {
        let totals = inventory(&engine).expect("nonterminal initial position");
        Self {
            memories: [Memory::new(&engine), Memory::new(&engine)],
            public: Memory::new(&engine),
            engine,
            totals,
        }
    }
    pub fn observe(&self, p: Player) -> Observation {
        let mut template = self.engine;
        let z = zones_mut(&mut template).expect("observe nonterminal");
        let sizes = z.hand.map(|h| h.iter().map(|n| *n as usize).sum::<usize>());
        let memory = self.memories[pid(p)].clone();
        for owner in 0..2 {
            z.library[owner].data.fill(Card(16));
            if owner != pid(p) {
                z.hand[owner] = memory.hand[owner];
            }
        }
        Observation {
            template,
            viewer: p,
            sizes,
            memory,
            public: self.public.clone(),
            totals: self.totals,
        }
    }
    pub fn apply(&mut self, p: Player, a: Action) -> Result<Vec<PublicEvent>> {
        // Memory tracking only needs the old library lengths and shuffle owner.
        let old_lengths = zones(&self.engine).map(|z| z.library.each_ref().map(|l| l.len));
        let shuffle = chance_owner(&self.engine);
        let events = self.engine.apply_with_events(p, a)?;
        let Some(after) = zones(&self.engine) else {
            return Ok(events);
        };
        let old_lengths = old_lengths.unwrap();
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
                let old_n = old_lengths[owner] as usize;
                let mulligan_return = matches!(self.engine.state, State::Mulligan { .. })
                    && n > old_n
                    && !matches!(a, Action::Bottom { .. });
                if mulligan_return {
                    m.library[owner] = vec![None; n];
                    m.hand[owner] = [0; 16];
                } else if shuffle == Some(Player(owner as u8)) {
                    // Shuffling a library does not hide cards already known in hand.
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
                    | Action::LandPlan { card, .. }
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
    pub fn audit(&self) -> Result {
        if inventory(&self.engine).is_some_and(|x| x != self.totals) {
            return Err(Error::Illegal("inventory changed"));
        }
        Ok(())
    }
}
/// Redacted rule-state skeleton plus explicit information known to ONE player.
/// Private fields prevent callers from accidentally passing a full world as an observation.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Observation {
    template: Engine,
    pub viewer: Player,
    sizes: [usize; 2],
    memory: Memory,
    public: Memory,
    totals: [[u32; 16]; 2],
}
impl Observation {
    pub fn sample(&self, rng: &mut Random) -> Result<Position> {
        let mut e = self.template;
        let present = inventory(&e).unwrap();
        for p in 0..2 {
            let mut counts = [0u32; 16];
            for c in 0..16 {
                counts[c] = self.totals[p][c]
                    .checked_sub(present[p][c])
                    .ok_or(Error::Illegal("inconsistent visible inventory"))?;
            }
            for c in self.memory.library[p].iter().flatten() {
                let n = &mut counts[c.0 as usize];
                *n = n
                    .checked_sub(1)
                    .ok_or(Error::Illegal("inconsistent library knowledge"))?;
            }
            let mut pool = Vec::new();
            for (c, n) in counts.iter().enumerate() {
                for _ in 0..*n {
                    pool.push(Card(c as u16));
                }
            }
            rng.shuffle(&mut pool);
            let z = zones_mut(&mut e).unwrap();
            let visible: usize = z.hand[p].iter().map(|n| *n as usize).sum();
            let missing = self.sizes[p]
                .checked_sub(visible)
                .ok_or(Error::Illegal("known hand too large"))?;
            for _ in 0..missing {
                let c = pool.pop().ok_or(Error::Illegal("hand sample exhausted"))?;
                z.hand[p][c.0 as usize] += 1;
            }
            for i in 0..z.library[p].len as usize {
                z.library[p].data[i] = match self.memory.library[p][i] {
                    Some(c) => c,
                    None => pool
                        .pop()
                        .ok_or(Error::Illegal("library sample exhausted"))?,
                };
            }
            if !pool.is_empty() {
                return Err(Error::Illegal("sample inventory remainder"));
            }
        }
        let mut position = Position::new(e);
        position.public = self.public.clone();
        for viewer in 0..2 {
            position.memories[viewer] = self.public.clone();
            position.memories[viewer].hand[viewer] = zones(&position.engine).unwrap().hand[viewer];
        }
        position.memories[pid(self.viewer)] = self.memory.clone();
        position.totals = self.totals;
        Ok(position)
    }
    pub fn actions(&self) -> Result<ActionSpace> {
        // Legality depends on public state, own hand, and own remaining library
        // composition, never the sampled opposing hand or either library order.
        let position = self.sample(&mut Random(0))?;
        ActionSpace::build(&position.engine)
    }
    /// Choices offered to a bot/human after automatic primitives have advanced.
    /// `actions` remains the primitive diagnostic/replay API.
    pub fn auto_actions(&self) -> Result<ActionSpace> {
        let position = self.sample(&mut Random(0))?;
        match auto_step(&position.engine)? {
            AutoStep::Decision { actor, actions } if actor == self.viewer => Ok(actions),
            _ => Err(Error::Illegal("auto advance required before search")),
        }
    }
}
impl Observation {
    pub fn summary(&self) -> serde_json::Value {
        use serde_json::json;
        let z = zones(&self.template).unwrap();
        let hand = (0..16)
            .filter(|c| z.hand[pid(self.viewer)][*c] > 0)
            .map(|c| json!({"card":NAMES[c],"count":z.hand[pid(self.viewer)][c]}))
            .collect::<Vec<_>>();
        let public = match &self.template.state {
            State::Playing {
                world,
                progress,
                execution,
            } => {
                json!({"turn":progress.turn.serial.0,"active":progress.turn.active.0,"phase":format!("{:?}",progress.phase),"life":world.players.map(|p|p.life),"mana":world.players.map(|p|p.mana),"battlefield":entries(&world.battlefield).iter().map(|b|json!({"id":b.ids.first.0,"card":NAMES[card_of(b).0 as usize],"owner":b.base_controller.0,"tapped":b.status&TAPPED!=0,"damage":b.damage})).collect::<Vec<_>>(),"stack":format!("{:?}",entries(&world.stack)),"execution":format!("{:?}",execution)})
            }
            State::Mulligan { process, .. } => json!({"phase":"mulligan","taken":process.taken}),
            _ => json!({}),
        };
        json!({"viewer":self.viewer.0,"own_hand":hand,"hand_sizes":self.sizes,"library_sizes":z.library.map(|l|l.len),"public":public})
    }
}

#[cfg(test)]
#[path = "../../tests/support/memory_equivalence.rs"]
mod memory_equivalence;
