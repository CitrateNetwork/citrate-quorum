//! QRM-S7.3 — DRAFT SPEC: clauses, Gherkin, and typed params that cannot disagree.
//!
//! Stage 3 of the authoring pipeline. Interview answers and ingested documents
//! become a Governance Spec: plain-English clauses beside Gherkin scenarios
//! beside typed parameters.
//!
//! # The Gherkin is RENDERED, not written
//!
//! The planset asks for "human-readable and machine-checkable side by side".
//! The obvious way to build that is to generate prose and generate Gherkin and
//! put them next to each other — and the obvious failure is that they drift, so
//! the scenario says one thing and the parameters another, with a reader
//! trusting whichever they happened to read.
//!
//! So the Gherkin here is a **pure function of the typed parameters**
//! ([`Clause::gherkin`]). The two cannot disagree, because there is only one
//! source. Editing a parameter re-renders the scenario; there is no path that
//! edits one without the other. That is the difference between "side by side"
//! and "in agreement".
//!
//! # Nothing is drafted from nothing
//!
//! Every clause carries the [`Provenance`] it came from — an interview answer
//! (with the human who gave it) or a document chunk. A clause with no source is
//! a clause the drafter invented, and [`draft`] cannot produce one: it iterates
//! answers, not topics.
//!
//! An unanswered topic yields **no clause**, not a blank one. A spec with a
//! placeholder clause reads, to everyone downstream, like a policy that was
//! written and left empty rather than a question that was never asked.
//!
//! # Why the extractor is allowed to be approximate
//!
//! [`propose_from_documents`] scans chunks for candidate values and attaches
//! them to the interview as **proposals** (S7.2). Because a proposal is not an
//! answer and cannot become one without a human, a false positive costs a
//! rejected suggestion rather than a wrong policy. That is what lets this be a
//! plain textual scan instead of something that has to be right — the safety
//! comes from the confirmation step, not from the cleverness of the extractor.

use std::collections::BTreeMap;

use crate::ingest::Ingested;
use crate::interview::{Interview, Source, Topic};

/// Where a clause came from. There is no `None` variant on purpose.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Provenance {
    /// A human answered this in the interview.
    Interview { topic: Topic, by: String },
    /// A human confirmed a value proposed from a document, which is named.
    ConfirmedFrom { topic: Topic, by: String, from: String },
}

impl Provenance {
    pub fn render(&self) -> String {
        match self {
            Provenance::Interview { topic, by } => {
                format!("interview · {} · answered by {by}", topic.as_str())
            }
            Provenance::ConfirmedFrom { topic, by, from } => {
                format!("interview · {} · {by} confirmed a value from {from}", topic.as_str())
            }
        }
    }
}

/// One clause: prose, parameters, and the scenario rendered from them.
#[derive(Clone, Debug)]
pub struct Clause {
    pub n: String,
    pub topic: Topic,
    /// The plain-English statement.
    pub en: String,
    /// Typed parameters. `BTreeMap` so the rendering is stable — a scenario
    /// whose line order changed between runs would produce spurious diffs in
    /// review, and review is the only thing standing between a spec and a
    /// deployment.
    pub params: BTreeMap<String, String>,
    pub provenance: Provenance,
}

impl Clause {
    /// The Gherkin scenario for this clause, rendered from [`Self::params`].
    ///
    /// Deliberately has no way to be overridden. If a scenario could be edited
    /// independently of the parameters it describes, the pair would drift and
    /// the "machine-checkable" half would become prose with keywords in it.
    pub fn gherkin(&self) -> String {
        let mut out = format!("Scenario: {}\n", self.topic.as_str());
        out.push_str(&format!("  Given a governed action in scope \"{}\"\n", self.topic.as_str()));
        for (k, v) in &self.params {
            out.push_str(&format!("  And {k} is \"{v}\"\n"));
        }
        out.push_str("  When an agent proposes the action\n");
        out.push_str(&format!("  Then the policy applies clause {}\n", self.n));
        out
    }
}

#[derive(Clone, Debug)]
pub struct Spec {
    pub id: String,
    pub title: String,
    pub clauses: Vec<Clause>,
}

