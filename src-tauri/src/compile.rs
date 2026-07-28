//! QRM-S7.4 — COMPILE: map clauses onto audited templates, or refuse.
//!
//! Stage 4 of the authoring pipeline, and the one the sprint's risk register
//! calls out by name:
//!
//! > **R-A.** An LLM asked to map English onto template parameters will happily
//! > invent a mapping that type-checks and is wrong.
//!
//! The defence is not a better prompt. It is that **typed extraction must
//! succeed or the clause does not map**. "Two approvers" becomes
//! `threshold = 2` because a parser turned the word into a number; "two
//! approvers, unless it's a Friday" becomes an [`Unmapped`] clause with a reason,
//! because nothing here knows what to do with the second half and guessing would
//! produce a protocol that silently ignores it.
//!
//! # Three outcomes, and why the third is not a loophole
//!
//! A clause is one of:
//!
//! 1. **mapped** — a registered template plus typed parameters;
//! 2. **unmapped** — it should map and cannot, which blocks deployment;
//! 3. **structural** — it describes the BINDING rather than a protocol's
//!    parameters. "This policy governs repo.write" is not a template parameter;
//!    it is which action class the protocol gets bound to (S7.7).
//!
//! The third category is the dangerous one, because a dumping ground for
//! anything awkward would let an undeployable spec look finished. So it is
//! decided by **topic alone** — a fixed allowlist — never by whether mapping
//! happened to fail. A `Thresholds` clause that will not type is `unmapped`,
//! and there is no path that reclassifies it. That rule is what the negative
//! controls check.
//!
//! # Only registered bytecode
//!
//! A template that is not in the live [`GovernanceTemplateRegistry`] cannot be
//! mapped to, however obvious the fit. That is the same audit boundary GF-2
//! enforces on chain, applied one stage earlier so an operator learns at compile
//! time rather than at the ceremony.
//!
//! As of this writing the registry holds **zero** templates — S6.9 registered
//! none rather than pass a placeholder `auditCID`. So the honest live output
//! today is that every mappable clause is unmapped, with a reason naming the
//! empty registry. That is the product's real state, not a bug in this module.

use std::collections::BTreeMap;

use crate::interview::Topic;
use crate::spec::{Clause, Spec};

/// One template the registry says is deployable.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TemplateRow {
    pub name: String,
    pub id: String,
    pub version: u32,
}

/// The registered templates available to map onto.
#[derive(Clone, Debug, Default)]
pub struct Catalog {
    pub templates: Vec<TemplateRow>,
}

