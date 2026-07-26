//! citrate-quorum — the audit evidence spine (WP-S4 core, HIC §4).
//!
//! Every governed action becomes a [`DecisionRecord`]; records are appended to a
//! per-tenant [`HashChain`] whose contiguity is provable with BLAKE3; and the
//! day's records are committed as a [`merkle_root`] to `AnchorRegistry` (the
//! `NightlyMerkle` anchor kind). Together these are the difference between a
//! dashboard and *evidence*: an auditor can pick any record, recompute its hash
//! locally, and check it against what was anchored on chain.
//!
//! Two properties this crate exists to guarantee:
//!
//! 1. **Tamper-evidence.** Editing, reordering, inserting, or dropping any record
//!    changes the chain head. A chain that verifies is a chain no one altered
//!    after the fact ([`HashChain::verify`]).
//! 2. **`ungoverned` is first-class.** A record with no live grant behind it is
//!    marked [`Verdict::Ungoverned`] and carries no `grant_id` — it is never
//!    silently dropped and never silently allowed. It is recorded, and it counts
//!    ([`HashChain::ungoverned_count`]).
//!
//! Pure and deterministic (no clock, no I/O) so the whole spine is exhaustively
//! testable; timestamps are supplied by the caller.

#![forbid(unsafe_code)]

use quorum_tenancy::TenantId;

/// The verdict a policy check produced for an action.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Verdict {
    Allow,
    RequireApproval,
    Deny,
    /// No live grant stood behind this action. Recorded and alerted, never
    /// silently allowed or dropped (the HIC-X state).
    Ungoverned,
    /// A human the action was escalated to REFUSED it. Distinct from `Deny`
    /// (which is policy refusing) because "the person said no" is a different
    /// fact about the organisation than "the rules said no", and an auditor
    /// needs to tell them apart.
    Rejected,
    /// A human the action was escalated to APPROVED it. Distinct from `Allow`
    /// for the same reason `Rejected` is distinct from `Deny`: "a person
    /// decided" and "the rules decided" are different facts, and the whole
    /// point of HIC-1 is being able to show which one happened.
    Approved,
}

impl Verdict {
    fn tag(self) -> u8 {
        match self {
            Verdict::Allow => 0,
            Verdict::RequireApproval => 1,
            Verdict::Deny => 2,
            Verdict::Ungoverned => 3,
            Verdict::Rejected => 4,
            Verdict::Approved => 5,
        }
    }
}

/// The HIC level in force for a recorded action (HIC-0..3, or X = ungoverned).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum HicLevel {
    Observed,
    ApproveEach,
    Budgeted,
    PostHoc,
    Ungoverned,
}

impl HicLevel {
    fn tag(self) -> u8 {
        match self {
            HicLevel::Observed => 0,
            HicLevel::ApproveEach => 1,
            HicLevel::Budgeted => 2,
            HicLevel::PostHoc => 3,
            HicLevel::Ungoverned => 0xFF,
        }
    }
}

/// One recorded governed action — the HIC evidence unit. Every field an auditor
/// needs to walk the action back to a human authority, or to see it flagged
/// `ungoverned`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DecisionRecord {
    /// The acting agent's `AgentSBT` id (or a human principal id for a human act).
    pub agent: String,
    /// The accountable human principal, if any. `None` iff `ungoverned`.
    pub principal: Option<String>,
    /// The capability grant that authorized the action, if any. `None` iff
    /// `ungoverned`.
    pub grant_id: Option<String>,
    /// The action class (e.g. `repo.write`, `spend`, `calendar.write`).
    pub action_class: String,
    /// keccak/blake of the ABI-encoded params — the *what*, without the payload.
    pub params_hash: [u8; 32],
    pub verdict: Verdict,
    pub hic: HicLevel,
    /// Which model (+ LoRA) produced the action — so "which model recommended
    /// this" is answerable.
    pub model_id: String,
    /// Correlation id threading related actions (meeting → grant → actions → PR).
    pub correlation_id: String,
    /// Caller-supplied timestamp (epoch-ms). No ambient clock here.
    pub timestamp_ms: i64,
}

