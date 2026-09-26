//! QRM-S7.6 — CEREMONY + DEPLOY: show the address before signing.
//!
//! Stages 6 and 7, in two phases matching `rooms.connectIntent`/`connectComplete`:
//!
//! * [`deploy_intent`] builds the transaction and returns the address the
//!   protocol WILL have. **Signs nothing, sends nothing.**
//! * [`deploy_complete`] records what the ceremony broadcast, and checks the
//!   address it actually got.
//!
//! # Why the predicted address is read from the factory
//!
//! GF-1 exists so a human approves a known address rather than "a protocol,
//! somewhere". The temptation is to compute CREATE2 locally — it is twenty lines
//! — but then the address shown to the human is produced by different code from
//! the address that gets deployed, and the two agree only as long as nobody
//! edits either.
//!
//! So this calls the factory's own `predict()`. S6.2 proved by negative control
//! that `predict` and the real CREATE2 are the same computation: breaking one
//! makes the other disagree and the test fails. Asking the contract inherits
//! that guarantee instead of re-earning it.
//!
//! # The ceremony cannot describe this transaction, and that is correct
//!
//! The kit's decoder does not ABI-decode selectors: a factory call renders as
//! `Call 0x… with N bytes calldata`. It is **right** not to invent a friendlier
//! summary — it refuses fabricated benign decodes by design. So everything a
//! human needs travels in [`DeployIntent`] instead, and the surface shows it
//! beside the ceremony's own honest, uninformative line.
//!
//! # Two refusals
//!
//! An **undeployable spec** cannot produce an intent. R-A is not a warning that
//! gets clicked through: a spec with a clause that maps to no audited template
//! must not reach a ceremony, because the ceremony is where a human takes
//! responsibility and they would be taking it for a policy that is missing a
//! piece.
//!
//! **Absent creation code** is refused rather than worked around. GF-2 requires
//! supplying bytecode that hashes to the registered `initCodeHash`, and this app
//! cannot invent it — the audited bytes have to be vendored in. Until they are,
//! this says so with the path it looked in.

use std::collections::BTreeMap;

use crate::compile::Compiled;

/// The audited creation code, EMBEDDED.
///
/// These are the exact bytes `GovernanceProtocolFactory` will hash against the
/// registry's pinned `initCodeHash` (GF-2). They are compiled into the binary
/// rather than read from a path, for two reasons:
///
/// * **A path is a substitution point.** Anyone able to set an environment
///   variable could point the app at different bytecode. GF-2 would still refuse
///   it on chain, so this is not a hole — but the failure would arrive at the
///   ceremony, after a human had already been asked to approve, and would read
///   as "the chain rejected your deploy" rather than "someone swapped your
///   templates".
/// * **A missing artifact stops being a runtime state.** The app either has the
///   audited bytes or does not build.
///
/// Vendored by `scripts/vendor-template-artifacts.sh`, which by default refuses
/// to write a file whose hash does not match the value read from the LIVE
/// registry. With `--from-build` it vendors a clean build of a pinned
/// citrate-chain revision instead, for templates whose source changed and are
/// not registered yet; `scripts/verify-template-hashes.sh` proves those bytes
/// equal a fresh build. `template_hashes.rs` records the values and where they
/// came from, and a test below asserts each embedded artifact hashes to its own.
const EMBEDDED: &[(&str, &str)] = &[
    ("ThresholdApproval", include_str!("../artifacts/templates/ThresholdApproval.hex")),
    ("ClassificationGate", include_str!("../artifacts/templates/ClassificationGate.hex")),
    ("BudgetedAutonomy", include_str!("../artifacts/templates/BudgetedAutonomy.hex")),
    ("SegregationOfDuties", include_str!("../artifacts/templates/SegregationOfDuties.hex")),
    ("TimeBoundedElevation", include_str!("../artifacts/templates/TimeBoundedElevation.hex")),
    ("ChangeControlBoard", include_str!("../artifacts/templates/ChangeControlBoard.hex")),
    ("SupplierAdmission", include_str!("../artifacts/templates/SupplierAdmission.hex")),
    ("IncidentEscalation", include_str!("../artifacts/templates/IncidentEscalation.hex")),
];

#[derive(Debug, PartialEq, Eq)]
pub enum DeployError {
    /// The spec has clauses that map to no audited template.
    NotDeployable { unmapped: usize },
    /// No template was mapped, so there is nothing to deploy.
    NothingToDeploy,
    /// This app does not carry the audited creation code for that template.
    NoCreationCode { template: String },
    /// The tenant this app is scoped to has no node in `TenantHierarchy`.
    ///
    /// GF-4 makes `getNode` revert for an unknown tenant, so a deploy against
    /// one fails on chain after a human has approved it. Catching it here turns
    /// an opaque mid-ceremony revert into a sentence naming the tenant and the
    /// id it hashes to.
    NoSuchTenant { tenant: String, id: String },
    /// The operator's wallet is not an admin of that tenant.
    NotTenantAdmin {
        tenant: String,
        operator: String,
        admins: Vec<String>,
    },
    /// The spec's classification is above what the tenant may hold.
    CeilingExceedsTenant { spec: String, tenant: String },
    /// The spec has no ingested sources, so its classification is unknown.
    NoClassification { spec_id: String },
}

impl DeployError {
    pub fn why(&self) -> String {
        match self {
            DeployError::NotDeployable { unmapped } => format!(
                "{unmapped} clause(s) map to no audited template. A ceremony is where \
                 a human takes responsibility for a policy, and they must not be asked \
                 to take it for one that is missing a piece — resolve the unmapped \
                 clauses first"
            ),
            DeployError::NothingToDeploy => {
                "no clause mapped to a template, so there is no protocol to deploy"
                    .to_string()
            }
            DeployError::NoCreationCode { template } => format!(
                "this app does not carry the audited creation code for {template}, so \
                 it cannot be deployed from here. GF-2 requires supplying bytecode that \
                 hashes to the registered initCodeHash and this app will not invent it \
                 — re-run scripts/vendor-template-artifacts.sh"
            ),
            DeployError::NoSuchTenant { tenant, id } => format!(
                "TenantHierarchy holds no node for \"{tenant}\" ({id}). A protocol is \
                 deployed INTO a tenant and GF-4 reverts for one that does not exist, \
                 so this cannot proceed — the node has to be created by an admin of its \
                 parent first"
            ),
            DeployError::NotTenantAdmin {
                tenant,
                operator,
                admins,
            } => format!(
                "{operator} is not an admin of \"{tenant}\" (admins: {}). GF-4 gates \
                 deployProtocol on tenant admin membership, so the factory would revert \
                 this after a human had already approved it",
                if admins.is_empty() {
                    "none".to_string()
                } else {
                    admins.join(", ")
                }
            ),
            DeployError::CeilingExceedsTenant { spec, tenant } => format!(
                "this spec is {spec} but the tenant's ceiling is {tenant}. GF-4 refuses a \
                 protocol whose classificationCeiling exceeds its tenant's, and lowering \
                 the spec's ceiling to fit would deploy a control that does not cover the \
                 documents it was drawn from"
            ),
            DeployError::NoClassification { spec_id } => format!(
                "{spec_id} has no ingested sources, so there is nothing that says what \
                 classification it governs at. A deployment ceiling that had to be \
                 assumed is not a ceiling anyone set — ingest the source documents first"
            ),
        }
    }
}