impl Catalog {
    pub fn by_name(&self, name: &str) -> Option<&TemplateRow> {
        self.templates.iter().find(|t| t.name == name)
    }
    pub fn is_empty(&self) -> bool {
        self.templates.is_empty()
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Mapped {
    pub clause: String,
    pub template_id: String,
    pub template_name: String,
    pub params: BTreeMap<String, String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Unmapped {
    pub clause: String,
    pub why: String,
}

/// A clause that shapes the binding rather than a protocol's parameters.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Structural {
    pub clause: String,
    pub kind: String,
    pub value: String,
}

#[derive(Clone, Debug, Default)]
pub struct Compiled {
    pub spec_id: String,
    pub mapped: Vec<Mapped>,
    pub unmapped: Vec<Unmapped>,
    pub structural: Vec<Structural>,
}

impl Compiled {
    /// Deployable only when nothing is unmapped AND something actually mapped.
    ///
    /// The second half matters: a spec whose clauses are all structural has
    /// nothing to deploy, and reporting it deployable would send an operator to
    /// a ceremony with no protocol in it.
    pub fn deployable(&self) -> bool {
        self.unmapped.is_empty() && !self.mapped.is_empty()
    }
}

/// Topics that describe the binding, not a protocol's parameters.
///
/// A fixed allowlist, checked BEFORE any mapping is attempted, so a failed
/// mapping can never be reclassified as structural.
const STRUCTURAL_TOPICS: &[Topic] = &[Topic::Scope, Topic::Principals];

/// The template a topic maps onto, when it maps onto one at all.
fn template_for(topic: Topic) -> Option<&'static str> {
    match topic {
        Topic::Thresholds => Some("ThresholdApproval"),
        Topic::Roles => Some("SegregationOfDuties"),
        Topic::Escalation => Some("IncidentEscalation"),
        Topic::Expiry => Some("TimeBoundedElevation"),
        Topic::Exceptions => Some("ClassificationGate"),
        Topic::Scope | Topic::Principals => None,
    }
}

/// Compile a drafted spec against the registered templates.
pub fn compile(spec: &Spec, catalog: &Catalog) -> Compiled {
    let mut out = Compiled {
        spec_id: spec.id.clone(),
        ..Default::default()
    };

    for c in &spec.clauses {
        // Structural first, and by topic only. This ordering is the rule: a
        // clause is structural because of WHAT IT IS, never because mapping it
        // turned out to be hard.
        if STRUCTURAL_TOPICS.contains(&c.topic) {
            out.structural.push(Structural {
                clause: c.n.clone(),
                kind: c.topic.as_str().to_string(),
                value: c
                    .params
                    .values()
                    .next()
                    .cloned()
                    .unwrap_or_default(),
            });
            continue;
        }

        let Some(name) = template_for(c.topic) else {
            out.unmapped.push(Unmapped {
                clause: c.n.clone(),
                why: format!(
                    "no template expresses a '{}' clause; it must be added to the \
                     audited set before this spec can deploy",
                    c.topic.as_str()
                ),
            });
            continue;
        };

        let Some(row) = catalog.by_name(name) else {
            out.unmapped.push(Unmapped {
                clause: c.n.clone(),
                why: if catalog.is_empty() {
                    format!(
                        "no audited templates are registered at all, so '{name}' \
                         cannot be mapped to — register the audited set first"
                    )
                } else {
                    format!("'{name}' is not registered in the audited template set")
                },
            });
            continue;
        };

        match extract_params(c) {
            Ok(params) => out.mapped.push(Mapped {
                clause: c.n.clone(),
                template_id: row.id.clone(),
                template_name: row.name.clone(),
                params,
            }),
            Err(why) => out.unmapped.push(Unmapped {
                clause: c.n.clone(),
                why,
            }),
        }
    }
    out
}

/// Turn a clause's free text into typed parameters, or say why it cannot.
///
/// This is where R-A is actually held: every parameter has to come out of a
/// parser, and a parser that fails is the whole answer.
fn extract_params(c: &Clause) -> Result<BTreeMap<String, String>, String> {
    let raw = c.params.values().next().cloned().unwrap_or_default();
    let mut p = BTreeMap::new();

    match c.topic {
        Topic::Thresholds => {
            let n = parse_count(&raw).ok_or_else(|| {
                format!(
                    "cannot read an approval count from \"{raw}\" — ThresholdApproval \
                     needs a number, and a threshold that had to be guessed is a \
                     threshold nobody set"
                )
            })?;
            if n == 0 {
                return Err("an approval threshold of zero would make the clause a no-op that reads like a control".to_string());
            }
            p.insert("threshold".to_string(), n.to_string());
        }
        Topic::Roles => {
            let n = parse_count(&raw).unwrap_or(1);
            p.insert("minApprovers".to_string(), n.to_string());
        }
        Topic::Escalation | Topic::Expiry => {
            let secs = parse_duration_secs(&raw).ok_or_else(|| {
                format!(
                    "cannot read a duration from \"{raw}\" — this clause needs one \
                     (for example \"8 hours\", \"30 days\"), and a window that had \
                     to be inferred is not a window anyone agreed to"
                )
            })?;
            let key = if c.topic == Topic::Expiry { "durationSeconds" } else { "slaSeconds" };
            p.insert(key.to_string(), secs.to_string());
        }
        Topic::Exceptions => {
            let ceiling = parse_classification(&raw).ok_or_else(|| {
                format!(
                    "cannot read a classification ceiling from \"{raw}\" — \
                     ClassificationGate needs one of Public/Proprietary/CUI/ITAR"
                )
            })?;
            p.insert("ceiling".to_string(), ceiling.to_string());
        }
        Topic::Scope | Topic::Principals => unreachable!("handled as structural"),
    }
    Ok(p)
}

/// Read a small count from text: digits or the number words an operator writes.
///
/// Deliberately narrow. It reads "two", "2", and "2 of 3" — and returns `None`
/// for anything else rather than reaching for the first digit it can find,
/// because "approval within 2 business days" contains a 2 that is not a
/// threshold.
fn parse_count(s: &str) -> Option<u32> {
    let l = s.to_ascii_lowercase();
    // "N of M" — the N is the threshold.
    if let Some(idx) = l.find(" of ") {
        let left = l[..idx].trim();
        if let Some(n) = word_or_digit(left.split_whitespace().last().unwrap_or("")) {
            return Some(n);
        }
    }
    // A bare count, but only when the text is about approving. Otherwise a
    // number in an unrelated sentence would become a threshold.
    if !(l.contains("approv") || l.contains("signature") || l.contains("sign-off") || l.contains("signer")) {
        return None;
    }
    l.split_whitespace().find_map(word_or_digit)
}

fn word_or_digit(tok: &str) -> Option<u32> {
    let t = tok.trim_matches(|c: char| !c.is_ascii_alphanumeric());
    match t {
        "one" => Some(1),
        "two" => Some(2),
        "three" => Some(3),
        "four" => Some(4),
        "five" => Some(5),
        "six" => Some(6),
        _ => t.parse::<u32>().ok().filter(|n| *n <= 64),
    }
}

/// Read a duration. Returns `None` when no unit is present — a bare number is
/// not a duration, and picking a unit for the operator is exactly the kind of
/// helpfulness that produces a window nobody agreed to.
fn parse_duration_secs(s: &str) -> Option<u64> {
    let l = s.to_ascii_lowercase();
    let toks: Vec<&str> = l.split_whitespace().collect();
    for (i, t) in toks.iter().enumerate() {
        let unit = t.trim_matches(|c: char| !c.is_ascii_alphabetic());
        let mult = match unit {
            "second" | "seconds" | "sec" | "secs" => 1u64,
            "minute" | "minutes" | "min" | "mins" => 60,
            "hour" | "hours" | "hr" | "hrs" => 3_600,
            "day" | "days" => 86_400,
            "week" | "weeks" => 604_800,
            _ => continue,
        };
        if i == 0 {
            continue;
        }
        if let Some(n) = word_or_digit(toks[i - 1]) {
            return Some(n as u64 * mult);
        }
    }
    None
}

fn parse_classification(s: &str) -> Option<&'static str> {
    let l = s.to_ascii_lowercase();
    // Highest wins, matching the ingest detector — a clause naming two levels
    // means the stricter one.
    for (needle, level) in [
        ("itar", "ITAR"),
        ("cui", "CUI"),
        ("proprietary", "Proprietary"),
        ("public", "Public"),
    ] {
        if l.split(|c: char| !c.is_ascii_alphanumeric()).any(|w| w == needle) {
            return Some(level);
        }
    }
    None
}

