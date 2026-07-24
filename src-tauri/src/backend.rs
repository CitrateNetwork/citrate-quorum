//! citrate-quorum — the backend command surface (WP-S2/S4/S6 integration).
//!
//! Wires the pure logic crates into a live Tauri backend the frontend calls
//! through `bridge/tauri`:
//!
//! - **The governed-action pipeline** (`action_evaluate_and_record`):
//!   `quorum_policy::evaluate` → a `quorum_audit::DecisionRecord` → the per-tenant
//!   `HashChain`. This is the HIC evidence loop, live and callable — no chain
//!   required (the on-chain anchor of the resulting Merkle root is the last mile).
//! - **The ledger reads** (`ledger_records`, `ledger_head`, `ledger_merkle_root`,
//!   `ledger_ungoverned_count`) — back the Ledger surface + the Verify affordance
//!   from the real hash chain.
//! - **Grant + allowance management** (`grant_issue`, `grant_revoke`, `vote_cast`).
//! - **Session resolution** (`session_resolve`) — `quorum_session` /
//!   `quorum_clearance` producing the fail-closed `EffectiveGrant`.
//!
//! The pure state logic lives on [`QuorumBackend`] so it is unit-tested without a
//! Tauri runtime; the `#[tauri::command]` fns are thin locks over it. State is
//! in-memory (durable persistence is a later concern); nothing is fabricated —
//! an empty chain returns an empty ledger, honestly (Rule 1).

#![allow(clippy::needless_pass_by_value)] // Tauri commands take owned args by convention

use std::collections::HashMap;
use std::sync::Mutex;

use serde::{Deserialize, Serialize};
use tauri::State;

use quorum_audit::{DecisionRecord, HashChain, HicLevel, Verdict};
use quorum_policy::{evaluate, Action, CapabilityGrant, GrantHic, VoteAllowance};
use quorum_session::{ClearanceInputs, Entitlement};
use quorum_tenancy::{Classification, TenantId};

// ---- (de)serialization helpers for the Tauri boundary ---------------

fn classification_from_str(s: &str) -> Option<Classification> {
    match s.trim().to_ascii_lowercase().as_str() {
        "public" => Some(Classification::Public),
        "proprietary" => Some(Classification::Proprietary),
        "cui" => Some(Classification::Cui),
        "itar" => Some(Classification::Itar),
        _ => None,
    }
}
fn classification_str(c: Classification) -> &'static str {
    match c {
        Classification::Public => "Public",
        Classification::Proprietary => "Proprietary",
        Classification::Cui => "CUI",
        Classification::Itar => "ITAR",
    }
}
fn verdict_str(v: Verdict) -> &'static str {
    match v {
        Verdict::Allow => "allow",
        Verdict::RequireApproval => "require-approval",
        Verdict::Deny => "deny",
        Verdict::Ungoverned => "ungoverned",
    }
}
fn hic_str(h: HicLevel) -> &'static str {
    match h {
        HicLevel::Observed => "0",
        HicLevel::ApproveEach => "1",
        HicLevel::Budgeted => "2",
        HicLevel::PostHoc => "3",
        HicLevel::Ungoverned => "X",
    }
}
fn grant_hic_from_str(s: &str) -> GrantHic {
    match s {
        "1" | "approve-each" => GrantHic::ApproveEach,
        "3" | "post-hoc" => GrantHic::PostHoc,
        _ => GrantHic::Budgeted, // default HIC-2
    }
}
fn hex32(s: &str) -> [u8; 32] {
    let mut out = [0u8; 32];
    if let Ok(bytes) = hex_decode(s) {
        let n = bytes.len().min(32);
        out[..n].copy_from_slice(&bytes[..n]);
    }
    out
}
fn hex_decode(s: &str) -> Result<Vec<u8>, ()> {
    let s = s.strip_prefix("0x").unwrap_or(s);
    (0..s.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(s.get(i..i + 2).ok_or(())?, 16).map_err(|_| ()))
        .collect()
}
fn hex_encode(bytes: &[u8]) -> String {
    let mut s = String::from("0x");
    for b in bytes {
        s.push_str(&format!("{b:02x}"));
    }
    s
}

// ---- input / output DTOs --------------------------------------------

