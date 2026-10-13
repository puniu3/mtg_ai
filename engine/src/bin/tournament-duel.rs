//! Tournament-only CLI: fixed auto, paired seeds, read-only MCTS advice.
//! Player processes never receive the session path; the trusted runner owns it.
use burn_engine::{
    harness::*,
    terminal_bot::{Budget, Config, search},
    *,
};
use serde_json::{Value, json};
use std::{
    env, fs,
    fs::OpenOptions,
    hash::{Hash, Hasher},
    io::{Read, Write},
    os::unix::fs::OpenOptionsExt,
    path::{Path, PathBuf},
};
type Failure<T> = std::result::Result<T, String>;

fn fingerprint() -> String {
    let mut h = std::collections::hash_map::DefaultHasher::new();
    for source in [
        include_str!("tournament-duel.rs"),
        include_str!("../terminal_bot/search.rs"),
        include_str!("../terminal_bot/features.rs"),
        include_str!("../terminal_bot/no_suicide.rs"),
        include_str!("../terminal_bot/rollout_policy.rs"),
        include_str!("../terminal_bot/rollout_model.json"),
        include_str!("../lib.rs"),
        include_str!("../harness/mod.rs"),
        include_str!("../harness/text.rs"),
        include_str!("../harness/actions.rs"),
        include_str!("../harness/auto.rs"),
        include_str!("../harness/observation.rs"),
        include_str!("../harness/view.rs"),
        include_str!("../../../research/full_state/src/lib.rs"),
    ] {
        source.hash(&mut h);
    }
    format!("tournament-{:016x}", h.finish())
}

fn boundary(p: &Position) -> Failure<AutoStep> {
    auto_step(&p.engine).map_err(|e| format!("{e:?}"))
}