/// The salt for a deployment.
///
/// `keccak256(tenantId ‖ specId ‖ templateId)` — deterministic, so re-running an
/// intent for the same spec predicts the same address, and distinct per tenant
/// so two tenants deploying the same template from the same spec do not collide
/// (GF-5 would refuse the second, which would look like a bug rather than a
/// namespace collision).
pub fn salt_for(tenant_id: &str, spec_id: &str, template_id: &str) -> String {
    let mut h = blake3::Hasher::new();
    h.update(tenant_id.as_bytes());
    h.update(spec_id.as_bytes());
    h.update(template_id.as_bytes());
    format!("0x{}", &h.finalize().to_hex()[..64])
}

/// ABI-encode `deployProtocol(...)`.
///
/// Hand-rolled, and pinned in tests against a vector produced by `cast calldata`
/// — the same discipline that caught a wrong selector and a stray `0x` in the
/// template-id encoder. An encoder that is subtly wrong here produces a
/// well-formed transaction that deploys something nobody described.
#[allow(clippy::too_many_arguments)]
pub fn encode_deploy_protocol(
    tenant_id: &str,
    template_id: &str,
    creation_code: &[u8],
    params: &[u8],
    spec_hash: &str,
    spec_cid: &str,
    classification_ceiling: u8,
    salt: &str,
    correlation_id: &str,
) -> String {
    // Head: 9 words. Three of them are offsets into the tail.
    const WORDS: usize = 9;
    let head_bytes = WORDS * 32;

    let cc = pad_bytes(creation_code);
    let pp = pad_bytes(params);
    let cid = pad_bytes(spec_cid.as_bytes());

    let off_cc = head_bytes;
    let off_pp = off_cc + 32 + cc.len() / 2;
    let off_cid = off_pp + 32 + pp.len() / 2;

    let mut out = String::from("0x3b945b77");
    out.push_str(&word_hex(tenant_id));
    out.push_str(&word_hex(template_id));
    out.push_str(&format!("{off_cc:064x}"));
    out.push_str(&format!("{off_pp:064x}"));
    out.push_str(&word_hex(spec_hash));
    out.push_str(&format!("{off_cid:064x}"));
    out.push_str(&format!("{:064x}", classification_ceiling));
    out.push_str(&word_hex(salt));
    out.push_str(&word_hex(correlation_id));

    // Tail, in the order the offsets name.
    out.push_str(&format!("{:064x}", creation_code.len()));
    out.push_str(&cc);
    out.push_str(&format!("{:064x}", params.len()));
    out.push_str(&pp);
    out.push_str(&format!("{:064x}", spec_cid.len()));
    out.push_str(&cid);
    out
}

/// A 32-byte value as 64 hex chars, accepting `0x`-prefixed or bare input.
fn word_hex(v: &str) -> String {
    let s = v.strip_prefix("0x").unwrap_or(v);
    format!("{s:0>64}")
}

/// Hex of `bytes`, right-padded to a 32-byte boundary. No `0x`.
fn pad_bytes(bytes: &[u8]) -> String {
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        s.push_str(&format!("{b:02x}"));
    }
    while s.len() % 64 != 0 {
        s.push('0');
    }
    s
}

/// Load the audited creation code for a template.
///
/// A template this app does not carry is refused by name. It cannot be
/// deployed from here, and saying so is better than handing the factory bytes
/// that will fail GF-2 in front of a human mid-ceremony.
pub fn creation_code(template: &str) -> Result<Vec<u8>, DeployError> {
    let hex_str = EMBEDDED
        .iter()
        .find(|(n, _)| *n == template)
        .map(|(_, h)| *h)
        .ok_or_else(|| DeployError::NoCreationCode {
            template: template.to_string(),
        })?;
    hex::decode(hex_str.trim().trim_start_matches("0x")).map_err(|_| {
        DeployError::NoCreationCode {
            template: template.to_string(),
        }
    })
}

/// The first mapped clause's template — what this deployment is for.
///
/// One protocol per ceremony deliberately. A ceremony that deployed several
/// contracts would ask a human to approve one thing and get several, and the
/// predicted address shown would be one of them.
pub fn target(compiled: &Compiled) -> Result<(&str, &str, &BTreeMap<String, String>), DeployError> {
    if !compiled.unmapped.is_empty() {
        return Err(DeployError::NotDeployable {
            unmapped: compiled.unmapped.len(),
        });
    }
    let m = compiled.mapped.first().ok_or(DeployError::NothingToDeploy)?;
    Ok((&m.template_name, &m.template_id, &m.params))
}

// ── Tauri seam ──────────────────────────────────────────────────────────

use serde::Serialize;

#[derive(Serialize, serde::Deserialize, Debug, Clone)]
pub struct DeployIntentDto {
    pub ceremony_id: String,
    pub spec_id: String,
    pub predicted_address: String,
    pub template_id: String,
    pub template_name: String,
    pub tenant_id: String,
    /// The tenant's display name, as `TenantHierarchy` holds it — the id alone
    /// is a hash, and a human approving a deploy needs the name.
    pub tenant_name: String,
    pub spec_hash: String,
    pub spec_cid: String,
    pub salt: String,
    pub ceremony_action: String,
    /// The classification this protocol is deployed at, from the spec's sources.
    pub classification: String,
    /// The principals whose approval the protocol will require, by name.
    pub approvers: Vec<String>,
    /// The action class the spec says this governs — what S7.7 binds it to.
    /// `None` when the spec has no scope clause, which BIND then refuses.
    pub action_class: Option<String>,
    /// Rule 11: the contracts, chain and book behind everything above.
    pub source: String,
}

/// The classification a protocol is deployed at, as the factory's `uint8`.
fn ceiling_of(spec_classification: &str) -> Option<u8> {
    Some(match crate::store::classification_from_str(spec_classification)? {
        quorum_tenancy::Classification::Public => 0,
        quorum_tenancy::Classification::Proprietary => 1,
        quorum_tenancy::Classification::Cui => 2,
        quorum_tenancy::Classification::Itar => 3,
    })
}

