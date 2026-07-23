//! citrate-quorum — RBAC contract bindings (WP-S1.5).
//!
//! Function selectors and event signatures for the six BFR-02 RBAC contracts
//! (`TenantHierarchy`, `RoleEscalation`, `ClassificationRegistry`,
//! `MultiSigEnvelope`, `AgentDecisionRegistryV2`, `ContradictionLedger`), the
//! on-chain access model Quorum reads (`01_RESEARCH_BASELINE.md` §3.1).
//!
//! ## Provenance — read before touching `generated/rbac.rs`
//! The generated module is produced by `scripts/gen_rbac_bindings.py` from the
//! chain's compiled ABIs. **It is regenerated fresh; it is never copied from
//! `citrate-boeing-shell/gui/citrate_rbac_bindings`** — that repo is private and
//! customer-specific to a different account (Q2). A CI drift check
//! (`--check`) fails the build if the committed output diverges from the ABIs.
//!
//! ## Scope today
//! Selectors + event signatures — the deterministic primitives for calldata
//! dispatch and log filtering, dependency-free. Richer typed encode/decode
//! (via `alloy`) arrives when a domain first needs it, in a later sprint.

#![forbid(unsafe_code)]

mod generated {
    #[allow(dead_code)]
    pub mod rbac {
        include!("generated/rbac.rs");
    }
}

pub use generated::rbac;

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::rbac;

    #[test]
    fn all_six_contracts_are_bound() {
        assert_eq!(rbac::CONTRACTS.len(), 6);
        for c in [
            "TenantHierarchy",
            "RoleEscalation",
            "ClassificationRegistry",
            "MultiSigEnvelope",
            "AgentDecisionRegistryV2",
            "ContradictionLedger",
        ] {
            assert!(rbac::CONTRACTS.contains(&c), "{c} must be bound");
        }
    }

    #[test]
    fn selectors_are_four_bytes_and_present() {
        // TenantHierarchy is the root of the access model; spot-check a known fn.
        let (_, _, sel) = rbac::tenanthierarchy::FUNCTIONS
            .iter()
            .find(|(n, _, _)| *n == "getPath")
            .expect("getPath must be bound");
        assert_eq!(sel.len(), 4);
        // getPath(bytes32) selector is 0x27403b34 (from foundry methodIdentifiers).
        assert_eq!(sel, &[0x27, 0x40, 0x3b, 0x34]);
    }

    #[test]
    fn contradiction_ledger_binds_its_events() {
        // The Belnap contradiction surface is a signature feature; its events
        // power the ledger view, so they must be bound.
        assert!(
            !rbac::contradictionledger::EVENTS.is_empty(),
            "ContradictionLedger must expose event signatures"
        );
    }

    #[test]
    fn selectors_are_unique_within_a_contract() {
        for (name, funcs) in [
            ("TenantHierarchy", rbac::tenanthierarchy::FUNCTIONS),
            ("ContradictionLedger", rbac::contradictionledger::FUNCTIONS),
        ] {
            let mut seen = std::collections::HashSet::new();
            for (_, sig, sel) in funcs {
                assert!(seen.insert(*sel), "{name}: duplicate selector for {sig}");
            }
        }
    }
}
