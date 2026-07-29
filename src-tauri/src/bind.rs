//! QRM-S7.7 — BIND: the stage where a deployed protocol starts governing.
//!
//! Stage 8, and the last one. A protocol deployed by S7.6 exists, is audited,
//! and governs **nothing**: `PolicyBinding.check` for an action class nobody has
//! bound returns `(Allow, PB_UNGOVERNED)`, which quorum's rule 5 records as
//! `ungoverned` and alerts on rather than treating as approval.
//!
//! Binding is what changes that answer. So the evidence this stage produces is
//! not "the transaction succeeded" — it is **`check` answering differently
//! afterwards than it did before**, read from the chain either side of the
//! transaction.
//!
//! # What binding is, and is not
//!
//! From S6.4's enforcement table, unchanged by this sprint: `PolicyBinding` has
//! **no on-chain caller**. Nothing in the product is prevented by a binding
//! today. What a binding does is make quorum's own gate — the advisory path, in
//! the agent adapter — able to ask a real question and get a real answer, and
//! make the resulting decision record carry a verdict rather than `ungoverned`.
//!
//! That is worth having and it is not enforcement. The surface must not imply
//! otherwise, and neither must this module.
//!
//! # Why binding is two-phase
//!
//! `bind` is a transaction, so it goes through the SignatureCeremony like every
//! other signature in this app (rule 3). The bridge contract as frozen at D-1
//! declared `bind(protocol, actionClass): Promise<void>` — one call, no result.
//! That cannot be right for a signed on-chain act: it gives a human nothing to
//! check and the ledger nothing to record, and it would report success for a
//! transaction that had merely been *enqueued*. So bind is split into
//! `bindIntent` / `bindComplete`, matching `deployIntent` / `deployComplete` and
//! `rooms.connectIntent` / `connectComplete` — the same shape D-1 chose for
//! deploy, for the same reason.

use serde::Serialize;

use crate::anchor::keccak256;

/// The `bytes32` an action class is known by on chain.
///
/// `keccak256("repo.write")` — `IGovernanceProtocol.check`'s own NatSpec gives
/// exactly that example, so this is the contract's convention rather than one
/// invented here. It is NOT lowercased: `PolicyBinding` keys a mapping on these
/// bytes, and folding case here would silently bind "Repo.Write" and
/// "repo.write" to the same slot while the adapter that asks about them may
/// not.
pub fn action_class_id(class: &str) -> [u8; 32] {
    keccak256(class.trim().as_bytes())
}

/// A verdict as `PolicyBinding` returns it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Verdict {
    Allow,
    Deny,
    RequireApproval,
    RequireVote,
}

impl Verdict {
    /// The enum's declaration order in `IGovernanceProtocol`, which is what the
    /// ABI encodes. Anything else is a return this app does not understand, and
    /// an unknown verdict must NOT collapse to `Allow` — that is a fail-open
    /// wearing a default's clothing.
    pub fn from_u8(v: u8) -> Option<Verdict> {
        match v {
            0 => Some(Verdict::Allow),
            1 => Some(Verdict::Deny),
            2 => Some(Verdict::RequireApproval),
            3 => Some(Verdict::RequireVote),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Verdict::Allow => "Allow",
            Verdict::Deny => "Deny",
            Verdict::RequireApproval => "RequireApproval",
            Verdict::RequireVote => "RequireVote",
        }
    }
}

/// A reason code, which the contracts store as a left-aligned ASCII `bytes32`
/// (`bytes32("PB_UNGOVERNED")`). Rendered back to text, with the padding
/// dropped — and returned as hex if it is not ASCII, rather than as a lossy
/// string that would read like a real code.
pub fn reason_str(word: &[u8; 32]) -> String {
    let end = word.iter().rposition(|b| *b != 0).map_or(0, |i| i + 1);
    let body = &word[..end];
    match std::str::from_utf8(body) {
        Ok(s) if body.iter().all(|b| b.is_ascii_graphic() || *b == b'_') => s.to_string(),
        _ => format!("0x{}", hex::encode(word)),
    }
}

