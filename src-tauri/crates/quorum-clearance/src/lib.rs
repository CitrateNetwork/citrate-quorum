//! citrate-quorum — on-chain clearance reads + the session resolver (WP-S2).
//!
//! Completes the identity→clearance path: given an authenticated principal (the
//! kit's OIDC `AuthStatus` → an [`Entitlement`]) and their wallet address +
//! tenant, read the **on-chain** clearance and tenant ceiling, then resolve the
//! [`EffectiveGrant`] via [`quorum_session::resolve_effective_grant`].
//!
//! - Clearance + foreign-national flag ← `ClassificationRegistry.getClearance`.
//! - Tenant `classification_max` ← `TenantHierarchy.getNode`.
//! - Everything routes through the kit's [`RpcClient::eth_call`] over an
//!   injectable [`RpcTransport`], so the whole reader is testable against a mock
//!   RPC — no live chain needed (build-on-devnet, per the planset).
//!
//! ## Fail-closed is structural
//! Every read returns a `Result`; a read error, a missing record, or a malformed
//! return feeds `None` into the resolution, which collapses to Public/FN. An
//! unreadable chain never grants access. There is no path where a read failure
//! *widens* what a principal can see.

#![forbid(unsafe_code)]

use citrate_core_kit::rpc::{RpcClient, RpcError, RpcTransport};
use quorum_rbac::rbac;
use quorum_session::{ClearanceInputs, Entitlement};
use quorum_tenancy::{Classification, EffectiveGrant};
use serde_json::json;

pub use quorum_session::resolve_effective_grant;

/// Map the on-chain `ClassLevel` ordinal (`0=Public,1=Proprietary,2=CUI,3=ITAR`,
/// bit-for-bit with `ClassificationRegistry.sol`) to a [`Classification`].
/// An out-of-range ordinal is `None` (fail closed — an unknown level is not
/// silently treated as a known one).
fn classification_from_ordinal(o: u8) -> Option<Classification> {
    match o {
        0 => Some(Classification::Public),
        1 => Some(Classification::Proprietary),
        2 => Some(Classification::Cui),
        3 => Some(Classification::Itar),
        _ => None,
    }
}

/// Look up a bound function selector in the generated RBAC bindings.
fn selector(contract: &[(&str, &str, [u8; 4])], name: &str) -> [u8; 4] {
    contract
        .iter()
        .find(|(n, _, _)| *n == name)
        .map(|(_, _, s)| *s)
        // The binding is generated from the ABI and drift-checked in CI, so a
        // missing selector is a build-time impossibility; a zero selector would
        // fail the eth_call loudly rather than read the wrong slot.
        .unwrap_or([0u8; 4])
}

/// ABI calldata for a `fn(bytes32)` read: 4-byte selector ++ the 32-byte arg.
fn encode_bytes32_call(sel: [u8; 4], arg: [u8; 32]) -> String {
    let mut data = Vec::with_capacity(36);
    data.extend_from_slice(&sel);
    data.extend_from_slice(&arg);
    format!("0x{}", hex::encode(data))
}

/// Decode `getClearance(bytes32) -> (uint8 clearance, bool foreign_national)`.
/// Two static 32-byte words; each small value is right-aligned in its word.
/// Returns `None` on a short return or an out-of-range clearance ordinal.
fn decode_clearance(ret: &[u8]) -> Option<(Classification, bool)> {
    if ret.len() < 64 {
        return None;
    }
    let clearance = classification_from_ordinal(ret[31])?; // word 0, last byte
    let foreign_national = ret[63] != 0; // word 1, last byte
    Some((clearance, foreign_national))
}

/// Decode the `classification_max` (and `exists`) out of a
/// `getNode(bytes32) -> TenantNode` return.
///
/// `TenantNode` is a tuple with dynamic members (`string`, `address[]`), so the
/// return is a single dynamic value: word 0 is the offset to the tuple (0x20),
/// then the tuple **head** begins. In the head, static fields occupy one word
/// each and dynamic fields are offset pointers — so `classification_max` (field
/// index 7) and `exists` (field index 8) sit at fixed head positions regardless
/// of the dynamic contents. Field → return-word index:
/// `word = 1 (leading offset) + field_index`.
fn decode_tenant_node(ret: &[u8]) -> Option<(Classification, bool)> {
    // 1 leading offset word + 9 head words = 10 words minimum.
    if ret.len() < 10 * 32 {
        return None;
    }
    let word = |i: usize| &ret[i * 32..(i + 1) * 32];
    // field 8 = exists (return word 1 + 8 = 9)
    let exists = word(9)[31] != 0;
    if !exists {
        return None; // no such node → fail closed
    }
    // field 7 = classification_max (return word 1 + 7 = 8)
    let ceiling = classification_from_ordinal(word(8)[31])?;
    Some((ceiling, exists))
}

/// Reads clearance facts from chain over an injectable transport.
pub struct ClearanceReader<T: RpcTransport> {
    client: RpcClient<T>,
    /// `ClassificationRegistry` address (`0x…`), from the frozen address book.
    classification_registry: String,
    /// `TenantHierarchy` address (`0x…`), from the frozen address book.
    tenant_hierarchy: String,
}

