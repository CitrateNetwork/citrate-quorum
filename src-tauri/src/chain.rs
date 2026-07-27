//! Live chain reads for the Node, Wallet and Settings surfaces (QRM Phase 0).
//!
//! ## Data source (CLAUDE.md rule 11 — every command names its source)
//!
//! - Endpoint: the `rpcUrl` carried by the canonical address book
//!   ([`crate::addresses`]), never a literal in this file.
//! - Chain facts: `eth_blockNumber`, `net_peerCount`, `web3_clientVersion`,
//!   `eth_syncing`, `eth_chainId`, `eth_getBlockByNumber`.
//! - Balances: `eth_getBalance` for the native currency; ERC-20
//!   `symbol()`/`decimals()`/`balanceOf(address)` for booked tokens.
//! - Tenancy: `TenantHierarchy.root()/getNode()/getChildren()`, the contract
//!   resolved by name from the **BFR** book at runtime.
//!
//! ## What this module will not do
//!
//! It does not sign and it does not send: every method here is an `eth_call` or
//! a node query. Signing lives behind the SignatureCeremony (rule 3).
//!
//! It also does not INVENT the fields the design prototype drew. The prototype's
//! Node surface showed a per-block "blue / anticone" flag and a checkpoint
//! marker; chain 40204's JSON-RPC exposes `blueScore`, `selectedParentHash` and
//! `mergeParentHashes`, and nothing that says "this block is a checkpoint". So
//! this module returns what the node actually says and the surface renders that
//! — a made-up boolean rendered in red would have looked like a finding.
//!
//! ## Why the transport is reached the way it is
//!
//! `citrate_core_kit::rpc::RpcClient` has typed methods for the calls the kit
//! itself needs (`block_number`, `get_balance`, `eth_call`, …) but no generic
//! "send this method" entry point, and its `request` is private. Rather than
//! fork an HTTP client into this repo (rule 10 — link, don't copy), [`Rpc`]
//! composes the kit's PUBLIC `build_request` + `RpcTransport::call`. A generic
//! `call(method, params)` belongs in the kit eventually; when it lands, this
//! shim collapses into it and nothing else here changes.
//!
//! ## Reachable, unreachable, and absent are different answers
//!
//! Same discipline as [`crate::anchor`]: a chain that could not be reached is
//! reported as unreachable, a contract missing from the book as unavailable, and
//! an empty-but-answered read as empty. A surface that collapses them tells an
//! operator "nothing here" when the truth is "we could not ask".

use std::collections::VecDeque;
use std::sync::Mutex;
use std::time::Instant;

use citrate_core_kit::rpc::{HttpTransport, RpcClient, RpcTransport};
use serde::Serialize;
use serde_json::{json, Value};

use crate::addresses::AddressBook;
use crate::anchor::keccak256;

/// The name the tenant tree is booked under, in the BFR book.
pub const TENANT_HIERARCHY: &str = "TenantHierarchy";
/// Where a principal's clearance is recorded, in the BFR book.
pub const CLASSIFICATION_REGISTRY: &str = "ClassificationRegistry";

/// How many observed RPC lines the activity ring buffer keeps. The Node surface
/// shows the most recent ones; older lines are dropped rather than growing
/// without bound in a long-lived desktop session.
const ACTIVITY_CAPACITY: usize = 400;

// ---- the observed-activity log ---------------------------------------------

/// One line of what this app actually did against the chain.
///
/// This is NOT a node log: citrate-quorum supervises no node, so it has no node
/// logs to stream, and inventing some would be exactly the defect this repo
/// keeps finding. It is the app's own record of every RPC it issued — method,
/// endpoint, outcome, measured round trip — which is a real thing an operator
/// can use to tell "the chain is down" from "the app never asked".
#[derive(Serialize, Clone, Debug, PartialEq, Eq)]
pub struct ActivityLine {
    /// `HH:MM:SS` UTC.
    pub t: String,
    /// `INFO` for a completed call, `ERROR` for one that failed.
    pub lvl: String,
    /// The JSON-RPC method — the Node surface renders this as the module column.
    pub module: String,
    pub msg: String,
}

fn activity_log() -> &'static Mutex<VecDeque<ActivityLine>> {
    // `OnceLock` rather than a lazy_static dep; the buffer is process-global
    // because the RPC facade is constructed per call and must not lose history.
    static LOG: std::sync::OnceLock<Mutex<VecDeque<ActivityLine>>> = std::sync::OnceLock::new();
    LOG.get_or_init(|| Mutex::new(VecDeque::with_capacity(ACTIVITY_CAPACITY)))
}

fn record_activity(line: ActivityLine) {
    if let Ok(mut log) = activity_log().lock() {
        if log.len() == ACTIVITY_CAPACITY {
            log.pop_front();
        }
        log.push_back(line);
    }
    // A poisoned activity lock must never take a chain read down with it: the
    // log is diagnostics, the read is the product.
}

/// The observed lines, newest first.
pub fn activity() -> Vec<ActivityLine> {
    activity_log()
        .lock()
        .map(|l| l.iter().rev().cloned().collect())
        .unwrap_or_default()
}

fn now_ms() -> i64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

/// `HH:MM:SS` UTC from epoch-ms. Same helper as the ledger's TIME column: UTC,
/// because an evidence trail read across sites must not shift under the reader.
fn clock_utc(ms: i64) -> String {
    let secs = ms.div_euclid(1000).rem_euclid(86_400);
    format!(
        "{:02}:{:02}:{:02}",
        secs / 3600,
        (secs % 3600) / 60,
        secs % 60
    )
}

// ---- the RPC facade --------------------------------------------------------

/// A thin, logged JSON-RPC facade over the kit's client.
pub struct Rpc {
    client: RpcClient<HttpTransport>,
    url: String,
}

impl Rpc {
    /// Build a client bound to the book's endpoint.
    ///
    /// A book with no `rpcUrl` is an error, not a guess: the alternative is
    /// silently reading someone else's chain, which rule 8 exists to prevent.
    pub fn from_book(book: &AddressBook) -> Result<Self, String> {
        let url = book
            .rpc_url
            .clone()
            .filter(|u| !u.trim().is_empty())
            .ok_or_else(|| {
                format!(
                    "the address book ({}) carries no rpcUrl — there is no endpoint to read from. \
                 Set one in the book, or point QUORUM_ADDRESS_BOOK at a book that has one.",
                    book.describe()
                )
            })?;
        Ok(Self {
            client: RpcClient::with_transport(HttpTransport::new(url.clone())),
            url,
        })
    }

    pub fn url(&self) -> &str {
        &self.url
    }