struct Session {
    position: Position,
    chance: Random,
    starting: u8,
    sequence: u64,
    viewer: Player,
    transcript: Vec<Value>,
    reveals: Vec<Value>,
}
fn public_action(s: &Session, actor: Player, a: Action) -> Option<Value> {
    if matches!(a, Action::Pass | Action::Shuffle { .. }) {
        return None;
    }
    Some(if actor != s.viewer && matches!(a, Action::Bottom { .. }) {
        json!({"kind":"bottom","card":"private"})
    } else {
        s.position.observe(s.viewer).action_view(a)
    })
}
fn apply(s: &mut Session, actor: Player, a: Action) -> Failure<()> {
    let visible = public_action(s, actor, a);
    let events = s.position.apply(actor, a).map_err(|e| format!("{e:?}"))?;
    s.position.audit().map_err(|e| format!("{e:?}"))?;
    s.sequence += 1;
    if let Some(action) = visible {
        s.transcript
            .push(json!({"after_sequence":s.sequence,"actor":actor.0,"action":action}));
    }
    for event in events {
        let mut row = event.agent_view();
        row["after_sequence"] = json!(s.sequence);
        s.reveals.push(row.clone());
        s.transcript
            .push(json!({"after_sequence":s.sequence,"public_event":row}));
    }
    Ok(())
}
fn fresh(seed: u64, starting: u8, viewer: Player) -> Failure<Session> {
    let mut rng = Random(seed);
    Ok(Session {
        position: Position::new(
            Engine::new([deck(&mut rng), deck(&mut rng)], Player(starting))
                .map_err(|e| format!("{e:?}"))?,
        ),
        chance: Random(seed ^ 0x6475656c),
        starting,
        sequence: 0,
        viewer,
        transcript: Vec::new(),
        reveals: Vec::new(),
    })
}
fn load(path: &Path, viewer: Player) -> Failure<Session> {
    let text = fs::read_to_string(path).map_err(|e| e.to_string())?;
    let mut lines = text.lines();
    let head: Value =
        serde_json::from_str(lines.next().ok_or("empty session")?).map_err(|e| e.to_string())?;
    if head["source"] != fingerprint() {
        return Err("session source mismatch; use its original binary".into());
    }
    let starting = head["starting"]
        .as_u64()
        .filter(|n| *n < 2)
        .ok_or("starting player")? as u8;
    let mut s = fresh(head["seed"].as_u64().ok_or("seed")?, starting, viewer)?;
    for line in lines {
        let row: Value = serde_json::from_str(line).map_err(|e| e.to_string())?;
        if row["sequence"].as_u64() != Some(s.sequence) {
            return Err("sequence mismatch".into());
        }
        let (actor, action) = match (row["kind"].as_str(), boundary(&s.position)?) {
            (Some("chance"), AutoStep::Chance) => (
                Player(0),
                chance_action(&s.position.engine, &mut s.chance).map_err(|e| format!("{e:?}"))?,
            ),
            (Some("auto"), AutoStep::Automatic { actor, action, .. }) => (actor, action),
            (Some("decision"), AutoStep::Decision { actor, actions }) => {
                let i = row["index"]
                    .as_str()
                    .ok_or("index")?
                    .parse::<u128>()
                    .map_err(|e| e.to_string())?;
                if i >= actions.len() {
                    return Err("index outside legal space".into());
                }
                (actor, actions.get(i))
            }
            _ => return Err("replay control boundary mismatch".into()),
        };
        if row["actor"].as_u64() != Some(actor.0 as u64) || row["action"] != action_text(action) {
            return Err("replay action mismatch".into());
        }
        apply(&mut s, actor, action)?;
    }
    Ok(s)
}
fn commit(
    s: &mut Session,
    path: &Path,
    actor: Player,
    action: Action,
    index: Option<u128>,
    kind: &str,
) -> Failure<()> {
    let mut next = s.position.clone();
    next.apply(actor, action).map_err(|e| format!("{e:?}"))?;
    next.audit().map_err(|e| format!("{e:?}"))?;
    let row = json!({"sequence":s.sequence,"actor":actor.0,"kind":kind,"index":index.map(|i|i.to_string()),"action":action_text(action)});
    let mut f = OpenOptions::new()
        .append(true)
        .open(path)
        .map_err(|e| e.to_string())?;
    writeln!(f, "{row}").map_err(|e| e.to_string())?;
    f.sync_data().map_err(|e| e.to_string())?;
    apply(s, actor, action)
}
fn advance(s: &mut Session, path: &Path) -> Failure<()> {
    for _ in 0..10000 {
        match boundary(&s.position)? {
            AutoStep::Finished(_) | AutoStep::Decision { .. } => return Ok(()),
            AutoStep::Chance => {
                let a = chance_action(&s.position.engine, &mut s.chance)
                    .map_err(|e| format!("{e:?}"))?;
                commit(s, path, Player(0), a, None, "chance")?;
            }
            AutoStep::Automatic { actor, action, .. } => {
                commit(s, path, actor, action, None, "auto")?
            }
        }
    }
    Err("auto advance watchdog; committed actions saved".into())
}
fn act(s: &mut Session, path: &Path, index: u128, sequence: u64) -> Failure<()> {
    if sequence != s.sequence {
        return Err("stale sequence; show the current view".into());
    }
    let AutoStep::Decision { actor, actions } = boundary(&s.position)? else {
        return Err("not at a decision".into());
    };
    if actor != s.viewer {
        return Err("not this player's decision".into());
    }
    if index >= actions.len() {
        return Err("index outside legal space".into());
    }
    commit(s, path, actor, actions.get(index), Some(index), "decision")?;
    advance(s, path)
}
fn view(s: &Session, since: u64, offset: u128) -> Failure<Value> {
    let transcript = s
        .transcript
        .iter()
        .filter(|v| v["after_sequence"].as_u64().unwrap() > since)
        .collect::<Vec<_>>();
    let mut out = json!({"control":"auto","human":0,"agent":1,"starting":s.starting,"viewer":s.viewer.0,"sequence":s.sequence,"transcript":transcript,"reveal_history":s.reveals,
        "automatic":"shared fixed auto, identical to G1 search"});
    match boundary(&s.position)? {
        AutoStep::Finished(outcome) => {
            out["finished"] = json!(format!("{outcome:?}"));
        }
        AutoStep::Decision { actor, actions } => {
            let o = s.position.observe(s.viewer);
            out["actor"] = json!(actor.0);
            out["observation"] = o.agent_view();
            if actor == s.viewer {
                out["legal_count"] = json!(actions.len().to_string());
                out["legal_actions"] = json!((offset..actions.len().min(offset.saturating_add(80))).map(|i| json!({"index":i.to_string(),"action":o.action_view(actions.get(i))})).collect::<Vec<_>>());
                out["action_offset"] = json!(offset.to_string());
                out["more_actions"] = json!(offset.saturating_add(80) < actions.len());
            } else {
                out["legal_count"] = json!("0");
                out["legal_actions"] = json!([]);
            }
        }
        _ => {
            out["needs_resume"] = json!(true);
            out["observation"] = s.position.observe(s.viewer).agent_view();
        }
    }
    Ok(out)
}

