//! citrate-quorum — the MCP gating proxy (WP-S4a.2).
//!
//! Quorum sits between an MCP client and an MCP server. Every `tools/call`
//! passes the policy gate **before** it reaches the server; everything else is
//! relayed byte-for-byte.
//!
//! ## Why it only understands `tools/call`
//!
//! MCP `2026-07-28` — released four days after this was written — is the
//! largest revision since launch. It removes the `initialize` handshake and
//! protocol-level sessions, drops `Mcp-Session-Id`, removes `ping` and
//! `logging/setLevel`, replaces server-initiated requests with Multi
//! Round-Trip Requests, adds a required `resultType` on every result, and
//! renumbers error codes. A proxy that modelled the protocol would break on the
//! 28th.
//!
//! This one is deliberately near-blind: it inspects exactly one method,
//! `tools/call`, whose shape (`params.name`, `params.arguments`) is stable
//! across every revision from `2024-11-05` to the draft. Everything else —
//! `server/discover`, `subscriptions/listen`, MRTR's `input_required` results,
//! extensions, methods that do not exist yet — is opaque and forwarded intact.
//! The proxy is protocol-era agnostic because it declines to have an opinion.
//!
//! ## What the new spec gives us
//!
//! - **Statelessness is a gift.** With no session to track, each `tools/call`
//!   is independently gateable and the proxy holds no per-connection state it
//!   could get wrong.
//! - **Identity travels on the request.** `_meta`'s
//!   `io.modelcontextprotocol/clientInfo` names the calling agent, so evidence
//!   records who called rather than a value we were configured to assume. On
//!   pre-2026-07-28 servers we fall back to the `initialize` handshake's
//!   `clientInfo`, which is the only place it appears in that era.
//! - **W3C trace context.** `_meta`'s `traceparent` becomes the decision's
//!   correlation id, so a governed action lines up with the same trace an
//!   OpenTelemetry backend sees.
//!
//! ## Error codes
//!
//! The revision partitions the JSON-RPC server-error range: `-32020..-32099` is
//! now **reserved for the specification**, and `-32000..-32019` stays
//! implementation-defined. A refusal from Quorum is an implementation concern,
//! so it uses [`REFUSED_CODE`] from the lower band. Using anything at or above
//! `-32020` would be squatting on space the spec has claimed.

use serde_json::{json, Map, Value};

use crate::{action_class, gate_call, params_hash, Gate, Intent, Outcome};

/// Quorum's "this action was refused" code.
///
/// In the implementation-defined band (`-32000..-32019`) per the 2026-07-28
/// error-code allocation policy. It is NOT a protocol error: the message is
/// well-formed and understood, and a human or a policy declined it.
pub const REFUSED_CODE: i64 = -32010;

/// What the proxy decided to do with one message from the client.
#[derive(Debug, PartialEq, Eq)]
pub enum Relay {
    /// Send it upstream unchanged.
    Forward,
    /// Do not send it. Answer the client with this JSON-RPC error instead.
    Refuse(String),
}

/// Identity the proxy has learned about the client it is proxying for.
#[derive(Default, Debug, Clone)]
pub struct ClientIdentity {
    /// From `_meta`'s `io.modelcontextprotocol/clientInfo` (2026-07-28+), or
    /// the `initialize` handshake on earlier revisions.
    pub name: Option<String>,
    /// The protocol era this client is speaking, when it says so.
    pub protocol_version: Option<String>,
}

impl ClientIdentity {
    /// Learn what a message tells us. Called for every client message; on
    /// pre-2026-07-28 revisions the only place `clientInfo` appears is the
    /// `initialize` request, which we observe rather than depend on.
    pub fn observe(&mut self, msg: &Value) {
        if let Some(meta) = msg.pointer("/params/_meta") {
            if let Some(v) = meta
                .get("io.modelcontextprotocol/protocolVersion")
                .and_then(Value::as_str)
            {
                self.protocol_version = Some(v.to_string());
            }
            if let Some(n) = meta
                .pointer("/io.modelcontextprotocol~1clientInfo/name")
                .and_then(Value::as_str)
            {
                self.name = Some(n.to_string());
            }
        }
        // Pre-2026-07-28: the handshake carried it. Harmless to keep reading —
        // a client that still sends `initialize` is telling us the truth about
        // itself, and a client that does not simply never triggers this.
        if msg.get("method").and_then(Value::as_str) == Some("initialize") {
            if let Some(n) = msg
                .pointer("/params/clientInfo/name")
                .and_then(Value::as_str)
            {
                self.name = Some(n.to_string());
            }
            if let Some(v) = msg
                .pointer("/params/protocolVersion")
                .and_then(Value::as_str)
            {
                self.protocol_version = Some(v.to_string());
            }
        }
    }
}

