//! citrate-quorum — the durable evidence store (WP-S4 persistence).
//!
//! The governed-action loop's output is the product's deliverable: audit
//! evidence. Holding it only in RAM meant a restart erased the record of what
//! agents had done. This module puts it on disk, crash-atomically, in a form
//! that **re-proves itself on load**.
//!
//! ## Layout
//!
//! ```text
//! <app_data>/evidence/
//!   scope.json                     the active tenant scope
//!   tenants/<blake3(tenant)>/
//!     tenant.json                  the tenant id this directory belongs to
//!     chain.jsonl                  append-only decision records (the evidence)
//!     grants.json                  live capability grants
//!     allowances.json              live vote allowances
//! ```
//!
//! Directory names are `blake3(tenant_id)` hex, never the tenant id itself:
//! [`TenantId`] only rejects empty/whitespace, so a raw id could contain `/` or
//! `..` and escape the store. The real id is recorded inside `tenant.json`.
//!
//! ## Integrity, and its honest limit
//!
//! `chain.jsonl` stores each record together with the chained hash it produced.
//! Loading replays every record through a fresh [`HashChain`] and checks each
//! recomputed hash against the stored one, so an edited, reordered, or deleted
//! record fails the load loudly instead of being served as evidence.
//!
//! What this does NOT defend against: someone who rewrites the whole file
//! consistently, or truncates its tail, since the file attests only to itself.
//! Only the on-chain Merkle anchor (QRM-S6) closes that, and until it lands the
//! at-rest guarantee is "detects corruption and naive tampering", not
//! "tamper-proof". Saying otherwise would be the kind of claim this product
//! exists to make unnecessary.
//!
//! ## Durability
//!
//! Records append with an `O_APPEND` write + `fsync`. Mutable sets (grants,
//! allowances, scope) are rewritten whole via temp-file + `fsync` + `rename`,
//! then the directory is `fsync`ed so the rename itself survives a crash.

use std::fs::{self, File, OpenOptions};
use std::io::{self, BufRead, BufReader, Write};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use quorum_audit::{DecisionRecord, HashChain, HicLevel, Verdict};
use quorum_meetings::{Attendee, Dissent, Meeting, MeetingState, MinuteDecision, Template};
use quorum_policy::{CapabilityGrant, GrantHic, VoteAllowance};
use quorum_tenancy::{Classification, TenantId};

// ---- the string codec ------------------------------------------------
//
// One definition per mapping, used BOTH on the Tauri wire and at rest. A stored
// record and the ledger row rendered from it therefore agree by construction.

pub fn verdict_str(v: Verdict) -> &'static str {
    match v {
        Verdict::Allow => "allow",
        Verdict::RequireApproval => "require-approval",
        Verdict::Deny => "deny",
        Verdict::Ungoverned => "ungoverned",
        Verdict::Rejected => "rejected",
        Verdict::Approved => "approved",
    }
}
pub fn verdict_from_str(s: &str) -> Option<Verdict> {
    match s {
        "allow" => Some(Verdict::Allow),
        "require-approval" => Some(Verdict::RequireApproval),
        "deny" => Some(Verdict::Deny),
        "ungoverned" => Some(Verdict::Ungoverned),
        "rejected" => Some(Verdict::Rejected),
        "approved" => Some(Verdict::Approved),
        _ => None,
    }
}
pub fn hic_str(h: HicLevel) -> &'static str {
    match h {
        HicLevel::Observed => "0",
        HicLevel::ApproveEach => "1",
        HicLevel::Budgeted => "2",
        HicLevel::PostHoc => "3",
        HicLevel::Ungoverned => "X",
    }
}
pub fn hic_from_str(s: &str) -> Option<HicLevel> {
    match s {
        "0" => Some(HicLevel::Observed),
        "1" => Some(HicLevel::ApproveEach),
        "2" => Some(HicLevel::Budgeted),
        "3" => Some(HicLevel::PostHoc),
        "X" => Some(HicLevel::Ungoverned),
        _ => None,
    }
}
pub fn classification_from_str(s: &str) -> Option<Classification> {
    match s.trim().to_ascii_lowercase().as_str() {
        "public" => Some(Classification::Public),
        "proprietary" => Some(Classification::Proprietary),
        "cui" => Some(Classification::Cui),
        "itar" => Some(Classification::Itar),
        _ => None,
    }
}
pub fn classification_str(c: Classification) -> &'static str {
    match c {
        Classification::Public => "Public",
        Classification::Proprietary => "Proprietary",
        Classification::Cui => "CUI",
        Classification::Itar => "ITAR",
    }
}
pub fn grant_hic_from_str(s: &str) -> GrantHic {
    match s {
        "1" | "approve-each" => GrantHic::ApproveEach,
        "3" | "post-hoc" => GrantHic::PostHoc,
        _ => GrantHic::Budgeted, // default HIC-2
    }
}
pub fn grant_hic_str(h: GrantHic) -> &'static str {
    match h {
        GrantHic::ApproveEach => "1",
        GrantHic::Budgeted => "2",
        GrantHic::PostHoc => "3",
    }
}

pub fn hex_encode(bytes: &[u8]) -> String {
    let mut s = String::from("0x");
    for b in bytes {
        s.push_str(&format!("{b:02x}"));
    }
    s
}
pub fn hex32(s: &str) -> [u8; 32] {
    let mut out = [0u8; 32];
    let s = s.strip_prefix("0x").unwrap_or(s);
    let mut i = 0;
    let mut n = 0;
    while i + 1 < s.len() + 1 && n < 32 {
        let Some(pair) = s.get(i..i + 2) else { break };
        let Ok(b) = u8::from_str_radix(pair, 16) else {
            break;
        };
        out[n] = b;
        n += 1;
        i += 2;
    }
    out
}

// ---- errors ----------------------------------------------------------

#[derive(Debug)]
pub enum StoreError {
    /// The filesystem said no. Evidence that cannot be written is not evidence.
    Io(String),
    /// The stored chain did not replay to the hashes it claims. Naming the line
    /// makes it actionable instead of a shrug.
    Corrupt { line: usize, detail: String },
}

impl std::fmt::Display for StoreError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(e) => write!(f, "evidence store I/O error: {e}"),
            Self::Corrupt { line, detail } => write!(
                f,
                "evidence chain failed to verify at record {line}: {detail} — \
                 refusing to serve this tenant's ledger"
            ),
        }
    }
}
impl From<io::Error> for StoreError {
    fn from(e: io::Error) -> Self {
        Self::Io(e.to_string())
    }
}

