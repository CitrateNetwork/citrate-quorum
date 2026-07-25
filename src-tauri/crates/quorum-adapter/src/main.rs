//! citrate-quorum — `quorum-adapter` (WP-S4a.1).
//!
//! The binary a vendor agent runs before it acts. Three ways in, one gate:
//!
//! ```text
//!   quorum-adapter status
//!       Is Quorum set up here, and can we reach it? Diagnostics only.
//!
//!   quorum-adapter claude-hook
//!       A Claude Code PreToolUse hook. Reads the tool call on stdin, gates it,
//!       and denies with a reason the model can act on. Install with
//!       `quorum-adapter install-claude-hook`.
//!
//!   quorum-adapter gate --agent <id> --tool <class> [...]
//!       The generic path, for Codex, Hermes, scripts and CI. Exit 0 = proceed,
//!       exit 1 = refused (reason on stderr), exit 2 = usage error.
//! ```
//!
//! No subcommand ever prints a key, a token or a signature: it submits an
//! unsigned intent and reports a verdict. The signing path is the human
//! ceremony in the desktop app, which this binary cannot reach.

use std::io::Read;
use std::time::Duration;

use quorum_adapter::{
    action_class, claude_hook_response, discover, gate_call, params_hash, Discovery, Gate,
    HttpGate, Intent, Outcome, PreToolUse,
};

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let cmd = args.first().map(String::as_str).unwrap_or("help");
    let code = match cmd {
        "status" => cmd_status(),
        "claude-hook" => cmd_claude_hook(&args[1..]),
        "install-claude-hook" => cmd_install_claude_hook(),
        "gate" => cmd_gate(&args[1..]),
        "mcp-proxy" => cmd_mcp_proxy(&args[1..]),
        "mcp-proxy-http" => cmd_mcp_proxy_http(&args[1..]),
        "codex-hook" => cmd_codex_hook(&args[1..]),
        "install-codex-hook" => cmd_install_codex_hook(),
        "help" | "-h" | "--help" => {
            print_help();
            0
        }
        other => {
            eprintln!("quorum-adapter: unknown command `{other}`\n");
            print_help();
            2
        }
    };
    std::process::exit(code);
}

fn print_help() {
    eprintln!(
        "quorum-adapter — gate an agent's tool calls through Citrate Quorum.\n\n\
         \x20 status                 is Quorum set up and reachable here?\n\
         \x20 claude-hook            Claude Code PreToolUse hook (reads stdin)\n\
         \x20 install-claude-hook    print the hooks.json to install it\n\
         \x20 codex-hook             Codex CLI PreToolUse hook (reads stdin)\n\
         \x20 install-codex-hook     print the hooks.json to install it\n\
         \x20 gate --agent <id> --tool <class> [--classification C] [--cost N]\n\
         \x20      [--threshold N] [--must-approve] [--correlation X] [--wait S]\n\
         \x20 mcp-proxy [--agent id] [--classification C] [--model M] [--wait S] -- <server cmd...>\n\
         \x20      gate an MCP server's tools/call; everything else relays untouched\n\
         \x20 mcp-proxy-http --listen ADDR --upstream HOST:PORT [--agent id] [...]\n\
         \x20      the same gate for the Streamable HTTP transport\n\n\
         Exit codes: 0 proceed · 1 refused · 2 usage error."
    );
}

fn open_gate() -> Result<Option<HttpGate>, String> {
    match discover(None) {
        Discovery::Ready { addr, token } => Ok(Some(HttpGate { addr, token })),
        Discovery::NotInstalled => Ok(None),
        Discovery::Broken(why) => Err(why),
    }
}