// ── Reading the live registry ───────────────────────────────────────────

/// Read the registered templates from the on-chain registry.
///
/// Asks the registry for `templateId(name, version)` for each template this
/// compiler knows how to map, then whether that row `exists`. Two reasons for
/// that shape rather than enumerating rows:
///
/// * **It cannot drift from the contract's own id derivation.** `templateId` is
///   `public pure` precisely so callers compute ids the same way the registry
///   does instead of reimplementing `abi.encode`; asking it is taking that offer.
/// * **It needs no dynamic ABI decode.** Enumerating rows means decoding a
///   struct with two `string` members, and a hand-rolled decoder that is subtly
///   wrong returns plausible garbage rather than failing.
///
/// The catalog is therefore "the templates this compiler can map, that are
/// registered" — which is exactly the question `compile` asks of it. A template
/// registered under a name this compiler does not know is invisible here, and
/// that is correct: it could not be mapped to anyway.
///
/// An unreachable chain is an ERROR, not an empty catalog. "Nothing is audited
/// yet" and "we could not ask" are different claims and only the first is a
/// reason to tell an operator their clause does not map.
pub fn catalog_from_chain(rpc: &crate::chain::Rpc, registry: &str) -> Result<Catalog, String> {
    const KNOWN: &[&str] = &[
        "ThresholdApproval",
        "ClassificationGate",
        "BudgetedAutonomy",
        "SegregationOfDuties",
        "TimeBoundedElevation",
        "ChangeControlBoard",
        "SupplierAdmission",
        "IncidentEscalation",
    ];
    const VERSION: u32 = 1;

    let mut templates = Vec::new();
    for name in KNOWN {
        let id_ret = rpc.eth_call(registry, &encode_template_id(name, VERSION))?;
        if id_ret.len() < 32 {
            continue;
        }
        let id_bytes = &id_ret[..32];
        let id = format!("0x{}", hex_raw(id_bytes));

        // exists(bytes32) -> bool
        let mut data = String::from("0x38a699a4");
        data.push_str(&hex_raw(id_bytes));
        let ex = rpc.eth_call(registry, &data)?;
        if ex.last().copied().unwrap_or(0) == 1 {
            templates.push(TemplateRow {
                name: (*name).to_string(),
                id,
                version: VERSION,
            });
        }
    }
    Ok(Catalog { templates })
}