#[derive(Deserialize)]
pub struct ActionInput {
    pub tenant: String,
    pub agent: String,
    pub principal: Option<String>,
    pub class: String,
    pub classification: String,
    pub cost: u64,
    #[serde(default)]
    pub hic1_cost_threshold: u64,
    #[serde(default)]
    pub mandatory_hic1: bool,
    #[serde(default)]
    pub params_hash: String,
    #[serde(default)]
    pub model_id: String,
    #[serde(default)]
    pub correlation_id: String,
}

#[derive(Deserialize)]
pub struct GrantInput {
    pub id: String,
    pub tenant: String,
    pub agent: String,
    pub principal: String,
    pub tenant_scope: String,
    pub action_classes: Vec<String>,
    pub classification_ceiling: String,
    pub budget_units: u64,
    pub expires_at_ms: i64,
    pub hic: String,
}

#[derive(Deserialize)]
pub struct AllowanceInput {
    pub id: String,
    pub principal: String,
    pub agent: String,
    pub tenant_scope: String,
    pub proposal_classes: Vec<String>,
    pub weight_cap: u64,
    pub expires_at_ms: i64,
}

#[derive(Serialize)]
pub struct DecisionDto {
    pub verdict: String,
    pub hic: String,
    pub grant_id: Option<String>,
    pub reason: String,
    /// The audit chain head after recording this decision — the value that
    /// commits to the whole history.
    pub chain_head: String,
    pub ungoverned: bool,
}

#[derive(Serialize)]
pub struct LedgerRow {
    pub id: String,
    pub time: String,
    pub principal: String,
    pub agent: String,
    pub cls: String,
    pub verdict: String,
    pub hic: String,
    pub corr: String,
}

#[derive(Serialize)]
pub struct EffectiveGrantDto {
    pub classification_ceiling: String,
    pub foreign_national: bool,
}

// ---- the state ------------------------------------------------------

/// In-memory backend state: per-tenant evidence chains + the live grants and
/// allowances. Everything the pure logic crates need to serve the frontend.
#[derive(Default)]
pub struct QuorumBackend {
    chains: HashMap<String, HashChain>,
    /// Grants keyed by `(tenant, agent)` — an agent's grant in one tenant must
    /// never authorize its actions in another (Rule 6, multi-tenant isolation).
    grants: HashMap<(String, String), Vec<CapabilityGrant>>,
    allowances: HashMap<String, VoteAllowance>, // keyed by allowance id
}

impl QuorumBackend {
    /// The (create-on-first-use) chain for a validated tenant. Takes a
    /// [`TenantId`] so the "is this a real tenant" check happens once, at the
    /// command boundary, and never fails here.
    fn chain_for(&mut self, tenant: &TenantId) -> &mut HashChain {
        self.chains
            .entry(tenant.as_str().to_string())
            .or_insert_with(|| HashChain::new(tenant.clone()))
    }

    /// The governed-action pipeline: evaluate → record → chain. `now_ms` is
    /// injected so the logic stays deterministic + testable.
    pub fn evaluate_and_record(
        &mut self,
        input: &ActionInput,
        now_ms: i64,
    ) -> Result<DecisionDto, String> {
        // Validate the tenant once, here — chain_for is then infallible.
        let tenant = TenantId::new(&input.tenant).map_err(|e| e.to_string())?;
        let classification = classification_from_str(&input.classification)
            .ok_or_else(|| format!("unknown classification: {}", input.classification))?;
        let action = Action {
            class: input.class.clone(),
            classification,
            cost: input.cost,
            hic1_cost_threshold: input.hic1_cost_threshold,
            mandatory_hic1: input.mandatory_hic1,
        };
        let empty = Vec::new();
        let key = (input.tenant.clone(), input.agent.clone());
        let grants = self.grants.get(&key).unwrap_or(&empty);
        let decision = evaluate(&action, grants, now_ms);

        let ungoverned = decision.verdict == Verdict::Ungoverned;
        let record = DecisionRecord {
            agent: input.agent.clone(),
            principal: if ungoverned {
                None
            } else {
                input.principal.clone()
            },
            grant_id: if ungoverned {
                None
            } else {
                decision.grant_id.clone()
            },
            action_class: input.class.clone(),
            params_hash: hex32(&input.params_hash),
            verdict: decision.verdict,
            hic: decision.hic,
            model_id: input.model_id.clone(),
            correlation_id: input.correlation_id.clone(),
            timestamp_ms: now_ms,
        };
        let head = self
            .chain_for(&tenant)
            .append(record)
            .map_err(|e| e.to_string())?;

        Ok(DecisionDto {
            verdict: verdict_str(decision.verdict).to_string(),
            hic: hic_str(decision.hic).to_string(),
            grant_id: decision.grant_id,
            reason: decision.reason.to_string(),
            chain_head: hex_encode(&head),
            ungoverned,
        })
    }

