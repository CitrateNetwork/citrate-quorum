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
//! - **The budget** — a governed action charges its grant (`max(1, cost)`), and
//!   `action_reject` refunds exactly that when the human refuses, recording the
//!   refusal as its own `Rejected` decision.
//! - **Session resolution** (`session_resolve`) — `quorum_session` /
//!   `quorum_clearance` producing the fail-closed `EffectiveGrant`.
//!
//! The pure state logic lives on [`QuorumBackend`] so it is unit-tested without a
//! Tauri runtime; the `#[tauri::command]` fns are thin locks over it. Nothing is
//! fabricated — an empty chain returns an empty ledger, honestly (Rule 1).
//!
//! **Evidence is durable.** State is held in memory and mirrored to a
//! [`crate::store::EvidenceStore`]: establishing a scope reloads that tenant's
//! chain (re-proving every link), and every mutation writes through before it is
//! reported as done. A durable write that fails degrades the backend and stops
//! further governed actions, so records cannot silently accumulate unpersisted.
//!
//! **The tenant scope is backend-owned (Rule 6).** No command accepts a tenant
//! from the caller: the frontend cannot name the tenant whose evidence chain it
//! writes to or reads from. `tenant_set` establishes the one active scope and
//! every tenant-keyed operation resolves it from there, failing closed with an
//! honest error when it is unset. The pure fns still take the tenant explicitly
//! so cross-tenant isolation is directly testable.

#![allow(clippy::needless_pass_by_value)] // Tauri commands take owned args by convention

use std::collections::HashMap;
use std::sync::Mutex;

use serde::{Deserialize, Serialize};
use tauri::State;

use quorum_audit::{DecisionRecord, HashChain, HicLevel, Verdict};
use quorum_policy::{evaluate, Action, CapabilityGrant, VoteAllowance};
use quorum_session::{ClearanceInputs, Entitlement};
use quorum_tenancy::TenantId;

// The string codec (verdict/HIC/classification/hex) lives in `store`, which
// owns the at-rest format — so a ledger row and the record persisted behind it
// agree by construction rather than by two parallel match arms.
use crate::store::{
    classification_from_str, classification_str, grant_hic_from_str, hex32, hex_encode, hic_str,
    verdict_str, Charge, EvidenceStore, PendingApproval, StoreError,
};

/// What one governed action costs its grant.
///
/// A spend charges its magnitude; everything else charges one unit. The floor
/// matters: if some actions were free, an agent could run unattended forever
/// inside a "budgeted" envelope, and HIC-2 would mean nothing. Charging at
/// least 1 guarantees every grant depletes and every unattended run eventually
/// returns to a human.
///
/// The denomination follows the grant's scope — a grant over `spend` is
/// denominated in the spend unit, one over work classes is denominated in
/// actions. The same number is used for the coverage check and the charge, so
/// `covers()` and `consume()` can never disagree.
fn charge_for(cost: u64) -> u64 {
    cost.max(1)
}

/// `HH:MM:SS` UTC from epoch-ms, for the Ledger's TIME column.
///
/// The column was rendering the raw epoch integer, which overflowed into the
/// next column and told an auditor nothing. Done by hand rather than pulling in
/// a date library for one format; UTC because an evidence ledger read across
/// sites must not shift under the reader.
fn clock_utc(ms: i64) -> String {
    let secs = ms.div_euclid(1000).rem_euclid(86_400);
    format!(
        "{:02}:{:02}:{:02}",
        secs / 3600,
        (secs % 3600) / 60,
        secs % 60
    )
}

// ---- input / output DTOs --------------------------------------------

/// A proposed governed action, as the frontend describes it. There is
/// deliberately no `tenant` field — the scope comes from backend state.
#[derive(Deserialize)]
pub struct ActionInput {
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

#[derive(Serialize, Debug)]
pub struct DecisionDto {
    /// This decision's index in the tenant's chain — the handle a rejection
    /// refers back to (`action_reject`).
    pub decision_id: u64,
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

/// Backend state: per-tenant evidence chains + the live grants and allowances,
/// held in memory and mirrored to a durable [`EvidenceStore`].
///
/// The in-memory copy is the working set; the store is the record of truth
/// across restarts. Establishing a tenant scope loads that tenant's state from
/// disk (re-proving the chain as it goes); every mutation writes through.
#[derive(Default)]
pub struct QuorumBackend {
    /// The one tenant scope this app instance is operating in. `None` until the
    /// session flow establishes it; every tenant-keyed command fails closed
    /// until then rather than guessing (Rule 6).
    active_tenant: Option<TenantId>,
    chains: HashMap<String, HashChain>,
    /// Grants keyed by `(tenant, agent)` — an agent's grant in one tenant must
    /// never authorize its actions in another (Rule 6, multi-tenant isolation).
    grants: HashMap<(String, String), Vec<CapabilityGrant>>,
    /// Allowances keyed by `(tenant, allowance id)` — the same isolation the
    /// grants have. An allowance id is meaningless outside its tenant.
    allowances: HashMap<(String, String), VoteAllowance>,
    /// Outstanding charges by `(tenant, decision index)` — what each decision
    /// took from its grant, so a rejection refunds exactly that and only once.
    charges: HashMap<(String, u64), Charge>,
    /// Escalations waiting on a human, by `(tenant, decision index)`. An agent
    /// that was told `require-approval` is sitting here until someone acts.
    pending: HashMap<(String, u64), PendingApproval>,
    /// How each answered escalation was answered, by `(tenant, decision index)`.
    ///
    /// Recorded against the EXACT decision rather than inferred from later
    /// records: two escalations from one agent for one tool share an action
    /// class and correlation id, and matching on those made one report the
    /// other's outcome — an agent could be told "approved" for something a
    /// human had refused.
    resolved: HashMap<(String, u64), String>,
    /// Where evidence actually lives. `None` only in unit tests of the pure
    /// state logic; the running app always has one.
    store: Option<EvidenceStore>,
    /// Set when a durable write failed. Once evidence cannot be persisted, we
    /// refuse to keep producing it — unpersisted records must not silently pile
    /// up in RAM and be lost at exit.
    degraded: Option<String>,
}

impl QuorumBackend {
    /// A backend backed by a durable store, resuming the scope it was last left
    /// in. A scope that fails to reload is left unset rather than half-restored;
    /// naming that tenant again surfaces the exact reason.
    pub fn with_store(store: EvidenceStore) -> Self {
        let mut backend = Self {
            store: Some(store),
            ..Self::default()
        };
        if let Some(tenant) = backend.store.as_ref().and_then(EvidenceStore::load_scope) {
            let _ = backend.set_active_tenant(&tenant);
        }
        backend
    }

    /// Establish the active tenant scope, loading that tenant's evidence, grants
    /// and allowances from the store. A chain that fails to re-prove itself is
    /// an error here: the scope is NOT set and nothing is served (fail closed).
    pub fn set_active_tenant(&mut self, tenant: &str) -> Result<(), String> {
        let id = TenantId::new(tenant).map_err(|e| e.to_string())?;
        if let Some(store) = self.store.clone() {
            let key = id.as_str().to_string();
            let chain = store.load_chain(&id).map_err(|e| e.to_string())?;
            let grants = store.load_grants(&key).map_err(|e| e.to_string())?;
            let allowances = store.load_allowances(&key).map_err(|e| e.to_string())?;
            let charges = store.load_charges(&key).map_err(|e| e.to_string())?;
            let pending = store.load_pending(&key).map_err(|e| e.to_string())?;
            let resolved = store.load_resolved(&key).map_err(|e| e.to_string())?;

            // Replace, never merge: re-establishing a scope must not duplicate
            // what is already in memory for it.
            self.grants.retain(|(t, _), _| t != &key);
            self.allowances.retain(|(t, _), _| t != &key);
            self.charges.retain(|(t, _), _| t != &key);
            self.pending.retain(|(t, _), _| t != &key);
            self.resolved.retain(|(t, _), _| t != &key);
            self.chains.insert(key.clone(), chain);
            for g in grants {
                self.grants
                    .entry((key.clone(), g.agent.clone()))
                    .or_default()
                    .push(g);
            }
            for a in allowances {
                self.allowances.insert((key.clone(), a.id.clone()), a);
            }
            for c in charges {
                self.charges.insert((key.clone(), c.decision), c);
            }
            for pa in pending {
                self.pending.insert((key.clone(), pa.decision), pa);
            }
            for (decision, outcome) in resolved {
                self.resolved.insert((key.clone(), decision), outcome);
            }
            store.save_scope(Some(&key)).map_err(|e| e.to_string())?;
        }
        self.active_tenant = Some(id);
        Ok(())
    }