fn cmd_status() -> i32 {
    match discover(None) {
        Discovery::NotInstalled => {
            println!("not installed — no Quorum endpoint on this machine; agents run ungoverned");
            0
        }
        Discovery::Broken(why) => {
            println!("BROKEN — Quorum is set up here but unusable: {why}");
            println!("every gated tool call will be REFUSED until this is fixed");
            1
        }
        Discovery::Ready { addr, token } => {
            let gate = HttpGate {
                addr: addr.clone(),
                token,
            };
            // A harmless read proves reachability without recording anything.
            match gate.status(u64::MAX) {
                Ok(_) => {
                    println!("ready — gate at {addr}");
                    0
                }
                Err(e) if e.contains("404") || e.contains("no such decision") => {
                    println!("ready — gate at {addr}");
                    0
                }
                Err(e) => {
                    println!("UNREACHABLE — endpoint {addr} is configured but not answering: {e}");
                    println!("every gated tool call will be REFUSED until Quorum is running");
                    1
                }
            }
        }
    }
}

/// The Claude Code PreToolUse hook.
///
/// Exit 0 with no output = the hook has no objection. Exit 0 with the deny JSON
/// = blocked, with a reason shown to the model. We never exit non-zero on a
/// refusal: the JSON is the channel that carries the reason, and a bare failure
/// would read as "the hook broke" rather than "this was not allowed".
fn cmd_claude_hook(args: &[String]) -> i32 {
    let wait = flag_value(args, "--wait")
        .and_then(|s| s.parse::<u64>().ok())
        .map(Duration::from_secs)
        .unwrap_or(Duration::ZERO);

    let mut raw = String::new();
    if std::io::stdin().read_to_string(&mut raw).is_err() {
        return 0; // Cannot read the call; do not pretend to have judged it.
    }
    let call: PreToolUse = serde_json::from_str(&raw).unwrap_or_default();
    if call.tool_name.is_empty() {
        return 0;
    }

    let gate = match open_gate() {
        Ok(Some(g)) => g,
        Ok(None) => return 0, // Quorum not set up here: stand aside.
        Err(why) => {
            // Configured but unusable — refuse, and say so in the reason.
            let out = claude_hook_response(&Outcome::Refuse {
                reason: format!(
                    "Quorum is installed here but unusable ({why}), so this action has not run."
                ),
            });
            if let Some(json) = out {
                println!("{json}");
            }
            return 0;
        }
    };

    let intent = Intent {
        agent: std::env::var("QUORUM_AGENT_ID").unwrap_or_else(|_| "claude-code".into()),
        principal: std::env::var("QUORUM_PRINCIPAL").ok(),
        tool: action_class(&call.tool_name),
        classification: std::env::var("QUORUM_CLASSIFICATION").unwrap_or_else(|_| "Public".into()),
        cost: 0,
        hic1_cost_threshold: 0,
        mandatory_hic1: false,
        params_hash: params_hash(&call.tool_input),
        model_id: std::env::var("QUORUM_MODEL_ID").unwrap_or_default(),
        correlation_id: call.session_id.clone(),
    };

    let outcome = gate_call(&gate, &intent, wait);
    if let Some(json) = claude_hook_response(&outcome) {
        println!("{json}");
    }
    0
}

/// Print the hooks.json that installs the hook, rather than editing the user's
/// settings behind their back. Governance you did not consent to is not
/// governance.
fn cmd_install_claude_hook() -> i32 {
    let exe = std::env::current_exe()
        .map(|p| p.display().to_string())
        .unwrap_or_else(|_| "quorum-adapter".into());
    println!(
        "{}",
        serde_json::json!({
            "hooks": {
                "PreToolUse": [{
                    "matcher": "*",
                    "hooks": [{ "type": "command", "command": format!("{exe} claude-hook") }]
                }]
            }
        })
    );
    eprintln!(
        "\nAdd the above to ~/.claude/settings.json (or a plugin's hooks.json).\n\
         Every tool call then passes the Quorum gate before it runs.\n\
         Narrow `matcher` to gate only some tools — e.g. \"Bash|Write|Edit\"."
    );
    0
}