/// Draft a spec from an interview and the documents it was built from.
///
/// Iterates **answers**, not topics: an unanswered topic produces no clause.
pub fn draft(spec_id: &str, title: &str, interview: &Interview) -> Spec {
    let mut clauses = Vec::new();
    for turn in &interview.turns {
        let Some(ans) = turn.answer.as_ref() else {
            continue;
        };
        let n = (clauses.len() + 1).to_string();
        let mut params = BTreeMap::new();
        params.insert(param_key(turn.topic).to_string(), ans.text.clone());

        clauses.push(Clause {
            n: n.clone(),
            topic: turn.topic,
            en: format!("{} {}", prose_lead(turn.topic), ans.text),
            params,
            provenance: match &ans.source {
                Source::Human => Provenance::Interview {
                    topic: turn.topic,
                    by: ans.by.clone(),
                },
                Source::ConfirmedProposal { from } => Provenance::ConfirmedFrom {
                    topic: turn.topic,
                    by: ans.by.clone(),
                    from: from.clone(),
                },
            },
        });
    }
    Spec {
        id: spec_id.to_string(),
        title: title.to_string(),
        clauses,
    }
}

fn param_key(t: Topic) -> &'static str {
    match t {
        Topic::Scope => "action_classes",
        Topic::Principals => "principals",
        Topic::Roles => "roles",
        Topic::Thresholds => "threshold",
        Topic::Escalation => "escalation",
        Topic::Expiry => "expires",
        Topic::Exceptions => "exceptions",
    }
}

fn prose_lead(t: Topic) -> &'static str {
    match t {
        Topic::Scope => "This policy governs",
        Topic::Principals => "Accountable principals are",
        Topic::Roles => "Approval and execution roles are",
        Topic::Thresholds => "Approval thresholds are",
        Topic::Escalation => "Refusals and timeouts escalate to",
        Topic::Expiry => "This authority lapses",
        Topic::Exceptions => "Explicitly excluded:",
    }
}

/// Scan ingested documents for candidate values and attach them to the
/// interview as proposals.
///
/// **This is the caller S7.2 named.** A proposal never answers a topic; it is a
/// suggestion carried beside the question with the chunk it came from, and it
/// costs a rejected suggestion when wrong.
pub fn propose_from_documents(interview: &mut Interview, files: &[Ingested]) {
    for f in files {
        for c in &f.chunks {
            let from = format!("{}#chunk-{}", f.name, c.ordinal);
            for (topic, sentence) in candidates(&c.text) {
                // First proposal per topic wins; a second document does not
                // silently replace the first, because "two sources disagree" is
                // something the human should see rather than have resolved for
                // them. `Interview::propose` replaces by topic, so only propose
                // when the topic has none yet.
                if interview.proposal_for(topic).is_none() {
                    interview.propose(topic, &sentence, &from);
                }
            }
        }
    }
}

/// Sentences that look like they state a governed value.
///
/// Textual and deliberately simple — see the module header on why an
/// approximate extractor is safe here.
fn candidates(text: &str) -> Vec<(Topic, String)> {
    let mut out = Vec::new();
    for raw in text.split(['.', '\n']) {
        let s = raw.trim();
        if s.is_empty() || s.len() > 240 {
            continue;
        }
        let l = s.to_ascii_lowercase();
        if l.contains("approv") && (l.contains("two") || l.contains("2") || l.contains("of")) {
            out.push((Topic::Thresholds, s.to_string()));
        } else if l.contains("escalat") {
            out.push((Topic::Escalation, s.to_string()));
        } else if l.contains("expire") || l.contains("lapse") || l.contains("valid until") {
            out.push((Topic::Expiry, s.to_string()));
        } else if l.contains("except") || l.contains("exclud") {
            out.push((Topic::Exceptions, s.to_string()));
        }
    }
    out
}

// ── Tauri seam ──────────────────────────────────────────────────────────

use serde::Serialize;

#[derive(Serialize, Debug, Clone)]
pub struct SpecClauseDto {
    pub n: String,
    pub en: String,
    pub gh: String,
    /// Null until COMPILE (S7.4) maps it to an audited template. Null here means
    /// "not yet mapped", never "maps to nothing" — those are different states
    /// and S7.4 owns the second one.
    pub tpl: Option<String>,
    pub ok: bool,
    pub why: Option<String>,
}

#[derive(Serialize, Debug, Clone)]
pub struct SpecProvenanceDto {
    pub clause: String,
    pub source: String,
}

#[derive(Serialize, Debug, Clone)]
pub struct GovernanceSpecDto {
    pub id: String,
    pub title: String,
    pub classification: String,
    pub clauses: Vec<SpecClauseDto>,
    pub provenance: Vec<SpecProvenanceDto>,
}