// ---- on-disk shapes --------------------------------------------------

#[derive(Serialize, Deserialize)]
struct StoredRecord {
    agent: String,
    principal: Option<String>,
    grant_id: Option<String>,
    action_class: String,
    params_hash: String,
    verdict: String,
    hic: String,
    model_id: String,
    correlation_id: String,
    timestamp_ms: i64,
    /// The chained hash this record produced. Replay must reproduce it exactly.
    chain_hash: String,
}

#[derive(Serialize, Deserialize)]
struct StoredGrant {
    id: String,
    agent: String,
    principal: String,
    tenant_scope: String,
    action_classes: Vec<String>,
    classification_ceiling: String,
    budget_units: u64,
    consumed: u64,
    expires_at_ms: i64,
    hic: String,
    revoked: bool,
}

/// A meeting on disk (WP-S5.3).
///
/// The agenda is stored as its items **and** its frozen hash rather than the
/// hash alone. Keeping only the hash would make a ratified meeting
/// unverifiable offline: you could confirm nothing, because the document the
/// signature commits to would be gone. Keeping only the items would let the
/// hash be recomputed — and therefore silently repaired — after an edit.
/// Storing both means a tampered agenda fails to match on load.
#[derive(Serialize, Deserialize)]
struct StoredMeeting {
    id: String,
    name: String,
    when: String,
    tenant: String,
    template: String,
    min_humans: usize,
    classification: String,
    state: String,
    agenda: Vec<StoredAgendaItem>,
    agenda_skipped: usize,
    /// Hex, or absent while the meeting is still `scheduled`.
    agenda_hash: Option<String>,
    agenda_source: String,
    attendance: Vec<StoredAttendee>,
    minutes: Vec<String>,
    decisions: Vec<StoredMinuteDecision>,
    dissent: Vec<StoredDissent>,
    ratified_by: Option<String>,
    ratified_at: Option<u64>,
    /// The chain length when the meeting opened — the lower bound of the
    /// window its minutes are composed from.
    opened_at_index: Option<u64>,
}

#[derive(Serialize, Deserialize)]
struct StoredAgendaItem {
    n: u32,
    text: String,
    src: String,
}

#[derive(Serialize, Deserialize)]
struct StoredAttendee {
    name: String,
    attested: bool,
    agent: Option<String>,
    note: Option<String>,
    clearance: Option<String>,
}

#[derive(Serialize, Deserialize)]
struct StoredMinuteDecision {
    id: String,
    text: String,
    link: bool,
}

#[derive(Serialize, Deserialize)]
struct StoredDissent {
    who: String,
    text: String,
}

#[derive(Serialize, Deserialize)]
struct StoredAllowance {
    id: String,
    principal: String,
    agent: String,
    tenant_scope: String,
    proposal_classes: Vec<String>,
    weight_cap: u64,
    spent: u64,
    expires_at_ms: i64,
    revoked: bool,
}

#[derive(Serialize, Deserialize, Default)]
struct StoredScope {
    active_tenant: Option<String>,
    /// The human operating this installation. Every approval is recorded
    /// against them, so without it nothing can be approved at all.
    operator: Option<String>,
}

/// What one decision took from its grant's budget. Kept so a rejection refunds
/// exactly what was charged, exactly once — never a free "give me budget" call.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct Charge {
    /// Index of the decision in the tenant's chain.
    pub decision: u64,
    pub agent: String,
    pub grant_id: String,
    pub units: u64,
}

// ---- the store -------------------------------------------------------

/// An escalation waiting on a human: a decision that came back
/// `require-approval` and has not yet been approved or rejected.
///
/// Persisted, because an agent that stopped and asked must still be waiting
/// after a restart — the alternative is an escalation that quietly evaporates,
/// which is the failure mode the whole HIC model exists to prevent.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct PendingApproval {
    /// Index of the escalated decision in the tenant's chain.
    pub decision: u64,
    pub agent: String,
    pub principal: Option<String>,
    pub action_class: String,
    pub classification: String,
    pub cost: u64,
    pub correlation_id: String,
    pub requested_at_ms: i64,
}

/// Which model endpoints may serve which classification (Q10 / `EgressPolicy`).
///
/// Q10 makes egress a governance protocol the customer deploys and amends, not
/// a config toggle, and fixes the fresh-install posture: **no external egress
/// until a protocol permits it**. The protocol-backed source lands with the
/// governance contracts (QRM-S6/S7); until then this file is its stand-in, and
/// it is named that way rather than pretending to be the final mechanism.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct EgressPolicy {
    /// Classifications egress is controlled at. Defaults to the
    /// export-controlled pair — the ones Q13 put in scope.
    pub controlled: Vec<String>,
    /// Permitted model endpoints, by classification. Absent or empty means
    /// nothing is permitted at that classification.
    pub allow: std::collections::BTreeMap<String, Vec<String>>,
}

impl Default for EgressPolicy {
    fn default() -> Self {
        Self {
            controlled: vec!["CUI".to_string(), "ITAR".to_string()],
            allow: std::collections::BTreeMap::new(),
        }
    }
}

impl EgressPolicy {
    /// Is `model` permitted to serve work at `classification`?
    ///
    /// An unnamed model is NOT permitted at a controlled classification: if we
    /// cannot say which endpoint backs the agent, we cannot assert the work
    /// stayed inside the boundary, and "we did not check" is not a permission.
    pub fn permits(&self, classification: &str, model: &str) -> bool {
        if !self.controlled.iter().any(|c| c == classification) {
            return true;
        }
        if model.trim().is_empty() {
            return false;
        }
        self.allow
            .get(classification)
            .is_some_and(|models| models.iter().any(|m| m == model))
    }
}

/// The ONE place the evidence store lives, given Tauri's app-data directory.
///
/// Every caller must route through this. It exists because they did not, and
/// the way that failed is the point: `lib.rs` opened the store at
/// `<app_data>/evidence` while the three QRM-S7 pipeline commands opened
/// `<app_data>`. Both succeeded — `EvidenceStore::open` creates what is missing —
/// so there was no error anywhere. The pipeline simply read an empty store,
/// found no tenant, and reported **"no tenant scope is established"** on a
/// machine whose `scope.json` was sitting one directory away.
///
/// Unit tests cannot catch that class: they build stores at explicit temp paths,
/// so the two spellings never have to agree. Only running the packaged app
/// against a real install does, which is where it was found. The source-scan
/// test below now makes the agreement structural.
pub fn evidence_dir(app_data: &std::path::Path) -> PathBuf {
    app_data.join("evidence")
}