fn cmd_gate(args: &[String]) -> i32 {
    let Some(agent) = flag_value(args, "--agent") else {
        eprintln!("quorum-adapter gate: --agent is required");
        return 2;
    };
    let Some(tool) = flag_value(args, "--tool") else {
        eprintln!("quorum-adapter gate: --tool is required");
        return 2;
    };
    let wait = flag_value(args, "--wait")
        .and_then(|s| s.parse::<u64>().ok())
        .map(Duration::from_secs)
        .unwrap_or(Duration::ZERO);

    let gate = match open_gate() {
        Ok(Some(g)) => g,
        Ok(None) => {
            eprintln!("quorum-adapter: not installed here — running ungoverned");
            return 0;
        }
        Err(why) => {
            eprintln!("quorum-adapter: REFUSED — Quorum is installed but unusable: {why}");
            return 1;
        }
    };

    let intent = Intent {
        agent,
        principal: flag_value(args, "--principal"),
        tool,
        classification: flag_value(args, "--classification").unwrap_or_else(|| "Public".into()),
        cost: flag_value(args, "--cost")
            .and_then(|s| s.parse().ok())
            .unwrap_or(0),
        hic1_cost_threshold: flag_value(args, "--threshold")
            .and_then(|s| s.parse().ok())
            .unwrap_or(0),
        mandatory_hic1: args.iter().any(|a| a == "--must-approve"),
        params_hash: flag_value(args, "--params-hash").unwrap_or_default(),
        model_id: flag_value(args, "--model").unwrap_or_default(),
        correlation_id: flag_value(args, "--correlation").unwrap_or_default(),
    };

    match gate_call(&gate, &intent, wait) {
        Outcome::Proceed { reason } => {
            eprintln!("proceed — {reason}");
            0
        }
        Outcome::Refuse { reason } => {
            eprintln!("REFUSED — {reason}");
            1
        }
        Outcome::NotGoverned => 0,
    }
}

fn flag_value(args: &[String], flag: &str) -> Option<String> {
    let i = args.iter().position(|a| a == flag)?;
    args.get(i + 1).cloned()
}