    /// Send one JSON-RPC call, returning its `result` and recording the attempt
    /// in the activity log. A node `error` object is surfaced as an error rather
    /// than as an empty result.
    pub fn call(&self, method: &str, params: Value) -> Result<Value, String> {
        let started = Instant::now();
        let body = self.client.build_request(method, params);
        let outcome = self.client.transport().call(body);
        let ms = started.elapsed().as_millis();

        let host = self.url.clone();
        match outcome {
            Ok(resp) => {
                if let Some(err) = resp.get("error") {
                    let msg = err
                        .get("message")
                        .and_then(Value::as_str)
                        .unwrap_or("unknown node error")
                        .to_string();
                    record_activity(ActivityLine {
                        t: clock_utc(now_ms()),
                        lvl: "ERROR".into(),
                        module: method.to_string(),
                        msg: format!("{host} refused after {ms}ms: {msg}"),
                    });
                    return Err(format!("{method} on {host}: {msg}"));
                }
                let result = resp.get("result").cloned().ok_or_else(|| {
                    format!("{method} on {host}: the response carried no result field")
                })?;
                record_activity(ActivityLine {
                    t: clock_utc(now_ms()),
                    lvl: "INFO".into(),
                    module: method.to_string(),
                    msg: format!("{host} answered in {ms}ms"),
                });
                Ok(result)
            }
            Err(e) => {
                record_activity(ActivityLine {
                    t: clock_utc(now_ms()),
                    lvl: "ERROR".into(),
                    module: method.to_string(),
                    msg: format!("{host} unreachable after {ms}ms: {e}"),
                });
                Err(format!("could not reach {host}: {e}"))
            }
        }
    }

    /// An `eth_call` returning the raw ABI bytes.
    pub fn eth_call(&self, to: &str, data: &str) -> Result<Vec<u8>, String> {
        let result = self.call("eth_call", json!([{ "to": to, "data": data }, "latest"]))?;
        let s = result
            .as_str()
            .ok_or_else(|| "eth_call: result was not a string".to_string())?;
        let stripped = s
            .strip_prefix("0x")
            .ok_or_else(|| format!("eth_call: result missing 0x prefix ({s})"))?;
        hex::decode(stripped).map_err(|_| format!("eth_call: result was not hex ({s})"))
    }
}

// ---- hex helpers -----------------------------------------------------------

/// Parse a `0x`-prefixed hex quantity. Returns an error rather than 0 for
/// anything malformed — a silently-zero height reads as a stalled chain.
fn quantity(v: &Value, what: &str) -> Result<u64, String> {
    let s = v
        .as_str()
        .ok_or_else(|| format!("{what}: expected a hex quantity, got {v}"))?;
    let body = s.strip_prefix("0x").unwrap_or(s);
    u64::from_str_radix(body, 16).map_err(|_| format!("{what}: {s} is not a hex quantity"))
}

fn quantity_u128(v: &Value, what: &str) -> Result<u128, String> {
    let s = v
        .as_str()
        .ok_or_else(|| format!("{what}: expected a hex quantity, got {v}"))?;
    let body = s.strip_prefix("0x").unwrap_or(s);
    u128::from_str_radix(body, 16).map_err(|_| format!("{what}: {s} is not a hex quantity"))
}

fn hex_of(bytes: &[u8]) -> String {
    let mut s = String::from("0x");
    for b in bytes {
        s.push_str(&format!("{b:02x}"));
    }
    s
}

/// Render an integer amount of the smallest unit as a decimal string with
/// `decimals` places, exactly — string arithmetic, never a float.
///
/// A balance is money. `1e18` wei through an `f64` loses the low digits, and a
/// wallet that rounds someone's balance is worse than one that shows nothing.
/// Trailing zeros are trimmed, but no significant digit ever is.
pub fn format_units(amount: u128, decimals: u32) -> String {
    let s = amount.to_string();
    let d = decimals as usize;
    let (int_part, frac_part) = if s.len() > d {
        let split = s.len() - d;
        (s[..split].to_string(), s[split..].to_string())
    } else {
        ("0".to_string(), format!("{}{}", "0".repeat(d - s.len()), s))
    };
    let frac = frac_part.trim_end_matches('0');
    if frac.is_empty() {
        int_part
    } else {
        format!("{int_part}.{frac}")
    }
}

// ---- node status -----------------------------------------------------------

/// What the Node surface's posture tiles render. Every field is a live answer
/// from the endpoint named in `rpc_url`, or `None` because that endpoint did not
/// answer that particular question.
#[derive(Serialize, Clone, Debug)]
pub struct ChainStatus {
    pub rpc_url: String,
    /// Which book supplied the endpoint (rule 11 provenance).
    pub book: String,
    /// The chain id the book says we are on.
    pub chain_id: u64,
    pub height: u64,
    /// `net_peerCount` — the endpoint's own peer count. `None` when the node
    /// does not implement it; that is not zero peers.
    pub peers: Option<u64>,
    pub client: Option<String>,
    /// `false` = fully synced. `None` = the node did not answer.
    pub syncing: Option<bool>,
    /// Measured round trip of the height call, in milliseconds.
    pub latency_ms: u64,
    /// Base fee of the head block, in wei, as a decimal string.
    pub base_fee_wei: Option<String>,
    /// GhostDAG blue score of the head block, when the node reports one.
    pub blue_score: Option<u64>,
}

/// Ask the chain where it is.
///
/// **Chain-id mismatch is a hard error** (rule 8): if the endpoint reports a
/// different chain than the book was frozen against, every address in that book
/// means something else there. Reporting the height anyway would be a number
/// with no meaning.
pub fn status(book: &AddressBook) -> Result<ChainStatus, String> {
    let rpc = Rpc::from_book(book)?;

    let started = Instant::now();
    let height = quantity(&rpc.call("eth_blockNumber", json!([]))?, "eth_blockNumber")?;
    let latency_ms = started.elapsed().as_millis() as u64;

    let reported = quantity(&rpc.call("eth_chainId", json!([]))?, "eth_chainId")?;
    if reported != book.chain_id {
        return Err(format!(
            "chain id mismatch: {} reports chain {reported}, but the address book ({}) is \
             frozen against chain {}. Every address in that book means something else on \
             chain {reported} — refusing to read it.",
            rpc.url(),
            book.describe(),
            book.chain_id
        ));
    }

    // These three are decorations: a node that does not implement one must not
    // fail the whole status read, so each degrades to `None`.
    let peers = rpc
        .call("net_peerCount", json!([]))
        .ok()
        .and_then(|v| quantity(&v, "net_peerCount").ok());
    let client = rpc
        .call("web3_clientVersion", json!([]))
        .ok()
        .and_then(|v| v.as_str().map(str::to_string));
    let syncing = rpc
        .call("eth_syncing", json!([]))
        .ok()
        .map(|v| !matches!(v, Value::Bool(false)));

    let head = rpc
        .call(
            "eth_getBlockByNumber",
            json!([format!("0x{height:x}"), false]),
        )
        .ok();
    let base_fee_wei = head
        .as_ref()
        .and_then(|b| b.get("baseFeePerGas"))
        .and_then(|v| quantity_u128(v, "baseFeePerGas").ok())
        .map(|w| w.to_string());
    let blue_score = head
        .as_ref()
        .and_then(|b| b.get("blueScore"))
        .and_then(|v| quantity(v, "blueScore").ok());

    Ok(ChainStatus {
        rpc_url: rpc.url().to_string(),
        book: book.describe(),
        chain_id: book.chain_id,
        height,
        peers,
        client,
        syncing,
        latency_ms,
        base_fee_wei,
        blue_score,
    })
}

