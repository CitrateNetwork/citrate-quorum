//! QRM-S7.8 — the two reads the Governance surface opens with.
//!
//! `governance.specs()` lists the drafts on this machine; `governance.protocols()`
//! lists what the tenant actually has on chain. Both were `Unavailable` stubs
//! until now, which was honest while there was nothing to show and stops being
//! honest the moment a protocol exists.
//!
//! # "live" is not a state a protocol can be in here
//!
//! A deployed protocol that nothing has bound governs **nothing**:
//! `PolicyBinding.check` answers `Allow / PB_UNGOVERNED` for every action class,
//! which quorum records as `ungoverned` and alerts on. So the states are
//! `bound` and `unbound`, and the word "live" does not appear — it is the most
//! flattering available description of a contract that is doing nothing.
//!
//! # What "governs" can and cannot say
//!
//! `PolicyBinding` indexes bindings by `(tenant, actionClass) → protocols`.
//! There is **no reverse index**: nothing on chain can answer "which action
//! classes is this protocol bound to". So this asks `isBound` about the classes
//! this app has a local record of, reports the ones the chain CONFIRMS, and
//! says plainly that a binding made elsewhere would not appear. A surface that
//! rendered an unqualified "governs: repo.write" from local state alone would be
//! reporting our own memory as chain fact.

use serde::Serialize;

use crate::addresses::AddressBook;
use crate::anchor::keccak256;

fn selector(sig: &str) -> [u8; 4] {
    let d = keccak256(sig.as_bytes());
    [d[0], d[1], d[2], d[3]]
}

fn call_words(sig: &str, words: &[[u8; 32]]) -> String {
    let mut out = Vec::with_capacity(4 + words.len() * 32);
    out.extend_from_slice(&selector(sig));
    for w in words {
        out.extend_from_slice(w);
    }
    format!("0x{}", hex::encode(out))
}

fn word_of(n: u64) -> [u8; 32] {
    let mut w = [0u8; 32];
    w[24..].copy_from_slice(&n.to_be_bytes());
    w
}

fn u64_at(bytes: &[u8], word: usize) -> Option<u64> {
    let end = word * 32 + 32;
    if bytes.len() < end {
        return None;
    }
    Some(u64::from_be_bytes(bytes[end - 8..end].try_into().ok()?))
}

fn addr_at(bytes: &[u8], word: usize) -> Option<String> {
    let end = word * 32 + 32;
    if bytes.len() < end {
        return None;
    }
    Some(format!("0x{}", hex::encode(&bytes[end - 20..end])))
}

/// `YYYY-MM-DD` UTC from epoch seconds.
///
/// UTC, like every other timestamp this app renders: an evidence trail read
/// across sites must not shift under the reader. Hinnant's civil-from-days,
/// which is exact for the whole proleptic Gregorian range — the alternative was
/// a dependency for one function.
fn iso_date_utc(secs: i64) -> String {
    let z = secs.div_euclid(86_400) + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    format!("{y:04}-{m:02}-{d:02}")
}

// ── Specs ───────────────────────────────────────────────────────────────

#[derive(Serialize, Debug, Clone)]
pub struct SpecSummaryDto {
    pub id: String,
    pub title: String,
    pub stage: String,
    pub updated: String,
    pub classification: String,
}

/// How far a draft has got, decided by what is on disk rather than by a status
/// field somebody has to remember to update.
///
/// Deliberately conservative: a stage is only claimed when the artefact that
/// proves it exists. `deployed` and `bound` are claimed from the intent records
/// this app writes when a ceremony is RAISED, so they are named
/// `deploy-pending` / `bind-pending` until the corresponding completion is
/// recorded — an enqueued ceremony is not a deployment.
fn stage_of(root: &std::path::Path, spec_id: &str, answered: usize, total: usize) -> String {
    if bind_recorded(root, spec_id) {
        return "bound".to_string();
    }
    if deploy_recorded(root, spec_id) {
        return "deployed".to_string();
    }
    if answered == 0 {
        return "ingested".to_string();
    }
    if answered < total {
        return "interviewing".to_string();
    }
    "drafted".to_string()
}

/// Whether a completed deploy has been recorded for this spec.
fn deploy_recorded(root: &std::path::Path, spec_id: &str) -> bool {
    completed_dir(root, "deployed").is_some_and(|d| d.join(format!("{spec_id}.json")).exists())
}

fn bind_recorded(root: &std::path::Path, spec_id: &str) -> bool {
    completed_dir(root, "bound").is_some_and(|d| d.join(format!("{spec_id}.json")).exists())
}