/// The action class a spec's structural scope clause names.
///
/// `PolicyBinding` keys on `keccak256(action class)`, and the class itself is a
/// string an operator wrote ("repo.write"). Returned as written so the surface
/// can show it; the hashing happens at bind time.
pub fn action_class_of(compiled: &Compiled) -> Option<String> {
    compiled
        .structural
        .iter()
        .find(|s| s.kind == "scope")
        .map(|s| s.value.trim().to_string())
        .filter(|v| !v.is_empty())
}

/// The principals a spec's structural clause names.
pub fn principals_of(compiled: &Compiled) -> Vec<String> {
    compiled
        .structural
        .iter()
        .filter(|s| s.kind == "principals")
        .flat_map(|s| crate::ctor::principals_of(&s.value))
        .collect()
}

/// `governance.deployIntent` — stages 6/7, phase one.
///
/// **Signs nothing. Sends nothing.** Builds the transaction, asks the FACTORY
/// for the address it will produce, and enqueues a ceremony the human approves
/// separately.
///
/// # Every gate the chain enforces is checked here first
///
/// GF-2 (audited bytecode), GF-4 (the tenant exists, the sender administers it,
/// the ceiling fits) and the constructor's own requires all fail on chain, and
/// on chain they fail AFTER a human has approved. A ceremony that ends in a
/// revert teaches an operator that approving is a gamble, so each of those is
/// answered before the ceremony is enqueued, and the whole call is finally
/// dry-run against the node as the operator's own address.
#[tauri::command]
pub fn governance_deploy_intent(
    app: tauri::AppHandle,
    ceremony: tauri::State<'_, citrate_core_kit::ceremony::CeremonyState>,
    custody: tauri::State<'_, citrate_core_kit::custody::CustodyState>,
    spec_id: String,
) -> Result<DeployIntentDto, String> {
    use citrate_core_kit::ceremony::{IntentKind, SignatureIntent};
    use tauri::Manager;

    let root = app
        .path()
        .app_data_dir()
        .map_err(|e| format!("no app data dir: {e}"))?;
    let book = crate::addresses::AddressBook::load().map_err(|e| e.to_string())?;
    let bfr = crate::addresses::AddressBook::load_bfr().map_err(|e| e.to_string())?;
    let factory = book
        .get("GovernanceProtocolFactory")
        .ok_or("GovernanceProtocolFactory is not in the address book")?
        .to_string();
    let registry = book
        .get("GovernanceTemplateRegistry")
        .ok_or("GovernanceTemplateRegistry is not in the address book")?;
    let rpc = crate::chain::Rpc::from_book(&book)?;

    let store = crate::store::EvidenceStore::open(crate::store::evidence_dir(&root))
        .map_err(|e| format!("evidence store: {e}"))?;
    let tenant = store.load_scope().ok_or("no tenant scope is established")?;

    // ── Who, and where ──────────────────────────────────────────────
    //
    // The tenant id is keccak256 of the name, because that is what
    // TenantHierarchy keys on. It was blake3 of the scope string until now,
    // which is the evidence store's hash for its own directories — a perfectly
    // good 32 bytes naming a tenant that has never existed on any chain.
    let tenant_id_bytes = crate::chain::tenant_id_of(&tenant);
    let tenant_id = format!("0x{}", hex::encode(tenant_id_bytes));
    let node = crate::chain::tenant_node(&bfr, tenant_id_bytes)?.ok_or_else(|| {
        DeployError::NoSuchTenant {
            tenant: tenant.clone(),
            id: tenant_id.clone(),
        }
        .why()
    })?;

    let from = citrate_core_kit::wallet::address(&custody.0).map_err(|e| e.to_string())?;
    if !node.is_admin(&from.address) {
        return Err(DeployError::NotTenantAdmin {
            tenant: tenant.clone(),
            operator: from.address.clone(),
            admins: node.admins.clone(),
        }
        .why());
    }

    // ── What, and at what classification ────────────────────────────
    let meta = crate::ingest::load_meta(&root, &spec_id).ok_or_else(|| {
        DeployError::NoClassification {
            spec_id: spec_id.clone(),
        }
        .why()
    })?;
    let ceiling = ceiling_of(&meta.classification)
        .ok_or_else(|| format!("{spec_id} carries an unreadable classification: {}", meta.classification))?;
    if ceiling > node.classification_max {
        return Err(DeployError::CeilingExceedsTenant {
            spec: meta.classification.clone(),
            tenant: crate::store::classification_str(
                crate::store::classification_from_str(
                    match node.classification_max {
                        0 => "public",
                        1 => "proprietary",
                        2 => "cui",
                        _ => "itar",
                    },
                )
                .unwrap_or(quorum_tenancy::Classification::Public),
            )
            .to_string(),
        }
        .why());
    }

    let catalog = crate::compile::catalog_from_chain(&rpc, registry)?;
    let interview = crate::interview::load(&root, &spec_id);
    let spec = crate::spec::draft(&spec_id, "Untitled policy", &interview);
    let compiled = crate::compile::compile(&spec, &catalog);

    let (template_name, template_id, params) = target(&compiled).map_err(|e| e.why())?;
    let code = creation_code(template_name).map_err(|e| e.why())?;

    let spec_hash = format!(
        "0x{}",
        hex::encode(blake3::hash(spec_id.as_bytes()).as_bytes())
    );
    let spec_cid = format!("spec://{spec_id}");
    let salt = salt_for(&tenant_id, &spec_id, template_id);

    // ── The constructor arguments, for real ─────────────────────────
    //
    // These are hashed into the predicted address. Passing `&[]` (as this did
    // until now) predicts, and shows a human, the address of a contract whose
    // constructor reverts on its first require.
    let approver_names = principals_of(&compiled);
    let ctor_params = crate::ctor::encode(
        template_name,
        &crate::ctor::DeployContext {
            tenant_id: tenant_id_bytes,
            template_id: word_bytes(template_id)?,
            version: 1,
            spec_hash: word_bytes(&spec_hash)?,
            spec_cid: &spec_cid,
            params,
            principals: &approver_names,
            bfr: &bfr,
        },
    )
    .map_err(|e| e.why())?;

    // The address is the FACTORY's answer, not a local CREATE2. S6.2 proved by
    // negative control that `predict` and the real deployment are the same
    // computation; asking the contract inherits that rather than re-earning it.
    let call = encode_predict(&code, &ctor_params, &salt);
    let ret = rpc.eth_call(&factory, &call)?;
    if ret.len() < 32 {
        return Err("the factory did not return an address for this deployment".to_string());
    }
    let predicted = format!("0x{}", hex::encode(&ret[12..32]));

    let data = encode_deploy_protocol(
        &tenant_id,
        template_id,
        &code,
        &ctor_params,
        &spec_hash,
        &spec_cid,
        ceiling,
        &salt,
        &spec_hash,
    );

    // ── The dry run ─────────────────────────────────────────────────
    //
    // As the operator, so GF-4's msg.sender check is real: a `from`-less
    // eth_call runs as the zero address and passes nothing. The factory returns
    // the deployed address, so a successful dry run also proves the prediction
    // — two answers from the same node, one of which is what a human is about
    // to approve.
    let dry = rpc
        .eth_call_from(&from.address, &factory, &data)
        .map_err(|e| {
            format!(
                "the factory refuses this deployment, so no ceremony was raised: {e}. \
                 Nothing was signed and nothing was sent"
            )
        })?;
    if dry.len() >= 32 {
        let would_be = format!("0x{}", hex::encode(&dry[12..32]));
        if !would_be.eq_ignore_ascii_case(&predicted) {
            return Err(format!(
                "predict() says {predicted} but a dry run of the real call deploys to \
                 {would_be}. These are the same computation on chain, so they \
                 disagreeing means this app built two different transactions — \
                 refusing rather than showing a human either one"
            ));
        }
    }

    let tx = format!(
        r#"{{"from":"{}","to":"{}","value":"0x0","data":"{}"}}"#,
        from.address, factory, data
    );

    // What the ceremony itself will render. It does not ABI-decode, and it is
    // right not to invent a friendlier summary — so this is repeated back here
    // and shown beside the governance detail, rather than dressed up.
    let ceremony_action = format!(
        "Call {} with {} bytes calldata (value 0 wei)",
        factory,
        (data.len() - 2) / 2
    );

    let view = ceremony.0.request(SignatureIntent {
        origin: format!("governance · deploy {template_name}"),
        kind: IntentKind::Transaction,
        chain_id: book.chain_id,
        raw: tx,
    });

    let dto = DeployIntentDto {
        ceremony_id: view.id,
        spec_id,
        predicted_address: predicted,
        template_id: template_id.to_string(),
        template_name: template_name.to_string(),
        tenant_id,
        tenant_name: node.display_name.clone(),
        spec_hash,
        spec_cid,
        salt,
        ceremony_action,
        classification: meta.classification.clone(),
        approvers: approver_names,
        action_class: action_class_of(&compiled),
        source: format!(
            "GovernanceProtocolFactory.predict/deployProtocol at {factory} · {} · chain {} · {}",
            node.source,
            book.chain_id,
            rpc.url()
        ),
    };

    // Persisted BEFORE the human is asked, so phase two compares against what
    // was actually shown rather than against a re-derivation. A re-derivation
    // would agree with itself no matter what changed in between, which is the
    // one thing the address check must not do.
    save_intent(&root, &dto)?;
    Ok(dto)
}