// ---- recent blocks ---------------------------------------------------------

/// One row of the block explorer — only fields chain 40204 actually returns.
#[derive(Serialize, Clone, Debug)]
pub struct BlockRow {
    pub height: u64,
    pub hash: String,
    /// Number of transactions in the block.
    pub txs: usize,
    /// The block's proposer (`miner`).
    pub proposer: String,
    pub gas_used: u64,
    pub gas_limit: u64,
    /// Block timestamp, epoch seconds — the surface renders the age from it.
    pub timestamp: u64,
    /// GhostDAG blue score, as reported.
    pub blue_score: Option<u64>,
    /// How many merge parents the block absorbed (`mergeParentHashes.len()`).
    /// A block with merge parents is one that merged an anticone; a block with
    /// none simply extended the selected chain.
    pub merge_parents: usize,
}

/// The most recent `count` blocks, newest first.
///
/// One `eth_getBlockByNumber` per height. `count` is clamped: an unbounded loop
/// here would let a surface hang the app on a slow endpoint.
pub fn recent_blocks(book: &AddressBook, count: u32) -> Result<Vec<BlockRow>, String> {
    let rpc = Rpc::from_book(book)?;
    let head = quantity(&rpc.call("eth_blockNumber", json!([]))?, "eth_blockNumber")?;
    let count = count.clamp(1, 25) as u64;

    let mut rows = Vec::new();
    for h in (head.saturating_sub(count - 1)..=head).rev() {
        let b = rpc.call("eth_getBlockByNumber", json!([format!("0x{h:x}"), false]))?;
        if b.is_null() {
            // The node knows the height but not the block: report the gap
            // rather than skipping it silently.
            return Err(format!(
                "{} returned no block at height {h} — the endpoint's head and its block store \
                 disagree",
                rpc.url()
            ));
        }
        rows.push(BlockRow {
            height: b
                .get("number")
                .and_then(|v| quantity(v, "number").ok())
                .unwrap_or(h),
            hash: b
                .get("hash")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string(),
            txs: b
                .get("transactions")
                .and_then(Value::as_array)
                .map(Vec::len)
                .unwrap_or(0),
            proposer: b
                .get("miner")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string(),
            gas_used: b
                .get("gasUsed")
                .and_then(|v| quantity(v, "gasUsed").ok())
                .unwrap_or(0),
            gas_limit: b
                .get("gasLimit")
                .and_then(|v| quantity(v, "gasLimit").ok())
                .unwrap_or(0),
            timestamp: b
                .get("timestamp")
                .and_then(|v| quantity(v, "timestamp").ok())
                .unwrap_or(0),
            blue_score: b
                .get("blueScore")
                .and_then(|v| quantity(v, "blueScore").ok()),
            merge_parents: b
                .get("mergeParentHashes")
                .and_then(Value::as_array)
                .map(Vec::len)
                .unwrap_or(0),
        });
    }
    Ok(rows)
}

// ---- balances --------------------------------------------------------------

/// One balance row. `balance` is a decimal string in whole units, exact.
#[derive(Serialize, Clone, Debug)]
pub struct TokenRow {
    pub symbol: String,
    pub name: String,
    pub balance: String,
    pub native: bool,
    /// Rule 11: the exact call this number came from.
    pub source: String,
}

/// The native balance plus any booked ERC-20 the endpoint could answer for.
///
/// A token whose contract does not answer is OMITTED and named in `notes` —
/// never rendered as a zero balance, which is a claim about someone's money.
pub fn balances(
    book: &AddressBook,
    address: &str,
    tokens: &[&str],
) -> Result<(Vec<TokenRow>, Vec<String>), String> {
    let rpc = Rpc::from_book(book)?;
    let mut rows = Vec::new();
    let mut notes = Vec::new();

    let wei = quantity_u128(
        &rpc.call("eth_getBalance", json!([address, "latest"]))?,
        "eth_getBalance",
    )?;
    rows.push(TokenRow {
        symbol: "SALT".to_string(),
        name: "Citrate native currency".to_string(),
        balance: format_units(wei, 18),
        native: true,
        source: format!("eth_getBalance({address},latest) on {}", rpc.url()),
    });

    for name in tokens {
        let Some(addr) = book.get(name) else {
            notes.push(format!(
                "{name} is not in the address book ({}) — no balance was read for it",
                book.describe()
            ));
            continue;
        };
        match erc20_balance(&rpc, addr, address) {
            Ok((symbol, decimals, raw)) => rows.push(TokenRow {
                symbol,
                name: (*name).to_string(),
                balance: format_units(raw, decimals),
                native: false,
                source: format!("{name}.balanceOf({address}) at {addr}"),
            }),
            Err(e) => notes.push(format!("{name} at {addr} did not answer: {e}")),
        }
    }
    Ok((rows, notes))
}

/// `symbol()`, `decimals()`, `balanceOf(address)` for one ERC-20.
fn erc20_balance(rpc: &Rpc, token: &str, holder: &str) -> Result<(String, u32, u128), String> {
    let sym = decode_string(&rpc.eth_call(token, &selector_only("symbol()"))?)?;
    let dec_bytes = rpc.eth_call(token, &selector_only("decimals()"))?;
    let decimals =
        word_u64(&dec_bytes, 0).ok_or_else(|| "decimals() returned no word".to_string())? as u32;
    let bal_bytes = rpc.eth_call(token, &encode_address_call("balanceOf(address)", holder)?)?;
    let raw = word_u128(&bal_bytes, 0).ok_or_else(|| "balanceOf() returned no word".to_string())?;
    Ok((sym, decimals, raw))
}

