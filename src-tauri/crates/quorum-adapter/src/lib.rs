//! citrate-quorum — the vendor adapter core (WP-S4a.1).
//!
//! One implementation of "ask before you act", shared by every adapter in the
//! fleet: Claude Code and Codex (CLI supervisor), MCP servers (stdio + HTTP),
//! and later A2A. Each vendor differs only in how it hands us a tool call and
//! how it wants the answer back; the governance is identical and lives here.
//!
//! ## What an adapter is for
//!
//! `02_ARCHITECTURE.md` §4.3 requires the policy gate to run **in the adapter,
//! before execution**. A prompt telling an agent to behave is not a control
//! (CLAUDE.md rule 4). This is the control: the tool call does not run until
//! Quorum has ruled on it and recorded the ruling.
//!
//! ## Installed vs reachable — the distinction that matters
//!
//! - **No endpoint file** → Quorum is not set up on this machine. The adapter
//!   is a no-op and the agent runs ungoverned, exactly as it would without us.
//!   Refusing here would break tools on machines that never opted in.
//! - **Endpoint file present but the bridge is unreachable** → someone DID set
//!   this up and the gate is down. Refuse. An adapter that waves calls through
//!   when it cannot reach the gate is governance theatre, and the failure mode
//!   is silent, which is the worst kind.
//!
//! There is deliberately no fail-open switch. An escape hatch on a control is
//! the thing an attacker looks for first, and the thing a hurried operator
//! reaches for permanently.

use std::io::{Read, Write};
use std::net::TcpStream;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

/// Where the running app publishes its bridge address and bearer token.
pub const ENDPOINT_FILE: &str = "endpoint.json";
pub const TOKEN_FILE: &str = "token";

/// The app identifier the desktop app registers under.
const APP_ID: &str = "ai.citrate.quorum";

// ---- locating the running app ---------------------------------------

/// The `<app_data>/agent` directory the desktop app writes.
///
/// `QUORUM_AGENT_DIR` overrides it — needed for tests, for a non-standard
/// install, and for running the adapter as a different user than the app.
pub fn agent_dir() -> Option<PathBuf> {
    if let Ok(dir) = std::env::var("QUORUM_AGENT_DIR") {
        return Some(PathBuf::from(dir));
    }
    let base = if cfg!(target_os = "macos") {
        std::env::var("HOME")
            .ok()
            .map(|h| PathBuf::from(h).join("Library/Application Support"))
    } else if cfg!(target_os = "windows") {
        std::env::var("APPDATA").ok().map(PathBuf::from)
    } else {
        std::env::var("XDG_DATA_HOME")
            .ok()
            .map(PathBuf::from)
            .or_else(|| {
                std::env::var("HOME")
                    .ok()
                    .map(|h| PathBuf::from(h).join(".local/share"))
            })
    }?;
    Some(base.join(APP_ID).join("agent"))
}

#[derive(Deserialize)]
struct Endpoint {
    addr: String,
}

/// How the adapter found (or failed to find) the gate.
#[derive(Debug, PartialEq, Eq)]
pub enum Discovery {
    /// Quorum is set up here and we know where to ask.
    Ready { addr: String, token: String },
    /// Quorum is not set up on this machine — the adapter stands aside.
    NotInstalled,
    /// Set up, but we cannot read what we need. Refuse; do not guess.
    Broken(String),
}

pub fn discover(dir: Option<PathBuf>) -> Discovery {
    let Some(dir) = dir.or_else(agent_dir) else {
        return Discovery::NotInstalled;
    };
    let ep = dir.join(ENDPOINT_FILE);
    if !ep.exists() {
        return Discovery::NotInstalled;
    }
    let raw = match std::fs::read_to_string(&ep) {
        Ok(r) => r,
        Err(e) => return Discovery::Broken(format!("cannot read {}: {e}", ep.display())),
    };
    let addr = match serde_json::from_str::<Endpoint>(&raw) {
        Ok(e) => e.addr,
        Err(e) => return Discovery::Broken(format!("{} is malformed: {e}", ep.display())),
    };
    match std::fs::read_to_string(dir.join(TOKEN_FILE)) {
        Ok(t) if !t.trim().is_empty() => Discovery::Ready {
            addr,
            token: t.trim().to_string(),
        },
        Ok(_) => Discovery::Broken("the agent token file is empty".into()),
        Err(e) => Discovery::Broken(format!("cannot read the agent token: {e}")),
    }
}