/// Parse a `0x`-prefixed 32-byte word.
fn word_bytes(s: &str) -> Result<[u8; 32], String> {
    let body = s.strip_prefix("0x").unwrap_or(s);
    let raw = hex::decode(body).map_err(|_| format!("not hex: {s}"))?;
    if raw.len() != 32 {
        return Err(format!("not a 32-byte word: {s}"));
    }
    let mut w = [0u8; 32];
    w.copy_from_slice(&raw);
    Ok(w)
}

fn intent_path(root: &std::path::Path, ceremony_id: &str) -> std::path::PathBuf {
    // The ceremony id is minted by the kit, but it reaches here as a string from
    // a command argument in phase two, so it is not allowed to shape a path.
    let safe: String = ceremony_id
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '-' || *c == '_')
        .collect();
    root.join("deploy-intents").join(format!("{safe}.json"))
}

pub fn save_intent(root: &std::path::Path, dto: &DeployIntentDto) -> Result<(), String> {
    let path = intent_path(root, &dto.ceremony_id);
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    let body = serde_json::to_vec_pretty(dto).map_err(|e| e.to_string())?;
    std::fs::write(&path, body).map_err(|e| e.to_string())
}

pub fn load_intent(root: &std::path::Path, ceremony_id: &str) -> Option<DeployIntentDto> {
    let raw = std::fs::read_to_string(intent_path(root, ceremony_id)).ok()?;
    serde_json::from_str(&raw).ok()
}

// ── Phase two: broadcast, and check what the chain actually built ───────

/// `topic0` of
/// `ProtocolDeployed(bytes32,address,bytes32,bytes32,string,address,bytes32)`.
///
/// Pinned as a literal and asserted against a keccak of the signature below, so
/// a change to the event signature in citrate-chain breaks a test here rather
/// than silently making every receipt look like it deployed nothing.
pub const PROTOCOL_DEPLOYED_TOPIC: &str =
    "0x3123b75b509b06f006e35cf452775f74eabbbca622ae2f10b3a6f57269878e45";

#[derive(Debug, PartialEq, Eq)]
pub enum CompleteError {
    /// Phase two was called for a ceremony phase one never recorded.
    NoIntent { ceremony_id: String },
    /// The transaction was mined and reverted.
    Reverted { tx_hash: String },
    /// Mined, but the factory logged no deployment.
    NoDeploymentLogged { tx_hash: String },
    /// **The check this whole phase exists for.**
    AddressMismatch {
        predicted: String,
        actual: String,
        tx_hash: String,
    },
    /// The chain agrees on the address but there is no code at it.
    NoCodeAtAddress { address: String },
}

impl CompleteError {
    pub fn why(&self) -> String {
        match self {
            CompleteError::NoIntent { ceremony_id } => format!(
                "no deploy intent was recorded for ceremony {ceremony_id}, so there is \
                 nothing to check the deployed address against. Refusing to broadcast: \
                 a deploy whose predicted address cannot be compared is a deploy nobody \
                 approved the address of"
            ),
            CompleteError::Reverted { tx_hash } => format!(
                "the deploy transaction {tx_hash} was mined and REVERTED. Nothing was \
                 deployed. The dry run in phase one should have caught this, so treat a \
                 revert here as chain state having changed underneath the ceremony"
            ),
            CompleteError::NoDeploymentLogged { tx_hash } => format!(
                "{tx_hash} succeeded but the factory logged no ProtocolDeployed event. \
                 Something was executed and it was not the deployment this ceremony \
                 described"
            ),
            CompleteError::AddressMismatch {
                predicted,
                actual,
                tx_hash,
            } => format!(
                "THE DEPLOYED ADDRESS DOES NOT MATCH WHAT WAS APPROVED. A human approved \
                 {predicted}; {tx_hash} deployed {actual}. The protocol at {actual} is \
                 not the one anyone signed for and must not be bound, cited, or \
                 recorded as governing anything"
            ),
            CompleteError::NoCodeAtAddress { address } => format!(
                "the chain reports a deployment at {address} but there is no code there. \
                 Nothing usable exists at that address"
            ),
        }
    }
}

