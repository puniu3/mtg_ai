use burn_engine::*;
use mtg_state_design::burn::{Execution, Phase, State, World};
use serde_json::{Value as V, json};
use std::{
    collections::BTreeMap,
    io::{self, BufRead},
    time::Instant,
};
fn card(v: &V) -> Card {
    Card(
        NAMES
            .iter()
            .position(|s| Some(*s) == v.as_str())
            .expect("card name") as u16,
    )
}
fn player(v: &V) -> Player {
    Player(v.as_u64().unwrap_or(0) as u8)
}
fn arr<'a>(v: &'a V) -> &'a [V] {
    v.as_array().map(Vec::as_slice).unwrap_or(&[])
}
fn active<T: Copy, const N: usize>(p: &Pool<T, N>) -> &[T] {
    &p.data[..p.len as usize]
}
struct Driver {
    e: Engine,
    tags: BTreeMap<String, ObjectId>,
}
impl Driver {
    fn setup(v: &V) -> Self {
        let mut w = World::default();
        w.identity = Identity {
            next: 1,
            next_stamp: 1,
        };
        for p in 0..2 {
            w.players[p].life = v["life"][p].as_i64().unwrap_or(20) as i32;
            for c in arr(&v["hands"][p]) {
                w.zones.hand[p][card(c).0 as usize] += 1;
            }
            for c in arr(&v["libraries"][p]).iter().rev() {
                w.zones.library[p].push(card(c)).unwrap();
            }
            for (i, n) in arr(&v["mana"][p]).iter().enumerate() {
                w.players[p].mana[i] = n.as_u64().unwrap() as u32;
            }
        }
        let mut d = Self {
            e: Engine::fixture(
                w,
                player(&v["active"]),
                v["turn"].as_u64().unwrap_or(3) as u32,
            ),
            tags: BTreeMap::new(),
        };
        for b in arr(&v["board"]) {
            let p = player(&b["player"]);
            let id =
                d.e.enter(card(&b["card"]), p, b["tapped"].as_bool().unwrap_or(false))
                    .unwrap();
            let n = d.e.world().battlefield.len as usize - 1;
            let per = &mut d.e.world_mut().battlefield.data[n];
            if !b["sick"].as_bool().unwrap_or(false) {
                per.status |= CONTROLLED;
            }
            per.damage = b["damage"].as_i64().unwrap_or(0) as i32;
            d.tags.insert(b["tag"].as_str().unwrap().to_owned(), id);
        }
        d.e.world_mut().turn_history = BurnHistory::default();
        d
    }
    fn reference(&self, v: &V) -> Subject {
        if let Some(s) = v.as_str() {
            return match s {
                "p0" => Subject::Player { player: Player(0) },
                "p1" => Subject::Player { player: Player(1) },
                _ => Subject::Object {
                    id: *self.tags.get(s).expect("tag"),
                },
            };
        }
        if v.is_null() {
            return Subject::Absent;
        }
        let c = card(&v["card"]);
        let p = player(&v["player"]);
        let n = v["index"].as_u64().unwrap_or(0) as usize;
        let per = active(&self.e.world().battlefield)
            .iter()
            .filter(|x| {
                matches!(x.origin,Origin::Card{card,..}if card==c) && x.base_controller == p
            })
            .nth(n)
            .expect("permanent reference");
        Subject::Object { id: per.ids.first }
    }
    fn object(&self, v: &V) -> ObjectId {
        if let Subject::Object { id } = self.reference(v) {
            id
        } else {
            panic!("object reference")
        }
    }
    fn finish_choices(&mut self, cmd: &V) -> Result {
        let mut rng = verification::Rng(42);
        let mut discard_cursor = 0;
        for _ in 0..100 {
            if self.e.finished().is_some() {
                return Ok(());
            }
            let Execution::Running { running } = self.e.execution() else {
                return Ok(());
            };
            let p = self.e.actor().unwrap_or(Player(0));
            match running.choice {
                Choice::Mana { .. } => return Err(Error::Illegal("unfinished payment")),
                Choice::YesNo { .. } => self.e.apply(
                    p,
                    Action::Shock {
                        pay: cmd["shock"].as_bool().unwrap_or(false),
                    },
                )?,
                Choice::Select {
                    rule: Behavior(302),
                    ..
                } => {
                    let c = if cmd["fetch"].is_null() {
                        None
                    } else {
                        Some(card(&cmd["fetch"]))
                    };
                    self.e.apply(p, Action::Search { card: c })?;
                }
                Choice::Select {
                    rule: Behavior(304),
                    ..
                } => {
                    if cmd["decline_suspend"].as_bool().unwrap_or(false) {
                        self.e.apply(p, Action::DeclineSuspend)?;
                    } else {
                        let t = self.reference(&cmd["targets"][0]);
                        self.e.apply(p, Action::SuspendTarget { target: t })?;
                    }
                }
                Choice::Select {
                    rule: Behavior(307),
                    ..
                } => {
                    let mut assignments = [(ObjectId(0), Subject::Absent, 0); 48];
                    let aa = arr(&cmd["assignments"]);
                    for (i, a) in aa.iter().enumerate() {
                        assignments[i] = (
                            self.object(&a[0]),
                            self.reference(&a[1]),
                            a[2].as_u64().unwrap() as u32,
                        );
                    }
                    self.e.apply(
                        p,
                        Action::Damage {
                            assignments,
                            len: aa.len() as u8,
                        },
                    )?;
                }
                Choice::Select {
                    rule: Behavior(303),
                    ..
                } if cmd["discard"].is_array() => {
                    let c = card(&cmd["discard"][discard_cursor]);
                    discard_cursor += 1;
                    self.e.apply(p, Action::Discard { card: c })?;
                }
                Choice::Select {
                    rule: Behavior(300 | 301),
                    ..
                } => return Ok(()),
                Choice::Chance { .. } => {
                    let mut a = verification::library_shuffle(&self.e, &mut rng);
                    if let Action::Shuffle { order, len } = &mut a {
                        for i in 0..*len as usize {
                            order[i] = i as u8;
                        }
                    }
                    self.e.apply(p, a)?;
                }
                _ => verification::drive_step(&mut self.e, &mut rng)?,
            }
        }
        Err(Error::Illegal("choice loop"))
    }
    fn pay(&mut self, p: Player, cost: Cost, cmd: &V) -> Result {
        for tap in arr(&cmd["taps"]) {
            let id = self.object(&tap["object"]);
            let color = if tap["color"] == "W" { WHITE } else { RED };
            self.e.apply(
                p,
                Action::Tap {
                    object: id,
                    color: color as u8,
                },
            )?;
        }
        let plan = verification::payment(&self.e, p, cost, None).ok_or(Error::Illegal("fixture mana"))?;
        if plan.len != 0 {
            return Err(Error::Illegal("unscripted tap"));
        }
        self.e.apply(p, Action::Pay { mana: plan.mana })
    }
    fn pass_pair(&mut self) -> Result {
        for _ in 0..2 {
            let p = self.e.actor().ok_or(Error::Illegal("no priority"))?;
            self.e.apply(p, Action::Pass)?;
        }
        Ok(())
    }
    fn step(&mut self, v: &V) -> Result {
        let p = player(&v["player"]);
        let op = v["op"].as_str().unwrap();
        // Fixture protocol directly selects priority holder for action windows. This is not exposed by the rules API.
        if matches!(op, "cast" | "land" | "tap" | "fetch" | "canyon" | "suspend") {
            if let State::Playing { execution, .. } = &mut self.e.state {
                *execution = Execution::Priority {
                    priority: Priority {
                        player: p,
                        consecutive_passes: 0,
                    },
                };
            }
        }
        match op {
            "cast" => {
                let c = card(&v["card"]);
                let ts = [
                    self.reference(&v["targets"][0]),
                    self.reference(&v["targets"][1]),
                ];
                let spectacle = v["spectacle"].as_bool().unwrap_or(false);
                self.e.apply(
                    p,
                    Action::Cast {
                        card: c,
                        targets: ts,
                        mode: v["mode"].as_u64().unwrap_or(0) as u8,
                        spectacle,
                    },
                )?;
                self.pay(p, cost(c, spectacle), v)?;
            }
            "land" => self.e.apply(
                p,
                Action::Land {
                    card: card(&v["card"]),
                },
            )?,
            "tap" => {
                let id = self.object(&v["object"]);
                self.e.apply(
                    p,
                    Action::Tap {
                        object: id,
                        color: if v["color"] == "W" { WHITE } else { RED } as u8,
                    },
                )?;
            }
            "fetch" => {
                let id = self.object(&v["object"]);
                self.e.apply(p, Action::Fetch { object: id })?;
            }
            "canyon" => {
                let id = self.object(&v["object"]);
                self.e.apply(p, Action::Canyon { object: id })?;
                self.pay(
                    p,
                    Cost {
                        generic: 1,
                        ..Default::default()
                    },
                    v,
                )?;
            }
            "suspend" => {
                self.e.apply(p, Action::Suspend)?;
                self.pay(
                    p,
                    Cost {
                        red: 1,
                        ..Default::default()
                    },
                    v,
                )?;
            }
            "resolve" => {
                if self.e.world().stack.len == 0 {
                    return Err(Error::Illegal("empty stack"));
                }
                self.pass_pair()?;
            }
            "combat_begin" => {
                self.pass_pair()?;
            }
            "attack" => {
                self.pass_pair()?;
                let mut objects = [ObjectId(0); 24];
                let aa = arr(&v["attackers"]);
                for (i, a) in aa.iter().enumerate() {
                    objects[i] = self.object(a);
                }
                self.e.apply(
                    self.e.actor().unwrap(),
                    Action::Attack {
                        objects,
                        len: aa.len() as u8,
                    },
                )?;
            }
            "block" => {
                self.pass_pair()?;
                let mut pairs = [(ObjectId(0), ObjectId(0)); 24];
                let aa = arr(&v["blocks"]);
                for (i, a) in aa.iter().enumerate() {
                    pairs[i] = (self.object(&a[0]), self.object(&a[1]));
                }
                self.e.apply(
                    self.e.actor().unwrap(),
                    Action::Block {
                        pairs,
                        len: aa.len() as u8,
                    },
                )?;
            }
            "damage" => self.pass_pair()?,
            "mulligan_phase" => {
                let w = *self.e.world();
                let starting = self.e.progress().turn.active;
                self.e.state = State::Mulligan {
                    setup: mtg_state_design::burn::Setup {
                        identity: w.identity,
                        zones: w.zones,
                        companion: w.companion,
                    },
                    process: mtg_state_design::Mulligan {
                        starting,
                        next: starting,
                        ..Default::default()
                    },
                    execution: Execution::Idle,
                };
                let mut declarations = [0u8; 2];
                for _ in 0..300 {
                    let State::Mulligan {
                        setup, execution, ..
                    } = self.e.state
                    else {
                        break;
                    };
                    let actor = self.e.actor().unwrap_or(Player(0));
                    match execution {
                        Execution::Idle => {
                            let take = declarations[actor.0 as usize]
                                < (v["mulligans"][actor.0 as usize].as_u64().unwrap() as u8);
                            declarations[actor.0 as usize] += 1;
                            self.e.apply(actor, Action::Mulligan { take })?;
                        }
                        Execution::Running { running } => match running.choice {
                            Choice::Chance { .. } => {
                                let who = if let Frame::MulliganAction { player, .. } =
                                    running.frames.data[0]
                                {
                                    player
                                } else {
                                    unreachable!()
                                };
                                let lib = setup.zones.library[who.0 as usize];
                                let mut indices: Vec<usize> = (0..lib.len as usize).collect();
                                indices.sort_by_key(|i| std::cmp::Reverse(lib.data[*i].0));
                                let mut order = [0; 60];
                                for (i, j) in indices.into_iter().enumerate() {
                                    order[i] = j as u8;
                                }
                                self.e.apply(
                                    Player(0),
                                    Action::Shuffle {
                                        order,
                                        len: lib.len as u8,
                                    },
                                )?;
                            }
                            Choice::Select { .. } => {
                                let c = (0..16)
                                    .find(|i| setup.zones.hand[actor.0 as usize][*i] > 0)
                                    .unwrap();
                                self.e.apply(
                                    actor,
                                    Action::Bottom {
                                        card: Card(c as u16),
                                    },
                                )?;
                            }
                            _ => return Err(Error::Illegal("mulligan fixture choice")),
                        },
                        _ => return Err(Error::Illegal("mulligan fixture boundary")),
                    }
                }
            }
            "next_turn" => {
                if let State::Playing {
                    progress,
                    execution,
                    ..
                } = &mut self.e.state
                {
                    progress.phase = Phase::Ending { cleanup: false };
                    *execution = Execution::Priority {
                        priority: Priority {
                            player: progress.turn.active,
                            consecutive_passes: 0,
                        },
                    };
                }
                self.pass_pair()?;
            }
            "upkeep" => {
                // Testing a delayed trigger at a specified upkeep. Normal games use pass/phase transitions.
                if let State::Playing {
                    progress,
                    execution,
                    ..
                } = &mut self.e.state
                {
                    progress.phase = Phase::Ending { cleanup: true };
                    progress.turn.next_normal = p;
                    *execution = Execution::Priority {
                        priority: Priority {
                            player: progress.turn.active,
                            consecutive_passes: 0,
                        },
                    };
                }
                self.pass_pair()?;
            }
            _ => return Err(Error::Illegal("protocol operation")),
        }
        self.finish_choices(v)
    }
    fn snapshot(&self) -> V {
        if let Some(out) = self.e.finished() {
            return json!({"over":true,"winner":match out{mtg_state_design::Outcome::Won{winner}=>winner.0 as i32,mtg_state_design::Outcome::Conceded{loser}=>(loser.0^1)as i32,_=>-1}});
        }
        let w = self.e.world();
        let mut ps = Vec::new();
        let count = |a: &[u8; 16]| -> BTreeMap<&str, u8> {
            NAMES
                .iter()
                .enumerate()
                .filter_map(|(i, n)| if a[i] > 0 { Some((*n, a[i])) } else { None })
                .collect()
        };
        for p in 0..2 {
            let mut exile = w.zones.ordinary_exile[p];
            for t in active(&w.zones.tracked) {
                if t.owner == Player(p as u8) && matches!(t.zone, OwnedZone::Exile { .. }) {
                    exile[t.card.0 as usize] += 1;
                }
            }
            let mut exile_detail = Vec::new();
            for (c, n) in w.zones.ordinary_exile[p].iter().enumerate() {
                for _ in 0..*n {
                    exile_detail.push((NAMES[c], 0u32));
                }
            }
            for t in active(&w.zones.tracked) {
                if t.owner == Player(p as u8) && matches!(t.zone, OwnedZone::Exile { .. }) {
                    let n = active(&w.counters)
                        .iter()
                        .filter(|x| x.subject == Subject::Object { id: t.id })
                        .map(|x| x.amount)
                        .sum::<u32>();
                    exile_detail.push((NAMES[t.card.0 as usize], n));
                }
            }
            exile_detail.sort();
            ps.push(json!({"exile_detail":exile_detail,"life":w.players[p].life,"hand":count(&w.zones.hand[p]),"graveyard":count(&w.zones.graveyard[p]),"library":active(&w.zones.library[p]).iter().rev().map(|c|NAMES[c.0 as usize]).collect::<Vec<_>>(),"mana":w.players[p].mana,"exile":count(&exile)}));
        }
        let board:Vec<_>=active(&w.battlefield).iter().map(|per|{let card=if let Origin::Card{card,..}=per.origin{card}else{unreachable!()};let(pt,tt)=self.e.power_toughness(per.ids.first).unwrap_or((0,0));json!({"card":NAMES[card.0 as usize],"player":per.base_controller.0,"tapped":per.status&TAPPED!=0,"damage":per.damage,"controlled_since_turn_start":per.status&CONTROLLED!=0,"power":pt,"toughness":tt,"indestructible":self.e.has_ability(per.ids.first,200),"double":self.e.has_ability(per.ids.first,201)})}).collect();
        let stack:Vec<_>=active(&w.stack).iter().map(|s|match s.body{StackBody::Spell{origin:Origin::Card{card,..},..}=>json!({"kind":"spell","card":NAMES[card.0 as usize],"player":s.controller.0}),StackBody::Ability{definition,..}=>json!({"kind":match definition.0{100=>"prowess",101=>"eidolon",102=>"guide",103=>"fetch",104=>"draw",105=>"time",106=>"suspend_cast",_=>"unknown"},"player":s.controller.0}),_=>unreachable!()}).collect();
        json!({"players":ps,"board":board,"stack":stack,"over":false})
    }
}
fn run_cases() {
    for line in io::stdin().lock().lines() {
        let line = line.unwrap();
        let request: V = serde_json::from_str(&line).unwrap();
        let mut d = Driver::setup(&request["setup"]);
        let mut snapshots = vec![d.snapshot()];
        let mut error = None;
        for cmd in arr(&request["actions"]) {
            if let Err(e) = d.step(cmd) {
                error = Some(format!("{e:?}"));
                break;
            }
            snapshots.push(d.snapshot());
        }
        println!(
            "{}",
            if let Some(err) = error {
                json!({"name":request["name"],"error":err,"snapshots":snapshots})
            } else {
                json!({"name":request["name"],"snapshots":snapshots})
            }
        );
    }
}
fn selfplay(n: usize) {
    let start = Instant::now();
    let mut games = Vec::new();
    for seed in 1..=n {
        let mut rng = verification::Rng(seed as u64 * 0x9e3779b9);
        let decks = [verification::deck(&mut rng), verification::deck(&mut rng)];
        let mut e = Engine::new(decks, Player((seed % 2) as u8)).unwrap();
        let mut steps = 0;
        let mut error = None;
        while e.finished().is_none() && steps < 20000 {
            steps += 1;
            if let Err(err) = verification::drive_step(&mut e, &mut rng) {
                error = Some(format!("{err:?}"));
                break;
            }
        }
        games.push(
            json!({"seed":seed,"steps":steps,"outcome":format!("{:?}",e.finished()),"error":error}),
        );
    }
    println!(
        "{}",
        json!({"games":games,"seconds":start.elapsed().as_secs_f64(),"state_bytes":size_of::<Engine>()})
    );
}
fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.get(1).map(String::as_str) == Some("selfplay") {
        selfplay(args.get(2).and_then(|x| x.parse().ok()).unwrap_or(100));
    } else {
        run_cases();
    }
}