/// A durable, crash-atomic home for one installation's evidence.
#[derive(Debug, Clone)]
pub struct EvidenceStore {
    root: PathBuf,
}

impl EvidenceStore {
    /// Open (creating if needed) the store rooted at `root`.
    pub fn open(root: PathBuf) -> Result<Self, StoreError> {
        fs::create_dir_all(&root)?;
        Ok(Self { root })
    }

    /// The per-tenant directory: `blake3(tenant)` hex, never the raw id (which
    /// may legally contain path separators).
    pub(crate) fn tenant_dir(&self, tenant: &str) -> PathBuf {
        let digest = blake3::hash(tenant.as_bytes());
        self.root.join("tenants").join(digest.to_hex().as_str())
    }

    fn ensure_tenant_dir(&self, tenant: &str) -> Result<PathBuf, StoreError> {
        let dir = self.tenant_dir(tenant);
        fs::create_dir_all(&dir)?;
        // Record which tenant this hashed directory belongs to, so the store is
        // readable by a human (and by a future migration) without the id table.
        let marker = dir.join("tenant.json");
        if !marker.exists() {
            write_atomic(
                &marker,
                serde_json::json!({ "tenant": tenant })
                    .to_string()
                    .as_bytes(),
            )?;
        }
        Ok(dir)
    }

    // ---- the active scope --------------------------------------------

    fn read_scope(&self) -> StoredScope {
        fs::read_to_string(self.root.join("scope.json"))
            .ok()
            .and_then(|raw| serde_json::from_str::<StoredScope>(&raw).ok())
            .unwrap_or_default()
    }

    pub fn load_scope(&self) -> Option<String> {
        self.read_scope().active_tenant
    }

    pub fn load_operator(&self) -> Option<String> {
        self.read_scope().operator
    }

    /// Each writer preserves the other field — the scope and the operator live
    /// in one file but are set at different moments.
    pub fn save_scope(&self, tenant: Option<&str>) -> Result<(), StoreError> {
        let body = serde_json::to_vec(&StoredScope {
            active_tenant: tenant.map(str::to_string),
            operator: self.read_scope().operator,
        })
        .map_err(|e| StoreError::Io(e.to_string()))?;
        write_atomic(&self.root.join("scope.json"), &body)
    }

    pub fn save_operator(&self, operator: Option<&str>) -> Result<(), StoreError> {
        let body = serde_json::to_vec(&StoredScope {
            active_tenant: self.read_scope().active_tenant,
            operator: operator.map(str::to_string),
        })
        .map_err(|e| StoreError::Io(e.to_string()))?;
        write_atomic(&self.root.join("scope.json"), &body)
    }

    // ---- the evidence chain ------------------------------------------

    /// Append one decision to the tenant's evidence log, durably.
    pub fn append_record(
        &self,
        tenant: &str,
        record: &DecisionRecord,
        chain_hash: [u8; 32],
    ) -> Result<(), StoreError> {
        let dir = self.ensure_tenant_dir(tenant)?;
        let stored = StoredRecord {
            agent: record.agent.clone(),
            principal: record.principal.clone(),
            grant_id: record.grant_id.clone(),
            action_class: record.action_class.clone(),
            params_hash: hex_encode(&record.params_hash),
            verdict: verdict_str(record.verdict).to_string(),
            hic: hic_str(record.hic).to_string(),
            model_id: record.model_id.clone(),
            correlation_id: record.correlation_id.clone(),
            timestamp_ms: record.timestamp_ms,
            chain_hash: hex_encode(&chain_hash),
        };
        let line = serde_json::to_string(&stored).map_err(|e| StoreError::Io(e.to_string()))?;
        let mut f = OpenOptions::new()
            .create(true)
            .append(true)
            .open(dir.join("chain.jsonl"))?;
        f.write_all(line.as_bytes())?;
        f.write_all(b"\n")?;
        f.sync_all()?;
        Ok(())
    }

    /// Rebuild a tenant's chain from disk, re-proving every link on the way.
    /// An empty or absent log yields a fresh (genesis-only) chain — that is a
    /// tenant with no evidence, which is different from a broken one.
    pub fn load_chain(&self, tenant: &TenantId) -> Result<HashChain, StoreError> {
        let mut chain = HashChain::new(tenant.clone());
        let path = self.tenant_dir(tenant.as_str()).join("chain.jsonl");
        let Ok(file) = File::open(&path) else {
            return Ok(chain); // no log yet
        };
        for (i, line) in BufReader::new(file).lines().enumerate() {
            let line = line?;
            if line.trim().is_empty() {
                continue;
            }
            let stored: StoredRecord =
                serde_json::from_str(&line).map_err(|e| StoreError::Corrupt {
                    line: i + 1,
                    detail: format!("record does not parse ({e})"),
                })?;
            let verdict = verdict_from_str(&stored.verdict).ok_or_else(|| StoreError::Corrupt {
                line: i + 1,
                detail: format!("unknown verdict {:?}", stored.verdict),
            })?;
            let hic = hic_from_str(&stored.hic).ok_or_else(|| StoreError::Corrupt {
                line: i + 1,
                detail: format!("unknown HIC level {:?}", stored.hic),
            })?;
            let record = DecisionRecord {
                agent: stored.agent,
                principal: stored.principal,
                grant_id: stored.grant_id,
                action_class: stored.action_class,
                params_hash: hex32(&stored.params_hash),
                verdict,
                hic,
                model_id: stored.model_id,
                correlation_id: stored.correlation_id,
                timestamp_ms: stored.timestamp_ms,
            };
            let replayed = chain.append(record).map_err(|e| StoreError::Corrupt {
                line: i + 1,
                detail: format!("record rejected by the chain ({e:?})"),
            })?;
            if hex_encode(&replayed) != stored.chain_hash {
                return Err(StoreError::Corrupt {
                    line: i + 1,
                    detail: format!(
                        "chain hash mismatch — stored {}, replayed {}",
                        stored.chain_hash,
                        hex_encode(&replayed)
                    ),
                });
            }
        }
        Ok(chain)
    }

    // ---- grants + allowances (mutable sets, rewritten whole) ----------