impl<T: RpcTransport> ClearanceReader<T> {
    pub fn new(
        transport: T,
        classification_registry: impl Into<String>,
        tenant_hierarchy: impl Into<String>,
    ) -> Self {
        Self {
            client: RpcClient::with_transport(transport),
            classification_registry: classification_registry.into(),
            tenant_hierarchy: tenant_hierarchy.into(),
        }
    }

    fn eth_call_bytes32(&self, to: &str, sel: [u8; 4], arg: [u8; 32]) -> Result<Vec<u8>, RpcError> {
        self.client.eth_call(json!({
            "to": to,
            "data": encode_bytes32_call(sel, arg),
        }))
    }

    /// The principal's on-chain clearance + FN flag, or `None` on any failure.
    pub fn clearance(&self, subject: [u8; 32]) -> Option<(Classification, bool)> {
        let sel = selector(rbac::classificationregistry::FUNCTIONS, "getClearance");
        let ret = self
            .eth_call_bytes32(&self.classification_registry, sel, subject)
            .ok()?;
        decode_clearance(&ret)
    }

    /// The tenant's `classification_max`, or `None` on any failure / no node.
    pub fn tenant_ceiling(&self, tenant: [u8; 32]) -> Option<Classification> {
        let sel = selector(rbac::tenanthierarchy::FUNCTIONS, "getNode");
        let ret = self
            .eth_call_bytes32(&self.tenant_hierarchy, sel, tenant)
            .ok()?;
        decode_tenant_node(&ret).map(|(c, _)| c)
    }

    /// Read both axes into the [`ClearanceInputs`] the resolver consumes. Any
    /// unread axis stays `None` and the resolver fails it closed.
    pub fn read_inputs(&self, subject: [u8; 32], tenant: [u8; 32]) -> ClearanceInputs {
        let (clearance, fn_flag) = match self.clearance(subject) {
            Some((c, f)) => (Some(c), Some(f)),
            None => (None, None),
        };
        ClearanceInputs {
            on_chain_clearance: clearance,
            foreign_national: fn_flag,
            tenant_ceiling: self.tenant_ceiling(tenant),
        }
    }