    /// All of a tenant's grants, flattened — the store keeps one file per
    /// tenant while memory keys them by `(tenant, agent)`.
    fn tenant_grants(&self, tenant: &str) -> Vec<CapabilityGrant> {
        self.grants
            .iter()
            .filter(|((t, _), _)| t == tenant)
            .flat_map(|(_, gs)| gs.iter().cloned())
            .collect()
    }

    fn tenant_allowances(&self, tenant: &str) -> Vec<VoteAllowance> {
        self.allowances
            .iter()
            .filter(|((t, _), _)| t == tenant)
            .map(|(_, a)| a.clone())
            .collect()
    }

    /// Record a failed durable write and turn it into the error the caller
    /// returns. After this, tenant-keyed operations refuse (see
    /// [`Self::check_not_degraded`]).
    fn degrade(&mut self, e: StoreError) -> String {
        let msg = format!(
            "{e} — refusing further governed actions until this is resolved, \
             so unpersisted evidence cannot accumulate"
        );
        self.degraded = Some(msg.clone());
        msg
    }

    fn check_not_degraded(&self) -> Result<(), String> {
        match &self.degraded {
            Some(msg) => Err(msg.clone()),
            None => Ok(()),
        }
    }

    fn persist_grants(&mut self, tenant: &str) -> Result<(), String> {
        let Some(store) = self.store.clone() else {
            return Ok(());
        };
        let grants = self.tenant_grants(tenant);
        store
            .save_grants(tenant, &grants)
            .map_err(|e| self.degrade(e))
    }

    fn persist_charges(&mut self, tenant: &str) -> Result<(), String> {
        let Some(store) = self.store.clone() else {
            return Ok(());
        };
        let charges: Vec<Charge> = self
            .charges
            .iter()
            .filter(|((t, _), _)| t == tenant)
            .map(|(_, c)| c.clone())
            .collect();
        store
            .save_charges(tenant, &charges)
            .map_err(|e| self.degrade(e))
    }

    fn persist_pending(&mut self, tenant: &str) -> Result<(), String> {
        let Some(store) = self.store.clone() else {
            return Ok(());
        };
        let pending: Vec<PendingApproval> = self
            .pending
            .iter()
            .filter(|((t, _), _)| t == tenant)
            .map(|(_, p)| p.clone())
            .collect();
        store
            .save_pending(tenant, &pending)
            .map_err(|e| self.degrade(e))
    }

    fn persist_resolved(&mut self, tenant: &str) -> Result<(), String> {
        let Some(store) = self.store.clone() else {
            return Ok(());
        };
        let resolved: Vec<(u64, String)> = self
            .resolved
            .iter()
            .filter(|((t, _), _)| t == tenant)
            .map(|((_, d), outcome)| (*d, outcome.clone()))
            .collect();
        store
            .save_resolved(tenant, &resolved)
            .map_err(|e| self.degrade(e))
    }

    /// The escalations a human still has to answer, oldest first — the ceremony
    /// queue's actual contents.
    pub fn pending_approvals(&self, tenant: &str) -> Vec<PendingApproval> {
        let mut v: Vec<PendingApproval> = self
            .pending
            .iter()
            .filter(|((t, _), _)| t == tenant)
            .map(|(_, p)| p.clone())
            .collect();
        v.sort_by_key(|p| (p.requested_at_ms, p.decision));
        v
    }

    fn persist_allowances(&mut self, tenant: &str) -> Result<(), String> {
        let Some(store) = self.store.clone() else {
            return Ok(());
        };
        let allowances = self.tenant_allowances(tenant);
        store
            .save_allowances(tenant, &allowances)
            .map_err(|e| self.degrade(e))
    }

    /// The active tenant scope, if one has been established.
    pub fn active_tenant(&self) -> Option<String> {
        self.active_tenant.as_ref().map(|t| t.as_str().to_string())
    }

    /// The active tenant as a validated id, for in-process callers that are not
    /// Tauri commands (the agent bridge). `None` means no scope is established,
    /// and the caller must refuse rather than pick one.
    pub fn active_tenant_id(&self) -> Option<TenantId> {
        self.active_tenant.clone()
    }

    /// The active tenant, or an honest error. This is the fail-closed boundary:
    /// no tenant scope means no evidence is read or written, ever.
    fn require_tenant(&self) -> Result<TenantId, String> {
        self.active_tenant.clone().ok_or_else(|| {
            "no active tenant scope — establish one before any governed action".into()
        })
    }

    /// The (create-on-first-use) chain for a validated tenant. Takes a
    /// [`TenantId`] so the "is this a real tenant" check happens once, at the
    /// command boundary, and never fails here.
    fn chain_for(&mut self, tenant: &TenantId) -> &mut HashChain {
        self.chains
            .entry(tenant.as_str().to_string())
            .or_insert_with(|| HashChain::new(tenant.clone()))
    }