// ---- the wire ------------------------------------------------------

#[derive(Serialize, Default)]
pub struct Intent {
    pub agent: String,
    pub principal: Option<String>,
    pub tool: String,
    pub classification: String,
    pub cost: u64,
    pub hic1_cost_threshold: u64,
    pub mandatory_hic1: bool,
    pub params_hash: String,
    pub model_id: String,
    pub correlation_id: String,
}

#[derive(Deserialize, Debug, Clone)]
pub struct Verdict {
    pub verdict: String,
    pub may_proceed: bool,
    pub hic: String,
    pub grant_id: Option<String>,
    pub reason: String,
    pub decision_id: u64,
    pub ungoverned: bool,
}

#[derive(Deserialize, Debug, Clone)]
pub struct Status {
    pub status: String,
    pub may_proceed: bool,
}

/// The gate, as the adapter needs it. A trait so the decision logic below is
/// testable without a socket or a running app.
pub trait Gate {
    fn submit(&self, intent: &Intent) -> Result<Verdict, String>;
    fn status(&self, decision_id: u64) -> Result<Status, String>;
}

/// The real gate: minimal HTTP/1.1 over loopback.
pub struct HttpGate {
    pub addr: String,
    pub token: String,
}

impl HttpGate {
    fn request(&self, method: &str, path: &str, body: Option<&str>) -> Result<String, String> {
        let mut stream = TcpStream::connect(&self.addr)
            .map_err(|e| format!("cannot reach the Quorum gate at {}: {e}", self.addr))?;
        stream
            .set_read_timeout(Some(Duration::from_secs(10)))
            .map_err(|e| e.to_string())?;
        let body = body.unwrap_or("");
        let req = format!(
            "{method} {path} HTTP/1.1\r\nHost: {}\r\nAuthorization: Bearer {}\r\n\
             Content-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            self.addr,
            self.token,
            body.len()
        );
        stream
            .write_all(req.as_bytes())
            .map_err(|e| format!("gate write failed: {e}"))?;
        let mut raw = String::new();
        stream
            .read_to_string(&mut raw)
            .map_err(|e| format!("gate read failed: {e}"))?;
        let (head, payload) = raw
            .split_once("\r\n\r\n")
            .ok_or_else(|| "malformed response from the gate".to_string())?;
        let status = head
            .lines()
            .next()
            .and_then(|l| l.split_whitespace().nth(1))
            .unwrap_or("000");
        if status != "200" {
            return Err(format!("gate returned {status}: {}", payload.trim()));
        }
        Ok(payload.to_string())
    }
}

impl Gate for HttpGate {
    fn submit(&self, intent: &Intent) -> Result<Verdict, String> {
        let body = serde_json::to_string(intent).map_err(|e| e.to_string())?;
        let raw = self.request("POST", "/intent", Some(&body))?;
        serde_json::from_str(&raw).map_err(|e| format!("cannot read the verdict: {e}"))
    }
    fn status(&self, decision_id: u64) -> Result<Status, String> {
        let raw = self.request("GET", &format!("/decision/{decision_id}"), None)?;
        serde_json::from_str(&raw).map_err(|e| format!("cannot read the decision status: {e}"))
    }
}

// ---- the decision ---------------------------------------------------

/// What the adapter tells its vendor to do.
#[derive(Debug, PartialEq, Eq)]
pub enum Outcome {
    /// Run it. Either policy allowed it or a human approved it.
    Proceed { reason: String },
    /// Do not run it, and say why in words the agent can act on.
    Refuse { reason: String },
    /// Quorum is not set up here; the adapter has no opinion.
    NotGoverned,
}