/// `bind(bytes32,bytes32,address)` calldata.
pub fn encode_bind(tenant_id: &[u8; 32], action_class: &[u8; 32], protocol: &str) -> String {
    let mut out = Vec::with_capacity(4 + 96);
    out.extend_from_slice(&selector("bind(bytes32,bytes32,address)"));
    out.extend_from_slice(tenant_id);
    out.extend_from_slice(action_class);
    out.extend_from_slice(&[0u8; 12]);
    out.extend_from_slice(&addr_bytes(protocol).unwrap_or([0u8; 20]));
    format!("0x{}", hex::encode(out))
}

/// The probe context a `check` evidence read asks about.
///
/// **This is a hypothetical question, and it is labelled as one everywhere it
/// surfaces.** `check` is a `view`: asking it costs nothing and changes nothing,
/// and the answer is "what would this policy say about an action shaped like
/// this". It is NOT a decision record, no action took place, and presenting it
/// as one would be a fabricated decision — the exact thing rule 1 forbids.
///
/// The shape is deliberately the least interesting action possible: the
/// operator as principal, no agent, no cost, the spec's own classification. A
/// probe tuned to produce a dramatic verdict would be a demo.
pub struct Probe<'a> {
    pub principal: &'a str,
    pub classification: u8,
    pub correlation_id: [u8; 32],
}

/// `check(bytes32,bytes32,(uint256,address,uint8,uint256,bytes32,bytes32))`.
///
/// `ActionContext` holds only static types, so it is inlined into the head
/// rather than pointed at by an offset — eight words with no tail. Encoding it
/// as dynamic would still produce a well-formed call, and the contract would
/// read an offset as an agent id.
pub fn encode_check(
    tenant_id: &[u8; 32],
    action_class: &[u8; 32],
    probe: &Probe,
) -> Result<String, String> {
    let principal = addr_bytes(probe.principal)
        .ok_or_else(|| format!("not an address: {}", probe.principal))?;
    let mut out = Vec::with_capacity(4 + 8 * 32);
    out.extend_from_slice(&selector(
        "check(bytes32,bytes32,(uint256,address,uint8,uint256,bytes32,bytes32))",
    ));
    out.extend_from_slice(tenant_id);
    out.extend_from_slice(action_class);
    out.extend_from_slice(&[0u8; 32]); // agentSbtId — a human is asking
    out.extend_from_slice(&[0u8; 12]);
    out.extend_from_slice(&principal);
    let mut cls = [0u8; 32];
    cls[31] = probe.classification;
    out.extend_from_slice(&cls);
    out.extend_from_slice(&[0u8; 32]); // cost — a probe spends nothing
    out.extend_from_slice(&keccak256(b"")); // paramsHash — no parameters
    out.extend_from_slice(&probe.correlation_id);
    Ok(format!("0x{}", hex::encode(out)))
}

/// What `check` answered.
#[derive(Serialize, serde::Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct CheckAnswer {
    pub verdict: String,
    pub reason: String,
    /// How many identities the protocol says must sign. Zero for `Allow`.
    pub required_signers: usize,
    /// True when this is the unbound answer — `Allow` with `PB_UNGOVERNED`.
    ///
    /// Kept as its own field rather than left for a caller to infer from the
    /// verdict, because `Allow/PB_UNGOVERNED` and `Allow/PB_ALLOWED` are the
    /// same verdict and opposite facts: one means nobody has said, the other
    /// means a protocol considered it and agreed.
    pub ungoverned: bool,
}