    /// The governed-action pipeline: evaluate → record → chain. The tenant is
    /// passed explicitly (commands resolve it from [`Self::require_tenant`]) and
    /// `now_ms` is injected, so the logic stays deterministic + testable.
    pub fn evaluate_and_record(
        &mut self,
        input: &ActionInput,
        tenant: &TenantId,
        now_ms: i64,
    ) -> Result<DecisionDto, String> {
        self.check_not_degraded()?;
        let classification = classification_from_str(&input.classification)
            .ok_or_else(|| format!("unknown classification: {}", input.classification))?;
        // One number for both the coverage check and the charge, so a grant can
        // never be found to "cover" an action it cannot actually pay for.
        let charge = charge_for(input.cost);
        let action = Action {
            class: input.class.clone(),
            classification,
            cost: charge,
            hic1_cost_threshold: input.hic1_cost_threshold,
            mandatory_hic1: input.mandatory_hic1,
        };
        let empty = Vec::new();
        let key = (tenant.as_str().to_string(), input.agent.clone());
        let grants = self.grants.get(&key).unwrap_or(&empty);
        let decision = evaluate(&action, grants, now_ms);

        let ungoverned = decision.verdict == Verdict::Ungoverned;
        // I-4: a governed record must name the accountable human. Without one
        // there is no authority to walk the action back to, so say that plainly
        // rather than emitting a record the chain will reject as inconsistent.
        if !ungoverned && input.principal.is_none() {
            return Err(
                "a governed action must name an accountable principal — no identity resolved"
                    .to_string(),
            );
        }
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
        let chain = self.chain_for(tenant);
        let head = chain.append(record.clone()).map_err(|e| e.to_string())?;
        let decision_id = (chain.len() - 1) as u64;

        // Spend the envelope. Both Allow and RequireApproval charge: escalating
        // to a human is itself use of the grant, and an escalation that cost
        // nothing would let an agent queue unlimited approvals. A rejection
        // gives it back (`reject_decision`).
        if let Some(grant_id) = decision.grant_id.clone() {
            let tenant_key = tenant.as_str().to_string();
            if let Some(gs) = self
                .grants
                .get_mut(&(tenant_key.clone(), input.agent.clone()))
            {
                if let Some(g) = gs.iter_mut().find(|g| g.id == grant_id) {
                    // `covers()` already proved the budget is there; this cannot
                    // fail, and if it somehow did we must not report success.
                    g.consume(charge).map_err(|e| {
                        format!("grant {grant_id} could not be charged {charge}: {e:?}")
                    })?;
                }
            }
            self.charges.insert(
                (tenant_key.clone(), decision_id),
                Charge {
                    decision: decision_id,
                    agent: input.agent.clone(),
                    grant_id,
                    units: charge,
                },
            );
            self.persist_grants(&tenant_key)?;
            self.persist_charges(&tenant_key)?;
        }

        // An escalation is a QUESTION ASKED OF A HUMAN. Park it where the
        // ceremony queue can find it; an agent stopped and is waiting on this.
        if decision.verdict == Verdict::RequireApproval {
            let tenant_key = tenant.as_str().to_string();
            self.pending.insert(
                (tenant_key.clone(), decision_id),
                PendingApproval {
                    decision: decision_id,
                    agent: input.agent.clone(),
                    principal: input.principal.clone(),
                    action_class: input.class.clone(),
                    classification: input.classification.clone(),
                    cost: input.cost,
                    correlation_id: input.correlation_id.clone(),
                    requested_at_ms: now_ms,
                },
            );
            self.persist_pending(&tenant_key)?;
        }

        // Durable BEFORE we report success. A decision the operator is told was
        // recorded, but which would vanish on restart, is not evidence.
        if let Some(store) = self.store.clone() {
            store
                .append_record(tenant.as_str(), &record, head)
                .map_err(|e| self.degrade(e))?;
        }

        Ok(DecisionDto {
            decision_id,
            verdict: verdict_str(decision.verdict).to_string(),
            hic: hic_str(decision.hic).to_string(),
            grant_id: decision.grant_id,
            reason: decision.reason.to_string(),
            chain_head: hex_encode(&head),
            ungoverned,
        })
    }

    /// The human refused. Give the grant back exactly what this decision took,
    /// exactly once, and record the refusal — "the person said no" is a
    /// different fact from "the rules said no", and both belong in the evidence.
    ///
    /// Rejecting an ungoverned decision refunds nothing (it charged nothing) but
    /// still records the refusal.
    pub fn reject_decision(
        &mut self,
        tenant: &TenantId,
        decision_id: u64,
        now_ms: i64,
    ) -> Result<DecisionDto, String> {
        self.check_not_degraded()?;
        let tenant_key = tenant.as_str().to_string();

        // The decision being refused must exist in this tenant's chain.
        let original = self
            .chains
            .get(&tenant_key)
            .and_then(|c| c.records().nth(decision_id as usize).cloned())
            .ok_or_else(|| format!("no decision {decision_id} in this tenant's ledger"))?;

        // The question has been answered; it leaves the queue either way, and
        // the answer is recorded against THIS decision id.
        if self
            .pending
            .remove(&(tenant_key.clone(), decision_id))
            .is_some()
        {
            self.persist_pending(&tenant_key)?;
        }
        self.resolved
            .insert((tenant_key.clone(), decision_id), "rejected".to_string());
        self.persist_resolved(&tenant_key)?;

        // Refund, once. Taking the charge out of the map first means a repeated
        // rejection cannot pay out twice.
        if let Some(charge) = self.charges.remove(&(tenant_key.clone(), decision_id)) {
            if let Some(gs) = self
                .grants
                .get_mut(&(tenant_key.clone(), charge.agent.clone()))
            {
                if let Some(g) = gs.iter_mut().find(|g| g.id == charge.grant_id) {
                    g.refund(charge.units);
                }
            }
            self.persist_grants(&tenant_key)?;
            self.persist_charges(&tenant_key)?;
        }

        let record = DecisionRecord {
            agent: original.agent.clone(),
            principal: original.principal.clone(),
            grant_id: original.grant_id.clone(),
            action_class: original.action_class.clone(),
            params_hash: original.params_hash,
            verdict: Verdict::Rejected,
            hic: HicLevel::ApproveEach,
            model_id: original.model_id.clone(),
            correlation_id: original.correlation_id.clone(),
            timestamp_ms: now_ms,
        };
        let chain = self.chain_for(tenant);
        let head = chain.append(record.clone()).map_err(|e| e.to_string())?;
        let new_id = (chain.len() - 1) as u64;
        if let Some(store) = self.store.clone() {
            store
                .append_record(tenant.as_str(), &record, head)
                .map_err(|e| self.degrade(e))?;
        }

        Ok(DecisionDto {
            decision_id: new_id,
            verdict: verdict_str(Verdict::Rejected).to_string(),
            hic: hic_str(HicLevel::ApproveEach).to_string(),
            grant_id: original.grant_id,
            reason: "RC-300 refused by the human it was escalated to".to_string(),
            chain_head: hex_encode(&head),
            ungoverned: false,
        })
    }

    /// The human said yes. Records the approval as its own decision — naming
    /// who approved it — and takes the escalation out of the queue.
    ///
    /// The charge STAYS spent: an approved action goes ahead, so it consumes
    /// the envelope it was always going to consume. Only a rejection refunds.
    ///
    /// `approver` is the human accountable for the decision. I-4 makes it
    /// mandatory: an approval nobody signed for is not evidence of anything.
    pub fn approve_decision(
        &mut self,
        tenant: &TenantId,
        decision_id: u64,
        approver: &str,
        now_ms: i64,
    ) -> Result<DecisionDto, String> {
        self.check_not_degraded()?;
        if approver.trim().is_empty() {
            return Err("an approval must name the human who gave it".to_string());
        }
        let tenant_key = tenant.as_str().to_string();

        // Only a live escalation can be approved. Approving something that was
        // never escalated — or was already answered — must not mint authority.
        let pending = self
            .pending
            .remove(&(tenant_key.clone(), decision_id))
            .ok_or_else(|| {
                format!("decision {decision_id} is not awaiting approval in this tenant")
            })?;
        self.persist_pending(&tenant_key)?;
        self.resolved
            .insert((tenant_key.clone(), decision_id), "approved".to_string());
        self.persist_resolved(&tenant_key)?;

        let original = self
            .chains
            .get(&tenant_key)
            .and_then(|c| c.records().nth(decision_id as usize).cloned())
            .ok_or_else(|| format!("no decision {decision_id} in this tenant's ledger"))?;

        let record = DecisionRecord {
            agent: original.agent.clone(),
            principal: Some(approver.to_string()),
            grant_id: original.grant_id.clone(),
            action_class: original.action_class.clone(),
            params_hash: original.params_hash,
            verdict: Verdict::Approved,
            hic: HicLevel::ApproveEach,
            model_id: original.model_id.clone(),
            correlation_id: original.correlation_id.clone(),
            timestamp_ms: now_ms,
        };
        let chain = self.chain_for(tenant);
        let head = chain.append(record.clone()).map_err(|e| e.to_string())?;
        let new_id = (chain.len() - 1) as u64;
        if let Some(store) = self.store.clone() {
            store
                .append_record(tenant.as_str(), &record, head)
                .map_err(|e| self.degrade(e))?;
        }

        Ok(DecisionDto {
            decision_id: new_id,
            verdict: verdict_str(Verdict::Approved).to_string(),
            hic: hic_str(HicLevel::ApproveEach).to_string(),
            grant_id: original.grant_id,
            reason: format!("RC-301 approved by {approver} for {}", pending.agent),
            chain_head: hex_encode(&head),
            ungoverned: false,
        })
    }