struct Lock(PathBuf);
impl Lock {
    fn acquire(path: &Path) -> Failure<Self> {
        let p = PathBuf::from(format!("{}.lock", path.display()));
        let mut f = OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&p)
            .map_err(|e| format!("session lock: {e}"))?;
        let lock = Self(p);
        writeln!(f, "{}", std::process::id()).map_err(|e| e.to_string())?;
        Ok(lock)
    }
}
impl Drop for Lock {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.0);
    }
}
fn option(args: &[String], name: &str) -> Option<String> {
    args.iter()
        .position(|a| a == name)
        .and_then(|i| args.get(i + 1))
        .cloned()
}
fn main() -> Failure<()> {
    let args = env::args().collect::<Vec<_>>();
    let command = args.get(1).map(String::as_str).unwrap_or("help");
    if command == "help" {
        println!(
            "tournament-duel new SESSION [--starting random|0|1] [--seed U64]\ntournament-duel show SESSION PLAYER [--since SEQUENCE] [--offset INDEX]\ntournament-duel act SESSION PLAYER INDEX EXPECTED_SEQUENCE [--since SEQUENCE]\ntournament-duel resume SESSION PLAYER [--since SEQUENCE]\ntournament-duel analyze SESSION PLAYER EXPECTED_SEQUENCE [--seconds 10] [--search-seed U64]\nP0/P1 external policies. Session log is private; only use observer views. New never overwrites."
        );
        return Ok(());
    }
    if !matches!(command, "new" | "show" | "act" | "resume" | "analyze") {
        return Err("unknown command".into());
    }
    // Validate output options before any session creation or committed move.
    let since = option(&args, "--since")
        .unwrap_or("0".into())
        .parse::<u64>()
        .map_err(|e| e.to_string())?;
    let offset = option(&args, "--offset")
        .unwrap_or("0".into())
        .parse::<u128>()
        .map_err(|e| e.to_string())?;
    let path = Path::new(args.get(2).ok_or("session path required")?);
    let viewer = if command == "new" {
        0
    } else {
        args.get(3)
            .ok_or("viewer required")?
            .parse::<u8>()
            .map_err(|e| e.to_string())?
    };
    if viewer > 1 {
        return Err("viewer must be 0 or 1".into());
    }
    let _lock = if matches!(command, "show" | "analyze") {
        None
    } else {
        Some(Lock::acquire(path)?)
    };
    if command == "new" {
        let mut random = [0u8; 9];
        fs::File::open("/dev/urandom")
            .map_err(|e| e.to_string())?
            .read_exact(&mut random)
            .map_err(|e| e.to_string())?;
        let starting = match option(&args, "--starting").as_deref().unwrap_or("random") {
            "random" => random[8] & 1,
            "0" => 0,
            "1" => 1,
            _ => return Err("starting must be random, 0 or 1".into()),
        };
        let seed = match option(&args, "--seed") {
            Some(v) => v.parse::<u64>().map_err(|e| e.to_string())?,
            None => u64::from_le_bytes(random[..8].try_into().unwrap()),
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
            json!({"source":fingerprint(),"seed":seed,"starting":starting,"control":"auto"})
        )
        .map_err(|e| e.to_string())?;
        f.sync_all().map_err(|e| e.to_string())?;
    }
    let mut s = load(path, Player(viewer))?;
    if command == "analyze" {
        let expected = args
            .get(4)
            .ok_or("expected sequence required")?
            .parse::<u64>()
            .map_err(|e| e.to_string())?;
        if s.sequence != expected {
            return Err("stale analysis sequence".into());
        }
        let seconds = option(&args, "--seconds")
            .unwrap_or("10".into())
            .parse::<f64>()
            .map_err(|e| e.to_string())?;
        if !seconds.is_finite() || seconds <= 0. || seconds > 10. {
            return Err("analysis budget must be >0 and <=10 seconds".into());
        }
        let seed = option(&args, "--search-seed")
            .unwrap_or("1".into())
            .parse::<u64>()
            .map_err(|e| e.to_string())?;
        println!(
            "{}",
            analyze(
                &s,
                expected,
                Config {
                    budget: Budget::Time(std::time::Duration::from_secs_f64(seconds)),
                    seed,
                    ..Config::default()
                }
            )?
        );
        return Ok(());
    }
    match command {
        "new" | "resume" => advance(&mut s, path)?,
        "act" => act(
            &mut s,
            path,
            args.get(4)
                .ok_or("index required")?
                .parse::<u128>()
                .map_err(|e| e.to_string())?,
            args.get(5)
                .ok_or("expected sequence required")?
                .parse::<u64>()
                .map_err(|e| e.to_string())?,
        )?,
        _ => (),
    }
    println!("{}", view(&s, since, offset)?);
    Ok(())
}