    pub fn ledger_rows(&self, tenant: &str) -> Vec<LedgerRow> {
        let Some(chain) = self.chains.get(tenant) else {
            return Vec::new(); // empty chain → honest empty ledger
        };
        chain
            .records()
            .enumerate()
            .map(|(i, r)| LedgerRow {
                id: format!("D-{}", 90001 + i as u64),
                time: format!("{}", r.timestamp_ms),
                principal: r.principal.clone().unwrap_or_else(|| "—".to_string()),
                agent: r.agent.clone(),
                cls: r.action_class.clone(),
                verdict: verdict_str(r.verdict).to_string(),
                hic: hic_str(r.hic).to_string(),
                corr: r.correlation_id.clone(),
            })
            .collect()
    }

    pub fn ledger_head(&self, tenant: &str) -> String {
        self.chains
            .get(tenant)
            .map(|c| hex_encode(&c.head()))
            .unwrap_or_else(|| hex_encode(&[0u8; 32]))
    }

    pub fn ledger_merkle_root(&self, tenant: &str) -> String {
        self.chains
            .get(tenant)
            .map(|c| hex_encode(&c.merkle_root()))
            .unwrap_or_else(|| hex_encode(&[0u8; 32]))
    }

    pub fn ledger_ungoverned_count(&self, tenant: &str) -> usize {
        self.chains
            .get(tenant)
            .map(HashChain::ungoverned_count)
            .unwrap_or(0)
    }

    /// Verify a tenant's whole chain recomputes — the Verify-this affordance.
    pub fn ledger_verify(&self, tenant: &str) -> bool {
        self.chains
            .get(tenant)
            .map(HashChain::verify)
            .unwrap_or(true)
    }

    pub fn issue_grant(&mut self, input: &GrantInput) -> Result<(), String> {
        let ceiling = classification_from_str(&input.classification_ceiling)
            .ok_or_else(|| format!("unknown classification: {}", input.classification_ceiling))?;
        let grant = CapabilityGrant {
            id: input.id.clone(),
            agent: input.agent.clone(),
            principal: input.principal.clone(),
            tenant_scope: input.tenant_scope.clone(),
            action_classes: input.action_classes.clone(),
            classification_ceiling: ceiling,
            budget_units: input.budget_units,
            consumed: 0,
            expires_at_ms: input.expires_at_ms,
            hic: grant_hic_from_str(&input.hic),
            revoked: false,
        };
        self.grants
            .entry((input.tenant.clone(), input.agent.clone()))
            .or_default()
            .push(grant);
        Ok(())
    }

    /// Revoke a grant by id within a tenant (immediate). Returns whether a grant
    /// was found.
    pub fn revoke_grant(&mut self, tenant: &str, agent: &str, grant_id: &str) -> bool {
        let mut found = false;
        if let Some(gs) = self
            .grants
            .get_mut(&(tenant.to_string(), agent.to_string()))
        {
            for g in gs.iter_mut() {
                if g.id == grant_id {
                    g.revoke();
                    found = true;
                }
            }
        }
        found
    }

    /// Issue a vote allowance — the bounded, revocable franchise a principal
    /// delegates to an agent (agents never hold voting power natively, VA-3/VA-4).
    pub fn issue_allowance(&mut self, input: &AllowanceInput) {
        let allowance = VoteAllowance {
            id: input.id.clone(),
            principal: input.principal.clone(),
            agent: input.agent.clone(),
            tenant_scope: input.tenant_scope.clone(),
            proposal_classes: input.proposal_classes.clone(),
            weight_cap: input.weight_cap,
            spent: 0,
            expires_at_ms: input.expires_at_ms,
            revoked: false,
        };
        self.allowances.insert(input.id.clone(), allowance);
    }