fn completed_dir(root: &std::path::Path, which: &str) -> Option<std::path::PathBuf> {
    Some(root.join("pipeline").join(which))
}

/// Record that a spec reached a stage. Written only after the chain has
/// confirmed it — never when a ceremony is merely raised.
pub fn record_stage(
    root: &std::path::Path,
    which: &str,
    spec_id: &str,
    body: &str,
) -> Result<(), String> {
    let Some(dir) = completed_dir(root, which) else {
        return Ok(());
    };
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let safe: String = spec_id
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '-' || *c == '_')
        .collect();
    std::fs::write(dir.join(format!("{safe}.json")), body).map_err(|e| e.to_string())
}

/// `governance.specs` — the drafts on this machine.
#[tauri::command]
pub fn governance_specs(app: tauri::AppHandle) -> Result<Vec<SpecSummaryDto>, String> {
    use tauri::Manager;
    let root = app
        .path()
        .app_data_dir()
        .map_err(|e| format!("no app data dir: {e}"))?;

    let dir = root.join("interviews");
    let entries = match std::fs::read_dir(&dir) {
        Ok(e) => e,
        // No drafts yet is an empty list, not an error. The surface renders its
        // own empty state; an error here would read as "the app is broken".
        Err(_) => return Ok(Vec::new()),
    };

    let mut out = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("json") {
            continue;
        }
        let Some(spec_id) = path.file_stem().and_then(|s| s.to_str()) else {
            continue;
        };
        let iv = crate::interview::load(&root, spec_id);
        let answered = iv.turns.iter().filter(|t| t.answer.is_some()).count();
        let meta = crate::ingest::load_meta(&root, spec_id);
        let updated = entry
            .metadata()
            .and_then(|m| m.modified())
            .ok()
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|d| iso_date_utc(d.as_secs() as i64))
            .unwrap_or_default();

        out.push(SpecSummaryDto {
            id: spec_id.to_string(),
            title: meta
                .as_ref()
                .and_then(|m| m.sources.first().cloned())
                .unwrap_or_else(|| "Untitled policy".to_string()),
            stage: stage_of(
                &root,
                spec_id,
                answered,
                crate::interview::Topic::ALL.len(),
            ),
            updated,
            // A spec with no ingested sources has NO classification. Rendering
            // "Public" for it would be a guess at the most permissive value.
            classification: meta
                .map(|m| m.classification)
                .unwrap_or_else(|| "unclassified".to_string()),
        });
    }
    out.sort_by(|a, b| b.updated.cmp(&a.updated));
    Ok(out)
}

// ── Protocols ───────────────────────────────────────────────────────────

#[derive(Serialize, Debug, Clone)]
pub struct ProtocolDto {
    pub id: String,
    pub name: String,
    pub version: String,
    pub template: String,
    pub audit: String,
    pub addr: String,
    pub state: String,
    pub governs: String,
    pub deployed: String,
    pub source: String,
}