    /// What an agent polling on its escalation should be told.
    pub fn decision_status(&self, tenant: &str, decision_id: u64) -> Option<&'static str> {
        let key = (tenant.to_string(), decision_id);
        if self.pending.contains_key(&key) {
            return Some("pending");
        }
        if let Some(outcome) = self.resolved.get(&key) {
            return match outcome.as_str() {
                "approved" => Some("approved"),
                "rejected" => Some("rejected"),
                _ => Some("pending"),
            };
        }
        // A known decision that was never escalated (allowed, or ungoverned
        // outright) is not waiting on anyone — and was never approved.
        self.chains
            .get(tenant)?
            .records()
            .nth(decision_id as usize)
            .map(|_| "pending")
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
                time: clock_utc(r.timestamp_ms),
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

    pub fn issue_grant(&mut self, input: &GrantInput, tenant: &TenantId) -> Result<(), String> {
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
            .entry((tenant.as_str().to_string(), input.agent.clone()))
            .or_default()
            .push(grant);
        self.persist_grants(tenant.as_str())
    }

    /// Revoke a grant by id within a tenant (immediate). Returns whether a grant
    /// was found. A revocation that cannot be persisted is an error, not a
    /// quiet success — the whole point is that it survives.
    pub fn revoke_grant(
        &mut self,
        tenant: &str,
        agent: &str,
        grant_id: &str,
    ) -> Result<bool, String> {
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
        if found {
            self.persist_grants(tenant)?;
        }
        Ok(found)
    }

    /// Issue a vote allowance — the bounded, revocable franchise a principal
    /// delegates to an agent (agents never hold voting power natively, VA-3/VA-4).
    /// Tenant-keyed: an allowance id is meaningless outside its own tenant.
    pub fn issue_allowance(
        &mut self,
        input: &AllowanceInput,
        tenant: &TenantId,
    ) -> Result<(), String> {
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
        self.allowances
            .insert((tenant.as_str().to_string(), input.id.clone()), allowance);
        self.persist_allowances(tenant.as_str())
    }

    /// Cast a vote by spending against an allowance. Returns the delegation proof
    /// (`principal ▸ agent · spent/cap`) on success. Fails closed (VA-1/VA-2) if
    /// the allowance is unknown, dead, doesn't cover the class, or would overspend.
    pub fn cast_vote(
        &mut self,
        tenant: &TenantId,
        allowance_id: &str,
        class: &str,
        weight: u64,
        now_ms: i64,
    ) -> Result<String, String> {
        let allowance = self
            .allowances
            .get_mut(&(tenant.as_str().to_string(), allowance_id.to_string()))
            .ok_or_else(|| format!("no such allowance in this tenant: {allowance_id}"))?;
        let proof = allowance
            .cast(class, weight, now_ms)
            .map_err(|e| e.to_string())?;
        // The spend must outlive the process, or the cap is not a cap.
        self.persist_allowances(tenant.as_str())?;
        Ok(proof)
    }