/// Submit one tool call and decide.
///
/// `wait` is how long to hold a `require-approval` open. Default zero: the
/// agent is told an approval is pending and told the decision id, rather than
/// blocking someone's editor while a human is at lunch. A wait is available
/// where blocking is the wanted behaviour (a batch run, a CI job).
pub fn gate_call<G: Gate>(gate: &G, intent: &Intent, wait: Duration) -> Outcome {
    let verdict = match gate.submit(intent) {
        Ok(v) => v,
        Err(e) => {
            // The gate was configured and did not answer. Refuse.
            return Outcome::Refuse {
                reason: format!("Quorum could not rule on this action, so it has not run. {e}"),
            };
        }
    };

    if verdict.may_proceed {
        return Outcome::Proceed {
            reason: format!(
                "allowed by {} ({})",
                verdict.grant_id.as_deref().unwrap_or("policy"),
                verdict.reason
            ),
        };
    }

    if verdict.ungoverned {
        return Outcome::Refuse {
            reason: format!(
                "No capability grant covers this action, so it has been recorded as \
                 ungoverned (decision #{}) and surfaced to a human. Ask for a grant \
                 covering `{}` before retrying.",
                verdict.decision_id, intent.tool
            ),
        };
    }

    if verdict.verdict == "require-approval" {
        let deadline = Instant::now() + wait;
        loop {
            if Instant::now() >= deadline {
                break;
            }
            std::thread::sleep(Duration::from_millis(500));
            match gate.status(verdict.decision_id) {
                Ok(s) if s.may_proceed => {
                    return Outcome::Proceed {
                        reason: format!("approved by a human (decision #{})", verdict.decision_id),
                    }
                }
                Ok(s) if s.status == "rejected" => {
                    return Outcome::Refuse {
                        reason: format!(
                            "A human refused this action (decision #{}). Do not retry it; \
                             ask what they would accept instead.",
                            verdict.decision_id
                        ),
                    }
                }
                _ => continue,
            }
        }
        return Outcome::Refuse {
            reason: format!(
                "This action needs a human approval ({}). It is decision #{} in the \
                 Quorum ceremony queue and has NOT run. Continue with other work, or \
                 ask for it to be approved and retry.",
                verdict.reason, verdict.decision_id
            ),
        };
    }

    Outcome::Refuse {
        reason: format!(
            "Quorum policy refused this action (decision #{}): {}",
            verdict.decision_id, verdict.reason
        ),
    }
}

// ---- vendor: Claude Code -------------------------------------------

/// The PreToolUse payload Claude Code writes to a hook's stdin.
#[derive(Deserialize, Default)]
pub struct PreToolUse {
    #[serde(default)]
    pub tool_name: String,
    #[serde(default)]
    pub tool_input: serde_json::Value,
    #[serde(default)]
    pub session_id: String,
    #[serde(default)]
    pub cwd: String,
}

/// A tool call's action class — the unit policy is written against.
///
/// This mapping IS a governance decision, so it is a table you can read, not a
/// guess buried in a match arm. An unmapped tool becomes `tool.<name>`: a new
/// tool is governed by default under its own name, rather than silently
/// inheriting some other class's grant.
pub fn action_class(tool_name: &str) -> String {
    match tool_name {
        "Bash" | "BashOutput" | "KillShell" => "shell.exec".to_string(),
        "Edit" | "Write" | "NotebookEdit" => "repo.write".to_string(),
        "Read" | "Glob" | "Grep" => "repo.read".to_string(),
        "WebFetch" | "WebSearch" => "net.fetch".to_string(),
        "Task" | "Agent" => "agent.spawn".to_string(),
        other => format!("tool.{}", other.to_ascii_lowercase()),
    }
}

/// A BLAKE3 commitment to the tool's parameters — the *what*, without the
/// payload. Serialized canonically so the same call always hashes the same.
pub fn params_hash(input: &serde_json::Value) -> String {
    let canonical = canonical_json(input);
    format!("0x{}", blake3::hash(canonical.as_bytes()).to_hex())
}

/// Deterministic JSON: object keys sorted, no incidental whitespace. Two runs
/// of the same call must produce the same hash or the commitment is worthless.
fn canonical_json(v: &serde_json::Value) -> String {
    match v {
        serde_json::Value::Object(map) => {
            let mut keys: Vec<&String> = map.keys().collect();
            keys.sort();
            let inner: Vec<String> = keys
                .iter()
                .map(|k| {
                    format!(
                        "{}:{}",
                        serde_json::to_string(k).unwrap_or_default(),
                        canonical_json(&map[*k])
                    )
                })
                .collect();
            format!("{{{}}}", inner.join(","))
        }
        serde_json::Value::Array(items) => {
            let inner: Vec<String> = items.iter().map(canonical_json).collect();
            format!("[{}]", inner.join(","))
        }
        other => other.to_string(),
    }
}

