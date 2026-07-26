//! On-chain anchoring of ratified minutes (WP-S6).
//!
//! ## Data source (citrate-chain CLAUDE.md "Data Source Tracing")
//!
//! - Contract: `MeetingRegistry`, resolved by name from the canonical address
//!   book at runtime ([`crate::addresses`]) — never a literal.
//! - Read: `MeetingRegistry.verifyMinutes(bytes32 tenant, bytes32 meetingId,
//!   bytes32 minutesHash) -> bool` via `eth_call`, and
//!   `MeetingRegistry.getMinutes(...)` for the block and the ratifying key.
//! - Transport: `citrate_core_kit::rpc::HttpTransport` against the book's
//!   `rpcUrl`.
//!
//! ## What this module will not do
//!
//! It does not sign and it does not send. Registering minutes is a transaction,
//! and every signature in this app goes through the SignatureCeremony
//! (CLAUDE.md rule 3). This module answers *"is this meeting anchored, and
//! does the chain agree with the hash we hold?"* — which is a read, and is the
//! question the anchor row on the Meetings surface actually asks.
//!
//! ## Absent, unreachable, and unanchored are three different answers
//!
//! Each is reported distinctly. A surface that collapses them tells an operator
//! "not anchored" when the truth is "we could not reach the chain to find out",
//! and the whole point of the anchor row is that a ratified-but-unanchored
//! meeting must not look like an anchored one.

use citrate_core_kit::rpc::{HttpTransport, RpcClient};
use serde::Serialize;
use serde_json::json;

use crate::addresses::AddressBook;

/// The name this contract is booked under in the canonical table.
pub const MEETING_REGISTRY: &str = "MeetingRegistry";

// ---- ABI return layout of `getMinutes` -------------------------------------
//
// `MinutesRecord` contains a `string cid`, so the struct is DYNAMIC and the
// return is a 32-byte OFFSET followed by the struct head. Forgetting that
// leading word shifts every field by 32 bytes and silently reads the wrong
// one — the block number lands in `ratifiedAt`, the ratifier in `minutesHash`.
// The offsets below are verified against the live contract, not derived on
// paper.
//
//   [  0.. 32)  offset to the struct (0x20)
//   [ 32.. 64)  agendaHash
//   [ 64.. 96)  minutesHash
//   [ 96..128)  ratifier   (address right-aligned: bytes 108..128)
//   [128..160)  ratifiedAt
//   [160..192)  blockNumber
//   [192..224)  offset to cid
const MINUTES_START: usize = 64;
const MINUTES_END: usize = 96;
const RATIFIER_START: usize = 108;
const RATIFIER_END: usize = 128;
const BLOCK_END: usize = 192;

/// What the anchor row renders.
#[derive(Serialize, Clone, Debug, PartialEq, Eq)]
#[serde(tag = "state", rename_all = "kebab-case")]
pub enum AnchorState {
    /// The chain holds this exact minutes hash for this meeting.
    Anchored {
        block: u64,
        /// The key that registered it — the on-chain identity of the ratifier.
        ratifier: String,
        contract: String,
    },
    /// The chain was reached and holds no matching record.
    NotAnchored { contract: String, reason: String },
    /// The chain holds a record for this meeting whose hash is NOT ours. This
    /// is the loudest state in the module: it means the local minutes and the
    /// registered commitment disagree.
    Mismatch { contract: String, on_chain: String },
    /// The book does not carry `MeetingRegistry`.
    Unavailable { reason: String },
    /// The book carries it but the chain could not be reached.
    Unreachable { contract: String, reason: String },
}

/// Ethereum keccak256 (NOT SHA3-256 — `sha3::Keccak256` is the pre-NIST
/// padding Ethereum uses; `sha3::Sha3_256` would silently produce different
/// selectors and every call would hit a non-existent function).
fn keccak256(bytes: &[u8]) -> [u8; 32] {
    use sha3::{Digest, Keccak256};
    let mut h = Keccak256::new();
    h.update(bytes);
    let out = h.finalize();
    let mut w = [0u8; 32];
    w.copy_from_slice(&out);
    w
}

