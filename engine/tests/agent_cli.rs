//! Exercise the actual process boundary, including rejected writes and the
//! legacy chat contract. Sessions here are synthetic and never use live logs.
use serde_json::Value;
use std::{fs, path::PathBuf, process::Command};

struct Scratch(PathBuf);
impl Scratch {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "burn-agent-cli-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir(&path).unwrap();
        Self(path)
    }
}
impl Drop for Scratch {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).unwrap();
    }
}
fn run(binary: &str, args: &[&str]) -> Value {
    let result = Command::new(binary).args(args).output().unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    serde_json::from_slice(&result.stdout).unwrap()
}

#[test]
fn chat_agent_format_is_opt_in_per_call() {
    let scratch = Scratch::new();
    let file = scratch.0.join("chat.jsonl");
    let path = file.to_str().unwrap();
    let binary = env!("CARGO_BIN_EXE_chat-bot");
    let legacy = run(binary, &["new", path, "--seed", "22"]);
    let saved = fs::read(&file).unwrap();
    let agent = run(binary, &["show", path, "--agent-view"]);
    assert!(legacy["observation"]["own_hand"].is_array());
    assert!(legacy["legal_actions"][0]["action"].is_string());
    assert_eq!(agent["observation"]["schema"], "burn-agent-view-v1");
    assert_eq!(agent["legal_actions"][0]["action"]["kind"], "keep");
    assert_eq!(agent["legal_actions"][1]["action"]["kind"], "mulligan");
    assert_eq!(legacy["sequence"], agent["sequence"]);
    assert_eq!(legacy["legal_count"], agent["legal_count"]);
    assert_eq!(
        agent["observation"]["players"][1]["hand"]["unknown_count"],
        7
    );
    assert_eq!(run(binary, &["show", path]), legacy);
    assert_eq!(fs::read(&file).unwrap(), saved);
}

#[test]
fn duel_rejects_invalid_requests_without_writing_and_keeps_views_private() {
    let scratch = Scratch::new();
    let file = scratch.0.join("duel.jsonl");
    let path = file.to_str().unwrap();
    let binary = env!("CARGO_BIN_EXE_agent-duel");
    let human = run(binary, &["new", path, "--starting", "0"]);
    assert_eq!(human["actor"], 0);
    let agent = run(binary, &["show", path, "1"]);
    assert_eq!(agent["legal_count"], "0");
    assert_eq!(
        agent["observation"]["players"][0]["hand"]["unknown_count"],
        7
    );
    assert_eq!(
        agent["observation"]["players"][1]["hand"]["unknown_count"],
        0
    );
    let saved = fs::read(&file).unwrap();
    for args in [
        vec!["new", path, "--starting", "0"],
        vec!["act", path, "1", "0", "0"],
        vec!["act", path, "0", "0", "99"],
        vec!["act", path, "0", "99", "0"],
        vec!["act", path, "0", "0", "0", "--since", "invalid"],
        vec!["act", path, "0", "0", "0", "--offset", "invalid"],
    ] {
        let result = Command::new(binary).args(args).output().unwrap();
        assert!(!result.status.success());
        assert_eq!(fs::read(&file).unwrap(), saved);
        assert!(!PathBuf::from(format!("{path}.lock")).exists());
    }
    let kept = run(binary, &["act", path, "0", "0", "0"]);
    assert_eq!(kept["actor"], 1);
    assert_eq!(run(binary, &["show", path, "0"]), kept);
    let agent = run(binary, &["show", path, "1", "--since", "1"]);
    assert_eq!(agent["legal_count"], "2");
    assert_eq!(agent["transcript"], serde_json::json!([]));
}