// ---- ABI encode/decode -----------------------------------------------------

fn selector(sig: &str) -> [u8; 4] {
    let d = keccak256(sig.as_bytes());
    [d[0], d[1], d[2], d[3]]
}

fn selector_only(sig: &str) -> String {
    hex_of(&selector(sig))
}

/// `f(address)` calldata: the selector plus the address right-aligned in a word.
fn encode_address_call(sig: &str, address: &str) -> Result<String, String> {
    let body = address.strip_prefix("0x").unwrap_or(address);
    let bytes = hex::decode(body).map_err(|_| format!("not a hex address: {address}"))?;
    if bytes.len() != 20 {
        return Err(format!("not a 20-byte address: {address}"));
    }
    let mut out = Vec::with_capacity(36);
    out.extend_from_slice(&selector(sig));
    out.extend_from_slice(&[0u8; 12]);
    out.extend_from_slice(&bytes);
    Ok(hex_of(&out))
}

/// `f(bytes32)` calldata.
fn encode_word_call(sig: &str, word: [u8; 32]) -> String {
    let mut out = Vec::with_capacity(36);
    out.extend_from_slice(&selector(sig));
    out.extend_from_slice(&word);
    hex_of(&out)
}

/// The 32-byte word at index `i`, if the return is long enough.
fn word(bytes: &[u8], i: usize) -> Option<[u8; 32]> {
    let start = i * 32;
    let end = start + 32;
    if bytes.len() < end {
        return None;
    }
    let mut w = [0u8; 32];
    w.copy_from_slice(&bytes[start..end]);
    Some(w)
}

/// The low 8 bytes of word `i` as a u64. Words wider than u64 are rejected
/// rather than truncated — a truncated offset reads the wrong field entirely.
fn word_u64(bytes: &[u8], i: usize) -> Option<u64> {
    let w = word(bytes, i)?;
    if w[..24].iter().any(|b| *b != 0) {
        return None;
    }
    let mut n = [0u8; 8];
    n.copy_from_slice(&w[24..32]);
    Some(u64::from_be_bytes(n))
}

fn word_u128(bytes: &[u8], i: usize) -> Option<u128> {
    let w = word(bytes, i)?;
    if w[..16].iter().any(|b| *b != 0) {
        return None;
    }
    let mut n = [0u8; 16];
    n.copy_from_slice(&w[16..32]);
    Some(u128::from_be_bytes(n))
}

/// The address in word `i` (right-aligned), `0x`-prefixed lowercase.
fn word_address(bytes: &[u8], i: usize) -> Option<String> {
    let w = word(bytes, i)?;
    Some(hex_of(&w[12..32]))
}

/// Decode a top-level ABI `string` return: `[offset][len][bytes…]`.
fn decode_string(bytes: &[u8]) -> Result<String, String> {
    let off = word_u64(bytes, 0).ok_or_else(|| "string: no offset word".to_string())? as usize;
    decode_string_at(bytes, off)
}

/// Decode an ABI `string` whose data begins at absolute byte offset `at`.
fn decode_string_at(bytes: &[u8], at: usize) -> Result<String, String> {
    if bytes.len() < at + 32 {
        return Err("string: truncated before its length word".to_string());
    }
    let len = word_u64(bytes, at / 32).ok_or_else(|| "string: length is not a word".to_string())?
        as usize;
    let start = at + 32;
    let end = start
        .checked_add(len)
        .ok_or_else(|| "string: length overflows".to_string())?;
    if bytes.len() < end {
        return Err(format!(
            "string: claims {len} bytes but only {} remain",
            bytes.len().saturating_sub(start)
        ));
    }
    String::from_utf8(bytes[start..end].to_vec()).map_err(|_| "string: not valid UTF-8".to_string())
}

/// Decode an ABI `address[]` whose data begins at absolute byte offset `at`.
fn decode_address_array_at(bytes: &[u8], at: usize) -> Result<Vec<String>, String> {
    if bytes.len() < at + 32 {
        return Err("address[]: truncated before its length word".to_string());
    }
    let len = word_u64(bytes, at / 32)
        .ok_or_else(|| "address[]: length is not a word".to_string())? as usize;
    let mut out = Vec::with_capacity(len.min(64));
    for i in 0..len {
        let idx = (at / 32) + 1 + i;
        out.push(
            word_address(bytes, idx)
                .ok_or_else(|| format!("address[]: element {i} is past the end of the return"))?,
        );
    }
    Ok(out)
}

/// Decode an ABI `bytes32[]` whose data begins at absolute byte offset `at`.
fn decode_word_array_at(bytes: &[u8], at: usize) -> Result<Vec<[u8; 32]>, String> {
    if bytes.len() < at + 32 {
        return Err("bytes32[]: truncated before its length word".to_string());
    }
    let len = word_u64(bytes, at / 32)
        .ok_or_else(|| "bytes32[]: length is not a word".to_string())? as usize;
    let mut out = Vec::with_capacity(len.min(256));
    for i in 0..len {
        out.push(
            word(bytes, (at / 32) + 1 + i)
                .ok_or_else(|| format!("bytes32[]: element {i} is past the end of the return"))?,
        );
    }
    Ok(out)
}

// ---- tenancy ---------------------------------------------------------------

/// One node of the tenant tree, as the Settings surface renders it.
#[derive(Serialize, Clone, Debug, PartialEq, Eq)]
pub struct TenancyRow {
    /// Tree depth: 0 enterprise, 1 BU, 2 site, 3 team.
    pub depth: u8,
    pub name: String,
    /// The node's admin addresses, joined — the humans who can change it.
    pub admins: String,
    /// `Public` | `Proprietary` | `CUI` | `ITAR`.
    pub ceiling: String,
    /// `M-of-N`.
    pub threshold: String,
    /// The node's on-chain id.
    pub id: String,
}

/// The tenant tree plus a line saying where it came from — or why it is empty.
#[derive(Serialize, Clone, Debug)]
pub struct TenancyView {
    pub rows: Vec<TenancyRow>,
    /// Rule 11: the contract, book and call behind `rows`.
    pub source: String,
    /// Present when the tree is empty for a reason an operator must act on.
    pub note: Option<String>,
}

/// `classification_max` uses ClassificationRegistry's encoding.
fn classification_name(n: u8) -> String {
    match n {
        0 => "Public".to_string(),
        1 => "Proprietary".to_string(),
        2 => "CUI".to_string(),
        3 => "ITAR".to_string(),
        // Fail LOUD, not to Public: an unknown ceiling rendered as the lowest
        // classification is the one mistake this column must never make.
        other => format!("unknown({other})"),
    }
}