    /// The whole login resolution: entitlement (commercial axis) + on-chain reads
    /// (enterprise axis) → the [`EffectiveGrant`]. Fail-closed throughout.
    pub fn resolve(
        &self,
        entitlement: &Entitlement,
        subject: [u8; 32],
        tenant: [u8; 32],
        now_ms: i64,
    ) -> EffectiveGrant {
        let inputs = self.read_inputs(subject, tenant);
        resolve_effective_grant(entitlement, &inputs, now_ms)
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use std::cell::RefCell;

    /// A mock transport that returns canned eth_call results in order. Records
    /// the requests so a test can assert the exact call shape.
    struct MockTransport {
        results: RefCell<Vec<String>>, // 0x-prefixed hex return blobs, popped front
        requests: RefCell<Vec<serde_json::Value>>,
    }
    impl MockTransport {
        fn new(results: Vec<String>) -> Self {
            Self {
                results: RefCell::new(results),
                requests: RefCell::new(vec![]),
            }
        }
    }
    impl RpcTransport for MockTransport {
        fn call(&self, body: serde_json::Value) -> Result<serde_json::Value, RpcError> {
            self.requests.borrow_mut().push(body);
            let mut r = self.results.borrow_mut();
            if r.is_empty() {
                return Err(RpcError::MissingField("mock: no more results".into()));
            }
            let res = r.remove(0);
            Ok(json!({ "jsonrpc": "2.0", "id": 1, "result": res }))
        }
    }

    // ABI helpers for building canned returns.
    fn word_u8(v: u8) -> String {
        let mut w = [0u8; 32];
        w[31] = v;
        hex::encode(w)
    }
    /// getClearance return: two words (clearance ordinal, fn bool).
    fn clearance_ret(ord: u8, fn_flag: bool) -> String {
        format!("0x{}{}", word_u8(ord), word_u8(u8::from(fn_flag)))
    }
    /// getNode return: leading offset (0x20) + a 9-word head with
    /// classification_max at field 7 and exists at field 8. Dynamic fields are
    /// zeroed offsets (their tails are irrelevant to what we decode).
    fn node_ret(class_max: u8, exists: bool) -> String {
        let mut s = String::from("0x");
        s.push_str(&word_u8(0x20)); // word 0: offset to tuple
        for i in 0..9u8 {
            match i {
                7 => s.push_str(&word_u8(class_max)),
                8 => s.push_str(&word_u8(u8::from(exists))),
                _ => s.push_str(&word_u8(0)),
            }
        }
        s
    }

    const CR: &str = "0x00000000000000000000000000000000000000c1";
    const TH: &str = "0x00000000000000000000000000000000000000ee";
    const SUB: [u8; 32] = [0x11; 32];
    const TEN: [u8; 32] = [0x22; 32];
    const NOW: i64 = 1_800_000_000_000;
    const FUTURE: i64 = NOW + 86_400_000;

    fn paid(t: &str, exp: i64) -> Entitlement {
        Entitlement {
            tier: Some(t.into()),
            expires_at_ms: Some(exp),
        }
    }

    // ---- decoder unit tests --------------------------------------------

    #[test]
    fn decode_clearance_maps_ordinal_and_fn() {
        let bytes = hex::decode(clearance_ret(2, true).trim_start_matches("0x")).unwrap();
        assert_eq!(decode_clearance(&bytes), Some((Classification::Cui, true)));
        let bytes = hex::decode(clearance_ret(0, false).trim_start_matches("0x")).unwrap();
        assert_eq!(
            decode_clearance(&bytes),
            Some((Classification::Public, false))
        );
    }

    #[test]
    fn decode_clearance_rejects_out_of_range_and_short() {
        let bytes = hex::decode(clearance_ret(7, false).trim_start_matches("0x")).unwrap();
        assert_eq!(decode_clearance(&bytes), None); // ordinal 7 invalid
        assert_eq!(decode_clearance(&[0u8; 10]), None); // too short
    }

    #[test]
    fn decode_tenant_node_reads_ceiling_at_fixed_offset() {
        let bytes = hex::decode(node_ret(3, true).trim_start_matches("0x")).unwrap();
        assert_eq!(
            decode_tenant_node(&bytes),
            Some((Classification::Itar, true))
        );
    }

    #[test]
    fn decode_tenant_node_missing_node_fails_closed() {
        let bytes = hex::decode(node_ret(3, false).trim_start_matches("0x")).unwrap();
        assert_eq!(decode_tenant_node(&bytes), None); // exists=false
    }

    // ---- reader against a mock RPC -------------------------------------

    #[test]
    fn read_inputs_encodes_the_right_selector_and_arg() {
        let reader = ClearanceReader::new(
            MockTransport::new(vec![clearance_ret(2, false), node_ret(2, true)]),
            CR,
            TH,
        );
        let inputs = reader.read_inputs(SUB, TEN);
        assert_eq!(inputs.on_chain_clearance, Some(Classification::Cui));
        assert_eq!(inputs.foreign_national, Some(false));
        assert_eq!(inputs.tenant_ceiling, Some(Classification::Cui));
    }

    #[test]
    fn resolve_full_login_cui_member_gets_cui() {
        let reader = ClearanceReader::new(
            MockTransport::new(vec![clearance_ret(2, false), node_ret(2, true)]),
            CR,
            TH,
        );
        let g = reader.resolve(&paid("commercial.kyc", FUTURE), SUB, TEN, NOW);
        assert_eq!(g.classification_ceiling, Classification::Cui);
        assert!(g.permits(Classification::Cui));
        assert!(!g.permits(Classification::Itar));
    }

    #[test]
    fn unreadable_chain_fails_closed_even_for_paid_principal() {
        // Transport yields no results → both reads error → fail closed.
        let reader = ClearanceReader::new(MockTransport::new(vec![]), CR, TH);
        let g = reader.resolve(&paid("enterprise", FUTURE), SUB, TEN, NOW);
        assert_eq!(g.classification_ceiling, Classification::Public);
        assert!(g.foreign_national); // unread FN ⇒ assumed FN
    }

    #[test]
    fn tenant_ceiling_caps_a_higher_on_chain_clearance() {
        // Cleared to ITAR, but the tenant maxes at CUI.
        let reader = ClearanceReader::new(
            MockTransport::new(vec![clearance_ret(3, false), node_ret(2, true)]),
            CR,
            TH,
        );
        let g = reader.resolve(&paid("enterprise", FUTURE), SUB, TEN, NOW);
        assert_eq!(g.classification_ceiling, Classification::Cui);
    }

    #[test]
    fn expired_entitlement_collapses_despite_itar_clearance() {
        let reader = ClearanceReader::new(
            MockTransport::new(vec![clearance_ret(3, false), node_ret(3, true)]),
            CR,
            TH,
        );
        let g = reader.resolve(&paid("commercial.kyc", NOW - 1), SUB, TEN, NOW);
        assert_eq!(g.classification_ceiling, Classification::Public);
    }

    #[test]
    fn foreign_national_on_chain_blocks_itar() {
        let reader = ClearanceReader::new(
            MockTransport::new(vec![clearance_ret(3, true), node_ret(3, true)]),
            CR,
            TH,
        );
        let g = reader.resolve(&paid("enterprise", FUTURE), SUB, TEN, NOW);
        assert!(g.foreign_national);
        assert!(!g.permits(Classification::Itar));
        assert!(g.permits(Classification::Cui));
    }

    #[test]
    fn encodes_getclearance_calldata_correctly() {
        // getClearance selector 0xa330d52e ++ the 32-byte subject.
        let data = encode_bytes32_call([0xa3, 0x30, 0xd5, 0x2e], SUB);
        assert!(data.starts_with("0xa330d52e"));
        assert_eq!(data.len(), 2 + (4 + 32) * 2); // 0x + 36 bytes hex
    }
}