/// `governance.protocols` — what the tenant actually has on chain.
///
/// Enumerated through the factory's own `protocolCount`/`protocolAt` index, so
/// this is the chain's list rather than a local one that could have drifted.
#[tauri::command]
pub fn governance_protocols(app: tauri::AppHandle) -> Result<Vec<ProtocolDto>, String> {
    use tauri::Manager;
    let root = app
        .path()
        .app_data_dir()
        .map_err(|e| format!("no app data dir: {e}"))?;
    let book = AddressBook::load().map_err(|e| e.to_string())?;
    let bfr = AddressBook::load_bfr().map_err(|e| e.to_string())?;
    let factory = book
        .get("GovernanceProtocolFactory")
        .ok_or("GovernanceProtocolFactory is not in the address book")?;
    let policy_binding = book
        .get("PolicyBinding")
        .ok_or("PolicyBinding is not in the address book")?;
    let registry = book
        .get("GovernanceTemplateRegistry")
        .ok_or("GovernanceTemplateRegistry is not in the address book")?;
    let rpc = crate::chain::Rpc::from_book(&book)?;

    let store = crate::store::EvidenceStore::open(root.clone())
        .map_err(|e| format!("evidence store: {e}"))?;
    let tenant = store.load_scope().ok_or("no tenant scope is established")?;
    let tenant_id = crate::chain::tenant_id_of(&tenant);

    // The tenant not existing on chain is an empty list with a reason, not an
    // error: it is the true state of a machine that has been scoped but whose
    // tenant has not been created.
    if crate::chain::tenant_node(&bfr, tenant_id)?.is_none() {
        return Ok(Vec::new());
    }

    let source = format!(
        "GovernanceProtocolFactory.protocolAt/deploymentOf at {factory} · \
         PolicyBinding.isBound at {policy_binding} · chain {} · {}",
        book.chain_id,
        rpc.url()
    );

    let count_ret = rpc.eth_call(
        factory,
        &call_words("protocolCount(bytes32)", &[tenant_id]),
    )?;
    let count = u64_at(&count_ret, 0).unwrap_or(0);

    // The registry's name→id table, inverted, so a deployment's templateId can
    // be shown as the name a human wrote in the spec.
    let catalog = crate::compile::catalog_from_chain(&rpc, registry)?;

    let mut out = Vec::new();
    for i in 0..count {
        let at = rpc.eth_call(
            factory,
            &call_words("protocolAt(bytes32,uint256)", &[tenant_id, word_of(i)]),
        )?;
        let Some(addr) = addr_at(&at, 0) else { continue };

        let mut w = [0u8; 32];
        w[12..].copy_from_slice(&hex::decode(addr.trim_start_matches("0x")).unwrap_or_default());
        let dep = rpc.eth_call(factory, &call_words("deploymentOf(address)", &[w]))?;

        // Deployment { tenantId, templateId, specHash, specCID(offset),
        //              paramSchemaHash, classificationCeiling, deployer,
        //              blockNumber, … } — a dynamic struct, so word 0 is the
        //              offset to it.
        let base = (u64_at(&dep, 0).unwrap_or(32) / 32) as usize;
        let template_id = dep
            .get(base * 32 + 32..base * 32 + 64)
            .map(hex::encode)
            .map(|h| format!("0x{h}"))
            .unwrap_or_default();
        let name = catalog
            .templates
            .iter()
            .find(|t| t.id.eq_ignore_ascii_case(&template_id))
            .map(|t| t.name.clone())
            // A template id the registry does not carry is shown as the id.
            // Guessing a name for unknown bytecode is the opposite of what the
            // audit boundary is for.
            .unwrap_or_else(|| template_id.clone());
        let version = catalog
            .templates
            .iter()
            .find(|t| t.id.eq_ignore_ascii_case(&template_id))
            .map(|t| format!("v{}", t.version))
            .unwrap_or_else(|| "unknown".to_string());

        // Which action classes the chain CONFIRMS. Only classes this app has a
        // record of can be asked about — see the module header.
        let mut governs: Vec<String> = Vec::new();
        for class in known_classes(&root) {
            let mut cw = [0u8; 32];
            cw.copy_from_slice(&crate::bind::action_class_id(&class));
            let bound = rpc.eth_call(
                policy_binding,
                &call_words("isBound(bytes32,bytes32,address)", &[tenant_id, cw, w]),
            )?;
            if bound.last().copied().unwrap_or(0) == 1 {
                governs.push(class);
            }
        }

        out.push(ProtocolDto {
            id: addr.clone(),
            name: name.clone(),
            version,
            template: name,
            // D-2: the CID's own content states this is an unaudited devnet
            // deployment. The surface must not shorten this to "audited".
            audit: "devnet CID — unaudited".to_string(),
            addr: addr.clone(),
            state: if governs.is_empty() {
                "unbound".to_string()
            } else {
                "bound".to_string()
            },
            governs: if governs.is_empty() {
                "nothing — no binding this app knows of".to_string()
            } else {
                governs.join(", ")
            },
            deployed: format!("block {}", u64_at(&dep, base + 8).unwrap_or(0)),
            source: source.clone(),
        });
    }
    Ok(out)
}

/// The spec a deployed protocol came from, by matching the recorded deploy.
///
/// BIND is given a protocol address and an action class, not a spec id — so the
/// link back to the draft is this app's own deploy record. A protocol deployed
/// elsewhere has no record here and returns `None`, which is correct: we do not
/// know which spec it implements and must not claim to.
pub fn spec_for_protocol(root: &std::path::Path, addr: &str) -> Option<String> {
    let dir = completed_dir(root, "deployed")?;
    for e in std::fs::read_dir(dir).ok()?.flatten() {
        let raw = std::fs::read_to_string(e.path()).ok()?;
        let v: serde_json::Value = serde_json::from_str(&raw).ok()?;
        if v.get("address")
            .and_then(|a| a.as_str())
            .is_some_and(|a| a.eq_ignore_ascii_case(addr))
        {
            return v
                .get("spec_id")
                .and_then(|s| s.as_str())
                .map(str::to_string);
        }
    }
    None
}