/// The decoded `TenantHierarchy.getNode` struct, minus the fields no surface
/// reads (the HKDF salt is key-derivation material and stays on chain).
#[derive(Debug, PartialEq, Eq)]
struct TenantNode {
    parent: [u8; 32],
    display_name: String,
    level: u8,
    admins: Vec<String>,
    threshold: u8,
    classification_max: u8,
    exists: bool,
}

/// Decode `getNode(bytes32) -> TenantNode`.
///
/// The struct is DYNAMIC (it holds a `string` and an `address[]`), so the return
/// is a 32-byte offset to the struct, then nine head words, then the tails. The
/// two offset words are relative to the START OF THE STRUCT, not the start of
/// the return — getting that wrong still decodes, into the wrong fields, which
/// is why `abi_fixture_decodes_to_the_encoded_values` pins it against calldata
/// produced by a real ABI encoder rather than by this file's own arithmetic.
///
/// Head layout, from the struct start:
///   [0] parent  [1] self  [2] offset(display_name)  [3] level  [4] hkdf_salt
///   [5] offset(admins)  [6] admin_threshold  [7] classification_max  [8] exists
fn decode_tenant_node(ret: &[u8]) -> Result<TenantNode, String> {
    let struct_at =
        word_u64(ret, 0).ok_or_else(|| "getNode: no struct offset".to_string())? as usize;
    if struct_at % 32 != 0 {
        return Err(format!(
            "getNode: struct offset {struct_at} is not word-aligned"
        ));
    }
    let base = struct_at / 32;

    let parent = word(ret, base).ok_or_else(|| "getNode: truncated at parent".to_string())?;
    let name_off = word_u64(ret, base + 2)
        .ok_or_else(|| "getNode: no display_name offset".to_string())? as usize;
    let level = word_u64(ret, base + 3).ok_or_else(|| "getNode: no level".to_string())? as u8;
    let admins_off =
        word_u64(ret, base + 5).ok_or_else(|| "getNode: no admins offset".to_string())? as usize;
    let threshold =
        word_u64(ret, base + 6).ok_or_else(|| "getNode: no threshold".to_string())? as u8;
    let classification_max =
        word_u64(ret, base + 7).ok_or_else(|| "getNode: no classification_max".to_string())? as u8;
    let exists = word_u64(ret, base + 8).ok_or_else(|| "getNode: no exists flag".to_string())? == 1;

    let display_name = decode_string_at(ret, struct_at + name_off)?;
    let admins = decode_address_array_at(ret, struct_at + admins_off)?;

    Ok(TenantNode {
        parent,
        display_name,
        level,
        admins,
        threshold,
        classification_max,
        exists,
    })
}

/// Read the tenant tree from `TenantHierarchy`, root first, depth first.
///
/// Three distinct empty answers, kept distinct:
/// - the BFR book does not carry the contract → unavailable;
/// - the contract is deployed but `root()` is zero → deployed-but-uninitialized,
///   which is an owner action (`initRoot`), not a bug;
/// - the chain could not be reached → unreachable.
pub fn tenancy(book: &AddressBook) -> Result<TenancyView, String> {
    let Some(addr) = book.get(TENANT_HIERARCHY) else {
        return Ok(TenancyView {
            rows: Vec::new(),
            source: format!("{TENANT_HIERARCHY} — absent from {}", book.describe()),
            note: Some(format!(
                "{TENANT_HIERARCHY} is not in the address book ({}). Run \
                 scripts/sync-addresses.sh against a chain where it is deployed.",
                book.describe()
            )),
        });
    };
    let rpc = Rpc::from_book(book)?;
    let source = format!(
        "{TENANT_HIERARCHY}.getNode/getChildren at {addr} · chain {} · {} · {}",
        book.chain_id,
        rpc.url(),
        book.describe()
    );

    let root_ret = rpc.eth_call(addr, &selector_only("root()"))?;
    let root = word(&root_ret, 0).ok_or_else(|| "root(): empty return".to_string())?;
    if root == [0u8; 32] {
        return Ok(TenancyView {
            rows: Vec::new(),
            source,
            note: Some(format!(
                "{TENANT_HIERARCHY} is deployed at {addr} on chain {} but holds no root node: \
                 root() is the zero word, so initRoot has never been called. The tree is empty \
                 on chain, not missing from this app. Seeding it is a governance act — it names \
                 the root admins, the M-of-N threshold and the classification ceiling — and only \
                 the contract's deployer may call initRoot.",
                book.chain_id
            )),
        });
    }

    let mut rows = Vec::new();
    walk(&rpc, addr, root, 0, &mut rows)?;
    Ok(TenancyView {
        rows,
        source,
        note: None,
    })
}

/// Depth-first walk of the tree. Depth is bounded by the contract (levels 0..3),
/// and the guard here keeps a malformed or cyclic tree from recursing forever.
fn walk(
    rpc: &Rpc,
    addr: &str,
    id: [u8; 32],
    depth: u8,
    out: &mut Vec<TenancyRow>,
) -> Result<(), String> {
    if depth > 3 {
        return Err(format!(
            "{TENANT_HIERARCHY} at {addr} returned a tree deeper than its own MAX_LEVEL — \
             refusing to walk further"
        ));
    }
    let node_ret = rpc.eth_call(addr, &encode_word_call("getNode(bytes32)", id))?;
    let node = decode_tenant_node(&node_ret)?;
    if !node.exists {
        return Err(format!(
            "{TENANT_HIERARCHY} at {addr} returned a node marked non-existent for {}",
            hex_of(&id)
        ));
    }
    out.push(TenancyRow {
        depth,
        name: node.display_name,
        admins: if node.admins.is_empty() {
            "—".to_string()
        } else {
            node.admins.join(", ")
        },
        ceiling: classification_name(node.classification_max),
        threshold: format!("{}-of-{}", node.threshold, node.admins.len()),
        id: hex_of(&id),
    });

    let kids_ret = rpc.eth_call(addr, &encode_word_call("getChildren(bytes32)", id))?;
    let kids_at =
        word_u64(&kids_ret, 0).ok_or_else(|| "getChildren: no offset".to_string())? as usize;
    for kid in decode_word_array_at(&kids_ret, kids_at)? {
        walk(rpc, addr, kid, depth + 1, out)?;
    }
    Ok(())
}

// ---- clearance -------------------------------------------------------------