/// `quorum-adapter mcp-proxy [flags] -- <server command...>`
///
/// Spawns the upstream MCP server and relays stdio between it and the client,
/// gating every `tools/call` on the way past. Everything else is forwarded
/// unread, which is what keeps this working across protocol revisions — MCP
/// 2026-07-28 removes the initialize handshake, sessions, ping and
/// logging/setLevel, and a proxy that modelled the protocol would break.
///
/// The server's stderr is inherited, not captured: the same revision tells
/// stdio servers to log there instead of using the (now deprecated) Logging
/// feature, so it must reach the operator's terminal.
fn cmd_mcp_proxy(args: &[String]) -> i32 {
    use quorum_adapter::mcp::{refusal_response, relay_decision, ClientIdentity, Relay};
    use std::io::{BufRead, BufReader, Write};
    use std::process::{Command, Stdio};
    use std::sync::{Arc, Mutex};

    let Some(sep) = args.iter().position(|a| a == "--") else {
        eprintln!("quorum-adapter mcp-proxy: expected `-- <server command...>`");
        return 2;
    };
    let (flags, server) = (&args[..sep], &args[sep + 1..]);
    let Some((program, rest)) = server.split_first() else {
        eprintln!("quorum-adapter mcp-proxy: no server command after `--`");
        return 2;
    };
    let agent = flag_value(flags, "--agent");
    let classification =
        flag_value(flags, "--classification").unwrap_or_else(|| "Public".to_string());
    // Q10: at a controlled classification an unnamed model endpoint is denied,
    // so this is a governance input, not a label.
    let model = flag_value(flags, "--model").unwrap_or_default();
    let wait = flag_value(flags, "--wait")
        .and_then(|s| s.parse::<u64>().ok())
        .map(Duration::from_secs)
        .unwrap_or(Duration::ZERO);

    let gate = match open_gate() {
        Ok(Some(g)) => Some(g),
        Ok(None) => {
            eprintln!("quorum-adapter: Quorum is not set up here — proxying UNGOVERNED");
            None
        }
        Err(why) => {
            eprintln!("quorum-adapter: REFUSED to start — Quorum is installed but unusable: {why}");
            return 1;
        }
    };

    let mut child = match Command::new(program)
        .args(rest)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()
    {
        Ok(c) => c,
        Err(e) => {
            eprintln!("quorum-adapter mcp-proxy: cannot start `{program}`: {e}");
            return 2;
        }
    };
    let Some(mut to_server) = child.stdin.take() else {
        return 2;
    };
    let Some(from_server) = child.stdout.take() else {
        return 2;
    };

    // Server → client: pure relay. One writer owns stdout so a refusal we
    // synthesize can never interleave with a server frame mid-line.
    let out = Arc::new(Mutex::new(std::io::stdout()));
    let out_relay = Arc::clone(&out);
    let pump = std::thread::spawn(move || {
        let mut reader = BufReader::new(from_server);
        let mut line = String::new();
        loop {
            line.clear();
            match reader.read_line(&mut line) {
                Ok(0) | Err(_) => break,
                Ok(_) => {}
            }
            if let Ok(mut o) = out_relay.lock() {
                if o.write_all(line.as_bytes()).is_err() || o.flush().is_err() {
                    break;
                }
            }
        }
    });

    // Client → server: gate `tools/call`, forward the rest.
    let mut identity = ClientIdentity::default();
    let stdin = std::io::stdin();
    let mut line = String::new();
    loop {
        line.clear();
        match stdin.lock().read_line(&mut line) {
            Ok(0) | Err(_) => break,
            Ok(_) => {}
        }
        if line.trim().is_empty() {
            continue;
        }
        let Ok(msg) = serde_json::from_str::<serde_json::Value>(&line) else {
            // Not JSON we can read. Forward it: mangling a frame we do not
            // understand is worse than letting the server reject it.
            let _ = to_server.write_all(line.as_bytes());
            let _ = to_server.flush();
            continue;
        };
        identity.observe(&msg);

        let decision = match &gate {
            Some(g) => relay_decision(
                &msg,
                &identity,
                g,
                agent.as_deref(),
                &classification,
                &model,
                wait,
            ),
            None => Relay::Forward,
        };
        match decision {
            Relay::Forward => {
                if to_server.write_all(line.as_bytes()).is_err() || to_server.flush().is_err() {
                    break;
                }
            }
            Relay::Refuse(reason) => {
                let body = refusal_response(&msg, &reason).to_string();
                if let Ok(mut o) = out.lock() {
                    let _ = o.write_all(body.as_bytes());
                    let _ = o.write_all(b"\n");
                    let _ = o.flush();
                }
            }
        }
    }

    drop(to_server);
    let _ = child.wait();
    let _ = pump.join();
    0
}

/// The Codex CLI PreToolUse hook.
///
/// Codex shares Claude Code's payload field names (`tool_name`, `tool_input`,
/// `session_id`, `cwd`), so the gating is identical. It differs in one respect:
/// published accounts of its parser disagree about the response shape, and its
/// documentation offers `exit 2` + stderr as an alternative denial channel. On
/// a refusal we use both; on an allow, neither.
///
/// Enable with `[features].codex_hooks = true` in `~/.codex/config.toml` and
/// register in `~/.codex/hooks.json` — `quorum-adapter install-codex-hook`.
fn cmd_codex_hook(args: &[String]) -> i32 {
    use quorum_adapter::{hook_exit_code, hook_response, HookVendor};

    let wait = flag_value(args, "--wait")
        .and_then(|s| s.parse::<u64>().ok())
        .map(Duration::from_secs)
        .unwrap_or(Duration::ZERO);

    let mut raw = String::new();
    if std::io::stdin().read_to_string(&mut raw).is_err() {
        return 0;
    }
    let call: PreToolUse = serde_json::from_str(&raw).unwrap_or_default();
    if call.tool_name.is_empty() {
        return 0;
    }

    let gate = match open_gate() {
        Ok(Some(g)) => g,
        Ok(None) => return 0,
        Err(why) => {
            let outcome = Outcome::Refuse {
                reason: format!(
                    "Quorum is installed here but unusable ({why}), so this action has not run."
                ),
            };
            if let Some(json) = hook_response(&outcome) {
                println!("{json}");
            }
            eprintln!("Quorum is installed here but unusable ({why}); this action has not run.");
            return hook_exit_code(HookVendor::Codex, &outcome);
        }
    };

    let intent = Intent {
        agent: std::env::var("QUORUM_AGENT_ID").unwrap_or_else(|_| "codex".into()),
        principal: std::env::var("QUORUM_PRINCIPAL").ok(),
        tool: action_class(&call.tool_name),
        classification: std::env::var("QUORUM_CLASSIFICATION").unwrap_or_else(|_| "Public".into()),
        cost: 0,
        hic1_cost_threshold: 0,
        mandatory_hic1: false,
        params_hash: params_hash(&call.tool_input),
        model_id: std::env::var("QUORUM_MODEL_ID").unwrap_or_default(),
        correlation_id: call.session_id.clone(),
    };

    let outcome = gate_call(&gate, &intent, wait);
    if let Some(json) = hook_response(&outcome) {
        println!("{json}");
    }
    if let Outcome::Refuse { reason } = &outcome {
        // The second denial channel Codex documents.
        eprintln!("{reason}");
    }
    hook_exit_code(HookVendor::Codex, &outcome)
}