fn analyze(s: &Session, expected: u64, config: Config) -> Failure<Value> {
    if expected != s.sequence {
        return Err("stale analysis sequence".into());
    }
    let AutoStep::Decision { actor, actions } = boundary(&s.position)? else {
        return Err("not at a decision".into());
    };
    if actor != s.viewer {
        return Err("not this player's decision".into());
    }
    let observation = s.position.observe(actor);
    let report = search(&observation, config);
    let index = report
        .selected
        .and_then(|a| (0..actions.len()).find(|i| actions.get(*i) == a));
    let mut out = report.json();
    out["sequence"] = json!(s.sequence);
    out["selected_index"] = json!(index.map(|i| i.to_string()));
    out["selected_action"] = json!(report.selected.map(|a| observation.action_view(a)));
    out["read_only"] = json!(true);
    if let Some(candidates) = out["details"]["candidates"].as_array_mut() {
        for c in candidates {
            let i = c["index"]
                .as_str()
                .ok_or("candidate index")?
                .parse::<u128>()
                .map_err(|e| e.to_string())?;
            c["action"] = observation.action_view(actions.get(i));
        }
    }
    Ok(out)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn advice_is_read_only_reproducible_and_rejects_wrong_viewer() {
        let mut s = fresh(71, 0, Player(0)).unwrap();
        let before = view(&s, 0, 0).unwrap();
        let config = Config {
            budget: Budget::Trials(16),
            seed: 908,
            ..Config::default()
        };
        let a = analyze(&s, 0, config).unwrap();
        assert!(a["error"].is_null());
        assert!(a["selected_index"].is_string());
        assert_eq!(before, view(&s, 0, 0).unwrap());
        assert_eq!(a["details"], analyze(&s, 0, config).unwrap()["details"]);
        assert!(analyze(&s, 1, config).is_err());
        s.viewer = Player(1);
        assert!(analyze(&s, 0, config).is_err());
    }
}