/// Decode `check`'s return: `(uint8, bytes32, bytes32[])`.
pub fn decode_check(ret: &[u8]) -> Result<CheckAnswer, String> {
    if ret.len() < 96 {
        return Err(format!(
            "PolicyBinding.check returned {} bytes; expected at least 96",
            ret.len()
        ));
    }
    let v = ret[31];
    let verdict = Verdict::from_u8(v)
        .ok_or_else(|| format!("PolicyBinding.check returned verdict {v}, which is not one \
                                of the four in IGovernanceProtocol. Refusing to interpret it"))?;
    let mut reason_word = [0u8; 32];
    reason_word.copy_from_slice(&ret[32..64]);
    let reason = reason_str(&reason_word);

    // The array's offset is measured from the start of the return.
    let off = u64::from_be_bytes(ret[88..96].try_into().map_err(|_| "bad offset")?) as usize;
    let required_signers = if ret.len() >= off + 32 {
        u64::from_be_bytes(ret[off + 24..off + 32].try_into().map_err(|_| "bad length")?) as usize
    } else {
        0
    };

    Ok(CheckAnswer {
        ungoverned: verdict == Verdict::Allow && reason == "PB_UNGOVERNED",
        verdict: verdict.as_str().to_string(),
        reason,
        required_signers,
    })
}

fn selector(sig: &str) -> [u8; 4] {
    let d = keccak256(sig.as_bytes());
    [d[0], d[1], d[2], d[3]]
}

fn addr_bytes(a: &str) -> Option<[u8; 20]> {
    let raw = hex::decode(a.strip_prefix("0x").unwrap_or(a)).ok()?;
    if raw.len() != 20 {
        return None;
    }
    let mut out = [0u8; 20];
    out.copy_from_slice(&raw);
    Some(out)
}

// ── Tauri seam ──────────────────────────────────────────────────────────

#[derive(Serialize, serde::Deserialize, Debug, Clone)]
pub struct BindIntentDto {
    pub ceremony_id: String,
    pub protocol: String,
    pub action_class: String,
    pub action_class_id: String,
    pub tenant_id: String,
    pub tenant_name: String,
    /// What `check` says RIGHT NOW, before the binding — the "before" half of
    /// the evidence, read from the chain rather than assumed.
    pub before: CheckAnswer,
    pub ceremony_action: String,
    pub source: String,
}

#[derive(Serialize, Debug, Clone)]
pub struct BindResultDto {
    pub tx_hash: String,
    pub block_number: Option<u64>,
    pub protocol: String,
    pub action_class: String,
    pub tenant_id: String,
    /// The verdict before the binding, carried forward from the intent.
    pub before: CheckAnswer,
    /// The verdict after, read from the chain once the transaction is mined.
    pub after: CheckAnswer,
    /// Whether the answer actually changed. A binding that leaves `check`
    /// saying the same thing has not started governing anything.
    pub changed: bool,
    /// How many protocols the tenant now has bound to this action class.
    pub protocol_count: usize,
    pub source: String,
}

/// The pieces every bind command needs: books, the contract, the tenant node.
struct BindCtx {
    root: std::path::PathBuf,
    policy_binding: String,
    rpc: crate::chain::Rpc,
    chain_id: u64,
    tenant_id: [u8; 32],
    tenant_name: String,
    admins: Vec<String>,
    classification_max: u8,
    node_source: String,
}