/// What the chain says was deployed, having been asked independently.
#[derive(Serialize, Debug, Clone)]
pub struct DeployResultDto {
    pub tx_hash: String,
    pub block_number: Option<u64>,
    /// The address the FACTORY logged — the chain's own answer, taken from the
    /// receipt rather than recomputed here.
    pub address: String,
    /// The address the human was shown before signing.
    pub predicted_address: String,
    pub spec_id: String,
    pub template_name: String,
    pub tenant_id: String,
    pub tenant_name: String,
    pub classification: String,
    /// What S7.7 will bind this to. `None` when the spec named no scope.
    pub action_class: Option<String>,
    /// Bytes of deployed code, read back from the chain.
    pub code_size: usize,
    pub source: String,
}

/// The protocol address in a receipt's `ProtocolDeployed` log.
///
/// The address is the SECOND indexed parameter, so it is `topics[2]` — read
/// from the log rather than from anything this app computed. That independence
/// is the point: comparing a prediction against a re-derivation of itself would
/// agree no matter what went wrong in between.
pub fn deployed_address_in(receipt: &serde_json::Value, factory: &str) -> Option<String> {
    let logs = receipt.get("logs")?.as_array()?;
    for log in logs {
        let addr = log.get("address")?.as_str()?;
        if !addr.eq_ignore_ascii_case(factory) {
            continue;
        }
        let topics = log.get("topics")?.as_array()?;
        let t0 = topics.first()?.as_str()?;
        if !t0.eq_ignore_ascii_case(PROTOCOL_DEPLOYED_TOPIC) {
            continue;
        }
        let t2 = topics.get(2)?.as_str()?;
        let body = t2.strip_prefix("0x").unwrap_or(t2);
        if body.len() != 64 {
            return None;
        }
        return Some(format!("0x{}", &body[24..]));
    }
    None
}

/// Whether a receipt reports success. Absent status is treated as FAILURE:
/// pre-Byzantium receipts have no status field, and this chain is not one, so an
/// absent status means a receipt shape we do not understand.
fn receipt_succeeded(receipt: &serde_json::Value) -> bool {
    matches!(
        receipt.get("status").and_then(|s| s.as_str()),
        Some("0x1") | Some("0x01")
    )
}

/// `governance.deployComplete` — stages 6/7, phase two.
///
/// **Signs nothing and broadcasts nothing.** The ceremony's own
/// `sign_and_broadcast` is the single path that signs (rule 3), and adding a
/// second one here would be a second signing path in the app whose whole claim
/// is that there is one. This takes the hash that path produced and asks the
/// chain what it actually built.
///
/// **A mismatch is a hard error.** There is no field on the success type for
/// "the address differed"; the only way to learn the address matched is to get
/// an `Ok` back. A warning would be worse than nothing — it would put the
/// wrong protocol's address into the ledger with a note beside it.
#[tauri::command]
pub fn governance_deploy_complete(
    app: tauri::AppHandle,
    ceremony_id: String,
    tx_hash: String,
) -> Result<DeployResultDto, String> {
    use tauri::Manager;

    let root = app
        .path()
        .app_data_dir()
        .map_err(|e| format!("no app data dir: {e}"))?;

    // Loaded FIRST. A ceremony with no recorded intent has no approved address
    // to check against, so there is nothing this call could honestly report.
    let intent = load_intent(&root, &ceremony_id).ok_or_else(|| {
        CompleteError::NoIntent {
            ceremony_id: ceremony_id.clone(),
        }
        .why()
    })?;

    let book = crate::addresses::AddressBook::load().map_err(|e| e.to_string())?;
    let factory = book
        .get("GovernanceProtocolFactory")
        .ok_or("GovernanceProtocolFactory is not in the address book")?
        .to_string();
    let rpc = crate::chain::Rpc::from_book(&book)?;

    // ── What the chain built ────────────────────────────────────────
    let receipt = rpc
        .receipt(&tx_hash)?
        .ok_or_else(|| format!(
            "no receipt for {tx_hash}. It may still be mined — check the hash rather \
             than re-running this"
        ))?;
    if !receipt_succeeded(&receipt) {
        return Err(CompleteError::Reverted {
            tx_hash: tx_hash.clone(),
        }
        .why());
    }
    // The hash arrives as an argument, so it is not allowed to name just any
    // transaction: this one has to be a call to the factory. Without this a
    // caller could point phase two at some unrelated successful tx.
    if !receipt
        .get("to")
        .and_then(|t| t.as_str())
        .is_some_and(|t| t.eq_ignore_ascii_case(&factory))
    {
        return Err(format!(
            "{tx_hash} is not a call to GovernanceProtocolFactory ({factory}), so it \
             cannot be the deployment this ceremony described"
        ));
    }

    let actual = deployed_address_in(&receipt, &factory).ok_or_else(|| {
        CompleteError::NoDeploymentLogged {
            tx_hash: tx_hash.clone(),
        }
        .why()
    })?;

    if !actual.eq_ignore_ascii_case(&intent.predicted_address) {
        return Err(CompleteError::AddressMismatch {
            predicted: intent.predicted_address.clone(),
            actual,
            tx_hash: tx_hash.clone(),
        }
        .why());
    }

    let block_number = receipt
        .get("blockNumber")
        .and_then(|b| b.as_str())
        .and_then(|s| u64::from_str_radix(s.trim_start_matches("0x"), 16).ok());

    // And there is really code there. The factory logging an address and the
    // address holding a contract are two different claims.
    let code = rpc.call(
        "eth_getCode",
        serde_json::json!([actual, "latest"]),
    )?;
    let code_size = code
        .as_str()
        .map(|s| s.trim_start_matches("0x").len() / 2)
        .unwrap_or(0);
    if code_size == 0 {
        return Err(CompleteError::NoCodeAtAddress { address: actual }.why());
    }

    let dto = DeployResultDto {
        tx_hash,
        block_number,
        address: actual,
        predicted_address: intent.predicted_address,
        spec_id: intent.spec_id,
        template_name: intent.template_name,
        tenant_id: intent.tenant_id,
        tenant_name: intent.tenant_name,
        classification: intent.classification,
        action_class: intent.action_class,
        code_size,
        source: format!(
            "GovernanceProtocolFactory.ProtocolDeployed log at {factory} · \
             eth_getTransactionReceipt · eth_getCode · chain {} · {}",
            book.chain_id,
            rpc.url()
        ),
    };

    // Recorded only NOW — after the chain confirmed the deployment and the
    // address matched. Writing this when the ceremony was raised would mark a
    // spec `deployed` on the strength of a human having been asked.
    let body = serde_json::to_string_pretty(&dto).map_err(|e| e.to_string())?;
    crate::protocols::record_stage(&root, "deployed", &dto.spec_id, &body)?;
    Ok(dto)
}

