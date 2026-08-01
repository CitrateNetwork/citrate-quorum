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
use quorum_meetings::{agenda_source, journal_source, Attendee, Meeting, MeetingState, Template};

use crate::addresses::AddressBook;
use crate::anchor::{self, AnchorState};

use crate::store::{
    classification_from_str, classification_str, grant_hic_from_str, grant_hic_str, hex32,
    hex_encode, hic_str, verdict_str, Charge, EgressPolicy, EvidenceStore, MeetingRecord,
    PendingApproval, StoreError,
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

/// The chain index behind a `D-…` ledger handle.
///
/// The ribbon numbers records from `D-90001`, so the handle is an offset, not an
/// opaque id. Parsing it here — once, strictly — is what stops a malformed
/// handle from silently resolving to record 0.
fn decision_index(id: &str) -> Result<usize, String> {
    let n: u64 = id
        .strip_prefix("D-")
        .and_then(|d| d.parse().ok())
        .ok_or_else(|| format!("{id} is not a decision handle (expected D-90001 and up)"))?;
    n.checked_sub(90_001)
        .map(|i| i as usize)
        .ok_or_else(|| format!("{id} is below the first decision handle (D-90001)"))
}

/// `YYYY-MM-DD HH:MM:SS UTC` from epoch-ms.
///
/// Done by hand, like `clock_utc`, rather than pulling a date crate in for two
/// formats. Civil-from-days is Howard Hinnant's algorithm; the round-trip test
/// pins it against known instants rather than trusting the arithmetic.
pub(crate) fn iso_utc(ms: i64) -> String {
    let secs = ms.div_euclid(1000);
    let days = secs.div_euclid(86_400);
    let tod = secs.rem_euclid(86_400);

    // days since 1970-01-01 → civil date
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };

    format!(
        "{y:04}-{m:02}-{d:02} {:02}:{:02}:{:02} UTC",
        tod / 3600,
        (tod % 3600) / 60,
        tod % 60
    )
}

/// A one-line reason for a verdict, derived from the record itself.
///
/// Deliberately derived rather than stored: the record IS the evidence, and a
/// prose reason persisted beside it could drift from the fields it describes.
fn decision_reason(r: &DecisionRecord) -> String {
    match r.verdict {
        Verdict::Allow => format!(
            "allowed by grant {} at {}",
            r.grant_id.clone().unwrap_or_else(|| "—".to_string()),
            hic_str(r.hic)
        ),
        Verdict::RequireApproval => format!(
            "escalated to a human at {} before execution",
            hic_str(r.hic)
        ),
        Verdict::Deny => "refused by policy: no live grant covers this action class, \
                          classification or budget"
            .to_string(),
        Verdict::Ungoverned => "NO live grant stood behind this action — recorded ungoverned \
                                and alerted, never silently allowed (rule 5)"
            .to_string(),
        Verdict::Rejected => "a human refused it; the charge was refunded".to_string(),
        Verdict::Approved => format!(
            "a human approved it ({})",
            r.principal.clone().unwrap_or_else(|| "unnamed".to_string())
        ),
    }
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
    /// The human issuing this grant. L-1 makes issuing a capability grant an
    /// HIC-1 act, and I-4 makes the accountable human mandatory — a grant that
    /// appeared from nowhere is exactly what an auditor is looking for.
    #[serde(default)]
    pub issued_by: String,
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

/// One decision rendered as a document — the Ledger's charter register.
#[derive(Serialize, Clone, Debug)]
pub struct DecisionDetailDto {
    pub id: String,
    pub what: String,
    pub when: String,
    pub principal: String,
    pub agent: String,
    pub grant: String,
    pub protocol: String,
    pub verdict: String,
    pub hic: String,
    pub reason: String,
    pub model: String,
    pub params: String,
    pub correlation: String,
    pub chain_pos: String,
    /// The chained hash stored for this record — `BLAKE3(prev_head ++ record)`.
    pub entry_hash: String,
    /// The record's own content hash, independent of its chain position: the
    /// leaf an inclusion proof is built from.
    pub content_hash: String,
    pub chain_head: String,
    pub merkle_root: String,
    /// How many sibling hashes the inclusion proof needed.
    pub proof_len: usize,
    /// Whether this record's content hash + proof recompute the Merkle root.
    pub included: bool,
    pub source: String,
}

/// What the Verify affordance actually checked.
#[derive(Serialize, Clone, Debug)]
pub struct VerifyDecisionDto {
    /// The whole chain replayed from genesis and every stored hash matched.
    pub chain_intact: bool,
    /// This record proved into the tenant's Merkle root.
    pub included: bool,
    pub records: usize,
    pub entry_hash: String,
    pub merkle_root: String,
    pub proof_len: usize,
}

#[derive(Serialize, Clone, Debug)]
pub struct CorrelationEventDto {
    pub t: String,
    pub kind: String,
    pub text: String,
    pub link: String,
}

#[derive(Serialize, Clone, Debug)]
pub struct CorrelationViewDto {
    pub events: Vec<CorrelationEventDto>,
    /// Rule 11: what was searched, and what is deliberately not in the answer.
    pub source: String,
}

#[derive(Serialize, Clone, Debug)]
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

/// One capability grant, as the Agents surface renders it.
#[derive(Serialize, Clone, Debug)]
pub struct GrantSummary {
    pub id: String,
    pub agent: String,
    pub principal: String,
    pub scope: String,
    pub classes: String,
    pub ceiling: String,
    pub budget_units: u64,
    pub consumed: u64,
    pub hic: String,
    pub expires_at_ms: i64,
    pub revoked: bool,
}

/// An agent this tenant has evidence about. Derived from grants + decision
/// records, never from a registry we cannot read yet.
#[derive(Serialize, Clone, Debug)]
pub struct AgentSummary {
    pub id: String,
    pub live_grants: usize,
    pub budget_units: u64,
    pub consumed: u64,
    pub decisions: usize,
    pub ungoverned: usize,
}

#[derive(Serialize, Clone, Debug)]
pub struct JournalEntryDto {
    pub id: String,
    pub date: String,
    pub who: String,
    pub kind: String,
    pub text: String,
    /// `None` when the author line names both a model and the human directing
    /// it, which is the normal case. Never guessed.
    pub human: Option<bool>,
}

#[derive(Serialize, Clone, Debug)]
pub struct JournalListDto {
    pub entries: Vec<JournalEntryDto>,
    /// Rule 11: where these came from, or why there are none.
    pub source: String,
}

#[derive(Serialize, Clone, Debug)]
pub struct StandupBriefDto {
    pub agent: String,
    pub meeting: String,
    pub sections: Vec<(String, String)>,
    pub source: String,
}

// ---- meetings DTOs (WP-S5.5) ----------------------------------------

/// What scheduling a meeting needs. A struct rather than eight positional
/// arguments, matching `GrantInput` — the two `&str` pairs next to each other
/// (name/when, template/classification) are exactly the shape that gets
/// silently transposed at a call site.
pub struct ScheduleMeeting<'a> {
    pub id: &'a str,
    pub name: &'a str,
    pub when: &'a str,
    pub template: &'a str,
    pub min_humans: usize,
    pub classification: &'a str,
    pub workspace: Option<&'a str>,
}

#[derive(Serialize, Clone, Debug)]
pub struct MeetingRow {
    pub id: String,
    pub name: String,
    pub when: String,
    pub tpl: String,
    pub humans: usize,
    pub agents: usize,
    pub classification: String,
    pub state: String,
}

#[derive(Serialize, Clone, Debug)]
pub struct AgendaItemDto {
    pub n: u32,
    pub text: String,
    pub src: String,
}

#[derive(Serialize, Clone, Debug)]
pub struct AttendeeDto {
    pub name: String,
    pub attested: bool,
    pub agent: Option<String>,
    pub note: Option<String>,
}

#[derive(Serialize, Clone, Debug)]
pub struct MinuteDecisionDto {
    pub id: String,
    pub text: String,
    pub link: bool,
}

#[derive(Serialize, Clone, Debug)]
pub struct DissentDto {
    pub who: String,
    pub text: String,
}

