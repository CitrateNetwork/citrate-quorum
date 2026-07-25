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
         \x20 gate --agent <id> --tool <class> [--classification C] [--cost N]\n\
         \x20      [--threshold N] [--must-approve] [--correlation X] [--wait S]\n\n\
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