/// The JSON Claude Code expects back from a PreToolUse hook.
pub fn claude_hook_response(outcome: &Outcome) -> Option<String> {
    let (decision, reason) = match outcome {
        // Staying silent lets Claude Code's own permission flow run. Returning
        // "allow" would OVERRIDE the user's settings — an adapter that governs
        // agents must not quietly widen what they may do.
        Outcome::Proceed { .. } | Outcome::NotGoverned => return None,
        Outcome::Refuse { reason } => ("deny", reason.clone()),
    };
    Some(
        serde_json::json!({
            "hookSpecificOutput": {
                "hookEventName": "PreToolUse",
                "permissionDecision": decision,
                "permissionDecisionReason": reason,
            }
        })
        .to_string(),
    )
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use std::cell::RefCell;

    struct FakeGate {
        verdict: RefCell<Option<Verdict>>,
        submit_err: Option<String>,
        statuses: RefCell<Vec<Status>>,
    }
    impl FakeGate {
        fn allowing() -> Self {
            Self::with(Verdict {
                verdict: "allow".into(),
                may_proceed: true,
                hic: "2".into(),
                grant_id: Some("G-1".into()),
                reason: "RC-200 within a live grant envelope".into(),
                decision_id: 0,
                ungoverned: false,
            })
        }
        fn with(v: Verdict) -> Self {
            Self {
                verdict: RefCell::new(Some(v)),
                submit_err: None,
                statuses: RefCell::new(Vec::new()),
            }
        }
        fn broken(msg: &str) -> Self {
            Self {
                verdict: RefCell::new(None),
                submit_err: Some(msg.into()),
                statuses: RefCell::new(Vec::new()),
            }
        }
    }
    impl Gate for FakeGate {
        fn submit(&self, _i: &Intent) -> Result<Verdict, String> {
            if let Some(e) = &self.submit_err {
                return Err(e.clone());
            }
            Ok(self.verdict.borrow().clone().unwrap())
        }
        fn status(&self, _id: u64) -> Result<Status, String> {
            let mut s = self.statuses.borrow_mut();
            if s.is_empty() {
                return Ok(Status {
                    status: "pending".into(),
                    may_proceed: false,
                });
            }
            Ok(s.remove(0))
        }
    }

    fn escalating() -> Verdict {
        Verdict {
            verdict: "require-approval".into(),
            may_proceed: false,
            hic: "1".into(),
            grant_id: Some("G-1".into()),
            reason: "RC-102 over single-action ceiling → HIC-1".into(),
            decision_id: 7,
            ungoverned: false,
        }
    }

    #[test]
    fn an_allowed_call_proceeds() {
        let out = gate_call(&FakeGate::allowing(), &Intent::default(), Duration::ZERO);
        assert!(matches!(out, Outcome::Proceed { .. }));
    }

    #[test]
    fn an_unreachable_gate_refuses_it_does_not_wave_the_call_through() {
        let out = gate_call(
            &FakeGate::broken("connection refused"),
            &Intent::default(),
            Duration::ZERO,
        );
        match out {
            Outcome::Refuse { reason } => {
                assert!(reason.contains("has not run"), "{reason}");
                assert!(reason.contains("connection refused"));
            }
            other => panic!("a gate that cannot rule must refuse, got {other:?}"),
        }
    }

    #[test]
    fn an_ungoverned_call_is_refused_and_names_the_class_to_ask_for() {
        let gate = FakeGate::with(Verdict {
            verdict: "ungoverned".into(),
            may_proceed: false,
            hic: "X".into(),
            grant_id: None,
            reason: "RC-000 no live grant covers this action".into(),
            decision_id: 3,
            ungoverned: true,
        });
        let intent = Intent {
            tool: "shell.exec".into(),
            ..Intent::default()
        };
        match gate_call(&gate, &intent, Duration::ZERO) {
            Outcome::Refuse { reason } => {
                assert!(reason.contains("ungoverned"));
                assert!(reason.contains("decision #3"));
                assert!(
                    reason.contains("shell.exec"),
                    "tell it what to ask for: {reason}"
                );
            }
            other => panic!("expected refusal, got {other:?}"),
        }
    }

    #[test]
    fn an_escalation_without_a_wait_refuses_and_hands_over_the_decision_id() {
        match gate_call(
            &FakeGate::with(escalating()),
            &Intent::default(),
            Duration::ZERO,
        ) {
            Outcome::Refuse { reason } => {
                assert!(reason.contains("decision #7"));
                assert!(reason.contains("has NOT run"), "{reason}");
            }
            other => panic!("expected refusal, got {other:?}"),
        }
    }

    #[test]
    fn an_escalation_proceeds_once_a_human_approves() {
        let gate = FakeGate::with(escalating());
        gate.statuses.borrow_mut().push(Status {
            status: "approved".into(),
            may_proceed: true,
        });
        match gate_call(&gate, &Intent::default(), Duration::from_secs(3)) {
            Outcome::Proceed { reason } => assert!(reason.contains("approved by a human")),
            other => panic!("expected proceed, got {other:?}"),
        }
    }

    #[test]
    fn a_human_refusal_stops_the_agent_retrying() {
        let gate = FakeGate::with(escalating());
        gate.statuses.borrow_mut().push(Status {
            status: "rejected".into(),
            may_proceed: false,
        });
        match gate_call(&gate, &Intent::default(), Duration::from_secs(3)) {
            Outcome::Refuse { reason } => {
                assert!(reason.contains("refused"));
                assert!(reason.contains("Do not retry"), "{reason}");
            }
            other => panic!("expected refusal, got {other:?}"),
        }
    }

    #[test]
    fn discovery_distinguishes_not_installed_from_broken() {
        let dir = std::env::temp_dir().join(format!("quorum-adapter-t{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        // Nothing there: Quorum is not set up, and we stand aside.
        assert_eq!(discover(Some(dir.clone())), Discovery::NotInstalled);

        // Set up but unreadable: refuse, never guess.
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join(ENDPOINT_FILE), "not json").unwrap();
        assert!(matches!(discover(Some(dir.clone())), Discovery::Broken(_)));

        std::fs::write(dir.join(ENDPOINT_FILE), r#"{"addr":"127.0.0.1:1"}"#).unwrap();
        assert!(
            matches!(discover(Some(dir.clone())), Discovery::Broken(_)),
            "an endpoint without a token is broken, not ready"
        );

        std::fs::write(dir.join(TOKEN_FILE), "abc123\n").unwrap();
        assert_eq!(
            discover(Some(dir.clone())),
            Discovery::Ready {
                addr: "127.0.0.1:1".into(),
                token: "abc123".into()
            }
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn tool_names_map_to_stated_action_classes() {
        assert_eq!(action_class("Bash"), "shell.exec");
        assert_eq!(action_class("Edit"), "repo.write");
        assert_eq!(action_class("Write"), "repo.write");
        assert_eq!(action_class("Read"), "repo.read");
        assert_eq!(action_class("WebFetch"), "net.fetch");
        // An unknown tool is governed under its own name, never folded into an
        // existing class whose grant would then cover it by accident.
        assert_eq!(action_class("SomeNewTool"), "tool.somenewtool");
    }

    #[test]
    fn the_params_hash_is_canonical_and_sensitive() {
        let a = serde_json::json!({ "command": "ls", "timeout": 5 });
        let b = serde_json::json!({ "timeout": 5, "command": "ls" });
        assert_eq!(
            params_hash(&a),
            params_hash(&b),
            "key order must not change the commitment"
        );
        let c = serde_json::json!({ "command": "rm -rf /", "timeout": 5 });
        assert_ne!(params_hash(&a), params_hash(&c));
        assert!(params_hash(&a).starts_with("0x"));
    }

    #[test]
    fn the_hook_denies_on_refusal_and_stays_silent_otherwise() {
        let deny = claude_hook_response(&Outcome::Refuse {
            reason: "nope".into(),
        })
        .unwrap();
        let v: serde_json::Value = serde_json::from_str(&deny).unwrap();
        assert_eq!(v["hookSpecificOutput"]["hookEventName"], "PreToolUse");
        assert_eq!(v["hookSpecificOutput"]["permissionDecision"], "deny");
        assert_eq!(v["hookSpecificOutput"]["permissionDecisionReason"], "nope");

        // Silence, not "allow": saying allow would override the user's own
        // permission settings and widen what the agent may do.
        assert!(claude_hook_response(&Outcome::Proceed {
            reason: "ok".into()
        })
        .is_none());
        assert!(claude_hook_response(&Outcome::NotGoverned).is_none());
    }
}
