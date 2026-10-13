//! Allocation-free rules for the fixed 16-card Boros Burn mirror.
//! Canonical storage is the reviewed state schema. Solvers and I/O are separate.
use mtg_state_design::burn::{Combat, Execution, Phase, Progress, Running, Setup, State, World};
pub use mtg_state_design::*;

pub const GUIDE: Card = Card(0);
pub const SWIFT: Card = Card(1);
pub const EIDOLON: Card = Card(2);
pub const BOLT: Card = Card(3);
pub const SPIKE: Card = Card(4);
pub const RIFT: Card = Card(5);
pub const SKEWER: Card = Card(6);
pub const CHARM: Card = Card(7);
pub const BLAZE: Card = Card(8);
pub const HELIX: Card = Card(9);
pub const MESA: Card = Card(10);
pub const MIRE: Card = Card(11);
pub const CANYON: Card = Card(12);
pub const VANTAGE: Card = Card(13);
pub const FOUNDRY: Card = Card(14);
pub const MOUNTAIN: Card = Card(15);
pub const NAMES: [&str; 16] = [
    "Goblin Guide",
    "Monastery Swiftspear",
    "Eidolon of the Great Revel",
    "Lightning Bolt",
    "Lava Spike",
    "Rift Bolt",
    "Skewer the Critics",
    "Boros Charm",
    "Searing Blaze",
    "Lightning Helix",
    "Arid Mesa",
    "Bloodstained Mire",
    "Sunbaked Canyon",
    "Inspiring Vantage",
    "Sacred Foundry",
    "Mountain",
];
pub const DECK_COUNTS: [u8; 16] = [4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 3, 4, 4, 2, 3];
pub const WHITE: usize = 0;
pub const RED: usize = 3;
pub const TAPPED: u16 = 1;
pub const CONTROLLED: u16 = 2;
const PROWESS: u16 = 100;
const EIDOLON_TRIGGER: u16 = 101;
const GUIDE_TRIGGER: u16 = 102;
const FETCH: u16 = 103;
const DRAW: u16 = 104;
const TIME: u16 = 105;
const SUSPEND_CAST: u16 = 106;
const INDESTRUCTIBLE: u16 = 200;
const DOUBLE: u16 = 201;
const ATTACK_CHOICE: u16 = 300;
const BLOCK_CHOICE: u16 = 301;
const FETCH_CHOICE: u16 = 302;
const DISCARD_CHOICE: u16 = 303;
const SUSPEND_CHOICE: u16 = 304;
const MULLIGAN_BOTTOM: u16 = 305;
const SHUFFLE_CHOICE: u16 = 306;
const DAMAGE_CHOICE: u16 = 307;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Action {
    #[default]
    Pass,
    Land {
        card: Card,
    },
    Cast {
        card: Card,
        targets: [Subject; 2],
        mode: u8,
        spectacle: bool,
    },
    Tap {
        object: ObjectId,
        color: u8,
    },
    Pay {
        mana: [u32; 6],
    },
    Fetch {
        object: ObjectId,
    },
    LandPlan {
        card: Card,
        pay: bool,
    },
    FetchPlan {
        object: ObjectId,
        card: Option<Card>,
        pay: bool,
    },
    SearchPlan {
        card: Option<Card>,
        pay: bool,
    },
    Canyon {
        object: ObjectId,
    },
    Suspend,
    Shock {
        pay: bool,
    },
    Search {
        card: Option<Card>,
    },
    Shuffle {
        order: [u8; 60],
        len: u8,
    },
    OrderTrigger {
        trigger: ObjectId,
    },
    Attack {
        objects: [ObjectId; 24],
        len: u8,
    },
    Block {
        pairs: [(ObjectId, ObjectId); 24],
        len: u8,
    },
    Damage {
        assignments: [(ObjectId, Subject, u32); 48],
        len: u8,
    },
    SuspendTarget {
        target: Subject,
    },
    DeclineSuspend,
    Discard {
        card: Card,
    },
    Mulligan {
        take: bool,
    },
    Bottom {
        card: Card,
    },
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    Illegal(&'static str),
    Capacity,
    Overflow,
}
pub type Result<T = ()> = std::result::Result<T, Error>;
impl From<CapacityError> for Error {
    fn from(_: CapacityError) -> Self {
        Self::Capacity
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Engine {
    pub state: State,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Cost {
    pub red: u32,
    pub white: u32,
    pub generic: u32,
}
pub fn creature(c: Card) -> bool {
    c.0 < 3
}
pub fn land(c: Card) -> bool {
    c.0 >= 10 && c.0 < 16
}
pub fn instant(c: Card) -> bool {
    matches!(c, BOLT | CHARM | BLAZE | HELIX)
}
pub fn mana_value(c: Card) -> u32 {
    match c {
        GUIDE | SWIFT | BOLT | SPIKE => 1,
        EIDOLON | CHARM | BLAZE | HELIX => 2,
        RIFT | SKEWER => 3,
        _ => 0,
    }
}
pub fn cost(c: Card, spectacle: bool) -> Cost {
    match c {
        GUIDE | SWIFT | BOLT | SPIKE => Cost {
            red: 1,
            ..Cost::default()
        },
        EIDOLON | BLAZE => Cost {
            red: 2,
            ..Cost::default()
        },
        CHARM | HELIX => Cost {
            red: 1,
            white: 1,
            generic: 0,
        },
        SKEWER if spectacle => Cost {
            red: 1,
            ..Cost::default()
        },
        RIFT | SKEWER => Cost {
            red: 1,
            generic: 2,
            white: 0,
        },
        _ => Cost::default(),
    }
}
fn require(ok: bool, why: &'static str) -> Result {
    if ok { Ok(()) } else { Err(Error::Illegal(why)) }
}
fn pop<T: Copy, const N: usize>(p: &mut Pool<T, N>) -> Option<T> {
    if p.len == 0 {
        None
    } else {
        p.len -= 1;
        Some(p.data[p.len as usize])
    }
}
fn remove<T: Copy, const N: usize>(p: &mut Pool<T, N>, i: usize) -> T {
    let v = p.data[i];
    p.data.copy_within(i + 1..p.len as usize, i);
    p.len -= 1;
    v
}
fn entries<T: Copy, const N: usize>(p: &Pool<T, N>) -> &[T] {
    &p.data[..p.len as usize]
}
fn pid(p: Player) -> usize {
    p.0 as usize
}
fn other(p: Player) -> Player {
    Player(p.0 ^ 1)
}
fn card_of(p: &Permanent) -> Card {
    if let Origin::Card { card, .. } = p.origin {
        card
    } else {
        panic!("Burn has no tokens")
    }
}
fn owner_of(p: &Permanent) -> Player {
    match p.origin {
        Origin::Card { owner, .. } | Origin::Token { owner, .. } => owner,
        _ => unreachable!(),
    }
}
fn push_count(count: &mut u8) -> Result {
    *count = count.checked_add(1).ok_or(Error::Overflow)?;
    Ok(())
}

impl Engine {
    pub fn new(libraries: [[Card; 60]; 2], starting: Player) -> Result<Self> {
        require(starting.0 < 2, "starting player")?;
        let mut setup = Setup::default();
        setup.identity = Identity {
            next: 1,
            next_stamp: 1,
        };
        for p in 0..2 {
            for &c in libraries[p].iter().rev() {
                require(c.0 < 16, "unsupported card")?;
                setup.zones.library[p].push(c)?;
            }
            for _ in 0..7 {
                let c = pop(&mut setup.zones.library[p]).unwrap();
                push_count(&mut setup.zones.hand[p][c.0 as usize])?;
            }
        }
        Ok(Self {
            state: State::Mulligan {
                setup,
                process: mtg_state_design::Mulligan {
                    starting,
                    next: starting,
                    ..Default::default()
                },
                execution: Execution::Idle,
            },
        })
    }
    /// Test fixtures only: enter a specified main-phase position without simulating its history.
    pub fn fixture(world: World, active: Player, turn: u32) -> Self {
        Self {
            state: State::Playing {
                world,
                progress: Progress {
                    turn: TurnHeader {
                        serial: TurnId(turn),
                        instance: RecordId(turn),
                        active,
                        next_normal: other(active),
                        decision_controller: active,
                    },
                    phase: Phase::Main { postcombat: false },
                },
                execution: Execution::Priority {
                    priority: Priority {
                        player: active,
                        consecutive_passes: 0,
                    },
                },
            },
        }
    }
    pub fn world(&self) -> &World {
        match &self.state {
            State::Playing { world, .. } | State::Opening { world, .. } => world,
            _ => panic!("not playing"),
        }
    }
    pub fn world_mut(&mut self) -> &mut World {
        match &mut self.state {
            State::Playing { world, .. } | State::Opening { world, .. } => world,
            _ => panic!("not playing"),
        }
    }
    pub fn progress(&self) -> Progress {
        if let State::Playing { progress, .. } = self.state {
            progress
        } else {
            panic!("not playing")
        }
    }
    fn progress_mut(&mut self) -> &mut Progress {
        if let State::Playing { progress, .. } = &mut self.state {
            progress
        } else {
            panic!("not playing")
        }
    }
    pub fn execution(&self) -> Execution {
        match self.state {
            State::Playing { execution, .. }
            | State::Mulligan { execution, .. }
            | State::Opening { execution, .. } => execution,
            _ => Execution::Idle,
        }
    }
    fn set_execution(&mut self, e: Execution) {
        match &mut self.state {
            State::Playing { execution, .. }
            | State::Mulligan { execution, .. }
            | State::Opening { execution, .. } => *execution = e,
            _ => {}
        }
    }
    fn running(&self) -> Running {
        if let Execution::Running { running } = self.execution() {
            running
        } else {
            Running::default()
        }
    }
    fn set_running(&mut self, running: Running) {
        self.set_execution(Execution::Running { running })
    }
    fn priority(&mut self, p: Player) {
        self.set_execution(Execution::Priority {
            priority: Priority {
                player: p,
                consecutive_passes: 0,
            },
        })
    }
    pub fn actor(&self) -> Option<Player> {
        match self.execution() {
            Execution::Priority { priority } => Some(priority.player),
            Execution::Running { running } => match running.choice {
                Choice::Select { player, .. }
                | Choice::Number { player, .. }
                | Choice::Order { player, .. }
                | Choice::YesNo { player }
                | Choice::Mana { player, .. } => Some(player),
                Choice::Chance { .. } => None,
                _ => None,
            },
            _ => {
                if let State::Mulligan { process, .. } = self.state {
                    Some(process.next)
                } else {
                    None
                }
            }
        }
    }
    pub fn finished(&self) -> Option<mtg_state_design::Outcome> {
        if let State::Finished { outcome } = self.state {
            Some(outcome)
        } else {
            None
        }
    }
    pub fn apply(&mut self, player: Player, action: Action) -> Result {
        // Atomic rejection. Fixed storage; no heap or aliases into the checkpoint.
        let saved = *self;
        let result = self.apply_inner(player, action);
        if result.is_err() {
            *self = saved;
        }
        result
    }
    fn apply_inner(&mut self, p: Player, a: Action) -> Result {
        require(p.0 < 2, "player")?;
        if matches!(self.state, State::Mulligan { .. }) {
            return self.mulligan_input(p, a);
        }
        require(
            matches!(self.state, State::Playing { .. }),
            "game not playing",
        )?;
        if let Execution::Running { running } = self.execution() {
            return self.choice_input(p, a, running);
        }
        require(self.actor() == Some(p), "priority")?;
        match a {
            Action::Pass => self.pass(p),
            Action::Land { card } => self.play_land(p, card),
            Action::LandPlan { card, pay } => {
                require(card == FOUNDRY, "land plan requires shockland")?;
                self.play_land(p, card)?;
                self.apply_inner(p, Action::Shock { pay })
            },
            Action::Cast {
                card,
                targets,
                mode,
                spectacle,
            } => self.start_cast(p, card, targets, mode, spectacle, false),
            Action::Tap { object, color } => {
                self.tap(p, object, color)?;
                self.stabilize(p)
            }
            Action::Fetch { object } => self.activate_fetch(p, object),
            Action::Canyon { object } => self.activate_canyon(p, object),
            Action::Suspend => self.suspend(p),
            _ => Err(Error::Illegal("unexpected action at priority")),
        }
    }
    fn new_id(&mut self) -> Result<ObjectId> {
        Ok(self.world_mut().identity.allocate_range(1)?.first)
    }
    fn stamp(&mut self) -> Result<Stamp> {
        let w = self.world_mut();
        let n = w.identity.next_stamp;
        w.identity.next_stamp = n.checked_add(1).ok_or(Error::Overflow)?;
        Ok(Stamp(n))
    }
    pub fn permanent(&self, id: ObjectId) -> Option<&Permanent> {
        entries(&self.world().battlefield)
            .iter()
            .find(|x| x.ids.contains(id))
    }
    fn permanent_mut(&mut self, id: ObjectId) -> Option<&mut Permanent> {
        let b = &mut self.world_mut().battlefield;
        b.data[..b.len as usize]
            .iter_mut()
            .find(|x| x.ids.contains(id))
    }
    fn sorcery(&self, p: Player) -> bool {
        self.progress().turn.active == p
            && matches!(self.progress().phase, Phase::Main { .. })
            && self.world().stack.len == 0
    }
    pub fn has_ability(&self, id: ObjectId, ability: u16) -> bool {
        entries(&self.world().effects).iter().any(|e| {
            matches!(e.selection,Selection::One{subject:Subject::Object{id:x}} if x==id)
                && matches!(e.body,EffectBody::GrantAbility{ability:a} if a.0==ability)
        })
    }
    pub fn power_toughness(&self, id: ObjectId) -> Option<(i32, i32)> {
        let p = self.permanent(id)?;
        let c = card_of(p);
        if !creature(c) {
            return None;
        }
        let (mut power, mut toughness) = if c == SWIFT { (1, 2) } else { (2, 2) };
        for e in entries(&self.world().effects) {
            if matches!(e.selection,Selection::One{subject:Subject::Object{id:x}} if x==id) {
                if let EffectBody::Pump {
                    power: p,
                    toughness: t,
                } = e.body
                {
                    power += p;
                    toughness += t;
                }
            }
        }
        Some((power, toughness))
    }
    fn valid_target(&self, t: Subject, only_player: bool, only_creature: bool) -> bool {
        match t {
            Subject::Player { player } => player.0 < 2 && !only_creature,
            Subject::Object { id } => {
                !only_player && self.permanent(id).is_some_and(|p| creature(card_of(p)))
            }
            _ => false,
        }
    }
    fn check_targets(&self, c: Card, t: [Subject; 2], mode: u8) -> Result {
        if creature(c) {
            return require(t == [Subject::Absent; 2] && mode == 0, "creature targets");
        }
        require(c.0 >= 3 && c.0 <= 9, "spell")?;
        if c == CHARM {
            require(mode < 3, "charm mode")?;
            if mode == 1 {
                return require(t == [Subject::Absent; 2], "indestructible has no target");
            }
            require(
                self.valid_target(t[0], mode == 0, mode == 2) && t[1] == Subject::Absent,
                "charm target",
            )
        } else if c == BLAZE {
            require(mode == 0, "mode")?;
            require(self.valid_target(t[0], true, false), "blaze player")?;
            if let (Subject::Player { player }, Subject::Object { id }) = (t[0], t[1]) {
                require(
                    self.permanent(id)
                        .is_some_and(|x| creature(card_of(x)) && x.base_controller == player),
                    "blaze creature controller",
                )
            } else {
                Err(Error::Illegal("blaze targets"))
            }
        } else {
            require(
                mode == 0 && t[1] == Subject::Absent && self.valid_target(t[0], c == SPIKE, false),
                "spell target",
            )
        }
    }
    fn play_land(&mut self, p: Player, c: Card) -> Result {
        require(
            land(c) && self.sorcery(p) && self.world().turn_history.land_plays[pid(p)] == 0,
            "land timing/limit",
        )?;
        require(
            self.world().zones.hand[pid(p)][c.0 as usize] > 0,
            "land in hand",
        )?;
        self.world_mut().zones.hand[pid(p)][c.0 as usize] -= 1;
        self.world_mut().turn_history.land_plays[pid(p)] += 1;
        self.begin_entry(c, p, false)?;
        if !matches!(self.execution(), Execution::Running { .. }) {
            self.stabilize(p)?
        }
        Ok(())
    }
    fn begin_entry(&mut self, c: Card, p: Player, from_fetch: bool) -> Result {
        if c == FOUNDRY {
            let mut r = Running::default();
            r.entering.push(Entering {
                origin: Origin::Card { card: c, owner: p },
                controller: p,
                ..Default::default()
            })?;
            r.choice = Choice::YesNo { player: p };
            r.frames.push(Frame::TurnAction {
                step: Step::PrecombatMain {},
                stage: if from_fetch { 2 } else { 1 },
                selected: Span::default(),
            })?;
            self.set_running(r);
            Ok(())
        } else {
            let tapped = c == VANTAGE
                && entries(&self.world().battlefield)
                    .iter()
                    .filter(|x| x.base_controller == p && land(card_of(x)))
                    .count()
                    >= 3;
            self.enter(c, p, tapped)?;
            Ok(())
        }
    }
    pub fn enter(&mut self, c: Card, p: Player, tapped: bool) -> Result<ObjectId> {
        let id = self.new_id()?;
        let stamp = self.stamp()?;
        self.world_mut().battlefield.push(Permanent {
            ids: IdRange {
                first: id,
                count: 1,
            },
            origin: Origin::Card { card: c, owner: p },
            timestamp: stamp,
            damage: 0,
            base_controller: p,
            status: if tapped { TAPPED } else { 0 },
        })?;
        if land(c) {
            self.world_mut().turn_history.land_entered_mask |= 1 << p.0;
        }
        Ok(id)
    }
    fn lose(&mut self, p: Player, n: u32) -> Result {
        let w = self.world_mut();
        w.players[pid(p)].life = w.players[pid(p)]
            .life
            .checked_sub(i32::try_from(n).map_err(|_| Error::Overflow)?)
            .ok_or(Error::Overflow)?;
        if n > 0 {
            w.turn_history.life_lost_mask |= 1 << p.0;
        }
        Ok(())
    }
    fn pay_life(&mut self, p: Player, n: u32) -> Result {
        require(
            self.world().players[pid(p)].life >= n as i32,
            "life payment",
        )?;
        self.lose(p, n)
    }
    fn tap(&mut self, p: Player, id: ObjectId, color: u8) -> Result {
        let per = *self.permanent(id).ok_or(Error::Illegal("mana source"))?;
        let c = card_of(&per);
        require(
            per.base_controller == p && per.status & TAPPED == 0,
            "mana source unavailable",
        )?;
        require(
            (color as usize == RED && matches!(c, MOUNTAIN | FOUNDRY | VANTAGE | CANYON))
                || (color as usize == WHITE && matches!(c, FOUNDRY | VANTAGE | CANYON)),
            "mana color",
        )?;
        if c == CANYON {
            self.pay_life(p, 1)?;
        }
        self.permanent_mut(id).unwrap().status |= TAPPED;
        let n = &mut self.world_mut().players[pid(p)].mana[color as usize];
        *n = n.checked_add(1).ok_or(Error::Overflow)?;
        Ok(())
    }
    fn pay_mana(&mut self, p: Player, cost: Cost, mana: [u32; 6]) -> Result {
        let total = mana
            .iter()
            .try_fold(0u32, |a, x| a.checked_add(*x))
            .ok_or(Error::Overflow)?;
        require(
            mana[RED] >= cost.red
                && mana[WHITE] >= cost.white
                && total == cost.red + cost.white + cost.generic,
            "payment mismatch",
        )?;
        require(
            mana.iter()
                .zip(self.world().players[pid(p)].mana)
                .all(|(a, b)| *a <= b),
            "mana unavailable",
        )?;
        for (i, n) in mana.into_iter().enumerate() {
            self.world_mut().players[pid(p)].mana[i] -= n;
        }
        Ok(())
    }
    fn start_cast(
        &mut self,
        p: Player,
        c: Card,
        targets: [Subject; 2],
        mode: u8,
        spectacle: bool,
        free: bool,
    ) -> Result {
        self.check_targets(c, targets, mode)?;
        require(free || instant(c) || self.sorcery(p), "spell timing")?;
        require(
            !spectacle
                || (c == SKEWER
                    && self.world().turn_history.life_lost_mask & (1 << other(p).0) != 0),
            "spectacle",
        )?;
        if !free {
            require(
                self.world().zones.hand[pid(p)][c.0 as usize] > 0,
                "spell in hand",
            )?;
            self.world_mut().zones.hand[pid(p)][c.0 as usize] -= 1;
        }
        let id = self.new_id()?;
        let record = RecordId(self.new_id()?.0);
        let start = self.world().targets.len;
        for target in targets {
            if target != Subject::Absent {
                self.world_mut().targets.push(Target {
                    subject: target,
                    role: 0,
                    division: 0,
                })?;
            }
        }
        let span = Span {
            start,
            len: self.world().targets.len - start,
        };
        self.world_mut().cast_records.push(CastRecord {
            id: record,
            modes: mode as u32,
            route: if free {
                CastRoute::Suspend {}
            } else if spectacle {
                CastRoute::Alternative { rule: Behavior(1) }
            } else {
                CastRoute::Normal
            },
            ..Default::default()
        })?;
        self.world_mut().stack.push(StackRun {
            ids: IdRange {
                first: id,
                count: 1,
            },
            controller: p,
            body: StackBody::Spell {
                origin: Origin::Card { card: c, owner: p },
                face: 0,
                cast: record,
            },
            targets: span,
        })?;
        let mut r = Running::default();
        r.choice = Choice::Mana {
            player: p,
            casting: record,
        };
        r.frames.push(Frame::Cast {
            stack: id,
            stage: 0,
            record,
            selected: Span::default(),
        })?;
        self.set_running(r);
        if free {
            self.finish_cast(p, c)?;
        }
        Ok(())
    }
    fn finish_cast(&mut self, p: Player, c: Card) -> Result {
        let mut r = Running::default();
        // Capture triggered instances before SBA. Their sources may subsequently leave.
        let board = self.world().battlefield;
        for per in entries(&board) {
            let k = card_of(per);
            if (k == EIDOLON && mana_value(c) <= 3)
                || (k == SWIFT && per.base_controller == p && !creature(c))
            {
                let id = self.new_id()?;
                let capture = if k == EIDOLON {
                    self.capture_player(p)?
                } else {
                    RecordId(0)
                };
                r.triggers.push(PendingTrigger {
                    ids: IdRange {
                        first: id,
                        count: 1,
                    },
                    source: AbilitySource::Fixed {
                        source: per.ids.first,
                    },
                    controller: per.base_controller,
                    ability: Behavior(if k == EIDOLON {
                        EIDOLON_TRIGGER
                    } else {
                        PROWESS
                    }),
                    capture,
                })?;
            }
        }
        self.set_running(r);
        self.stabilize(p)
    }
    fn capture_player(&mut self, p: Player) -> Result<RecordId> {
        let id = RecordId(self.new_id()?.0);
        self.world_mut().captures.push(Capture {
            id,
            body: CaptureBody::Number { value: p.0 as i32 },
        })?;
        Ok(id)
    }
    fn capture_value(&self, id: RecordId) -> Option<i32> {
        entries(&self.world().captures).iter().find_map(|x| {
            if x.id == id {
                if let CaptureBody::Number { value } = x.body {
                    Some(value)
                } else {
                    None
                }
            } else {
                None
            }
        })
    }
    fn remove_permanent(&mut self, id: ObjectId) -> Result {
        let i = entries(&self.world().battlefield)
            .iter()
            .position(|x| x.ids.first == id)
            .ok_or(Error::Illegal("permanent absent"))?;
        let p = remove(&mut self.world_mut().battlefield, i);
        let c = card_of(&p);
        push_count(&mut self.world_mut().zones.graveyard[pid(owner_of(&p))][c.0 as usize])?;
        // Effects targeting this incarnation expire; stack targets deliberately retain the stale ID.
        let mut j = 0;
        while j < self.world().effects.len as usize {
            if matches!(self.world().effects.data[j].selection,Selection::One{subject:Subject::Object{id:x}} if x==id)
            {
                remove(&mut self.world_mut().effects, j);
            } else {
                j += 1
            }
        }
        Ok(())
    }
    fn activate_fetch(&mut self, p: Player, id: ObjectId) -> Result {
        let per = *self.permanent(id).ok_or(Error::Illegal("fetch absent"))?;
        require(
            per.base_controller == p
                && per.status & TAPPED == 0
                && matches!(card_of(&per), MESA | MIRE),
            "fetch unavailable",
        )?;
        self.pay_life(p, 1)?;
        self.remove_permanent(id)?;
        self.push_ability(p, FETCH, id, RecordId(0))?;
        self.stabilize(p)
    }
    fn activate_canyon(&mut self, p: Player, id: ObjectId) -> Result {
        let per = *self.permanent(id).ok_or(Error::Illegal("canyon absent"))?;
        require(
            per.base_controller == p && per.status & TAPPED == 0 && card_of(&per) == CANYON,
            "canyon unavailable",
        )?;
        // Activation announces the ability, then exposes its mana-payment opportunity.
        let sid = self.push_ability(p, DRAW, id, RecordId(0))?;
        let mut r = Running::default();
        r.choice = Choice::Mana {
            player: p,
            casting: RecordId(sid.0),
        };
        r.frames.push(Frame::Pay {
            stack: sid,
            stage: 1,
            cost: Behavior(DRAW),
            remaining: [0, 0, 0, 0, 0, 1],
        })?;
        self.set_running(r);
        Ok(())
    }
    fn push_ability(
        &mut self,
        p: Player,
        kind: u16,
        source: ObjectId,
        capture: RecordId,
    ) -> Result<ObjectId> {
        let id = self.new_id()?;
        let start = self.world().targets.len;
        self.world_mut().stack.push(StackRun {
            ids: IdRange {
                first: id,
                count: 1,
            },
            controller: p,
            body: StackBody::Ability {
                definition: Behavior(kind),
                source: AbilitySource::Fixed { source },
                capture,
            },
            targets: Span { start, len: 0 },
        })?;
        Ok(id)
    }
    fn suspend(&mut self, p: Player) -> Result {
        require(
            self.sorcery(p) && self.world().zones.hand[pid(p)][RIFT.0 as usize] > 0,
            "suspend timing/card",
        )?;
        // Suspend is a special action, not a spell. Its cost may be paid using mana abilities.
        let mut r = Running::default();
        r.choice = Choice::Mana {
            player: p,
            casting: RecordId(0),
        };
        r.frames.push(Frame::Pay {
            stack: ObjectId(0),
            stage: 2,
            cost: Behavior(SUSPEND_CAST),
            remaining: [0, 0, 0, 1, 0, 0],
        })?;
        self.set_running(r);
        Ok(())
    }
    fn complete_suspend(&mut self, p: Player) -> Result {
        self.world_mut().zones.hand[pid(p)][RIFT.0 as usize] -= 1;
        let id = self.new_id()?;
        let group = RecordId(self.new_id()?.0);
        let stamp = self.stamp()?;
        self.world_mut().zones.exile_groups.push(ExileGroup {
            id: group,
            ..Default::default()
        })?;
        self.world_mut().zones.tracked.push(TrackedCard {
            id,
            card: RIFT,
            owner: p,
            zone: OwnedZone::Exile { group },
        })?;
        self.world_mut().counters.push(Counter {
            subject: Subject::Object { id },
            kind: Symbol(1),
            amount: 1,
            timestamp: stamp,
        })?;
        self.set_running(Running::default());
        self.stabilize(p)
    }
    fn stabilize(&mut self, return_to: Player) -> Result {
        let mut r = self.running();
        let mut losers = r.failed_draw_mask;
        for p in 0..2 {
            if self.world().players[p].life <= 0 {
                losers |= 1 << p
            }
        }
        if losers != 0 {
            self.state = State::Finished {
                outcome: if losers == 3 {
                    mtg_state_design::Outcome::Draw
                } else {
                    mtg_state_design::Outcome::Won {
                        winner: Player(if losers == 1 { 1 } else { 0 }),
                    }
                },
            };
            return Ok(());
        }
        let mut i = 0;
        while i < self.world().battlefield.len as usize {
            let per = self.world().battlefield.data[i];
            let dead = self.power_toughness(per.ids.first).is_some_and(|(_, t)| {
                t <= 0 || (per.damage >= t && !self.has_ability(per.ids.first, INDESTRUCTIBLE))
            });
            if dead {
                self.remove_permanent(per.ids.first)?;
            } else {
                i += 1
            }
        }
        r.frames = Pool::default();
        r.frames.push(Frame::OrderTriggers {
            player: return_to,
            remaining: Span::default(),
        })?;
        self.set_running(r);
        self.drain_triggers(return_to)
    }
    fn drain_triggers(&mut self, return_to: Player) -> Result {
        let mut r = self.running();
        loop {
            if r.triggers.len == 0 {
                self.priority(return_to);
                return Ok(());
            }
            let active = self.progress().turn.active;
            let p = if entries(&r.triggers).iter().any(|x| x.controller == active) {
                active
            } else {
                other(active)
            };
            let n = entries(&r.triggers)
                .iter()
                .filter(|x| x.controller == p)
                .count();
            if n > 1 {
                r.choice = Choice::Order {
                    player: p,
                    objects: Span::default(),
                    chosen: Span::default(),
                };
                self.set_running(r);
                return Ok(());
            }
            let i = entries(&r.triggers)
                .iter()
                .position(|x| x.controller == p)
                .unwrap();
            let trigger = remove(&mut r.triggers, i);
            self.stack_trigger(trigger)?;
        }
    }
    fn stack_trigger(&mut self, t: PendingTrigger) -> Result {
        let start = self.world().targets.len;
        self.world_mut().stack.push(StackRun {
            ids: t.ids,
            controller: t.controller,
            body: StackBody::Ability {
                definition: t.ability,
                source: t.source,
                capture: t.capture,
            },
            targets: Span { start, len: 0 },
        })?;
        Ok(())
    }
    fn choice_input(&mut self, p: Player, a: Action, mut r: Running) -> Result {
        if !matches!(r.choice, Choice::Chance { .. }) {
            require(self.actor() == Some(p), "choice player")?;
        }
        match (r.choice, a) {
            (Choice::Mana { .. }, Action::Tap { object, color }) => self.tap(p, object, color),
            (Choice::Mana { .. }, Action::Pay { mana }) => match r.frames.data[0] {
                Frame::Cast { record, .. } => {
                    let top = self.world().stack.data[self.world().stack.len as usize - 1];
                    let c = if let StackBody::Spell {
                        origin: Origin::Card { card, .. },
                        ..
                    } = top.body
                    {
                        card
                    } else {
                        unreachable!()
                    };
                    let rec = entries(&self.world().cast_records)
                        .iter()
                        .find(|x| x.id == record)
                        .unwrap();
                    self.pay_mana(
                        p,
                        cost(c, matches!(rec.route, CastRoute::Alternative { .. })),
                        mana,
                    )?;
                    self.finish_cast(p, c)
                }
                Frame::Pay { stage: 1, .. } => {
                    self.pay_mana(
                        p,
                        Cost {
                            generic: 1,
                            ..Default::default()
                        },
                        mana,
                    )?;
                    let top = self.world().stack.data[self.world().stack.len as usize - 1];
                    if let StackBody::Ability {
                        source: AbilitySource::Fixed { source },
                        ..
                    } = top.body
                    {
                        require(
                            self.permanent(source)
                                .is_some_and(|x| x.status & TAPPED == 0),
                            "canyon tapped for its own cost",
                        )?;
                        self.remove_permanent(source)?;
                    }
                    self.set_running(Running::default());
                    self.stabilize(p)
                }
                Frame::Pay { stage: 2, .. } => {
                    self.pay_mana(
                        p,
                        Cost {
                            red: 1,
                            ..Default::default()
                        },
                        mana,
                    )?;
                    self.complete_suspend(p)
                }
                _ => Err(Error::Illegal("payment frame")),
            },
            (Choice::Order { .. }, Action::OrderTrigger { trigger }) => {
                let return_to = if let Frame::OrderTriggers { player, .. } = r.frames.data[0] {
                    player
                } else {
                    unreachable!()
                };
                let i = entries(&r.triggers)
                    .iter()
                    .position(|x| x.ids.first == trigger && x.controller == p)
                    .ok_or(Error::Illegal("trigger order"))?;
                let t = remove(&mut r.triggers, i);
                self.stack_trigger(t)?;
                self.set_running(r);
                self.drain_triggers(return_to)
            }
            (Choice::YesNo { .. }, Action::Shock { pay }) => {
                let e = r.entering.data[0];
                let c = if let Origin::Card { card, .. } = e.origin {
                    card
                } else {
                    unreachable!()
                };
                if pay {
                    self.pay_life(p, 2)?;
                }
                self.enter(c, p, !pay)?;
                if matches!(r.frames.data[0], Frame::TurnAction { stage: 2, .. }) {
                    self.request_shuffle(p)
                } else {
                    self.set_running(Running::default());
                    self.stabilize(p)
                }
            }
            (
                Choice::Select {
                    rule: Behavior(FETCH_CHOICE),
                    ..
                },
                Action::Search { card },
            ) => {
                if let Some(c) = card {
                    require(matches!(c, MOUNTAIN | FOUNDRY), "fetch land type")?;
                    let i = entries(&self.world().zones.library[pid(p)])
                        .iter()
                        .position(|x| *x == c)
                        .ok_or(Error::Illegal("search absent"))?;
                    remove(&mut self.world_mut().zones.library[pid(p)], i);
                    self.set_execution(Execution::Idle);
                    self.begin_entry(c, p, true)?;
                    if matches!(self.execution(), Execution::Running { .. }) {
                        return Ok(());
                    }
                }
                self.request_shuffle(p)
            }
            (
                Choice::Chance {
                    rule: Behavior(SHUFFLE_CHOICE),
                    ..
                },
                Action::Shuffle { order, len },
            ) => {
                let owner = if let Frame::Resolve { controller, .. } = r.frames.data[0] {
                    controller
                } else {
                    return Err(Error::Illegal("shuffle frame"));
                };
                Self::permute(&mut self.world_mut().zones.library[pid(owner)], order, len)?;
                self.finish_resolution()
            }
            (
                Choice::Select {
                    rule: Behavior(ATTACK_CHOICE),
                    ..
                },
                Action::Attack { objects, len },
            ) => self.declare_attackers(p, objects, len),
            (
                Choice::Select {
                    rule: Behavior(BLOCK_CHOICE),
                    ..
                },
                Action::Block { pairs, len },
            ) => self.declare_blockers(p, pairs, len),
            (
                Choice::Select {
                    rule: Behavior(DAMAGE_CHOICE),
                    ..
                },
                Action::Damage { assignments, len },
            ) => self.assign_damage(p, assignments, len),
            (
                Choice::Select {
                    rule: Behavior(SUSPEND_CHOICE),
                    ..
                },
                Action::DeclineSuspend,
            ) => {
                let top = self.world().stack.data[self.world().stack.len as usize - 1];
                let source = if let StackBody::Ability {
                    source: AbilitySource::Fixed { source },
                    ..
                } = top.body
                {
                    source
                } else {
                    unreachable!()
                };
                let i = entries(&self.world().zones.tracked)
                    .iter()
                    .position(|x| x.id == source)
                    .ok_or(Error::Illegal("suspended card gone"))?;
                let card = remove(&mut self.world_mut().zones.tracked, i);
                push_count(
                    &mut self.world_mut().zones.ordinary_exile[pid(card.owner)]
                        [card.card.0 as usize],
                )?;
                self.remove_exile_metadata(source);
                self.set_running(Running::default());
                self.finish_resolution()
            }
            (
                Choice::Select {
                    rule: Behavior(SUSPEND_CHOICE),
                    ..
                },
                Action::SuspendTarget { target },
            ) => {
                let top = self.world().stack.data[self.world().stack.len as usize - 1];
                let source = if let StackBody::Ability {
                    source: AbilitySource::Fixed { source },
                    ..
                } = top.body
                {
                    source
                } else {
                    unreachable!()
                };
                let i = entries(&self.world().zones.tracked)
                    .iter()
                    .position(|x| x.id == source)
                    .ok_or(Error::Illegal("suspended card gone"))?;
                self.check_targets(RIFT, [target, Subject::Absent], 0)?;
                remove(&mut self.world_mut().zones.tracked, i);
                self.remove_exile_metadata(source);
                self.pop_stack()?;
                self.set_execution(Execution::Idle);
                self.start_cast(p, RIFT, [target, Subject::Absent], 0, false, true)
            }
            (
                Choice::Select {
                    rule: Behavior(DISCARD_CHOICE),
                    ..
                },
                Action::Discard { card },
            ) => {
                require(
                    card.0 < 16 && self.world().zones.hand[pid(p)][card.0 as usize] > 0,
                    "discard card",
                )?;
                self.world_mut().zones.hand[pid(p)][card.0 as usize] -= 1;
                push_count(&mut self.world_mut().zones.graveyard[pid(p)][card.0 as usize])?;
                self.cleanup()
            }
            _ => Err(Error::Illegal("unexpected choice response")),
        }
    }
    fn request_shuffle(&mut self, p: Player) -> Result {
        let mut r = Running::default();
        r.choice = Choice::Chance {
            rule: Behavior(SHUFFLE_CHOICE),
            objects: Span::default(),
        };
        r.frames.push(Frame::Resolve {
            ability: Behavior(FETCH),
            stage: 1,
            source: ObjectId(0),
            controller: p,
            capture: RecordId(0),
            selected: Span::default(),
        })?;
        self.set_running(r);
        Ok(())
    }
    fn permute(lib: &mut Pool<Card, 60>, order: [u8; 60], len: u8) -> Result {
        require(len as u16 == lib.len, "shuffle length")?;
        let old = *lib;
        let mut seen = 0u64;
        for i in 0..len as usize {
            let j = order[i] as usize;
            require(
                j < len as usize && (seen & (1 << j)) == 0,
                "shuffle permutation",
            )?;
            seen |= 1 << j;
            lib.data[i] = old.data[j];
        }
        Ok(())
    }
    fn pass(&mut self, p: Player) -> Result {
        let previous = if let Execution::Priority { priority } = self.execution() {
            priority.consecutive_passes
        } else {
            0
        };
        if previous == 0 {
            self.set_execution(Execution::Priority {
                priority: Priority {
                    player: other(p),
                    consecutive_passes: 1,
                },
            });
            return Ok(());
        }
        if self.world().stack.len > 0 {
            self.resolve_top()
        } else {
            self.advance_step()
        }
    }
    fn targets(&self, s: StackRun) -> [Subject; 2] {
        let mut out = [Subject::Absent; 2];
        for i in 0..s.targets.len as usize {
            out[i] = self.world().targets.data[s.targets.start as usize + i].subject;
        }
        out
    }
    fn deal(&mut self, target: Subject, n: u32) -> Result {
        match target {
            Subject::Player { player } => self.lose(player, n),
            Subject::Object { id } => {
                if let Some(per) = self.permanent_mut(id) {
                    per.damage = per.damage.checked_add(n as i32).ok_or(Error::Overflow)?;
                }
                Ok(())
            }
            _ => Ok(()),
        }
    }
    fn effect(&mut self, id: ObjectId, body: EffectBody) -> Result {
        // Same-subject prowess pumps and repeated keyword grants are safely coalesced in this scope.
        let n = self.world().effects.len as usize;
        for i in 0..n {
            let e = self.world().effects.data[i];
            if matches!(e.selection,Selection::One{subject:Subject::Object{id:x}} if x==id) {
                match (e.body, body) {
                    (
                        EffectBody::Pump {
                            power: a,
                            toughness: b,
                        },
                        EffectBody::Pump {
                            power: c,
                            toughness: d,
                        },
                    ) => {
                        self.world_mut().effects.data[i].body = EffectBody::Pump {
                            power: a.checked_add(c).ok_or(Error::Overflow)?,
                            toughness: b.checked_add(d).ok_or(Error::Overflow)?,
                        };
                        return Ok(());
                    }
                    (a, b) if a == b => return Ok(()),
                    _ => {}
                }
            }
        }
        let eid = EffectId(self.new_id()?.0);
        let stamp = self.stamp()?;
        let turn = self.progress().turn.serial;
        self.world_mut().effects.push(Effect {
            id: eid,
            timestamp: stamp,
            selection: Selection::One {
                subject: Subject::Object { id },
            },
            expiry: Expiry::EndOfTurn { turn },
            body,
            ..Default::default()
        })?;
        Ok(())
    }
    fn resolve_top(&mut self) -> Result {
        let s = self.world().stack.data[self.world().stack.len as usize - 1];
        let p = s.controller;
        let ts = self.targets(s);
        self.set_running(Running::default());
        match s.body {
            StackBody::Spell {
                origin: Origin::Card { card: c, .. },
                cast,
                ..
            } => {
                let rec = *entries(&self.world().cast_records)
                    .iter()
                    .find(|x| x.id == cast)
                    .ok_or(Error::Illegal("cast record"))?;
                let valid = [
                    self.valid_target(
                        ts[0],
                        c == SPIKE || (c == CHARM && rec.modes == 0),
                        c == CHARM && rec.modes == 2,
                    ),
                    self.valid_target(ts[1], false, true),
                ];
                let fizzles = s.targets.len > 0 && !valid[0] && !valid[1];
                if !fizzles {
                    match c {
                        GUIDE | SWIFT | EIDOLON => {
                            self.enter(c, p, false)?;
                        }
                        BOLT | SPIKE | RIFT | SKEWER => {
                            self.deal(ts[0], 3)?;
                        }
                        HELIX => {
                            self.deal(ts[0], 3)?;
                            self.world_mut().players[pid(p)].life = self.world().players[pid(p)]
                                .life
                                .checked_add(3)
                                .ok_or(Error::Overflow)?;
                        }
                        BLAZE => {
                            let n = if self.world().turn_history.land_entered_mask & (1 << p.0) != 0
                            {
                                3
                            } else {
                                1
                            };
                            if valid[0] {
                                self.deal(ts[0], n)?;
                            }
                            if valid[1] {
                                self.deal(ts[1], n)?;
                            }
                        }
                        CHARM => match rec.modes {
                            0 => self.deal(ts[0], 4)?,
                            1 => {
                                let board = self.world().battlefield;
                                for per in entries(&board) {
                                    if per.base_controller == p {
                                        self.effect(
                                            per.ids.first,
                                            EffectBody::GrantAbility {
                                                ability: Behavior(INDESTRUCTIBLE),
                                            },
                                        )?;
                                    }
                                }
                            }
                            2 => {
                                if let Subject::Object { id } = ts[0] {
                                    self.effect(
                                        id,
                                        EffectBody::GrantAbility {
                                            ability: Behavior(DOUBLE),
                                        },
                                    )?;
                                }
                            }
                            _ => unreachable!(),
                        },
                        _ => return Err(Error::Illegal("unsupported spell")),
                    }
                }
                if !creature(c) || fizzles {
                    push_count(&mut self.world_mut().zones.graveyard[pid(p)][c.0 as usize])?;
                }
            }
            StackBody::Ability {
                definition: Behavior(kind),
                source: AbilitySource::Fixed { source },
                capture,
            } => match kind {
                PROWESS => {
                    if self.permanent(source).is_some() {
                        self.effect(
                            source,
                            EffectBody::Pump {
                                power: 1,
                                toughness: 1,
                            },
                        )?;
                    }
                }
                EIDOLON_TRIGGER => {
                    let target = Player(
                        self.capture_value(capture)
                            .ok_or(Error::Illegal("trigger capture"))?
                            as u8,
                    );
                    self.lose(target, 2)?;
                }
                GUIDE_TRIGGER => {
                    let target = Player(
                        self.capture_value(capture)
                            .ok_or(Error::Illegal("guide capture"))? as u8,
                    );
                    let lib = &mut self.world_mut().zones.library[pid(target)];
                    if lib.len > 0 && land(lib.data[lib.len as usize - 1]) {
                        let c = pop(lib).unwrap();
                        push_count(&mut self.world_mut().zones.hand[pid(target)][c.0 as usize])?;
                    }
                }
                FETCH => {
                    let mut r = Running::default();
                    r.choice = Choice::Select {
                        player: p,
                        rule: Behavior(FETCH_CHOICE),
                        min: 0,
                        max: 1,
                        selected: Span::default(),
                    };
                    self.set_running(r);
                    return Ok(());
                }
                DRAW => self.draw(p)?,
                TIME => {
                    if let Some(i) = entries(&self.world().counters)
                        .iter()
                        .position(|x| x.subject == Subject::Object { id: source } && x.amount > 0)
                    {
                        self.world_mut().counters.data[i].amount -= 1;
                        if self.world().counters.data[i].amount == 0 {
                            let mut r = self.running();
                            let id = self.new_id()?;
                            r.triggers.push(PendingTrigger {
                                ids: IdRange {
                                    first: id,
                                    count: 1,
                                },
                                source: AbilitySource::Fixed { source },
                                controller: p,
                                ability: Behavior(SUSPEND_CAST),
                                capture: RecordId(0),
                            })?;
                            self.set_running(r);
                        }
                    }
                }
                SUSPEND_CAST => {
                    if entries(&self.world().zones.tracked)
                        .iter()
                        .any(|x| x.id == source)
                    {
                        let mut r = Running::default();
                        r.choice = Choice::Select {
                            player: p,
                            rule: Behavior(SUSPEND_CHOICE),
                            min: 0,
                            max: 1,
                            selected: Span::default(),
                        };
                        self.set_running(r);
                        return Ok(());
                    }
                }
                _ => return Err(Error::Illegal("unsupported ability")),
            },
            _ => return Err(Error::Illegal("stack body")),
        }
        self.finish_resolution()
    }
    fn pop_stack(&mut self) -> Result {
        let s = pop(&mut self.world_mut().stack).ok_or(Error::Illegal("empty stack"))?;
        self.world_mut().targets.len = s.targets.start;
        match s.body {
            StackBody::Spell { cast, .. } => {
                if let Some(i) = entries(&self.world().cast_records)
                    .iter()
                    .position(|x| x.id == cast)
                {
                    remove(&mut self.world_mut().cast_records, i);
                }
            }
            StackBody::Ability { capture, .. } => {
                if let Some(i) = entries(&self.world().captures)
                    .iter()
                    .position(|x| x.id == capture)
                {
                    remove(&mut self.world_mut().captures, i);
                }
            }
            _ => {}
        }
        Ok(())
    }
    fn finish_resolution(&mut self) -> Result {
        self.pop_stack()?;
        self.stabilize(self.progress().turn.active)
    }
    fn remove_exile_metadata(&mut self, source: ObjectId) {
        let mut i = 0;
        while i < self.world().counters.len as usize {
            if self.world().counters.data[i].subject == (Subject::Object { id: source }) {
                remove(&mut self.world_mut().counters, i);
            } else {
                i += 1
            }
        }
        let mut i = 0;
        while i < self.world().zones.exile_groups.len as usize {
            let id = self.world().zones.exile_groups.data[i].id;
            if !entries(&self.world().zones.tracked)
                .iter()
                .any(|x| x.zone == OwnedZone::Exile { group: id })
            {
                remove(&mut self.world_mut().zones.exile_groups, i);
            } else {
                i += 1
            }
        }
    }
    fn draw(&mut self, p: Player) -> Result {
        if let Some(c) = pop(&mut self.world_mut().zones.library[pid(p)]) {
            push_count(&mut self.world_mut().zones.hand[pid(p)][c.0 as usize])?;
        } else {
            let mut r = self.running();
            r.failed_draw_mask |= 1 << p.0;
            self.set_running(r);
        }
        Ok(())
    }
    fn advance_step(&mut self) -> Result {
        for p in &mut self.world_mut().players {
            p.mana = [0; 6];
        }
        let active = self.progress().turn.active;
        self.set_running(Running::default());
        match self.progress().phase {
            Phase::Beginning {
                step: BeginningStep::Upkeep {},
            } => {
                if self.progress().turn.serial.0 == 1 {
                    self.progress_mut().phase = Phase::Main { postcombat: false };
                } else {
                    self.progress_mut().phase = Phase::Beginning {
                        step: BeginningStep::Draw {},
                    };
                    self.draw(active)?;
                }
                self.stabilize(active)
            }
            Phase::Beginning {
                step: BeginningStep::Draw {},
            } => {
                self.progress_mut().phase = Phase::Main { postcombat: false };
                self.stabilize(active)
            }
            Phase::Main { postcombat: false } => {
                let id = RecordId(self.new_id()?.0);
                self.progress_mut().phase = Phase::Combat {
                    combat: Combat {
                        header: CombatHeader {
                            id,
                            step: CombatStep::Begin,
                            first_damage_happened: false,
                        },
                        ..Default::default()
                    },
                };
                self.stabilize(active)
            }
            Phase::Combat { mut combat } => match combat.header.step {
                CombatStep::Begin => {
                    combat.header.step = CombatStep::Attackers {};
                    self.progress_mut().phase = Phase::Combat { combat };
                    self.select(active, ATTACK_CHOICE)
                }
                CombatStep::Attackers {} => {
                    if combat.attackers.len > 0 {
                        combat.header.step = CombatStep::Blockers {};
                        self.progress_mut().phase = Phase::Combat { combat };
                        self.select(other(active), BLOCK_CHOICE)
                    } else {
                        combat.header.step = CombatStep::End {};
                        self.progress_mut().phase = Phase::Combat { combat };
                        self.stabilize(active)
                    }
                }
                CombatStep::Blockers {} => {
                    let board = self.world().battlefield;
                    for per in entries(&board) {
                        if self.in_combat(per.ids.first, &combat)
                            && self.has_ability(per.ids.first, DOUBLE)
                        {
                            combat.first_strikers.push(per.ids)?;
                        }
                    }
                    if combat.first_strikers.len > 0 {
                        combat.header.step = CombatStep::FirstDamage {};
                        combat.header.first_damage_happened = true;
                    } else {
                        combat.header.step = CombatStep::NormalDamage {};
                    }
                    self.progress_mut().phase = Phase::Combat { combat };
                    self.damage_step()
                }
                CombatStep::FirstDamage {} => {
                    combat.header.step = CombatStep::NormalDamage {};
                    self.progress_mut().phase = Phase::Combat { combat };
                    self.damage_step()
                }
                CombatStep::NormalDamage {} => {
                    combat.header.step = CombatStep::End {};
                    self.progress_mut().phase = Phase::Combat { combat };
                    self.stabilize(active)
                }
                CombatStep::End {} => {
                    self.progress_mut().phase = Phase::Main { postcombat: true };
                    self.stabilize(active)
                }
            },
            Phase::Main { postcombat: true } => {
                self.progress_mut().phase = Phase::Ending { cleanup: false };
                self.stabilize(active)
            }
            Phase::Ending { cleanup: false } => {
                self.progress_mut().phase = Phase::Ending { cleanup: true };
                self.cleanup()
            }
            Phase::Ending { cleanup: true } => self.cleanup(),
            _ => Err(Error::Illegal("phase transition")),
        }
    }
    fn select(&mut self, p: Player, rule: u16) -> Result {
        let mut r = Running::default();
        r.choice = Choice::Select {
            player: p,
            rule: Behavior(rule),
            min: 0,
            max: 24,
            selected: Span::default(),
        };
        self.set_running(r);
        Ok(())
    }
    fn combat(&self) -> Combat {
        if let Phase::Combat { combat } = self.progress().phase {
            combat
        } else {
            panic!("not combat")
        }
    }
    fn in_combat(&self, id: ObjectId, c: &Combat) -> bool {
        entries(&c.attackers)
            .iter()
            .any(|a| a.attackers.contains(id))
            || entries(&c.blocks).iter().any(|b| b.blocker == id)
    }
    fn declare_attackers(&mut self, p: Player, objects: [ObjectId; 24], len: u8) -> Result {
        require(len <= 24, "attack capacity")?;
        let mut combat = self.combat();
        let mut r = Running::default();
        for (i, &id) in objects[..len as usize].iter().enumerate() {
            require(!objects[..i].contains(&id), "duplicate attacker")?;
            let per = *self
                .permanent(id)
                .ok_or(Error::Illegal("attacker absent"))?;
            let c = card_of(&per);
            require(
                per.base_controller == p
                    && creature(c)
                    && per.status & TAPPED == 0
                    && (per.status & CONTROLLED != 0 || c == GUIDE || c == SWIFT),
                "illegal attacker",
            )?;
            self.permanent_mut(id).unwrap().status |= TAPPED;
            combat.attackers.push(Attacker {
                attackers: per.ids,
                defender: Subject::Player { player: other(p) },
                was_blocked: false,
            })?;
            if c == GUIDE {
                let capture = self.capture_player(other(p))?;
                let tid = self.new_id()?;
                r.triggers.push(PendingTrigger {
                    ids: IdRange {
                        first: tid,
                        count: 1,
                    },
                    source: AbilitySource::Fixed { source: id },
                    controller: p,
                    ability: Behavior(GUIDE_TRIGGER),
                    capture,
                })?;
            }
        }
        self.progress_mut().phase = Phase::Combat { combat };
        self.set_running(r);
        self.stabilize(p)
    }
    fn declare_blockers(
        &mut self,
        p: Player,
        pairs: [(ObjectId, ObjectId); 24],
        len: u8,
    ) -> Result {
        require(len <= 24, "block capacity")?;
        let mut combat = self.combat();
        for (i, &(blocker, attacker)) in pairs[..len as usize].iter().enumerate() {
            require(
                !pairs[..i].iter().any(|x| x.0 == blocker),
                "blocker used twice",
            )?;
            let per = *self
                .permanent(blocker)
                .ok_or(Error::Illegal("blocker absent"))?;
            require(
                per.base_controller == p && creature(card_of(&per)) && per.status & TAPPED == 0,
                "illegal blocker",
            )?;
            require(self.permanent(attacker).is_some(), "attacker left")?;
            let a = combat.attackers.data[..combat.attackers.len as usize]
                .iter_mut()
                .find(|x| x.attackers.first == attacker)
                .ok_or(Error::Illegal("not attacker"))?;
            a.was_blocked = true;
            combat.blocks.push(Block { blocker, attacker })?;
        }
        self.progress_mut().phase = Phase::Combat { combat };
        self.set_running(Running::default());
        self.stabilize(self.progress().turn.active)
    }
    fn damage_eligible(&self, id: ObjectId, c: &Combat) -> bool {
        let initial = entries(&c.first_strikers).iter().any(|r| r.contains(id));
        if matches!(c.header.step, CombatStep::FirstDamage {}) {
            initial
        } else {
            !initial || self.has_ability(id, DOUBLE)
        }
    }
    fn damage_step(&mut self) -> Result {
        // No damage-order rule in current CR: controller may divide among blockers freely.
        // A decision is needed only for a creature with multiple living recipients.
        let combat = self.combat();
        let active = self.progress().turn.active;
        for a in entries(&combat.attackers) {
            let id = a.attackers.first;
            if self.permanent(id).is_some()
                && self.damage_eligible(id, &combat)
                && entries(&combat.blocks)
                    .iter()
                    .filter(|b| b.attacker == id && self.permanent(b.blocker).is_some())
                    .count()
                    > 1
            {
                return self.select(active, DAMAGE_CHOICE);
            }
        }
        self.assign_damage(active, [(ObjectId(0), Subject::Absent, 0); 48], 0)
    }
    fn assign_damage(
        &mut self,
        p: Player,
        assignments: [(ObjectId, Subject, u32); 48],
        len: u8,
    ) -> Result {
        require(len <= 48, "damage capacity")?;
        let combat = self.combat();
        let mut events = Pool::<DamageAssignment, 64>::default();
        let mut used = [false; 48];
        for a in entries(&combat.attackers) {
            let id = a.attackers.first;
            if self.permanent(id).is_none() || !self.damage_eligible(id, &combat) {
                continue;
            }
            let power = self.power_toughness(id).unwrap().0.max(0) as u32;
            let mut blockers = [ObjectId(0); 24];
            let mut n = 0;
            for b in entries(&combat.blocks) {
                if b.attacker == id && self.permanent(b.blocker).is_some() {
                    blockers[n] = b.blocker;
                    n += 1;
                }
            }
            if n > 1 {
                let mut total = 0u32;
                for (j, &(source, target, amount)) in assignments[..len as usize].iter().enumerate()
                {
                    if source == id {
                        require(
                            matches!(target,Subject::Object{id:t} if blockers[..n].contains(&t)),
                            "damage recipient",
                        )?;
                        used[j] = true;
                        total = total.checked_add(amount).ok_or(Error::Overflow)?;
                        events.push(DamageAssignment {
                            sources: IdRange {
                                first: id,
                                count: 1,
                            },
                            target,
                            amount_each: amount,
                        })?;
                    }
                }
                require(total == power, "assign full combat damage")?;
            } else if n == 1 {
                events.push(DamageAssignment {
                    sources: a.attackers,
                    target: Subject::Object { id: blockers[0] },
                    amount_each: power,
                })?;
            } else if !a.was_blocked {
                events.push(DamageAssignment {
                    sources: a.attackers,
                    target: a.defender,
                    amount_each: power,
                })?;
            }
        }
        require(
            used[..len as usize].iter().all(|x| *x),
            "extraneous assignment",
        )?;
        for b in entries(&combat.blocks) {
            if self.permanent(b.blocker).is_some()
                && self.permanent(b.attacker).is_some()
                && self.damage_eligible(b.blocker, &combat)
            {
                let power = self.power_toughness(b.blocker).unwrap().0.max(0) as u32;
                events.push(DamageAssignment {
                    sources: IdRange {
                        first: b.blocker,
                        count: 1,
                    },
                    target: Subject::Object { id: b.attacker },
                    amount_each: power,
                })?;
            }
        }
        // All assignments are based on pre-damage state; SBA runs after all damage.
        for e in entries(&events) {
            self.deal(e.target, e.amount_each)?;
        }
        self.set_running(Running::default());
        self.stabilize(p)
    }
    fn cleanup(&mut self) -> Result {
        let p = self.progress().turn.active;
        let hand: u32 = self.world().zones.hand[pid(p)]
            .iter()
            .map(|x| *x as u32)
            .sum();
        if hand > 7 {
            return self.select(p, DISCARD_CHOICE);
        }
        self.world_mut().effects = Pool::default();
        let live = self.world().battlefield.len as usize;
        for per in &mut self.world_mut().battlefield.data[..live] {
            per.damage = 0;
        }
        // In this card scope cleanup cannot create triggers or toughness-zero creatures.
        let next = self.progress().turn.next_normal;
        let serial = self
            .progress()
            .turn
            .serial
            .0
            .checked_add(1)
            .ok_or(Error::Overflow)?;
        self.world_mut().turn_history = BurnHistory::default();
        for pl in &mut self.world_mut().players {
            pl.mana = [0; 6];
        }
        for per in &mut self.world_mut().battlefield.data[..live] {
            if per.base_controller == next {
                per.status = (per.status & !TAPPED) | CONTROLLED;
            }
        }
        let instance = RecordId(self.new_id()?.0);
        self.progress_mut().turn = TurnHeader {
            serial: TurnId(serial),
            instance,
            active: next,
            next_normal: other(next),
            decision_controller: next,
        };
        self.progress_mut().phase = Phase::Beginning {
            step: BeginningStep::Upkeep {},
        };
        let mut r = Running::default();
        let tracked = self.world().zones.tracked;
        for card in entries(&tracked) {
            if card.owner == next
                && entries(&self.world().counters)
                    .iter()
                    .any(|c| c.subject == Subject::Object { id: card.id } && c.amount > 0)
            {
                let id = self.new_id()?;
                r.triggers.push(PendingTrigger {
                    ids: IdRange {
                        first: id,
                        count: 1,
                    },
                    source: AbilitySource::Fixed { source: card.id },
                    controller: next,
                    ability: Behavior(TIME),
                    capture: RecordId(0),
                })?;
            }
        }
        self.set_running(r);
        self.stabilize(next)
    }
    fn mulligan_input(&mut self, p: Player, a: Action) -> Result {
        let (mut setup, mut process, execution) = if let State::Mulligan {
            setup,
            process,
            execution,
        } = self.state
        {
            (setup, process, execution)
        } else {
            unreachable!()
        };
        match (execution, a) {
            (Execution::Idle, Action::Mulligan { take }) => {
                require(
                    p == process.next && process.kept_mask & (1 << p.0) == 0,
                    "mulligan player",
                )?;
                require(!take || process.taken[pid(p)] < 7, "mulligan limit")?;
                process.declared_mask |= 1 << p.0;
                if take {
                    process.mulligan_mask |= 1 << p.0;
                } else {
                    process.kept_mask |= 1 << p.0;
                }
                let pending = 3 & !process.kept_mask & !process.declared_mask;
                if pending != 0 {
                    process.next = if pending & (1 << process.starting.0) != 0 {
                        process.starting
                    } else {
                        other(process.starting)
                    };
                    self.state = State::Mulligan {
                        setup,
                        process,
                        execution: Execution::Idle,
                    };
                    return Ok(());
                }
            }
            (Execution::Running { mut running }, Action::Shuffle { order, len })
                if matches!(running.choice, Choice::Chance { .. }) =>
            {
                let who = if let Frame::MulliganAction { player, .. } = running.frames.data[0] {
                    player
                } else {
                    unreachable!()
                };
                Self::permute(&mut setup.zones.library[pid(who)], order, len)?;
                for _ in 0..7 {
                    let c = pop(&mut setup.zones.library[pid(who)])
                        .ok_or(Error::Illegal("mulligan library"))?;
                    push_count(&mut setup.zones.hand[pid(who)][c.0 as usize])?;
                }
                running.choice = Choice::Select {
                    player: who,
                    rule: Behavior(MULLIGAN_BOTTOM),
                    min: process.taken[pid(who)] as u32,
                    max: process.taken[pid(who)] as u32,
                    selected: Span::default(),
                };
                running.frames.data[0] = Frame::MulliganAction {
                    player: who,
                    stage: process.taken[pid(who)],
                    bottom: Span::default(),
                };
                self.state = State::Mulligan {
                    setup,
                    process,
                    execution: Execution::Running { running },
                };
                return Ok(());
            }
            (Execution::Running { mut running }, Action::Bottom { card }) => {
                let (who, n) =
                    if let Frame::MulliganAction { player, stage, .. } = running.frames.data[0] {
                        (player, stage)
                    } else {
                        return Err(Error::Illegal("bottom frame"));
                    };
                require(
                    p == who
                        && matches!(
                            running.choice,
                            Choice::Select {
                                rule: Behavior(MULLIGAN_BOTTOM),
                                ..
                            }
                        )
                        && card.0 < 16
                        && setup.zones.hand[pid(p)][card.0 as usize] > 0,
                    "bottom choice",
                )?;
                setup.zones.hand[pid(p)][card.0 as usize] -= 1;
                let lib = &mut setup.zones.library[pid(p)];
                require(lib.len < 60, "library capacity")?;
                lib.data.copy_within(0..lib.len as usize, 1);
                lib.data[0] = card;
                lib.len += 1;
                if n > 1 {
                    running.frames.data[0] = Frame::MulliganAction {
                        player: who,
                        stage: n - 1,
                        bottom: Span::default(),
                    };
                    self.state = State::Mulligan {
                        setup,
                        process,
                        execution: Execution::Running { running },
                    };
                    return Ok(());
                }
                process.mulligan_mask &= !(1 << p.0);
            }
            _ => return Err(Error::Illegal("mulligan input")),
        }
        if process.mulligan_mask != 0 {
            let who = if process.mulligan_mask & (1 << process.starting.0) != 0 {
                process.starting
            } else {
                other(process.starting)
            };
            process.taken[pid(who)] += 1;
            for c in 0..16 {
                for _ in 0..setup.zones.hand[pid(who)][c] {
                    setup.zones.library[pid(who)].push(Card(c as u16))?;
                }
                setup.zones.hand[pid(who)][c] = 0;
            }
            let mut running = Running::default();
            running.choice = Choice::Chance {
                rule: Behavior(SHUFFLE_CHOICE),
                objects: Span::default(),
            };
            running.frames.push(Frame::MulliganAction {
                player: who,
                stage: 0,
                bottom: Span::default(),
            })?;
            self.state = State::Mulligan {
                setup,
                process,
                execution: Execution::Running { running },
            };
        } else if process.kept_mask == 3 {
            let mut world = World::default();
            world.zones = setup.zones;
            world.identity = setup.identity;
            world.players = [PlayerCore {
                life: 20,
                mana: [0; 6],
            }; 2];
            let active = process.starting;
            self.state = State::Playing {
                world,
                progress: Progress {
                    turn: TurnHeader {
                        serial: TurnId(1),
                        instance: RecordId(0),
                        active,
                        next_normal: other(active),
                        decision_controller: active,
                    },
                    phase: Phase::Beginning {
                        step: BeginningStep::Upkeep {},
                    },
                },
                execution: Execution::Priority {
                    priority: Priority {
                        player: active,
                        consecutive_passes: 0,
                    },
                },
            };
        } else {
            process.declared_mask = 0;
            process.next = if process.kept_mask & (1 << process.starting.0) == 0 {
                process.starting
            } else {
                other(process.starting)
            };
            self.state = State::Mulligan {
                setup,
                process,
                execution: Execution::Idle,
            };
        }
        Ok(())
    }
}
/// Test harness helpers; not part of the rules interface.
#[doc(hidden)]
pub mod verification;

/// Legal interaction and observer knowledge, shared by both bot approaches.
pub mod harness;

/// Terminal-outcome search bot; retains interaction re-exports for compatibility.
pub mod terminal_bot;