    pub fn save_grants(&self, tenant: &str, grants: &[CapabilityGrant]) -> Result<(), StoreError> {
        let dir = self.ensure_tenant_dir(tenant)?;
        let stored: Vec<StoredGrant> = grants
            .iter()
            .map(|g| StoredGrant {
                id: g.id.clone(),
                agent: g.agent.clone(),
                principal: g.principal.clone(),
                tenant_scope: g.tenant_scope.clone(),
                action_classes: g.action_classes.clone(),
                classification_ceiling: classification_str(g.classification_ceiling).to_string(),
                budget_units: g.budget_units,
                consumed: g.consumed,
                expires_at_ms: g.expires_at_ms,
                hic: grant_hic_str(g.hic).to_string(),
                revoked: g.revoked,
            })
            .collect();
        let body = serde_json::to_vec(&stored).map_err(|e| StoreError::Io(e.to_string()))?;
        write_atomic(&dir.join("grants.json"), &body)
    }

    pub fn load_grants(&self, tenant: &str) -> Result<Vec<CapabilityGrant>, StoreError> {
        let path = self.tenant_dir(tenant).join("grants.json");
        let Ok(raw) = fs::read_to_string(&path) else {
            return Ok(Vec::new());
        };
        let stored: Vec<StoredGrant> =
            serde_json::from_str(&raw).map_err(|e| StoreError::Corrupt {
                line: 0,
                detail: format!("grants.json does not parse ({e})"),
            })?;
        stored
            .into_iter()
            .map(|g| {
                let ceiling =
                    classification_from_str(&g.classification_ceiling).ok_or_else(|| {
                        StoreError::Corrupt {
                            line: 0,
                            detail: format!("grant {} has unknown classification", g.id),
                        }
                    })?;
                Ok(CapabilityGrant {
                    id: g.id,
                    agent: g.agent,
                    principal: g.principal,
                    tenant_scope: g.tenant_scope,
                    action_classes: g.action_classes,
                    classification_ceiling: ceiling,
                    budget_units: g.budget_units,
                    consumed: g.consumed,
                    expires_at_ms: g.expires_at_ms,
                    hic: grant_hic_from_str(&g.hic),
                    revoked: g.revoked,
                })
            })
            .collect()
    }

    pub fn save_allowances(
        &self,
        tenant: &str,
        allowances: &[VoteAllowance],
    ) -> Result<(), StoreError> {
        let dir = self.ensure_tenant_dir(tenant)?;
        let stored: Vec<StoredAllowance> = allowances
            .iter()
            .map(|a| StoredAllowance {
                id: a.id.clone(),
                principal: a.principal.clone(),
                agent: a.agent.clone(),
                tenant_scope: a.tenant_scope.clone(),
                proposal_classes: a.proposal_classes.clone(),
                weight_cap: a.weight_cap,
                spent: a.spent,
                expires_at_ms: a.expires_at_ms,
                revoked: a.revoked,
            })
            .collect();
        let body = serde_json::to_vec(&stored).map_err(|e| StoreError::Io(e.to_string()))?;
        write_atomic(&dir.join("allowances.json"), &body)
    }

    // ---- outstanding charges ------------------------------------------

    pub fn save_charges(&self, tenant: &str, charges: &[Charge]) -> Result<(), StoreError> {
        let dir = self.ensure_tenant_dir(tenant)?;
        let body = serde_json::to_vec(charges).map_err(|e| StoreError::Io(e.to_string()))?;
        write_atomic(&dir.join("charges.json"), &body)
    }

    pub fn load_charges(&self, tenant: &str) -> Result<Vec<Charge>, StoreError> {
        let path = self.tenant_dir(tenant).join("charges.json");
        let Ok(raw) = fs::read_to_string(&path) else {
            return Ok(Vec::new());
        };
        serde_json::from_str(&raw).map_err(|e| StoreError::Corrupt {
            line: 0,
            detail: format!("charges.json does not parse ({e})"),
        })
    }

    // ---- escalations waiting on a human ------------------------------

    pub fn save_pending(
        &self,
        tenant: &str,
        pending: &[PendingApproval],
    ) -> Result<(), StoreError> {
        let dir = self.ensure_tenant_dir(tenant)?;
        let body = serde_json::to_vec(pending).map_err(|e| StoreError::Io(e.to_string()))?;
        write_atomic(&dir.join("pending.json"), &body)
    }

    pub fn load_pending(&self, tenant: &str) -> Result<Vec<PendingApproval>, StoreError> {
        let path = self.tenant_dir(tenant).join("pending.json");
        let Ok(raw) = fs::read_to_string(&path) else {
            return Ok(Vec::new());
        };
        serde_json::from_str(&raw).map_err(|e| StoreError::Corrupt {
            line: 0,
            detail: format!("pending.json does not parse ({e})"),
        })
    }

    // ---- answered escalations ----------------------------------------

    pub fn save_resolved(
        &self,
        tenant: &str,
        resolved: &[(u64, String)],
    ) -> Result<(), StoreError> {
        let dir = self.ensure_tenant_dir(tenant)?;
        let body = serde_json::to_vec(resolved).map_err(|e| StoreError::Io(e.to_string()))?;
        write_atomic(&dir.join("resolved.json"), &body)
    }

    pub fn load_resolved(&self, tenant: &str) -> Result<Vec<(u64, String)>, StoreError> {
        let path = self.tenant_dir(tenant).join("resolved.json");
        let Ok(raw) = fs::read_to_string(&path) else {
            return Ok(Vec::new());
        };
        serde_json::from_str(&raw).map_err(|e| StoreError::Corrupt {
            line: 0,
            detail: format!("resolved.json does not parse ({e})"),
        })
    }

    // ---- egress policy (Q10) ------------------------------------------

    /// The deployed egress policy, or the fail-closed default when none is.
    pub fn load_egress(&self) -> EgressPolicy {
        fs::read_to_string(self.root.join("egress.json"))
            .ok()
            .and_then(|raw| serde_json::from_str(&raw).ok())
            .unwrap_or_default()
    }

    pub fn save_egress(&self, policy: &EgressPolicy) -> Result<(), StoreError> {
        let body = serde_json::to_vec(policy).map_err(|e| StoreError::Io(e.to_string()))?;
        write_atomic(&self.root.join("egress.json"), &body)
    }