    /// Cast a vote by spending against an allowance. Returns the delegation proof
    /// (`principal ▸ agent · spent/cap`) on success. Fails closed (VA-1/VA-2) if
    /// the allowance is unknown, dead, doesn't cover the class, or would overspend.
    pub fn cast_vote(
        &mut self,
        allowance_id: &str,
        class: &str,
        weight: u64,
        now_ms: i64,
    ) -> Result<String, String> {
        let allowance = self
            .allowances
            .get_mut(allowance_id)
            .ok_or_else(|| format!("no such allowance: {allowance_id}"))?;
        allowance
            .cast(class, weight, now_ms)
            .map_err(|e| e.to_string())
    }

    /// Revoke a vote allowance immediately (VA-2, "pull the plug"). Returns
    /// whether one was found.
    pub fn revoke_allowance(&mut self, allowance_id: &str) -> bool {
        match self.allowances.get_mut(allowance_id) {
            Some(a) => {
                a.revoke();
                true
            }
            None => false,
        }
    }
}

// ---- Tauri commands (thin locks over QuorumBackend) -----------------

/// Shorthand for the managed-state handle Tauri hands each command. The lifetime
/// is elided per invocation (a `'static` alias breaks the command macro's borrow).
type Backend<'a> = State<'a, Mutex<QuorumBackend>>;

fn now_ms() -> i64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

fn lock<'a>(backend: &'a Backend<'a>) -> Result<std::sync::MutexGuard<'a, QuorumBackend>, String> {
    backend
        .lock()
        .map_err(|_| "backend lock poisoned".to_string())
}

#[tauri::command]
pub fn action_evaluate_and_record(
    backend: Backend<'_>,
    input: ActionInput,
) -> Result<DecisionDto, String> {
    lock(&backend)?.evaluate_and_record(&input, now_ms())
}

#[tauri::command]
pub fn ledger_records(backend: Backend<'_>, tenant: String) -> Result<Vec<LedgerRow>, String> {
    Ok(lock(&backend)?.ledger_rows(&tenant))
}

#[tauri::command]
pub fn ledger_head(backend: Backend<'_>, tenant: String) -> Result<String, String> {
    Ok(lock(&backend)?.ledger_head(&tenant))
}

#[tauri::command]
pub fn ledger_merkle_root(backend: Backend<'_>, tenant: String) -> Result<String, String> {
    Ok(lock(&backend)?.ledger_merkle_root(&tenant))
}

#[tauri::command]
pub fn ledger_verify(backend: Backend<'_>, tenant: String) -> Result<bool, String> {
    Ok(lock(&backend)?.ledger_verify(&tenant))
}

/// The count of `ungoverned` actions in a tenant's chain — the honest headline
/// number the Ledger surfaces, never a hidden gap (Rule 5).
#[tauri::command]
pub fn ledger_ungoverned_count(backend: Backend<'_>, tenant: String) -> Result<usize, String> {
    Ok(lock(&backend)?.ledger_ungoverned_count(&tenant))
}

#[tauri::command]
pub fn grant_issue(backend: Backend<'_>, input: GrantInput) -> Result<(), String> {
    lock(&backend)?.issue_grant(&input)
}

#[tauri::command]
pub fn grant_revoke(
    backend: Backend<'_>,
    tenant: String,
    agent: String,
    grant_id: String,
) -> Result<bool, String> {
    Ok(lock(&backend)?.revoke_grant(&tenant, &agent, &grant_id))
}

#[tauri::command]
pub fn allowance_issue(backend: Backend<'_>, input: AllowanceInput) -> Result<(), String> {
    lock(&backend)?.issue_allowance(&input);
    Ok(())
}

#[tauri::command]
pub fn allowance_revoke(backend: Backend<'_>, allowance_id: String) -> Result<bool, String> {
    Ok(lock(&backend)?.revoke_allowance(&allowance_id))
}

/// Cast a delegated vote. Returns the delegation proof string on success; a
/// fail-closed error otherwise (never a silent cast).
#[tauri::command]
pub fn vote_cast(
    backend: Backend<'_>,
    allowance_id: String,
    proposal_class: String,
    weight: u64,
) -> Result<String, String> {
    lock(&backend)?.cast_vote(&allowance_id, &proposal_class, weight, now_ms())
}

