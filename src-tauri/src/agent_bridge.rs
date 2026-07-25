//! citrate-quorum — the keyless agent bridge (WP-S4.1).
//! @rule8 · a bearer-authed intake surface that agents submit UNSIGNED intents to.
//!
//! This is the uniform contract every adapter in the fleet plugs into — MCP
//! (stdio + HTTP), the CLI-agent supervisor (Claude Code, Codex, …), and later
//! A2A. It generalizes citrate-core's node-agent bridge (`agent.rs`), with the
//! direction reversed: there, core polls a sidecar that serves; here, quorum
//! serves and the agent calls in before it acts.
//!
//! Four properties from `02_ARCHITECTURE.md` §4 and CLAUDE.md rule 4 are the
//! entire point of this module:
//!
//! 1. **No agent ever holds a chain key.** Nothing here signs, and no response
//!    can carry a signature, key, or seed. An agent submits an intent and gets
//!    back a *verdict*. A signature only ever comes from a human moving through
//!    the [`citrate_core_kit::ceremony`] — which this module cannot reach: the
//!    gated signer is `pub(crate)` to the kit.
//! 2. **Loopback-only, bearer-authed, token via a `0600` file, `Zeroizing` in
//!    memory.** The listener binds `127.0.0.1` and nothing else. A fresh 256-bit
//!    token is minted per session with `OsRng`, written to a `0600` file that is
//!    the inter-process channel, and compared in constant time.
//! 3. **Every tool invocation is recorded** — the policy gate runs HERE, in the
//!    adapter, *before* the agent executes, and every submission produces a
//!    decision record on the tenant's BLAKE3 chain. Including the ones that come
//!    back `ungoverned`.
//! 4. **Outside its envelope is a permission request, not a swallowed error.**
//!    A verdict of `require-approval` or `ungoverned` is returned to the agent as
//!    a first-class outcome, with the decision id that a human will act on.
//!
//! ## Shape
//!
//! The protocol handling is a pure function ([`handle`]) over an already-read
//! request, so every property above is unit-testable without a socket. The
//! socket loop ([`serve`]) is a thin shell around it.
//!
//! ```text
//!   GET  /health          open      → {"ok":true}          (liveness only)
//!   POST /intent          bearer    → the recorded verdict
//!   anything else                   → 404
//!   missing/bad bearer              → 401, no detail
//! ```

use std::io::{BufRead, BufReader, Read, Write};
use std::net::{Ipv4Addr, SocketAddr, TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use rand::RngCore;
use serde::{Deserialize, Serialize};
use zeroize::Zeroizing;

use crate::backend::{ActionInput, QuorumBackend};

/// The loopback address the bridge binds. Never anything but loopback: an
/// agent intake reachable off-box is a remote-code-execution surface.
pub const BRIDGE_HOST: Ipv4Addr = Ipv4Addr::LOCALHOST;

/// Max bytes we will read from an agent request body. An intent is a small
/// JSON object; anything larger is a mistake or an attack.
const MAX_BODY: usize = 64 * 1024;

// ---- the token ------------------------------------------------------

/// A per-session bearer token: 256 bits of `OsRng`, hex-encoded, held only as
/// [`Zeroizing`] so it is wiped on drop and never lands in a `Debug` render.
pub struct BridgeToken {
    secret: Zeroizing<String>,
}

impl std::fmt::Debug for BridgeToken {
    /// Deliberately opaque — a token that can be printed WILL be printed.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("BridgeToken(<redacted>)")
    }
}

impl BridgeToken {
    /// Mint a fresh token. Uses the OS CSPRNG; there is no seeded variant,
    /// because a predictable agent token is a full bypass of the gate.
    pub fn mint() -> Self {
        let mut bytes = Zeroizing::new([0u8; 32]);
        rand::rngs::OsRng.fill_bytes(bytes.as_mut());
        Self {
            secret: Zeroizing::new(hex::encode(bytes.as_ref())),
        }
    }

    /// Write the token where the agent can read it, `0600`, replacing any
    /// previous one. The FILE is the inter-process channel — the same pattern
    /// citrate-core uses with the node-agent.
    pub fn write_to(&self, path: &Path) -> std::io::Result<()> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        // Create with the restrictive mode from the start, so the secret is
        // never briefly world-readable between create and chmod.
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            let mut f = std::fs::OpenOptions::new()
                .write(true)
                .create(true)
                .truncate(true)
                .mode(0o600)
                .open(path)?;
            f.write_all(self.secret.as_bytes())?;
            f.sync_all()?;
        }
        #[cfg(not(unix))]
        {
            let mut f = std::fs::File::create(path)?;
            f.write_all(self.secret.as_bytes())?;
            f.sync_all()?;
        }
        Ok(())
    }

    /// Constant-time comparison against a presented bearer value. A
    /// short-circuiting `==` on a secret leaks its prefix through timing.
    pub fn matches(&self, presented: &str) -> bool {
        let a = self.secret.as_bytes();
        let b = presented.as_bytes();
        if a.len() != b.len() {
            return false;
        }
        let mut diff: u8 = 0;
        for (x, y) in a.iter().zip(b.iter()) {
            diff |= x ^ y;
        }
        diff == 0
    }
}