impl DecisionRecord {
    /// Is this record governed — i.e. does a live human authority stand behind
    /// it? An `ungoverned` verdict and the presence of `principal`+`grant_id`
    /// must agree; [`DecisionRecord::is_consistent`] checks that.
    pub fn is_governed(&self) -> bool {
        self.verdict != Verdict::Ungoverned
    }

    /// The invariant that keeps `ungoverned` honest: a record is `Ungoverned`
    /// **iff** it has no principal and no grant. You cannot record an ungoverned
    /// action that secretly names an authority, nor a governed one with none.
    pub fn is_consistent(&self) -> bool {
        let has_authority = self.principal.is_some() && self.grant_id.is_some();
        match self.verdict {
            Verdict::Ungoverned => {
                self.principal.is_none()
                    && self.grant_id.is_none()
                    && self.hic == HicLevel::Ungoverned
            }
            // A rejection is meaningful whether or not the refused action named
            // an authority: a human can refuse an ungoverned action too, and
            // that refusal is exactly the evidence worth keeping. Constraining
            // it to `has_authority` would make the most important record in the
            // system the one we could not write.
            Verdict::Rejected => true,
            // An approval names the human who gave it, ALWAYS — that is the
            // entire evidentiary value of an HIC-1 approval. The grant is
            // optional, because two different things are recorded as approved:
            // a human answering an agent's escalation (which names the agent's
            // grant), and a human acting directly (which names no grant,
            // because the human IS the authority — they do not act under one).
            // Recording the latter as `Ungoverned` would be a lie in the other
            // direction: HIC-X is an alert state, never a description of the
            // operator doing their job.
            Verdict::Approved => self.principal.is_some(),
            _ => has_authority,
        }
    }

    /// Canonical, length-prefixed bytes for hashing. Deterministic across
    /// machines: every field is framed so no two distinct records share an
    /// encoding (no field-boundary ambiguity).
    fn canonical(&self) -> Vec<u8> {
        // Length-prefix each variable-length field so no two distinct records
        // share an encoding (no field-boundary ambiguity).
        fn frame(b: &mut Vec<u8>, bytes: &[u8]) {
            b.extend_from_slice(&(bytes.len() as u32).to_be_bytes());
            b.extend_from_slice(bytes);
        }
        let mut b = Vec::new();
        frame(&mut b, self.agent.as_bytes());
        frame(&mut b, self.principal.as_deref().unwrap_or("").as_bytes());
        frame(&mut b, self.grant_id.as_deref().unwrap_or("").as_bytes());
        frame(&mut b, self.action_class.as_bytes());
        frame(&mut b, &self.params_hash);
        b.push(self.verdict.tag());
        b.push(self.hic.tag());
        frame(&mut b, self.model_id.as_bytes());
        frame(&mut b, self.correlation_id.as_bytes());
        b.extend_from_slice(&self.timestamp_ms.to_be_bytes());
        b
    }

    /// The record's own content hash (independent of chain position).
    pub fn content_hash(&self) -> [u8; 32] {
        *blake3::hash(&self.canonical()).as_bytes()
    }
}

/// A per-tenant, append-only, tamper-evident chain of decision records. Each
/// entry's hash chains the previous head: `h_i = BLAKE3(h_{i-1} ++ record_i)`.
#[derive(Debug)]
pub struct HashChain {
    tenant: TenantId,
    entries: Vec<(DecisionRecord, [u8; 32])>,
    head: [u8; 32],
}

impl HashChain {
    /// A fresh chain for `tenant`, seeded with a genesis hash bound to the tenant
    /// id (so two tenants' empty chains never share a head).
    pub fn new(tenant: TenantId) -> Self {
        let genesis = *blake3::hash(tenant.as_str().as_bytes()).as_bytes();
        Self {
            tenant,
            entries: Vec::new(),
            head: genesis,
        }
    }

    /// Append a record, returning its chained hash. Rejects an inconsistent
    /// record (the `ungoverned`↔authority invariant) so a malformed record can
    /// never enter the evidence chain.
    pub fn append(&mut self, record: DecisionRecord) -> Result<[u8; 32], AuditError> {
        if !record.is_consistent() {
            return Err(AuditError::InconsistentRecord);
        }
        let mut hasher = blake3::Hasher::new();
        hasher.update(&self.head);
        hasher.update(&record.canonical());
        let h = *hasher.finalize().as_bytes();
        self.entries.push((record, h));
        self.head = h;
        Ok(h)
    }