pub fn to_dto(spec: &Spec, classification: &str) -> GovernanceSpecDto {
    GovernanceSpecDto {
        id: spec.id.clone(),
        title: spec.title.clone(),
        classification: classification.to_string(),
        clauses: spec
            .clauses
            .iter()
            .map(|c| SpecClauseDto {
                n: c.n.clone(),
                en: c.en.clone(),
                gh: c.gherkin(),
                tpl: None,
                ok: true,
                why: None,
            })
            .collect(),
        provenance: spec
            .clauses
            .iter()
            .map(|c| SpecProvenanceDto {
                clause: c.n.clone(),
                source: c.provenance.render(),
            })
            .collect(),
    }
}

/// `governance.spec` — stage 3 of the authoring pipeline.
///
/// Drafts from the interview on disk. Read-only with respect to the interview:
/// drafting must never quietly answer a question on the operator's behalf.
#[tauri::command]
pub fn governance_spec(
    app: tauri::AppHandle,
    spec_id: String,
    title: Option<String>,
    classification: Option<String>,
) -> Result<GovernanceSpecDto, String> {
    use tauri::Manager;
    let root = app
        .path()
        .app_data_dir()
        .map_err(|e| format!("no app data dir: {e}"))?;
    let interview = crate::interview::load(&root, &spec_id);
    let spec = draft(
        &spec_id,
        title.as_deref().unwrap_or("Untitled policy"),
        &interview,
    );
    Ok(to_dto(&spec, classification.as_deref().unwrap_or("Public")))
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use crate::ingest;

    const NOW: i64 = 1_785_000_000_000;

    fn answered_all() -> Interview {
        let mut i = Interview::new("spec-1");
        for (n, _) in Topic::ALL.iter().enumerate() {
            i.answer("a value", "larry", NOW + n as i64).expect("answer");
        }
        i
    }

    // ── The Gherkin cannot disagree with the params ─────────────────

    /// The scenario is rendered FROM the parameters, so there is no path that
    /// edits one without the other. "Side by side" is worth nothing if the two
    /// can drift; this is what makes them agree by construction.
    #[test]
    fn the_gherkin_is_a_function_of_the_params() {
        let mut c = Clause {
            n: "1".into(),
            topic: Topic::Thresholds,
            en: "Approval thresholds are two".into(),
            params: BTreeMap::from([("threshold".to_string(), "two".to_string())]),
            provenance: Provenance::Interview {
                topic: Topic::Thresholds,
                by: "larry".into(),
            },
        };
        let before = c.gherkin();
        assert!(before.contains("And threshold is \"two\""));

        c.params.insert("threshold".to_string(), "three".to_string());
        let after = c.gherkin();
        assert!(after.contains("And threshold is \"three\""));
        assert_ne!(before, after, "changing a param must change the scenario");
    }

    /// Rendering is stable across runs — an unstable line order would produce
    /// spurious diffs in review, and review is the only thing between a spec and
    /// a deployment.
    #[test]
    fn rendering_is_stable() {
        let c = Clause {
            n: "1".into(),
            topic: Topic::Scope,
            en: "x".into(),
            params: BTreeMap::from([
                ("z".to_string(), "1".to_string()),
                ("a".to_string(), "2".to_string()),
                ("m".to_string(), "3".to_string()),
            ]),
            provenance: Provenance::Interview {
                topic: Topic::Scope,
                by: "larry".into(),
            },
        };
        assert_eq!(c.gherkin(), c.gherkin());
        let g = c.gherkin();
        let a = g.find("And a is").expect("a");
        let m = g.find("And m is").expect("m");
        let z = g.find("And z is").expect("z");
        assert!(a < m && m < z, "params must render in a stable order");
    }

    // ── Nothing is drafted from nothing ─────────────────────────────

    /// An unanswered topic produces NO clause. A placeholder clause would read
    /// downstream like a policy that was written and left empty, rather than a
    /// question that was never asked.
    #[test]
    fn an_unanswered_topic_produces_no_clause() {
        let mut i = Interview::new("spec-1");
        i.answer("repo.write", "larry", NOW).expect("answer");

        let s = draft("spec-1", "T", &i);
        assert_eq!(s.clauses.len(), 1, "only the answered topic yields a clause");
        assert_eq!(s.clauses[0].topic, Topic::Scope);
    }

    /// Every clause carries a source. There is no variant of `Provenance` that
    /// means "invented", and `draft` iterates answers rather than topics, so an
    /// unsourced clause cannot be constructed by this path.
    #[test]
    fn every_clause_cites_a_source() {
        let s = draft("spec-1", "T", &answered_all());
        assert_eq!(s.clauses.len(), Topic::ALL.len());
        for c in &s.clauses {
            let r = c.provenance.render();
            assert!(r.contains("interview"), "{r}");
            assert!(r.contains("larry"), "the human must be named: {r}");
        }
    }

    /// A confirmed proposal keeps the document it came from all the way into
    /// the spec's provenance — so a reader can go from a clause back to the
    /// paragraph a human accepted.
    #[test]
    fn a_confirmed_proposal_carries_its_document_into_the_spec() {
        let mut i = Interview::new("spec-1");
        i.propose(Topic::Scope, "repo.write", "resolution.md#chunk-2");
        i.confirm_proposal("larry", NOW).expect("confirm");

        let s = draft("spec-1", "T", &i);
        let r = s.clauses[0].provenance.render();
        assert!(r.contains("resolution.md#chunk-2"), "{r}");
        assert!(r.contains("confirmed"), "{r}");
    }

    // ── The S7.2 caller obligation ──────────────────────────────────

    /// S7.2 left `Interview::propose` with its caller named as this function.
    /// This is that caller, and a proposal it attaches still does not answer
    /// anything.
    #[test]
    fn proposals_from_documents_do_not_answer_the_interview() {
        let body = "CUI\n\nAny spend requires two approvals.\n\nRefusals escalate to the audit committee.\n\n";
        let f = ingest::ingest_bytes("resolution.md", body.as_bytes()).expect("ingested");
        let mut i = Interview::new("spec-1");

        propose_from_documents(&mut i, &[f]);

        assert!(i.proposal_for(Topic::Thresholds).is_some(), "a threshold was proposed");
        assert_eq!(
            i.outstanding().len(),
            Topic::ALL.len(),
            "a proposal must not answer a topic"
        );
        let p = i.proposal_for(Topic::Thresholds).expect("proposal");
        assert!(p.from.starts_with("resolution.md#chunk-"), "{}", p.from);
        assert!(p.text.contains("two approvals"), "{}", p.text);
    }

    /// A second document does not silently replace the first proposal. Two
    /// sources disagreeing is something the human should see, not something
    /// resolved for them by document order.
    #[test]
    fn a_later_document_does_not_overwrite_an_earlier_proposal() {
        let a = ingest::ingest_bytes("a.md", b"CUI\n\nSpend requires two approvals.\n\n")
            .expect("a");
        let b = ingest::ingest_bytes("b.md", b"CUI\n\nSpend requires 2 of 5 approvals.\n\n")
            .expect("b");
        let mut i = Interview::new("spec-1");
        propose_from_documents(&mut i, &[a, b]);

        let p = i.proposal_for(Topic::Thresholds).expect("proposal");
        assert!(p.from.starts_with("a.md#"), "the first source must stand: {}", p.from);
    }

    // ── The DTO ─────────────────────────────────────────────────────

    /// `tpl` is null at draft time and that means "not yet mapped", not "maps to
    /// nothing" — S7.4 owns the second state, and conflating them would let an
    /// unmappable clause look merely un-compiled.
    #[test]
    fn the_dto_leaves_template_mapping_to_compile() {
        let d = to_dto(&draft("spec-1", "T", &answered_all()), "CUI");
        assert_eq!(d.clauses.len(), Topic::ALL.len());
        for c in &d.clauses {
            assert!(c.tpl.is_none(), "drafting must not claim a template");
            assert!(c.why.is_none());
            assert!(c.gh.contains("Scenario:"), "the scenario must render");
            assert!(!c.en.is_empty());
        }
        assert_eq!(d.provenance.len(), d.clauses.len(), "every clause needs a source row");
    }

    #[test]
    fn clause_numbers_are_sequential_and_match_provenance() {
        let d = to_dto(&draft("spec-1", "T", &answered_all()), "CUI");
        for (i, c) in d.clauses.iter().enumerate() {
            assert_eq!(c.n, (i + 1).to_string());
            assert_eq!(d.provenance[i].clause, c.n);
        }
    }
}