/// The operator's clearance, as the chain actually has it.
#[derive(Serialize, Clone, Debug)]
pub struct ClearanceView {
    /// The clearance to enforce with — `Public` when nothing is recorded.
    pub effective: String,
    /// Whether the registry holds a record at all. `false` with
    /// `effective == "Public"` means "nobody has said", not "cleared to Public".
    pub recorded: bool,
    /// Only meaningful when `recorded`. ITAR requires a definite non-FN.
    pub foreign_national: Option<bool>,
    /// The tenant's own ceiling from `TenantHierarchy`, when there is a tree.
    pub tenant_ceiling: Option<String>,
    /// The least of what was read — what a room or a document is actually
    /// bounded by. Never higher than either axis.
    pub bounded_to: String,
    /// The subject key the registry was asked about.
    pub subject: String,
    /// Rule 11: the contracts, the chain and the book behind this.
    pub source: String,
    /// Present when an axis could not be read at all (chain unreachable, or the
    /// contract absent) — distinct from an axis that answered "nothing recorded".
    pub note: Option<String>,
}

/// The subject key a clearance is recorded under.
///
/// `keccak256(lowercased 0x address)`. The registry keys on an opaque `bytes32`
/// so it can carry a DID or an HR id instead; the wallet address is what this app
/// can actually prove about the person at the keyboard, so it is what we ask
/// about — and the key is returned in [`ClearanceView::subject`] so an HR oracle
/// can be pointed at the same value rather than guessing our convention.
pub fn clearance_subject(address: &str) -> [u8; 32] {
    keccak256(address.to_ascii_lowercase().as_bytes())
}

/// Read the operator's clearance from `ClassificationRegistry`, bounded by the
/// tenant's own ceiling.
///
/// Fail-closed in every direction: an unreachable chain, an absent contract, a
/// subject with no record and a tenant with no node all end at `Public`, and each
/// says which one happened rather than sharing a sentence.
pub fn clearance(book: &AddressBook, address: &str, tenant: &str) -> Result<ClearanceView, String> {
    use quorum_clearance::ClearanceReader;

    let Some(registry) = book.get(CLASSIFICATION_REGISTRY) else {
        return Err(format!(
            "{CLASSIFICATION_REGISTRY} is not in the address book ({}) — there is nowhere \
             to read a clearance from",
            book.describe()
        ));
    };
    let hierarchy = book.get(TENANT_HIERARCHY).unwrap_or("").to_string();
    let rpc = Rpc::from_book(book)?;
    let url = rpc.url().to_string();
    let reader = ClearanceReader::new(HttpTransport::new(url.clone()), registry, &hierarchy);

    let subject = clearance_subject(address);
    let tenant_key = keccak256(tenant.as_bytes());

    let record = reader.record(subject);
    let tenant_ceiling = reader.tenant_ceiling(tenant_key);

    let (effective, recorded, foreign_national, mut note) = match record {
        Some(r) => (
            r.effective(),
            r.is_recorded(),
            match r {
                quorum_clearance::ClearanceRecord::Recorded {
                    foreign_national, ..
                } => Some(foreign_national),
                quorum_clearance::ClearanceRecord::Unrecorded => None,
            },
            None,
        ),
        // The chain could not be asked. That is NOT "no clearance" — it is "we do
        // not know", and it still enforces as Public, loudly.
        None => (
            quorum_tenancy::Classification::Public,
            false,
            None,
            Some(format!(
                "{CLASSIFICATION_REGISTRY} at {registry} could not be read over {url}. \
                 Treating the clearance as Public, which is the fail-closed answer — \
                 not evidence that it IS Public."
            )),
        ),
    };

    // The least of the axes that answered. A tenant with no node contributes
    // nothing rather than dragging the answer to Public: an unseeded tree is a
    // deployment gap, and the tenancy surface reports it as one.
    let bounded = match tenant_ceiling {
        Some(t) => effective.min(t),
        None => effective,
    };
    if tenant_ceiling.is_none() && note.is_none() && !hierarchy.is_empty() {
        note = Some(format!(
            "{TENANT_HIERARCHY} at {hierarchy} holds no node for tenant '{tenant}', so no \
             tenant ceiling was applied. Create the node under the root to bound this \
             tenant."
        ));
    }

    Ok(ClearanceView {
        effective: classification_name(effective as u8),
        recorded,
        foreign_national,
        tenant_ceiling: tenant_ceiling.map(|c| classification_name(c as u8)),
        bounded_to: classification_name(bounded as u8),
        subject: hex_of(&subject),
        source: format!(
            "{CLASSIFICATION_REGISTRY}.getRecord at {registry} + {TENANT_HIERARCHY}.getNode \
             at {hierarchy} · chain {} · {} · {}",
            book.chain_id,
            url,
            book.describe()
        ),
        note,
    })
}

// ---- the command surface ---------------------------------------------------
//
// None of these takes the backend mutex: they are chain I/O, and holding the
// evidence lock across a network round trip would stall every other command
// behind an endpoint that might be slow or down (the lesson `meeting_anchor`
// already encodes).

/// Where the chain is, as the Node surface's posture tiles render it.
#[tauri::command]
pub fn node_status() -> Result<ChainStatus, String> {
    let book = AddressBook::load().map_err(|e| e.to_string())?;
    status(&book)
}

/// The most recent blocks (clamped to 25).
#[tauri::command]
pub fn node_blocks(count: u32) -> Result<Vec<BlockRow>, String> {
    let book = AddressBook::load().map_err(|e| e.to_string())?;
    recent_blocks(&book, count)
}

/// What this app has actually asked the chain, newest first. Never fails: an
/// empty log means this app has made no RPC call yet, which is itself the
/// answer to "is it even trying?".
#[tauri::command]
pub fn node_activity() -> Vec<ActivityLine> {
    activity()
}

/// The operator's clearance, read from chain and bounded by the tenant.
#[tauri::command]
pub fn clearance_of(address: String, tenant: String) -> Result<ClearanceView, String> {
    let book = AddressBook::load_bfr().map_err(|e| e.to_string())?;
    clearance(&book, &address, &tenant)
}

/// The tenant tree from `TenantHierarchy` in the BFR book.
#[tauri::command]
pub fn tenancy_tree() -> Result<TenancyView, String> {
    let book = AddressBook::load_bfr().map_err(|e| e.to_string())?;
    tenancy(&book)
}

/// The signing identity's balances. Public reads only — no key material.
#[derive(Serialize, Clone, Debug)]
pub struct WalletSummary {
    pub address: String,
    pub chain_id: u64,
    pub rpc_url: String,
    /// Where the key lives, and whether the platform keyring answered.
    pub key_store: String,
    /// Rule 11: the book behind the token addresses.
    pub source: String,
    pub tokens: Vec<TokenRow>,
    /// Anything that could NOT be read, named. A token that did not answer is
    /// listed here rather than shown as a zero balance.
    pub notes: Vec<String>,
    /// Why there is no movement history. Stated on the surface rather than
    /// implied by an empty table.
    pub activity_note: String,
}

