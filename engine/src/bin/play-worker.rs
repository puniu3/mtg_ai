//! Private persistent worker for the local human GUI. Stdout is observer-safe JSONL.
use burn_engine::{
    harness::*,
    terminal_bot::{
        macro_control::{Controls, Mode, primitive},
        macro_search::{Budget, Config, SearchSession, ponder_candidate_mode},
    },
    *,
};
use serde_json::{Value as V, json};
use std::{
    collections::HashMap,
    env,
    fs::{self, OpenOptions},
    io::{self, BufRead, Write},
    os::unix::fs::OpenOptionsExt,
    path::PathBuf,
    sync::mpsc,
    time::{Duration, Instant},
};
type Res<T> = std::result::Result<T, String>;
fn err(e: impl std::fmt::Debug) -> String {
    format!("{e:?}")
}
struct Game {
    position: Position,
    controls: Controls,
    chance: Random,
    seed: u64,
    sequence: u64,
    starting: u8,
    path: PathBuf,
    budget: f64,
    budget_steps: Option<u64>,
    log: Vec<V>,
    last_view: V,
    outcome: Option<V>,
    requests: HashMap<String, V>,
    ponder: bool,
    visible_until: Instant,
    fault: Option<String>,
    candidates: Vec<SearchSession>,
    active: Option<SearchSession>,
    cursor: usize,
    forecast_attempts: u64,
    forecast_sequence: u64,
    reused: f64,
    reused_steps: u64,
    last_search: V,
    trace: bool,
    timeline: Vec<V>,
}
impl Game {
    fn open(path: PathBuf, seed: u64, starting: u8, ms: u64) -> Res<Self> {
        Self::open_budget(path, seed, starting, ms, None)
    }
    fn open_budget(
        path: PathBuf,
        seed: u64,
        starting: u8,
        ms: u64,
        steps: Option<u64>,
    ) -> Res<Self> {
        if steps == Some(0) {
            return Err("step budget must be positive".into());
        }
        if !path.exists() {
            let mut file = OpenOptions::new()
                .write(true)
                .create_new(true)
                .mode(0o600)
                .open(&path)
                .map_err(err)?;
            writeln!(
                file,
                "{}",
                json!({"schema":3,"auto_policy":6,"seed":seed,"starting":starting,"budget_ms":ms,"budget_steps":steps})
            )
            .map_err(err)?;
            file.sync_all().map_err(err)?;
        }
        let mut bytes = fs::read(&path).map_err(err)?;
        // Only an incomplete final append is recoverable; never skip a corrupt full record.
        if !bytes.ends_with(b"\n") {
            let end = bytes
                .iter()
                .rposition(|&b| b == b'\n')
                .ok_or("broken header")?
                + 1;
            OpenOptions::new()
                .write(true)
                .open(&path)
                .map_err(err)?
                .set_len(end as u64)
                .map_err(err)?;
            bytes.truncate(end);
        }
        let content = String::from_utf8(bytes).map_err(err)?;
        let mut rows = content.lines();
        let h: V = serde_json::from_str(rows.next().ok_or("missing header")?).map_err(err)?;
        if h["schema"] != 2 && h["schema"] != 3 {
            return Err("This game requires its original pinned worker.".into());
        }
        let seed = h["seed"].as_u64().ok_or("seed")?;
        let starting = h["starting"]
            .as_u64()
            .filter(|s| *s < 2)
            .ok_or("starting")? as u8;
        let mut rng = Random(seed);
        let position = Position::new(
            Engine::new([deck(&mut rng), deck(&mut rng)], Player(starting)).map_err(err)?,
        );
        let mut g = Self {
            last_view: position.observe(Player(0)).agent_view(),
            controls: Controls::new(Player(0), Mode::named(if h["schema"] == 3 { "both" } else { "legacy" }), None),
            position,
            chance: Random(seed ^ 0x706c6179),
            seed,
            sequence: 0,
            starting,
            path,
            budget: h["budget_ms"].as_u64().ok_or("budget")? as f64 / 1000.,
            budget_steps: h["budget_steps"].as_u64(),
            log: vec![],
            outcome: None,
            requests: HashMap::new(),
            ponder: true,
            visible_until: Instant::now(),
            fault: None,
            candidates: vec![],
            active: None,
            cursor: 0,
            forecast_attempts: 0,
            forecast_sequence: 0,
            reused: 0.,
            reused_steps: 0,
            last_search: V::Null,
            trace: env::args().any(|a| a == "--trace"),
            timeline: vec![],
        };
        for line in rows {
            let row: V = serde_json::from_str(line).map_err(err)?;
            if row["sequence"].as_u64() != Some(g.sequence) {
                return Err("sequence mismatch".into());
            }
            if row["kind"] == "ponder" {
                g.ponder = row["enabled"].as_bool().ok_or("ponder setting")?;
                g.sequence += 1;
            } else if row["kind"] == "concede" {
                g.outcome = Some(json!({"winner":1,"reason":"concede"}));
                g.sequence += 1;
            } else {
                let (actor, a) = g.recorded_action(&row)?;
                g.apply(actor, a)?;
            }
            if let Some(id) = row["request_id"].as_str() {
                g.requests.insert(id.into(), row["request"].clone());
            }
            if !row["search"].is_null() {
                g.last_search = row["search"].clone();
            }
        }
        Ok(g)
    }
    fn recorded_action(&mut self, row: &V) -> Res<(Player, Action)> {
        let (actor, a) = match row["kind"].as_str() {
            Some("chance") => (
                Player(0),
                chance_action(&self.position.engine, &mut self.chance).map_err(err)?,
            ),
            Some("auto") => match self.controls.step(&self.position.engine).map_err(err)? {
                AutoStep::Automatic { actor, action, .. } => (actor, action),
                _ => return Err("auto replay boundary".into()),
            },
            Some("human" | "bot") => {
                let actor = self.position.engine.actor().ok_or("actor")?;
                let actions = self.controls.actions(&self.position.engine).map_err(err)?;
                let i = row["index"]
                    .as_str()
                    .ok_or("index")?
                    .parse::<u128>()
                    .map_err(err)?;
                if i >= actions.len() {
                    return Err("index out of bounds".into());
                }
                (actor, actions.get(i))
            }
            _ => return Err("unknown row".into()),
        };
        if row["action"] != action_text(a) {
            return Err("replay differs".into());
        }
        Ok((actor, a))
    }
    fn append(&self, row: &V) -> Res<()> {
        let mut f = OpenOptions::new()
            .append(true)
            .open(&self.path)
            .map_err(err)?;
        let before = f.metadata().map_err(err)?.len();
        let mut bytes = serde_json::to_vec(row).map_err(err)?;
        bytes.push(b'\n');
        if let Err(e) = f.write_all(&bytes).and_then(|_| f.sync_data()) {
            let _ = f.set_len(before);
            return Err(err(e));
        }
        Ok(())
    }
    fn apply(&mut self, actor: Player, a: Action) -> Res<()> {
        let before = self.position.observe(Player(0)).agent_view();
        let visible = if actor == Player(1) && matches!(a, Action::Bottom { .. }) {
            json!({"kind":"bottom","card":"private"})
        } else {
            self.position
                .observe(Player(0))
                .action_view(if actor == Player(1) && !matches!(a, Action::LandPlan { .. }) { primitive(a) } else { a })
        };
        let cancelled_before = self.controls.players[0].cancelled;
        let events = self
            .controls
            .apply(&mut self.position, actor, a)
            .map_err(err)?;
        if self.controls.players[0].cancelled > cancelled_before {
            self.log
                .push(json!({"sequence":self.sequence+1,"plan_cancelled":true}));
        }
        self.position.audit().map_err(err)?;
        self.sequence += 1;
        if !matches!(
            a,
            Action::Pass
                | Action::Shuffle { .. }
                | Action::Pay { .. }
                | Action::Tap { .. }
                | Action::OrderTrigger { .. }
        ) {
            self.log
                .push(json!({"sequence":self.sequence,"actor":actor.0,"action":visible}));
        }
        for event in &events {
            self.log
                .push(json!({"sequence":self.sequence,"event":event.agent_view()}));
        }
        if let Some(outcome) = self.position.engine.finished() {
            self.outcome = Some(match outcome {
                Outcome::Won { winner } => json!({"winner":winner.0,"reason":"game"}),
                Outcome::Conceded { loser } => json!({"winner":1-loser.0,"reason":"concede"}),
                _ => json!({"winner":null,"reason":"draw"}),
            });
            self.last_view = before.clone();
        } else {
            self.last_view = self.position.observe(Player(0)).agent_view();
            let b = &before["public"]["life"];
            let n = &self.last_view["public"]["life"];
            if b.is_array() && b != n {
                self.log
                    .push(json!({"sequence":self.sequence,"life":n,"before":b}));
            }
        }
        if self.trace {
            self.timeline.push(json!({"sequence":self.sequence,"actor":actor.0,"action":visible,
                "before":before,"after":self.last_view,"events":events.iter().map(|e|e.agent_view()).collect::<Vec<_>>(),"outcome":self.outcome}));
        }
        Ok(())
    }
    fn commit(
        &mut self,
        actor: Player,
        a: Action,
        index: Option<u128>,
        kind: &str,
        request: Option<&V>,
    ) -> Res<()> {
        let mut next = self.position.clone();
        self.controls
            .clone()
            .apply(&mut next, actor, a)
            .map_err(err)?;
        next.audit().map_err(err)?;
        let row = json!({"sequence":self.sequence,"actor":actor.0,"action":action_text(a),"index":index.map(|i|i.to_string()),"kind":kind,
            "request_id":request.and_then(|r|r["id"].as_str()),"request":request,"search":if kind=="bot" {self.last_search.clone()}else{V::Null}});
        self.append(&row)?;
        self.apply(actor, a)?;
        if let Some(r) = request {
            self.requests
                .insert(r["id"].as_str().unwrap().into(), r.clone());
        }
        Ok(())
    }
    fn search_complete(budget: f64, steps: Option<u64>, s: &SearchSession) -> bool {
        steps.map_or_else(|| s.elapsed() >= budget, |n| s.steps() >= n)
    }
    fn slice(budget: f64, steps: Option<u64>, s: &SearchSession, seconds: f64) -> Budget {
        match steps {
            Some(n) => Budget::Steps(n.saturating_sub(s.steps()).min(32_000)),
            None => Budget::Time(Duration::from_secs_f64(
                (budget - s.elapsed()).max(0.).min(seconds),
            )),
        }
    }
    fn forecast_complete(&self) -> bool {
        self.forecast_sequence == self.sequence
            && (self.candidates.len() >= 8 || self.forecast_attempts >= 64)
            && self
                .candidates
                .iter()
                .all(|s| Self::search_complete(self.budget, self.budget_steps, s))
    }
    fn idle(&self) -> bool {
        self.outcome.is_some()
            || self.fault.is_some()
            || (matches!(
                self.controls.step(&self.position.engine),
                Ok(AutoStep::Decision {
                    actor: Player(0),
                    ..
                })
            ) && (!self.ponder
                || Instant::now() >= self.visible_until
                || self.forecast_complete()))
    }
    fn state(&self) -> V {
        let mut editor = V::Null;
        let mut status = if self.outcome.is_some() {
            "finished"
        } else if self.fault.is_some() {
            "paused"
        } else {
            "thinking"
        };
        if self.outcome.is_none()
            && matches!(
                self.controls.step(&self.position.engine),
                Ok(AutoStep::Decision {
                    actor: Player(0),
                    ..
                })
            )
        {
            if self.fault.is_none() {
                status = "human";
            }
            if let Ok(actions) = self.controls.actions(&self.position.engine) {
                editor = actions.editor(&self.position.observe(Player(0)));
            }
        }
        json!({"control":if self.controls.players[0].mode.land_macro {"auto-macro-v6"} else {"auto-macro-v4"},"human_plan":self.controls.plan(Player(0)).map(|p|self.position.observe(Player(0)).action_view(Action::SearchPlan{card:p.card,pay:p.pay})),"sequence":self.sequence,"starting":self.starting,"status":status,"observation":self.last_view,"editor":editor,"outcome":self.outcome,
            "timeline":if self.trace {Some(&self.timeline)} else {None},"log":self.log,"ponder_enabled":self.ponder,"pondering":status=="human"&&self.ponder&&Instant::now()<self.visible_until&&!self.forecast_complete(),
            "ponder_ready":status=="human"&&self.ponder&&self.forecast_complete()&&!self.candidates.is_empty(),
            "thinking_seconds":self.active.as_ref().map_or(0.,|s|s.elapsed()),"budget_seconds":if self.budget_steps.is_none() {Some(self.budget)} else {None},
            "budget_kind":if self.budget_steps.is_some() {"steps"} else {"time"},"budget_steps":self.budget_steps,
            "thinking_steps":self.active.as_ref().map_or(0, |s|s.steps()),"reused_steps":self.reused_steps,
            "reused_seconds":self.reused,"last_search":self.last_search,"error":self.fault})
    }
    fn command(&mut self, r: &V) -> Res<()> {
        let id = r["id"]
            .as_str()
            .filter(|s| s.len() <= 100)
            .ok_or("request id")?;
        if let Some(old) = self.requests.get(id) {
            return if old == r {
                Ok(())
            } else {
                Err("同じ要求IDが別の操作に使われました。".into())
            };
        }
        match r["op"].as_str() {
            Some("presence") => {
                self.visible_until = if r["visible"] == true {
                    Instant::now() + Duration::from_secs(15)
                } else {
                    Instant::now()
                };
            }
            Some("ponder") => {
                let enabled = r["enabled"].as_bool().ok_or("enabled")?;
                self.append(&json!({"kind":"ponder","sequence":self.sequence,"enabled":enabled,"request_id":id,"request":r}))?;
                self.ponder = enabled;
                self.sequence += 1;
                self.forecast_attempts = 0;
                self.requests.insert(id.into(), r.clone());
                if !self.ponder {
                    self.candidates.clear();
                }
            }
            Some("resume") => {
                if self.fault.take().is_some() {
                    self.active = None;
                }
            }
            Some("act" | "concede") => {
                let sequence_valid = r["sequence"].as_u64().is_some_and(|seen| {
                    if r["op"] == "concede" {
                        seen <= self.sequence
                    } else {
                        seen == self.sequence
                    }
                });
                if !sequence_valid {
                    return Err("盤面が更新されました。現在の選択を確認してください。".into());
                }
                if self.outcome.is_some() {
                    return Err("対戦は終了しています。".into());
                }
                if r["op"] == "concede" {
                    self.append(&json!({"kind":"concede","sequence":self.sequence,"request_id":id,"request":r}))?;
                    self.sequence += 1;
                    self.outcome = Some(json!({"winner":1,"reason":"concede"}));
                    self.requests.insert(id.into(), r.clone());
                    self.active = None;
                    self.candidates.clear();
                } else {
                    if self.fault.is_some() {
                        return Err("対局を再開してから操作してください。".into());
                    }
                    let actions = match self.controls.step(&self.position.engine).map_err(err)? {
                        AutoStep::Decision {
                            actor: Player(0),
                            actions,
                        } => actions,
                        _ => return Err("botの思考中です。".into()),
                    };
                    let index = actions.editor_index(&r["selection"])?;
                    self.commit(Player(0), actions.get(index), Some(index), "human", Some(r))?;
                }
            }
            Some("show") => (),
            _ => return Err("unknown command".into()),
        }
        Ok(())
    }
    fn tick(&mut self) -> Res<bool> {
        if self.outcome.is_some() || self.fault.is_some() {
            return Ok(false);
        }
        for _ in 0..256 {
            match self.controls.step(&self.position.engine).map_err(err)? {
                AutoStep::Finished(_) => return Ok(true),
                AutoStep::Chance => {
                    let saved = self.chance.clone();
                    let a = chance_action(&self.position.engine, &mut self.chance).map_err(err)?;
                    if let Err(e) = self.commit(Player(0), a, None, "chance", None) {
                        self.chance = saved;
                        return Err(e);
                    }
                }
                AutoStep::Automatic { actor, action, .. } => {
                    self.commit(actor, action, None, "auto", None)?
                }
                AutoStep::Decision {
                    actor: Player(0), ..
                } => {
                    if self.ponder && Instant::now() < self.visible_until {
                        self.ponder_tick();
                    }
                    return Ok(false);
                }
                AutoStep::Decision { actor, .. } => {
                    if self.active.is_none() {
                        let o = self.position.observe(actor);
                        let found = self.candidates.iter().position(|c| {
                            c.observation() == &o && c.plan() == self.controls.plan(actor)
                        });
                        self.active = Some(if let Some(i) = found {
                            self.candidates.remove(i)
                        } else {
                            SearchSession::new(
                                &o,
                                Config {
                                    control: self.controls.players[actor.0 as usize].mode,
                                    seed: self.seed.wrapping_add(self.sequence * 1000003),
                                    plan: self.controls.plan(actor),
                                    ..Default::default()
                                },
                            )?
                        });
                        self.reused = self.active.as_ref().unwrap().elapsed();
                        self.reused_steps = self.active.as_ref().unwrap().steps();
                        self.candidates.clear();
                        self.active.as_mut().unwrap().memory_limit = 7 * 1024 * 1024;
                        self.active.as_mut().unwrap().interruptible = self.budget_steps.is_none();
                    }
                    let s = self.active.as_mut().unwrap();
                    let report = s.run(Self::slice(self.budget, self.budget_steps, s, 0.05));
                    if let Some(e) = report.error {
                        return Err(format!("探索を保留しました: {e}"));
                    }
                    if Self::search_complete(self.budget, self.budget_steps, s) {
                        let a = report.selected.ok_or("no completed bot trial")?;
                        let index = report.details["candidates"]
                            .as_array()
                            .and_then(|rows| rows.iter().find(|r| r["action"] == action_text(a)))
                            .and_then(|r| r["index"].as_str())
                            .ok_or("selected index")?
                            .parse()
                            .map_err(err)?;
                        self.last_search = json!({"seconds":s.elapsed(),"reused_seconds":self.reused,"trials":report.completed,"steps":report.steps,"reused_steps":self.reused_steps,"budget_steps":self.budget_steps,"incomplete":report.incomplete});
                        self.commit(actor, a, Some(index), "bot", None)?;
                        self.active = None;
                    }
                    return Ok(true);
                }
            }
        }
        Ok(true)
    }
    fn ponder_tick(&mut self) {
        if self.forecast_sequence != self.sequence {
            self.forecast_sequence = self.sequence;
            self.forecast_attempts = 0;
        }
        if self.candidates.len() < 8 && self.forecast_attempts < 64 {
            self.forecast_attempts += 1;
            let seed = self
                .seed
                .wrapping_add(self.sequence * 1000003)
                .wrapping_add(self.forecast_attempts * 7919);
            if let Some((o, plan)) = ponder_candidate_mode(
                &self.position.observe(Player(1)),
                seed,
                self.controls.plan(Player(1)),
                self.controls.players[1].mode,
            ) {
                if !self
                    .candidates
                    .iter()
                    .any(|s| s.observation() == &o && s.plan() == plan)
                {
                    if let Ok(mut s) = SearchSession::new(
                        &o,
                        Config {
                            control: self.controls.players[1].mode,
                            seed,
                            plan,
                            ..Default::default()
                        },
                    ) {
                        s.memory_limit = 7 * 1024 * 1024;
                        s.interruptible = self.budget_steps.is_none();
                        self.candidates.push(s);
                    }
                }
            }
        }
        if self.candidates.is_empty() {
            return;
        }
        self.cursor = (self.cursor + 1) % self.candidates.len();
        let s = &mut self.candidates[self.cursor];
        if !Self::search_complete(self.budget, self.budget_steps, s) {
            let r = s.run(Self::slice(self.budget, self.budget_steps, s, 0.04));
            if r.error.is_some() {
                self.candidates.remove(self.cursor);
            }
        }
        while self
            .candidates
            .iter()
            .map(|s| s.memory_bytes())
            .sum::<usize>()
            > 64 * 1024 * 1024
        {
            self.candidates.remove(0);
        }
    }
}
fn emit(value: V) {
    println!("{value}");
    let _ = io::stdout().flush();
}
fn main() -> Res<()> {
    let args: Vec<_> = env::args().collect();
    let path = PathBuf::from(args.get(1).ok_or("session path")?);
    let seed = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(1);
    let starting = args.get(3).and_then(|s| s.parse().ok()).unwrap_or(0);
    let fixed_steps = args.get(4).is_none() || args.get(5).is_some_and(|s| s == "--steps");
    let amount = args
        .get(4)
        .map(|s| s.parse::<u64>().map_err(err))
        .transpose()?
        .unwrap_or(800_000);
    let mut g = if fixed_steps {
        Game::open_budget(path, seed, starting, 0, Some(amount))?
    } else {
        Game::open(path, seed, starting, amount)?
    };
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        for line in io::stdin().lock().lines() {
            match line {
                Ok(l) => {
                    if tx.send(l).is_err() {
                        break;
                    }
                }
                Err(_) => break,
            }
        }
    });
    emit(json!({"state":g.state()}));
    let mut published = g.sequence;
    let mut last = Instant::now();
    loop {
        match rx.recv_timeout(Duration::from_millis(if g.idle() { 250 } else { 5 })) {
            Ok(line) => {
                let r: V = match serde_json::from_str(&line) {
                    Ok(r) => r,
                    Err(_) => continue,
                };
                let error = g.command(&r).err();
                emit(json!({"id":r["id"],"error":error,"state":g.state()}));
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => break,
            Err(mpsc::RecvTimeoutError::Timeout) => (),
        }
        if let Err(e) = g.tick() {
            g.fault = Some(e);
            g.active = None;
            g.candidates.clear();
        }
        if published != g.sequence || last.elapsed() > Duration::from_millis(500) {
            emit(json!({"state":g.state()}));
            published = g.sequence;
            last = Instant::now();
        }
    }
    Ok(())
}
#[cfg(test)]
#[path = "../../tests/support/play_worker.rs"]
mod tests;
