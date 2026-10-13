use burn_engine::{
    harness::*,
    terminal_bot::{Budget, Config, Method, search},
    *,
};
use serde_json::{Value, json};
use std::{
    env, fs,
    hash::{Hash, Hasher},
    io::{self, Write},
    time::{Duration, Instant},
};
fn source_id() -> String {
    let mut h = std::collections::hash_map::DefaultHasher::new();
    for source in [
        include_str!("../lib.rs"),
        include_str!("../terminal_bot/mod.rs"),
        include_str!("../harness/mod.rs"),
        include_str!("../harness/text.rs"),
        include_str!("../harness/actions.rs"),
        include_str!("../harness/auto.rs"),
        include_str!("../harness/observation.rs"),
        include_str!("../harness/view.rs"),
        include_str!("../terminal_bot/search.rs"),
        include_str!("../terminal_bot/no_suicide.rs"),
        include_str!("../terminal_bot/features.rs"),
        include_str!("../terminal_bot/rollout_policy.rs"),
        include_str!("../terminal_bot/rollout_model.json"),
        include_str!("terminal-bot.rs"),
        include_str!("../../Cargo.lock"),
        include_str!("../../../research/full_state/src/lib.rs"),
    ] {
        source.hash(&mut h);
    }
    format!("rust-source-{:016x}", h.finish())
}
fn arg(args: &[String], key: &str, default: &str) -> String {
    args.iter()
        .position(|a| a == key)
        .map(|i| args.get(i + 1).expect("missing argument").clone())
        .unwrap_or(default.into())
}
fn number(args: &[String], key: &str, default: u64) -> u64 {
    arg(args, key, &default.to_string())
        .parse()
        .expect("invalid integer")
}
fn method(s: &str) -> Method {
    match s {
        "mcts" => Method::Mcts,
        "uniform" => Method::Uniform,
        _ => panic!("method must be mcts or uniform"),
    }
}
fn config(args: &[String], seed: u64, method: Method) -> Config {
    let budget = if args.iter().any(|x| x == "--trials") {
        Budget::Trials(number(args, "--trials", 128))
    } else {
        Budget::Time(Duration::from_secs_f64(
            arg(args, "--seconds", "20").parse().expect("seconds"),
        ))
    };
    Config {
        method,
        budget,
        seed,
        ..Default::default()
    }
}
fn emit(out: &mut impl Write, value: Value) {
    serde_json::to_writer(&mut *out, &value).unwrap();
    writeln!(out).unwrap();
    out.flush().unwrap();
}
fn fresh(seed: u64) -> Position {
    let mut rng = Random(seed);
    Position::new(Engine::new([deck(&mut rng), deck(&mut rng)], Player(0)).unwrap())
}
fn replay(path: &str, game: u64, decision: u64) -> std::result::Result<Position, String> {
    let text = fs::read_to_string(path).map_err(|e| e.to_string())?;
    let mut position = None;
    for line in text.lines() {
        let row: Value = serde_json::from_str(line).map_err(|e| e.to_string())?;
        if row["game"].as_u64() != Some(game) {
            continue;
        }
        match row["type"].as_str() {
            Some("game_start") => {
                if row["source_id"] != source_id() {
                    return Err("source differs from recording".into());
                }
                position = Some(fresh(row["deck_seed"].as_u64().unwrap()));
            }
            Some("chance") => {
                let p = position.as_mut().ok_or("missing game start")?;
                let list = row["order"].as_array().ok_or("chance order")?;
                let mut order = [0; 60];
                for (i, n) in list.iter().enumerate() {
                    order[i] = n.as_u64().unwrap() as u8;
                }
                p.apply(
                    Player(0),
                    Action::Shuffle {
                        order,
                        len: list.len() as u8,
                    },
                )
                .map_err(|e| format!("{e:?}"))?;
            }
            Some("automatic") => {
                let p = position.as_mut().ok_or("missing game start")?;
                match auto_step(&p.engine).map_err(|e| format!("{e:?}"))? {
                    AutoStep::Automatic {
                        actor,
                        action,
                        reason,
                    } => {
                        if row["actor"] != actor.0
                            || row["action"] != action_text(action)
                            || row["reason"] != reason
                        {
                            return Err("replay automatic action differs".into());
                        }
                        p.apply(actor, action).map_err(|e| format!("{e:?}"))?;
                    }
                    _ => return Err("replay automatic boundary differs".into()),
                }
            }
            Some("decision") => {
                let p = position.as_mut().ok_or("missing game start")?;
                let observed = p.observe(p.engine.actor().ok_or("no actor")?).summary();
                if observed != row["observation"] {
                    return Err("replay observation differs".into());
                }
                if row["decision"].as_u64() == Some(decision) {
                    return Ok(p.clone());
                }
                let actor = p.engine.actor().ok_or("no actor")?;
                let space = p
                    .observe(actor)
                    .auto_actions()
                    .map_err(|e| format!("{e:?}"))?;
                let i = row["action_index"]
                    .as_str()
                    .ok_or("no selected index")?
                    .parse::<u128>()
                    .map_err(|e| e.to_string())?;
                if i >= space.len() {
                    return Err("replay action index out of range".into());
                }
                let a = space.get(i);
                if row["report"]["selected"] != action_text(a) {
                    return Err("replay action differs".into());
                }
                p.apply(actor, a).map_err(|e| format!("{e:?}"))?;
                p.audit().map_err(|e| format!("{e:?}"))?;
            }
            Some("game_end") => {
                let p = position.as_ref().ok_or("missing game start")?;
                let outcome = p
                    .engine
                    .finished()
                    .ok_or("recording ends before terminal")?;
                if row["outcome"] != format!("{outcome:?}") {
                    return Err("replay outcome differs".into());
                }
                if decision == u64::MAX {
                    return Ok(p.clone());
                }
            }
            _ => (),
        }
    }
    Err("requested decision not in recording".into())
}
fn main() -> std::result::Result<(), String> {
    let args = env::args().collect::<Vec<_>>();
    let command = args.get(1).map(String::as_str).unwrap_or("help");
    if command == "help" || command == "--help" {
        println!(
            "terminal-bot match [--games 2] [--seed 1] [--trials N | --seconds 20] [--a mcts] [--b uniform] [--out FILE]\nterminal-bot probe [--seed 1] [--steps 0] [--method mcts] [--trials N | --seconds 20] [--replay FILE --game 0 --decision N] [--out FILE]\nterminal-bot replay --replay FILE --game 0 --decision N\nPaired games share initial deck orders and swap method seats. Fixed auto mode in play, tree and rollouts. Default time budget: 20 seconds per auto decision. In-progress trials finish after the soft deadline. Replay files must match compiled source identifier."
        );
        return Ok(());
    }
    let mut output: Box<dyn Write> = if args.iter().any(|a| a == "--out") {
        Box::new(fs::File::create(arg(&args, "--out", "")).map_err(|e| e.to_string())?)
    } else {
        Box::new(io::stdout())
    };
    if command == "probe" || command == "replay" {
        let seed = number(&args, "--seed", 1);
        let mut p = if args.iter().any(|a| a == "--replay") {
            replay(
                &arg(&args, "--replay", ""),
                number(&args, "--game", 0),
                number(
                    &args,
                    "--decision",
                    if command == "replay" { u64::MAX } else { 0 },
                ),
            )?
        } else {
            fresh(seed)
        };
        if command == "replay" {
            if let Some(outcome) = p.engine.finished() {
                emit(
                    &mut output,
                    json!({"control":"auto","source_id":source_id(),"finished":format!("{outcome:?}")}),
                );
                return Ok(());
            }
        }
        let mut rng = Random(seed ^ 0x7761726d);
        let mut env_rng = Random(seed ^ 0x656e76);
        for _ in 0..number(&args, "--steps", 0) {
            if p.engine.finished().is_some() {
                return Err("warmup reached terminal; choose fewer steps".into());
            }
            match auto_step(&p.engine).map_err(|e| format!("{e:?}"))? {
                AutoStep::Chance => {
                    resolve_chance(&mut p, &mut env_rng).map_err(|e| format!("{e:?}"))?
                }
                AutoStep::Automatic { actor, action, .. } => {
                    p.apply(actor, action).map_err(|e| format!("{e:?}"))?;
                }
                AutoStep::Decision { actor, actions } => {
                    p.apply(actor, actions.get(rng.below(actions.len())))
                        .map_err(|e| format!("{e:?}"))?;
                }
                AutoStep::Finished(_) => return Err("warmup reached terminal".into()),
            }
        }
        let mut reached = false;
        for _ in 0..100_000 {
            match auto_step(&p.engine).map_err(|e| format!("{e:?}"))? {
                AutoStep::Decision { .. } => {
                    reached = true;
                    break;
                }
                AutoStep::Chance => {
                    resolve_chance(&mut p, &mut env_rng).map_err(|e| format!("{e:?}"))?
                }
                AutoStep::Automatic { actor, action, .. } => {
                    p.apply(actor, action).map_err(|e| format!("{e:?}"))?;
                }
                AutoStep::Finished(_) => return Err("warmup reached terminal".into()),
            }
        }
        if !reached {
            return Err("auto advance watchdog".into());
        }
        let actor = p.engine.actor().unwrap();
        let o = p.observe(actor);
        if command == "replay" {
            emit(
                &mut output,
                json!({"control":"auto","source_id":source_id(),"observation":o.summary()}),
            );
            return Ok(());
        }
        let report = search(
            &o,
            config(&args, seed, method(&arg(&args, "--method", "mcts"))),
        );
        emit(
            &mut output,
            json!({"type":"probe","control":"auto","source_id":source_id(),"observation":o.summary(),"report":report.json()}),
        );
        if let Some(e) = report.error {
            return Err(e);
        }
        return Ok(());
    }
    if command != "match" {
        return Err("unknown command (use --help)".into());
    }
    let seed = number(&args, "--seed", 1);
    let games = number(&args, "--games", 2);
    let a = method(&arg(&args, "--a", "mcts"));
    let b = method(&arg(&args, "--b", "uniform"));
    let mut outcomes = [0u64; 3];
    let start = Instant::now();
    for game in 0..games {
        let deck_seed = seed + game / 2;
        let methods = if game % 2 == 0 { [a, b] } else { [b, a] };
        let mut p = fresh(deck_seed);
        let mut env_rng = Random(deck_seed ^ 0x656e7669726f6e);
        let mut decision = 0u64;
        let mut primitive_steps = 0u64;
        let mut automatic_steps = 0u64;
        emit(
            &mut output,
            json!({"type":"game_start","game":game,"deck_seed":deck_seed,"control":"auto","source_id":source_id(),"methods":methods.map(|m|format!("{m:?}")),"config":format!("{:?}",config(&args,seed,a))}),
        );
        loop {
            if let Some(outcome) = p.engine.finished() {
                let result = match outcome {
                    Outcome::Draw => 2,
                    Outcome::Won { winner } => {
                        if winner.0 as u64 == game % 2 {
                            0
                        } else {
                            1
                        }
                    }
                    Outcome::Conceded { loser } => {
                        if loser.0 as u64 == game % 2 {
                            1
                        } else {
                            0
                        }
                    }
                };
                outcomes[result] += 1;
                emit(
                    &mut output,
                    json!({"type":"game_end","game":game,"outcome":format!("{outcome:?}"),"a_result":result,"decisions":decision,"primitive_steps":primitive_steps,"automatic_steps":automatic_steps}),
                );
                eprintln!("game {}: {:?}, {} decisions", game, outcome, decision);
                break;
            }
            if primitive_steps >= 100_000 {
                return Err("actual-game watchdog; unfinished".into());
            }
            primitive_steps += 1;
            let actor = match auto_step(&p.engine).map_err(|e| format!("{e:?}"))? {
                AutoStep::Chance => {
                    let action =
                        chance_action(&p.engine, &mut env_rng).map_err(|e| format!("{e:?}"))?;
                    let Action::Shuffle { order, len } = action else {
                        unreachable!()
                    };
                    emit(
                        &mut output,
                        json!({"type":"chance","game":game,"order":&order[..len as usize]}),
                    );
                    p.apply(Player(0), action).map_err(|e| format!("{e:?}"))?;
                    p.audit().map_err(|e| format!("{e:?}"))?;
                    continue;
                }
                AutoStep::Automatic {
                    actor,
                    action,
                    reason,
                } => {
                    emit(
                        &mut output,
                        json!({"type":"automatic","game":game,"actor":actor.0,"action":action_text(action),"reason":reason}),
                    );
                    p.apply(actor, action).map_err(|e| format!("{e:?}"))?;
                    p.audit().map_err(|e| format!("{e:?}"))?;
                    automatic_steps += 1;
                    continue;
                }
                AutoStep::Decision { actor, .. } => actor,
                AutoStep::Finished(_) => unreachable!(),
            };
            let observation = p.observe(actor);
            let cfg = config(
                &args,
                deck_seed.wrapping_mul(1000003).wrapping_add(decision),
                methods[actor.0 as usize],
            );
            let report = search(&observation, cfg);
            let space = observation.auto_actions().map_err(|e| format!("{e:?}"))?;
            // Root index is logged directly; do not enumerate a large lazy space to find it.
            let index = if space.len() == 1 {
                Some("0".to_string())
            } else {
                report.details["candidates"]
                    .as_array()
                    .and_then(|v| {
                        v.iter().find(|x| {
                            Some(x["action"].as_str().unwrap())
                                == report.selected.map(action_text).as_deref()
                        })
                    })
                    .map(|x| x["index"].as_str().unwrap().to_string())
            };
            emit(
                &mut output,
                json!({"type":"decision","game":game,"decision":decision,"actor":actor.0,"observation":observation.summary(),"action_index":index,"report":report.json()}),
            );
            let action = report
                .selected
                .ok_or_else(|| report.error.unwrap_or("no selected move".into()))?;
            p.apply(actor, action)
                .map_err(|e| format!("game {game} decision {decision}: {e:?}"))?;
            p.audit().map_err(|e| format!("game inventory: {e:?}"))?;
            decision += 1;
        }
    }
    emit(
        &mut output,
        json!({"type":"summary","control":"auto","source_id":source_id(),"games":games,"a_wins":outcomes[0],"b_wins":outcomes[1],"draws":outcomes[2],"seconds":start.elapsed().as_secs_f64(),"paired_seed":true}),
    );
    Ok(())
}
