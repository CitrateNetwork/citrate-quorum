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

    pub fn load_scope(&self) -> Option<String> {
        let raw = fs::read_to_string(self.root.join("scope.json")).ok()?;
        serde_json::from_str::<StoredScope>(&raw)
            .ok()?
            .active_tenant
    }

    pub fn save_scope(&self, tenant: Option<&str>) -> Result<(), StoreError> {
        let body = serde_json::to_vec(&StoredScope {
            active_tenant: tenant.map(str::to_string),
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
    fn hex_round_trips_a_params_hash() {
        let h = [0xab_u8; 32];
        assert_eq!(hex32(&hex_encode(&h)), h);
    }
}