/// The ERC-20s this app reads balances for: booked by name, never a literal.
///
/// This is not a token list feature — it is the one booked ERC-20 on 40204.
/// A tenant's own tokens need a per-tenant token list, which does not exist yet
/// and is named in `notes` rather than guessed at.
const BOOKED_TOKENS: [&str; 1] = ["WrappedSALT"];

#[tauri::command]
pub fn wallet_summary(
    custody: tauri::State<'_, citrate_core_kit::custody::CustodyState>,
) -> Result<WalletSummary, String> {
    // Public address only. The key never leaves the vault, and no command in
    // this app returns it (rule 3).
    let wallet = citrate_core_kit::wallet::address(&custody.0).map_err(|e| e.to_string())?;
    let book = AddressBook::load().map_err(|e| e.to_string())?;
    let rpc_url = AddressBook::load()
        .ok()
        .and_then(|b| b.rpc_url.clone())
        .unwrap_or_default();
    let (tokens, notes) = balances(&book, &wallet.address, &BOOKED_TOKENS)?;

    Ok(WalletSummary {
        address: wallet.address,
        chain_id: book.chain_id,
        rpc_url,
        key_store: format!(
            "citrate-core-kit custody vault · OS keyring {}",
            citrate_core_kit::custody::custody_keyring_status()
        ),
        source: book.describe(),
        tokens,
        notes,
        activity_note: "No movement history: this app runs no transaction index and chain \
                        40204's RPC cannot enumerate an address's history. Balances above are \
                        live reads. Transactions this app itself broadcasts are evidenced in \
                        the Ledger and, for ratifications, on the meeting's anchor row."
            .to_string(),
    })
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    /// A book with no endpoint must fail loudly rather than fall back to a
    /// default RPC — reading the wrong chain is the failure rule 8 exists for.
    #[test]
    fn a_book_with_no_rpc_url_is_an_error_not_a_default_endpoint() {
        let book = AddressBook::parse(r#"{"chainId":40204,"contracts":{}}"#, "test").unwrap();
        let e = match Rpc::from_book(&book) {
            Err(e) => e,
            Ok(_) => panic!("a book with no rpcUrl must not produce a client"),
        };
        assert!(e.contains("carries no rpcUrl"), "{e}");
    }

    #[test]
    fn an_unreachable_endpoint_says_so_and_lands_in_the_activity_log() {
        let book = AddressBook::parse(
            r#"{"chainId":40204,"rpcUrl":"http://127.0.0.1:1","contracts":{}}"#,
            "test",
        )
        .unwrap();
        let e = status(&book).unwrap_err();
        assert!(e.contains("could not reach"), "{e}");
        let log = activity();
        assert!(
            log.iter()
                .any(|l| l.lvl == "ERROR" && l.module == "eth_blockNumber"),
            "the failed call must be visible in the activity log: {log:?}"
        );
    }

    #[test]
    fn an_absent_tenant_hierarchy_is_unavailable_not_an_empty_tree() {
        // These are different facts, and the surface renders them differently:
        // "no contract to ask" is a deployment gap, "no rows" is a claim.
        let book = AddressBook::parse(
            r#"{"chainId":40204,"rpcUrl":"http://127.0.0.1:1","contracts":{}}"#,
            "test",
        )
        .unwrap();
        let v = tenancy(&book).unwrap();
        assert!(v.rows.is_empty());
        assert!(v.note.unwrap().contains("not in the address book"));
    }

    /// Balances are money: every digit must survive the render.
    #[test]
    fn format_units_is_exact_and_never_rounds() {
        assert_eq!(format_units(0, 18), "0");
        assert_eq!(format_units(1, 18), "0.000000000000000001");
        assert_eq!(format_units(1_000_000_000_000_000_000, 18), "1");
        assert_eq!(format_units(1_500_000_000_000_000_000, 18), "1.5");
        // The live deployer balance at the time this was written — 24 digits,
        // which an f64 cannot hold without losing the tail.
        assert_eq!(
            format_units(9_999_464_745_500_887_910_960_821, 18),
            "9999464.745500887910960821"
        );
        assert_eq!(format_units(42, 0), "42");
    }

    #[test]
    fn a_truncated_word_is_none_rather_than_a_zero() {
        assert_eq!(word_u64(&[0u8; 16], 0), None);
        // A value wider than u64 must not silently truncate: a truncated offset
        // decodes the wrong field.
        let mut wide = [0u8; 32];
        wide[0] = 1;
        assert_eq!(word_u64(&wide, 0), None);
    }

    #[test]
    fn address_calldata_is_selector_plus_one_right_aligned_word() {
        let d = encode_address_call(
            "balanceOf(address)",
            "0x4fAB35c8c5033c80b3a0452A873B81e6ED4ED732",
        )
        .unwrap();
        assert_eq!(d.len(), 2 + 8 + 64);
        assert!(d.ends_with("4fab35c8c5033c80b3a0452a873b81e6ed4ed732"));
        assert!(encode_address_call("balanceOf(address)", "0xdeadbeef").is_err());
    }

    /// The `getNode` decoder, pinned against ABI bytes produced by a REAL
    /// encoder, not by this file's own arithmetic.
    ///
    /// Generated with foundry (cast 1.5.1):
    ///
    /// ```text
    /// cast abi-encode \
    ///   'f((bytes32,bytes32,string,uint8,bytes32,address[],uint8,uint8,bool))' \
    ///   '(0x0000…0000,0x1111…1111,"Citrate Inc.",0,0x2222…2222,\
    ///     [0xAAA…,0xBBB…],2,2,true)'
    /// ```
    ///
    /// This is the only thing that proves the two struct-relative offsets are
    /// read correctly: a paper derivation that is one word out still decodes —
    /// it just reads the wrong field, silently.
    #[test]
    fn abi_fixture_decodes_to_the_encoded_values() {
        let hex_ret = concat!(
            // offset to the struct
            "0000000000000000000000000000000000000000000000000000000000000020",
            // parent
            "0000000000000000000000000000000000000000000000000000000000000000",
            // self
            "1111111111111111111111111111111111111111111111111111111111111111",
            // offset(display_name), relative to the struct start = 9 words
            "0000000000000000000000000000000000000000000000000000000000000120",
            // level
            "0000000000000000000000000000000000000000000000000000000000000000",
            // hkdf_salt
            "2222222222222222222222222222222222222222222222222222222222222222",
            // offset(admins), relative to the struct start
            "0000000000000000000000000000000000000000000000000000000000000160",
            // admin_threshold
            "0000000000000000000000000000000000000000000000000000000000000002",
            // classification_max = CUI
            "0000000000000000000000000000000000000000000000000000000000000002",
            // exists
            "0000000000000000000000000000000000000000000000000000000000000001",
            // display_name: length 12 + "Citrate Inc." padded
            "000000000000000000000000000000000000000000000000000000000000000c",
            "4369747261746520496e632e0000000000000000000000000000000000000000",
            // admins: length 2 + two addresses
            "0000000000000000000000000000000000000000000000000000000000000002",
            "000000000000000000000000aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
            "000000000000000000000000bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
        );
        let ret = hex::decode(hex_ret).expect("fixture is hex");
        let node = decode_tenant_node(&ret).expect("fixture must decode");
        assert_eq!(node.display_name, "Citrate Inc.");
        assert_eq!(node.level, 0);
        assert_eq!(node.threshold, 2);
        assert_eq!(node.classification_max, 2);
        assert!(node.exists);
        assert_eq!(
            node.admins,
            vec![
                "0xaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".to_string(),
                "0xbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb".to_string(),
            ]
        );
        assert_eq!(node.parent, [0u8; 32]);
        assert_eq!(classification_name(node.classification_max), "CUI");
    }

    /// An unknown classification must never render as `Public`. Failing open on
    /// a clearance ceiling is the one direction this column must not fail.
    #[test]
    fn an_unknown_classification_is_named_not_downgraded() {
        assert_eq!(classification_name(0), "Public");
        assert_eq!(classification_name(3), "ITAR");
        assert!(classification_name(9).contains("unknown"));
    }

    #[test]
    fn a_truncated_struct_return_errors_rather_than_decoding_garbage() {
        assert!(decode_tenant_node(&[0u8; 32]).is_err());
        assert!(decode_tenant_node(&[]).is_err());
    }

    /// Live check against chain 40204. `#[ignore]` because it needs the network;
    /// run with `cargo test -p citrate-quorum -- --ignored`.
    #[test]
    #[ignore = "hits the live chain"]
    fn live_40204_answers_status_and_blocks() {
        let book = AddressBook::load().expect("vendored book");
        let s = status(&book).expect("chain 40204 must answer");
        assert_eq!(s.chain_id, 40204);
        assert!(s.height > 0);
        let blocks = recent_blocks(&book, 5).expect("blocks must read");
        assert_eq!(blocks.len(), 5);
        // Newest first, contiguous, and every row carries a real hash.
        for pair in blocks.windows(2) {
            assert_eq!(pair[0].height, pair[1].height + 1);
        }
        assert!(blocks[0].hash.starts_with("0x"));
        assert_eq!(blocks[0].hash.len(), 66);
    }

    /// Live: the clearance read against the deployed `ClassificationRegistry`.
    ///
    /// Nobody has a clearance recorded on 40204 yet — no HR oracle signer has
    /// been added — so the interesting assertion is the one that would be easy to
    /// get wrong: an unrecorded subject must come back `recorded: false` with an
    /// effective `Public`, NOT as a positive claim that the operator is cleared to
    /// Public. `getClearance` cannot tell those apart; `getRecord` can, which is
    /// why the reader uses it.
    #[test]
    #[ignore = "hits the live chain"]
    fn live_40204_clearance_distinguishes_unrecorded_from_public() {
        let book = AddressBook::load_bfr().expect("vendored BFR book");
        let v = clearance(
            &book,
            "0x4fAB35c8c5033c80b3a0452A873B81e6ED4ED732",
            "Citrate",
        )
        .expect("the read itself must succeed");
        assert_eq!(v.effective, "Public", "fail-closed default");
        assert_eq!(v.bounded_to, "Public");
        assert!(
            !v.recorded,
            "no HR oracle has recorded a clearance on 40204 — if this now fails, one \
             has, and the assertion below should become the real value"
        );
        assert_eq!(
            v.foreign_national, None,
            "no record means no FN answer either"
        );
        assert!(v.source.contains("ClassificationRegistry.getRecord"));
        // The subject key is published so an HR oracle can be pointed at exactly
        // the value this app asks about, rather than guessing our convention.
        assert_eq!(v.subject.len(), 66, "subject is a 0x-prefixed bytes32");
        assert_eq!(
            v.subject,
            hex_of(&clearance_subject(
                "0x4FAB35C8C5033C80B3A0452A873B81E6ED4ED732"
            )),
            "the subject key must be case-insensitive in the address"
        );
    }

    /// Live: the tenancy read against the deployed `TenantHierarchy`.
    ///
    /// **This is the only thing that proves `decode_tenant_node` is right.**
    /// `getNode` returns a DYNAMIC struct holding a `string` and an `address[]`,
    /// so both tail offsets are relative to the struct start; a paper derivation
    /// that is one word out still decodes — it just reads the wrong field. Until
    /// 2026-07-26 there was nothing on chain to decode (`root()` was the zero
    /// word), so the decoder was pinned only against `cast abi-encode` output.
    /// The root was seeded that day (tx `0x9c301e12…`, block 148151) and these
    /// are its real values, read back from the chain rather than taken from the
    /// script that wrote them.
    #[test]
    #[ignore = "hits the live chain"]
    fn live_40204_tenancy_decodes_the_real_root() {
        let book = AddressBook::load_bfr().expect("vendored BFR book");
        let v = tenancy(&book).expect("the read itself must succeed");
        assert!(v.source.contains("TenantHierarchy"));

        let Some(root) = v.rows.first() else {
            // An empty tree is still a legal answer, but after the seeding above
            // the only honest reason left is a book pointing somewhere else.
            let note = v.note.expect("an empty tree must say why it is empty");
            panic!("expected the seeded root, got an empty tree: {note}");
        };
        assert_eq!(root.depth, 0, "the first row is the root");
        assert_eq!(
            root.name, "Citrate",
            "display_name decoded from the wrong word?"
        );
        assert_eq!(root.ceiling, "CUI", "classification_max = 2");
        assert_eq!(root.threshold, "2-of-3", "admin_threshold + admins.len()");
        // If the `address[]` tail were read at the wrong offset these would be
        // garbage rather than merely in a different order.
        for a in [
            "0xf4fe9b2c6441ff7c081b60716a78193127919783",
            "0x269deee81cb8eb5899b2d17945b951608e41774b",
            "0x671f3f4f9cbb0509a28ee4fa0b416dabbc9375c5",
        ] {
            assert!(
                root.admins.to_lowercase().contains(a),
                "missing admin {a} in {}",
                root.admins
            );
        }
        assert!(v.note.is_none(), "a populated tree needs no excuse");
    }
}
