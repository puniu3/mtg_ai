//! Shared human/bot auto control. Compound choices still execute primitive rules.
use super::no_suicide::SearchChoices;
use super::*;
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Mode {
    pub own_top_pass: bool,
    pub fetch_macro: bool,
    pub land_macro: bool,
    pub suspend_progress_pass: bool,
}
impl Mode {
    pub fn named(name: &str) -> Self {
        match name {
            "pass" => Self {
                own_top_pass: true,
                fetch_macro: false,
                land_macro: false,
                suspend_progress_pass: true,
            },
            "fetch" => Self {
                own_top_pass: false,
                fetch_macro: true,
                land_macro: false,
                suspend_progress_pass: false,
            },
            "legacy" => Self {
                own_top_pass: true, fetch_macro: true, land_macro: false, suspend_progress_pass: true,
            },
            "both" => Self {
                own_top_pass: true,
                fetch_macro: true,
                land_macro: true,
                suspend_progress_pass: true,
            },
            _ => Self::default(),
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct FetchPlan {
    pub source: ObjectId,
    pub card: Option<Card>,
    pub pay: bool,
    pub searched: bool,
}
#[derive(Clone)]
pub struct Control {
    pub owner: Player,
    pub mode: Mode,
    pub plan: Option<FetchPlan>,
    pub auto_passes: u64,
    pub macro_steps: u64,
    pub cancelled: u64,
}
impl Control {
    pub fn new(owner: Player, mode: Mode, plan: Option<FetchPlan>) -> Self {
        Self {
            owner,
            mode,
            plan,
            auto_passes: 0,
            macro_steps: 0,
            cancelled: 0,
        }
    }
    pub fn step(&self, e: &Engine) -> Result<AutoStep> {
        if let Some(actor) = e.actor() {
            if actor == self.owner {
                // Skip both suspend bookkeeping triggers, never the resulting spell.
                // Target selection is a Running choice, so it remains a decision.
                if self.mode.suspend_progress_pass
                    && matches!(e.execution(), Execution::Priority { .. })
                    && entries(&e.world().stack).last().is_some_and(|s| {
                        matches!(
                            s.body,
                            StackBody::Ability {
                                definition: Behavior(TIME | SUSPEND_CAST),
                                ..
                            }
                        )
                    })
                {
                    return Ok(AutoStep::Automatic {
                        actor,
                        action: Action::Pass,
                        reason: "suspend progression",
                    });
                }
                if self.mode.own_top_pass
                    && matches!(e.execution(), Execution::Priority { .. })
                    && entries(&e.world().stack)
                        .last()
                        .is_some_and(|s| s.controller == actor)
                {
                    return Ok(AutoStep::Automatic {
                        actor,
                        action: Action::Pass,
                        reason: "own top stack object",
                    });
                }
                if let Some(plan) = self.plan {
                    if let Some(action) = continuation(e, actor, plan)? {
                        return Ok(AutoStep::Automatic {
                            actor,
                            action,
                            reason: "committed fetch plan",
                        });
                    }
                }
            }
        }
        match auto_step(e)? {
            AutoStep::Decision { actor, actions }
                if actor == self.owner && (self.mode.fetch_macro || self.mode.land_macro) =>
            {
                Ok(AutoStep::Decision {
                    actor,
                    actions: expand_mode(e, actor, actions, self.mode)?,
                })
            }
            other => Ok(other),
        }
    }
    /// The actual runner calls this with the public classification, not hidden opponent actions.
    pub fn observe_action(
        &mut self,
        actor: Player,
        own: Option<Action>,
        pass: bool,
        chance: bool,
        reveal: bool,
        information: bool,
    ) {
        if chance || reveal || information || (actor != self.owner && !pass) {
            self.cancelled += u64::from(self.plan.is_some());
            self.plan = None;
            return;
        }
        if actor != self.owner {
            return;
        }
        match own {
            Some(Action::FetchPlan { object, card, pay }) => {
                self.plan = Some(FetchPlan {
                    source: object,
                    card,
                    pay,
                    searched: false,
                })
            }
            Some(Action::SearchPlan { card, pay }) => {
                // Source installed before applying this action, from the current fetch ability.
                if let Some(p) = &mut self.plan {
                    p.card = card;
                    p.pay = pay;
                    p.searched = true;
                }
                if card != Some(FOUNDRY) {
                    self.plan = None;
                }
            }
            Some(Action::Search { card }) => {
                if let Some(p) = &mut self.plan {
                    if p.card != card {
                        self.plan = None;
                    } else if card == Some(FOUNDRY) {
                        p.searched = true;
                    } else {
                        self.plan = None;
                    }
                }
            }
            Some(Action::Shock { .. }) => self.plan = None,
            Some(Action::Pass) | None => (),
            _ => {
                self.cancelled += u64::from(self.plan.is_some());
                self.plan = None;
            }
        }
    }
    pub fn prepare(&mut self, e: &Engine, actor: Player, action: Action) -> Result {
        if let Action::SearchPlan { card, pay } = action {
            let source = fetch_source(e, actor)
                .ok_or(Error::Illegal("search plan outside fetch resolution"))?;
            self.plan = Some(FetchPlan {
                source,
                card,
                pay,
                searched: false,
            });
        }
        Ok(())
    }
    pub fn apply(&mut self, p: &mut Position, actor: Player, action: Action) -> Result {
        self.prepare(&p.engine, actor, action)?;
        let old = zones(&p.engine).map(|z| z.library.each_ref().map(|l| l.len));
        let events = p.apply(actor, executed(action))?;
        let info = match (old.as_ref(), p.engine.finished()) {
            (Some(before), None) => {
                Some(*before) != zones(&p.engine).map(|z| z.library.each_ref().map(|l| l.len))
                    && !matches!(action, Action::Search { .. } | Action::SearchPlan { .. })
            }
            _ => false,
        };
        self.observe_action(
            actor,
            (actor == self.owner).then_some(action),
            action == Action::Pass,
            matches!(action, Action::Shuffle { .. }),
            !events.is_empty(),
            info,
        );
        Ok(())
    }
}
fn executed(a: Action) -> Action {
    if matches!(a, Action::LandPlan { .. }) { a } else { primitive(a) }
}
pub fn primitive(a: Action) -> Action {
    match a {
        Action::LandPlan { card, .. } => Action::Land { card },
        Action::FetchPlan { object, .. } => Action::Fetch { object },
        Action::SearchPlan { card, .. } => Action::Search { card },
        _ => a,
    }
}
fn fetch_source(e: &Engine, actor: Player) -> Option<ObjectId> {
    match entries(&e.world().stack).last()? {
        StackRun {
            controller,
            body:
                StackBody::Ability {
                    definition: Behavior(FETCH),
                    source: AbilitySource::Fixed { source },
                    ..
                },
            ..
        } if *controller == actor => Some(*source),
        _ => None,
    }
}
fn continuation(e: &Engine, actor: Player, p: FetchPlan) -> Result<Option<Action>> {
    if fetch_source(e, actor) != Some(p.source) {
        return Ok(None);
    }
    let action = match e.execution() {
        Execution::Running { running }
            if !p.searched
                && matches!(
                    running.choice,
                    Choice::Select {
                        rule: Behavior(FETCH_CHOICE),
                        ..
                    }
                ) =>
        {
            Action::Search { card: p.card }
        }
        Execution::Running { running }
            if p.searched
                && p.card == Some(FOUNDRY)
                && matches!(running.choice, Choice::YesNo { .. }) =>
        {
            Action::Shock { pay: p.pay }
        }
        _ => return Ok(None),
    };
    let mut copy = *e;
    if copy.apply(actor, action).is_err() {
        return Ok(None);
    }
    Ok(Some(action))
}
/// Advance a COPY through passes only, stopping at the fetch search choice.
/// Used solely to enumerate/score intended land choices; never advances the real opponent.
fn search_state(e: &Engine, actor: Player, a: Action) -> Result<Engine> {
    let mut e = *e;
    if matches!(a, Action::Fetch { .. }) {
        e.apply(actor, a)?;
    }
    for _ in 0..64 {
        if e.finished().is_some() {
            return Err(Error::Illegal("fetch ends game"));
        }
        if let Execution::Running { running } = e.execution() {
            if matches!(
                running.choice,
                Choice::Select {
                    rule: Behavior(FETCH_CHOICE),
                    ..
                }
            ) {
                return Ok(e);
            }
        }
        let p = e.actor().ok_or(Error::Illegal("fetch forecast actor"))?;
        match auto_step(&e)? {
            AutoStep::Automatic { actor, action, .. } => e.apply(actor, action)?,
            AutoStep::Decision { .. } if matches!(e.execution(), Execution::Priority { .. }) => {
                e.apply(p, Action::Pass)?
            }
            _ => return Err(Error::Illegal("unexpected fetch forecast boundary")),
        }
    }
    Err(Error::Illegal("fetch forecast watchdog"))
}
pub fn expand(e: &Engine, actor: Player, base: ActionSpace) -> Result<ActionSpace> {
    expand_mode(e, actor, base, Mode::named("both"))
}
fn expand_mode(e: &Engine, actor: Player, base: ActionSpace, mode: Mode) -> Result<ActionSpace> {
    let Some(actions) = base.small() else {
        return Ok(base);
    };
    if !actions
        .iter()
        .any(|a| (mode.fetch_macro && matches!(a, Action::Fetch { .. } | Action::Search { .. })) || (mode.land_macro && matches!(a, Action::Land { card: FOUNDRY })))
    {
        return Ok(base);
    }
    let mut out = Vec::new();
    for &a in actions {
        match a {
            Action::Land { card: FOUNDRY } if mode.land_macro => {
                out.push(Action::LandPlan { card: FOUNDRY, pay: false });
                if e.world().players[pid(actor)].life > 2 {
                    out.push(Action::LandPlan { card: FOUNDRY, pay: true });
                }
            }
            Action::Fetch { object } if mode.fetch_macro => {
                // Preserve the same cost-suicide exclusion as baseline; don't forecast a terminal state.
                if e.world().players[pid(actor)].life <= 1 {
                    continue;
                }
                let state = search_state(e, actor, a)?;
                let space = ActionSpace::build_auto(&state)?;
                for i in 0..space.len() {
                    if let Action::Search { card } = space.get(i) {
                        out.push(Action::FetchPlan {
                            object,
                            card,
                            pay: false,
                        });
                        if card == Some(FOUNDRY) && state.world().players[pid(actor)].life > 2 {
                            out.push(Action::FetchPlan {
                                object,
                                card,
                                pay: true,
                            });
                        }
                    }
                }
            }
            Action::Search { card } if mode.fetch_macro => {
                out.push(Action::SearchPlan { card, pay: false });
                if card == Some(FOUNDRY) && e.world().players[pid(actor)].life > 2 {
                    out.push(Action::SearchPlan { card, pay: true });
                }
            }
            _ => out.push(a),
        }
    }
    Ok(ActionSpace::from_macro_actions(out))
}
fn conditional(e: &Engine, actor: Player, a: Action) -> Result<f64> {
    if let Action::LandPlan { card, pay } = a {
        let mut next = *e;
        next.apply(actor, Action::Land { card })?;
        let space = ActionSpace::build_auto(&next)?;
        let choices = SearchChoices::new(&next, actor, &space)?;
        let priors = super::rollout_policy::prior(&next, actor, &space, &choices)
            .ok_or(Error::Illegal("land conditional prior"))?;
        return (0..choices.len()).find(|&i| space.get(choices.index(i)) == Action::Shock { pay })
            .map(|i| priors[i as usize]).ok_or(Error::Illegal("land payment unavailable"));
    }
    let (card, pay) = match a {
        Action::FetchPlan { card, pay, .. } | Action::SearchPlan { card, pay } => (card, pay),
        _ => return Ok(1.),
    };
    let state = search_state(e, actor, primitive(a))?;
    let mut mass = 1.;
    if matches!(a, Action::FetchPlan { .. }) {
        let space = ActionSpace::build_auto(&state)?;
        let choices = SearchChoices::new(&state, actor, &space)?;
        let priors = super::rollout_policy::prior(&state, actor, &space, &choices)
            .ok_or(Error::Illegal("fetch conditional prior"))?;
        mass = (0..choices.len())
            .find(|&i| space.get(choices.index(i)) == Action::Search { card })
            .map(|i| priors[i as usize])
            .ok_or(Error::Illegal("fetch card unavailable"))?;
    }
    if card == Some(FOUNDRY) {
        let mut shock = state;
        shock.apply(actor, Action::Search { card })?;
        let space = ActionSpace::build_auto(&shock)?;
        let choices = SearchChoices::new(&shock, actor, &space)?;
        let priors = super::rollout_policy::prior(&shock, actor, &space, &choices)
            .ok_or(Error::Illegal("shock conditional prior"))?;
        mass *= (0..choices.len())
            .find(|&i| space.get(choices.index(i)) == Action::Shock { pay })
            .map(|i| priors[i as usize])
            .ok_or(Error::Illegal("shock payment unavailable"))?;
    }
    Ok(mass)
}
pub(super) fn prior(
    e: &Engine,
    actor: Player,
    space: &ActionSpace,
    choices: &SearchChoices,
) -> Option<Vec<f64>> {
    if choices.len() > 64 {
        return None;
    }
    let acts: Vec<_> = (0..choices.len())
        .map(|i| space.get(choices.index(i)))
        .collect();
    if !acts
        .iter()
        .any(|a| matches!(a, Action::FetchPlan { .. } | Action::SearchPlan { .. } | Action::LandPlan { .. }))
    {
        return super::rollout_policy::prior(e, actor, space, choices);
    }
    let mut bases = Vec::new();
    for &a in &acts {
        let p = primitive(a);
        if !bases.contains(&p) {
            bases.push(p);
        }
    }
    let base = ActionSpace::from_macro_actions(bases.clone());
    let bc = SearchChoices::new(e, actor, &base).ok()?;
    let bp = super::rollout_policy::prior(e, actor, &base, &bc)?;
    let mut out = Vec::new();
    for a in acts {
        let i = (0..bc.len()).find(|&i| base.get(bc.index(i)) == primitive(a))?;
        out.push(bp[i as usize] * conditional(e, actor, a).ok()?);
    }
    let sum: f64 = out.iter().sum();
    for p in &mut out {
        *p /= sum;
    }
    Some(out)
}
pub(super) fn choose(
    e: &Engine,
    actor: Player,
    space: &ActionSpace,
    choices: &SearchChoices,
    rng: &mut Random,
) -> u128 {
    if choices.len() > 64 {
        return super::rollout_policy::choose(e, actor, space, choices, rng);
    }
    let acts: Vec<_> = (0..choices.len())
        .map(|i| space.get(choices.index(i)))
        .collect();
    if !acts
        .iter()
        .any(|a| matches!(a, Action::FetchPlan { .. } | Action::SearchPlan { .. } | Action::LandPlan { .. }))
    {
        return super::rollout_policy::choose(e, actor, space, choices, rng);
    }
    // Preserve the baseline rollout's primitive choice, then choose search and shock using
    // the SAME rollout policy on the projected intermediate states. No uniform fetch bias.
    let mut bases = Vec::new();
    for &a in &acts {
        let p = primitive(a);
        if !bases.contains(&p) {
            bases.push(p);
        }
    }
    let base = ActionSpace::from_macro_actions(bases);
    let bc = SearchChoices::new(e, actor, &base).expect("base choices");
    let selected = base.get(super::rollout_policy::choose(e, actor, &base, &bc, rng));
    if let Action::Land { card: FOUNDRY } = selected {
        if acts.iter().any(|a| matches!(a, Action::LandPlan { .. })) {
            let mut next = *e;
            next.apply(actor, selected).unwrap();
            let space = ActionSpace::build_auto(&next).unwrap();
            let choices_next = SearchChoices::new(&next, actor, &space).unwrap();
            let Action::Shock { pay } = space.get(super::rollout_policy::choose(&next, actor, &space, &choices_next, rng)) else { unreachable!() };
            return choices.index(acts.iter().position(|a| *a == Action::LandPlan { card: FOUNDRY, pay }).unwrap() as u128);
        }
    }
    if !matches!(selected, Action::Fetch { .. } | Action::Search { .. }) {
        return choices.index(acts.iter().position(|&a| a == selected).unwrap() as u128);
    }
    let mut state = search_state(e, actor, selected).expect("validated macro forecast");
    let card = if let Action::Search { card } = selected {
        card
    } else {
        let a = ActionSpace::build_auto(&state).unwrap();
        let c = SearchChoices::new(&state, actor, &a).unwrap();
        let Action::Search { card } =
            a.get(super::rollout_policy::choose(&state, actor, &a, &c, rng))
        else {
            unreachable!()
        };
        card
    };
    let pay = if card == Some(FOUNDRY) {
        state.apply(actor, Action::Search { card }).unwrap();
        let a = ActionSpace::build_auto(&state).unwrap();
        let c = SearchChoices::new(&state, actor, &a).unwrap();
        let Action::Shock { pay } =
            a.get(super::rollout_policy::choose(&state, actor, &a, &c, rng))
        else {
            unreachable!()
        };
        pay
    } else {
        false
    };
    let i = acts
        .iter()
        .position(|a| {
            primitive(*a) == selected
                && match a {
                    Action::FetchPlan {
                        card: c, pay: p, ..
                    }
                    | Action::SearchPlan { card: c, pay: p } => *c == card && *p == pay,
                    _ => false,
                }
        })
        .unwrap();
    choices.index(i as u128)
}

#[cfg(test)]
#[path = "macro_tests.rs"]
mod tests;

/// Trusted runner tracks both private intents. A search starts with only its viewer's intent.
#[derive(Clone)]
pub struct Controls {
    pub players: [Control; 2],
}
impl Controls {
    pub fn new(owner: Player, mode: Mode, plan: Option<FetchPlan>) -> Self {
        let mut players = [
            Control::new(Player(0), mode, None),
            Control::new(Player(1), mode, None),
        ];
        players[owner.0 as usize].plan = plan;
        Self { players }
    }
    pub fn plan(&self, actor: Player) -> Option<FetchPlan> {
        self.players[actor.0 as usize].plan
    }
    pub fn step(&self, e: &Engine) -> Result<AutoStep> {
        self.players[e.actor().unwrap_or(Player(0)).0 as usize].step(e)
    }
    pub fn actions(&self, e: &Engine) -> Result<ActionSpace> {
        match self.step(e)? {
            AutoStep::Decision { actions, .. } => Ok(actions),
            _ => Err(Error::Illegal("not a decision")),
        }
    }
    pub fn apply(
        &mut self,
        p: &mut Position,
        actor: Player,
        action: Action,
    ) -> Result<Vec<PublicEvent>> {
        // Prepare on copies so a rejected primitive cannot corrupt a pending intent.
        let mut next = self.clone();
        next.players[actor.0 as usize].prepare(&p.engine, actor, action)?;
        let old = zones(&p.engine).map(|z| z.library.each_ref().map(|l| l.len));
        let events = p.apply(actor, executed(action))?;
        let information = p.engine.finished().is_none()
            && old != zones(&p.engine).map(|z| z.library.each_ref().map(|l| l.len))
            && !matches!(action, Action::Search { .. } | Action::SearchPlan { .. });
        for c in &mut next.players {
            c.observe_action(
                actor,
                (actor == c.owner).then_some(action),
                action == Action::Pass,
                matches!(action, Action::Shuffle { .. }),
                !events.is_empty(),
                information,
            );
        }
        *self = next;
        Ok(events)
    }
}