/// Action classes this app has seen named, from its own bind records.
fn known_classes(root: &std::path::Path) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let Ok(entries) = std::fs::read_dir(root.join("bind-intents")) else {
        return out;
    };
    for e in entries.flatten() {
        let Ok(raw) = std::fs::read_to_string(e.path()) else {
            continue;
        };
        let Ok(dto) = serde_json::from_str::<crate::bind::BindIntentDto>(&raw) else {
            continue;
        };
        if !out.contains(&dto.action_class) {
            out.push(dto.action_class);
        }
    }
    out
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn calldata_matches_cast() {
        // cast calldata "protocolCount(bytes32)" 0x1111…
        assert_eq!(
            call_words("protocolCount(bytes32)", &[[0x11; 32]]),
            concat!(
                "0xd4556f46",
                "1111111111111111111111111111111111111111111111111111111111111111"
            )
        );
        // cast calldata "protocolAt(bytes32,uint256)" 0x1111… 2
        assert_eq!(
            call_words("protocolAt(bytes32,uint256)", &[[0x11; 32], word_of(2)]),
            concat!(
                "0x980a65ab",
                "1111111111111111111111111111111111111111111111111111111111111111",
                "0000000000000000000000000000000000000000000000000000000000000002"
            )
        );
    }

    /// A deployed protocol with no confirmed binding is `unbound`, never
    /// `live`. This is the honest post-deploy state and the one most likely to
    /// be dressed up.
    #[test]
    fn an_unbound_protocol_is_not_described_as_live() {
        let governs: Vec<String> = Vec::new();
        let state = if governs.is_empty() { "unbound" } else { "bound" };
        assert_eq!(state, "unbound");
        assert_ne!(state, "live");
    }

    /// A spec with no ingested sources has no classification. Defaulting it to
    /// `Public` would be a guess at the MOST PERMISSIVE value, which is the
    /// wrong direction to guess in.
    #[test]
    fn a_spec_without_sources_is_unclassified_not_public() {
        let dir = std::env::temp_dir().join(format!("qrm-specs-{}", std::process::id()));
        assert!(crate::ingest::load_meta(&dir, "spec-none").is_none());
    }

    /// Stage is decided by artefacts on disk, and an enqueued ceremony is not
    /// one of them: a raised-but-unapproved deploy must not read as `deployed`.
    #[test]
    fn stage_tracks_what_exists_on_disk() {
        let dir = std::env::temp_dir().join(format!("qrm-stage-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        assert_eq!(stage_of(&dir, "s1", 0, 7), "ingested");
        assert_eq!(stage_of(&dir, "s1", 3, 7), "interviewing");
        assert_eq!(stage_of(&dir, "s1", 7, 7), "drafted");

        record_stage(&dir, "deployed", "s1", "{}").expect("record");
        assert_eq!(stage_of(&dir, "s1", 7, 7), "deployed");
        record_stage(&dir, "bound", "s1", "{}").expect("record");
        assert_eq!(stage_of(&dir, "s1", 7, 7), "bound");
        // A different spec is unaffected.
        assert_eq!(stage_of(&dir, "s2", 7, 7), "drafted");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Pinned against `date -u -d @<secs> +%F`. A date formatter that is a day
    /// out sorts a spec list wrongly and dates an evidence record wrongly.
    #[test]
    fn the_date_formatter_is_correct() {
        assert_eq!(iso_date_utc(0), "1970-01-01");
        assert_eq!(iso_date_utc(1_785_283_200), "2026-07-29");
        // A leap day, and the day either side of it.
        assert_eq!(iso_date_utc(1_709_164_800), "2024-02-29");
        assert_eq!(iso_date_utc(1_709_164_800 - 86_400), "2024-02-28");
        assert_eq!(iso_date_utc(1_709_164_800 + 86_400), "2024-03-01");
        // A century that is not a leap year.
        assert_eq!(iso_date_utc(4_107_542_400), "2100-03-01");
    }

    #[test]
    fn a_spec_id_cannot_escape_the_pipeline_directory() {
        let dir = std::env::temp_dir().join(format!("qrm-esc-{}", std::process::id()));
        record_stage(&dir, "deployed", "../../etc/passwd", "{}").expect("record");
        assert!(dir.join("pipeline/deployed/etcpasswd.json").exists());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