fn context(app: &tauri::AppHandle) -> Result<BindCtx, String> {
    use tauri::Manager;
    let root = app
        .path()
        .app_data_dir()
        .map_err(|e| format!("no app data dir: {e}"))?;
    let book = crate::addresses::AddressBook::load().map_err(|e| e.to_string())?;
    let bfr = crate::addresses::AddressBook::load_bfr().map_err(|e| e.to_string())?;
    let policy_binding = book
        .get("PolicyBinding")
        .ok_or("PolicyBinding is not in the address book")?
        .to_string();
    let rpc = crate::chain::Rpc::from_book(&book)?;

    let store = crate::store::EvidenceStore::open(crate::store::evidence_dir(&root))
        .map_err(|e| format!("evidence store: {e}"))?;
    let tenant = store.load_scope().ok_or("no tenant scope is established")?;
    let tenant_id = crate::chain::tenant_id_of(&tenant);
    let node = crate::chain::tenant_node(&bfr, tenant_id)?.ok_or_else(|| {
        format!(
            "TenantHierarchy holds no node for \"{tenant}\" (0x{}), so there is no \
             tenant to bind into",
            hex::encode(tenant_id)
        )
    })?;

    Ok(BindCtx {
        root,
        policy_binding,
        chain_id: book.chain_id,
        rpc,
        tenant_id,
        tenant_name: node.display_name,
        admins: node.admins,
        classification_max: node.classification_max,
        node_source: node.source,
    })
}

/// Ask `check` what it says today.
fn ask(ctx: &BindCtx, action_class: &[u8; 32], principal: &str) -> Result<CheckAnswer, String> {
    let data = encode_check(
        &ctx.tenant_id,
        action_class,
        &Probe {
            principal,
            classification: ctx.classification_max,
            correlation_id: *action_class,
        },
    )?;
    let ret = ctx.rpc.eth_call(&ctx.policy_binding, &data)?;
    decode_check(&ret)
}

/// `governance.bindIntent` — stage 8, phase one. **Signs nothing, sends
/// nothing.**
#[tauri::command]
pub fn governance_bind_intent(
    app: tauri::AppHandle,
    ceremony: tauri::State<'_, citrate_core_kit::ceremony::CeremonyState>,
    custody: tauri::State<'_, citrate_core_kit::custody::CustodyState>,
    protocol: String,
    action_class: String,
) -> Result<BindIntentDto, String> {
    use citrate_core_kit::ceremony::{IntentKind, SignatureIntent};

    let class = action_class.trim();
    if class.is_empty() {
        return Err("an action class is required. A binding with no action class would \
                    govern nothing, and choosing one on the operator's behalf would \
                    govern something they never named"
            .to_string());
    }
    if addr_bytes(&protocol).is_none() {
        return Err(format!("{protocol} is not a 20-byte address"));
    }

    let ctx = context(&app)?;
    let from = citrate_core_kit::wallet::address(&custody.0).map_err(|e| e.to_string())?;
    if !ctx
        .admins
        .iter()
        .any(|a| a.eq_ignore_ascii_case(&from.address))
    {
        return Err(format!(
            "{} is not an admin of \"{}\" (admins: {}). PolicyBinding.bind requires tenant \
             admin membership, so this would revert after a human had approved it",
            from.address,
            ctx.tenant_name,
            if ctx.admins.is_empty() {
                "none".to_string()
            } else {
                ctx.admins.join(", ")
            }
        ));
    }

    let class_id = action_class_id(class);
    let before = ask(&ctx, &class_id, &from.address)?;

    let data = encode_bind(&ctx.tenant_id, &class_id, &protocol);

    // As the operator, so PB-2 (`wasDeployedHere`), PB-3 (right tenant) and the
    // admin gate are all really exercised. A protocol this factory did not
    // deploy is refused here rather than at the end of a ceremony.
    ctx.rpc
        .eth_call_from(&from.address, &ctx.policy_binding, &data)
        .map_err(|e| {
            format!(
                "PolicyBinding refuses this binding, so no ceremony was raised: {e}. \
                 Nothing was signed and nothing was sent"
            )
        })?;

    let tx = format!(
        r#"{{"from":"{}","to":"{}","value":"0x0","data":"{}"}}"#,
        from.address, ctx.policy_binding, data
    );
    let ceremony_action = format!(
        "Call {} with {} bytes calldata (value 0 wei)",
        ctx.policy_binding,
        (data.len() - 2) / 2
    );

    let view = ceremony.0.request(SignatureIntent {
        origin: format!("governance · bind {class}"),
        kind: IntentKind::Transaction,
        chain_id: ctx.chain_id,
        raw: tx,
    });

    let dto = BindIntentDto {
        ceremony_id: view.id,
        protocol,
        action_class: class.to_string(),
        action_class_id: format!("0x{}", hex::encode(class_id)),
        tenant_id: format!("0x{}", hex::encode(ctx.tenant_id)),
        tenant_name: ctx.tenant_name.clone(),
        before,
        ceremony_action,
        source: format!(
            "PolicyBinding.check/bind at {} · {} · chain {} · {}",
            ctx.policy_binding,
            ctx.node_source,
            ctx.chain_id,
            ctx.rpc.url()
        ),
    };
    save_intent(&ctx.root, &dto)?;
    Ok(dto)
}