/// Is this message a tool call we must gate?
pub fn is_tool_call(msg: &Value) -> bool {
    msg.get("method").and_then(Value::as_str) == Some("tools/call")
        // A notification has no id and expects no response; `tools/call` is a
        // request, so anything without an id is not one.
        && msg.get("id").is_some()
}

/// The correlation id for a tool call: the W3C trace context when the client
/// propagates one (2026-07-28 documents `traceparent` in `_meta`), else the
/// JSON-RPC request id, so a decision can always be tied back to its call.
pub fn correlation_of(msg: &Value) -> String {
    msg.pointer("/params/_meta/traceparent")
        .and_then(Value::as_str)
        .map(str::to_string)
        .unwrap_or_else(|| match msg.get("id") {
            Some(Value::String(s)) => s.clone(),
            Some(other) => other.to_string(),
            None => String::new(),
        })
}

/// Build the intent for a `tools/call`.
pub fn intent_for(
    msg: &Value,
    identity: &ClientIdentity,
    agent_override: Option<&str>,
    classification: &str,
    // `model_id`: the endpoint backing this agent. Q10 egress control denies
    // work at a controlled classification unless the deployed policy permits
    // the named endpoint — and an unnamed one is never permitted, so this is
    // not cosmetic.
    model_id: &str,
) -> Intent {
    let tool_name = msg
        .pointer("/params/name")
        .and_then(Value::as_str)
        .unwrap_or("");
    let arguments = msg
        .pointer("/params/arguments")
        .cloned()
        .unwrap_or(Value::Null);
    Intent {
        agent: agent_override
            .map(str::to_string)
            .or_else(|| identity.name.clone())
            .unwrap_or_else(|| "mcp-client".to_string()),
        principal: None, // The grant names the accountable human, not the agent.
        tool: action_class(tool_name),
        classification: classification.to_string(),
        cost: 0,
        hic1_cost_threshold: 0,
        mandatory_hic1: false,
        params_hash: params_hash(&arguments),
        model_id: model_id.to_string(),
        correlation_id: correlation_of(msg),
    }
}

/// Gate one client message.
///
/// Anything that is not a `tools/call` request is forwarded without inspection
/// — that is what keeps this proxy working across protocol revisions.
pub fn relay_decision<G: Gate>(
    msg: &Value,
    identity: &ClientIdentity,
    gate: &G,
    agent_override: Option<&str>,
    classification: &str,
    model_id: &str,
    wait: std::time::Duration,
) -> Relay {
    // A batch is an array of messages. Batching was removed from MCP, and
    // gating one element while forwarding the rest would need us to synthesize
    // a partial batch response. Refuse a batch that hides a tool call rather
    // than let it through ungated.
    if let Some(items) = msg.as_array() {
        if items.iter().any(is_tool_call) {
            return Relay::Refuse(
                "Quorum will not gate a JSON-RPC batch containing a tools/call. \
                 Send tool calls as individual requests (batching was removed from MCP)."
                    .to_string(),
            );
        }
        return Relay::Forward;
    }

    if !is_tool_call(msg) {
        return Relay::Forward;
    }

    let intent = intent_for(msg, identity, agent_override, classification, model_id);
    match gate_call(gate, &intent, wait) {
        Outcome::Proceed { .. } | Outcome::NotGoverned => Relay::Forward,
        Outcome::Refuse { reason } => Relay::Refuse(reason),
    }
}

/// The JSON-RPC error the client receives for a refused call.
///
/// The request `id` is echoed exactly — a client matching a string id against a
/// number would hang forever waiting for a reply that never comes.
pub fn refusal_response(msg: &Value, reason: &str) -> Value {
    let id = msg.get("id").cloned().unwrap_or(Value::Null);
    let mut data = Map::new();
    data.insert("governedBy".into(), json!("citrate-quorum"));
    if let Some(tool) = msg.pointer("/params/name").and_then(Value::as_str) {
        data.insert("tool".into(), json!(tool));
    }
    json!({
        "jsonrpc": "2.0",
        "id": id,
        "error": {
            "code": REFUSED_CODE,
            "message": reason,
            "data": Value::Object(data),
        }
    })
}

// ---- Streamable HTTP ------------------------------------------------

/// The header 2026-07-28 requires on every Streamable HTTP POST, so a gateway
/// can route without parsing the body.
pub const MCP_METHOD_HEADER: &str = "mcp-method";
/// Required for `tools/call`, `resources/read` and `prompts/get` — the methods
/// a gateway most wants to see. We read it for evidence, not for routing.
pub const MCP_NAME_HEADER: &str = "mcp-name";