    /// The current chain head — the single value that commits to the entire
    /// history. Anchor this (or a Merkle root) to prove the log on chain.
    pub fn head(&self) -> [u8; 32] {
        self.head
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Recompute the chain from genesis and confirm every stored hash — proves no
    /// record was edited, reordered, inserted, or removed after the fact.
    pub fn verify(&self) -> bool {
        let mut prev = *blake3::hash(self.tenant.as_str().as_bytes()).as_bytes();
        for (record, stored) in &self.entries {
            let mut hasher = blake3::Hasher::new();
            hasher.update(&prev);
            hasher.update(&record.canonical());
            let h = *hasher.finalize().as_bytes();
            if &h != stored {
                return false;
            }
            prev = h;
        }
        prev == self.head
    }

    /// How many recorded actions were `ungoverned`. This is a headline number,
    /// not a hidden gap — the ledger surfaces it and an auditor samples it.
    pub fn ungoverned_count(&self) -> usize {
        self.entries
            .iter()
            .filter(|(r, _)| !r.is_governed())
            .count()
    }

    /// The records, in order.
    pub fn records(&self) -> impl Iterator<Item = &DecisionRecord> {
        self.entries.iter().map(|(r, _)| r)
    }

    /// One entry: its record and the chained hash that was stored for it.
    ///
    /// The chained hash is what makes a single decision citable — "record 3,
    /// entry `b3:…`" is checkable by anyone holding the preceding records,
    /// where the record alone is not.
    pub fn entry(&self, index: usize) -> Option<(&DecisionRecord, [u8; 32])> {
        self.entries.get(index).map(|(r, h)| (r, *h))
    }

    /// A Merkle inclusion proof for record `index` against [`Self::merkle_root`].
    ///
    /// Each step is `(sibling, sibling_is_right)`. Feed it to
    /// [`verify_inclusion`] with the record's content hash to recompute the
    /// root — which is what makes "verify this decision" a real check rather
    /// than a spinner: an auditor holding one record, this proof and the
    /// anchored root needs nothing else from us.
    ///
    /// Mirrors [`merkle_root`] exactly, including the odd-node duplication rule;
    /// `inclusion_proofs_verify_against_the_root` pins the two together for
    /// every chain length up to 9, which is where an off-by-one in the
    /// duplication rule shows up.
    pub fn merkle_proof(&self, index: usize) -> Option<Vec<([u8; 32], bool)>> {
        if index >= self.entries.len() {
            return None;
        }
        let mut level: Vec<[u8; 32]> = self
            .entries
            .iter()
            .map(|(r, _)| leaf_hash(&r.content_hash()))
            .collect();
        let mut i = index;
        let mut proof = Vec::new();
        while level.len() > 1 {
            let sibling_is_right = i % 2 == 0;
            let sibling_index = if sibling_is_right { i + 1 } else { i - 1 };
            // An odd level duplicates its last node, so the last element's
            // sibling is itself.
            let sibling = *level.get(sibling_index).unwrap_or(&level[i]);
            proof.push((sibling, sibling_is_right));

            let mut next = Vec::with_capacity(level.len().div_ceil(2));
            for pair in level.chunks(2) {
                let left = pair[0];
                let right = if pair.len() == 2 { pair[1] } else { pair[0] };
                next.push(node_hash(&left, &right));
            }
            level = next;
            i /= 2;
        }
        Some(proof)
    }

    /// The Merkle root over this chain's record content hashes — the value
    /// committed to `AnchorRegistry` as the day's `NightlyMerkle` anchor. Anyone
    /// with the records can recompute it and verify inclusion.
    pub fn merkle_root(&self) -> [u8; 32] {
        merkle_root(
            &self
                .entries
                .iter()
                .map(|(r, _)| r.content_hash())
                .collect::<Vec<_>>(),
        )
    }
}

/// The domain-separated leaf hash. Split out so [`merkle_root`] and
/// [`HashChain::merkle_proof`] cannot drift apart — a proof built with a
/// different leaf rule than the root verifies against nothing.
fn leaf_hash(leaf: &[u8; 32]) -> [u8; 32] {
    let mut h = blake3::Hasher::new();
    h.update(b"\x00leaf");
    h.update(leaf);
    *h.finalize().as_bytes()
}

/// The domain-separated internal-node hash.
fn node_hash(left: &[u8; 32], right: &[u8; 32]) -> [u8; 32] {
    let mut h = blake3::Hasher::new();
    h.update(b"\x01node");
    h.update(left);
    h.update(right);
    *h.finalize().as_bytes()
}

/// A binary Merkle root over `leaves` (each already a 32-byte hash). An empty set
/// hashes to the zero root; an odd level duplicates its last node (standard).
/// Domain-separated (leaf vs node) so a leaf can't be reinterpreted as a node.
pub fn merkle_root(leaves: &[[u8; 32]]) -> [u8; 32] {
    if leaves.is_empty() {
        return [0u8; 32];
    }
    let mut level: Vec<[u8; 32]> = leaves.iter().map(leaf_hash).collect();
    while level.len() > 1 {
        let mut next = Vec::with_capacity(level.len().div_ceil(2));
        for pair in level.chunks(2) {
            let left = pair[0];
            let right = if pair.len() == 2 { pair[1] } else { pair[0] };
            next.push(node_hash(&left, &right));
        }
        level = next;
    }
    level[0]
}

/// Recompute a Merkle root from one leaf and its inclusion proof.
///
/// `leaf` is the record's own content hash — NOT the leaf hash: the domain
/// separation is applied here, so a caller cannot accidentally verify a node as
/// though it were a leaf.
pub fn verify_inclusion(leaf: [u8; 32], proof: &[([u8; 32], bool)], root: [u8; 32]) -> bool {
    let mut acc = leaf_hash(&leaf);
    for (sibling, sibling_is_right) in proof {
        acc = if *sibling_is_right {
            node_hash(&acc, sibling)
        } else {
            node_hash(sibling, &acc)
        };
    }
    acc == root
}

#[derive(Debug, PartialEq, Eq)]
pub enum AuditError {
    /// The record violates the `ungoverned`↔authority invariant.
    InconsistentRecord,
}

impl core::fmt::Display for AuditError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            AuditError::InconsistentRecord => {
                write!(
                    f,
                    "decision record violates the ungoverned/authority invariant"
                )
            }
        }
    }
}
impl std::error::Error for AuditError {}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    fn tenant() -> TenantId {
        TenantId::new("bca").unwrap()
    }
    fn governed(class: &str, corr: &str, ts: i64) -> DecisionRecord {
        DecisionRecord {
            agent: "sbt-41".into(),
            principal: Some("R. Ortiz".into()),
            grant_id: Some("G-2201".into()),
            action_class: class.into(),
            params_hash: [7u8; 32],
            verdict: Verdict::Allow,
            hic: HicLevel::Budgeted,
            model_id: "claude-sonnet-4-5+gov-lora".into(),
            correlation_id: corr.into(),
            timestamp_ms: ts,
        }
    }
    fn ungoverned(ts: i64) -> DecisionRecord {
        DecisionRecord {
            agent: "sbt-55".into(),
            principal: None,
            grant_id: None,
            action_class: "net.egress".into(),
            params_hash: [9u8; 32],
            verdict: Verdict::Ungoverned,
            hic: HicLevel::Ungoverned,
            model_id: "swe-1.5".into(),
            correlation_id: "X-7104".into(),
            timestamp_ms: ts,
        }
    }

    #[test]
    fn appends_and_verifies() {
        let mut c = HashChain::new(tenant());
        c.append(governed("repo.write", "X-1", 1)).unwrap();
        c.append(governed("pr.open", "X-1", 2)).unwrap();
        c.append(ungoverned(3)).unwrap();
        assert_eq!(c.len(), 3);
        assert!(c.verify());
        assert_eq!(c.ungoverned_count(), 1);
    }

    #[test]
    fn head_changes_per_append() {
        let mut c = HashChain::new(tenant());
        let empty = c.head();
        let h1 = c.append(governed("repo.write", "X-1", 1)).unwrap();
        assert_ne!(empty, h1);
        let h2 = c.append(governed("repo.write", "X-1", 2)).unwrap();
        assert_ne!(h1, h2);
        assert_eq!(c.head(), h2);
    }

    #[test]
    fn tampering_with_a_record_breaks_verification() {
        let mut c = HashChain::new(tenant());
        c.append(governed("repo.write", "X-1", 1)).unwrap();
        c.append(governed("spend", "X-1", 2)).unwrap();
        assert!(c.verify());
        // Silently edit a stored record's action class.
        c.entries[0].0.action_class = "repo.delete".into();
        assert!(!c.verify(), "an edited record must break the chain");
    }

    #[test]
    fn reordering_records_breaks_verification() {
        let mut c = HashChain::new(tenant());
        c.append(governed("a", "X-1", 1)).unwrap();
        c.append(governed("b", "X-1", 2)).unwrap();
        c.entries.swap(0, 1);
        assert!(!c.verify(), "reordering must break the chain");
    }

    #[test]
    fn two_tenants_have_distinct_genesis_and_heads() {
        let a = HashChain::new(TenantId::new("t-a").unwrap());
        let b = HashChain::new(TenantId::new("t-b").unwrap());
        assert_ne!(
            a.head(),
            b.head(),
            "empty chains of different tenants must differ"
        );
    }

    #[test]
    fn inconsistent_records_are_rejected() {
        let mut c = HashChain::new(tenant());
        // Ungoverned verdict but names an authority — forbidden.
        let mut bad = ungoverned(1);
        bad.principal = Some("R. Ortiz".into());
        assert_eq!(c.append(bad), Err(AuditError::InconsistentRecord));
        // Governed verdict with no grant — forbidden.
        let mut bad2 = governed("repo.write", "X-1", 1);
        bad2.grant_id = None;
        assert_eq!(c.append(bad2), Err(AuditError::InconsistentRecord));
        assert!(c.is_empty());
    }

    #[test]
    fn content_hash_is_deterministic_and_sensitive() {
        let r = governed("repo.write", "X-1", 1);
        assert_eq!(r.content_hash(), r.content_hash());
        let mut r2 = r.clone();
        r2.timestamp_ms = 2;
        assert_ne!(r.content_hash(), r2.content_hash());
    }

    #[test]
    fn canonical_encoding_has_no_field_boundary_ambiguity() {
        // ("ab","c") vs ("a","bc") on adjacent string fields must not collide.
        let mut x = governed("x", "y", 1);
        x.agent = "ab".into();
        x.action_class = "c".into();
        let mut z = governed("x", "y", 1);
        z.agent = "a".into();
        z.action_class = "bc".into();
        assert_ne!(x.content_hash(), z.content_hash());
    }

    #[test]
    fn merkle_root_is_deterministic_empty_and_single() {
        assert_eq!(merkle_root(&[]), [0u8; 32]);
        let one = [[1u8; 32]];
        assert_eq!(merkle_root(&one), merkle_root(&one));
        // single != raw leaf (leaf is domain-separated)
        assert_ne!(merkle_root(&one), [1u8; 32]);
    }

    #[test]
    fn merkle_root_changes_if_any_leaf_changes() {
        let a = merkle_root(&[[1u8; 32], [2u8; 32], [3u8; 32]]);
        let b = merkle_root(&[[1u8; 32], [2u8; 32], [4u8; 32]]);
        assert_ne!(a, b);
        // order matters
        let c = merkle_root(&[[2u8; 32], [1u8; 32], [3u8; 32]]);
        assert_ne!(a, c);
    }

    #[test]
    fn chain_merkle_root_tracks_its_records() {
        let mut c = HashChain::new(tenant());
        c.append(governed("repo.write", "X-1", 1)).unwrap();
        let r1 = c.merkle_root();
        c.append(governed("pr.open", "X-1", 2)).unwrap();
        let r2 = c.merkle_root();
        assert_ne!(r1, r2, "adding a record changes the anchor root");
    }

    #[test]
    fn a_human_acting_directly_is_approved_not_ungoverned() {
        // No grant: the operator does not act under one, they issue them.
        let mut direct = governed("grant.issue", "X-1", 1);
        direct.verdict = Verdict::Approved;
        direct.hic = HicLevel::ApproveEach;
        direct.grant_id = None;
        assert!(
            direct.is_consistent(),
            "an operator issuing a grant is the authority, not an alert"
        );

        // But it must still name WHO. An approval nobody owns is not evidence.
        let mut anonymous = direct.clone();
        anonymous.principal = None;
        assert!(!anonymous.is_consistent());
    }

    #[test]
    fn a_human_rejection_is_recordable_with_or_without_authority() {
        // Governed action refused by the human it was escalated to.
        let mut governed_rejection = governed("spend", "X-9", 5);
        governed_rejection.verdict = Verdict::Rejected;
        governed_rejection.hic = HicLevel::ApproveEach;
        assert!(governed_rejection.is_consistent());

        // The same refusal of an action that had no grant behind it. This must
        // also be recordable — it is the most telling record of the two.
        let mut ungoverned_rejection = governed("spend", "X-9", 5);
        ungoverned_rejection.verdict = Verdict::Rejected;
        ungoverned_rejection.hic = HicLevel::ApproveEach;
        ungoverned_rejection.principal = None;
        ungoverned_rejection.grant_id = None;
        assert!(ungoverned_rejection.is_consistent());

        let mut c = HashChain::new(tenant());
        assert!(c.append(governed_rejection).is_ok());
        assert!(c.append(ungoverned_rejection).is_ok());
        assert!(c.verify());
    }

    /// The whole point of an inclusion proof: a holder of ONE record plus the
    /// proof can recompute the anchored root without the other records.
    ///
    /// Swept across every length up to 9 because the odd-node duplication rule
    /// is where a proof and a root drift apart, and it only bites at odd levels.
    #[test]
    fn inclusion_proofs_verify_against_the_root() {
        for n in 1..=9usize {
            let mut c = HashChain::new(tenant());
            for i in 0..n {
                c.append(governed("repo.write", &format!("X-{i}"), i as i64))
                    .unwrap();
            }
            let root = c.merkle_root();
            for i in 0..n {
                let (record, _) = c.entry(i).expect("entry in range");
                let proof = c.merkle_proof(i).expect("proof in range");
                assert!(
                    verify_inclusion(record.content_hash(), &proof, root),
                    "chain of {n}: record {i} did not prove into the root"
                );
            }
        }
    }

    /// A proof must not verify a record the chain does not hold — otherwise
    /// "verified" would mean nothing.
    #[test]
    fn a_foreign_record_does_not_prove_into_the_root() {
        let mut c = HashChain::new(tenant());
        for i in 0..4 {
            c.append(governed("repo.write", &format!("X-{i}"), i))
                .unwrap();
        }
        let proof = c.merkle_proof(2).unwrap();
        let forged = governed("spend", "X-forged", 99);
        assert!(!verify_inclusion(
            forged.content_hash(),
            &proof,
            c.merkle_root()
        ));
        // …nor the right record against the wrong root.
        let (record, _) = c.entry(2).unwrap();
        assert!(!verify_inclusion(record.content_hash(), &proof, [0u8; 32]));
    }

    #[test]
    fn an_out_of_range_index_has_no_entry_and_no_proof() {
        let mut c = HashChain::new(tenant());
        c.append(governed("repo.write", "X-1", 1)).unwrap();
        assert!(c.entry(1).is_none());
        assert!(c.merkle_proof(1).is_none());
        assert!(c.merkle_proof(0).is_some());
    }

    /// The entry hash a citation names must be the one the chain actually
    /// stored — not one recomputed from a different rule.
    #[test]
    fn the_entry_hash_is_the_hash_append_returned() {
        let mut c = HashChain::new(tenant());
        let h = c.append(governed("repo.write", "X-1", 1)).unwrap();
        let (_, stored) = c.entry(0).unwrap();
        assert_eq!(h, stored);
        assert_eq!(stored, c.head());
    }

    #[test]
    fn a_rejection_does_not_collide_with_any_other_verdict() {
        let mut deny = governed("spend", "X-9", 5);
        deny.verdict = Verdict::Deny;
        let mut rejected = governed("spend", "X-9", 5);
        rejected.verdict = Verdict::Rejected;
        assert_ne!(
            deny.content_hash(),
            rejected.content_hash(),
            "policy saying no and a human saying no must not hash alike"
        );
    }
}