    /// Revoke a vote allowance immediately (VA-2, "pull the plug"). Returns
    /// whether one was found. Like a grant revocation, it must be durable.
    pub fn revoke_allowance(
        &mut self,
        tenant: &TenantId,
        allowance_id: &str,
    ) -> Result<bool, String> {
        let found = match self
            .allowances
            .get_mut(&(tenant.as_str().to_string(), allowance_id.to_string()))
        {
            Some(a) => {
                a.revoke();
                true
            }
            None => false,
        };
        if found {
            self.persist_allowances(tenant.as_str())?;
        }
        Ok(found)
    }
}

// ---- Tauri commands (thin locks over QuorumBackend) -----------------

/// Shorthand for the managed-state handle Tauri hands each command. The lifetime
/// is elided per invocation (a `'static` alias breaks the command macro's borrow).
///
/// `Arc` because the agent bridge serves on its own threads and shares exactly
/// this state — an agent's intent and an operator's click land on one backend,
/// one evidence chain, one budget.
type Backend<'a> = State<'a, std::sync::Arc<Mutex<QuorumBackend>>>;

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

/// Establish the active tenant scope for this app instance. Until this succeeds
/// every command below fails closed.
#[tauri::command]
pub fn tenant_set(backend: Backend<'_>, tenant: String) -> Result<(), String> {
    lock(&backend)?.set_active_tenant(&tenant)
}

/// The active tenant scope, or `null` if none is established yet.
#[tauri::command]
pub fn tenant_active(backend: Backend<'_>) -> Result<Option<String>, String> {
    Ok(lock(&backend)?.active_tenant())
}

/// **The policy gate.** Evaluate a proposed action against the tenant's live
/// grants, record the verdict in the tenant's evidence chain, and return what
/// was recorded. Every governed action passes through here *before* it executes
/// or is signed (I-3/I-4) — including the ones that turn out to be ungoverned.
#[tauri::command]
pub fn action_evaluate_and_record(
    backend: Backend<'_>,
    input: ActionInput,
) -> Result<DecisionDto, String> {
    let mut b = lock(&backend)?;
    let tenant = b.require_tenant()?;
    b.evaluate_and_record(&input, &tenant, now_ms())
}

/// The escalations waiting on a human — the ceremony queue's contents.
#[tauri::command]
pub fn approvals_pending(backend: Backend<'_>) -> Result<Vec<PendingApproval>, String> {
    let b = lock(&backend)?;
    let tenant = b.require_tenant()?;
    Ok(b.pending_approvals(tenant.as_str()))
}

/// The human approved this escalated decision. Records who approved it; the
/// charge stays spent because the action now goes ahead.
#[tauri::command]
pub fn action_approve(
    backend: Backend<'_>,
    decision_id: u64,
    approver: String,
) -> Result<DecisionDto, String> {
    let mut b = lock(&backend)?;
    let tenant = b.require_tenant()?;
    b.approve_decision(&tenant, decision_id, &approver, now_ms())
}

/// The human refused this decision: refund what it charged and record the
/// refusal. Idempotent on the refund — a second call cannot pay out twice.
#[tauri::command]
pub fn action_reject(backend: Backend<'_>, decision_id: u64) -> Result<DecisionDto, String> {
    let mut b = lock(&backend)?;
    let tenant = b.require_tenant()?;
    b.reject_decision(&tenant, decision_id, now_ms())
}

#[tauri::command]
pub fn ledger_records(backend: Backend<'_>) -> Result<Vec<LedgerRow>, String> {
    let b = lock(&backend)?;
    let tenant = b.require_tenant()?;
    Ok(b.ledger_rows(tenant.as_str()))
}

#[tauri::command]
pub fn ledger_head(backend: Backend<'_>) -> Result<String, String> {
    let b = lock(&backend)?;
    let tenant = b.require_tenant()?;
    Ok(b.ledger_head(tenant.as_str()))
}

#[tauri::command]
pub fn ledger_merkle_root(backend: Backend<'_>) -> Result<String, String> {
    let b = lock(&backend)?;
    let tenant = b.require_tenant()?;
    Ok(b.ledger_merkle_root(tenant.as_str()))
}

#[tauri::command]
pub fn ledger_verify(backend: Backend<'_>) -> Result<bool, String> {
    let b = lock(&backend)?;
    let tenant = b.require_tenant()?;
    Ok(b.ledger_verify(tenant.as_str()))
}

/// The count of `ungoverned` actions in a tenant's chain — the honest headline
/// number the Ledger surfaces, never a hidden gap (Rule 5).
#[tauri::command]
pub fn ledger_ungoverned_count(backend: Backend<'_>) -> Result<usize, String> {
    let b = lock(&backend)?;
    let tenant = b.require_tenant()?;
    Ok(b.ledger_ungoverned_count(tenant.as_str()))
}

#[tauri::command]
pub fn grant_issue(backend: Backend<'_>, input: GrantInput) -> Result<(), String> {
    let mut b = lock(&backend)?;
    let tenant = b.require_tenant()?;
    b.issue_grant(&input, &tenant)
}

#[tauri::command]
pub fn grant_revoke(backend: Backend<'_>, agent: String, grant_id: String) -> Result<bool, String> {
    let mut b = lock(&backend)?;
    let tenant = b.require_tenant()?;
    b.revoke_grant(tenant.as_str(), &agent, &grant_id)
}

#[tauri::command]
pub fn allowance_issue(backend: Backend<'_>, input: AllowanceInput) -> Result<(), String> {
    let mut b = lock(&backend)?;
    let tenant = b.require_tenant()?;
    b.issue_allowance(&input, &tenant)
}

#[tauri::command]
pub fn allowance_revoke(backend: Backend<'_>, allowance_id: String) -> Result<bool, String> {
    let mut b = lock(&backend)?;
    let tenant = b.require_tenant()?;
    b.revoke_allowance(&tenant, &allowance_id)
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
    let mut b = lock(&backend)?;
    let tenant = b.require_tenant()?;
    b.cast_vote(&tenant, &allowance_id, &proposal_class, weight, now_ms())
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

    /// A tenant for the pure fns. Valid by construction — `TenantId::new` is
    /// exercised on its own in `set_active_tenant_rejects_a_malformed_tenant`.
    fn t(id: &str) -> TenantId {
        TenantId::new(id).expect("test tenant id is well-formed")
    }

    fn action(agent: &str, class: &str, classification: &str, cost: u64) -> ActionInput {
        ActionInput {
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
            .evaluate_and_record(&action("sbt-9", "repo.write", "Public", 1), &t("bca"), 1000)
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
        b.issue_grant(
            &grant("sbt-41", &["repo.write"], "CUI", 100, "2"),
            &t("bca"),
        )
        .unwrap();
        let head0 = b.ledger_head("bca");
        let d = b
            .evaluate_and_record(
                &action("sbt-41", "repo.write", "Proprietary", 5),
                &t("bca"),
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
        // Grant is issued into tenant "bca".
        b.issue_grant(
            &grant("sbt-41", &["repo.write"], "CUI", 100, "2"),
            &t("bca"),
        )
        .unwrap();
        // Same agent, same action, DIFFERENT tenant → no covering grant.
        let d = b
            .evaluate_and_record(
                &action("sbt-41", "repo.write", "Public", 1),
                &t("sea"),
                1000,
            )
            .unwrap();
        assert_eq!(
            d.verdict, "ungoverned",
            "grants must not cross tenant boundaries"
        );
        // And it IS governed in its own tenant.
        let d2 = b
            .evaluate_and_record(
                &action("sbt-41", "repo.write", "Public", 1),
                &t("bca"),
                1000,
            )
            .unwrap();
        assert_eq!(d2.verdict, "allow");
    }

    #[test]
    fn over_ceiling_records_require_approval() {
        let mut b = QuorumBackend::default();
        b.issue_grant(&grant("sbt-41", &["spend"], "CUI", 1000, "2"), &t("bca"))
            .unwrap();
        let mut a = action("sbt-41", "spend", "Public", 220);
        a.hic1_cost_threshold = 150;
        let d = b.evaluate_and_record(&a, &t("bca"), 1000).unwrap();
        assert_eq!(d.verdict, "require-approval");
        assert_eq!(d.hic, "1");
    }

    #[test]
    fn revoked_grant_makes_next_action_ungoverned() {
        let mut b = QuorumBackend::default();
        b.issue_grant(
            &grant("sbt-41", &["repo.write"], "CUI", 100, "2"),
            &t("bca"),
        )
        .unwrap();
        assert!(b.revoke_grant("bca", "sbt-41", "G-1").unwrap());
        let d = b
            .evaluate_and_record(
                &action("sbt-41", "repo.write", "Public", 1),
                &t("bca"),
                1000,
            )
            .unwrap();
        assert_eq!(d.verdict, "ungoverned");
    }

    #[test]
    fn tampering_a_recorded_chain_is_detectable_via_verify() {
        let mut b = QuorumBackend::default();
        b.issue_grant(
            &grant("sbt-41", &["repo.write"], "CUI", 100, "2"),
            &t("bca"),
        )
        .unwrap();
        b.evaluate_and_record(
            &action("sbt-41", "repo.write", "Public", 1),
            &t("bca"),
            1000,
        )
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

    fn allowance(id: &str) -> AllowanceInput {
        AllowanceInput {
            id: id.into(),
            principal: "R. Ortiz".into(),
            agent: "claude-code".into(),
            tenant_scope: "t3:bca".into(),
            proposal_classes: vec!["standup".into()],
            weight_cap: 5,
            expires_at_ms: i64::MAX,
        }
    }

    #[test]
    fn vote_allowance_casts_within_cap_then_fails_closed() {
        let mut b = QuorumBackend::default();
        b.issue_allowance(&allowance("VA-1"), &t("bca")).unwrap();
        let proof = b.cast_vote(&t("bca"), "VA-1", "standup", 3, 1000).unwrap();
        assert!(proof.contains("R. Ortiz ▸ claude-code"));
        assert!(
            b.cast_vote(&t("bca"), "VA-1", "standup", 3, 1000).is_err(),
            "over cap fails closed"
        );
        assert!(
            b.cast_vote(&t("bca"), "VA-1", "treasury", 1, 1000).is_err(),
            "uncovered class fails closed"
        );
        assert!(
            b.cast_vote(&t("bca"), "VA-nope", "standup", 1, 1000)
                .is_err(),
            "unknown allowance fails closed"
        );
        assert!(b.revoke_allowance(&t("bca"), "VA-1").unwrap());
        assert!(
            b.cast_vote(&t("bca"), "VA-1", "standup", 1, 1000).is_err(),
            "revoked allowance fails closed"
        );
    }

    #[test]
    fn an_allowance_in_one_tenant_cannot_be_spent_from_another() {
        let mut b = QuorumBackend::default();
        b.issue_allowance(&allowance("VA-1"), &t("bca")).unwrap();
        assert!(
            b.cast_vote(&t("sea"), "VA-1", "standup", 1, 1000).is_err(),
            "an allowance id must not be spendable outside its own tenant"
        );
        assert!(
            !b.revoke_allowance(&t("sea"), "VA-1").unwrap(),
            "nor revocable from another tenant"
        );
        // Still intact and spendable where it actually lives.
        assert!(b.cast_vote(&t("bca"), "VA-1", "standup", 1, 1000).is_ok());
    }

    #[test]
    fn no_active_tenant_fails_closed() {
        let b = QuorumBackend::default();
        assert!(b.active_tenant().is_none());
        let err = b
            .require_tenant()
            .expect_err("must fail closed with no scope");
        assert!(err.contains("no active tenant"), "honest error: {err}");
    }

    #[test]
    fn set_active_tenant_rejects_a_malformed_tenant() {
        let mut b = QuorumBackend::default();
        assert!(b.set_active_tenant("").is_err(), "empty tenant is rejected");
        assert!(
            b.active_tenant().is_none(),
            "a rejected tenant must not become the active scope"
        );
        b.set_active_tenant("bca").unwrap();
        assert_eq!(b.active_tenant().as_deref(), Some("bca"));
        assert_eq!(b.require_tenant().unwrap().as_str(), "bca");
    }

    // ---- the budget actually depletes --------------------------------

    #[test]
    fn every_governed_action_costs_at_least_one_unit() {
        let mut b = QuorumBackend::default();
        // Budget of 3, and repo.write declares no cost at all.
        b.issue_grant(&grant("sbt-41", &["repo.write"], "CUI", 3, "2"), &t("bca"))
            .unwrap();
        for i in 0..3 {
            let d = b
                .evaluate_and_record(
                    &action("sbt-41", "repo.write", "Public", 0),
                    &t("bca"),
                    1000,
                )
                .unwrap();
            assert_eq!(
                d.verdict, "allow",
                "action {i} should be inside the envelope"
            );
        }
        // Fourth: the envelope is spent, so there is no longer a covering grant.
        let d = b
            .evaluate_and_record(
                &action("sbt-41", "repo.write", "Public", 0),
                &t("bca"),
                1000,
            )
            .unwrap();
        assert_eq!(
            d.verdict, "ungoverned",
            "a budget that never depletes is not a budget"
        );
    }

    #[test]
    fn a_spend_charges_its_magnitude() {
        let mut b = QuorumBackend::default();
        b.issue_grant(&grant("sbt-41", &["spend"], "CUI", 500, "2"), &t("bca"))
            .unwrap();
        b.evaluate_and_record(&action("sbt-41", "spend", "Public", 300), &t("bca"), 1000)
            .unwrap();
        // 300 spent of 500; a second 300 no longer fits.
        let d = b
            .evaluate_and_record(&action("sbt-41", "spend", "Public", 300), &t("bca"), 1000)
            .unwrap();
        assert_eq!(
            d.verdict, "ungoverned",
            "the remaining 200 cannot cover 300"
        );
        // But 200 exactly still fits.
        let d = b
            .evaluate_and_record(&action("sbt-41", "spend", "Public", 200), &t("bca"), 1000)
            .unwrap();
        assert_eq!(d.verdict, "allow");
    }

    #[test]
    fn an_escalation_to_a_human_also_spends_the_envelope() {
        let mut b = QuorumBackend::default();
        b.issue_grant(&grant("sbt-41", &["spend"], "CUI", 400, "2"), &t("bca"))
            .unwrap();
        let mut a = action("sbt-41", "spend", "Public", 220);
        a.hic1_cost_threshold = 150;
        let d = b.evaluate_and_record(&a, &t("bca"), 1000).unwrap();
        assert_eq!(d.verdict, "require-approval");
        // 220 of 400 is now committed, so a second 220 escalation cannot fit.
        let d2 = b.evaluate_and_record(&a, &t("bca"), 1000).unwrap();
        assert_eq!(
            d2.verdict, "ungoverned",
            "escalations must consume, or an agent can queue unlimited approvals"
        );
    }

    #[test]
    fn a_rejection_refunds_exactly_once_and_is_recorded() {
        let mut b = QuorumBackend::default();
        b.issue_grant(&grant("sbt-41", &["spend"], "CUI", 400, "2"), &t("bca"))
            .unwrap();
        let mut a = action("sbt-41", "spend", "Public", 220);
        a.hic1_cost_threshold = 150;
        let d = b.evaluate_and_record(&a, &t("bca"), 1000).unwrap();

        let rejection = b.reject_decision(&t("bca"), d.decision_id, 2000).unwrap();
        assert_eq!(rejection.verdict, "rejected");
        assert_eq!(rejection.hic, "1");
        assert_eq!(b.ledger_rows("bca").len(), 2, "the refusal is evidence too");

        // Refunded: the same 220 escalation fits again.
        let again = b.evaluate_and_record(&a, &t("bca"), 3000).unwrap();
        assert_eq!(again.verdict, "require-approval", "the refund gave it back");

        // Rejecting the SAME decision again must not pay out a second time.
        b.reject_decision(&t("bca"), d.decision_id, 4000).unwrap();
        b.reject_decision(&t("bca"), d.decision_id, 5000).unwrap();
        let third = b.evaluate_and_record(&a, &t("bca"), 6000).unwrap();
        assert_eq!(
            third.verdict, "ungoverned",
            "a repeated rejection must not manufacture budget"
        );
    }

    // ---- the escalation reaches a human ------------------------------

    fn escalating_action() -> ActionInput {
        let mut a = action("sbt-41", "spend", "Public", 220);
        a.hic1_cost_threshold = 150;
        a
    }

    #[test]
    fn an_escalation_lands_in_the_ceremony_queue() {
        let mut b = QuorumBackend::default();
        assert!(b.pending_approvals("bca").is_empty());
        b.issue_grant(&grant("sbt-41", &["spend"], "CUI", 1000, "2"), &t("bca"))
            .unwrap();
        let d = b
            .evaluate_and_record(&escalating_action(), &t("bca"), 1000)
            .unwrap();
        assert_eq!(d.verdict, "require-approval");

        let queue = b.pending_approvals("bca");
        assert_eq!(
            queue.len(),
            1,
            "an agent stopped and asked — a human must see it"
        );
        assert_eq!(queue[0].decision, d.decision_id);
        assert_eq!(queue[0].agent, "sbt-41");
        assert_eq!(queue[0].cost, 220);
        // An allowed action is not a question, so it does not queue.
        b.evaluate_and_record(&action("sbt-41", "spend", "Public", 5), &t("bca"), 1100)
            .unwrap();
        assert_eq!(b.pending_approvals("bca").len(), 1);
    }

    #[test]
    fn approving_records_who_approved_and_clears_the_queue() {
        let mut b = QuorumBackend::default();
        b.issue_grant(&grant("sbt-41", &["spend"], "CUI", 1000, "2"), &t("bca"))
            .unwrap();
        let d = b
            .evaluate_and_record(&escalating_action(), &t("bca"), 1000)
            .unwrap();

        let approval = b
            .approve_decision(&t("bca"), d.decision_id, "R. Ortiz", 2000)
            .unwrap();
        assert_eq!(approval.verdict, "approved");
        assert_eq!(approval.hic, "1");
        assert!(approval.reason.contains("R. Ortiz"));
        assert!(
            b.pending_approvals("bca").is_empty(),
            "the question is answered"
        );
        assert_eq!(b.ledger_rows("bca").len(), 2);
        assert!(b.ledger_verify("bca"));
        // The approver is named in the record — that IS the evidence.
        assert_eq!(b.ledger_rows("bca")[1].principal, "R. Ortiz");
    }

    #[test]
    fn an_approved_action_keeps_its_charge_but_a_rejected_one_is_refunded() {
        let mut b = QuorumBackend::default();
        b.issue_grant(&grant("sbt-41", &["spend"], "CUI", 400, "2"), &t("bca"))
            .unwrap();
        let d = b
            .evaluate_and_record(&escalating_action(), &t("bca"), 1000)
            .unwrap();
        b.approve_decision(&t("bca"), d.decision_id, "R. Ortiz", 2000)
            .unwrap();
        // 220 of 400 stays spent, so a second 220 cannot fit.
        assert_eq!(
            b.evaluate_and_record(&escalating_action(), &t("bca"), 3000)
                .unwrap()
                .verdict,
            "ungoverned",
            "an approved action goes ahead, so it keeps consuming its envelope"
        );

        // The same flow, rejected, gives the budget back.
        let mut b2 = QuorumBackend::default();
        b2.issue_grant(&grant("sbt-41", &["spend"], "CUI", 400, "2"), &t("bca"))
            .unwrap();
        let d2 = b2
            .evaluate_and_record(&escalating_action(), &t("bca"), 1000)
            .unwrap();
        b2.reject_decision(&t("bca"), d2.decision_id, 2000).unwrap();
        assert_eq!(
            b2.evaluate_and_record(&escalating_action(), &t("bca"), 3000)
                .unwrap()
                .verdict,
            "require-approval"
        );
    }

    #[test]
    fn a_rejection_also_clears_the_queue() {
        let mut b = QuorumBackend::default();
        b.issue_grant(&grant("sbt-41", &["spend"], "CUI", 1000, "2"), &t("bca"))
            .unwrap();
        let d = b
            .evaluate_and_record(&escalating_action(), &t("bca"), 1000)
            .unwrap();
        b.reject_decision(&t("bca"), d.decision_id, 2000).unwrap();
        assert!(b.pending_approvals("bca").is_empty());
    }

    #[test]
    fn approval_cannot_mint_authority_for_something_never_escalated() {
        let mut b = QuorumBackend::default();
        b.issue_grant(&grant("sbt-41", &["spend"], "CUI", 1000, "2"), &t("bca"))
            .unwrap();
        // An ALLOWED action was never a question — approving it is meaningless.
        let allowed = b
            .evaluate_and_record(&action("sbt-41", "spend", "Public", 5), &t("bca"), 1000)
            .unwrap();
        assert!(b
            .approve_decision(&t("bca"), allowed.decision_id, "R. Ortiz", 2000)
            .is_err());
        // Nor can a decision be approved twice.
        let d = b
            .evaluate_and_record(&escalating_action(), &t("bca"), 1100)
            .unwrap();
        assert!(b
            .approve_decision(&t("bca"), d.decision_id, "R. Ortiz", 2000)
            .is_ok());
        assert!(
            b.approve_decision(&t("bca"), d.decision_id, "R. Ortiz", 3000)
                .is_err(),
            "an answered question cannot be answered again"
        );
    }

    #[test]
    fn an_approval_must_name_the_human_who_gave_it() {
        let mut b = QuorumBackend::default();
        b.issue_grant(&grant("sbt-41", &["spend"], "CUI", 1000, "2"), &t("bca"))
            .unwrap();
        let d = b
            .evaluate_and_record(&escalating_action(), &t("bca"), 1000)
            .unwrap();
        let err = b
            .approve_decision(&t("bca"), d.decision_id, "   ", 2000)
            .expect_err("an anonymous approval is not evidence");
        assert!(err.contains("name the human"), "honest error: {err}");
        assert_eq!(
            b.pending_approvals("bca").len(),
            1,
            "a refused approval must leave the escalation waiting, not drop it"
        );
    }

    /// RED: two escalations from the same agent for the same tool share an
    /// action class and correlation id. Answering them differently must not
    /// make one report the other's outcome — an agent told "approved" when a
    /// human said no would act on a refusal.
    #[test]
    fn two_escalations_that_look_alike_do_not_borrow_each_others_answers() {
        let mut b = QuorumBackend::default();
        b.issue_grant(&grant("sbt-41", &["spend"], "CUI", 10_000, "2"), &t("bca"))
            .unwrap();
        let first = b
            .evaluate_and_record(&escalating_action(), &t("bca"), 1000)
            .unwrap();
        let second = b
            .evaluate_and_record(&escalating_action(), &t("bca"), 1100)
            .unwrap();
        assert_ne!(first.decision_id, second.decision_id);

        b.approve_decision(&t("bca"), first.decision_id, "R. Ortiz", 2000)
            .unwrap();
        b.reject_decision(&t("bca"), second.decision_id, 2100)
            .unwrap();

        assert_eq!(
            b.decision_status("bca", first.decision_id),
            Some("approved")
        );
        assert_eq!(
            b.decision_status("bca", second.decision_id),
            Some("rejected"),
            "the human REFUSED this one — telling its agent 'approved' would be \
             acting on a refusal"
        );
    }

    #[test]
    fn a_waiting_agent_can_learn_what_the_human_decided() {
        let mut b = QuorumBackend::default();
        b.issue_grant(&grant("sbt-41", &["spend"], "CUI", 1000, "2"), &t("bca"))
            .unwrap();
        let d = b
            .evaluate_and_record(&escalating_action(), &t("bca"), 1000)
            .unwrap();
        assert_eq!(b.decision_status("bca", d.decision_id), Some("pending"));
        b.approve_decision(&t("bca"), d.decision_id, "R. Ortiz", 2000)
            .unwrap();
        assert_eq!(b.decision_status("bca", d.decision_id), Some("approved"));

        let mut b2 = QuorumBackend::default();
        b2.issue_grant(&grant("sbt-41", &["spend"], "CUI", 1000, "2"), &t("bca"))
            .unwrap();
        let d2 = b2
            .evaluate_and_record(&escalating_action(), &t("bca"), 1000)
            .unwrap();
        b2.reject_decision(&t("bca"), d2.decision_id, 2000).unwrap();
        assert_eq!(b2.decision_status("bca", d2.decision_id), Some("rejected"));
        assert_eq!(
            b2.decision_status("bca", 999),
            None,
            "unknown decision is unknown"
        );
    }

    #[test]
    fn rejecting_an_ungoverned_decision_records_it_and_refunds_nothing() {
        let mut b = QuorumBackend::default();
        let d = b
            .evaluate_and_record(&action("sbt-9", "repo.write", "Public", 1), &t("bca"), 1000)
            .unwrap();
        assert!(d.ungoverned);
        let r = b.reject_decision(&t("bca"), d.decision_id, 2000).unwrap();
        assert_eq!(r.verdict, "rejected");
        assert_eq!(b.ledger_rows("bca").len(), 2);
        assert!(b.ledger_verify("bca"));
    }

    #[test]
    fn rejecting_a_decision_that_does_not_exist_fails_closed() {
        let mut b = QuorumBackend::default();
        let err = b
            .reject_decision(&t("bca"), 41, 1000)
            .expect_err("there is no decision 41");
        assert!(err.contains("no decision 41"), "honest error: {err}");
    }

    #[test]
    fn a_governed_action_without_a_principal_is_refused_not_recorded() {
        let mut b = QuorumBackend::default();
        b.issue_grant(
            &grant("sbt-41", &["repo.write"], "CUI", 100, "2"),
            &t("bca"),
        )
        .unwrap();
        let mut a = action("sbt-41", "repo.write", "Public", 1);
        a.principal = None;
        let err = b
            .evaluate_and_record(&a, &t("bca"), 1000)
            .expect_err("a governed record must name its authority (I-4)");
        assert!(err.contains("accountable principal"), "honest error: {err}");
        assert!(
            b.ledger_rows("bca").is_empty(),
            "nothing half-recorded on the way out"
        );
    }

    // ---- durability: what survives a restart -------------------------
    //
    // These drive the backend the way the commands do, drop it, and build a
    // fresh one over the same store — the closest thing to "quit and reopen
    // the app" that a unit test can be.

    struct TempRoot(std::path::PathBuf);
    impl TempRoot {
        fn new(tag: &str) -> Self {
            let p = std::env::temp_dir()
                .join(format!("quorum-backend-test-{}-{tag}", std::process::id()));
            let _ = std::fs::remove_dir_all(&p);
            Self(p)
        }
        /// A backend over this root, as if the app had just started.
        fn boot(&self) -> QuorumBackend {
            QuorumBackend::with_store(EvidenceStore::open(self.0.clone()).unwrap())
        }
    }
    impl Drop for TempRoot {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn evidence_and_scope_survive_a_restart() {
        let root = TempRoot::new("restart");
        let head_before;
        {
            let mut b = root.boot();
            b.set_active_tenant("bca").unwrap();
            b.issue_grant(
                &grant("sbt-41", &["repo.write"], "CUI", 100, "2"),
                &t("bca"),
            )
            .unwrap();
            b.evaluate_and_record(
                &action("sbt-41", "repo.write", "Public", 1),
                &t("bca"),
                1000,
            )
            .unwrap();
            // An ungoverned one too — the gap must survive as faithfully as the rest.
            b.evaluate_and_record(&action("sbt-9", "repo.write", "Public", 1), &t("bca"), 2000)
                .unwrap();
            head_before = b.ledger_head("bca");
        }

        let b = root.boot();
        assert_eq!(
            b.active_tenant().as_deref(),
            Some("bca"),
            "the scope resumes without re-onboarding"
        );
        assert_eq!(b.ledger_rows("bca").len(), 2, "both records survive");
        assert_eq!(b.ledger_head("bca"), head_before, "the head is identical");
        assert_eq!(b.ledger_ungoverned_count("bca"), 1, "the gap survives too");
        assert!(b.ledger_verify("bca"));
    }

    #[test]
    fn a_grant_still_governs_after_a_restart_and_a_revoked_one_still_does_not() {
        let root = TempRoot::new("grants");
        {
            let mut b = root.boot();
            b.set_active_tenant("bca").unwrap();
            b.issue_grant(
                &grant("sbt-41", &["repo.write"], "CUI", 100, "2"),
                &t("bca"),
            )
            .unwrap();
        }
        {
            let mut b = root.boot();
            let d = b
                .evaluate_and_record(
                    &action("sbt-41", "repo.write", "Public", 1),
                    &t("bca"),
                    1000,
                )
                .unwrap();
            assert_eq!(d.verdict, "allow", "a persisted grant still authorizes");
            assert!(b.revoke_grant("bca", "sbt-41", "G-1").unwrap());
        }
        let mut b = root.boot();
        let d = b
            .evaluate_and_record(
                &action("sbt-41", "repo.write", "Public", 1),
                &t("bca"),
                2000,
            )
            .unwrap();
        assert_eq!(
            d.verdict, "ungoverned",
            "a revocation must not be undone by a restart"
        );
    }

    #[test]
    fn a_spent_vote_allowance_does_not_refill_on_restart() {
        let root = TempRoot::new("allowance");
        {
            let mut b = root.boot();
            b.set_active_tenant("bca").unwrap();
            b.issue_allowance(&allowance("VA-1"), &t("bca")).unwrap();
            b.cast_vote(&t("bca"), "VA-1", "standup", 5, 1000).unwrap();
        }
        let mut b = root.boot();
        assert!(
            b.cast_vote(&t("bca"), "VA-1", "standup", 1, 2000).is_err(),
            "the cap is not a cap if restarting refills it"
        );
    }

    #[test]
    fn a_charge_can_still_be_refunded_after_a_restart() {
        let root = TempRoot::new("charges");
        let decision_id;
        {
            let mut b = root.boot();
            b.set_active_tenant("bca").unwrap();
            b.issue_grant(&grant("sbt-41", &["spend"], "CUI", 400, "2"), &t("bca"))
                .unwrap();
            let mut a = action("sbt-41", "spend", "Public", 220);
            a.hic1_cost_threshold = 150;
            decision_id = b
                .evaluate_and_record(&a, &t("bca"), 1000)
                .unwrap()
                .decision_id;
        }
        // The human comes back tomorrow and rejects it.
        let mut b = root.boot();
        b.reject_decision(&t("bca"), decision_id, 2000).unwrap();
        let mut a = action("sbt-41", "spend", "Public", 220);
        a.hic1_cost_threshold = 150;
        assert_eq!(
            b.evaluate_and_record(&a, &t("bca"), 3000).unwrap().verdict,
            "require-approval",
            "the refund must know what yesterday's decision charged"
        );
    }

    #[test]
    fn an_agent_still_waiting_is_still_waiting_after_a_restart() {
        let root = TempRoot::new("pending");
        let decision_id;
        {
            let mut b = root.boot();
            b.set_active_tenant("bca").unwrap();
            b.issue_grant(&grant("sbt-41", &["spend"], "CUI", 1000, "2"), &t("bca"))
                .unwrap();
            let mut a = action("sbt-41", "spend", "Public", 220);
            a.hic1_cost_threshold = 150;
            decision_id = b
                .evaluate_and_record(&a, &t("bca"), 1000)
                .unwrap()
                .decision_id;
            assert_eq!(b.pending_approvals("bca").len(), 1);
        }
        let mut b = root.boot();
        let queue = b.pending_approvals("bca");
        assert_eq!(
            queue.len(),
            1,
            "an escalation must not evaporate across a restart — the agent is still blocked"
        );
        assert_eq!(queue[0].decision, decision_id);
        // And it can still be answered.
        assert!(b
            .approve_decision(&t("bca"), decision_id, "R. Ortiz", 5000)
            .is_ok());
        assert!(b.pending_approvals("bca").is_empty());
    }

    #[test]
    fn the_answer_a_human_gave_survives_a_restart() {
        let root = TempRoot::new("resolved");
        let (approved, rejected);
        {
            let mut b = root.boot();
            b.set_active_tenant("bca").unwrap();
            b.issue_grant(&grant("sbt-41", &["spend"], "CUI", 10_000, "2"), &t("bca"))
                .unwrap();
            let mut a = action("sbt-41", "spend", "Public", 220);
            a.hic1_cost_threshold = 150;
            approved = b
                .evaluate_and_record(&a, &t("bca"), 1000)
                .unwrap()
                .decision_id;
            rejected = b
                .evaluate_and_record(&a, &t("bca"), 1100)
                .unwrap()
                .decision_id;
            b.approve_decision(&t("bca"), approved, "R. Ortiz", 2000)
                .unwrap();
            b.reject_decision(&t("bca"), rejected, 2100).unwrap();
        }
        // A blocked agent polls again tomorrow. It must get the same answers.
        let b = root.boot();
        assert_eq!(b.decision_status("bca", approved), Some("approved"));
        assert_eq!(
            b.decision_status("bca", rejected),
            Some("rejected"),
            "a refusal must not decay into anything an agent could act on"
        );
    }

    #[test]
    fn a_corrupt_chain_refuses_the_scope_instead_of_serving_it() {
        let root = TempRoot::new("corrupt");
        {
            let mut b = root.boot();
            b.set_active_tenant("bca").unwrap();
            b.evaluate_and_record(&action("sbt-9", "repo.write", "Public", 1), &t("bca"), 1000)
                .unwrap();
        }
        // Tamper with the evidence on disk.
        let store = EvidenceStore::open(root.0.clone()).unwrap();
        let path = store.tenant_dir("bca").join("chain.jsonl");
        let raw = std::fs::read_to_string(&path).unwrap();
        std::fs::write(&path, raw.replace("repo.write", "repo.admin")).unwrap();

        let mut b = root.boot();
        assert!(
            b.active_tenant().is_none(),
            "a tenant whose evidence does not verify must not become the active scope"
        );
        let err = b
            .set_active_tenant("bca")
            .expect_err("naming it again must surface the reason");
        assert!(err.contains("failed to verify"), "honest error: {err}");
        // And with no scope, nothing is served.
        assert!(b.require_tenant().is_err());
    }

    #[test]
    fn the_ledger_clock_is_readable_and_utc() {
        assert_eq!(clock_utc(0), "00:00:00");
        // The record the packaged app wrote during the QRM-S4 bridge run. The
        // file listing showed 18:44 LOCAL; the ledger is UTC, and that gap is
        // the point of fixing the column to UTC rather than local time.
        assert_eq!(clock_utc(1_784_943_915_399), "01:45:15");
        // Negative (pre-epoch) timestamps must not panic or go nonsensical.
        assert_eq!(clock_utc(-1).len(), 8);
    }

    #[test]
    fn hex_roundtrip() {
        assert_eq!(hex_encode(&[0xab, 0x01]), "0xab01");
        assert_eq!(hex32("0xdeadbeef")[0..4], [0xde, 0xad, 0xbe, 0xef]);
    }
}