/// Does this request need gating?
///
/// The revision lets a gateway decide from `Mcp-Method` alone, and that is the
/// fast path. But routing on the header ALONE would mean a client could skip
/// the gate by omitting a header it is merely required to send. So the body is
/// checked too, and either signal is enough. Cheap to be safe: the body is
/// already in hand, and only well-formed JSON containing `tools/call` triggers
/// the slow path.
pub fn should_gate(headers: &[(String, String)], body: &Value) -> bool {
    let header_says = headers
        .iter()
        .any(|(k, v)| k == MCP_METHOD_HEADER && v.trim() == "tools/call");
    header_says || is_tool_call(body) || body.as_array().is_some_and(|a| a.iter().any(is_tool_call))
}

/// Parse a raw HTTP header block into lowercased name/value pairs.
pub fn parse_headers(head: &str) -> Vec<(String, String)> {
    head.lines()
        .skip(1)
        .filter_map(|l| l.split_once(':'))
        .map(|(k, v)| (k.trim().to_ascii_lowercase(), v.trim().to_string()))
        .collect()
}

/// Headers a proxy must not copy upstream: they describe THIS hop.
pub fn is_hop_by_hop(name: &str) -> bool {
    matches!(
        name,
        "connection"
            | "keep-alive"
            | "proxy-authenticate"
            | "proxy-authorization"
            | "te"
            | "trailer"
            | "transfer-encoding"
            | "upgrade"
            | "host"
            | "content-length"
    )
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use crate::{Status, Verdict};
    use std::cell::RefCell;
    use std::time::Duration;

    struct FixedGate(Verdict);
    impl Gate for FixedGate {
        fn submit(&self, _i: &Intent) -> Result<Verdict, String> {
            Ok(self.0.clone())
        }
        fn status(&self, _id: u64) -> Result<Status, String> {
            Ok(Status {
                status: "pending".into(),
                may_proceed: false,
            })
        }
    }
    struct RecordingGate(RefCell<Vec<String>>);
    impl Gate for RecordingGate {
        fn submit(&self, i: &Intent) -> Result<Verdict, String> {
            self.0.borrow_mut().push(format!("{}|{}", i.agent, i.tool));
            Ok(allow())
        }
        fn status(&self, _id: u64) -> Result<Status, String> {
            unreachable!()
        }
    }
    fn allow() -> Verdict {
        Verdict {
            verdict: "allow".into(),
            may_proceed: true,
            hic: "2".into(),
            grant_id: Some("G-1".into()),
            reason: "ok".into(),
            decision_id: 1,
            ungoverned: false,
        }
    }
    fn deny() -> Verdict {
        Verdict {
            verdict: "ungoverned".into(),
            may_proceed: false,
            hic: "X".into(),
            grant_id: None,
            reason: "RC-000 no live grant covers this action".into(),
            decision_id: 2,
            ungoverned: true,
        }
    }

    fn call(tool: &str) -> Value {
        json!({
            "jsonrpc": "2.0",
            "id": 7,
            "method": "tools/call",
            "params": { "name": tool, "arguments": { "path": "/etc/passwd" } }
        })
    }

    #[test]
    fn a_tool_call_is_gated_and_a_refusal_never_reaches_the_server() {
        let d = relay_decision(
            &call("Read"),
            &ClientIdentity::default(),
            &FixedGate(deny()),
            None,
            "Public",
            "",
            Duration::ZERO,
        );
        match d {
            Relay::Refuse(reason) => assert!(reason.contains("ungoverned"), "{reason}"),
            other => panic!("a refused call must not be forwarded, got {other:?}"),
        }
    }

    #[test]
    fn an_allowed_tool_call_is_forwarded() {
        assert_eq!(
            relay_decision(
                &call("Read"),
                &ClientIdentity::default(),
                &FixedGate(allow()),
                None,
                "Public",
                "",
                Duration::ZERO
            ),
            Relay::Forward
        );
    }

    /// The whole design rests on this: methods the proxy has never heard of —
    /// including every method the 2026-07-28 revision adds — pass through.
    #[test]
    fn everything_that_is_not_a_tool_call_passes_through_unread() {
        let untouched = [
            json!({"jsonrpc":"2.0","id":1,"method":"server/discover"}),
            json!({"jsonrpc":"2.0","id":2,"method":"subscriptions/listen","params":{"types":["toolsListChanged"]}}),
            json!({"jsonrpc":"2.0","id":3,"method":"tools/list"}),
            json!({"jsonrpc":"2.0","id":4,"method":"resources/read","params":{"uri":"file:///x"}}),
            json!({"jsonrpc":"2.0","id":5,"method":"tasks/get","params":{"taskId":"t1"}}),
            json!({"jsonrpc":"2.0","id":6,"method":"some/method/invented/in/2027"}),
            json!({"jsonrpc":"2.0","method":"notifications/cancelled"}),
        ];
        for msg in untouched {
            assert_eq!(
                relay_decision(
                    &msg,
                    &ClientIdentity::default(),
                    &FixedGate(deny()),
                    None,
                    "Public",
                    "",
                    Duration::ZERO
                ),
                Relay::Forward,
                "the proxy must not have an opinion about {}",
                msg["method"]
            );
        }
    }

    #[test]
    fn a_tools_call_notification_is_not_a_request_and_is_not_gated() {
        // No id: nothing is expecting a response, so it is not a tool call we
        // could refuse coherently.
        let notif = json!({"jsonrpc":"2.0","method":"tools/call","params":{"name":"Read"}});
        assert!(!is_tool_call(&notif));
    }

    #[test]
    fn a_batch_hiding_a_tool_call_is_refused_rather_than_passed_through() {
        let batch = json!([
            json!({"jsonrpc":"2.0","id":1,"method":"tools/list"}),
            call("Bash"),
        ]);
        match relay_decision(
            &batch,
            &ClientIdentity::default(),
            &FixedGate(allow()),
            None,
            "Public",
            "",
            Duration::ZERO,
        ) {
            Relay::Refuse(r) => assert!(r.contains("batch"), "{r}"),
            other => panic!("a batch must not smuggle a tool call past the gate: {other:?}"),
        }
        // A batch with nothing gateable is still fine.
        let harmless = json!([json!({"jsonrpc":"2.0","id":1,"method":"tools/list"})]);
        assert_eq!(
            relay_decision(
                &harmless,
                &ClientIdentity::default(),
                &FixedGate(allow()),
                None,
                "Public",
                "",
                Duration::ZERO
            ),
            Relay::Forward
        );
    }

    #[test]
    fn identity_comes_from_meta_on_the_new_revision() {
        let mut id = ClientIdentity::default();
        id.observe(&json!({
            "jsonrpc":"2.0","id":1,"method":"tools/call",
            "params":{"name":"Read","_meta":{
                "io.modelcontextprotocol/protocolVersion":"2026-07-28",
                "io.modelcontextprotocol/clientInfo":{"name":"codex","version":"1.0"}
            }}
        }));
        assert_eq!(id.name.as_deref(), Some("codex"));
        assert_eq!(id.protocol_version.as_deref(), Some("2026-07-28"));
    }

    #[test]
    fn identity_falls_back_to_the_initialize_handshake_on_older_revisions() {
        let mut id = ClientIdentity::default();
        id.observe(&json!({
            "jsonrpc":"2.0","id":0,"method":"initialize",
            "params":{"protocolVersion":"2025-06-18","clientInfo":{"name":"legacy-client"}}
        }));
        assert_eq!(id.name.as_deref(), Some("legacy-client"));
        assert_eq!(id.protocol_version.as_deref(), Some("2025-06-18"));
    }

    #[test]
    fn the_learned_identity_is_what_gets_recorded() {
        let gate = RecordingGate(RefCell::new(Vec::new()));
        let mut id = ClientIdentity::default();
        id.observe(&json!({
            "jsonrpc":"2.0","id":0,"method":"initialize",
            "params":{"clientInfo":{"name":"codex"}}
        }));
        relay_decision(
            &call("Bash"),
            &id,
            &gate,
            None,
            "Public",
            "",
            Duration::ZERO,
        );
        assert_eq!(gate.0.borrow()[0], "codex|shell.exec");

        // An explicit --agent wins: an operator naming the fleet member beats
        // whatever the client calls itself.
        let gate2 = RecordingGate(RefCell::new(Vec::new()));
        relay_decision(
            &call("Bash"),
            &id,
            &gate2,
            Some("sbt-41"),
            "Public",
            "",
            Duration::ZERO,
        );
        assert_eq!(gate2.0.borrow()[0], "sbt-41|shell.exec");
    }

    #[test]
    fn the_correlation_id_prefers_w3c_trace_context() {
        let traced = json!({
            "jsonrpc":"2.0","id":9,"method":"tools/call",
            "params":{"name":"Read","_meta":{"traceparent":"00-4bf92f-00f067aa0ba9-01"}}
        });
        assert_eq!(correlation_of(&traced), "00-4bf92f-00f067aa0ba9-01");
        // No trace context: fall back to the request id so the decision can
        // still be tied to the call.
        assert_eq!(correlation_of(&call("Read")), "7");
        let string_id =
            json!({"jsonrpc":"2.0","id":"abc","method":"tools/call","params":{"name":"Read"}});
        assert_eq!(correlation_of(&string_id), "abc");
    }

    #[test]
    fn the_refusal_echoes_the_request_id_exactly() {
        // A client matching a string id against a number waits forever.
        let string_id =
            json!({"jsonrpc":"2.0","id":"req-1","method":"tools/call","params":{"name":"Bash"}});
        let r = refusal_response(&string_id, "nope");
        assert_eq!(r["id"], json!("req-1"));
        assert_eq!(r["jsonrpc"], "2.0");
        assert_eq!(r["error"]["code"], json!(REFUSED_CODE));
        assert_eq!(r["error"]["message"], "nope");
        assert_eq!(r["error"]["data"]["governedBy"], "citrate-quorum");
        assert_eq!(r["error"]["data"]["tool"], "Bash");

        let num = refusal_response(&call("Read"), "nope");
        assert_eq!(num["id"], json!(7));
    }

    #[test]
    fn the_refusal_code_respects_the_2026_07_28_allocation_policy() {
        // -32020..-32099 is reserved for the specification; -32000..-32019 is
        // implementation-defined. A refusal is ours, so it lives in the lower
        // band. Squatting on the reserved range would collide with a future
        // spec-defined error.
        assert!(
            (-32019..=-32000).contains(&REFUSED_CODE),
            "REFUSED_CODE {REFUSED_CODE} must stay in the implementation-defined band"
        );
    }

    #[test]
    fn the_http_gate_reads_the_routing_header() {
        let headers = vec![
            ("content-type".to_string(), "application/json".to_string()),
            ("mcp-method".to_string(), "tools/call".to_string()),
            ("mcp-name".to_string(), "Bash".to_string()),
        ];
        // The fast path the revision designs for: decide from the header.
        assert!(should_gate(&headers, &json!({})));
        let listing = vec![("mcp-method".to_string(), "tools/list".to_string())];
        assert!(!should_gate(
            &listing,
            &json!({"jsonrpc":"2.0","id":1,"method":"tools/list"})
        ));
    }

    #[test]
    fn omitting_the_routing_header_does_not_skip_the_gate() {
        // `Mcp-Method` is required, but a client that omits it must not thereby
        // escape governance — the body is checked too.
        assert!(
            should_gate(&[], &call("Bash")),
            "a tools/call with no Mcp-Method header must still be gated"
        );
        // And a lying header cannot hide a tool call in the body either.
        let lying = vec![("mcp-method".to_string(), "tools/list".to_string())];
        assert!(should_gate(&lying, &call("Bash")));
        // A batch hiding one is caught as well.
        assert!(should_gate(&[], &json!([call("Bash")])));
    }

    #[test]
    fn headers_are_parsed_case_insensitively_and_hop_by_hop_ones_are_dropped() {
        let head = "POST /mcp HTTP/1.1\r\nHost: x\r\nMCP-Method: tools/call\r\n\
                    Authorization: Bearer t\r\nTransfer-Encoding: chunked\r\n";
        let h = parse_headers(head);
        assert!(h
            .iter()
            .any(|(k, v)| k == "mcp-method" && v == "tools/call"));
        assert!(should_gate(&h, &json!({})));
        // These describe this hop and must not be replayed upstream.
        for hop in ["host", "transfer-encoding", "connection", "content-length"] {
            assert!(is_hop_by_hop(hop), "{hop} must not be forwarded");
        }
        assert!(
            !is_hop_by_hop("authorization"),
            "auth must reach the server"
        );
        assert!(!is_hop_by_hop("mcp-protocol-version"));
    }

    #[test]
    fn the_params_hash_commits_to_the_tool_arguments() {
        let i = intent_for(
            &call("Read"),
            &ClientIdentity::default(),
            None,
            "Public",
            "",
        );
        assert_eq!(i.tool, "repo.read");
        let same = intent_for(
            &call("Read"),
            &ClientIdentity::default(),
            None,
            "Public",
            "",
        );
        assert_eq!(i.params_hash, same.params_hash);
        let other = json!({
            "jsonrpc":"2.0","id":7,"method":"tools/call",
            "params": { "name": "Read", "arguments": { "path": "/etc/shadow" } }
        });
        let diff = intent_for(&other, &ClientIdentity::default(), None, "Public", "");
        assert_ne!(i.params_hash, diff.params_hash);
    }
}
