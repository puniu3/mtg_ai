//! Persistent conversation adapter. Bot policy is the existing terminal-only MCTS.
//! Session logs contain private information; stdout is restricted to human observation.
use burn_engine::{
    harness::*,
    terminal_bot::{Budget, Config, search},
    *,
};
use serde_json::{Value, json};
use std::{
    env,
    fs::{self, OpenOptions},
    hash::{Hash, Hasher},
    io::{Read, Write},
    os::unix::fs::OpenOptionsExt,
    path::Path,
    time::Duration,
};
type Failure<T> = std::result::Result<T, String>;
fn fingerprint() -> String {
    let mut hash = std::collections::hash_map::DefaultHasher::new();
    for s in [
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
        include_str!("chat-bot.rs"),
        include_str!("../../../research/full_state/src/lib.rs"),
    ] {
        s.hash(&mut hash);
    }
    format!("chat-{:016x}", hash.finish())
}
fn append(path: &Path, row: Value) -> Failure<()> {
    let mut f = OpenOptions::new()
        .append(true)
        .open(path)
        .map_err(|e| e.to_string())?;
    serde_json::to_writer(&mut f, &row).map_err(|e| e.to_string())?;
    writeln!(f).map_err(|e| e.to_string())?;
    f.sync_data().map_err(|e| e.to_string())
}
struct Session {
    position: Position,
    chance: Random,
    seed: u64,
    sequence: u64,
    seconds: u64,
    reveal_history: Vec<Value>,
}
#[derive(Default)]
struct Transcript {
    legacy: Vec<Value>,
    agent: Vec<Value>,
}
fn remember_reveals(s: &mut Session, events: &[PublicEvent]) {
    for &event in events {
        let mut row = event.agent_view();
        row["after_sequence"] = json!(s.sequence + 1);
        s.reveal_history.push(row);
    }
}
fn load(path: &Path) -> Failure<Session> {
    let text = fs::read_to_string(path).map_err(|e| e.to_string())?;
    let mut lines = text.lines();
    let head: Value =
        serde_json::from_str(lines.next().ok_or("empty session")?).map_err(|e| e.to_string())?;
    if head["source"] != fingerprint() {
        return Err("session source mismatch".into());
    }
    let seed = head["seed"].as_u64().ok_or("seed")?;
    let mut rng = Random(seed);
    let mut session = Session {
        position: Position::new(
            Engine::new([deck(&mut rng), deck(&mut rng)], Player(0))
                .map_err(|e| format!("{e:?}"))?,
        ),
        chance: Random(seed ^ 0x6368616e6365),
        seed,
        sequence: 0,
        seconds: head["seconds"].as_u64().ok_or("seconds")?,
        reveal_history: Vec::new(),
    };
    for line in lines {
        let row: Value = serde_json::from_str(line).map_err(|e| e.to_string())?;
        if row["sequence"].as_u64() != Some(session.sequence) {
            return Err("sequence mismatch".into());
        }
        let (actor, a) = if row["kind"] == "chance" {
            let a = chance_action(&session.position.engine, &mut session.chance)
                .map_err(|e| format!("{e:?}"))?;
            (Player(0), a)
        } else if row["kind"] == "auto" {
            match auto_step(&session.position.engine).map_err(|e| format!("{e:?}"))? {
                AutoStep::Automatic { actor, action, .. } => {
                    if row["actor"].as_u64() != Some(actor.0 as u64) {
                        return Err("auto actor mismatch".into());
                    }
                    (actor, action)
                }
                _ => return Err("replay auto boundary mismatch".into()),
            }
        } else {
            let actor = session.position.engine.actor().ok_or("no decision actor")?;
            if row["actor"].as_u64() != Some(actor.0 as u64) {
                return Err("actor mismatch".into());
            }
            let actions = session
                .position
                .observe(actor)
                .auto_actions()
                .map_err(|e| format!("{e:?}"))?;
            let i = row["index"]
                .as_str()
                .ok_or("index")?
                .parse::<u128>()
                .map_err(|e| e.to_string())?;
            if i >= actions.len() {
                return Err("replay index out of range".into());
            }
            (actor, actions.get(i))
        };
        if row["action"] != action_text(a) {
            return Err("replay action mismatch".into());
        }
        let events = session
            .position
            .apply(actor, a)
            .map_err(|e| format!("{e:?}"))?;
        remember_reveals(&mut session, &events);
        session.position.audit().map_err(|e| format!("{e:?}"))?;
        session.sequence += 1;
    }
    Ok(session)
}
fn visible_action(observation: &Observation, actor: Player, a: Action) -> Value {
    if actor == Player(1) && matches!(a, Action::Bottom { .. }) {
        json!({"kind":"bottom","card":"private"})
    } else {
        observation.action_view(a)
    }
}
fn legacy_action(actor: Player, a: Action) -> String {
    if actor == Player(1) && matches!(a, Action::Bottom { .. }) {
        "bot: put one card on bottom (private)".into()
    } else {
        format!("P{}: {}", actor.0, action_text(a))
    }
}
fn commit(
    s: &mut Session,
    path: &Path,
    actor: Player,
    a: Action,
    index: Option<u128>,
    kind: &str,
    report: Option<Value>,
    transcript: &mut Transcript,
) -> Failure<()> {
    let visible = if !matches!(a, Action::Pass | Action::Shuffle { .. }) {
        Some(visible_action(&s.position.observe(Player(0)), actor, a))
    } else {
        None
    };
    let mut next = s.position.clone();
    let events = next.apply(actor, a).map_err(|e| format!("{e:?}"))?;
    next.audit().map_err(|e| format!("{e:?}"))?;
    append(
        path,
        json!({"sequence":s.sequence,"kind":kind,"actor":actor.0,"index":index.map(|i|i.to_string()),"action":action_text(a),"report":report}),
    )?;
    s.position = next;
    remember_reveals(s, &events);
    s.sequence += 1;
    if let Some(action) = visible {
        transcript
            .legacy
            .push(json!({"action":legacy_action(actor,a)}));
        transcript
            .agent
            .push(json!({"actor":actor.0,"action":action}));
    }
    for e in events {
        transcript
            .legacy
            .push(json!({"public_event":format!("{e:?}")}));
        transcript
            .agent
            .push(json!({"public_event":e.agent_view()}));
    }
    Ok(())
}
fn advance(s: &mut Session, path: &Path, transcript: &mut Transcript) -> Failure<()> {
    for _ in 0..10000 {
        if s.position.engine.finished().is_some() {
            return Ok(());
        }
        let actor = match auto_step(&s.position.engine).map_err(|e| format!("{e:?}"))? {
            AutoStep::Finished(_) => return Ok(()),
            AutoStep::Chance => {
                let a = chance_action(&s.position.engine, &mut s.chance)
                    .map_err(|e| format!("{e:?}"))?;
                commit(s, path, Player(0), a, None, "chance", None, transcript)?;
                continue;
            }
            AutoStep::Automatic { actor, action, .. } => {
                commit(s, path, actor, action, None, "auto", None, transcript)?;
                continue;
            }
            AutoStep::Decision {
                actor: Player(0), ..
            } => return Ok(()),
            AutoStep::Decision { actor, .. } => actor,
        };
        let o = s.position.observe(actor);
        let report = search(
            &o,
            Config {
                budget: Budget::Time(Duration::from_secs(s.seconds)),
                seed: s.seed.wrapping_add(s.sequence.wrapping_mul(1000003)),
                ..Default::default()
            },
        );
        let a = report
            .selected
            .ok_or_else(|| report.error.clone().unwrap_or("no bot action".into()))?;
        let index = report.details["candidates"]
            .as_array()
            .and_then(|v| v.iter().find(|r| r["action"] == action_text(a)))
            .and_then(|r| r["index"].as_str())
            .ok_or("missing bot index")?
            .parse::<u128>()
            .map_err(|e| e.to_string())?;
        eprintln!(
            "bot decision: {} terminals / {:.3}s",
            report.completed, report.elapsed_seconds
        );
        commit(
            s,
            path,
            actor,
            a,
            Some(index),
            "bot",
            Some(report.json()),
            transcript,
        )?;
    }
    Err("adapter watchdog; session saved before stop".into())
}
fn response(s: &Session, transcript: &Transcript, agent_view: bool) -> Failure<Value> {
    let transcript = if agent_view {
        &transcript.agent
    } else {
        &transcript.legacy
    };
    if let Some(outcome) = s.position.engine.finished() {
        let mut out = json!({"finished":format!("{outcome:?}"),"sequence":s.sequence,"transcript":transcript});
        if agent_view {
            out["reveal_history"] = json!(s.reveal_history);
        }
        return Ok(out);
    }
    let o = s.position.observe(Player(0));
    let actor = s.position.engine.actor();
    let (count, actions) = if matches!(
        auto_step(&s.position.engine).map_err(|e| format!("{e:?}"))?,
        AutoStep::Decision {
            actor: Player(0),
            ..
        }
    ) {
        let a = o.auto_actions().map_err(|e| format!("{e:?}"))?;
        (
            a.len().to_string(),
            (0..a.len().min(80))
                .map(|i| json!({"index":i.to_string(),"action":if agent_view { o.action_view(a.get(i)) } else { json!(action_text(a.get(i))) }}))
                .collect::<Vec<_>>(),
        )
    } else {
        ("0".into(), Vec::new())
    };
    let mut out = json!({"control":"auto","human":0,"bot":1,"bot_seconds_per_decision":s.seconds,"sequence":s.sequence,"actor":actor.map(|p|p.0),"observation":if agent_view { o.agent_view() } else { o.summary() },"legal_count":count,"legal_actions":actions,"display_only_limit":80,"transcript":transcript});
    if agent_view {
        out["reveal_history"] = json!(s.reveal_history);
    }
    Ok(out)
}
fn main() -> Failure<()> {
    let args = env::args().collect::<Vec<_>>();
    let command = args.get(1).map(String::as_str).unwrap_or("help");
    if command == "help" {
        println!(
            "chat-bot new SESSION [--seed N] [--agent-view]\nchat-bot show SESSION [--agent-view]\nchat-bot act SESSION INDEX EXPECTED_SEQUENCE [--agent-view]\nchat-bot resume SESSION [--agent-view]\nDefault output preserves the legacy diagnostic format. --agent-view selects burn-agent-view-v1, named actions, and replayed public reveal history for this call.\nFixed auto mode. Human P0 starts. Bot: 10 seconds per auto decision. New never overwrites. Session is private; stdout only shows human observation. Existing sessions require their original binary."
        );
        return Ok(());
    }
    let path = Path::new(args.get(2).ok_or("session path required")?);
    if command == "new" {
        let mut bytes = [0; 8];
        fs::File::open("/dev/urandom")
            .map_err(|e| e.to_string())?
            .read_exact(&mut bytes)
            .map_err(|e| e.to_string())?;
        let seed = if let Some(i) = args.iter().position(|a| a == "--seed") {
            args.get(i + 1)
                .ok_or("seed value")?
                .parse::<u64>()
                .map_err(|e| e.to_string())?
        } else {
            u64::from_le_bytes(bytes)
        };
        let mut f = OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(path)
            .map_err(|e| e.to_string())?;
        writeln!(
            f,
            "{}",
            json!({"control":"auto","source":fingerprint(),"seed":seed,"human":0,"starting":0,"seconds":10})
        )
        .map_err(|e| e.to_string())?;
        f.sync_all().map_err(|e| e.to_string())?;
    }
    let mut s = load(path)?;
    let mut transcript = Transcript::default();
    match command {
        "show" => (),
        "new" | "resume" => advance(&mut s, path, &mut transcript)?,
        "act" => {
            let sequence = args
                .get(4)
                .ok_or("expected sequence required")?
                .parse::<u64>()
                .map_err(|e| e.to_string())?;
            if s.sequence != sequence {
                return Err("stale sequence; inspect current state".into());
            }
            if s.position.engine.actor() != Some(Player(0)) {
                return Err("not human decision".into());
            }
            let index = args
                .get(3)
                .ok_or("action index required")?
                .parse::<u128>()
                .map_err(|e| e.to_string())?;
            let a = s
                .position
                .observe(Player(0))
                .auto_actions()
                .map_err(|e| format!("{e:?}"))?;
            if index >= a.len() {
                return Err("index outside legal space".into());
            }
            commit(
                &mut s,
                path,
                Player(0),
                a.get(index),
                Some(index),
                "human",
                None,
                &mut transcript,
            )?;
            advance(&mut s, path, &mut transcript)?;
        }
        _ => return Err("unknown command".into()),
    }
    println!(
        "{}",
        response(&s, &transcript, args.iter().any(|a| a == "--agent-view"))?
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn opponent_bottom_is_redacted_and_reveals_survive_reload() {
        let seed = (1..1000)
            .find(|seed| {
                let mut rng = Random(*seed);
                let p = Position::new(
                    Engine::new([deck(&mut rng), deck(&mut rng)], Player(0)).unwrap(),
                );
                p.observe(Player(0)).agent_view()["players"][0]["hand"]["known"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|c| c["card"] == "Goblin Guide")
            })
            .unwrap();
        let path = env::temp_dir().join(format!(
            "mtg-view-reload-{}-{}.jsonl",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&path)
            .unwrap();
        writeln!(
            file,
            "{}",
            json!({"source":fingerprint(),"seed":seed,"seconds":10})
        )
        .unwrap();
        drop(file);
        let mut s = load(&path).unwrap();
        // Existing consumers keep string actions and the diagnostic observation;
        // agents explicitly opt into the structured, observer-safe contract.
        let empty = Transcript::default();
        let legacy = response(&s, &empty, false).unwrap();
        let agent = response(&s, &empty, true).unwrap();
        assert!(legacy["observation"]["own_hand"].is_array());
        assert!(legacy["observation"]["schema"].is_null());
        assert!(legacy["reveal_history"].is_null());
        assert!(
            legacy["legal_actions"]
                .as_array()
                .unwrap()
                .iter()
                .all(|a| a["action"].is_string())
        );
        assert_eq!(agent["observation"]["schema"], "burn-agent-view-v1");
        assert_eq!(agent["legal_actions"][0]["action"]["kind"], "keep");
        assert_eq!(agent["legal_actions"][1]["action"]["kind"], "mulligan");
        assert_eq!(legacy["legal_count"], agent["legal_count"]);
        assert_eq!(
            legacy_action(Player(1), Action::Bottom { card: BOLT }),
            "bot: put one card on bottom (private)"
        );
        let redacted = visible_action(
            &s.position.observe(Player(0)),
            Player(1),
            Action::Bottom { card: BOLT },
        );
        assert_eq!(redacted, json!({"kind":"bottom","card":"private"}));
        let mut transcript = Transcript::default();
        for _ in 0..1000 {
            let (actor, action, index, kind) = match auto_step(&s.position.engine).unwrap() {
                AutoStep::Finished(_) => break,
                AutoStep::Chance => (
                    Player(0),
                    chance_action(&s.position.engine, &mut s.chance).unwrap(),
                    None,
                    "chance",
                ),
                AutoStep::Automatic { actor, action, .. } => (actor, action, None, "auto"),
                AutoStep::Decision { actor, actions } => {
                    let index = (0..actions.len())
                        .min_by_key(|i| match actions.get(*i) {
                            Action::Mulligan { take: false } => 0,
                            Action::Land { .. } => 1,
                            Action::Cast { card: GUIDE, .. } => 2,
                            Action::Attack { len, .. } if len > 0 => 3,
                            Action::Pass => 4,
                            _ => 5,
                        })
                        .unwrap();
                    (actor, actions.get(index), Some(index), "human")
                }
            };
            commit(
                &mut s,
                &path,
                actor,
                action,
                index,
                kind,
                None,
                &mut transcript,
            )
            .unwrap();
            if !s.reveal_history.is_empty() {
                break;
            }
        }
        assert!(!s.reveal_history.is_empty());
        assert!(s.reveal_history[0]["card"].is_string());
        let loaded = load(&path).unwrap();
        assert_eq!(loaded.reveal_history, s.reveal_history);
        assert_eq!(
            loaded.position.observe(Player(0)).agent_view(),
            s.position.observe(Player(0)).agent_view()
        );
        assert!(
            transcript
                .agent
                .iter()
                .any(|r| r["public_event"]["kind"] == "reveal_top")
        );
        assert!(
            transcript
                .legacy
                .iter()
                .all(|r| r["action"].is_string() || r["public_event"].is_string())
        );
        assert_eq!(
            response(&s, &transcript, false).unwrap()["transcript"],
            json!(transcript.legacy)
        );
        assert_eq!(
            response(&loaded, &empty, true).unwrap()["reveal_history"],
            json!(s.reveal_history)
        );
        fs::remove_file(path).unwrap();
    }
}
