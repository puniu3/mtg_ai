//! Two external players, private observer views, and a persistent replay log.
//! No search policy runs here. Forced steps and fixed payment/ordering advance
//! automatically; all meaningful non-mana priority windows remain available.
use burn_engine::{harness::*, *};
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
        include_str!("agent-duel.rs"),
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
    format!("duel-{:016x}", h.finish())
}

fn boundary(p: &Position) -> Failure<AutoStep> {
    external_step(&p.engine).map_err(|e| format!("{e:?}"))
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
    let mut out = json!({"control":"external_players","human":0,"agent":1,"starting":s.starting,"viewer":s.viewer.0,"sequence":s.sequence,"transcript":transcript,"reveal_history":s.reveals,
        "automatic":"mana payment and trigger ordering; pass only when no non-mana action is available"});
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
            "agent-duel new SESSION [--starting random|0|1]\nagent-duel show SESSION PLAYER [--since SEQUENCE] [--offset INDEX]\nagent-duel act SESSION PLAYER INDEX EXPECTED_SEQUENCE [--since SEQUENCE]\nagent-duel resume SESSION PLAYER [--since SEQUENCE]\nP0 human, P1 external agent. Session log is private; only use observer views. New never overwrites."
        );
        return Ok(());
    }
    if !matches!(command, "new" | "show" | "act" | "resume") {
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
    let _lock = if command == "show" {
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
        let seed = u64::from_le_bytes(random[..8].try_into().unwrap());
        let mut f = OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(path)
            .map_err(|e| e.to_string())?;
        writeln!(f,"{}",json!({"source":fingerprint(),"seed":seed,"starting":starting,"control":"external_players"})).map_err(|e|e.to_string())?;
        f.sync_all().map_err(|e| e.to_string())?;
    }
    let mut s = load(path, Player(viewer))?;
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

#[cfg(test)]
mod tests {
    use super::*;
    use mtg_state_design::burn::World;

    fn session_file(seed: u64) -> PathBuf {
        let path = env::temp_dir().join(format!(
            "mtg-duel-test-{}-{}.jsonl",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let mut f = OpenOptions::new()
            .create_new(true)
            .write(true)
            .mode(0o600)
            .open(&path)
            .unwrap();
        writeln!(
            f,
            "{}",
            json!({"source":fingerprint(),"seed":seed,"starting":0})
        )
        .unwrap();
        path
    }
    #[test]
    fn replay_views_and_wrong_actor_or_stale_action() {
        let path = session_file(22);
        let mut human = load(&path, Player(0)).unwrap();
        let original = fs::read(&path).unwrap();
        let mut opponent = load(&path, Player(1)).unwrap();
        assert!(act(&mut opponent, &path, 0, 0).is_err());
        assert_eq!(fs::read(&path).unwrap(), original);
        assert_eq!(view(&opponent, 0, 0).unwrap()["legal_count"], "0");
        for viewer in [Player(0), Player(1)] {
            let s = load(&path, viewer).unwrap();
            let v = view(&s, 0, 0).unwrap();
            assert_eq!(
                v["observation"]["players"][viewer.0 as usize]["hand"]["unknown_count"],
                0
            );
            assert_eq!(
                v["observation"]["players"][1 - viewer.0 as usize]["hand"]["unknown_count"],
                7
            );
        }
        act(&mut human, &path, 0, 0).unwrap();
        let saved = fs::read(&path).unwrap();
        assert!(act(&mut human, &path, 0, 0).is_err());
        assert_eq!(fs::read(&path).unwrap(), saved);
        let reloaded = load(&path, Player(0)).unwrap();
        assert_eq!(view(&human, 0, 0).unwrap(), view(&reloaded, 0, 0).unwrap());
        let opponent = load(&path, Player(1)).unwrap();
        assert_eq!(
            public_action(&opponent, Player(0), Action::Bottom { card: BOLT }).unwrap(),
            json!({"kind":"bottom","card":"private"})
        );
        fs::remove_file(path).unwrap();
    }
    #[test]
    fn session_lock_rejects_a_second_writer_and_releases_on_drop() {
        let path = session_file(23);
        let lock = Lock::acquire(&path).unwrap();
        assert!(Lock::acquire(&path).is_err());
        assert!(lock.0.exists());
        drop(lock);
        drop(Lock::acquire(&path).unwrap());
        fs::remove_file(path).unwrap();
    }
    #[test]
    fn attacker_can_respond_after_guide_reveal_before_damage() {
        let mut w = World::default();
        w.identity.next = 1;
        for p in 0..2 {
            w.players[p].life = 20;
            w.zones.library[p].push(MOUNTAIN).unwrap();
        }
        w.zones.hand[0][BOLT.0 as usize] = 1;
        let mut e = Engine::fixture(w, Player(0), 3);
        e.enter(MOUNTAIN, Player(0), false).unwrap();
        let guide = e.enter(GUIDE, Player(0), false).unwrap();
        let mut p = Position::new(e);
        let mut revealed = false;
        for _ in 0..100 {
            let (actor, a) = match boundary(&p).unwrap() {
                AutoStep::Automatic { actor, action, .. } => (actor, action),
                AutoStep::Decision { actor, actions } => {
                    if revealed && actor == Player(0) {
                        assert_eq!(p.observe(actor).agent_view()["phase"], "declare_attackers");
                        assert!(
                            (0..actions.len())
                                .any(|i| matches!(actions.get(i), Action::Cast { card: BOLT, .. }))
                        );
                        return;
                    }
                    let a = (0..actions.len())
                        .map(|i| actions.get(i))
                        .find(|a| matches!(a,Action::Attack{objects,len:1} if objects[0]==guide))
                        .or_else(|| {
                            (0..actions.len())
                                .map(|i| actions.get(i))
                                .find(|a| *a == Action::Pass)
                        })
                        .unwrap_or(actions.get(0));
                    (actor, a)
                }
                _ => panic!("unexpected boundary"),
            };
            revealed |= !p.apply(actor, a).unwrap().is_empty();
        }
        panic!("no post-reveal decision");
    }
}
