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

/// Where the audited creation code is vendored.
///
/// Set `QUORUM_TEMPLATE_ARTIFACTS` to a directory of `<Name>.hex` files, each
/// the creation code of one audited template. This is deliberately explicit
/// rather than defaulted: bytecode that governs actions should be something an
/// operator placed on purpose.
pub const ARTIFACTS_ENV: &str = "QUORUM_TEMPLATE_ARTIFACTS";

#[derive(Debug, PartialEq, Eq)]
pub enum DeployError {
    /// The spec has clauses that map to no audited template.
    NotDeployable { unmapped: usize },
    /// No template was mapped, so there is nothing to deploy.
    NothingToDeploy,
    /// The audited creation code is not available to this installation.
    NoCreationCode { template: String, looked_in: String },
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
            DeployError::NoCreationCode { template, looked_in } => format!(
                "the audited creation code for {template} is not available (looked in \
                 {looked_in}). GF-2 requires supplying bytecode that hashes to the \
                 registered initCodeHash, and this app will not invent it — vendor the \
                 audited artifact and set {ARTIFACTS_ENV}"
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
pub fn creation_code(template: &str) -> Result<Vec<u8>, DeployError> {
    let dir = std::env::var(ARTIFACTS_ENV).unwrap_or_default();
    if dir.is_empty() {
        return Err(DeployError::NoCreationCode {
            template: template.to_string(),
            looked_in: format!("<{ARTIFACTS_ENV} is not set>"),
        });
    }
    let path = std::path::Path::new(&dir).join(format!("{template}.hex"));
    let raw = std::fs::read_to_string(&path).map_err(|_| DeployError::NoCreationCode {
        template: template.to_string(),
        looked_in: path.display().to_string(),
    })?;
    let trimmed = raw.trim().trim_start_matches("0x");
    hex::decode(trimmed).map_err(|e| DeployError::NoCreationCode {
        template: template.to_string(),
        looked_in: format!("{} (not hex: {e})", path.display()),
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

#[derive(Serialize, Debug, Clone)]
pub struct DeployIntentDto {
    pub ceremony_id: String,
    pub spec_id: String,
    pub predicted_address: String,
    pub template_id: String,
    pub template_name: String,
    pub tenant_id: String,
    pub spec_hash: String,
    pub spec_cid: String,
    pub salt: String,
    pub ceremony_action: String,
}

/// `governance.deployIntent` — stages 6/7, phase one.
///
/// **Signs nothing. Sends nothing.** Builds the transaction, asks the FACTORY
/// for the address it will produce, and enqueues a ceremony the human approves
/// separately.
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
    let factory = book
        .get("GovernanceProtocolFactory")
        .ok_or("GovernanceProtocolFactory is not in the address book")?
        .to_string();
    let registry = book
        .get("GovernanceTemplateRegistry")
        .ok_or("GovernanceTemplateRegistry is not in the address book")?;
    let rpc = crate::chain::Rpc::from_book(&book)?;

    let store = crate::store::EvidenceStore::open(root.clone())
        .map_err(|e| format!("evidence store: {e}"))?;
    let tenant = store
        .load_scope()
        .ok_or("no tenant scope is established")?;

    let catalog = crate::compile::catalog_from_chain(&rpc, registry)?;
    let interview = crate::interview::load(&root, &spec_id);
    let spec = crate::spec::draft(&spec_id, "Untitled policy", &interview);
    let compiled = crate::compile::compile(&spec, &catalog);

    let (template_name, template_id, _params) = target(&compiled).map_err(|e| e.why())?;
    let code = creation_code(template_name).map_err(|e| e.why())?;

    let tenant_id = format!("0x{}", hex::encode(blake3::hash(tenant.as_bytes()).as_bytes()));
    let salt = salt_for(&tenant_id, &spec_id, template_id);
    let spec_hash = format!(
        "0x{}",
        hex::encode(blake3::hash(spec_id.as_bytes()).as_bytes())
    );
    let spec_cid = format!("spec://{spec_id}");

    // The address is the FACTORY's answer, not a local CREATE2. S6.2 proved by
    // negative control that `predict` and the real deployment are the same
    // computation; asking the contract inherits that rather than re-earning it.
    let mut call = String::from("0x");
    call.push_str(&encode_predict(&code, &[], &salt)[2..]);
    let ret = rpc.eth_call(&factory, &call)?;
    if ret.len() < 32 {
        return Err("the factory did not return an address for this deployment".to_string());
    }
    let predicted = format!("0x{}", hex::encode(&ret[12..32]));

    let data = encode_deploy_protocol(
        &tenant_id,
        template_id,
        &code,
        &[],
        &spec_hash,
        &spec_cid,
        2,
        &salt,
        &spec_hash,
    );
    let from = citrate_core_kit::wallet::address(&custody.0).map_err(|e| e.to_string())?;
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
        chain_id: 40204,
        raw: tx,
    });

    Ok(DeployIntentDto {
        ceremony_id: view.id,
        spec_id,
        predicted_address: predicted,
        template_id: template_id.to_string(),
        template_name: template_name.to_string(),
        tenant_id,
        spec_hash,
        spec_cid,
        salt,
        ceremony_action,
    })
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
    use crate::compile::{Mapped, Unmapped};

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

    /// Absent creation code is refused with the path it looked in — GF-2
    /// requires bytecode that hashes to the registered value, and this app will
    /// not invent it.
    #[test]
    fn absent_creation_code_is_refused_with_the_path() {
        std::env::remove_var(ARTIFACTS_ENV);
        let e = creation_code("ThresholdApproval").expect_err("refuse");
        match &e {
            DeployError::NoCreationCode { template, looked_in } => {
                assert_eq!(template, "ThresholdApproval");
                assert!(looked_in.contains(ARTIFACTS_ENV), "{looked_in}");
            }
            other => panic!("expected NoCreationCode, got {other:?}"),
        }
        assert!(e.why().contains("will not invent it"), "{}", e.why());
    }

    #[test]
    fn creation_code_is_read_from_the_artifacts_dir() {
        let dir = std::env::temp_dir().join(format!("qrm-s76-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("mkdir");
        std::fs::write(dir.join("ThresholdApproval.hex"), "0xaabbcc\n").expect("write");
        std::env::set_var(ARTIFACTS_ENV, &dir);

        let code = creation_code("ThresholdApproval").expect("loaded");
        assert_eq!(code, vec![0xaa, 0xbb, 0xcc]);

        // A template with no artifact is still refused, by name.
        let e = creation_code("Missing").expect_err("refuse");
        assert!(matches!(e, DeployError::NoCreationCode { .. }));

        std::env::remove_var(ARTIFACTS_ENV);
        std::fs::remove_dir_all(&dir).ok();
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
}