    pub fn load_allowances(&self, tenant: &str) -> Result<Vec<VoteAllowance>, StoreError> {
        let path = self.tenant_dir(tenant).join("allowances.json");
        let Ok(raw) = fs::read_to_string(&path) else {
            return Ok(Vec::new());
        };
        let stored: Vec<StoredAllowance> =
            serde_json::from_str(&raw).map_err(|e| StoreError::Corrupt {
                line: 0,
                detail: format!("allowances.json does not parse ({e})"),
            })?;
        Ok(stored
            .into_iter()
            .map(|a| VoteAllowance {
                id: a.id,
                principal: a.principal,
                agent: a.agent,
                tenant_scope: a.tenant_scope,
                proposal_classes: a.proposal_classes,
                weight_cap: a.weight_cap,
                spent: a.spent,
                expires_at_ms: a.expires_at_ms,
                revoked: a.revoked,
            })
            .collect())
    }

    // ---- the workspace (WP-S5.7) -------------------------------------

    /// The directory this tenant's `.agentile` artifacts live in.
    ///
    /// A tenant-level fact, not a per-meeting one: the same workspace feeds
    /// every agenda AND every standup brief, so storing it on one meeting
    /// would leave briefs guessing which meeting to ask.
    pub fn save_workspace(&self, tenant: &str, workspace: &str) -> Result<(), StoreError> {
        let dir = self.ensure_tenant_dir(tenant)?;
        let body = serde_json::json!({ "workspace": workspace }).to_string();
        write_atomic(&dir.join("workspace.json"), body.as_bytes())
    }

    pub fn load_workspace(&self, tenant: &str) -> Option<String> {
        let raw = fs::read_to_string(self.tenant_dir(tenant).join("workspace.json")).ok()?;
        let v: serde_json::Value = serde_json::from_str(&raw).ok()?;
        v.get("workspace")?.as_str().map(str::to_string)
    }

    // ---- meetings (WP-S5.3) ------------------------------------------

    pub fn save_meetings(
        &self,
        tenant: &str,
        meetings: &[MeetingRecord],
    ) -> Result<(), StoreError> {
        let dir = self.ensure_tenant_dir(tenant)?;
        let stored: Vec<StoredMeeting> = meetings.iter().map(store_meeting).collect();
        let body = serde_json::to_vec(&stored).map_err(|e| StoreError::Io(e.to_string()))?;
        write_atomic(&dir.join("meetings.json"), &body)
    }

    pub fn load_meetings(&self, tenant: &str) -> Result<Vec<MeetingRecord>, StoreError> {
        let path = self.tenant_dir(tenant).join("meetings.json");
        let Ok(raw) = fs::read_to_string(&path) else {
            return Ok(Vec::new());
        };
        let stored: Vec<StoredMeeting> =
            serde_json::from_str(&raw).map_err(|e| StoreError::Corrupt {
                line: 0,
                detail: format!("meetings.json does not parse ({e})"),
            })?;
        stored.into_iter().map(restore_meeting).collect()
    }
}

/// A meeting plus the two facts the domain crate deliberately does not hold:
/// where its agenda came from, and the chain index it opened at.
#[derive(Clone, Debug)]
pub struct MeetingRecord {
    pub meeting: Meeting,
    /// The Rule 11 provenance line for the agenda.
    pub agenda_source: String,
    /// Chain length when the meeting opened — the lower bound of the window
    /// its minutes are composed from.
    pub opened_at_index: Option<u64>,
}

fn store_meeting(r: &MeetingRecord) -> StoredMeeting {
    let m = &r.meeting;
    StoredMeeting {
        id: m.id.clone(),
        name: m.name.clone(),
        when: m.when.clone(),
        tenant: m.tenant.clone(),
        template: m.template.name.clone(),
        min_humans: m.template.min_humans,
        classification: classification_str(m.classification).to_string(),
        state: m.state().as_str().to_string(),
        agenda: m
            .agenda
            .items()
            .iter()
            .map(|i| StoredAgendaItem {
                n: i.n,
                text: i.text.clone(),
                src: i.src.clone(),
            })
            .collect(),
        agenda_skipped: m.agenda.skipped,
        agenda_hash: m.agenda_hash().map(|h| hex_encode(&h)),
        agenda_source: r.agenda_source.clone(),
        attendance: m
            .attendance
            .iter()
            .map(|a| StoredAttendee {
                name: a.name.clone(),
                attested: a.attested,
                agent: a.agent.clone(),
                note: a.note.clone(),
                clearance: a.clearance.map(|c| classification_str(c).to_string()),
            })
            .collect(),
        minutes: m.minutes.clone(),
        decisions: m
            .decisions
            .iter()
            .map(|d| StoredMinuteDecision {
                id: d.id.clone(),
                text: d.text.clone(),
                link: d.link,
            })
            .collect(),
        dissent: m
            .dissent
            .iter()
            .map(|d| StoredDissent {
                who: d.who.clone(),
                text: d.text.clone(),
            })
            .collect(),
        ratified_by: m.ratified_by.clone(),
        ratified_at: m.ratified_at,
        opened_at_index: r.opened_at_index,
    }
}

fn restore_meeting(s: StoredMeeting) -> Result<MeetingRecord, StoreError> {
    let classification =
        classification_from_str(&s.classification).ok_or_else(|| StoreError::Corrupt {
            line: 0,
            detail: format!("meeting {} has unknown classification", s.id),
        })?;
    let state = MeetingState::from_wire(&s.state).ok_or_else(|| StoreError::Corrupt {
        line: 0,
        detail: format!("meeting {} has unknown state {}", s.id, s.state),
    })?;

    let mut m = Meeting::schedule(
        s.id.clone(),
        s.name,
        s.when,
        s.tenant,
        Template::new(s.template, s.min_humans),
        classification,
    );

    // Rebuild the agenda BEFORE restoring the state, because `add_agenda_item`
    // refuses once a hash is present — the freeze is real even on reload.
    for item in &s.agenda {
        m.add_agenda_item(item.text.clone(), item.src.clone())
            .map_err(|e| StoreError::Corrupt {
                line: 0,
                detail: format!("meeting {}: {e}", s.id),
            })?;
    }
    m.agenda.skipped = s.agenda_skipped;

    let agenda_hash = s.agenda_hash.as_ref().map(|hex| hex32(hex));

    // A stored agenda whose items no longer hash to the stored frozen value has
    // been edited on disk. Refusing to load it is the point: a ratified meeting
    // whose agenda can be swapped underneath the signature is not evidence.
    if let Some(expected) = agenda_hash {
        if m.agenda.hash() != expected {
            return Err(StoreError::Corrupt {
                line: 0,
                detail: format!(
                    "meeting {}: the stored agenda does not match its frozen hash — \
                     it was edited after the meeting opened",
                    s.id
                ),
            });
        }
    }

    for a in s.attendance {
        let clearance = match &a.clearance {
            Some(c) => Some(
                classification_from_str(c).ok_or_else(|| StoreError::Corrupt {
                    line: 0,
                    detail: format!(
                        "meeting {}: attendee {} has unknown clearance",
                        s.id, a.name
                    ),
                })?,
            ),
            None => None,
        };
        let mut att = if let Some(vendor) = a.agent {
            Attendee::agent(a.name, vendor)
        } else {
            Attendee::human(a.name)
        };
        att.attested = a.attested;
        att.note = a.note;
        att.clearance = clearance;
        // Pushed directly rather than via `admit`: reloading is not admission,
        // and re-running MR-4 here would reject a meeting that was legitimately
        // held before someone's clearance changed.
        m.attendance.push(att);
    }

    m.minutes = s.minutes;
    m.decisions = s
        .decisions
        .into_iter()
        .map(|d| MinuteDecision {
            id: d.id,
            text: d.text,
            link: d.link,
        })
        .collect();
    m.dissent = s
        .dissent
        .into_iter()
        .map(|d| Dissent {
            who: d.who,
            text: d.text,
        })
        .collect();

    m.restore(state, agenda_hash, s.ratified_by, s.ratified_at);

    Ok(MeetingRecord {
        meeting: m,
        agenda_source: s.agenda_source,
        opened_at_index: s.opened_at_index,
    })
}