/// `governance.bindComplete` — stage 8, phase two.
///
/// **Signs nothing and broadcasts nothing** — same reason as
/// `governance_deploy_complete`: the ceremony's `sign_and_broadcast` is the one
/// path that signs. This re-reads `check` and reports both answers.
///
/// The `before` is the one recorded at intent time, not a fresh read: re-reading
/// it after the transaction would produce two identical "after" values and a
/// diff that is always empty.
#[tauri::command]
pub fn governance_bind_complete(
    app: tauri::AppHandle,
    custody: tauri::State<'_, citrate_core_kit::custody::CustodyState>,
    ceremony_id: String,
    tx_hash: String,
) -> Result<BindResultDto, String> {
    let ctx = context(&app)?;
    let intent = load_intent(&ctx.root, &ceremony_id).ok_or_else(|| {
        format!(
            "no bind intent was recorded for ceremony {ceremony_id}, so there is no \
             'before' verdict to compare against"
        )
    })?;

    let receipt = ctx.rpc.receipt(&tx_hash)?.ok_or_else(|| {
        format!(
            "no receipt for {tx_hash}. It may still be mined — check the hash rather \
             than re-running this"
        )
    })?;
    if !matches!(
        receipt.get("status").and_then(|s| s.as_str()),
        Some("0x1") | Some("0x01")
    ) {
        return Err(format!(
            "the bind transaction {tx_hash} was mined and REVERTED. Nothing is bound"
        ));
    }
    // The hash is an argument, so it must really be a call to PolicyBinding.
    if !receipt
        .get("to")
        .and_then(|t| t.as_str())
        .is_some_and(|t| t.eq_ignore_ascii_case(&ctx.policy_binding))
    {
        return Err(format!(
            "{tx_hash} is not a call to PolicyBinding ({}), so it cannot be the \
             binding this ceremony described",
            ctx.policy_binding
        ));
    }
    let block_number = receipt
        .get("blockNumber")
        .and_then(|b| b.as_str())
        .and_then(|s| u64::from_str_radix(s.trim_start_matches("0x"), 16).ok());

    let from = citrate_core_kit::wallet::address(&custody.0).map_err(|e| e.to_string())?;
    let class_id = action_class_id(&intent.action_class);
    let after = ask(&ctx, &class_id, &from.address)?;

    // How many protocols the tenant now has on this action class — read from
    // the contract, so "it is bound" is the chain's claim and not ours.
    let mut count_data = Vec::with_capacity(68);
    count_data.extend_from_slice(&selector("protocolCount(bytes32,bytes32)"));
    count_data.extend_from_slice(&ctx.tenant_id);
    count_data.extend_from_slice(&class_id);
    let count_ret = ctx.rpc.eth_call(
        &ctx.policy_binding,
        &format!("0x{}", hex::encode(count_data)),
    )?;
    let protocol_count = if count_ret.len() >= 32 {
        u64::from_be_bytes(
            count_ret[24..32]
                .try_into()
                .map_err(|_| "protocolCount: short return")?,
        ) as usize
    } else {
        0
    };

    let dto = BindResultDto {
        tx_hash,
        block_number,
        protocol: intent.protocol,
        action_class: intent.action_class,
        tenant_id: intent.tenant_id,
        changed: after != intent.before,
        before: intent.before,
        after,
        protocol_count,
        source: format!(
            "PolicyBinding.check/protocolCount at {} · eth_getTransactionReceipt · \
             chain {} · {}",
            ctx.policy_binding,
            ctx.chain_id,
            ctx.rpc.url()
        ),
    };

    // Mark the spec `bound`, if this app is the one that deployed the protocol.
    // A protocol deployed elsewhere has no record here, and inventing a link to
    // a local draft would claim a provenance we cannot show.
    if let Some(spec_id) = crate::protocols::spec_for_protocol(&ctx.root, &dto.protocol) {
        let body = serde_json::to_string_pretty(&dto).map_err(|e| e.to_string())?;
        crate::protocols::record_stage(&ctx.root, "bound", &spec_id, &body)?;
    }
    Ok(dto)
}