/// `predict(bytes,bytes,bytes32)` calldata.
fn encode_predict(creation_code: &[u8], params: &[u8], salt: &str) -> String {
    let cc = pad_bytes(creation_code);
    let pp = pad_bytes(params);
    let head = 3 * 32;
    let off_cc = head;
    let off_pp = off_cc + 32 + cc.len() / 2;

    let mut out = String::from("0x23828ab5");
    out.push_str(&format!("{off_cc:064x}"));
    out.push_str(&format!("{off_pp:064x}"));
    out.push_str(&word_hex(salt));
    out.push_str(&format!("{:064x}", creation_code.len()));
    out.push_str(&cc);
    out.push_str(&format!("{:064x}", params.len()));
    out.push_str(&pp);
    out
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use crate::compile::{Mapped, Structural, Unmapped};

    fn mapped(name: &str) -> Mapped {
        Mapped {
            clause: "1".into(),
            template_id: "0x2222222222222222222222222222222222222222222222222222222222222222".into(),
            template_name: name.into(),
            params: BTreeMap::from([("threshold".to_string(), "2".to_string())]),
        }
    }

    // ── The encoder, pinned against cast ────────────────────────────

    /// Pinned against `cast calldata "deployProtocol(...)"`. A hand-rolled ABI
    /// encoder that is subtly wrong produces a well-formed transaction that
    /// deploys something nobody described — and it would sail through a
    /// ceremony, because the ceremony cannot decode it either.
    #[test]
    fn the_encoder_matches_cast() {
        let got = encode_deploy_protocol(
            "0x1111111111111111111111111111111111111111111111111111111111111111",
            "0x2222222222222222222222222222222222222222222222222222222222222222",
            &[0xaa, 0xbb],
            &[0xcc, 0xdd],
            "0x3333333333333333333333333333333333333333333333333333333333333333",
            "bafySpec",
            2,
            "0x4444444444444444444444444444444444444444444444444444444444444444",
            "0x5555555555555555555555555555555555555555555555555555555555555555",
        );
        // Produced by:
        //   cast calldata "deployProtocol(bytes32,bytes32,bytes,bytes,bytes32,string,uint8,bytes32,bytes32)" \
        //     0x1111… 0x2222… 0xaabb 0xccdd 0x3333… bafySpec 2 0x4444… 0x5555…
        let want = concat!(
            "0x3b945b77",
            "1111111111111111111111111111111111111111111111111111111111111111",
            "2222222222222222222222222222222222222222222222222222222222222222",
            "0000000000000000000000000000000000000000000000000000000000000120",
            "0000000000000000000000000000000000000000000000000000000000000160",
            "3333333333333333333333333333333333333333333333333333333333333333",
            "00000000000000000000000000000000000000000000000000000000000001a0",
            "0000000000000000000000000000000000000000000000000000000000000002",
            "4444444444444444444444444444444444444444444444444444444444444444",
            "5555555555555555555555555555555555555555555555555555555555555555",
            "0000000000000000000000000000000000000000000000000000000000000002",
            "aabb000000000000000000000000000000000000000000000000000000000000",
            "0000000000000000000000000000000000000000000000000000000000000002",
            "ccdd000000000000000000000000000000000000000000000000000000000000",
            "0000000000000000000000000000000000000000000000000000000000000008",
            "6261667953706563000000000000000000000000000000000000000000000000",
        );
        assert_eq!(got, want, "encoder drifted from cast's encoding");
    }

    /// The `predict` encoder, pinned against cast. This is the call whose
    /// answer is shown to a human before they approve — an encoder that is
    /// wrong here shows them the address of something else.
    #[test]
    fn the_predict_encoder_matches_cast() {
        let got = encode_predict(
            &[0xaa, 0xbb],
            &[0xcc, 0xdd],
            "0x4444444444444444444444444444444444444444444444444444444444444444",
        );
        let want = concat!(
            "0x23828ab5",
            "0000000000000000000000000000000000000000000000000000000000000060",
            "00000000000000000000000000000000000000000000000000000000000000a0",
            "4444444444444444444444444444444444444444444444444444444444444444",
            "0000000000000000000000000000000000000000000000000000000000000002",
            "aabb000000000000000000000000000000000000000000000000000000000000",
            "0000000000000000000000000000000000000000000000000000000000000002",
            "ccdd000000000000000000000000000000000000000000000000000000000000",
        );
        assert_eq!(got, want, "predict encoder drifted from cast's encoding");
    }

    #[test]
    fn word_hex_accepts_prefixed_and_bare() {
        assert_eq!(word_hex("0x01").len(), 64);
        assert_eq!(word_hex("01"), word_hex("0x01"));
        assert!(word_hex("0xff").ends_with("ff"));
    }

    // ── The two refusals ────────────────────────────────────────────

    /// **R-A carried into the ceremony.** A spec with an unmapped clause must
    /// not reach a human for approval — the ceremony is where responsibility is
    /// taken, and it must not be taken for a policy that is missing a piece.
    #[test]
    fn an_undeployable_spec_cannot_produce_an_intent() {
        let c = Compiled {
            spec_id: "s".into(),
            mapped: vec![mapped("ThresholdApproval")],
            unmapped: vec![Unmapped { clause: "2".into(), why: "no template".into() }],
            ..Default::default()
        };
        let e = target(&c).expect_err("must refuse");
        assert_eq!(e, DeployError::NotDeployable { unmapped: 1 });
        assert!(e.why().contains("missing a piece"), "{}", e.why());
    }

    /// A spec that mapped nothing has no protocol to deploy — distinct from an
    /// unmapped clause, and it gets its own sentence.
    #[test]
    fn a_spec_with_nothing_mapped_is_refused_separately() {
        let c = Compiled { spec_id: "s".into(), ..Default::default() };
        assert_eq!(target(&c).expect_err("refuse"), DeployError::NothingToDeploy);
    }

    /// **The property the whole vendoring step exists for.** Every embedded
    /// artifact must still hash to the `initCodeHash` registered on chain.
    ///
    /// If this fails, the app is carrying bytecode that the factory will refuse
    /// — and it would refuse it mid-ceremony, after a human had already been
    /// asked to approve a deploy. Catching it here costs a test run; catching it
    /// there costs the operator's trust in the ceremony.
    #[test]
    fn every_embedded_artifact_hashes_to_what_the_registry_pinned() {
        use crate::template_hashes::PINNED_INIT_CODE_HASHES;
        assert_eq!(
            PINNED_INIT_CODE_HASHES.len(),
            EMBEDDED.len(),
            "the pinned table and the embedded set disagree about how many templates exist"
        );
        for (name, expected) in PINNED_INIT_CODE_HASHES {
            let code = creation_code(name)
                .unwrap_or_else(|e| panic!("{name} is pinned but not embedded: {}", e.why()));
            // keccak over the RAW BYTES, which is what
            // `keccak256(type(T).creationCode)` hashes in Solidity and what
            // `cast keccak 0x…` computes (it decodes the hex first). Hashing the
            // ASCII hex instead produces a plausible 32 bytes that match
            // nothing — this test was written that way first and caught it.
            let got = format!("0x{}", hex::encode(crate::anchor::keccak256(&code)));
            assert_eq!(
                &got.as_str(),
                expected,
                "{name}: embedded bytecode does not match the pinned initCodeHash"
            );
        }
    }

    /// The pinned table says where its values came from. A build-sourced table
    /// must name the chain revision and the toolchain, or
    /// `scripts/verify-template-hashes.sh` has nothing to rebuild against.
    #[test]
    fn the_pinned_hashes_name_their_source() {
        use crate::template_hashes::{
            BUILT_FROM_CHAIN_REV, CHAIN_REV, FORGE_VERSION, PINNED_INIT_CODE_HASHES, SOLC_VERSION,
        };
        assert_eq!(CHAIN_REV.len(), 40, "CHAIN_REV is a full commit sha");
        assert!(CHAIN_REV.chars().all(|c| c.is_ascii_hexdigit()));
        assert!(!FORGE_VERSION.is_empty());
        assert_eq!(SOLC_VERSION, "0.8.36", "citrate-chain CI compiles with solc 0.8.36");
        // Every build-sourced name is a pinned template, named once.
        for (i, b) in BUILT_FROM_CHAIN_REV.iter().enumerate() {
            assert!(
                PINNED_INIT_CODE_HASHES.iter().any(|(n, _)| n == b),
                "{b} is marked build-sourced but has no pinned row"
            );
            assert!(!BUILT_FROM_CHAIN_REV[i + 1..].contains(b), "{b} listed twice");
        }
    }

    /// A template this app does not carry is refused BY NAME, rather than
    /// handed to the factory as bytes that will fail GF-2 in front of a human.
    #[test]
    fn an_uncarried_template_is_refused_by_name() {
        let e = creation_code("NotATemplate").expect_err("refuse");
        match &e {
            DeployError::NoCreationCode { template } => assert_eq!(template, "NotATemplate"),
            other => panic!("expected NoCreationCode, got {other:?}"),
        }
        assert!(e.why().contains("will not invent it"), "{}", e.why());
    }

    /// The artifacts are real bytecode, not placeholders — every one decodes and
    /// starts with a constructor preamble.
    #[test]
    fn the_embedded_artifacts_are_real_bytecode() {
        for (name, _) in EMBEDDED {
            let code = creation_code(name).expect("embedded");
            assert!(code.len() > 1_000, "{name} is suspiciously small: {}", code.len());
            // `via_ir` output begins with PUSH2 (0x61), not PUSH1. Accept the
            // whole PUSH family rather than pinning one opcode a compiler
            // setting can change.
            assert!(
                (0x60..=0x7f).contains(&code[0]),
                "{name} does not start with a PUSH opcode: 0x{:02x}",
                code[0]
            );
        }
    }

    // ── The salt ────────────────────────────────────────────────────

    /// Deterministic, so re-running an intent for the same spec predicts the
    /// same address rather than a new one each time.
    #[test]
    fn the_salt_is_deterministic() {
        let a = salt_for("t", "spec-1", "0x01");
        assert_eq!(a, salt_for("t", "spec-1", "0x01"));
        assert_eq!(a.len(), 66);
    }

    /// Distinct per tenant: two tenants deploying the same template from the
    /// same spec must not collide. GF-5 would refuse the second, which would
    /// look like a bug rather than a namespace collision.
    #[test]
    fn the_salt_separates_tenants_and_templates() {
        assert_ne!(salt_for("t1", "spec-1", "0x01"), salt_for("t2", "spec-1", "0x01"));
        assert_ne!(salt_for("t", "spec-1", "0x01"), salt_for("t", "spec-1", "0x02"));
        assert_ne!(salt_for("t", "spec-1", "0x01"), salt_for("t", "spec-2", "0x01"));
    }

    /// One protocol per ceremony: the target is the first mapped clause, so the
    /// predicted address shown is the address of the thing being approved.
    #[test]
    fn the_target_is_a_single_protocol() {
        let c = Compiled {
            spec_id: "s".into(),
            mapped: vec![mapped("ThresholdApproval"), mapped("ClassificationGate")],
            ..Default::default()
        };
        let (name, _, _) = target(&c).expect("target");
        assert_eq!(name, "ThresholdApproval");
    }

    // ── Phase two: what the chain actually built ────────────────────

    const FACTORY: &str = "0x260ffedd17cd05a2e4daa41e1339c19a083f9e57";

    /// A receipt shaped like the node's, with one `ProtocolDeployed` log.
    fn receipt_with(protocol: &str, status: &str, from_addr: &str) -> serde_json::Value {
        serde_json::json!({
            "status": status,
            "blockNumber": "0x1234",
            "logs": [
                // An unrelated log first, so "find the right one" is load-bearing
                // rather than "take the first log there is".
                {
                    "address": "0x00000000000000000000000000000000000000ff",
                    "topics": ["0xdeadbeef00000000000000000000000000000000000000000000000000000000"],
                },
                {
                    "address": from_addr,
                    "topics": [
                        PROTOCOL_DEPLOYED_TOPIC,
                        "0x1111111111111111111111111111111111111111111111111111111111111111",
                        format!("0x000000000000000000000000{}", protocol.trim_start_matches("0x")),
                        "0x2222222222222222222222222222222222222222222222222222222222222222",
                    ],
                }
            ]
        })
    }

    /// The topic literal must be the keccak of the event signature it claims to
    /// be. Pinning a wrong constant would make every receipt look as though it
    /// deployed nothing — `NoDeploymentLogged` for a perfectly good deploy.
    #[test]
    fn the_event_topic_is_the_keccak_of_the_signature() {
        let want = format!(
            "0x{}",
            hex::encode(crate::anchor::keccak256(
                b"ProtocolDeployed(bytes32,address,bytes32,bytes32,string,address,bytes32)"
            ))
        );
        assert_eq!(PROTOCOL_DEPLOYED_TOPIC, want);
    }

    /// The address comes from `topics[2]` — the event's SECOND indexed
    /// parameter. Reading `topics[1]` instead yields the tenant id, which
    /// truncates to a plausible-looking address and would fail the comparison
    /// for a reason that has nothing to do with the deployment.
    #[test]
    fn the_deployed_address_is_read_from_the_log() {
        let r = receipt_with("0xabababababababababababababababababababab", "0x1", FACTORY);
        assert_eq!(
            deployed_address_in(&r, FACTORY).as_deref(),
            Some("0xabababababababababababababababababababab")
        );
    }

    /// A `ProtocolDeployed` log emitted by something that is not the factory is
    /// not this factory's deployment. Anyone may emit an event with that
    /// signature, so the log's `address` has to be checked.
    #[test]
    fn a_log_from_another_contract_is_not_our_deployment() {
        let r = receipt_with(
            "0xabababababababababababababababababababab",
            "0x1",
            "0x00000000000000000000000000000000000000aa",
        );
        assert_eq!(deployed_address_in(&r, FACTORY), None);
    }

    /// A receipt with no matching log yields nothing rather than a guess.
    #[test]
    fn a_receipt_without_the_event_deploys_nothing() {
        let r = serde_json::json!({ "status": "0x1", "logs": [] });
        assert_eq!(deployed_address_in(&r, FACTORY), None);
    }

    /// **Fail closed on an unfamiliar receipt.** A missing `status` is not
    /// success — it means a receipt shape this app does not understand, and
    /// treating it as success would report a deployment for a transaction whose
    /// outcome is unknown.
    #[test]
    fn a_receipt_without_a_status_is_not_a_success() {
        assert!(receipt_succeeded(&serde_json::json!({ "status": "0x1" })));
        assert!(receipt_succeeded(&serde_json::json!({ "status": "0x01" })));
        assert!(!receipt_succeeded(&serde_json::json!({ "status": "0x0" })));
        assert!(!receipt_succeeded(&serde_json::json!({})));
        assert!(!receipt_succeeded(&serde_json::json!({ "status": 1 })));
    }

    /// **The check phase two exists for.** A mismatch is an error with no
    /// success value attached — there is no way for a caller to receive an
    /// address and a warning, because a warning beside a wrong address is how
    /// the wrong address ends up in the ledger.
    #[test]
    fn an_address_mismatch_is_an_error_that_names_both() {
        let e = CompleteError::AddressMismatch {
            predicted: "0xaaaa000000000000000000000000000000000000".into(),
            actual: "0xbbbb000000000000000000000000000000000000".into(),
            tx_hash: "0xfeed".into(),
        };
        let why = e.why();
        assert!(why.contains("0xaaaa000000000000000000000000000000000000"), "{why}");
        assert!(why.contains("0xbbbb000000000000000000000000000000000000"), "{why}");
        assert!(why.contains("must not be bound"), "{why}");
    }

    /// Case is not identity. A checksummed prediction and a lowercase log entry
    /// are the same address, and reporting them as a mismatch would refuse a
    /// correct deployment — the failure mode that makes people disable a check.
    #[test]
    fn address_comparison_ignores_checksum_case() {
        let predicted = "0xABababABababABababABababABababABababABab";
        let actual = "0xabababababababababababababababababababab";
        assert!(actual.eq_ignore_ascii_case(predicted));
    }

    // ── The intent, persisted ───────────────────────────────────────

    fn dto(ceremony_id: &str, predicted: &str) -> DeployIntentDto {
        DeployIntentDto {
            ceremony_id: ceremony_id.into(),
            spec_id: "spec-1".into(),
            predicted_address: predicted.into(),
            template_id: "0x22".into(),
            template_name: "ThresholdApproval".into(),
            tenant_id: "0x11".into(),
            tenant_name: "Citrate".into(),
            spec_hash: "0x33".into(),
            spec_cid: "spec://spec-1".into(),
            salt: "0x44".into(),
            ceremony_action: "Call … with N bytes calldata".into(),
            classification: "CUI".into(),
            approvers: vec!["R. Ortiz".into()],
            action_class: Some("repo.write".into()),
            source: "test".into(),
        }
    }

    /// Phase two compares against what phase one WROTE DOWN, not against a
    /// fresh derivation. A re-derivation would agree with itself however the
    /// inputs changed in between, which is exactly the failure the comparison
    /// is supposed to catch.
    #[test]
    fn the_intent_round_trips_through_disk() {
        let dir = std::env::temp_dir().join(format!("qrm-deploy-{}", std::process::id()));
        let d = dto("cer-1", "0xaaaa000000000000000000000000000000000000");
        save_intent(&dir, &d).expect("save");
        let back = load_intent(&dir, "cer-1").expect("load");
        assert_eq!(back.predicted_address, d.predicted_address);
        assert_eq!(back.action_class.as_deref(), Some("repo.write"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A ceremony id arriving as a command argument must not shape a path. It
    /// is filtered, not trusted — the alternative is a caller reading or
    /// writing outside the intents directory.
    #[test]
    fn a_ceremony_id_cannot_escape_its_directory() {
        let p = intent_path(std::path::Path::new("/root"), "../../etc/passwd");
        assert_eq!(p, std::path::Path::new("/root/deploy-intents/etcpasswd.json"));
    }

    /// Phase two on a ceremony phase one never recorded refuses BEFORE
    /// broadcasting. Broadcasting first and discovering afterwards that there
    /// is nothing to compare against would deploy a contract nobody approved
    /// the address of.
    #[test]
    fn a_ceremony_with_no_recorded_intent_is_refused() {
        let dir = std::env::temp_dir().join(format!("qrm-nointent-{}", std::process::id()));
        assert!(load_intent(&dir, "cer-missing").is_none());
        let e = CompleteError::NoIntent {
            ceremony_id: "cer-missing".into(),
        };
        assert!(e.why().contains("nothing to check"), "{}", e.why());
    }

    // ── The classification ceiling ──────────────────────────────────

    /// The ceiling is the spec's own, not a constant. It was hardcoded to 2
    /// (CUI) for every deployment, which silently inflates a Public policy and
    /// understates an ITAR one — invisible, because the ceiling is a
    /// constructor argument and no surface renders it.
    #[test]
    fn the_ceiling_comes_from_the_classification() {
        assert_eq!(ceiling_of("Public"), Some(0));
        assert_eq!(ceiling_of("Proprietary"), Some(1));
        assert_eq!(ceiling_of("CUI"), Some(2));
        assert_eq!(ceiling_of("ITAR"), Some(3));
        assert_eq!(ceiling_of("Secret"), None);
    }

    // ── The structural clauses the deploy reads ─────────────────────

    #[test]
    fn the_action_class_comes_from_the_scope_clause() {
        let c = Compiled {
            spec_id: "s".into(),
            structural: vec![
                Structural { clause: "1".into(), kind: "principals".into(), value: "R. Ortiz".into() },
                Structural { clause: "2".into(), kind: "scope".into(), value: " repo.write ".into() },
            ],
            ..Default::default()
        };
        assert_eq!(action_class_of(&c).as_deref(), Some("repo.write"));
        assert_eq!(principals_of(&c), vec!["R. Ortiz"]);
    }

    /// A spec with no scope clause yields no action class, rather than a
    /// plausible default. BIND then refuses — binding to a guessed action class
    /// would govern something nobody named.
    #[test]
    fn a_spec_without_a_scope_clause_names_no_action_class() {
        let c = Compiled { spec_id: "s".into(), ..Default::default() };
        assert_eq!(action_class_of(&c), None);
        let blank = Compiled {
            spec_id: "s".into(),
            structural: vec![Structural {
                clause: "1".into(),
                kind: "scope".into(),
                value: "   ".into(),
            }],
            ..Default::default()
        };
        assert_eq!(action_class_of(&blank), None);
    }
}