/// Write `bytes` to `path` so a crash leaves either the old file or the new one,
/// never a half-written one: temp file → fsync → rename → fsync the directory.
fn write_atomic(path: &Path, bytes: &[u8]) -> Result<(), StoreError> {
    let tmp = path.with_extension("tmp");
    {
        let mut f = File::create(&tmp)?;
        f.write_all(bytes)?;
        f.sync_all()?;
    }
    fs::rename(&tmp, path)?;
    if let Some(dir) = path.parent() {
        // Durability of the rename itself. Best-effort: not every platform
        // permits opening a directory, and failing here would be worse than the
        // (small) window it closes.
        if let Ok(d) = File::open(dir) {
            let _ = d.sync_all();
        }
    }
    Ok(())
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU32, Ordering};

    static SEQ: AtomicU32 = AtomicU32::new(0);

    struct TempRoot(PathBuf);
    impl TempRoot {
        fn new() -> Self {
            let n = SEQ.fetch_add(1, Ordering::SeqCst);
            let p =
                std::env::temp_dir().join(format!("quorum-store-test-{}-{n}", std::process::id()));
            let _ = fs::remove_dir_all(&p);
            Self(p)
        }
        fn store(&self) -> EvidenceStore {
            EvidenceStore::open(self.0.clone()).unwrap()
        }
    }
    impl Drop for TempRoot {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn t(id: &str) -> TenantId {
        TenantId::new(id).unwrap()
    }

    fn rec(class: &str, ts: i64) -> DecisionRecord {
        DecisionRecord {
            agent: "sbt-41".into(),
            principal: Some("R. Ortiz".into()),
            grant_id: Some("G-1".into()),
            action_class: class.into(),
            params_hash: [7u8; 32],
            verdict: Verdict::Allow,
            hic: HicLevel::Budgeted,
            model_id: "claude-sonnet-4-5".into(),
            correlation_id: "X-7104".into(),
            timestamp_ms: ts,
        }
    }

    /// Append records the way the backend does: chain first, then persist.
    fn append(store: &EvidenceStore, chain: &mut HashChain, tenant: &str, r: DecisionRecord) {
        let h = chain.append(r.clone()).unwrap();
        store.append_record(tenant, &r, h).unwrap();
    }

    #[test]
    fn a_chain_survives_a_restart_and_replays_to_the_same_head() {
        let root = TempRoot::new();
        let store = root.store();
        let mut chain = HashChain::new(t("bca"));
        append(&store, &mut chain, "bca", rec("repo.write", 1000));
        append(&store, &mut chain, "bca", rec("spend", 2000));
        let head = chain.head();

        // "Restart": nothing in memory, everything from disk.
        let reloaded = store.load_chain(&t("bca")).unwrap();
        assert_eq!(reloaded.len(), 2);
        assert_eq!(reloaded.head(), head, "the head must survive verbatim");
        assert!(reloaded.verify());
    }

    #[test]
    fn a_tenant_with_no_evidence_loads_as_empty_not_broken() {
        let root = TempRoot::new();
        let chain = root.store().load_chain(&t("never-seen")).unwrap();
        assert!(chain.is_empty());
        assert_eq!(chain.head(), HashChain::new(t("never-seen")).head());
    }

    #[test]
    fn an_edited_record_is_caught_on_load() {
        let root = TempRoot::new();
        let store = root.store();
        let mut chain = HashChain::new(t("bca"));
        append(&store, &mut chain, "bca", rec("repo.write", 1000));
        append(&store, &mut chain, "bca", rec("spend", 2000));

        // Tamper: rewrite the first record's action class, leaving its hash.
        let path = store.tenant_dir("bca").join("chain.jsonl");
        let raw = fs::read_to_string(&path).unwrap();
        fs::write(&path, raw.replacen("repo.write", "repo.reads", 1)).unwrap();

        match store.load_chain(&t("bca")) {
            Err(StoreError::Corrupt { line, .. }) => assert_eq!(line, 1),
            other => panic!("tampering must be caught, got {other:?}"),
        }
    }

    #[test]
    fn a_deleted_record_is_caught_on_load() {
        let root = TempRoot::new();
        let store = root.store();
        let mut chain = HashChain::new(t("bca"));
        append(&store, &mut chain, "bca", rec("repo.write", 1000));
        append(&store, &mut chain, "bca", rec("spend", 2000));

        // Drop the FIRST record — every later hash now fails to reproduce.
        let path = store.tenant_dir("bca").join("chain.jsonl");
        let raw = fs::read_to_string(&path).unwrap();
        let kept: Vec<&str> = raw.lines().skip(1).collect();
        fs::write(&path, kept.join("\n") + "\n").unwrap();

        assert!(
            matches!(store.load_chain(&t("bca")), Err(StoreError::Corrupt { .. })),
            "deleting evidence must not load cleanly"
        );
    }

    #[test]
    fn two_tenants_never_share_a_directory_or_a_chain() {
        let root = TempRoot::new();
        let store = root.store();
        let mut a = HashChain::new(t("bca"));
        let mut b = HashChain::new(t("sea"));
        append(&store, &mut a, "bca", rec("repo.write", 1000));
        append(&store, &mut b, "sea", rec("spend", 1000));

        assert_ne!(store.tenant_dir("bca"), store.tenant_dir("sea"));
        assert_eq!(store.load_chain(&t("bca")).unwrap().len(), 1);
        assert_eq!(store.load_chain(&t("sea")).unwrap().len(), 1);
        assert_ne!(
            store.load_chain(&t("bca")).unwrap().head(),
            store.load_chain(&t("sea")).unwrap().head()
        );
    }

    #[test]
    fn a_path_hostile_tenant_id_cannot_escape_the_store() {
        let root = TempRoot::new();
        let store = root.store();
        let hostile = "../../../etc/quorum-pwned";
        let mut chain = HashChain::new(t(hostile));
        append(&store, &mut chain, hostile, rec("repo.write", 1000));

        let dir = store.tenant_dir(hostile);
        assert!(
            dir.starts_with(&store.root),
            "tenant dir {dir:?} escaped the store root"
        );
        assert!(!dir.to_string_lossy().contains(".."));
        assert_eq!(store.load_chain(&t(hostile)).unwrap().len(), 1);
    }

    #[test]
    fn grants_round_trip_with_their_consumed_budget_and_revocation() {
        let root = TempRoot::new();
        let store = root.store();
        let grants = vec![CapabilityGrant {
            id: "G-1".into(),
            agent: "sbt-41".into(),
            principal: "R. Ortiz".into(),
            tenant_scope: "t3:bca".into(),
            action_classes: vec!["repo.write".into()],
            classification_ceiling: Classification::Cui,
            budget_units: 100,
            consumed: 37,
            expires_at_ms: 9_999_999,
            hic: GrantHic::PostHoc,
            revoked: true,
        }];
        store.save_grants("bca", &grants).unwrap();
        let back = store.load_grants("bca").unwrap();
        assert_eq!(back.len(), 1);
        assert_eq!(back[0].consumed, 37, "spent budget must not reset");
        assert!(back[0].revoked, "a revoked grant must not come back alive");
        assert_eq!(back[0].classification_ceiling, Classification::Cui);
        assert_eq!(back[0].hic, GrantHic::PostHoc);
    }

    #[test]
    fn allowances_round_trip_with_their_spent_weight() {
        let root = TempRoot::new();
        let store = root.store();
        let a = vec![VoteAllowance {
            id: "VA-1".into(),
            principal: "R. Ortiz".into(),
            agent: "claude-code".into(),
            tenant_scope: "t3:bca".into(),
            proposal_classes: vec!["standup".into()],
            weight_cap: 5,
            spent: 4,
            expires_at_ms: 9_999_999,
            revoked: false,
        }];
        store.save_allowances("bca", &a).unwrap();
        let back = store.load_allowances("bca").unwrap();
        assert_eq!(back[0].spent, 4, "spent weight must not reset to zero");
        assert_eq!(back[0].weight_cap, 5);
    }

    #[test]
    fn the_operator_survives_a_restart_and_does_not_clobber_the_scope() {
        let root = TempRoot::new();
        let store = root.store();
        store.save_scope(Some("bca")).unwrap();
        store.save_operator(Some("R. Ortiz")).unwrap();
        // Each write must preserve the other field.
        assert_eq!(root.store().load_scope().as_deref(), Some("bca"));
        assert_eq!(root.store().load_operator().as_deref(), Some("R. Ortiz"));
        store.save_scope(Some("sea")).unwrap();
        assert_eq!(root.store().load_operator().as_deref(), Some("R. Ortiz"));
    }

    #[test]
    fn the_active_scope_survives_a_restart() {
        let root = TempRoot::new();
        let store = root.store();
        assert!(store.load_scope().is_none());
        store.save_scope(Some("bca")).unwrap();
        assert_eq!(root.store().load_scope().as_deref(), Some("bca"));
        store.save_scope(None).unwrap();
        assert!(root.store().load_scope().is_none());
    }

    #[test]
    fn outstanding_charges_round_trip() {
        let root = TempRoot::new();
        let store = root.store();
        assert!(store.load_charges("bca").unwrap().is_empty());
        let charges = vec![Charge {
            decision: 3,
            agent: "sbt-41".into(),
            grant_id: "G-1".into(),
            units: 220,
        }];
        store.save_charges("bca", &charges).unwrap();
        assert_eq!(store.load_charges("bca").unwrap(), charges);
    }

    #[test]
    fn a_human_rejection_survives_the_round_trip() {
        let root = TempRoot::new();
        let store = root.store();
        let mut chain = HashChain::new(t("bca"));
        let mut r = rec("spend", 1000);
        r.verdict = Verdict::Rejected;
        r.hic = HicLevel::ApproveEach;
        append(&store, &mut chain, "bca", r);

        let back = store.load_chain(&t("bca")).unwrap();
        assert_eq!(back.len(), 1);
        let stored = back.records().next().unwrap();
        assert_eq!(stored.verdict, Verdict::Rejected);
        assert_eq!(back.head(), chain.head());
    }

    #[test]
    fn an_escalation_waiting_on_a_human_survives_a_restart() {
        let root = TempRoot::new();
        let store = root.store();
        assert!(store.load_pending("bca").unwrap().is_empty());
        let pending = vec![PendingApproval {
            decision: 3,
            agent: "claude-code".into(),
            principal: Some("R. Ortiz".into()),
            action_class: "spend".into(),
            classification: "Public".into(),
            cost: 220,
            correlation_id: "X-7104".into(),
            requested_at_ms: 1000,
        }];
        store.save_pending("bca", &pending).unwrap();
        assert_eq!(
            root.store().load_pending("bca").unwrap(),
            pending,
            "an agent that stopped and asked must still be waiting after a restart"
        );
    }

    #[test]
    fn a_fresh_install_permits_no_external_egress_at_a_controlled_classification() {
        let root = TempRoot::new();
        let p = root.store().load_egress();
        assert_eq!(p, EgressPolicy::default());

        // Q10: nothing external until a protocol permits it.
        assert!(!p.permits("CUI", "gpt-northstar"));
        assert!(!p.permits("ITAR", "gpt-northstar"));
        // An unnamed model is not a permitted one — "we did not check" is not
        // a permission.
        assert!(!p.permits("CUI", ""));
        assert!(!p.permits("ITAR", "   "));
        // Uncontrolled classifications are unaffected.
        assert!(p.permits("Public", "gpt-northstar"));
        assert!(p.permits("Proprietary", ""));
    }

    #[test]
    fn a_deployed_policy_permits_exactly_what_it_names() {
        let root = TempRoot::new();
        let store = root.store();
        let mut policy = EgressPolicy::default();
        policy
            .allow
            .insert("CUI".into(), vec!["local/gemma-3-4b".into()]);
        store.save_egress(&policy).unwrap();

        let back = root.store().load_egress();
        assert_eq!(back, policy, "the policy survives a restart");
        assert!(back.permits("CUI", "local/gemma-3-4b"));
        assert!(!back.permits("CUI", "gpt-northstar"), "only what it names");
        assert!(
            !back.permits("ITAR", "local/gemma-3-4b"),
            "permitting a model at CUI must not permit it at ITAR"
        );
    }

    #[test]
    fn hex_round_trips_a_params_hash() {
        let h = [0xab_u8; 32];
        assert_eq!(hex32(&hex_encode(&h)), h);
    }

    // ---- meetings (WP-S5.3) ------------------------------------------

    fn ratified_meeting() -> MeetingRecord {
        let mut m = Meeting::schedule(
            "m-1",
            "Weekly Standup",
            "2026-07-23T09:00:00Z",
            "citrate",
            Template::new("Standup", 2),
            Classification::Proprietary,
        );
        for who in ["R. Ortiz", "M. Okonkwo"] {
            m.admit(
                Attendee::human(who)
                    .attested()
                    .cleared_to(Classification::Cui),
            )
            .unwrap();
        }
        m.add_agenda_item("S5.1 — the meetings domain", "sprint-qrm-s5/SCOPE.md")
            .unwrap();
        m.open().unwrap();
        m.close(vec!["Reports accepted.".into()]).unwrap();
        let h = m.content_hash();
        m.ratify("R. Ortiz", 1_753_460_000, h).unwrap();
        MeetingRecord {
            meeting: m,
            agenda_source: "generated from sprint-qrm-s5".into(),
            opened_at_index: Some(3),
        }
    }

    #[test]
    fn a_ratified_meeting_reloads_ratified_and_still_frozen() {
        let root = TempRoot::new();
        let store = root.store();
        let rec = ratified_meeting();
        let signed = rec.meeting.content_hash();

        store.save_meetings("acme", &[rec]).unwrap();
        let back = store.load_meetings("acme").unwrap();

        assert_eq!(back.len(), 1);
        let m = &back[0].meeting;
        assert_eq!(m.state(), MeetingState::Ratified);
        assert_eq!(m.ratified_by.as_deref(), Some("R. Ortiz"));
        assert_eq!(back[0].opened_at_index, Some(3));
        assert_eq!(back[0].agenda_source, "generated from sprint-qrm-s5");
        // The hash the human signed survives the round trip — otherwise the
        // signature commits to something the reloaded app cannot reproduce.
        assert_eq!(m.content_hash(), signed);
        // And the agenda is still frozen: reloading is not a way back in.
        assert!(m.agenda_hash().is_some());
    }

    #[test]
    fn a_meeting_whose_agenda_was_edited_on_disk_refuses_to_load() {
        // The attack this exists for: swap the agenda under a ratified
        // meeting, and the signature would appear to cover text nobody signed.
        let root = TempRoot::new();
        let store = root.store();
        store.save_meetings("acme", &[ratified_meeting()]).unwrap();

        let path = store.tenant_dir("acme").join("meetings.json");
        let raw = fs::read_to_string(&path).unwrap();
        let tampered = raw.replace(
            "S5.1 — the meetings domain",
            "S5.1 — something else entirely",
        );
        assert_ne!(raw, tampered, "the test must actually change the agenda");
        fs::write(&path, tampered).unwrap();

        let err = store.load_meetings("acme").unwrap_err();
        assert!(
            format!("{err}").contains("does not match its frozen hash"),
            "expected a frozen-hash mismatch, got: {err}"
        );
    }

    #[test]
    fn an_empty_tenant_has_no_meetings_rather_than_an_error() {
        let root = TempRoot::new();
        assert!(root.store().load_meetings("never-seen").unwrap().is_empty());
    }

    /// **Every production caller must open the store at the SAME path.**
    ///
    /// A source scan, because the property is about agreement between modules
    /// that never meet in a unit test. `lib.rs` opened `<app_data>/evidence`
    /// while five QRM-S7 commands opened `<app_data>`; both calls SUCCEED,
    /// because `open` creates what is missing. The result was a pipeline reading
    /// a freshly-created empty store and reporting "no tenant scope is
    /// established" — and SIMULATE replaying an empty corpus and reporting
    /// zeros, which reads as a narrow corpus rather than a wrong directory.
    ///
    /// Nothing failed. That is why this is a scan and not an assertion: there
    /// was no error to assert on.
    #[test]
    fn every_production_call_site_opens_the_same_store() {
        let files = [
            ("lib.rs", include_str!("lib.rs")),
            ("deploy.rs", include_str!("deploy.rs")),
            ("bind.rs", include_str!("bind.rs")),
            ("protocols.rs", include_str!("protocols.rs")),
            ("interview.rs", include_str!("interview.rs")),
            ("simulate.rs", include_str!("simulate.rs")),
            ("backend.rs", include_str!("backend.rs")),
        ];
        for (name, src) in files {
            // Production half only: test helpers legitimately open stores at
            // explicit temp paths, which is the whole reason unit tests could
            // not see this bug.
            let prod = src.split("#[cfg(test)]").next().unwrap_or(src);
            for (i, line) in prod.lines().enumerate() {
                if !line.contains("EvidenceStore::open(") {
                    continue;
                }
                assert!(
                    line.contains("evidence_dir("),
                    "{name}:{} opens the evidence store without store::evidence_dir(): {}\n\
                     Every caller must resolve the path the same way or they silently \
                     read different stores.",
                    i + 1,
                    line.trim()
                );
            }
        }
    }

    /// The canonical path is `<app_data>/evidence` — pinned, because changing it
    /// silently orphans every existing installation's evidence chain.
    #[test]
    fn the_evidence_directory_is_pinned() {
        assert_eq!(
            evidence_dir(std::path::Path::new("/tmp/appdata")),
            std::path::Path::new("/tmp/appdata/evidence")
        );
    }
}