// ---- the wire shapes ------------------------------------------------

/// An UNSIGNED intent: an agent saying what it is about to do, before it does
/// it. There is no signature field and no key field, by construction.
#[derive(Deserialize)]
pub struct IntentRequest {
    /// The acting agent's id (its `AgentSBT` id once the registry is live).
    pub agent: String,
    /// The accountable human behind the agent's grant.
    pub principal: Option<String>,
    /// The tool the agent is about to invoke, e.g. `repo.write`.
    pub tool: String,
    /// The classification of the material it touches.
    pub classification: String,
    #[serde(default)]
    pub cost: u64,
    #[serde(default)]
    pub hic1_cost_threshold: u64,
    #[serde(default)]
    pub mandatory_hic1: bool,
    /// Hash of the tool's parameters — the *what*, without the payload.
    #[serde(default)]
    pub params_hash: String,
    #[serde(default)]
    pub model_id: String,
    #[serde(default)]
    pub correlation_id: String,
}

/// What the agent gets back. A verdict and a receipt — never a signature.
#[derive(Serialize)]
pub struct IntentVerdict {
    /// `allow` | `require-approval` | `deny` | `ungoverned`.
    pub verdict: String,
    /// Whether the agent may proceed. ONLY `allow` is true; an escalation is
    /// not a yes, and the agent must stop and wait for the human.
    pub may_proceed: bool,
    pub hic: String,
    pub grant_id: Option<String>,
    pub reason: String,
    /// The decision's index in the tenant's evidence chain — the handle a human
    /// approves or rejects.
    pub decision_id: u64,
    /// The chain head this decision produced.
    pub chain_head: String,
    pub ungoverned: bool,
}

/// A minimal HTTP response, built by [`handle`] and written by [`serve`].
#[derive(Debug, PartialEq, Eq)]
pub struct Response {
    pub status: u16,
    pub body: String,
}

impl Response {
    fn json(status: u16, body: String) -> Self {
        Self { status, body }
    }
    fn error(status: u16, msg: &str) -> Self {
        Self::json(status, serde_json::json!({ "error": msg }).to_string())
    }
    fn reason(&self) -> &'static str {
        match self.status {
            200 => "OK",
            400 => "Bad Request",
            401 => "Unauthorized",
            403 => "Forbidden",
            404 => "Not Found",
            409 => "Conflict",
            413 => "Payload Too Large",
            _ => "Internal Server Error",
        }
    }
}

// ---- the pure handler -----------------------------------------------