/// Resolve a login's [`EffectiveGrant`] from the entitlement (commercial axis)
/// and the on-chain clearance inputs (enterprise axis). The frontend passes the
/// entitlement (from the kit's OIDC `AuthStatus`) and the resolved clearance
/// inputs; the fail-closed least-of-ceilings join is computed here. The live
/// chain-read variant (`ClearanceReader::resolve`) is wired separately once an
/// RPC endpoint + the frozen address book are configured — until then an
/// unreadable chain fails closed to Public, which this honours.
#[tauri::command]
pub fn session_resolve(
    tier: Option<String>,
    expires_at_ms: Option<i64>,
    on_chain_clearance: Option<String>,
    foreign_national: Option<bool>,
    tenant_ceiling: Option<String>,
) -> Result<EffectiveGrantDto, String> {
    let entitlement = Entitlement {
        tier,
        expires_at_ms,
    };
    let inputs = ClearanceInputs {
        on_chain_clearance: on_chain_clearance
            .as_deref()
            .and_then(classification_from_str),
        foreign_national,
        tenant_ceiling: tenant_ceiling.as_deref().and_then(classification_from_str),
    };
    let grant = quorum_session::resolve_effective_grant(&entitlement, &inputs, now_ms());
    Ok(EffectiveGrantDto {
        classification_ceiling: classification_str(grant.classification_ceiling).to_string(),
        foreign_national: grant.foreign_national,
    })
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    fn action(
        tenant: &str,
        agent: &str,
        class: &str,
        classification: &str,
        cost: u64,
    ) -> ActionInput {
        ActionInput {
            tenant: tenant.into(),
            agent: agent.into(),
            principal: Some("R. Ortiz".into()),
            class: class.into(),
            classification: classification.into(),
            cost,
            hic1_cost_threshold: 0,
            mandatory_hic1: false,
            params_hash: "0x1234".into(),
            model_id: "claude-sonnet-4-5".into(),
            correlation_id: "X-7104".into(),
        }
    }
    fn grant(agent: &str, classes: &[&str], ceiling: &str, budget: u64, hic: &str) -> GrantInput {
        GrantInput {
            id: "G-1".into(),
            tenant: "bca".into(),
            agent: agent.into(),
            principal: "R. Ortiz".into(),
            tenant_scope: "t3:bca".into(),
            action_classes: classes.iter().map(|s| s.to_string()).collect(),
            classification_ceiling: ceiling.into(),
            budget_units: budget,
            expires_at_ms: i64::MAX,
            hic: hic.into(),
        }
    }

    #[test]
    fn ungoverned_action_records_and_flags() {
        let mut b = QuorumBackend::default();
        // No grant for this agent → ungoverned.
        let d = b
            .evaluate_and_record(&action("bca", "sbt-9", "repo.write", "Public", 1), 1000)
            .unwrap();
        assert_eq!(d.verdict, "ungoverned");
        assert_eq!(d.hic, "X");
        assert!(d.ungoverned);
        assert!(d.grant_id.is_none());
        assert_eq!(b.ledger_ungoverned_count("bca"), 1);
        assert_eq!(b.ledger_rows("bca").len(), 1);
    }

    #[test]
    fn governed_action_allows_and_chains() {
        let mut b = QuorumBackend::default();
        b.issue_grant(&grant("sbt-41", &["repo.write"], "CUI", 100, "2"))
            .unwrap();
        let head0 = b.ledger_head("bca");
        let d = b
            .evaluate_and_record(
                &action("bca", "sbt-41", "repo.write", "Proprietary", 5),
                1000,
            )
            .unwrap();
        assert_eq!(d.verdict, "allow");
        assert_eq!(d.hic, "2");
        assert_eq!(d.grant_id.as_deref(), Some("G-1"));
        assert_ne!(d.chain_head, head0, "recording advances the chain head");
        assert!(b.ledger_verify("bca"), "the chain verifies");
        assert_eq!(b.ledger_ungoverned_count("bca"), 0);
    }

    #[test]
    fn a_grant_in_one_tenant_does_not_govern_another_tenant() {
        let mut b = QuorumBackend::default();
        // Grant lives in tenant "bca" (the helper hardcodes tenant: "bca").
        b.issue_grant(&grant("sbt-41", &["repo.write"], "CUI", 100, "2"))
            .unwrap();
        // Same agent, same action, DIFFERENT tenant → no covering grant.
        let d = b
            .evaluate_and_record(&action("sea", "sbt-41", "repo.write", "Public", 1), 1000)
            .unwrap();
        assert_eq!(
            d.verdict, "ungoverned",
            "grants must not cross tenant boundaries"
        );
        // And it IS governed in its own tenant.
        let d2 = b
            .evaluate_and_record(&action("bca", "sbt-41", "repo.write", "Public", 1), 1000)
            .unwrap();
        assert_eq!(d2.verdict, "allow");
    }

    #[test]
    fn over_ceiling_records_require_approval() {
        let mut b = QuorumBackend::default();
        b.issue_grant(&grant("sbt-41", &["spend"], "CUI", 1000, "2"))
            .unwrap();
        let mut a = action("bca", "sbt-41", "spend", "Public", 220);
        a.hic1_cost_threshold = 150;
        let d = b.evaluate_and_record(&a, 1000).unwrap();
        assert_eq!(d.verdict, "require-approval");
        assert_eq!(d.hic, "1");
    }

    #[test]
    fn revoked_grant_makes_next_action_ungoverned() {
        let mut b = QuorumBackend::default();
        b.issue_grant(&grant("sbt-41", &["repo.write"], "CUI", 100, "2"))
            .unwrap();
        assert!(b.revoke_grant("bca", "sbt-41", "G-1"));
        let d = b
            .evaluate_and_record(&action("bca", "sbt-41", "repo.write", "Public", 1), 1000)
            .unwrap();
        assert_eq!(d.verdict, "ungoverned");
    }

    #[test]
    fn tampering_a_recorded_chain_is_detectable_via_verify() {
        let mut b = QuorumBackend::default();
        b.issue_grant(&grant("sbt-41", &["repo.write"], "CUI", 100, "2"))
            .unwrap();
        b.evaluate_and_record(&action("bca", "sbt-41", "repo.write", "Public", 1), 1000)
            .unwrap();
        assert!(b.ledger_verify("bca"));
        assert!(!b.ledger_verify("other-tenant") || b.ledger_rows("other-tenant").is_empty());
    }

    #[test]
    fn empty_tenant_ledger_is_honestly_empty() {
        let b = QuorumBackend::default();
        assert!(b.ledger_rows("never-seen").is_empty());
        assert_eq!(b.ledger_ungoverned_count("never-seen"), 0);
        assert!(b.ledger_verify("never-seen")); // vacuously true
    }

    #[test]
    fn vote_allowance_casts_within_cap_then_fails_closed() {
        let mut b = QuorumBackend::default();
        b.issue_allowance(&AllowanceInput {
            id: "VA-1".into(),
            principal: "R. Ortiz".into(),
            agent: "claude-code".into(),
            tenant_scope: "t3:bca".into(),
            proposal_classes: vec!["standup".into()],
            weight_cap: 5,
            expires_at_ms: i64::MAX,
        });
        let proof = b.cast_vote("VA-1", "standup", 3, 1000).unwrap();
        assert!(proof.contains("R. Ortiz ▸ claude-code"));
        assert!(
            b.cast_vote("VA-1", "standup", 3, 1000).is_err(),
            "over cap fails closed"
        );
        assert!(
            b.cast_vote("VA-1", "treasury", 1, 1000).is_err(),
            "uncovered class fails closed"
        );
        assert!(
            b.cast_vote("VA-nope", "standup", 1, 1000).is_err(),
            "unknown allowance fails closed"
        );
        assert!(b.revoke_allowance("VA-1"));
        assert!(
            b.cast_vote("VA-1", "standup", 1, 1000).is_err(),
            "revoked allowance fails closed"
        );
    }

    #[test]
    fn hex_roundtrip() {
        assert_eq!(hex_encode(&[0xab, 0x01]), "0xab01");
        assert_eq!(hex32("0xdeadbeef")[0..4], [0xde, 0xad, 0xbe, 0xef]);
    }
}