fn cmd_install_codex_hook() -> i32 {
    let exe = std::env::current_exe()
        .map(|p| p.display().to_string())
        .unwrap_or_else(|_| "quorum-adapter".into());
    println!(
        "{}",
        serde_json::json!({
            "hooks": {
                "PreToolUse": [{ "command": format!("{exe} codex-hook") }]
            }
        })
    );
    eprintln!(
        "\nAdd the above to ~/.codex/hooks.json, and enable hooks with\n\
         \x20   [features]\n\
         \x20   codex_hooks = true\n\
         in ~/.codex/config.toml (they are off by default and are a no-op without it).\n\n\
         COVERAGE: published accounts disagree on which tools fire PreToolUse on which\n\
         Codex version — some say shell only, others include apply_patch and MCP calls.\n\
         Do not assume. The Quorum ledger answers it for YOUR version: if you only ever\n\
         see shell.exec decisions, the rest is not reaching the hook. Govern Codex's MCP\n\
         tool calls with `quorum-adapter mcp-proxy` regardless — that path does not\n\
         depend on the hook at all."
    );
    0
}

/// `quorum-adapter mcp-proxy-http --listen ADDR --upstream URL`
///
/// The Streamable HTTP counterpart of `mcp-proxy`. A `tools/call` is gated
/// before it is forwarded; everything else is relayed.
///
/// The response is copied through as OPAQUE BYTES — headers and body, whether
/// that body is a single JSON document or an SSE stream. The proxy never parses
/// a response, so `resultType`, MRTR's `input_required`, extensions and
/// anything the protocol grows later pass through untouched.
///
/// It caches nothing, which satisfies `cacheScope: "private"` by construction:
/// a shared intermediary that never stores a response cannot leak one.
fn cmd_mcp_proxy_http(args: &[String]) -> i32 {
    use quorum_adapter::mcp::{
        is_hop_by_hop, parse_headers, refusal_response, relay_decision, ClientIdentity, Relay,
    };
    use std::io::{BufRead, BufReader, Read, Write};
    use std::net::{TcpListener, TcpStream};

    let Some(listen) = flag_value(args, "--listen") else {
        eprintln!("quorum-adapter mcp-proxy-http: --listen <addr> is required");
        return 2;
    };
    let Some(upstream) = flag_value(args, "--upstream") else {
        eprintln!("quorum-adapter mcp-proxy-http: --upstream <host:port> is required");
        return 2;
    };
    let agent = flag_value(args, "--agent");
    let classification =
        flag_value(args, "--classification").unwrap_or_else(|| "Public".to_string());
    let model = flag_value(args, "--model").unwrap_or_default();
    let wait = flag_value(args, "--wait")
        .and_then(|s| s.parse::<u64>().ok())
        .map(Duration::from_secs)
        .unwrap_or(Duration::ZERO);

    let gate = match open_gate() {
        Ok(Some(g)) => Some(g),
        Ok(None) => {
            eprintln!("quorum-adapter: Quorum is not set up here — proxying UNGOVERNED");
            None
        }
        Err(why) => {
            eprintln!("quorum-adapter: REFUSED to start — Quorum is installed but unusable: {why}");
            return 1;
        }
    };

    let listener = match TcpListener::bind(&listen) {
        Ok(l) => l,
        Err(e) => {
            eprintln!("quorum-adapter mcp-proxy-http: cannot bind {listen}: {e}");
            return 2;
        }
    };
    eprintln!("quorum-adapter: gating {listen} -> {upstream}");

    for conn in listener.incoming() {
        let Ok(mut client) = conn else { continue };
        let mut reader = BufReader::new(match client.try_clone() {
            Ok(c) => c,
            Err(_) => continue,
        });

        // Request line + headers.
        let mut head = String::new();
        loop {
            let mut line = String::new();
            match reader.read_line(&mut line) {
                Ok(0) | Err(_) => break,
                Ok(_) => {}
            }
            if line == "\r\n" || line == "\n" {
                break;
            }
            head.push_str(&line);
        }
        if head.is_empty() {
            continue;
        }
        let request_line = head.lines().next().unwrap_or_default().to_string();
        let headers = parse_headers(&head);
        let len: usize = headers
            .iter()
            .find(|(k, _)| k == "content-length")
            .and_then(|(_, v)| v.parse().ok())
            .unwrap_or(0);
        let mut body = vec![0u8; len];
        if len > 0 && reader.read_exact(&mut body).is_err() {
            continue;
        }
        let parsed: serde_json::Value =
            serde_json::from_slice(&body).unwrap_or(serde_json::Value::Null);

        // Gate, if this is a tool call. `should_gate` checks the routing header
        // AND the body, so omitting the header does not skip the gate.
        if let Some(g) = &gate {
            if quorum_adapter::mcp::should_gate(&headers, &parsed) {
                let mut identity = ClientIdentity::default();
                identity.observe(&parsed);
                if let Relay::Refuse(reason) = relay_decision(
                    &parsed,
                    &identity,
                    g,
                    agent.as_deref(),
                    &classification,
                    &model,
                    wait,
                ) {
                    // A JSON-RPC error rides on HTTP 200: the transport
                    // succeeded, the call was declined.
                    let payload = refusal_response(&parsed, &reason).to_string();
                    let resp = format!(
                        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\n\
                         Content-Length: {}\r\nConnection: close\r\n\r\n{payload}",
                        payload.len()
                    );
                    let _ = client.write_all(resp.as_bytes());
                    let _ = client.flush();
                    continue;
                }
            }
        }

        // Forward, then copy the response back as opaque bytes.
        let Ok(mut up) = TcpStream::connect(&upstream) else {
            let msg = "HTTP/1.1 502 Bad Gateway\r\nContent-Length: 0\r\nConnection: close\r\n\r\n";
            let _ = client.write_all(msg.as_bytes());
            continue;
        };
        let mut out = format!("{request_line}\r\nHost: {upstream}\r\n");
        for (k, v) in &headers {
            if !is_hop_by_hop(k) {
                out.push_str(&format!("{k}: {v}\r\n"));
            }
        }
        out.push_str(&format!(
            "Content-Length: {}\r\nConnection: close\r\n\r\n",
            body.len()
        ));
        if up.write_all(out.as_bytes()).is_err() || up.write_all(&body).is_err() {
            continue;
        }
        let _ = up.flush();
        // Byte-for-byte, so SSE and chunked bodies stream through unparsed.
        let mut buf = [0u8; 8192];
        loop {
            match up.read(&mut buf) {
                Ok(0) | Err(_) => break,
                Ok(n) => {
                    if client.write_all(&buf[..n]).is_err() || client.flush().is_err() {
                        break;
                    }
                }
            }
        }
    }
    0
}