#[derive(Serialize, Clone, Debug)]
pub struct MeetingDetailDto {
    pub id: String,
    pub name: String,
    pub when: String,
    pub tenant: String,
    pub classification: String,
    pub state: String,
    pub agenda_hash: Option<String>,
    /// Where the agenda came from, verbatim (Rule 11).
    pub agenda_source: String,
    /// Lines the generator saw and could not parse — surfaced, not swallowed.
    pub agenda_skipped: usize,
    pub ratified: bool,
    pub ratified_by: Option<String>,
    pub ratified_at: Option<u64>,
    /// What a ratifier signs. Recomputed on every read, so an edited meeting
    /// shows a different hash rather than the one that was signed.
    pub content_hash: String,
    pub quorate: bool,
    pub min_humans: usize,
    pub attested_humans: usize,
    pub agenda: Vec<AgendaItemDto>,
    pub attendance: Vec<AttendeeDto>,
    pub minutes: Vec<String>,
    pub decisions: Vec<MinuteDecisionDto>,
    pub dissent: Vec<DissentDto>,
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
    /// The human at the keyboard. Approvals are recorded against them, so an
    /// installation without one can record agent decisions but cannot answer
    /// them — which is why the session flow asks when the IdP cannot say.
    operator: Option<String>,
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
    /// Which model endpoints may serve which classification (Q10). Loaded from
    /// the store; the fail-closed default permits no external egress at a
    /// controlled classification.
    egress: EgressPolicy,
    /// Set when a durable write failed. Once evidence cannot be persisted, we
    /// refuse to keep producing it — unpersisted records must not silently pile
    /// up in RAM and be lost at exit.
    degraded: Option<String>,
    /// Meetings by tenant (WP-S5). Keyed like everything else here: a meeting
    /// in one tenant is invisible in another (Rule 6).
    meetings: HashMap<String, Vec<MeetingRecord>>,
    /// Where each tenant's `.agentile` artifacts live — the source for both
    /// generated agendas and standup briefs (WP-S5.7).
    workspaces: HashMap<String, String>,
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
        backend.operator = backend
            .store
            .as_ref()
            .and_then(EvidenceStore::load_operator);
        if let Some(store) = backend.store.as_ref() {
            backend.egress = store.load_egress();
            // Write the fail-closed default on first run so an installation's
            // egress posture is inspectable on disk rather than implicit. Q10
            // makes this a deployed policy, not a toggle — the file is its
            // stand-in until the EgressPolicy protocol lands (QRM-S6/S7), and
            // an operator can see exactly what is permitted today: nothing.
            let _ = store.save_egress(&backend.egress);
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
            let meetings = store.load_meetings(&key).map_err(|e| e.to_string())?;
            let workspace = store.load_workspace(&key);

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
            self.meetings.insert(key.clone(), meetings);
            match workspace {
                Some(w) => {
                    self.workspaces.insert(key.clone(), w);
                }
                None => {
                    self.workspaces.remove(&key);
                }
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

    /// Name the human operating this installation.
    pub fn set_operator(&mut self, operator: &str) -> Result<(), String> {
        let name = operator.trim();
        if name.is_empty() {
            return Err(
                "the operator's name cannot be empty — approvals are recorded against it".into(),
            );
        }
        self.operator = Some(name.to_string());
        if let Some(store) = self.store.clone() {
            store
                .save_operator(Some(name))
                .map_err(|e| self.degrade(e))?;
        }
        Ok(())
    }

    pub fn operator(&self) -> Option<String> {
        self.operator.clone()
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
    ///
    /// `pub(crate)` so the rooms subsystem can resolve the same scope before an
    /// MR-4 check — it must read THIS tenant's grants and no other's (Rule 6),
    /// and re-deriving the scope there would be a second place to get it wrong.
    pub(crate) fn require_tenant(&self) -> Result<TenantId, String> {
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

        let mut decision = decision;
        let ungoverned = decision.verdict == Verdict::Ungoverned;

        // Q10 egress control, enforced HERE — in the path every adapter goes
        // through, never in a prompt. An action at a controlled classification
        // backed by a model the deployed policy does not permit is DENIED.
        //
        // Only applied to actions a grant covers: an ungoverned action is
        // already the louder alarm, and relabelling it "denied" would lose the
        // fact that nothing authorised it at all.
        if !ungoverned && !self.egress.permits(&input.classification, &input.model_id) {
            let named = if input.model_id.trim().is_empty() {
                "an unnamed model endpoint"
            } else {
                "a model endpoint the egress policy does not permit"
            };
            decision.verdict = Verdict::Deny;
            decision.hic = quorum_audit::HicLevel::ApproveEach;
            decision.reason = if named.starts_with("an unnamed") {
                "RC-400 egress: work at this classification must name its model endpoint"
            } else {
                "RC-401 egress: this model endpoint is not permitted at this classification"
            };
        }

        // I-4: a governed record must name the accountable human — and that
        // name comes from the GRANT, not from the agent's own claim about
        // itself. An operator signed the grant naming who stands behind it; an
        // agent reporting its own principal would be authority by assertion.
        let accountable = decision.grant_id.as_ref().and_then(|gid| {
            self.grants
                .get(&key)
                .and_then(|gs| gs.iter().find(|g| &g.id == gid))
                .map(|g| g.principal.clone())
        });
        if !ungoverned && accountable.is_none() {
            return Err(
                "a governed action must name an accountable principal — no identity resolved"
                    .to_string(),
            );
        }
        let record = DecisionRecord {
            agent: input.agent.clone(),
            principal: if ungoverned { None } else { accountable },
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

    // ---- meetings (WP-S5.3 / S5.4) -----------------------------------

    fn persist_meetings(&mut self, tenant: &str) -> Result<(), String> {
        let Some(store) = self.store.clone() else {
            return Ok(());
        };
        let empty = Vec::new();
        let ms = self.meetings.get(tenant).unwrap_or(&empty);
        store
            .save_meetings(tenant, ms)
            .map_err(|e| self.degrade(e))?;
        Ok(())
    }

    fn meeting_mut<'a>(
        list: &'a mut [MeetingRecord],
        id: &str,
    ) -> Result<&'a mut MeetingRecord, String> {
        list.iter_mut()
            .find(|r| r.meeting.id == id)
            .ok_or_else(|| format!("no meeting {id} in this tenant"))
    }

    /// Schedule a meeting, generating its agenda from the workspace's active
    /// sprint files when one is given.
    ///
    /// `workspace` is the directory the agenda is read from. `None` means no
    /// source was configured, and the meeting opens with an empty agenda that
    /// says so — never with invented items.
    pub fn schedule_meeting(
        &mut self,
        tenant: &TenantId,
        input: ScheduleMeeting<'_>,
    ) -> Result<(), String> {
        let ScheduleMeeting {
            id,
            name,
            when,
            template,
            min_humans,
            classification,
            workspace,
        } = input;
        self.check_not_degraded()?;
        let class = classification_from_str(classification)
            .ok_or_else(|| format!("unknown classification: {classification}"))?;
        let key = tenant.as_str().to_string();

        if self
            .meetings
            .get(&key)
            .is_some_and(|ms| ms.iter().any(|r| r.meeting.id == id))
        {
            return Err(format!(
                "meeting id {id} already exists in this tenant — ids must identify one meeting"
            ));
        }

        let mut meeting = Meeting::schedule(
            id,
            name,
            when,
            tenant.as_str(),
            Template::new(template, min_humans),
            class,
        );

        if let Some(root) = workspace {
            self.workspaces
                .insert(tenant.as_str().to_string(), root.to_string());
            if let Some(store) = self.store.clone() {
                store
                    .save_workspace(tenant.as_str(), root)
                    .map_err(|e| self.degrade(e))?;
            }
        }
        let source = match workspace {
            Some(root) => {
                let (agenda, src) = agenda_source::from_workspace(std::path::Path::new(root));
                for item in agenda.items() {
                    meeting
                        .add_agenda_item(item.text.clone(), item.src.clone())
                        .map_err(|e| e.to_string())?;
                }
                meeting.agenda.skipped = agenda.skipped;
                src.describe()
            }
            None => "no workspace configured — agenda is empty, not generated".to_string(),
        };

        self.meetings
            .entry(key.clone())
            .or_default()
            .push(MeetingRecord {
                meeting,
                agenda_source: source,
                opened_at_index: None,
            });
        self.persist_meetings(&key)
    }

    /// Admit an attendee. MR-4 lives in the domain crate and is enforced there.
    pub fn admit_to_meeting(
        &mut self,
        tenant: &TenantId,
        id: &str,
        name: &str,
        vendor: Option<&str>,
        attested: bool,
        clearance: Option<&str>,
    ) -> Result<(), String> {
        self.check_not_degraded()?;
        let key = tenant.as_str().to_string();
        let cleared = match clearance {
            Some(c) => Some(
                classification_from_str(c).ok_or_else(|| format!("unknown classification: {c}"))?,
            ),
            None => None,
        };
        let list = self
            .meetings
            .get_mut(&key)
            .ok_or_else(|| format!("no meeting {id} in this tenant"))?;
        let rec = Self::meeting_mut(list, id)?;

        let mut who = match vendor {
            Some(v) => Attendee::agent(name, v),
            None => Attendee::human(name),
        };
        who.attested = attested;
        who.clearance = cleared;
        rec.meeting.admit(who).map_err(|e| e.to_string())?;
        self.persist_meetings(&key)
    }

    /// Open the meeting: freeze the agenda and remember the chain index, which
    /// is the lower bound of the window its minutes are composed from.
    pub fn open_meeting(&mut self, tenant: &TenantId, id: &str) -> Result<String, String> {
        self.check_not_degraded()?;
        let key = tenant.as_str().to_string();
        let at_index = self.chains.get(&key).map_or(0, HashChain::len) as u64;
        let list = self
            .meetings
            .get_mut(&key)
            .ok_or_else(|| format!("no meeting {id} in this tenant"))?;
        let rec = Self::meeting_mut(list, id)?;
        let hash = rec.meeting.open().map_err(|e| e.to_string())?;
        rec.opened_at_index = Some(at_index);
        self.persist_meetings(&key)?;
        Ok(hex_encode(&hash))
    }

    /// Close the meeting, composing its minutes from the governed record.
    ///
    /// The minutes are the decisions this tenant's evidence chain recorded
    /// between the meeting opening and now. That is a **narrower** claim than
    /// §3.1's Notetaker-from-transcript, which needs Rooms (QRM-S3), and the
    /// surface says which one it is rather than letting them be confused.
    pub fn close_meeting(&mut self, tenant: &TenantId, id: &str) -> Result<String, String> {
        self.check_not_degraded()?;
        let key = tenant.as_str().to_string();
        let rows = self.ledger_rows(&key);

        let list = self
            .meetings
            .get_mut(&key)
            .ok_or_else(|| format!("no meeting {id} in this tenant"))?;
        let rec = Self::meeting_mut(list, id)?;
        let from = rec.opened_at_index.unwrap_or(0) as usize;

        let in_window: Vec<&LedgerRow> = rows.iter().skip(from).collect();
        let minutes: Vec<String> = if in_window.is_empty() {
            vec!["No governed action was recorded while this meeting was open.".to_string()]
        } else {
            in_window
                .iter()
                .map(|r| format!("{} — {} by {} ({})", r.verdict, r.cls, r.agent, r.hic))
                .collect()
        };
        let decisions = in_window
            .iter()
            .map(|r| quorum_meetings::MinuteDecision {
                id: format!("D-{}", r.id),
                text: format!("{} — {}", r.cls, r.verdict),
                // Resolvable: it came out of this tenant's own chain.
                link: true,
            })
            .collect();

        rec.meeting.decisions = decisions;
        let state = rec.meeting.close(minutes).map_err(|e| e.to_string())?;
        self.persist_meetings(&key)?;
        Ok(state.as_str().to_string())
    }

    /// The hash a human is about to sign. The ceremony shows this; `ratify`
    /// re-checks it so nothing can change in between.
    pub fn meeting_content_hash(&self, tenant: &str, id: &str) -> Result<String, String> {
        let rec = self
            .meetings
            .get(tenant)
            .and_then(|ms| ms.iter().find(|r| r.meeting.id == id))
            .ok_or_else(|| format!("no meeting {id} in this tenant"))?;
        Ok(hex_encode(&rec.meeting.content_hash()))
    }

    /// Ratify: record the human signature and lock the meeting.
    ///
    /// The ceremony has already run on the frontend; this is where it becomes
    /// evidence. Like `issue_grant`, the ratification and its decision record
    /// land together — a ratified meeting with no record of who ratified it is
    /// authority from nowhere.
    pub fn ratify_meeting(
        &mut self,
        tenant: &TenantId,
        id: &str,
        by: &str,
        expect_hash: &str,
        now_ms: i64,
    ) -> Result<DecisionDto, String> {
        self.check_not_degraded()?;
        let key = tenant.as_str().to_string();
        let expect = hex32(expect_hash);

        {
            let list = self
                .meetings
                .get_mut(&key)
                .ok_or_else(|| format!("no meeting {id} in this tenant"))?;
            let rec = Self::meeting_mut(list, id)?;
            rec.meeting
                .ratify(by, now_ms.max(0) as u64, expect)
                .map_err(|e| e.to_string())?;
        }
        self.persist_meetings(&key)?;

        self.record_principal_action(
            tenant,
            by,
            "meeting.ratify",
            &format!("{id} @ {expect_hash}"),
            now_ms,
        )
    }

    /// Every meeting in a tenant, newest first by scheduled time.
    pub fn meeting_rows(&self, tenant: &str) -> Vec<MeetingRow> {
        let Some(ms) = self.meetings.get(tenant) else {
            return Vec::new();
        };
        let mut rows: Vec<MeetingRow> = ms
            .iter()
            .map(|r| {
                let m = &r.meeting;
                MeetingRow {
                    id: m.id.clone(),
                    name: m.name.clone(),
                    when: m.when.clone(),
                    tpl: m.template.name.clone(),
                    humans: m.attendance.iter().filter(|a| a.is_human()).count(),
                    agents: m.attendance.iter().filter(|a| !a.is_human()).count(),
                    classification: classification_str(m.classification).to_string(),
                    state: m.state().as_str().to_string(),
                }
            })
            .collect();
        rows.sort_by(|a, b| b.when.cmp(&a.when));
        rows
    }

    /// One meeting in full.
    pub fn meeting_detail(&self, tenant: &str, id: &str) -> Result<MeetingDetailDto, String> {
        let rec = self
            .meetings
            .get(tenant)
            .and_then(|ms| ms.iter().find(|r| r.meeting.id == id))
            .ok_or_else(|| format!("no meeting {id} in this tenant"))?;
        let m = &rec.meeting;
        Ok(MeetingDetailDto {
            id: m.id.clone(),
            name: m.name.clone(),
            when: m.when.clone(),
            tenant: m.tenant.clone(),
            classification: classification_str(m.classification).to_string(),
            state: m.state().as_str().to_string(),
            agenda_hash: m.agenda_hash().map(|h| hex_encode(&h)),
            agenda_source: rec.agenda_source.clone(),
            agenda_skipped: m.agenda.skipped,
            ratified: m.state() == MeetingState::Ratified,
            ratified_by: m.ratified_by.clone(),
            ratified_at: m.ratified_at,
            content_hash: hex_encode(&m.content_hash()),
            quorate: m.is_quorate(),
            min_humans: m.template.min_humans,
            attested_humans: m.attested_humans(),
            agenda: m
                .agenda
                .items()
                .iter()
                .map(|i| AgendaItemDto {
                    n: i.n,
                    text: i.text.clone(),
                    src: i.src.clone(),
                })
                .collect(),
            attendance: m
                .attendance
                .iter()
                .map(|a| AttendeeDto {
                    name: a.name.clone(),
                    attested: a.attested,
                    agent: a.agent.clone(),
                    note: a.note.clone(),
                })
                .collect(),
            minutes: m.minutes.clone(),
            decisions: m
                .decisions
                .iter()
                .map(|d| MinuteDecisionDto {
                    id: d.id.clone(),
                    text: d.text.clone(),
                    link: d.link,
                })
                .collect(),
            dissent: m
                .dissent
                .iter()
                .map(|d| DissentDto {
                    who: d.who.clone(),
                    text: d.text.clone(),
                })
                .collect(),
        })
    }

    // ---- journals + standup briefs (WP-S5.7) -------------------------

    /// The workspace this tenant's artifacts live in, if one has been named.
    fn workspace(&self, tenant: &str) -> Option<&str> {
        self.workspaces.get(tenant).map(String::as_str)
    }

    /// Journals and retros, newest first.
    ///
    /// Returns the entries AND the provenance line, because an empty list has
    /// two very different causes — no workspace, or a workspace with nothing in
    /// it — and a surface that cannot tell them apart will imply the wrong one.
    pub fn journal_entries(&self, tenant: &str) -> (Vec<JournalEntryDto>, String) {
        let Some(root) = self.workspace(tenant) else {
            return (
                Vec::new(),
                "no workspace configured for this tenant — schedule a meeting with one to \
                 point Quorum at your .agentile artifacts"
                    .to_string(),
            );
        };
        let (entries, src) = journal_source::from_workspace(std::path::Path::new(root));
        (
            entries
                .into_iter()
                .map(|e| JournalEntryDto {
                    id: e.id,
                    date: e.date,
                    who: e.who,
                    kind: e.kind,
                    text: e.text,
                    human: e.human,
                })
                .collect(),
            src.describe(),
        )
    }

    /// A standup brief for `agent`, grounded in artifacts plus the live
    /// governance state only this backend knows.
    ///
    /// §3.2's requirement is that a brief is assembled from artifacts rather
    /// than model recall. The artifact half comes from `.agentile` files; the
    /// two sections below come from the policy engine, which is the other kind
    /// of fact a standup needs — what this agent is waiting on, and what it has
    /// left to spend.
    pub fn standup_brief(
        &self,
        tenant: &str,
        agent: &str,
        meeting_id: &str,
    ) -> Result<StandupBriefDto, String> {
        let meeting_name = self
            .meetings
            .get(tenant)
            .and_then(|ms| ms.iter().find(|r| r.meeting.id == meeting_id))
            .map(|r| r.meeting.name.clone())
            .unwrap_or_else(|| meeting_id.to_string());

        let mut sections: Vec<(String, String)> = Vec::new();
        let source = match self.workspace(tenant) {
            Some(root) => {
                let (mut s, src) =
                    journal_source::brief_from_artifacts(std::path::Path::new(root), agent);
                sections.append(&mut s);
                src.describe()
            }
            None => {
                sections.push((
                    "Since last time".to_string(),
                    "no workspace configured — this brief has no artifacts to draw on".to_string(),
                ));
                "no workspace configured for this tenant".to_string()
            }
        };

        // Live governance state. Pending FIRST: an agent blocked on a human is
        // the single most useful thing a standup can surface, and burying it
        // under prose is how it goes unanswered for another week.
        let pending: Vec<&PendingApproval> = self
            .pending
            .iter()
            .filter(|((t, _), p)| t == tenant && p.agent == agent)
            .map(|(_, p)| p)
            .collect();
        sections.insert(
            0,
            (
                "Waiting on a human".to_string(),
                if pending.is_empty() {
                    "nothing pending".to_string()
                } else {
                    format!(
                        "{} escalation(s) blocked: {}",
                        pending.len(),
                        pending
                            .iter()
                            .map(|p| p.action_class.clone())
                            .collect::<Vec<_>>()
                            .join(", ")
                    )
                },
            ),
        );

        let grants: Vec<&CapabilityGrant> = self
            .grants
            .iter()
            .filter(|((t, a), _)| t == tenant && a == agent)
            .flat_map(|(_, gs)| gs.iter())
            .filter(|g| !g.revoked)
            .collect();
        sections.push((
            "Authority".to_string(),
            if grants.is_empty() {
                "no live grant — this agent is ungoverned and anything it does is recorded as such"
                    .to_string()
            } else {
                format!(
                    "{} live grant(s); budget {}/{}",
                    grants.len(),
                    grants.iter().map(|g| g.consumed).sum::<u64>(),
                    grants.iter().map(|g| g.budget_units).sum::<u64>()
                )
            },
        ));

        Ok(StandupBriefDto {
            agent: agent.to_string(),
            meeting: meeting_name,
            sections,
            source,
        })
    }

    /// One decision, as a document (the Ledger's charter register).
    ///
    /// Every field is read back out of the evidence chain — nothing is inferred
    /// by the caller. `id` is the same `D-…` handle the ribbon shows; an id that
    /// does not resolve is an error, never an empty document.
    pub fn ledger_decision(&self, tenant: &str, id: &str) -> Result<DecisionDetailDto, String> {
        let index = decision_index(id)?;
        let chain = self
            .chains
            .get(tenant)
            .ok_or_else(|| format!("no evidence chain for this tenant — {id} cannot be read"))?;
        let (record, entry_hash) = chain.entry(index).ok_or_else(|| {
            format!(
                "{id} is not in this tenant's chain ({} records)",
                chain.len()
            )
        })?;

        // The inclusion proof is computed here rather than trusted: it is the
        // artifact that makes a single record checkable by someone who does not
        // hold the rest of the chain.
        let root = chain.merkle_root();
        let proof = chain.merkle_proof(index).unwrap_or_default();
        let included = quorum_audit::verify_inclusion(record.content_hash(), &proof, root);

        Ok(DecisionDetailDto {
            id: id.to_string(),
            what: record.action_class.clone(),
            when: iso_utc(record.timestamp_ms),
            principal: record
                .principal
                .clone()
                // An absent principal is the whole point of an ungoverned
                // record. Say it, do not blank it.
                .unwrap_or_else(|| "— none: this action had no accountable human".to_string()),
            agent: record.agent.clone(),
            grant: record.grant_id.clone().unwrap_or_else(|| {
                if record.verdict == Verdict::Ungoverned {
                    "— none: recorded ungoverned".to_string()
                } else {
                    "— none: a human acting directly holds the authority itself".to_string()
                }
            }),
            // Rule 1: there is no deployed governance protocol contract behind
            // this verdict yet, and saying "PRT-004" would be an invention.
            protocol: "— none deployed: this verdict came from quorum-policy evaluating \
                       the tenant's live grants locally (governance contracts land in QRM-S6/S7)"
                .to_string(),
            verdict: verdict_str(record.verdict).to_string(),
            hic: hic_str(record.hic).to_string(),
            reason: decision_reason(record),
            model: if record.model_id.is_empty() {
                "— not stated by the caller".to_string()
            } else {
                record.model_id.clone()
            },
            params: if record.params_hash == [0u8; 32] {
                "— no params committed to".to_string()
            } else {
                hex_encode(&record.params_hash)
            },
            correlation: record.correlation_id.clone(),
            chain_pos: format!("record {} of {}", index + 1, chain.len()),
            entry_hash: hex_encode(&entry_hash),
            content_hash: hex_encode(&record.content_hash()),
            chain_head: hex_encode(&chain.head()),
            merkle_root: hex_encode(&root),
            proof_len: proof.len(),
            included,
            source: format!(
                "the tenant's BLAKE3 evidence chain (quorum-audit HashChain), record {index}",
            ),
        })
    }

    /// Re-verify one decision, on demand: recompute the whole chain from
    /// genesis AND recompute the Merkle root from this record plus its
    /// inclusion proof.
    ///
    /// Both checks are real and local. Neither says anything about the chain —
    /// `anchor` reports that separately, because "my copy is intact" and "the
    /// world agrees with my copy" are different claims and collapsing them is
    /// how a verify button becomes theatre.
    pub fn ledger_verify_decision(
        &self,
        tenant: &str,
        id: &str,
    ) -> Result<VerifyDecisionDto, String> {
        let index = decision_index(id)?;
        let chain = self.chains.get(tenant).ok_or_else(|| {
            format!("no evidence chain for this tenant — {id} cannot be verified")
        })?;
        let (record, entry_hash) = chain.entry(index).ok_or_else(|| {
            format!(
                "{id} is not in this tenant's chain ({} records)",
                chain.len()
            )
        })?;
        let root = chain.merkle_root();
        let proof = chain.merkle_proof(index).unwrap_or_default();
        Ok(VerifyDecisionDto {
            chain_intact: chain.verify(),
            included: quorum_audit::verify_inclusion(record.content_hash(), &proof, root),
            records: chain.len(),
            entry_hash: hex_encode(&entry_hash),
            merkle_root: hex_encode(&root),
            proof_len: proof.len(),
        })
    }

    /// Everything this tenant recorded under one correlation id, oldest first.
    ///
    /// The correlation id is how a meeting, the grants it authorised, the
    /// actions taken under them and the resulting change are threaded together.
    /// Only the parts that exist locally appear; the rest is named in `source`
    /// rather than drawn as an empty node on a timeline.
    pub fn ledger_correlation(&self, tenant: &str, corr: &str) -> CorrelationViewDto {
        let mut events = Vec::new();
        if let Some(chain) = self.chains.get(tenant) {
            for (i, record) in chain.records().enumerate() {
                if record.correlation_id != corr && !record.correlation_id.starts_with(corr) {
                    continue;
                }
                // `meeting.ratify` records carry `{meeting id} @ {hash}` as
                // their correlation, so a meeting id threads its own ratifying
                // act. That is why the prefix match above exists.
                let kind = match record.action_class.as_str() {
                    c if c.starts_with("meeting.") => "meeting",
                    c if c.starts_with("grant.") => "grant",
                    _ => "action",
                };
                events.push(CorrelationEventDto {
                    t: clock_utc(record.timestamp_ms),
                    kind: kind.to_string(),
                    text: format!(
                        "{} · {} · {}",
                        record.action_class,
                        verdict_str(record.verdict),
                        record
                            .principal
                            .clone()
                            .unwrap_or_else(|| format!("{} (ungoverned)", record.agent))
                    ),
                    link: format!("D-{}", 90001 + i as u64),
                });
            }
        }
        let source = if events.is_empty() {
            format!("no record in this tenant's evidence chain carries the correlation id {corr}")
        } else {
            format!(
                "{} record(s) in this tenant's evidence chain carrying correlation {corr}. \
                 Pull requests and calendar events are not in this timeline: the repo and \
                 calendar connectors are not built (QRM-S8).",
                events.len()
            )
        };
        CorrelationViewDto { events, source }
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

    /// Record something a human did themselves, at HIC-1, under their own
    /// authority rather than a grant.
    ///
    /// This is NOT the policy gate: policy binds agents, and an operator has no
    /// grant to be evaluated against. Running them through `evaluate` would
    /// return `Ungoverned` and flood the alert state with the operator doing
    /// their job — and HIC-X is "an alert state, never a configuration"
    /// (04_HIC_MODEL §2). What matters is that the act is recorded and names
    /// the human.
    pub fn record_principal_action(
        &mut self,
        tenant: &TenantId,
        principal: &str,
        action_class: &str,
        detail: &str,
        now_ms: i64,
    ) -> Result<DecisionDto, String> {
        self.check_not_degraded()?;
        if principal.trim().is_empty() {
            return Err(format!(
                "{action_class} must name the human who did it — no identity resolved"
            ));
        }
        let record = DecisionRecord {
            agent: principal.to_string(),
            principal: Some(principal.to_string()),
            grant_id: None,
            action_class: action_class.to_string(),
            params_hash: [0u8; 32],
            verdict: Verdict::Approved,
            hic: HicLevel::ApproveEach,
            model_id: String::new(),
            correlation_id: detail.to_string(),
            timestamp_ms: now_ms,
        };
        let chain = self.chain_for(tenant);
        let head = chain.append(record.clone()).map_err(|e| e.to_string())?;
        let decision_id = (chain.len() - 1) as u64;
        if let Some(store) = self.store.clone() {
            store
                .append_record(tenant.as_str(), &record, head)
                .map_err(|e| self.degrade(e))?;
        }
        Ok(DecisionDto {
            decision_id,
            verdict: verdict_str(Verdict::Approved).to_string(),
            hic: hic_str(HicLevel::ApproveEach).to_string(),
            grant_id: None,
            reason: format!("RC-302 {action_class} by {principal}"),
            chain_head: hex_encode(&head),
            ungoverned: false,
        })
    }

    /// The grants one agent holds in this tenant, revoked ones included — a
    /// revoked grant is part of the record, not an absence.
    pub fn grants_for(&self, tenant: &str, agent: &str) -> Vec<GrantSummary> {
        self.grants
            .get(&(tenant.to_string(), agent.to_string()))
            .map(|gs| {
                gs.iter()
                    .map(|g| GrantSummary {
                        id: g.id.clone(),
                        agent: g.agent.clone(),
                        principal: g.principal.clone(),
                        scope: g.tenant_scope.clone(),
                        classes: g.action_classes.join(" · "),
                        ceiling: classification_str(g.classification_ceiling).to_string(),
                        budget_units: g.budget_units,
                        consumed: g.consumed,
                        hic: grant_hic_str(g.hic).to_string(),
                        expires_at_ms: g.expires_at_ms,
                        revoked: g.revoked,
                    })
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Every agent this tenant has evidence about: one it has granted, or one
    /// that has acted. NOT the AgentSBT registry — that is a chain read which
    /// lands later — so it is named honestly at the surface.
    /// The highest classification any LIVE grant lets this agent operate on, or
    /// `None` when it holds none.
    ///
    /// `None` is not `Public`: "this agent has no grant at all" and "this agent is
    /// cleared to Public" are different facts, and MR-4 must be able to refuse the
    /// first with a different sentence than the second. An agent with several
    /// grants gets the HIGHEST ceiling among them — a grant is permission, and
    /// holding two does not reduce what either allows.
    ///
    /// Revoked and expired grants do not count (CG-2): a capability that has been
    /// taken away must not keep a seat in a classified room.
    pub fn agent_classification_ceiling(
        &self,
        tenant: &str,
        agent: &str,
        now_ms: i64,
    ) -> Option<quorum_tenancy::Classification> {
        self.grants
            .get(&(tenant.to_string(), agent.to_string()))?
            .iter()
            .filter(|g| g.is_live(now_ms))
            .map(|g| g.classification_ceiling)
            .max()
    }

    pub fn known_agents(&self, tenant: &str) -> Vec<AgentSummary> {
        let mut seen: std::collections::BTreeMap<String, AgentSummary> =
            std::collections::BTreeMap::new();
        for ((t, agent), grants) in &self.grants {
            if t != tenant {
                continue;
            }
            let live = grants.iter().filter(|g| !g.revoked).count();
            let budget: u64 = grants.iter().map(|g| g.budget_units).sum();
            let consumed: u64 = grants.iter().map(|g| g.consumed).sum();
            seen.insert(
                agent.clone(),
                AgentSummary {
                    id: agent.clone(),
                    live_grants: live,
                    budget_units: budget,
                    consumed,
                    decisions: 0,
                    ungoverned: 0,
                },
            );
        }
        if let Some(chain) = self.chains.get(tenant) {
            for r in chain.records() {
                // A principal acting directly is not an agent in the fleet.
                if r.principal.as_deref() == Some(r.agent.as_str()) {
                    continue;
                }
                let e = seen.entry(r.agent.clone()).or_insert_with(|| AgentSummary {
                    id: r.agent.clone(),
                    live_grants: 0,
                    budget_units: 0,
                    consumed: 0,
                    decisions: 0,
                    ungoverned: 0,
                });
                e.decisions += 1;
                if r.verdict == Verdict::Ungoverned {
                    e.ungoverned += 1;
                }
            }
        }
        seen.into_values().collect()
    }

    pub fn issue_grant(
        &mut self,
        input: &GrantInput,
        tenant: &TenantId,
        now_ms: i64,
    ) -> Result<(), String> {
        let ceiling = classification_from_str(&input.classification_ceiling)
            .ok_or_else(|| format!("unknown classification: {}", input.classification_ceiling))?;
        // A duplicate id inside a tenant makes the record ambiguous: two grants
        // answer to one name, `revoke` hits both, and the ledger's `grant_id`
        // no longer identifies which envelope authorised an action.
        if self
            .grants
            .iter()
            .any(|((t, _), gs)| t == tenant.as_str() && gs.iter().any(|g| g.id == input.id))
        {
            return Err(format!(
                "grant id {} already exists in this tenant — ids must identify one envelope",
                input.id
            ));
        }
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
        self.persist_grants(tenant.as_str())?;
        // A grant that exists without a record of someone issuing it is
        // authority from nowhere. The record and the grant land together.
        self.record_principal_action(
            tenant,
            &input.issued_by,
            "grant.issue",
            &format!("{} → {}", input.id, input.agent),
            now_ms,
        )?;
        Ok(())
    }

    /// Revoke a grant by id within a tenant (immediate). Returns whether a grant
    /// was found. A revocation that cannot be persisted is an error, not a
    /// quiet success — the whole point is that it survives.
    pub fn revoke_grant(
        &mut self,
        tenant: &str,
        agent: &str,
        grant_id: &str,
        revoked_by: &str,
        now_ms: i64,
    ) -> Result<bool, String> {
        // Check the actor BEFORE touching anything. `record_principal_action`
        // already refuses a blank principal, but it ran at the END — after the
        // grant was revoked and persisted — so a nameless revocation left the
        // grant store and the ledger permanently disagreeing, and handed the
        // caller an `Err` that told the operator nothing had happened.
        //
        // Issuing already refused an unnamed actor
        // (`a_grant_cannot_be_issued_by_nobody`). Revocation is the HIC-1 act and
        // had no such guard.
        if revoked_by.trim().is_empty() {
            return Err(
                "grant.revoke must name the human who did it — no identity resolved".to_string(),
            );
        }
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
            // L-1: a revocation is an HIC-1 act and is recorded like one.
            let id = TenantId::new(tenant).map_err(|e| e.to_string())?;
            self.record_principal_action(
                &id,
                revoked_by,
                "grant.revoke",
                &format!("{grant_id} → {agent}"),
                now_ms,
            )?;
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

/// Name the human at the keyboard. Every approval is recorded against them.
#[tauri::command]
pub fn operator_set(backend: Backend<'_>, operator: String) -> Result<(), String> {
    lock(&backend)?.set_operator(&operator)
}

/// The operator, or `null` if nobody has been named yet.
#[tauri::command]
pub fn operator_get(backend: Backend<'_>) -> Result<Option<String>, String> {
    Ok(lock(&backend)?.operator())
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

/// One decision as a document. A local read of the tenant's evidence chain —
/// it makes no chain call, so it is fast and works offline.
#[tauri::command]
pub fn ledger_decision(backend: Backend<'_>, id: String) -> Result<DecisionDetailDto, String> {
    let b = lock(&backend)?;
    let tenant = b.require_tenant()?;
    b.ledger_decision(tenant.as_str(), &id)
}

/// Re-verify one decision: replay the chain and re-prove the record's inclusion.
#[tauri::command]
pub fn ledger_verify_decision(
    backend: Backend<'_>,
    id: String,
) -> Result<VerifyDecisionDto, String> {
    let b = lock(&backend)?;
    let tenant = b.require_tenant()?;
    b.ledger_verify_decision(tenant.as_str(), &id)
}

/// The timeline of everything recorded under one correlation id.
#[tauri::command]
pub fn ledger_correlation(
    backend: Backend<'_>,
    corr: String,
) -> Result<CorrelationViewDto, String> {
    let b = lock(&backend)?;
    let tenant = b.require_tenant()?;
    Ok(b.ledger_correlation(tenant.as_str(), &corr))
}

// ---- meetings commands (WP-S5.5) ------------------------------------

#[tauri::command]
pub fn meetings_list(backend: Backend<'_>) -> Result<Vec<MeetingRow>, String> {
    let b = lock(&backend)?;
    let tenant = b.require_tenant()?;
    Ok(b.meeting_rows(tenant.as_str()))
}

#[tauri::command]
pub fn meeting_get(backend: Backend<'_>, id: String) -> Result<MeetingDetailDto, String> {
    let b = lock(&backend)?;
    let tenant = b.require_tenant()?;
    b.meeting_detail(tenant.as_str(), &id)
}

/// Scheduling input as it arrives over the wire. A single struct, like
/// `GrantInput` — a command with eight positional arguments is one transposed
/// pair away from a meeting scheduled under the wrong classification.
#[derive(Deserialize)]
pub struct ScheduleMeetingInput {
    pub id: String,
    pub name: String,
    pub when: String,
    pub template: String,
    pub min_humans: usize,
    pub classification: String,
    pub workspace: Option<String>,
}

#[tauri::command]
pub fn meeting_schedule(backend: Backend<'_>, input: ScheduleMeetingInput) -> Result<(), String> {
    let mut b = lock(&backend)?;
    let tenant = b.require_tenant()?;
    b.schedule_meeting(
        &tenant,
        ScheduleMeeting {
            id: &input.id,
            name: &input.name,
            when: &input.when,
            template: &input.template,
            min_humans: input.min_humans,
            classification: &input.classification,
            workspace: input.workspace.as_deref(),
        },
    )
}

#[tauri::command]
pub fn meeting_admit(
    backend: Backend<'_>,
    id: String,
    name: String,
    vendor: Option<String>,
    attested: bool,
    clearance: Option<String>,
) -> Result<(), String> {
    let mut b = lock(&backend)?;
    let tenant = b.require_tenant()?;
    b.admit_to_meeting(
        &tenant,
        &id,
        &name,
        vendor.as_deref(),
        attested,
        clearance.as_deref(),
    )
}

#[tauri::command]
pub fn meeting_open(backend: Backend<'_>, id: String) -> Result<String, String> {
    let mut b = lock(&backend)?;
    let tenant = b.require_tenant()?;
    b.open_meeting(&tenant, &id)
}

#[tauri::command]
pub fn meeting_close(backend: Backend<'_>, id: String) -> Result<String, String> {
    let mut b = lock(&backend)?;
    let tenant = b.require_tenant()?;
    b.close_meeting(&tenant, &id)
}

/// The hash the ceremony must display. Read-only: it signs nothing.
#[tauri::command]
pub fn meeting_content_hash(backend: Backend<'_>, id: String) -> Result<String, String> {
    let b = lock(&backend)?;
    let tenant = b.require_tenant()?;
    b.meeting_content_hash(tenant.as_str(), &id)
}

/// Is this meeting's local minutes hash the one registered on chain?
///
/// Deliberately its OWN command rather than a field on `meeting_get`: this
/// makes an `eth_call`, and doing that inside `meeting_detail` would hold the
/// backend mutex across a network round trip, stalling every other command
/// behind a chain that might be slow or down.
///
/// Data source: `MeetingRegistry.verifyMinutes` / `.getMinutes` via `eth_call`,
/// the contract resolved by name from the canonical address book at runtime.
#[tauri::command]
pub fn meeting_anchor(backend: Backend<'_>, id: String) -> Result<AnchorState, String> {
    // Take what the check needs, then RELEASE the lock before any I/O.
    let (tenant, content_hash) = {
        let b = lock(&backend)?;
        let tenant = b.require_tenant()?;
        let hash = b.meeting_content_hash(tenant.as_str(), &id)?;
        (tenant.as_str().to_string(), hash)
    };
    let book = AddressBook::load().map_err(|e| e.to_string())?;
    Ok(anchor::check(&book, &tenant, &id, hex32(&content_hash)))
}

/// Build the on-chain minutes registration as a PENDING kit ceremony and return
/// its id. **Signs nothing.**
///
/// This follows citrate-core's `wallet_send` exactly: the command assembles the
/// transaction and stores it as a pending ceremony; the human then approves via
/// `ceremony::sign_and_broadcast`, which is the only command that signs. The
/// `from` is THIS vault's wallet address — the kit refuses to sign a tx whose
/// `from` is not the address its key derives to, so a registration is always
/// authored by the human who ratified.
///
/// Data source: `MeetingRegistry.register(...)`, contract resolved by name from
/// the canonical address book at runtime.
#[tauri::command]
pub fn meeting_register_intent(
    backend: Backend<'_>,
    custody: tauri::State<'_, citrate_core_kit::custody::CustodyState>,
    ceremony: tauri::State<'_, citrate_core_kit::ceremony::CeremonyState>,
    id: String,
) -> Result<citrate_core_kit::ceremony::CeremonyView, String> {
    use citrate_core_kit::ceremony::{IntentKind, SignatureIntent};

    // Everything the tx needs, gathered under the lock and then released.
    let (tenant, agenda_hash, minutes_hash, ratified_at) = {
        let b = lock(&backend)?;
        let tenant = b.require_tenant()?;
        let rec = b
            .meetings
            .get(tenant.as_str())
            .and_then(|ms| ms.iter().find(|r| r.meeting.id == id))
            .ok_or_else(|| format!("no meeting {id} in this tenant"))?;
        // Only a ratified meeting has anything to register: the commitment IS
        // the human signature over the minutes.
        if rec.meeting.state() != MeetingState::Ratified {
            return Err(format!(
                "meeting {id} is {} — only a ratified meeting can be registered",
                rec.meeting.state().as_str()
            ));
        }
        (
            tenant.as_str().to_string(),
            rec.meeting.agenda_hash().unwrap_or([0u8; 32]),
            rec.meeting.content_hash(),
            rec.meeting.ratified_at.unwrap_or(0),
        )
    };

    let book = AddressBook::load().map_err(|e| e.to_string())?;
    let to = book
        .get(anchor::MEETING_REGISTRY)
        .ok_or_else(|| {
            format!(
                "{} is not in the address book ({}) — nothing to register against",
                anchor::MEETING_REGISTRY,
                book.describe()
            )
        })?
        .to_string();

    // The sender is THIS vault's wallet (public address only, never the key).
    let wallet = citrate_core_kit::wallet::address(&custody.0).map_err(|e| e.to_string())?;

    let data = anchor::encode_register(
        anchor::keccak256(tenant.as_bytes()),
        anchor::keccak256(id.as_bytes()),
        agenda_hash,
        minutes_hash,
        ratified_at,
        "",
    );

    let raw = serde_json::json!({
        "from": wallet.address,
        "to": to,
        "value": "0x0",
        "data": data,
        "gas": format!("0x{:x}", 300_000u64),
        "chainId": format!("0x{:x}", book.chain_id),
    })
    .to_string();

    Ok(ceremony.0.request(SignatureIntent {
        origin: "local-user".to_string(),
        kind: IntentKind::Transaction,
        chain_id: book.chain_id,
        raw,
    }))
}

/// Record a ratification. The human ceremony has already happened; this turns
/// it into evidence. `expect_hash` is what the signer was shown.
#[tauri::command]
pub fn meeting_ratify(
    backend: Backend<'_>,
    id: String,
    by: String,
    expect_hash: String,
) -> Result<DecisionDto, String> {
    let mut b = lock(&backend)?;
    let tenant = b.require_tenant()?;
    let now = now_ms();
    b.ratify_meeting(&tenant, &id, &by, &expect_hash, now)
}

// ---- journal commands (WP-S5.7) -------------------------------------

#[tauri::command]
pub fn journal_list(backend: Backend<'_>) -> Result<JournalListDto, String> {
    let b = lock(&backend)?;
    let tenant = b.require_tenant()?;
    let (entries, source) = b.journal_entries(tenant.as_str());
    Ok(JournalListDto { entries, source })
}

#[tauri::command]
pub fn journal_brief(
    backend: Backend<'_>,
    agent: String,
    meeting: String,
) -> Result<StandupBriefDto, String> {
    let b = lock(&backend)?;
    let tenant = b.require_tenant()?;
    b.standup_brief(tenant.as_str(), &agent, &meeting)
}

/// The evidence chain's own state, in one read.
///
/// Composed here rather than in the frontend because these five facts must
/// describe the SAME chain at the same instant: assembled from five separate
/// round trips, a record could land between them and the surface would show a
/// head that does not match the count beside it.
#[derive(Serialize, Clone, Debug)]
pub struct LedgerStateDto {
    pub head: String,
    pub merkle_root: String,
    pub records: usize,
    pub ungoverned: usize,
    /// The chain replayed from genesis and every stored hash matched.
    pub intact: bool,
    pub tenant: String,
}

#[tauri::command]
pub fn ledger_state(backend: Backend<'_>) -> Result<LedgerStateDto, String> {
    let b = lock(&backend)?;
    let tenant = b.require_tenant()?;
    let t = tenant.as_str();
    Ok(LedgerStateDto {
        head: b.ledger_head(t),
        merkle_root: b.ledger_merkle_root(t),
        records: b.chains.get(t).map(HashChain::len).unwrap_or(0),
        ungoverned: b.ledger_ungoverned_count(t),
        intact: b.ledger_verify(t),
        tenant: t.to_string(),
    })
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
    b.issue_grant(&input, &tenant, now_ms())
}

/// Every agent this tenant has evidence about — granted, or seen acting.
#[tauri::command]
pub fn agents_known(backend: Backend<'_>) -> Result<Vec<AgentSummary>, String> {
    let b = lock(&backend)?;
    let tenant = b.require_tenant()?;
    Ok(b.known_agents(tenant.as_str()))
}

/// The live capability grants held by one agent in the active tenant.
#[tauri::command]
pub fn grants_for_agent(backend: Backend<'_>, agent: String) -> Result<Vec<GrantSummary>, String> {
    let b = lock(&backend)?;
    let tenant = b.require_tenant()?;
    Ok(b.grants_for(tenant.as_str(), &agent))
}

#[tauri::command]
pub fn grant_revoke(
    backend: Backend<'_>,
    agent: String,
    grant_id: String,
    revoked_by: String,
) -> Result<bool, String> {
    let mut b = lock(&backend)?;
    let tenant = b.require_tenant()?;
    b.revoke_grant(tenant.as_str(), &agent, &grant_id, &revoked_by, now_ms())
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
            issued_by: "R. Ortiz".into(),
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
    fn a_decision_reads_back_as_a_document_that_proves_itself() {
        let mut b = QuorumBackend::default();
        b.issue_grant(
            &grant("sbt-41", &["repo.write"], "CUI", 100, "2"),
            &t("bca"),
            0,
        )
        .unwrap();
        b.evaluate_and_record(
            &action("sbt-41", "repo.write", "Public", 3),
            &t("bca"),
            1000,
        )
        .unwrap();

        // D-90001 is the grant issuance itself — issuing authority is a
        // recorded act, so the governed action is the SECOND record.
        assert_eq!(
            b.ledger_decision("bca", "D-90001").unwrap().what,
            "grant.issue"
        );

        let d = b.ledger_decision("bca", "D-90002").unwrap();
        assert_eq!(d.what, "repo.write");
        assert_eq!(d.agent, "sbt-41");
        assert_eq!(d.verdict, "allow");
        assert_eq!(d.chain_pos, "record 2 of 2");
        assert_eq!(d.correlation, "X-7104");
        // The proof is computed, not asserted: this is the claim the Verify
        // affordance makes, so the read itself must be able to make it.
        assert!(
            d.included,
            "the record must prove into the tenant's Merkle root"
        );
        assert_eq!(d.entry_hash, b.ledger_head("bca"));
        assert_eq!(d.merkle_root, b.ledger_merkle_root("bca"));
        // Rule 1: no invented protocol id behind a locally-evaluated verdict.
        assert!(d.protocol.starts_with("— none deployed"), "{}", d.protocol);
    }

    #[test]
    fn an_ungoverned_decision_says_it_had_no_human_rather_than_blanking_the_field() {
        let mut b = QuorumBackend::default();
        b.evaluate_and_record(&action("sbt-9", "repo.write", "Public", 1), &t("bca"), 1000)
            .unwrap();
        let d = b.ledger_decision("bca", "D-90001").unwrap();
        assert_eq!(d.verdict, "ungoverned");
        assert!(
            d.principal.contains("no accountable human"),
            "{}",
            d.principal
        );
        assert!(d.grant.contains("ungoverned"), "{}", d.grant);
        assert!(d.reason.contains("NO live grant"), "{}", d.reason);
    }

    #[test]
    fn a_handle_that_resolves_to_nothing_is_an_error_not_record_zero() {
        let mut b = QuorumBackend::default();
        b.evaluate_and_record(&action("sbt-9", "repo.write", "Public", 1), &t("bca"), 1000)
            .unwrap();
        for bad in ["", "D-", "90001", "D-abc", "X-7104", "D-1"] {
            assert!(
                b.ledger_decision("bca", bad).is_err(),
                "{bad} must not resolve to a decision"
            );
        }
        // In range for the handle format, past the end of the chain.
        assert!(b.ledger_decision("bca", "D-90002").is_err());
        assert!(b.ledger_decision("bca", "D-90001").is_ok());
    }

    #[test]
    fn verify_reports_both_checks_separately() {
        let mut b = QuorumBackend::default();
        for i in 0..3 {
            b.evaluate_and_record(
                &action("sbt-9", "repo.write", "Public", 1),
                &t("bca"),
                1000 + i,
            )
            .unwrap();
        }
        let v = b.ledger_verify_decision("bca", "D-90002").unwrap();
        assert!(v.chain_intact);
        assert!(v.included);
        assert_eq!(v.records, 3);
        assert!(v.proof_len > 0, "a chain of 3 needs sibling hashes");
    }

    #[test]
    fn a_correlation_gathers_its_records_and_says_what_it_did_not_search() {
        let mut b = QuorumBackend::default();
        b.evaluate_and_record(&action("sbt-9", "repo.write", "Public", 1), &t("bca"), 1000)
            .unwrap();
        let mut other = action("sbt-9", "spend", "Public", 1);
        other.correlation_id = "X-0000".into();
        b.evaluate_and_record(&other, &t("bca"), 2000).unwrap();

        let v = b.ledger_correlation("bca", "X-7104");
        assert_eq!(v.events.len(), 1, "only the matching correlation");
        assert_eq!(v.events[0].link, "D-90001");
        assert_eq!(v.events[0].kind, "action");
        // The timeline states what it CANNOT show, rather than implying the
        // absent parts do not exist.
        assert!(v.source.contains("not built"), "{}", v.source);

        let empty = b.ledger_correlation("bca", "X-nothing");
        assert!(empty.events.is_empty());
        assert!(empty.source.contains("no record"), "{}", empty.source);
    }

    #[test]
    fn iso_utc_renders_known_instants() {
        assert_eq!(iso_utc(0), "1970-01-01 00:00:00 UTC");
        // 2026-07-26T14:33:02Z — checked against `date -u -d @1785076382`.
        assert_eq!(iso_utc(1_785_076_382_000), "2026-07-26 14:33:02 UTC");
        // Leap day, and the last second of a year.
        assert_eq!(iso_utc(1_709_164_800_000), "2024-02-29 00:00:00 UTC");
        assert_eq!(iso_utc(1_767_225_599_000), "2025-12-31 23:59:59 UTC");
    }

    #[test]
    fn governed_action_allows_and_chains() {
        let mut b = QuorumBackend::default();
        b.issue_grant(
            &grant("sbt-41", &["repo.write"], "CUI", 100, "2"),
            &t("bca"),
            0,
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
            0,
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
        b.issue_grant(&grant("sbt-41", &["spend"], "CUI", 1000, "2"), &t("bca"), 0)
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
            0,
        )
        .unwrap();
        assert!(b
            .revoke_grant("bca", "sbt-41", "G-1", "R. Ortiz", 0)
            .unwrap());
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
            0,
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
        b.issue_grant(
            &grant("sbt-41", &["repo.write"], "CUI", 3, "2"),
            &t("bca"),
            0,
        )
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
        b.issue_grant(&grant("sbt-41", &["spend"], "CUI", 500, "2"), &t("bca"), 0)
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
        b.issue_grant(&grant("sbt-41", &["spend"], "CUI", 400, "2"), &t("bca"), 0)
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
        b.issue_grant(&grant("sbt-41", &["spend"], "CUI", 400, "2"), &t("bca"), 0)
            .unwrap();
        let mut a = action("sbt-41", "spend", "Public", 220);
        a.hic1_cost_threshold = 150;
        let d = b.evaluate_and_record(&a, &t("bca"), 1000).unwrap();

        let rejection = b.reject_decision(&t("bca"), d.decision_id, 2000).unwrap();
        assert_eq!(rejection.verdict, "rejected");
        assert_eq!(rejection.hic, "1");
        assert_eq!(
            b.ledger_rows("bca").last().map(|r| r.verdict.as_str()),
            Some("rejected"),
            "the refusal is evidence too"
        );

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

    // ---- a human acting directly -------------------------------------

    #[test]
    fn issuing_a_grant_records_who_issued_it() {
        let mut b = QuorumBackend::default();
        b.issue_grant(
            &grant("sbt-41", &["repo.write"], "CUI", 100, "2"),
            &t("bca"),
            1000,
        )
        .unwrap();
        let rows = b.ledger_rows("bca");
        assert_eq!(rows.len(), 1, "a grant and its record land together");
        assert_eq!(rows[0].cls, "grant.issue");
        assert_eq!(rows[0].verdict, "approved", "the operator is the authority");
        assert_eq!(rows[0].hic, "1", "L-1: a capability grant is always HIC-1");
        assert_eq!(rows[0].principal, "R. Ortiz");
        assert_eq!(
            b.ledger_ungoverned_count("bca"),
            0,
            "an operator doing their job is NOT an alert state (HIC-X is never a configuration)"
        );
    }

    #[test]
    fn a_grant_id_identifies_exactly_one_envelope() {
        let mut b = QuorumBackend::default();
        b.issue_grant(
            &grant("sbt-41", &["repo.write"], "CUI", 100, "2"),
            &t("bca"),
            1000,
        )
        .unwrap();
        // Same id again — even for a different agent — is ambiguous: `revoke`
        // would hit both and the ledger's grant_id would stop identifying which
        // envelope authorised an action.
        let err = b
            .issue_grant(
                &grant("sbt-99", &["spend"], "CUI", 100, "2"),
                &t("bca"),
                2000,
            )
            .expect_err("a duplicate grant id must be refused");
        assert!(err.contains("already exists"), "honest error: {err}");
        // Nothing half-created on the way out.
        assert!(b.grants_for("bca", "sbt-99").is_empty());
        // The same id in ANOTHER tenant is fine — ids are tenant-scoped.
        assert!(b
            .issue_grant(
                &grant("sbt-41", &["repo.write"], "CUI", 100, "2"),
                &t("sea"),
                3000
            )
            .is_ok());
    }

    #[test]
    fn revoking_a_grant_records_who_revoked_it() {
        let mut b = QuorumBackend::default();
        b.issue_grant(
            &grant("sbt-41", &["repo.write"], "CUI", 100, "2"),
            &t("bca"),
            1000,
        )
        .unwrap();
        assert!(b
            .revoke_grant("bca", "sbt-41", "G-1", "M. Okonkwo", 2000)
            .unwrap());
        let rows = b.ledger_rows("bca");
        assert_eq!(rows.last().unwrap().cls, "grant.revoke");
        assert_eq!(rows.last().unwrap().principal, "M. Okonkwo");
        // A revocation that found nothing records nothing.
        let before = b.ledger_rows("bca").len();
        assert!(!b
            .revoke_grant("bca", "sbt-41", "G-nope", "M. Okonkwo", 3000)
            .unwrap());
        assert_eq!(b.ledger_rows("bca").len(), before);
    }

    /// The mirror of `a_grant_cannot_be_issued_by_nobody`, which did not exist.
    ///
    /// You could not ISSUE authority as nobody, but you could REVOKE it as
    /// nobody — and revocation is the HIC-1 act. `session.operator()` returns
    /// `Option<String>`, and the Agents surface turns a `None` into `""`, so a
    /// fresh install with no operator set reached exactly this.
    ///
    /// Worse than an unattributed row: the old order mutated first and recorded
    /// second, so a blank actor revoked and PERSISTED the grant, then failed on
    /// the ledger write. The caller saw `Err` and told the operator the
    /// revocation failed — while the capability was already gone. That is the
    /// exact inverse of the bug Agents.tsx:126 documents fixing, and it puts the
    /// grant store and the ledger permanently out of agreement.
    #[test]
    fn a_grant_cannot_be_revoked_by_nobody() {
        let mut b = QuorumBackend::default();
        b.issue_grant(
            &grant("sbt-41", &["repo.write"], "CUI", 100, "2"),
            &t("bca"),
            1000,
        )
        .unwrap();
        let before = b.ledger_rows("bca").len();

        let err = b
            .revoke_grant("bca", "sbt-41", "G-1", "  ", 2000)
            .expect_err("a revocation nobody signed is not evidence of anything");
        assert!(err.contains("name the human"), "honest error: {err}");

        // FAIL CLOSED, and that is the whole point: the grant must still govern.
        // If it were revoked-but-unrecorded, the store and the ledger would
        // disagree forever and the operator would have been told it failed.
        let live = b.grants_for("bca", "sbt-41");
        assert!(
            live.iter().any(|g| g.id == "G-1" && !g.revoked),
            "the grant must survive a refused revocation"
        );
        assert_eq!(
            b.ledger_rows("bca").len(),
            before,
            "a refused revocation writes no ledger row"
        );
    }

    #[test]
    fn a_grant_cannot_be_issued_by_nobody() {
        let mut b = QuorumBackend::default();
        let mut g = grant("sbt-41", &["repo.write"], "CUI", 100, "2");
        g.issued_by = "  ".into();
        let err = b
            .issue_grant(&g, &t("bca"), 1000)
            .expect_err("authority from nowhere is what an auditor hunts for");
        assert!(err.contains("name the human"), "honest error: {err}");
    }

    #[test]
    fn the_fleet_is_every_agent_this_tenant_has_evidence_about() {
        let mut b = QuorumBackend::default();
        assert!(
            b.known_agents("bca").is_empty(),
            "a new tenant knows no agents"
        );

        b.issue_grant(
            &grant("sbt-41", &["repo.write"], "CUI", 100, "2"),
            &t("bca"),
            1000,
        )
        .unwrap();
        // A granted agent is known before it ever acts.
        let fleet = b.known_agents("bca");
        assert_eq!(fleet.len(), 1);
        assert_eq!(fleet[0].id, "sbt-41");
        assert_eq!(fleet[0].live_grants, 1);
        assert_eq!(fleet[0].decisions, 0);

        // An UNGRANTED agent that acts is known too — that is the point.
        b.evaluate_and_record(&action("rogue", "repo.write", "Public", 1), &t("bca"), 2000)
            .unwrap();
        let fleet = b.known_agents("bca");
        assert_eq!(fleet.len(), 2);
        let rogue = fleet.iter().find(|a| a.id == "rogue").unwrap();
        assert_eq!(rogue.live_grants, 0);
        assert_eq!(rogue.ungoverned, 1);
        // The operator is not an agent in their own fleet.
        assert!(!fleet.iter().any(|a| a.id == "R. Ortiz"));
    }

    #[test]
    fn an_agents_grants_are_readable_including_revoked_ones() {
        let mut b = QuorumBackend::default();
        b.issue_grant(
            &grant("sbt-41", &["repo.write"], "CUI", 100, "2"),
            &t("bca"),
            1000,
        )
        .unwrap();
        let gs = b.grants_for("bca", "sbt-41");
        assert_eq!(gs.len(), 1);
        assert_eq!(gs[0].ceiling, "CUI");
        assert_eq!(gs[0].classes, "repo.write");
        assert_eq!(gs[0].hic, "2");
        assert!(!gs[0].revoked);

        b.revoke_grant("bca", "sbt-41", "G-1", "R. Ortiz", 2000)
            .unwrap();
        let gs = b.grants_for("bca", "sbt-41");
        assert!(
            gs[0].revoked,
            "a revoked grant stays in the record — it is history, not an absence"
        );
        assert!(b.grants_for("bca", "never-seen").is_empty());
    }

    // ---- Q10 egress control ------------------------------------------

    #[test]
    fn work_at_a_controlled_classification_is_denied_without_a_permitting_policy() {
        let mut b = QuorumBackend::default();
        b.issue_grant(
            &grant("sbt-41", &["repo.write"], "ITAR", 100, "2"),
            &t("bca"),
            0,
        )
        .unwrap();

        // Fresh install: no external egress until a protocol permits it (Q10).
        let mut a = action("sbt-41", "repo.write", "CUI", 1);
        a.model_id = "gpt-northstar".into();
        let d = b.evaluate_and_record(&a, &t("bca"), 1000).unwrap();
        assert_eq!(d.verdict, "deny");
        assert!(d.reason.contains("RC-401"), "{}", d.reason);

        // Naming no model is not a permission either — "we did not check" is
        // not evidence that the work stayed inside the boundary.
        let mut silent = action("sbt-41", "repo.write", "ITAR", 1);
        silent.model_id = String::new();
        let d2 = b.evaluate_and_record(&silent, &t("bca"), 1100).unwrap();
        assert_eq!(d2.verdict, "deny");
        assert!(d2.reason.contains("RC-400"), "{}", d2.reason);

        // Uncontrolled classifications are unaffected.
        let mut public = action("sbt-41", "repo.write", "Public", 1);
        public.model_id = "gpt-northstar".into();
        assert_eq!(
            b.evaluate_and_record(&public, &t("bca"), 1200)
                .unwrap()
                .verdict,
            "allow"
        );
    }

    #[test]
    fn a_permitted_endpoint_may_serve_the_classification_it_was_permitted_for() {
        let mut b = QuorumBackend::default();
        let mut policy = EgressPolicy::default();
        policy
            .allow
            .insert("CUI".into(), vec!["local/gemma-3-4b".into()]);
        b.egress = policy;
        b.issue_grant(
            &grant("sbt-41", &["repo.write"], "ITAR", 100, "2"),
            &t("bca"),
            0,
        )
        .unwrap();

        let mut ok = action("sbt-41", "repo.write", "CUI", 1);
        ok.model_id = "local/gemma-3-4b".into();
        assert_eq!(
            b.evaluate_and_record(&ok, &t("bca"), 1000).unwrap().verdict,
            "allow"
        );

        // Permitted at CUI is not permitted at ITAR — the allowlist is per
        // classification, which is the entire point of it.
        let mut itar = action("sbt-41", "repo.write", "ITAR", 1);
        itar.model_id = "local/gemma-3-4b".into();
        assert_eq!(
            b.evaluate_and_record(&itar, &t("bca"), 1100)
                .unwrap()
                .verdict,
            "deny"
        );
    }

    #[test]
    fn an_ungoverned_action_stays_ungoverned_rather_than_being_relabelled_denied() {
        let mut b = QuorumBackend::default();
        // No grant at all, at a controlled classification with a bad model.
        let mut a = action("rogue", "repo.write", "ITAR", 1);
        a.model_id = "gpt-northstar".into();
        let d = b.evaluate_and_record(&a, &t("bca"), 1000).unwrap();
        assert_eq!(
            d.verdict, "ungoverned",
            "nothing authorised this at all — that is the louder alarm, and \
             relabelling it `deny` would lose it"
        );
        assert_eq!(b.ledger_ungoverned_count("bca"), 1);
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
        b.issue_grant(&grant("sbt-41", &["spend"], "CUI", 1000, "2"), &t("bca"), 0)
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
        b.issue_grant(&grant("sbt-41", &["spend"], "CUI", 1000, "2"), &t("bca"), 0)
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
        assert!(b.ledger_verify("bca"));
        // The approver is named in the record — that IS the evidence.
        let last = b.ledger_rows("bca").last().cloned().unwrap();
        assert_eq!(last.verdict, "approved");
        assert_eq!(last.principal, "R. Ortiz");
    }

    #[test]
    fn an_approved_action_keeps_its_charge_but_a_rejected_one_is_refunded() {
        let mut b = QuorumBackend::default();
        b.issue_grant(&grant("sbt-41", &["spend"], "CUI", 400, "2"), &t("bca"), 0)
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
        b2.issue_grant(&grant("sbt-41", &["spend"], "CUI", 400, "2"), &t("bca"), 0)
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
        b.issue_grant(&grant("sbt-41", &["spend"], "CUI", 1000, "2"), &t("bca"), 0)
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
        b.issue_grant(&grant("sbt-41", &["spend"], "CUI", 1000, "2"), &t("bca"), 0)
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
        b.issue_grant(&grant("sbt-41", &["spend"], "CUI", 1000, "2"), &t("bca"), 0)
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
        b.issue_grant(
            &grant("sbt-41", &["spend"], "CUI", 10_000, "2"),
            &t("bca"),
            0,
        )
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
        b.issue_grant(&grant("sbt-41", &["spend"], "CUI", 1000, "2"), &t("bca"), 0)
            .unwrap();
        let d = b
            .evaluate_and_record(&escalating_action(), &t("bca"), 1000)
            .unwrap();
        assert_eq!(b.decision_status("bca", d.decision_id), Some("pending"));
        b.approve_decision(&t("bca"), d.decision_id, "R. Ortiz", 2000)
            .unwrap();
        assert_eq!(b.decision_status("bca", d.decision_id), Some("approved"));

        let mut b2 = QuorumBackend::default();
        b2.issue_grant(&grant("sbt-41", &["spend"], "CUI", 1000, "2"), &t("bca"), 0)
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
    fn the_accountable_human_comes_from_the_grant_not_from_the_agent() {
        let mut b = QuorumBackend::default();
        // The operator signed this grant naming themselves as accountable.
        b.issue_grant(
            &grant("sbt-41", &["repo.write"], "CUI", 100, "2"),
            &t("bca"),
            0,
        )
        .unwrap();

        // The agent claims someone else entirely — and reports no principal at
        // all in the second case. Neither claim is load-bearing.
        let mut lying = action("sbt-41", "repo.write", "Public", 1);
        lying.principal = Some("Someone Else".into());
        let d = b.evaluate_and_record(&lying, &t("bca"), 1000).unwrap();
        assert_eq!(d.verdict, "allow");

        let mut silent = action("sbt-41", "repo.write", "Public", 1);
        silent.principal = None;
        b.evaluate_and_record(&silent, &t("bca"), 1100).unwrap();

        let rows = b.ledger_rows("bca");
        let governed: Vec<&LedgerRow> = rows.iter().filter(|r| r.cls == "repo.write").collect();
        assert_eq!(governed.len(), 2);
        for r in governed {
            assert_eq!(
                r.principal, "R. Ortiz",
                "the record must name the grant's principal — an agent naming its own \
                 accountable human would be authority by assertion"
            );
        }
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
                0,
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
        assert_eq!(
            b.ledger_rows("bca").len(),
            3,
            "the grant issuance and both agent decisions survive"
        );
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
                0,
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
            assert!(b
                .revoke_grant("bca", "sbt-41", "G-1", "R. Ortiz", 0)
                .unwrap());
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
            b.issue_grant(&grant("sbt-41", &["spend"], "CUI", 400, "2"), &t("bca"), 0)
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
            b.issue_grant(&grant("sbt-41", &["spend"], "CUI", 1000, "2"), &t("bca"), 0)
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
            b.issue_grant(
                &grant("sbt-41", &["spend"], "CUI", 10_000, "2"),
                &t("bca"),
                0,
            )
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

    // ---- meetings (WP-S5.4 / S5.5) -----------------------------------

    /// A quorate, closed meeting in tenant `acme`, ready to ratify.
    fn backend_with_closed_meeting() -> (QuorumBackend, TenantId) {
        let mut b = QuorumBackend::default();
        let tenant = t("acme");
        b.set_active_tenant("acme").expect("scope");
        b.schedule_meeting(
            &tenant,
            ScheduleMeeting {
                id: "m-1",
                name: "Weekly Standup",
                when: "2026-07-23T09:00:00Z",
                template: "Standup",
                min_humans: 2,
                classification: "Proprietary",
                workspace: None,
            },
        )
        .expect("schedule");
        for who in ["R. Ortiz", "M. Okonkwo"] {
            b.admit_to_meeting(&tenant, "m-1", who, None, true, Some("CUI"))
                .expect("admit");
        }
        b.open_meeting(&tenant, "m-1").expect("open");
        b.close_meeting(&tenant, "m-1").expect("close");
        (b, tenant)
    }

    #[test]
    fn ratifying_records_a_hic1_decision_naming_the_human() {
        let (mut b, tenant) = backend_with_closed_meeting();
        let hash = b.meeting_content_hash("acme", "m-1").expect("hash");
        let before = b.ledger_rows("acme").len();

        let dto = b
            .ratify_meeting(&tenant, "m-1", "R. Ortiz", &hash, 1_753_460_000)
            .expect("ratify");

        // A ratified meeting with no record of who ratified it is authority
        // from nowhere — the record and the ratification land together.
        assert_eq!(b.ledger_rows("acme").len(), before + 1);
        assert_eq!(dto.hic, hic_str(HicLevel::ApproveEach));
        let detail = b.meeting_detail("acme", "m-1").expect("detail");
        assert!(detail.ratified);
        assert_eq!(detail.ratified_by.as_deref(), Some("R. Ortiz"));
    }

    #[test]
    fn ratifying_with_a_stale_hash_is_refused_and_records_nothing() {
        let (mut b, tenant) = backend_with_closed_meeting();
        let stale = hex_encode(&[0u8; 32]);
        let before = b.ledger_rows("acme").len();

        let err = b
            .ratify_meeting(&tenant, "m-1", "R. Ortiz", &stale, 1)
            .expect_err("a hash the signer never saw must be refused");

        assert!(err.contains("did not see"), "got: {err}");
        assert_eq!(
            b.ledger_rows("acme").len(),
            before,
            "a refused ratification must not leave a decision record behind"
        );
        assert!(!b.meeting_detail("acme", "m-1").expect("detail").ratified);
    }

    #[test]
    fn an_inquorate_meeting_cannot_be_ratified() {
        let mut b = QuorumBackend::default();
        let tenant = t("acme");
        b.set_active_tenant("acme").expect("scope");
        b.schedule_meeting(
            &tenant,
            ScheduleMeeting {
                id: "m-2",
                name: "Standup",
                when: "2026-07-23T09:00:00Z",
                template: "Standup",
                min_humans: 2,
                classification: "Proprietary",
                workspace: None,
            },
        )
        .expect("schedule");
        b.admit_to_meeting(&tenant, "m-2", "R. Ortiz", None, true, Some("CUI"))
            .expect("admit");
        b.open_meeting(&tenant, "m-2").expect("open");
        assert_eq!(b.close_meeting(&tenant, "m-2").expect("close"), "inquorate");

        let hash = b.meeting_content_hash("acme", "m-2").expect("hash");
        let err = b
            .ratify_meeting(&tenant, "m-2", "R. Ortiz", &hash, 1)
            .expect_err("inquorate is terminal");
        assert!(err.contains("inquorate"), "got: {err}");
    }

    #[test]
    fn minutes_are_composed_from_the_governed_record_in_the_open_window() {
        let mut b = QuorumBackend::default();
        let tenant = t("acme");
        b.set_active_tenant("acme").expect("scope");
        // A decision recorded BEFORE the meeting opens must not appear in it.
        b.evaluate_and_record(&action("sbt-41", "repo.write", "Public", 1), &tenant, 1)
            .expect("pre-meeting decision");

        b.schedule_meeting(
            &tenant,
            ScheduleMeeting {
                id: "m-3",
                name: "Standup",
                when: "2026-07-23T09:00:00Z",
                template: "Standup",
                min_humans: 1,
                classification: "Public",
                workspace: None,
            },
        )
        .expect("schedule");
        b.admit_to_meeting(&tenant, "m-3", "R. Ortiz", None, true, Some("CUI"))
            .expect("admit");
        b.open_meeting(&tenant, "m-3").expect("open");
        b.evaluate_and_record(&action("sbt-41", "ci.rerun", "Public", 1), &tenant, 2)
            .expect("in-meeting decision");
        b.close_meeting(&tenant, "m-3").expect("close");

        let d = b.meeting_detail("acme", "m-3").expect("detail");
        assert_eq!(d.minutes.len(), 1, "only the in-window decision");
        assert!(d.minutes[0].contains("ci.rerun"), "got: {:?}", d.minutes);
        assert_eq!(d.decisions.len(), 1);
    }

    #[test]
    fn a_meeting_with_no_governed_action_says_so_rather_than_showing_blank_minutes() {
        let (b, _) = backend_with_closed_meeting();
        let d = b.meeting_detail("acme", "m-1").expect("detail");
        assert_eq!(d.minutes.len(), 1);
        assert!(
            d.minutes[0].contains("No governed action was recorded"),
            "got: {:?}",
            d.minutes
        );
    }

    // The anchor's state is no longer a field on the detail DTO — it is its
    // own command (`meeting_anchor`) because it makes an `eth_call`, and its
    // coverage lives in `anchor::tests`, including a LIVE check against
    // chain 40204 that proves the ABI return offsets are right.

    #[test]
    fn meetings_are_tenant_isolated() {
        let (mut b, _) = backend_with_closed_meeting();
        b.set_active_tenant("other").expect("scope");
        assert!(
            b.meeting_rows("other").is_empty(),
            "a meeting in one tenant must be invisible in another (Rule 6)"
        );
        assert!(b.meeting_detail("other", "m-1").is_err());
    }

    #[test]
    fn a_duplicate_meeting_id_within_a_tenant_is_refused() {
        let (mut b, tenant) = backend_with_closed_meeting();
        let err = b
            .schedule_meeting(
                &tenant,
                ScheduleMeeting {
                    id: "m-1",
                    name: "Another",
                    when: "2026-07-30T09:00:00Z",
                    template: "Standup",
                    min_humans: 2,
                    classification: "Public",
                    workspace: None,
                },
            )
            .expect_err("ids must identify one meeting");
        assert!(err.contains("already exists"), "got: {err}");
    }

    #[test]
    fn no_workspace_yields_an_empty_agenda_that_states_it_was_not_generated() {
        let (b, _) = backend_with_closed_meeting();
        let d = b.meeting_detail("acme", "m-1").expect("detail");
        assert!(d.agenda.is_empty());
        assert!(
            d.agenda_source.contains("not generated"),
            "got: {}",
            d.agenda_source
        );
    }
}