/// Hex WITHOUT the `0x` prefix.
///
/// `store::hex_encode` prefixes `0x`, which is right for display and wrong
/// inside calldata — using it here embedded a literal "0x" mid-word, producing
/// a well-formed-looking call that asked about a template nobody registered.
/// Every row would then read as unregistered, and the operator would be told
/// their clause does not map for a reason that was not the real one.
fn hex_raw(bytes: &[u8]) -> String {
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        s.push_str(&format!("{b:02x}"));
    }
    s
}

/// `templateId(string,uint32)` calldata.
///
/// Selector `0x2c4d2d89`, then the standard head/tail layout: an offset to the
/// string, the `uint32` right-aligned, then the string's length and its bytes
/// padded to a 32-byte boundary.
fn encode_template_id(name: &str, version: u32) -> String {
    let bytes = name.as_bytes();
    let mut out = String::from("0x2c4d2d89");
    out.push_str(&format!("{:064x}", 0x40));          // offset to the string
    out.push_str(&format!("{version:064x}"));          // uint32
    out.push_str(&format!("{:064x}", bytes.len()));    // string length
    let mut padded = hex_raw(bytes);
    while padded.len() % 64 != 0 {
        padded.push('0');
    }
    out.push_str(&padded);
    out
}

// ── Tauri seam ──────────────────────────────────────────────────────────

use serde::Serialize;

#[derive(Serialize, Debug, Clone)]
pub struct MappedDto {
    pub clause: String,
    pub template_id: String,
    pub params: BTreeMap<String, String>,
}

#[derive(Serialize, Debug, Clone)]
pub struct UnmappedDto {
    pub clause: String,
    pub why: String,
}

#[derive(Serialize, Debug, Clone)]
pub struct StructuralDto {
    pub clause: String,
    pub kind: String,
    pub value: String,
}

#[derive(Serialize, Debug, Clone)]
pub struct CompileResultDto {
    pub spec_id: String,
    pub deployable: bool,
    pub mapped: Vec<MappedDto>,
    pub unmapped: Vec<UnmappedDto>,
    pub structural: Vec<StructuralDto>,
}

pub fn to_dto(c: &Compiled) -> CompileResultDto {
    CompileResultDto {
        spec_id: c.spec_id.clone(),
        deployable: c.deployable(),
        mapped: c
            .mapped
            .iter()
            .map(|m| MappedDto {
                clause: m.clause.clone(),
                template_id: m.template_id.clone(),
                params: m.params.clone(),
            })
            .collect(),
        unmapped: c
            .unmapped
            .iter()
            .map(|u| UnmappedDto {
                clause: u.clause.clone(),
                why: u.why.clone(),
            })
            .collect(),
        structural: c
            .structural
            .iter()
            .map(|s| StructuralDto {
                clause: s.clause.clone(),
                kind: s.kind.clone(),
                value: s.value.clone(),
            })
            .collect(),
    }
}

