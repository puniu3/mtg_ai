use super::*;

#[derive(Clone, Debug)]
struct DamageGroup {
    source: ObjectId,
    targets: Vec<ObjectId>,
    power: u32,
    ways: u128,
}
/// Indexed lazy enumeration: no attack/block/damage candidate cap.
#[derive(Clone, Debug)]
pub struct ActionSpace {
    kind: Kind,
    len: u128,
}
#[derive(Clone, Debug)]
enum Kind {
    Small(Vec<Action>),
    Attack(Vec<ObjectId>),
    Block {
        blockers: Vec<ObjectId>,
        attackers: Vec<ObjectId>,
    },
    Damage(Vec<DamageGroup>),
}
fn choose(n: u32, k: u32) -> u128 {
    let k = k.min(n - k);
    let mut x = 1u128;
    for i in 0..k {
        x = x
            .checked_mul((n - i) as u128)
            .expect("composition count overflow")
            / (i + 1) as u128;
    }
    x
}
fn ways(power: u32, n: usize) -> u128 {
    if n == 0 {
        u128::from(power == 0)
    } else {
        choose(power + n as u32 - 1, n as u32 - 1)
    }
}
impl ActionSpace {
    pub(crate) fn from_macro_actions(v: Vec<Action>) -> Self {
        Self {
            len: v.len() as u128,
            kind: Kind::Small(v),
        }
    }
    pub(crate) fn small(&self) -> Option<&[Action]> {
        if let Kind::Small(v) = &self.kind {
            Some(v)
        } else {
            None
        }
    }
    pub(super) fn without_taps(self) -> Self {
        let Kind::Small(mut v) = self.kind else {
            return self;
        };
        v.retain(|a| !matches!(a, Action::Tap { .. }));
        Self {
            len: v.len() as u128,
            kind: Kind::Small(v),
        }
    }
    pub fn len(&self) -> u128 {
        self.len
    }
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }
    pub fn get(&self, mut index: u128) -> Action {
        assert!(index < self.len);
        match &self.kind {
            Kind::Small(v) => v[index as usize],
            Kind::Attack(ids) => {
                let mut objects = [ObjectId(0); 24];
                let mut len = 0;
                for (i, id) in ids.iter().enumerate() {
                    if index & (1 << i) != 0 {
                        objects[len] = *id;
                        len += 1;
                    }
                }
                Action::Attack {
                    objects,
                    len: len as u8,
                }
            }
            Kind::Block {
                blockers,
                attackers,
            } => {
                let mut pairs = [(ObjectId(0), ObjectId(0)); 24];
                let mut len = 0;
                let radix = attackers.len() as u128 + 1;
                for &b in blockers {
                    let digit = index % radix;
                    index /= radix;
                    if digit > 0 {
                        pairs[len] = (b, attackers[digit as usize - 1]);
                        len += 1;
                    }
                }
                Action::Block {
                    pairs,
                    len: len as u8,
                }
            }
            Kind::Damage(groups) => {
                let mut assignments = [(ObjectId(0), Subject::Absent, 0); 48];
                let mut len = 0;
                for g in groups {
                    let mut rank = index % g.ways;
                    index /= g.ways;
                    let mut remaining = g.power;
                    for (i, &id) in g.targets.iter().enumerate() {
                        let left = g.targets.len() - i - 1;
                        let amount = if left == 0 {
                            remaining
                        } else {
                            let mut found = 0;
                            for a in 0..=remaining {
                                let count = ways(remaining - a, left);
                                if rank < count {
                                    found = a;
                                    break;
                                }
                                rank -= count;
                            }
                            found
                        };
                        assignments[len] = (g.source, Subject::Object { id }, amount);
                        len += 1;
                        remaining -= amount;
                    }
                }
                Action::Damage {
                    assignments,
                    len: len as u8,
                }
            }
        }
    }
    /// Compact human editor contract; never enumerate exponential combat spaces.
    pub fn editor(&self, observation: &Observation) -> serde_json::Value {
        use serde_json::json;
        match &self.kind {
            Kind::Small(v) => json!({"kind":"choices", "choices": v.iter().enumerate().map(|(i,a)|
                json!({"index":i.to_string(),"action":observation.action_view(*a)})).collect::<Vec<_>>()}),
            Kind::Attack(ids) => {
                json!({"kind":"attack", "attackers":ids.iter().map(|x|x.0).collect::<Vec<_>>()})
            }
            Kind::Block {
                blockers,
                attackers,
            } => {
                json!({"kind":"block", "blockers":blockers.iter().map(|x|x.0).collect::<Vec<_>>(),"attackers":attackers.iter().map(|x|x.0).collect::<Vec<_>>()})
            }
            Kind::Damage(groups) => {
                json!({"kind":"damage", "groups":groups.iter().map(|g|json!({"source":g.source.0,"targets":g.targets.iter().map(|x|x.0).collect::<Vec<_>>(),"power":g.power})).collect::<Vec<_>>()})
            }
        }
    }
    /// Rank structured combat input in the same lazy legal space used by search.
    pub fn editor_index(&self, input: &serde_json::Value) -> std::result::Result<u128, String> {
        let invalid = || "選択が現在の合法手と一致しません。".to_string();
        let id = |v: &serde_json::Value| -> std::result::Result<ObjectId, String> {
            let n = v.as_u64().ok_or_else(invalid)?;
            Ok(ObjectId(u32::try_from(n).map_err(|_| invalid())?))
        };
        let index = match &self.kind {
            Kind::Small(_) => input["index"]
                .as_str()
                .ok_or_else(invalid)?
                .parse::<u128>()
                .map_err(|_| invalid())?,
            Kind::Attack(ids) => {
                let mut index = 0;
                for v in input["attackers"].as_array().ok_or_else(invalid)? {
                    let i = ids
                        .iter()
                        .position(|x| Some(*x) == id(v).ok())
                        .ok_or_else(invalid)?;
                    if index & (1 << i) != 0 {
                        return Err(invalid());
                    }
                    index |= 1 << i;
                }
                index
            }
            Kind::Block {
                blockers,
                attackers,
            } => {
                let mut digits = vec![0u128; blockers.len()];
                for pair in input["blocks"].as_array().ok_or_else(invalid)? {
                    let b = blockers
                        .iter()
                        .position(|x| Some(*x) == id(&pair[0]).ok())
                        .ok_or_else(invalid)?;
                    let a = attackers
                        .iter()
                        .position(|x| Some(*x) == id(&pair[1]).ok())
                        .ok_or_else(invalid)?;
                    if digits[b] != 0 {
                        return Err(invalid());
                    }
                    digits[b] = a as u128 + 1;
                }
                digits
                    .iter()
                    .rev()
                    .fold(0, |n, d| n * (attackers.len() as u128 + 1) + d)
            }
            Kind::Damage(groups) => {
                let values = input["damage"].as_array().ok_or_else(invalid)?;
                if values.len() != groups.len() {
                    return Err(invalid());
                }
                let mut index = 0;
                let mut multiplier = 1;
                for (g, values) in groups.iter().zip(values) {
                    let values = values.as_array().ok_or_else(invalid)?;
                    if values.len() != g.targets.len() {
                        return Err(invalid());
                    }
                    let mut remaining = g.power;
                    let mut rank = 0;
                    for (i, v) in values.iter().enumerate() {
                        let n = u32::try_from(v.as_u64().ok_or_else(invalid)?)
                            .map_err(|_| invalid())?;
                        if n > remaining {
                            return Err(invalid());
                        }
                        let left = values.len() - i - 1;
                        if left == 0 && n != remaining {
                            return Err(invalid());
                        }
                        for a in 0..n {
                            rank += ways(remaining - a, left);
                        }
                        remaining -= n;
                    }
                    index += rank * multiplier;
                    multiplier *= g.ways;
                }
                index
            }
        };
        if index >= self.len {
            return Err(invalid());
        }
        Ok(index)
    }
    pub(super) fn build(e: &Engine) -> Result<Self> {
        Self::build_mode(e, false)
    }
    pub(crate) fn build_auto(e: &Engine) -> Result<Self> {
        Self::build_mode(e, true)
    }
    fn build_mode(e: &Engine, auto: bool) -> Result<Self> {
        let p = e
            .actor()
            .ok_or(Error::Illegal("actions require decision"))?;
        // Public, unresolved opponent targets define the sole friendly-fire exception.
        // Inspect the whole stack: an unrelated object may be above the threat.
        let opponent_targets: Vec<Subject> = if auto && matches!(e.state, State::Playing { .. }) {
            entries(&e.world().stack).iter()
                .filter(|s| s.controller != p)
                .flat_map(|s| entries(&e.world().targets)
                    [s.targets.start as usize..(s.targets.start + s.targets.len) as usize]
                    .iter().map(|t| t.subject))
                .collect()
        } else { Vec::new() };
        // Auto never spends a fetch solely to fail the search. A search already
        // resolving with no targets still needs its one forced completion.
        let fetch_target_available = matches!(e.state, State::Playing { .. }) && entries(&e.world().zones.library[pid(p)])
            .iter().any(|card| matches!(*card, MOUNTAIN | FOUNDRY));
        let mut out = Vec::new();
        let mut add = |a: Action| -> Result {
            // Auto policy excludes friendly damage targets, not beneficial modes
            // such as Boros Charm's double strike. Primitive legality is unchanged.
            if auto {
                let friendly = |target: Subject| match target {
                    Subject::Player { player } => player == p,
                    Subject::Object { id } => e.permanent(id).is_some_and(|b| b.base_controller == p)
                        && !opponent_targets.contains(&target),
                    Subject::Absent => false,
                };
                let excluded = match a {
                    Action::Fetch { .. } => !fetch_target_available,
                    Action::Search { card: None } => fetch_target_available,
                    Action::Cast { card, targets, mode, .. }
                        if matches!(card, BOLT | SPIKE | BLAZE | HELIX | RIFT | SKEWER)
                            || (card == CHARM && mode == 0) => targets.into_iter().any(friendly),
                    Action::SuspendTarget { target } => friendly(target),
                    _ => false,
                };
                if excluded { return Ok(()); }
            }
            let mut copy = *e;
            match copy.apply(p, a) {
                Ok(()) => {
                    if !out.contains(&a) {
                        out.push(a);
                    }
                    Ok(())
                }
                Err(Error::Illegal(_)) => Ok(()),
                Err(x) => Err(x),
            }
        };
        if let State::Mulligan {
            setup, execution, ..
        } = e.state
        {
            if matches!(execution, Execution::Idle) {
                add(Action::Mulligan { take: false })?;
                add(Action::Mulligan { take: true })?;
            } else {
                for c in 0..16 {
                    if setup.zones.hand[pid(p)][c] > 0 {
                        add(Action::Bottom {
                            card: Card(c as u16),
                        })?;
                    }
                }
            }
        } else if let Execution::Running { running } = e.execution() {
            match running.choice {
                Choice::Mana { .. } => {
                    let (cost, excluded) = pending_cost(e)?;
                    let mana = e.world().players[pid(p)].mana;
                    // Costs are at most three mana, enumerate every payment vector.
                    fn payments(
                        i: usize,
                        left: u32,
                        available: &[u32; 6],
                        v: &mut [u32; 6],
                        emit: &mut impl FnMut(Action) -> Result,
                    ) -> Result {
                        if i == 6 {
                            if left == 0 {
                                emit(Action::Pay { mana: *v })?;
                            }
                            return Ok(());
                        }
                        for n in 0..=left.min(available[i]) {
                            v[i] = n;
                            payments(i + 1, left - n, available, v, emit)?;
                        }
                        Ok(())
                    }
                    payments(
                        0,
                        cost.red + cost.white + cost.generic,
                        &mana,
                        &mut [0; 6],
                        &mut add,
                    )?;
                    for per in entries(&e.world().battlefield) {
                        if Some(per.ids.first) == excluded {
                            continue;
                        }
                        for color in [WHITE, RED] {
                            let a = Action::Tap {
                                object: per.ids.first,
                                color: color as u8,
                            };
                            let mut copy = *e;
                            if copy.apply(p, a).is_ok() && affordable(&copy, p, cost, excluded) {
                                add(a)?;
                            }
                        }
                    }
                }
                Choice::Order { .. } => {
                    for t in entries(&running.triggers) {
                        if t.controller == p {
                            add(Action::OrderTrigger {
                                trigger: t.ids.first,
                            })?;
                        }
                    }
                }
                Choice::YesNo { .. } => {
                    add(Action::Shock { pay: false })?;
                    add(Action::Shock { pay: true })?;
                }
                Choice::Select {
                    rule: Behavior(ATTACK_CHOICE),
                    ..
                } => {
                    let ids = entries(&e.world().battlefield)
                        .iter()
                        .filter(|b| {
                            b.base_controller == p
                                && creature(card_of(b))
                                && b.status & TAPPED == 0
                                && (b.status & CONTROLLED != 0
                                    || matches!(card_of(b), GUIDE | SWIFT))
                        })
                        .map(|b| b.ids.first)
                        .collect::<Vec<_>>();
                    return Ok(Self {
                        len: 1 << ids.len(),
                        kind: Kind::Attack(ids),
                    });
                }
                Choice::Select {
                    rule: Behavior(BLOCK_CHOICE),
                    ..
                } => {
                    let blockers = entries(&e.world().battlefield)
                        .iter()
                        .filter(|b| {
                            b.base_controller == p && creature(card_of(b)) && b.status & TAPPED == 0
                        })
                        .map(|b| b.ids.first)
                        .collect::<Vec<_>>();
                    let attackers = entries(&e.combat().attackers)
                        .iter()
                        .map(|a| a.attackers.first)
                        .filter(|id| e.permanent(*id).is_some())
                        .collect::<Vec<_>>();
                    let len = (attackers.len() as u128 + 1)
                        .checked_pow(blockers.len() as u32)
                        .ok_or(Error::Overflow)?;
                    return Ok(Self {
                        len,
                        kind: Kind::Block {
                            blockers,
                            attackers,
                        },
                    });
                }
                Choice::Select {
                    rule: Behavior(DAMAGE_CHOICE),
                    ..
                } => {
                    let combat = e.combat();
                    let mut groups = Vec::new();
                    let mut len = 1u128;
                    for a in entries(&combat.attackers) {
                        let id = a.attackers.first;
                        if e.permanent(id).is_none() || !e.damage_eligible(id, &combat) {
                            continue;
                        }
                        let targets = entries(&combat.blocks)
                            .iter()
                            .filter(|b| b.attacker == id && e.permanent(b.blocker).is_some())
                            .map(|b| b.blocker)
                            .collect::<Vec<_>>();
                        if targets.len() > 1 {
                            let power = e.power_toughness(id).unwrap().0.max(0) as u32;
                            let count = ways(power, targets.len());
                            len = len.checked_mul(count).ok_or(Error::Overflow)?;
                            groups.push(DamageGroup {
                                source: id,
                                targets,
                                power,
                                ways: count,
                            });
                        }
                    }
                    return Ok(Self {
                        len,
                        kind: Kind::Damage(groups),
                    });
                }
                Choice::Select {
                    rule: Behavior(FETCH_CHOICE),
                    ..
                } => {
                    add(Action::Search { card: None })?;
                    for c in [MOUNTAIN, FOUNDRY] {
                        add(Action::Search { card: Some(c) })?;
                    }
                }
                Choice::Select {
                    rule: Behavior(DISCARD_CHOICE),
                    ..
                } => {
                    for c in 0..16 {
                        if e.world().zones.hand[pid(p)][c] > 0 {
                            add(Action::Discard {
                                card: Card(c as u16),
                            })?;
                        }
                    }
                }
                Choice::Select {
                    rule: Behavior(SUSPEND_CHOICE),
                    ..
                } => {
                    add(Action::DeclineSuspend)?;
                    for target in targets(e) {
                        add(Action::SuspendTarget { target })?;
                    }
                }
                _ => return Err(Error::Illegal("unhandled decision")),
            }
        } else {
            add(Action::Pass)?;
            // Allocate the unchanged target order once, only if a spell needs it.
            let mut cached_targets = None;
            for c in 0..16 {
                let card = Card(c as u16);
                if e.world().zones.hand[pid(p)][c] == 0 {
                    continue;
                }
                if auto && !instant(card) && !e.sorcery(p) {
                    continue;
                }
                if land(card) {
                    add(Action::Land { card })?;
                    continue;
                }
                for spectacle in [false, true] {
                    if spectacle
                        && (card != SKEWER
                            || (auto
                                && e.world().turn_history.life_lost_mask & (1 << other(p).0) == 0))
                    {
                        continue;
                    }
                    if !affordable(e, p, cost(card, spectacle), None) {
                        continue;
                    }
                    if creature(card) {
                        add(Action::Cast {
                            card,
                            targets: [Subject::Absent; 2],
                            mode: 0,
                            spectacle,
                        })?;
                        continue;
                    }
                    if card == CHARM {
                        add(Action::Cast {
                            card,
                            targets: [Subject::Absent; 2],
                            mode: 1,
                            spectacle,
                        })?;
                    }
                    let spell_targets = cached_targets.get_or_insert_with(|| targets(e));
                    for &target in spell_targets.iter() {
                        if auto && card == SPIKE && !matches!(target, Subject::Player { .. }) {
                            continue;
                        }
                        if card == BLAZE {
                            if auto && !matches!(target, Subject::Player { .. }) {
                                continue;
                            }
                            for &target2 in spell_targets.iter() {
                                if auto {
                                    // Match Blaze's player/creature controller; friendly damage is filtered by add.
                                    let (Subject::Player { player }, Subject::Object { id }) =
                                        (target, target2)
                                    else {
                                        continue;
                                    };
                                    if !e
                                        .permanent(id)
                                        .is_some_and(|per| per.base_controller == player)
                                    {
                                        continue;
                                    }
                                }
                                add(Action::Cast {
                                    card,
                                    targets: [target, target2],
                                    mode: 0,
                                    spectacle,
                                })?;
                            }
                        } else {
                            for &mode in if card == CHARM { &[0, 2][..] } else { &[0][..] } {
                                if auto
                                    && card == CHARM
                                    && (mode == 0) != matches!(target, Subject::Player { .. })
                                {
                                    continue;
                                }
                                add(Action::Cast {
                                    card,
                                    targets: [target, Subject::Absent],
                                    mode,
                                    spectacle,
                                })?;
                            }
                        }
                    }
                }
            }
            if (!auto || (e.sorcery(p) && e.world().zones.hand[pid(p)][RIFT.0 as usize] > 0))
                && affordable(
                    e,
                    p,
                    Cost {
                        red: 1,
                        ..Default::default()
                    },
                    None,
                )
            {
                add(Action::Suspend)?;
            }
            for per in entries(&e.world().battlefield) {
                let object = per.ids.first;
                let card = card_of(per);
                if auto
                    && (per.base_controller != p
                        || per.status & TAPPED != 0
                        || !matches!(card, MESA | MIRE | CANYON))
                {
                    continue;
                }
                if !auto || matches!(card, MESA | MIRE) {
                    add(Action::Fetch { object })?;
                }
                if (!auto || card == CANYON)
                    && affordable(
                        e,
                        p,
                        Cost {
                            generic: 1,
                            ..Default::default()
                        },
                        Some(object),
                    )
                {
                    add(Action::Canyon { object })?;
                }
                for color in if auto { &[][..] } else { &[WHITE, RED][..] } {
                    add(Action::Tap {
                        object,
                        color: *color as u8,
                    })?;
                }
            }
        }
        if out.is_empty() {
            return Err(Error::Illegal("empty legal action space"));
        }
        Ok(Self {
            len: out.len() as u128,
            kind: Kind::Small(out),
        })
    }
}
fn targets(e: &Engine) -> Vec<Subject> {
    let mut out = vec![
        Subject::Player { player: Player(0) },
        Subject::Player { player: Player(1) },
    ];
    out.extend(
        entries(&e.world().battlefield)
            .iter()
            .filter(|b| creature(card_of(b)))
            .map(|b| Subject::Object { id: b.ids.first }),
    );
    out
}
pub(super) fn pending_cost(e: &Engine) -> Result<(Cost, Option<ObjectId>)> {
    let Execution::Running { running } = e.execution() else {
        return Err(Error::Illegal("not paying"));
    };
    match running.frames.data[0] {
        Frame::Cast { record, .. } => {
            let rec = entries(&e.world().cast_records)
                .iter()
                .find(|x| x.id == record)
                .ok_or(Error::Illegal("missing cast"))?;
            let top = entries(&e.world().stack).last().unwrap();
            let StackBody::Spell {
                origin: Origin::Card { card, .. },
                ..
            } = top.body
            else {
                return Err(Error::Illegal("missing spell"));
            };
            Ok((
                cost(card, matches!(rec.route, CastRoute::Alternative { .. })),
                None,
            ))
        }
        Frame::Pay { stage: 1, .. } => {
            let top = entries(&e.world().stack).last().unwrap();
            let StackBody::Ability {
                source: AbilitySource::Fixed { source },
                ..
            } = top.body
            else {
                return Err(Error::Illegal("missing canyon"));
            };
            Ok((
                Cost {
                    generic: 1,
                    ..Default::default()
                },
                Some(source),
            ))
        }
        Frame::Pay { stage: 2, .. } => Ok((
            Cost {
                red: 1,
                ..Default::default()
            },
            None,
        )),
        _ => Err(Error::Illegal("unknown payment")),
    }
}
/// Exact feasibility for this card pool's red-only and red/white sources.
/// This only excludes payments that cannot complete; it does not choose a plan.
pub(crate) fn affordable(e: &Engine, p: Player, cost: Cost, excluded: Option<ObjectId>) -> bool {
    let w = e.world();
    let mana = w.players[pid(p)].mana;
    let r = cost.red.saturating_sub(mana[RED]);
    let wh = cost.white.saturating_sub(mana[WHITE]);
    let surplus = mana.iter().sum::<u32>() - mana[RED].min(cost.red) - mana[WHITE].min(cost.white);
    let generic = cost.generic.saturating_sub(surplus);
    let mut red = 0;
    let mut dual = 0;
    let mut pain = 0;
    for per in entries(&w.battlefield) {
        if per.base_controller != p || per.status & TAPPED != 0 || Some(per.ids.first) == excluded {
            continue;
        }
        match card_of(per) {
            MOUNTAIN => red += 1,
            FOUNDRY | VANTAGE => dual += 1,
            CANYON => pain += 1,
            _ => (),
        }
    }
    dual += pain.min(w.players[pid(p)].life.max(0) as u32);
    dual >= wh && red + dual >= r + wh + generic
}