/// Handle one already-read request. Pure over `(method, path, bearer, body)`
/// plus the backend, so the auth and gate properties are testable without a
/// socket.
///
/// `now_ms` is injected for the same reason the rest of the backend injects it.
pub fn handle(
    method: &str,
    path: &str,
    bearer: Option<&str>,
    body: &str,
    token: &BridgeToken,
    backend: &Mutex<QuorumBackend>,
    now_ms: i64,
) -> Response {
    // Liveness is open so a supervisor can tell "not up yet" from "wrong
    // token". It reveals nothing but that a process is listening.
    if method == "GET" && path == "/health" {
        return Response::json(200, r#"{"ok":true}"#.to_string());
    }

    // Everything else is bearer-gated, and the failure says nothing about why.
    let Some(presented) = bearer else {
        return Response::error(401, "bearer token required");
    };
    if !token.matches(presented) {
        return Response::error(401, "bearer token required");
    }

    if !(method == "POST" && path == "/intent") {
        return Response::error(404, "no such endpoint");
    }

    let intent: IntentRequest = match serde_json::from_str(body) {
        Ok(i) => i,
        Err(e) => return Response::error(400, &format!("malformed intent: {e}")),
    };

    // THE GATE. Runs before the agent acts, and records the decision whatever
    // the verdict — including `ungoverned`, which is never silently allowed and
    // never silently dropped.
    let mut guard = match backend.lock() {
        Ok(g) => g,
        Err(_) => return Response::error(500, "backend lock poisoned"),
    };
    let tenant =
        match guard.active_tenant_id() {
            Some(t) => t,
            None => return Response::error(
                409,
                "no active tenant scope — the operator must establish one before agents can act",
            ),
        };
    let input = ActionInput {
        agent: intent.agent,
        principal: intent.principal,
        class: intent.tool,
        classification: intent.classification,
        cost: intent.cost,
        hic1_cost_threshold: intent.hic1_cost_threshold,
        mandatory_hic1: intent.mandatory_hic1,
        params_hash: intent.params_hash,
        model_id: intent.model_id,
        correlation_id: intent.correlation_id,
    };
    let decision = match guard.evaluate_and_record(&input, &tenant, now_ms) {
        Ok(d) => d,
        Err(e) => return Response::error(400, &e),
    };

    let verdict = IntentVerdict {
        // Only a plain allow lets the agent act. An escalation is not a yes.
        may_proceed: decision.verdict == "allow",
        verdict: decision.verdict,
        hic: decision.hic,
        grant_id: decision.grant_id,
        reason: decision.reason,
        decision_id: decision.decision_id,
        chain_head: decision.chain_head,
        ungoverned: decision.ungoverned,
    };
    match serde_json::to_string(&verdict) {
        Ok(json) => Response::json(200, json),
        Err(e) => Response::error(500, &format!("could not encode verdict: {e}")),
    }
}

// ---- the socket shell -----------------------------------------------

/// A running bridge: the address it bound and where the token file lives, so
/// the operator (and the adapters) can find it.
pub struct RunningBridge {
    pub addr: SocketAddr,
    pub token_path: PathBuf,
}

/// Bind the loopback listener and serve intents until the process exits.
///
/// `port` of 0 asks the OS for an ephemeral port — the default, so two
/// installations on one machine do not collide. The chosen address is returned
/// before the loop starts.
pub fn serve(
    port: u16,
    token_path: PathBuf,
    backend: Arc<Mutex<QuorumBackend>>,
    token: Arc<BridgeToken>,
) -> std::io::Result<RunningBridge> {
    let listener = TcpListener::bind(SocketAddr::from((BRIDGE_HOST, port)))?;
    let addr = listener.local_addr()?;
    token.write_to(&token_path)?;

    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(stream) = stream else { continue };
            let backend = Arc::clone(&backend);
            let token = Arc::clone(&token);
            // One thread per connection: an agent adapter is a handful of
            // callers, not a public web server.
            std::thread::spawn(move || {
                let _ = serve_one(stream, &token, &backend);
            });
        }
    });

    Ok(RunningBridge { addr, token_path })
}

fn now_ms() -> i64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

fn serve_one(
    mut stream: TcpStream,
    token: &BridgeToken,
    backend: &Mutex<QuorumBackend>,
) -> std::io::Result<()> {
    let peer = stream.peer_addr()?;
    // Defence in depth: we bound loopback, so this should be unreachable —
    // but an agent intake must never serve a non-local caller.
    if !peer.ip().is_loopback() {
        return write_response(&mut stream, &Response::error(403, "loopback only"));
    }

    let mut reader = BufReader::new(stream.try_clone()?);
    let mut request_line = String::new();
    reader.read_line(&mut request_line)?;
    let mut parts = request_line.split_whitespace();
    let method = parts.next().unwrap_or_default().to_string();
    let path = parts.next().unwrap_or_default().to_string();

    let mut bearer: Option<String> = None;
    let mut content_length = 0usize;
    loop {
        let mut line = String::new();
        if reader.read_line(&mut line)? == 0 {
            break;
        }
        let line = line.trim_end();
        if line.is_empty() {
            break;
        }
        let lower = line.to_ascii_lowercase();
        if let Some(v) = lower.strip_prefix("authorization:") {
            let v = v.trim();
            if let Some(t) = v.strip_prefix("bearer ") {
                // Take the ORIGINAL casing for the token itself.
                let start = line.len() - t.len();
                bearer = Some(line[start..].trim().to_string());
            }
        } else if let Some(v) = lower.strip_prefix("content-length:") {
            content_length = v.trim().parse().unwrap_or(0);
        }
    }

    if content_length > MAX_BODY {
        return write_response(&mut stream, &Response::error(413, "intent too large"));
    }
    let mut body = vec![0u8; content_length];
    if content_length > 0 {
        reader.read_exact(&mut body)?;
    }
    let body = String::from_utf8_lossy(&body).to_string();

    let response = handle(
        &method,
        &path,
        bearer.as_deref(),
        &body,
        token,
        backend,
        now_ms(),
    );
    write_response(&mut stream, &response)
}