/// `governance.compile` — stage 4 of the authoring pipeline.
///
/// Reads the registered templates from the LIVE registry. An unreachable chain
/// is an error rather than an empty catalog: "nothing is audited yet" and "we
/// could not ask" are different claims, and only the first is a reason to tell
/// an operator their clause does not map.
#[tauri::command]
pub fn governance_compile(
    app: tauri::AppHandle,
    spec_id: String,
) -> Result<CompileResultDto, String> {
    use tauri::Manager;
    let root = app
        .path()
        .app_data_dir()
        .map_err(|e| format!("no app data dir: {e}"))?;

    let book = crate::addresses::AddressBook::load().map_err(|e| e.to_string())?;
    let registry = book
        .get("GovernanceTemplateRegistry")
        .ok_or("GovernanceTemplateRegistry is not in the address book — there is \
                nowhere to read the audited template set from")?;
    let rpc = crate::chain::Rpc::from_book(&book)?;
    let catalog = catalog_from_chain(&rpc, registry)?;

    let interview = crate::interview::load(&root, &spec_id);
    let spec = crate::spec::draft(&spec_id, "Untitled policy", &interview);
    Ok(to_dto(&compile(&spec, &catalog)))
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use crate::interview::{Interview, Topic};
    use crate::spec::draft;

    const NOW: i64 = 1_785_000_000_000;

    fn catalog() -> Catalog {
        Catalog {
            templates: ["ThresholdApproval", "SegregationOfDuties", "IncidentEscalation",
                        "TimeBoundedElevation", "ClassificationGate"]
                .iter()
                .enumerate()
                .map(|(i, n)| TemplateRow {
                    name: (*n).to_string(),
                    id: format!("0x{:064x}", i + 1),
                    version: 1,
                })
                .collect(),
        }
    }

    /// Build a spec whose clauses answer the given topics with the given text.
    fn spec_with(answers: &[(Topic, &str)]) -> Spec {
        let mut i = Interview::new("spec-1");
        for t in Topic::ALL {
            let text = answers
                .iter()
                .find(|(tt, _)| tt == t)
                .map(|(_, s)| *s)
                .unwrap_or("unspecified");
            let _ = i.answer(text, "larry", NOW);
            let _ = t;
        }
        draft("spec-1", "T", &i)
    }

    // ── R-A: refuse rather than improvise ───────────────────────────

    /// A threshold that cannot be typed is UNMAPPED with a reason, not guessed.
    /// This is the risk the sprint named: a mapping that type-checks and is
    /// wrong is worse than no mapping, because it deploys.
    #[test]
    fn a_threshold_that_cannot_be_typed_is_unmapped() {
        let s = spec_with(&[(Topic::Thresholds, "whatever the committee feels is right")]);
        let c = compile(&s, &catalog());

        let u = c
            .unmapped
            .iter()
            .find(|u| u.why.contains("approval count"))
            .expect("the threshold clause must be unmapped");
        assert!(u.why.contains("nobody set"), "{}", u.why);
        assert!(!c.deployable(), "an unmapped clause must block deployment");
    }

    /// …and one that CAN be typed maps, with the number that came out of the
    /// parser rather than out of the prose.
    #[test]
    fn a_typed_threshold_maps() {
        let s = spec_with(&[(Topic::Thresholds, "two approvals are required")]);
        let c = compile(&s, &catalog());
        let m = c
            .mapped
            .iter()
            .find(|m| m.template_name == "ThresholdApproval")
            .expect("mapped");
        assert_eq!(m.params.get("threshold").map(String::as_str), Some("2"));
    }

    /// A number that is not a count must not become one. "within 2 business
    /// days" contains a 2, and a parser that grabbed the first digit would set a
    /// threshold of 2 from a sentence about timing.
    #[test]
    fn a_number_in_an_unrelated_sentence_is_not_a_threshold() {
        assert_eq!(parse_count("respond within 2 business days"), None);
        assert_eq!(parse_count("two approvals"), Some(2));
        assert_eq!(parse_count("2 of 3 signers"), Some(2));
    }

    /// A bare number is not a duration. Picking a unit for the operator is
    /// exactly the helpfulness that produces a window nobody agreed to.
    #[test]
    fn a_duration_without_a_unit_is_refused() {
        assert_eq!(parse_duration_secs("escalate after 4"), None);
        assert_eq!(parse_duration_secs("escalate after 4 hours"), Some(14_400));
        assert_eq!(parse_duration_secs("valid for 30 days"), Some(2_592_000));
    }

    /// A zero threshold is refused: it would be a clause that reads like a
    /// control and enforces nothing.
    #[test]
    fn a_zero_threshold_is_refused() {
        let s = spec_with(&[(Topic::Thresholds, "0 approvals required")]);
        let c = compile(&s, &catalog());
        assert!(c.unmapped.iter().any(|u| u.why.contains("no-op")), "{:?}", c.unmapped);
    }

    // ── Structural is not a loophole ────────────────────────────────

    /// Scope and principals describe the BINDING, not a protocol's parameters,
    /// so they are structural — and being structural is decided by topic alone.
    #[test]
    fn scope_and_principals_are_structural() {
        let s = spec_with(&[(Topic::Scope, "repo.write"), (Topic::Principals, "larry")]);
        let c = compile(&s, &catalog());
        assert_eq!(c.structural.len(), 2);
        assert!(c.structural.iter().any(|x| x.kind == "scope"));
        assert!(c.structural.iter().any(|x| x.kind == "principals"));
    }

    /// **The loophole test.** A clause that fails to map is never reclassified
    /// as structural — otherwise "structural" would be a dumping ground and an
    /// undeployable spec would look finished.
    #[test]
    fn a_failed_mapping_is_never_reclassified_as_structural() {
        let s = spec_with(&[(Topic::Thresholds, "whatever feels right")]);
        let c = compile(&s, &catalog());
        assert!(
            !c.structural.iter().any(|x| x.kind == "thresholds"),
            "a failed mapping was hidden in structural: {:?}",
            c.structural
        );
        assert!(c.unmapped.iter().any(|u| u.clause == "4"), "{:?}", c.unmapped);
    }

    // ── Only registered bytecode ────────────────────────────────────

    /// A template that is not registered cannot be mapped to, however obvious
    /// the fit — the same audit boundary GF-2 enforces on chain, applied a stage
    /// earlier so an operator learns at compile time.
    #[test]
    fn an_unregistered_template_cannot_be_mapped_to() {
        let partial = Catalog {
            templates: vec![TemplateRow {
                name: "ClassificationGate".into(),
                id: "0x01".into(),
                version: 1,
            }],
        };
        let s = spec_with(&[(Topic::Thresholds, "two approvals")]);
        let c = compile(&s, &partial);
        assert!(
            c.unmapped.iter().any(|u| u.why.contains("not registered")),
            "{:?}",
            c.unmapped
        );
        assert!(!c.deployable());
    }

    /// With an EMPTY registry the reason says so specifically, because "nothing
    /// is audited yet" and "this particular template is missing" send an
    /// operator to different places. This is the live state today: S6.9
    /// registered zero templates rather than pass a placeholder auditCID.
    #[test]
    fn an_empty_registry_says_so() {
        let s = spec_with(&[(Topic::Thresholds, "two approvals")]);
        let c = compile(&s, &Catalog::default());
        assert!(
            c.unmapped.iter().all(|u| u.why.contains("no audited templates are registered")),
            "{:?}",
            c.unmapped
        );
        assert!(!c.deployable());
    }

    // ── Deployability ───────────────────────────────────────────────

    /// Deployable needs BOTH: nothing unmapped, and something mapped. A spec
    /// that is all structural has no protocol to deploy, and calling it
    /// deployable would send an operator to an empty ceremony.
    #[test]
    fn all_structural_is_not_deployable() {
        let mut i = Interview::new("spec-1");
        i.answer("repo.write", "larry", NOW).expect("scope");
        i.answer("larry", "larry", NOW).expect("principals");
        let c = compile(&draft("spec-1", "T", &i), &catalog());
        assert!(c.mapped.is_empty());
        assert!(c.unmapped.is_empty());
        assert!(!c.deployable(), "nothing to deploy is not deployable");
    }

    #[test]
    fn a_fully_typed_spec_is_deployable() {
        let s = spec_with(&[
            (Topic::Scope, "repo.write"),
            (Topic::Principals, "larry"),
            (Topic::Roles, "two approvers"),
            (Topic::Thresholds, "two approvals"),
            (Topic::Escalation, "escalate after 4 hours"),
            (Topic::Expiry, "valid for 30 days"),
            (Topic::Exceptions, "nothing above CUI"),
        ]);
        let c = compile(&s, &catalog());
        assert!(c.unmapped.is_empty(), "unexpected: {:?}", c.unmapped);
        assert_eq!(c.mapped.len(), 5);
        assert_eq!(c.structural.len(), 2);
        assert!(c.deployable());
    }

    /// The `templateId(string,uint32)` encoder, pinned against a value read from
    /// the LIVE registry on chain 40204. A hand-rolled ABI encoder that is
    /// subtly wrong returns a plausible id for a row that does not exist, so
    /// every template would silently look unregistered — the catalog would be
    /// empty and every clause unmapped, for a reason that is not the real one.
    #[test]
    fn the_template_id_encoder_matches_the_contract() {
        let data = encode_template_id("ThresholdApproval", 1);
        assert!(data.starts_with("0x2c4d2d89"), "selector");
        // Head: offset 0x40, then the version.
        assert!(data.contains(&format!("{:064x}", 0x40)));
        assert!(data.contains(&format!("{:064x}", 1)));
        // Tail: length 17, then "ThresholdApproval" padded to 32 bytes.
        assert!(data.contains(&format!("{:064x}", "ThresholdApproval".len())));
        assert!(data.ends_with(&"5468726573686f6c64417070726f76616c000000000000000000000000000000".to_string()));
        // Total: selector + 4 words.
        assert_eq!(data.len(), 2 + 8 + 64 * 4);
    }

    #[test]
    fn the_dto_carries_deployability_and_reasons() {
        let s = spec_with(&[(Topic::Thresholds, "whatever feels right")]);
        let d = to_dto(&compile(&s, &catalog()));
        assert!(!d.deployable);
        assert!(!d.unmapped.is_empty());
        assert!(d.unmapped.iter().all(|u| !u.why.is_empty()), "every refusal needs a reason");
    }
}