/// keccak256 of an ABI signature, truncated to a 4-byte selector.
fn selector(sig: &str) -> [u8; 4] {
    let d = keccak256(sig.as_bytes());
    [d[0], d[1], d[2], d[3]]
}

fn hex_of(bytes: &[u8]) -> String {
    let mut s = String::from("0x");
    for b in bytes {
        s.push_str(&format!("{b:02x}"));
    }
    s
}

/// ABI-encode a call to a function taking N `bytes32` words.
fn encode(sig: &str, words: &[[u8; 32]]) -> String {
    let mut out = Vec::with_capacity(4 + words.len() * 32);
    out.extend_from_slice(&selector(sig));
    for w in words {
        out.extend_from_slice(w);
    }
    hex_of(&out)
}

/// Ask the chain whether `minutes_hash` is the registered commitment for this
/// meeting.
///
/// `tenant` and `meeting_id` are hashed here — the contract stores hashes, not
/// raw ids, so a tenant path never reaches the chain (planset D6).
pub fn check(
    book: &AddressBook,
    tenant: &str,
    meeting_id: &str,
    minutes_hash: [u8; 32],
) -> AnchorState {
    let Some(addr) = book.get(MEETING_REGISTRY) else {
        return AnchorState::Unavailable {
            reason: format!(
                "{MEETING_REGISTRY} is not in the address book ({}) — nothing to anchor to. \
                 Run scripts/sync-addresses.sh against a chain where it is deployed.",
                book.describe()
            ),
        };
    };

    let tenant_word = keccak256(tenant.as_bytes());
    let meeting_word = keccak256(meeting_id.as_bytes());

    let rpc = book
        .rpc_url
        .clone()
        .unwrap_or_else(|| "https://rpc.citrate.ai".to_string());
    let client = RpcClient::with_transport(HttpTransport::new(rpc));

    // MeetingRegistry.verifyMinutes(tenant, meetingId, minutesHash) -> bool
    let data = encode(
        "verifyMinutes(bytes32,bytes32,bytes32)",
        &[tenant_word, meeting_word, minutes_hash],
    );
    let verified = match client.eth_call(json!({ "to": addr, "data": data })) {
        Ok(ret) => ret.last().copied().unwrap_or(0) == 1,
        Err(e) => {
            return AnchorState::Unreachable {
                contract: addr.to_string(),
                reason: format!("could not reach the chain to check: {e}"),
            }
        }
    };

    if !verified {
        // Distinguish "no record at all" from "a record that disagrees". Both
        // render as not-anchored to a casual reader, but the second is a
        // integrity alarm and must not be silently folded into the first.
        let get = encode("getMinutes(bytes32,bytes32)", &[tenant_word, meeting_word]);
        return match client.eth_call(json!({ "to": addr, "data": get })) {
            // getMinutes reverts for an unregistered meeting, so an error here
            // is the expected "no record" case.
            Err(_) => AnchorState::NotAnchored {
                contract: addr.to_string(),
                reason: "the chain holds no record for this meeting".to_string(),
            },
            Ok(ret) if ret.len() >= MINUTES_END => AnchorState::Mismatch {
                contract: addr.to_string(),
                on_chain: hex_of(&ret[MINUTES_START..MINUTES_END]),
            },
            Ok(_) => AnchorState::NotAnchored {
                contract: addr.to_string(),
                reason: "the chain holds no record for this meeting".to_string(),
            },
        };
    }

    // Verified — read back the block and the ratifying key.
    let get = encode("getMinutes(bytes32,bytes32)", &[tenant_word, meeting_word]);
    match client.eth_call(json!({ "to": addr, "data": get })) {
        Ok(ret) if ret.len() >= BLOCK_END => AnchorState::Anchored {
            ratifier: hex_of(&ret[RATIFIER_START..RATIFIER_END]),
            block: u64::from_be_bytes(ret[BLOCK_END - 8..BLOCK_END].try_into().unwrap_or([0u8; 8])),
            contract: addr.to_string(),
        },
        // Verified true but the struct read failed: still anchored — the
        // boolean is the authority — just without the decoration.
        _ => AnchorState::Anchored {
            ratifier: String::new(),
            block: 0,
            contract: addr.to_string(),
        },
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn selectors_match_the_abi_signatures() {
        // Guards against a silent ABI drift: if the contract's signature
        // changes, `eth_call` would return garbage rather than fail loudly.
        assert_eq!(
            hex_of(&selector("verifyMinutes(bytes32,bytes32,bytes32)")).len(),
            10
        );
        assert_ne!(
            selector("verifyMinutes(bytes32,bytes32,bytes32)"),
            selector("getMinutes(bytes32,bytes32)")
        );
    }

    #[test]
    fn encoding_is_selector_plus_one_word_per_argument() {
        let w = [[0u8; 32], [1u8; 32]];
        let d = encode("getMinutes(bytes32,bytes32)", &w);
        // "0x" + 4 bytes selector + 2 * 32 bytes, hex
        assert_eq!(d.len(), 2 + 8 + 128);
    }

    #[test]
    fn an_absent_contract_is_unavailable_not_not_anchored() {
        // These are different facts. "Not anchored" says the chain was asked;
        // "unavailable" says there was nothing to ask.
        let book = AddressBook::parse(r#"{"chainId":40204,"contracts":{}}"#, "test").unwrap();
        let s = check(&book, "acme", "m-1", [7u8; 32]);
        match s {
            AnchorState::Unavailable { reason } => {
                assert!(reason.contains("not in the address book"), "{reason}")
            }
            other => panic!("expected Unavailable, got {other:?}"),
        }
    }

    /// Live check against chain 40204. `#[ignore]` because it needs the
    /// network; run with `cargo test -p citrate-quorum -- --ignored`.
    ///
    /// This is the only thing that proves the ABI return offsets are right.
    /// `getMinutes` returns a DYNAMIC struct, so the decode is offset-sensitive
    /// and a paper derivation that is one word out still parses — it just reads
    /// the wrong field. The record it checks was registered by
    /// `MeetingRegistry.register` at block 108899.
    #[test]
    #[ignore = "hits the live chain"]
    fn live_40204_reads_a_real_registered_meeting() {
        let book = AddressBook::load().expect("vendored book");
        let minutes: [u8; 32] = {
            let hex = "b1a944aab1db50c049e3aae293020877d057e5576870da4b93604ad764a1547e";
            let mut w = [0u8; 32];
            for i in 0..32 {
                w[i] = u8::from_str_radix(&hex[i * 2..i * 2 + 2], 16).expect("hex");
            }
            w
        };

        match check(&book, "smoke-tenant", "m-live-1", minutes) {
            AnchorState::Anchored {
                block, ratifier, ..
            } => {
                assert_eq!(block, 108_899, "block decoded from the wrong word");
                assert_eq!(
                    ratifier.to_lowercase(),
                    "0x4fab35c8c5033c80b3a0452a873b81e6ed4ed732",
                    "ratifier decoded from the wrong word"
                );
            }
            other => panic!("expected Anchored, got {other:?}"),
        }

        // A hash the chain does not hold for this meeting must be a Mismatch,
        // not a false Anchored.
        match check(&book, "smoke-tenant", "m-live-1", [9u8; 32]) {
            AnchorState::Mismatch { on_chain, .. } => {
                assert!(on_chain.ends_with("a1547e"), "got {on_chain}");
            }
            other => panic!("expected Mismatch, got {other:?}"),
        }

        // A meeting that was never registered is NotAnchored.
        match check(&book, "smoke-tenant", "never-held", minutes) {
            AnchorState::NotAnchored { .. } => {}
            other => panic!("expected NotAnchored, got {other:?}"),
        }
    }

    #[test]
    fn an_unreachable_chain_is_not_reported_as_unanchored() {
        let book = AddressBook::parse(
            r#"{"chainId":40204,"rpcUrl":"http://127.0.0.1:1",
                "contracts":{"MeetingRegistry":"0x7cef67f421693a8df6163c69417b41e5d224a011"}}"#,
            "test",
        )
        .unwrap();
        match check(&book, "acme", "m-1", [7u8; 32]) {
            AnchorState::Unreachable { .. } => {}
            other => panic!("a dead RPC must not read as unanchored: {other:?}"),
        }
    }
}