fn write_response(stream: &mut TcpStream, r: &Response) -> std::io::Result<()> {
    let out = format!(
        "HTTP/1.1 {} {}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        r.status,
        r.reason(),
        r.body.len(),
        r.body
    );
    stream.write_all(out.as_bytes())?;
    stream.flush()
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use crate::backend::GrantInput;

    fn backend_with_grant() -> Mutex<QuorumBackend> {
        let mut b = QuorumBackend::default();
        b.set_active_tenant("bca").unwrap();
        b.issue_grant(
            &GrantInput {
                id: "G-1".into(),
                agent: "claude-code".into(),
                principal: "R. Ortiz".into(),
                tenant_scope: "t3:bca".into(),
                action_classes: vec!["repo.write".into(), "spend".into()],
                classification_ceiling: "CUI".into(),
                budget_units: 1000,
                expires_at_ms: i64::MAX,
                hic: "2".into(),
            },
            &quorum_tenancy::TenantId::new("bca").unwrap(),
        )
        .unwrap();
        Mutex::new(b)
    }

    fn intent_json(tool: &str, cost: u64) -> String {
        serde_json::json!({
            "agent": "claude-code",
            "principal": "R. Ortiz",
            "tool": tool,
            "classification": "Public",
            "cost": cost,
            "hic1_cost_threshold": 150,
            "correlation_id": "X-7104",
        })
        .to_string()
    }

    fn post(
        bearer: Option<&str>,
        body: &str,
        token: &BridgeToken,
        b: &Mutex<QuorumBackend>,
    ) -> Response {
        handle("POST", "/intent", bearer, body, token, b, 1000)
    }

    #[test]
    fn health_is_open_but_says_nothing() {
        let t = BridgeToken::mint();
        let b = backend_with_grant();
        let r = handle("GET", "/health", None, "", &t, &b, 1000);
        assert_eq!(r.status, 200);
        assert_eq!(r.body, r#"{"ok":true}"#);
    }

    #[test]
    fn an_intent_without_a_bearer_is_refused() {
        let t = BridgeToken::mint();
        let b = backend_with_grant();
        let r = post(None, &intent_json("repo.write", 1), &t, &b);
        assert_eq!(r.status, 401);
        // Nothing was recorded: an unauthenticated caller is not an agent.
        assert_eq!(b.lock().unwrap().ledger_rows("bca").len(), 0);
    }

    #[test]
    fn a_wrong_bearer_is_refused_and_the_error_reveals_nothing() {
        let t = BridgeToken::mint();
        let b = backend_with_grant();
        let r = post(Some("deadbeef"), &intent_json("repo.write", 1), &t, &b);
        assert_eq!(r.status, 401);
        assert_eq!(r.body, r#"{"error":"bearer token required"}"#);
    }

    #[test]
    fn the_token_compares_in_constant_time_and_rejects_near_misses() {
        let t = BridgeToken::mint();
        let mut wrong = format!("{:?}", t); // the Debug render is redacted…
        assert!(
            wrong.contains("redacted"),
            "token must not be Debug-printable"
        );
        wrong = "0".repeat(64);
        assert!(!t.matches(&wrong));
        assert!(!t.matches(""));
        assert!(!t.matches("short"));
    }

    #[test]
    fn a_governed_tool_call_is_allowed_and_recorded_before_it_runs() {
        let t = BridgeToken::mint();
        let b = backend_with_grant();
        let r = handle(
            "POST",
            "/intent",
            Some(current(&t).as_str()),
            &intent_json("repo.write", 1),
            &t,
            &b,
            1000,
        );
        assert_eq!(r.status, 200);
        let v: serde_json::Value = serde_json::from_str(&r.body).unwrap();
        assert_eq!(v["verdict"], "allow");
        assert_eq!(v["may_proceed"], true);
        assert_eq!(v["grant_id"], "G-1");
        // Recorded BEFORE the agent acts — the record exists already.
        assert_eq!(b.lock().unwrap().ledger_rows("bca").len(), 1);
    }

    /// NOTE: the grant's budget must actually cover the spend, or `covers()`
    /// finds no live grant and the verdict is `ungoverned` — a different (also
    /// correct) outcome. That distinction is what this fixture pins.
    #[test]
    fn an_escalation_is_not_permission_to_proceed() {
        let t = BridgeToken::mint();
        let b = backend_with_grant();
        // 220 > the 150 threshold → HIC-1.
        let r = post(
            Some(current(&t).as_str()),
            &intent_json("spend", 220),
            &t,
            &b,
        );
        let v: serde_json::Value = serde_json::from_str(&r.body).unwrap();
        assert_eq!(v["verdict"], "require-approval");
        assert_eq!(
            v["may_proceed"], false,
            "an escalation must stop the agent, not wave it through"
        );
        assert!(v["decision_id"].is_number(), "the human needs a handle");
    }

    #[test]
    fn an_ungranted_agent_is_recorded_ungoverned_not_errored_away() {
        let t = BridgeToken::mint();
        let b = backend_with_grant();
        let body = serde_json::json!({
            "agent": "rogue-agent",
            "tool": "repo.write",
            "classification": "Public",
        })
        .to_string();
        let r = post(Some(current(&t).as_str()), &body, &t, &b);
        assert_eq!(r.status, 200);
        let v: serde_json::Value = serde_json::from_str(&r.body).unwrap();
        assert_eq!(v["verdict"], "ungoverned");
        assert_eq!(v["may_proceed"], false);
        assert_eq!(v["ungoverned"], true);
        assert_eq!(
            b.lock().unwrap().ledger_ungoverned_count("bca"),
            1,
            "an ungoverned attempt is evidence, not a 4xx"
        );
    }

    #[test]
    fn no_response_can_carry_key_or_signature_material() {
        let t = BridgeToken::mint();
        let b = backend_with_grant();
        for body in [
            intent_json("repo.write", 1),
            intent_json("spend", 220),
            "{\"agent\":\"x\",\"tool\":\"y\",\"classification\":\"Public\"}".to_string(),
        ] {
            let r = post(Some(current(&t).as_str()), &body, &t, &b);
            let lower = r.body.to_ascii_lowercase();
            for banned in [
                "signature",
                "privkey",
                "private_key",
                "seed",
                "mnemonic",
                "0x30450",
            ] {
                assert!(
                    !lower.contains(banned),
                    "an agent response must never carry {banned}: {}",
                    r.body
                );
            }
        }
    }

    #[test]
    fn unknown_endpoints_are_404_even_with_a_valid_token() {
        let t = BridgeToken::mint();
        let b = backend_with_grant();
        let r = handle(
            "POST",
            "/sign",
            Some(current(&t).as_str()),
            "{}",
            &t,
            &b,
            1000,
        );
        assert_eq!(
            r.status, 404,
            "there is no signing endpoint, and never will be"
        );
    }

    #[test]
    fn a_malformed_intent_is_a_400_and_records_nothing() {
        let t = BridgeToken::mint();
        let b = backend_with_grant();
        let r = post(Some(current(&t).as_str()), "not json", &t, &b);
        assert_eq!(r.status, 400);
        assert_eq!(b.lock().unwrap().ledger_rows("bca").len(), 0);
    }

    #[test]
    fn with_no_tenant_scope_agents_cannot_act_at_all() {
        let t = BridgeToken::mint();
        let b = Mutex::new(QuorumBackend::default()); // no scope established
        let r = post(
            Some(current(&t).as_str()),
            &intent_json("repo.write", 1),
            &t,
            &b,
        );
        assert_eq!(r.status, 409);
        assert!(r.body.contains("no active tenant scope"));
    }

    #[test]
    fn the_token_file_is_written_0600() {
        let t = BridgeToken::mint();
        let dir = std::env::temp_dir().join(format!("quorum-bridge-tok-{}", std::process::id()));
        let path = dir.join("agent-token");
        t.write_to(&path).unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(&path).unwrap().permissions().mode();
            assert_eq!(
                mode & 0o777,
                0o600,
                "the token file must not be readable by others"
            );
        }
        let written = std::fs::read_to_string(&path).unwrap();
        assert!(t.matches(&written), "the file carries the live token");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_bridge_binds_loopback_only() {
        assert!(BRIDGE_HOST.is_loopback());
        let backend = Arc::new(backend_with_grant());
        let token = Arc::new(BridgeToken::mint());
        let dir = std::env::temp_dir().join(format!("quorum-bridge-{}", std::process::id()));
        let running = serve(0, dir.join("agent-token"), backend, token).unwrap();
        assert!(
            running.addr.ip().is_loopback(),
            "bound {} — an agent intake must never be reachable off-box",
            running.addr
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Read the live token back out of a `BridgeToken` for test requests. There
    /// is deliberately no public accessor — production hands the secret to the
    /// child through the `0600` file, never through an API.
    fn current(t: &BridgeToken) -> String {
        let dir = std::env::temp_dir().join(format!(
            "quorum-bridge-cur-{}-{:p}",
            std::process::id(),
            t as *const _
        ));
        let path = dir.join("tok");
        t.write_to(&path).unwrap();
        let s = std::fs::read_to_string(&path).unwrap();
        let _ = std::fs::remove_dir_all(&dir);
        s
    }
}