fn intent_path(root: &std::path::Path, ceremony_id: &str) -> std::path::PathBuf {
    let safe: String = ceremony_id
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '-' || *c == '_')
        .collect();
    root.join("bind-intents").join(format!("{safe}.json"))
}

pub fn save_intent(root: &std::path::Path, dto: &BindIntentDto) -> Result<(), String> {
    let path = intent_path(root, &dto.ceremony_id);
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    let body = serde_json::to_vec_pretty(dto).map_err(|e| e.to_string())?;
    std::fs::write(&path, body).map_err(|e| e.to_string())
}

pub fn load_intent(root: &std::path::Path, ceremony_id: &str) -> Option<BindIntentDto> {
    let raw = std::fs::read_to_string(intent_path(root, ceremony_id)).ok()?;
    serde_json::from_str(&raw).ok()
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    const TENANT: [u8; 32] = [0x11; 32];
    const PROTOCOL: &str = "0xabababababababababababababababababababab";

    // ── Calldata, pinned against cast ───────────────────────────────

    /// Pinned against `cast calldata`. A wrong selector here binds nothing and
    /// reverts; a wrong argument ORDER binds a protocol to the tenant id as
    /// though it were an action class, which succeeds and governs nothing
    /// anyone can find.
    #[test]
    fn the_bind_calldata_matches_cast() {
        let got = encode_bind(&TENANT, &[0x22; 32], PROTOCOL);
        // cast calldata "bind(bytes32,bytes32,address)" 0x1111… 0x2222… 0xabab…
        let want = concat!(
            "0x13431471",
            "1111111111111111111111111111111111111111111111111111111111111111",
            "2222222222222222222222222222222222222222222222222222222222222222",
            "000000000000000000000000abababababababababababababababababababab",
        );
        assert_eq!(got, want, "bind calldata drifted from cast");
    }

    /// `ActionContext` is all static types, so it is INLINED — eight words, no
    /// tail. Encoding it as a dynamic struct produces a call the contract reads
    /// with an offset where the agent id should be.
    #[test]
    fn the_check_calldata_matches_cast() {
        let got = encode_check(
            &TENANT,
            &[0x22; 32],
            &Probe {
                principal: "0x00000000000000000000000000000000000000ab",
                classification: 2,
                correlation_id: [0x33; 32],
            },
        )
        .expect("encodes");
        let want = concat!(
            "0xc291f32a",
            "1111111111111111111111111111111111111111111111111111111111111111",
            "2222222222222222222222222222222222222222222222222222222222222222",
            "0000000000000000000000000000000000000000000000000000000000000000",
            "00000000000000000000000000000000000000000000000000000000000000ab",
            "0000000000000000000000000000000000000000000000000000000000000002",
            "0000000000000000000000000000000000000000000000000000000000000000",
            "c5d2460186f7233c927e7db2dcc703c0e500b653ca82273b7bfad8045d85a470",
            "3333333333333333333333333333333333333333333333333333333333333333",
        );
        assert_eq!(got, want, "check calldata drifted from cast");
        // 4-byte selector + 8 words, and nothing after them.
        assert_eq!((got.len() - 2) / 2, 4 + 8 * 32);
    }

    #[test]
    fn the_action_class_is_the_keccak_the_natspec_names() {
        assert_eq!(
            hex::encode(action_class_id("repo.write")),
            hex::encode(keccak256(b"repo.write"))
        );
        assert_eq!(action_class_id(" repo.write "), action_class_id("repo.write"));
    }

    /// Case is NOT folded. `PolicyBinding` keys a mapping on these bytes, and an
    /// adapter asking about "Repo.Write" is asking about a different class —
    /// pretending otherwise here would report a binding that does not apply.
    #[test]
    fn the_action_class_does_not_fold_case() {
        assert_ne!(action_class_id("Repo.Write"), action_class_id("repo.write"));
    }

    // ── Decoding the verdict ────────────────────────────────────────

    fn check_return(verdict: u8, reason: &str, signers: usize) -> Vec<u8> {
        let mut out = Vec::new();
        let mut w = [0u8; 32];
        w[31] = verdict;
        out.extend_from_slice(&w);
        let mut r = [0u8; 32];
        r[..reason.len()].copy_from_slice(reason.as_bytes());
        out.extend_from_slice(&r);
        let mut off = [0u8; 32];
        off[31] = 96;
        out.extend_from_slice(&off);
        let mut len = [0u8; 32];
        len[31] = signers as u8;
        out.extend_from_slice(&len);
        for _ in 0..signers {
            out.extend_from_slice(&[0xaa; 32]);
        }
        out
    }

    /// **The distinction the whole stage turns on.** `Allow/PB_UNGOVERNED` and
    /// `Allow/PB_ALLOWED` are the same verdict and opposite facts: one means
    /// nobody has said anything, the other means a protocol considered it and
    /// agreed. Collapsing them would make an unbound action class look governed.
    #[test]
    fn ungoverned_is_distinguished_from_allowed() {
        let unbound = decode_check(&check_return(0, "PB_UNGOVERNED", 0)).expect("decodes");
        assert_eq!(unbound.verdict, "Allow");
        assert!(unbound.ungoverned);

        let allowed = decode_check(&check_return(0, "PB_ALLOWED", 0)).expect("decodes");
        assert_eq!(allowed.verdict, "Allow");
        assert!(!allowed.ungoverned, "PB_ALLOWED must not read as ungoverned");
    }

    /// What a freshly bound `ThresholdApproval` actually answers, with no
    /// envelope proposed: approval required, and the approver set named. This
    /// is the honest post-bind state and the surface must be able to show it
    /// without dressing it up as "working approvals".
    #[test]
    fn a_require_approval_verdict_carries_its_signers() {
        let a = decode_check(&check_return(2, "TA_NOT_PROPOSED", 3)).expect("decodes");
        assert_eq!(a.verdict, "RequireApproval");
        assert_eq!(a.required_signers, 3);
        assert!(!a.ungoverned);
    }

    /// **Fail loud, not open.** A verdict outside the four in the enum is a
    /// return this app does not understand. Defaulting it to `Allow` would be a
    /// fail-open that looks like a sensible default.
    #[test]
    fn an_unknown_verdict_is_refused_rather_than_allowed() {
        assert_eq!(Verdict::from_u8(4), None);
        let e = decode_check(&check_return(7, "PB_ALLOWED", 0)).expect_err("must refuse");
        assert!(e.contains("Refusing to interpret"), "{e}");
    }

    #[test]
    fn a_short_return_is_an_error_not_an_allow() {
        assert!(decode_check(&[0u8; 32]).is_err());
        assert!(decode_check(&[]).is_err());
    }

    /// Reason codes are ASCII in a left-aligned `bytes32`; anything else is
    /// shown as hex rather than as a mangled string that would read like a code.
    #[test]
    fn reason_codes_render_as_text_or_honest_hex() {
        let mut w = [0u8; 32];
        w[..13].copy_from_slice(b"PB_UNGOVERNED");
        assert_eq!(reason_str(&w), "PB_UNGOVERNED");
        assert_eq!(reason_str(&[0u8; 32]), "");
        let mut bad = [0u8; 32];
        bad[0] = 0xff;
        assert!(reason_str(&bad).starts_with("0x"));
    }

    // ── The diff ────────────────────────────────────────────────────

    /// A binding that leaves `check` saying exactly what it said before has not
    /// started governing anything, and `changed` has to say so. This is the
    /// field that stops "the transaction succeeded" from being mistaken for
    /// "the policy took effect".
    #[test]
    fn an_unchanged_verdict_is_reported_as_unchanged() {
        let before = decode_check(&check_return(0, "PB_UNGOVERNED", 0)).expect("decodes");
        let same = decode_check(&check_return(0, "PB_UNGOVERNED", 0)).expect("decodes");
        let after = decode_check(&check_return(2, "TA_NOT_PROPOSED", 2)).expect("decodes");
        assert_eq!(before, same, "identical answers must compare equal");
        assert_ne!(before, after);
    }

    #[test]
    fn a_ceremony_id_cannot_escape_its_directory() {
        let p = intent_path(std::path::Path::new("/root"), "../../etc/passwd");
        assert_eq!(p, std::path::Path::new("/root/bind-intents/etcpasswd.json"));
    }

    #[test]
    fn the_bind_intent_round_trips_through_disk() {
        let dir = std::env::temp_dir().join(format!("qrm-bind-{}", std::process::id()));
        let d = BindIntentDto {
            ceremony_id: "cer-1".into(),
            protocol: PROTOCOL.into(),
            action_class: "repo.write".into(),
            action_class_id: "0x22".into(),
            tenant_id: "0x11".into(),
            tenant_name: "Citrate".into(),
            before: decode_check(&check_return(0, "PB_UNGOVERNED", 0)).expect("decodes"),
            ceremony_action: "Call …".into(),
            source: "test".into(),
        };
        save_intent(&dir, &d).expect("save");
        let back = load_intent(&dir, "cer-1").expect("load");
        assert!(back.before.ungoverned);
        assert_eq!(back.action_class, "repo.write");
        let _ = std::fs::remove_dir_all(&dir);
    }

    // ── Live ────────────────────────────────────────────────────────

    /// The unbound answer, read from the LIVE contract. Everything above pins
    /// this module against its own arithmetic; this is the only test that pins
    /// it against the deployed `PolicyBinding`.
    ///
    /// An action class nobody has bound must answer `Allow/PB_UNGOVERNED`. If
    /// this returns something else, either the encoding is wrong or the tenant
    /// has bindings this test does not know about — and both are worth failing
    /// for.
    #[test]
    #[ignore = "hits the live chain"]
    fn live_40204_answers_ungoverned_for_an_unbound_class() {
        let book = crate::addresses::AddressBook::load().expect("book");
        let pb = book.get("PolicyBinding").expect("PolicyBinding in book");
        let rpc = crate::chain::Rpc::from_book(&book).expect("rpc");
        let tenant = crate::chain::tenant_id_of("Citrate");
        let class = action_class_id("quorum.test.never-bound");
        let data = encode_check(
            &tenant,
            &class,
            &Probe {
                principal: "0x00000000000000000000000000000000000000ab",
                classification: 2,
                correlation_id: class,
            },
        )
        .expect("encodes");
        let ret = rpc.eth_call(pb, &data).expect("the read must succeed");
        let a = decode_check(&ret).expect("decodes");
        assert_eq!(a.verdict, "Allow");
        assert_eq!(a.reason, "PB_UNGOVERNED");
        assert!(a.ungoverned);
    }
}
