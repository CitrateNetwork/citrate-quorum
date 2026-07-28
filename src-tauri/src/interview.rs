//! QRM-S7.2 — INTERVIEW: survey the HIC, attribute every answer, infer nothing.
//!
//! Stage 2 of the authoring pipeline. Between a pile of documents and a
//! governance spec sits a set of questions only a human can answer, and this is
//! the state machine that asks them.
//!
//! # The rule this module exists to hold
//!
//! **A value read out of an ingested document is a PROPOSAL, never an answer.**
//!
//! The temptation is obvious and strong: the board resolution says "two
//! signatures", so pre-fill the threshold and save the operator a question. Do
//! that and the spec now contains a number nobody chose, attributed to a human
//! who never saw it. When the protocol later blocks a $50M action, "who set this
//! threshold?" has no answer — and the audit trail says a person did.
//!
//! So a proposal is carried NEXT TO the question, with the file and chunk it
//! came from, and the topic stays [`outstanding`](InterviewState::outstanding)
//! until a human confirms or overrides it. Confirming is one action, so this
//! costs a click, not a conversation — but the click is the point: it is the
//! moment a human takes responsibility for the value.
//!
//! # Attribution
//!
//! Every answer records who gave it and when. An interview whose answers cannot
//! be attributed produces a spec nobody is accountable for, which is the exact
//! failure this product exists to prevent.

use serde::{Deserialize, Serialize};

/// The topics a governance spec cannot be drafted without.
///
/// Fixed and ordered. A missing topic is not a gap the drafter fills in with a
/// sensible default — it is a question that was never asked.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Topic {
    Scope,
    Principals,
    Roles,
    Thresholds,
    Escalation,
    Expiry,
    Exceptions,
}

impl Topic {
    pub const ALL: &'static [Topic] = &[
        Topic::Scope,
        Topic::Principals,
        Topic::Roles,
        Topic::Thresholds,
        Topic::Escalation,
        Topic::Expiry,
        Topic::Exceptions,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            Topic::Scope => "scope",
            Topic::Principals => "principals",
            Topic::Roles => "roles",
            Topic::Thresholds => "thresholds",
            Topic::Escalation => "escalation",
            Topic::Expiry => "expiry",
            Topic::Exceptions => "exceptions",
        }
    }

    /// The question put to the human. Phrased so that "I don't know" is a
    /// legible answer rather than something the operator has to invent around.
    pub fn question(self) -> &'static str {
        match self {
            Topic::Scope => {
                "Which actions does this policy govern? Name the action classes \
                 (for example repo.write, treasury.transfer)."
            }
            Topic::Principals => {
                "Who is accountable for actions under this policy? Name the humans, \
                 not the agents — an agent acts, a principal answers for it."
            }
            Topic::Roles => "Which roles may approve, and which may execute?",
            Topic::Thresholds => {
                "How many approvals are required, and above what cost does an \
                 action stop being unattended?"
            }
            Topic::Escalation => "When an approval is refused or times out, who is it escalated to?",
            Topic::Expiry => "When does this authority lapse, and what happens to work in flight?",
            Topic::Exceptions => {
                "What is explicitly excluded from this policy? Write \"none\" if \
                 nothing is — an empty answer is not the same as none."
            }
        }
    }
}

/// Where an answer came from. Recorded so an auditor can tell a value a human
/// chose from a value a human merely accepted.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum Source {
    /// A human wrote it.
    Human,
    /// A human confirmed a value proposed from a document, which is named.
    ConfirmedProposal { from: String },
}

#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct Answer {
    pub text: String,
    /// The principal who answered. Never empty — see [`InterviewError`].
    pub by: String,
    pub at_ms: i64,
    pub source: Source,
}

/// A value the drafter read out of an ingested document.
///
/// **Not an answer.** Carried alongside the question so the human can confirm it
/// in one action, with the provenance visible.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct Proposal {
    pub topic: Topic,
    pub text: String,
    /// Where it was read from — file and chunk, so a human can check it.
    pub from: String,
}

#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct Turn {
    pub topic: Topic,
    pub question: String,
    pub answer: Option<Answer>,
}

/// The interview for one spec.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Interview {
    pub spec_id: String,
    pub turns: Vec<Turn>,
    pub proposals: Vec<Proposal>,
}

#[derive(Debug, PartialEq, Eq)]
pub enum InterviewError {
    /// An answer with no principal cannot be attributed, so it is not recorded.
    Unattributed,
    /// An empty answer is not an answer. `Exceptions` in particular has "none"
    /// as a real answer, which is why blank cannot be allowed to stand in for it.
    Empty,
    /// Confirming a proposal that was never made.
    NoSuchProposal(Topic),
    /// The interview has no such topic outstanding.
    AlreadyAnswered(Topic),
}

impl InterviewError {
    pub fn why(&self) -> String {
        match self {
            InterviewError::Unattributed => {
                "an answer must name the principal giving it — an unattributable \
                 answer produces a spec nobody is accountable for"
                    .to_string()
            }
            InterviewError::Empty => {
                "an empty answer is not an answer; write \"none\" if that is what \
                 you mean, so the record says you decided rather than skipped"
                    .to_string()
            }
            InterviewError::NoSuchProposal(t) => {
                format!("no proposal was made for {}", t.as_str())
            }
            InterviewError::AlreadyAnswered(t) => {
                format!("{} is already answered", t.as_str())
            }
        }
    }
}

impl Interview {
    pub fn new(spec_id: &str) -> Self {
        Self {
            spec_id: spec_id.to_string(),
            turns: Topic::ALL
                .iter()
                .map(|t| Turn {
                    topic: *t,
                    question: t.question().to_string(),
                    answer: None,
                })
                .collect(),
            proposals: Vec::new(),
        }
    }

    /// Attach a value read from a document. Does NOT answer the topic.
    ///
    /// Called by `spec::propose_from_documents` (S7.3), which reads the
    /// ingested chunks. S7.2 owns the rule that a proposal is not an answer;
    /// the tests in both modules are what hold it.
    pub fn propose(&mut self, topic: Topic, text: &str, from: &str) {
        self.proposals.retain(|p| p.topic != topic);
        self.proposals.push(Proposal {
            topic,
            text: text.to_string(),
            from: from.to_string(),
        });
    }

    /// The next unanswered topic, or `None` when the interview is complete.
    ///
    /// A topic that carries only a proposal is still unanswered — that is the
    /// whole point of the distinction.
    pub fn pending(&self) -> Option<Topic> {
        self.turns
            .iter()
            .find(|t| t.answer.is_none())
            .map(|t| t.topic)
    }

    /// Every topic still without a human answer.
    pub fn outstanding(&self) -> Vec<Topic> {
        self.turns
            .iter()
            .filter(|t| t.answer.is_none())
            .map(|t| t.topic)
            .collect()
    }

    pub fn is_complete(&self) -> bool {
        self.pending().is_none()
    }

    /// The proposal attached to a topic, if any.
    pub fn proposal_for(&self, topic: Topic) -> Option<&Proposal> {
        self.proposals.iter().find(|p| p.topic == topic)
    }

    /// Record a human's answer to `topic`.
    fn record(
        &mut self,
        topic: Topic,
        text: &str,
        by: &str,
        at_ms: i64,
        source: Source,
    ) -> Result<(), InterviewError> {
        if by.trim().is_empty() {
            return Err(InterviewError::Unattributed);
        }
        if text.trim().is_empty() {
            return Err(InterviewError::Empty);
        }
        let turn = self
            .turns
            .iter_mut()
            .find(|t| t.topic == topic)
            .ok_or(InterviewError::AlreadyAnswered(topic))?;
        if turn.answer.is_some() {
            return Err(InterviewError::AlreadyAnswered(topic));
        }
        turn.answer = Some(Answer {
            text: text.trim().to_string(),
            by: by.to_string(),
            at_ms,
            source,
        });
        Ok(())
    }

    /// Answer the currently pending topic in the human's own words.
    pub fn answer(&mut self, text: &str, by: &str, at_ms: i64) -> Result<(), InterviewError> {
        let topic = self.pending().ok_or(InterviewError::AlreadyAnswered(Topic::Scope))?;
        self.record(topic, text, by, at_ms, Source::Human)
    }

    /// Confirm the proposal attached to the pending topic.
    ///
    /// The recorded answer is the proposal's text, but its [`Source`] says it
    /// was proposed and names where from — so an auditor can always separate a
    /// value a human composed from one they accepted.
    pub fn confirm_proposal(&mut self, by: &str, at_ms: i64) -> Result<(), InterviewError> {
        let topic = self
            .pending()
            .ok_or(InterviewError::AlreadyAnswered(Topic::Scope))?;
        let p = self
            .proposal_for(topic)
            .cloned()
            .ok_or(InterviewError::NoSuchProposal(topic))?;
        self.record(
            topic,
            &p.text,
            by,
            at_ms,
            Source::ConfirmedProposal { from: p.from },
        )
    }
}

// ── Tauri seam ──────────────────────────────────────────────────────────

#[derive(Serialize, Debug, Clone)]
pub struct TurnDto {
    pub q: String,
    pub a: String,
    /// Empty until answered. The surface shows this next to the answer so a
    /// reader never has to ask who said it.
    pub by: String,
    pub at: String,
    /// `"human"` or `"confirmed: <provenance>"`.
    pub source: String,
}

#[derive(Serialize, Debug, Clone)]
pub struct InterviewStateDto {
    pub spec_id: String,
    pub turns: Vec<TurnDto>,
    /// The question awaiting an answer, or null when complete.
    pub pending: Option<String>,
    pub outstanding: Vec<String>,
    /// True only when every topic has a human answer.
    pub complete: bool,
    /// The proposal for the pending topic, if one was read from a document.
    /// Rendered as a suggestion to confirm — never as a filled-in answer.
    pub proposal: Option<ProposalDto>,
}

#[derive(Serialize, Debug, Clone)]
pub struct ProposalDto {
    pub text: String,
    pub from: String,
}

pub fn to_dto(iv: &Interview) -> InterviewStateDto {
    InterviewStateDto {
        spec_id: iv.spec_id.clone(),
        turns: iv
            .turns
            .iter()
            .map(|t| {
                let (a, by, at, source) = match &t.answer {
                    None => (String::new(), String::new(), String::new(), String::new()),
                    Some(ans) => (
                        ans.text.clone(),
                        ans.by.clone(),
                        crate::backend::iso_utc(ans.at_ms),
                        match &ans.source {
                            Source::Human => "human".to_string(),
                            Source::ConfirmedProposal { from } => format!("confirmed: {from}"),
                        },
                    ),
                };
                TurnDto {
                    q: t.question.clone(),
                    a,
                    by,
                    at,
                    source,
                }
            })
            .collect(),
        pending: iv.pending().map(|t| t.question().to_string()),
        outstanding: iv.outstanding().iter().map(|t| t.as_str().to_string()).collect(),
        complete: iv.is_complete(),
        proposal: iv.pending().and_then(|t| iv.proposal_for(t)).map(|p| ProposalDto {
            text: p.text.clone(),
            from: p.from.clone(),
        }),
    }
}

// ── Persistence ─────────────────────────────────────────
//
// An interview that loses answers on restart would ask a human the same
// questions twice and record the second set as if the first never happened.
// Answers are evidence, so they are written where the rest of the evidence
// lives.

use std::path::PathBuf;

fn interview_path(root: &std::path::Path, spec_id: &str) -> PathBuf {
    root.join("interviews").join(format!("{spec_id}.json"))
}

pub fn load(root: &std::path::Path, spec_id: &str) -> Interview {
    match std::fs::read_to_string(interview_path(root, spec_id)) {
        Ok(raw) => serde_json::from_str(&raw).unwrap_or_else(|_| Interview::new(spec_id)),
        Err(_) => Interview::new(spec_id),
    }
}

pub fn save(root: &std::path::Path, iv: &Interview) -> Result<(), String> {
    let path = interview_path(root, &iv.spec_id);
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    let body = serde_json::to_vec_pretty(iv).map_err(|e| e.to_string())?;
    std::fs::write(&path, body).map_err(|e| e.to_string())
}

/// `governance.interview` — stage 2 of the authoring pipeline.
///
/// Omit `answer` to read the current question (start or resume). Supply it to
/// record an answer and advance. The literal `__confirm__` confirms the
/// proposal attached to the pending topic — a distinct verb, because confirming
/// and composing are different acts and the record must say which.
#[tauri::command]
pub fn governance_interview(
    app: tauri::AppHandle,
    spec_id: String,
    answer: Option<String>,
) -> Result<InterviewStateDto, String> {
    use tauri::Manager;
    let root = app
        .path()
        .app_data_dir()
        .map_err(|e| format!("no app data dir: {e}"))?;

    // Attribution is READ, never accepted from the caller. An answer that names
    // whoever the frontend says gave it is not attributable — a bug or a
    // tampered renderer could record a decision against a colleague who never
    // saw the question. The operator is whoever this installation is signed in
    // as, and that is the only name this command will write.
    let store = crate::store::EvidenceStore::open(root.clone())
        .map_err(|e| format!("evidence store: {e}"))?;
    let by = store
        .load_operator()
        .ok_or("no operator is set for this installation — an answer cannot be \
                attributed, so it is not recorded")?;

    let mut iv = load(&root, &spec_id);

    if let Some(text) = answer {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis() as i64)
            .unwrap_or_default();
        let res = if text == "__confirm__" {
            iv.confirm_proposal(&by, now)
        } else {
            iv.answer(&text, &by, now)
        };
        res.map_err(|e| e.why())?;
        save(&root, &iv)?;
    }
    Ok(to_dto(&iv))
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    const NOW: i64 = 1_785_000_000_000;

    fn iv() -> Interview {
        Interview::new("spec-1")
    }

    // ── The rule this module exists for ─────────────────────────────

    /// **A proposal is not an answer.** The board resolution saying "two
    /// signatures" does not mean a human chose two — and a spec containing a
    /// number nobody chose, attributed to a human who never saw it, is exactly
    /// what makes "who set this threshold?" unanswerable later.
    #[test]
    fn a_proposal_does_not_answer_anything() {
        let mut i = iv();
        for t in Topic::ALL {
            i.propose(*t, "read from the doc", "resolution.md#chunk-2");
        }
        assert_eq!(i.outstanding().len(), Topic::ALL.len(), "proposals answered a topic");
        assert!(!i.is_complete());
        assert_eq!(i.pending(), Some(Topic::Scope));
    }

    /// Confirming is one action — but it records that it WAS a confirmation, and
    /// where the value came from, so an auditor can separate a value a human
    /// composed from one they merely accepted.
    #[test]
    fn a_confirmed_proposal_records_that_it_was_proposed() {
        let mut i = iv();
        i.propose(Topic::Scope, "repo.write, treasury.transfer", "resolution.md#chunk-2");
        i.confirm_proposal("larry", NOW).expect("confirm");

        let t = &i.turns[0];
        let a = t.answer.as_ref().expect("answered");
        assert_eq!(a.text, "repo.write, treasury.transfer");
        assert_eq!(a.by, "larry");
        assert_eq!(
            a.source,
            Source::ConfirmedProposal { from: "resolution.md#chunk-2".to_string() },
            "the provenance must survive into the record"
        );
    }

    /// A human's own words are recorded as theirs, not as a confirmation.
    #[test]
    fn a_typed_answer_is_recorded_as_human() {
        let mut i = iv();
        i.propose(Topic::Scope, "proposed value", "doc#1");
        i.answer("something else entirely", "larry", NOW).expect("answer");
        assert_eq!(i.turns[0].answer.as_ref().expect("a").source, Source::Human);
        assert_eq!(i.turns[0].answer.as_ref().expect("a").text, "something else entirely");
    }

    /// Confirming a proposal that was never made is an error, not a silent
    /// no-op — otherwise a UI bug would look like an answered question.
    #[test]
    fn confirming_a_proposal_that_does_not_exist_is_refused() {
        let mut i = iv();
        assert_eq!(
            i.confirm_proposal("larry", NOW).expect_err("refuse"),
            InterviewError::NoSuchProposal(Topic::Scope)
        );
        assert!(!i.is_complete());
    }

    // ── Attribution ─────────────────────────────────────────────────

    /// An unattributable answer is refused. A spec nobody is accountable for is
    /// the failure this product exists to prevent.
    #[test]
    fn an_answer_without_a_principal_is_refused() {
        let mut i = iv();
        assert_eq!(
            i.answer("repo.write", "  ", NOW).expect_err("refuse"),
            InterviewError::Unattributed
        );
        assert!(i.turns[0].answer.is_none(), "nothing may be recorded");
    }

    /// Every answer carries who and when.
    #[test]
    fn every_answer_carries_who_and_when() {
        let mut i = iv();
        i.answer("repo.write", "larry", NOW).expect("answer");
        let a = i.turns[0].answer.as_ref().expect("a");
        assert_eq!(a.by, "larry");
        assert_eq!(a.at_ms, NOW);
    }

    // ── Nothing skipped, nothing defaulted ──────────────────────────

    /// An empty answer is refused. `Exceptions` is the reason this matters:
    /// "none" is a real answer, so blank must not be allowed to stand in for it
    /// — otherwise "we decided there are no exceptions" and "nobody looked" are
    /// the same record.
    #[test]
    fn an_empty_answer_is_refused_because_none_is_a_real_answer() {
        let mut i = iv();
        assert_eq!(
            i.answer("   ", "larry", NOW).expect_err("refuse"),
            InterviewError::Empty
        );

        // …and "none" is accepted.
        i.answer("none", "larry", NOW).expect("none is an answer");
        assert_eq!(i.turns[0].answer.as_ref().expect("a").text, "none");
    }

    /// The interview is complete only when every topic has a human answer, and
    /// it asks them in a fixed order so no topic can be skipped.
    #[test]
    fn the_interview_completes_only_when_every_topic_is_answered() {
        let mut i = iv();
        for (n, t) in Topic::ALL.iter().enumerate() {
            assert_eq!(i.pending(), Some(*t), "asked out of order");
            assert!(!i.is_complete(), "complete with {} topics left", Topic::ALL.len() - n);
            i.answer("an answer", "larry", NOW + n as i64).expect("answer");
        }
        assert!(i.is_complete());
        assert!(i.pending().is_none());
        assert!(i.outstanding().is_empty());
    }

    /// An answered topic cannot be silently overwritten — changing a recorded
    /// answer is an amendment, not a re-answer, and it is not this call.
    #[test]
    fn an_answered_topic_is_not_overwritten() {
        let mut i = iv();
        i.answer("first", "larry", NOW).expect("answer");
        assert_eq!(
            i.record(Topic::Scope, "second", "someone-else", NOW, Source::Human)
                .expect_err("refuse"),
            InterviewError::AlreadyAnswered(Topic::Scope)
        );
        assert_eq!(i.turns[0].answer.as_ref().expect("a").text, "first");
    }

    /// The DTO surfaces the proposal separately from the answer, so the UI
    /// cannot render a suggestion in the answer field by accident.
    #[test]
    fn the_dto_keeps_proposals_out_of_the_answer_field() {
        let mut i = iv();
        i.propose(Topic::Scope, "repo.write", "resolution.md#chunk-2");
        let d = to_dto(&i);
        assert_eq!(d.turns[0].a, "", "an unanswered turn has no answer text");
        assert_eq!(d.turns[0].by, "");
        let p = d.proposal.as_ref().expect("proposal surfaced");
        assert_eq!(p.text, "repo.write");
        assert_eq!(p.from, "resolution.md#chunk-2");
        assert_eq!(d.outstanding.len(), Topic::ALL.len());
    }

    #[test]
    fn the_dto_reports_completion_and_attribution() {
        let mut i = iv();
        for (n, _) in Topic::ALL.iter().enumerate() {
            i.answer("x", "larry", NOW + n as i64).expect("answer");
        }
        let d = to_dto(&i);
        assert!(d.pending.is_none());
        assert!(d.outstanding.is_empty());
        assert!(d.proposal.is_none());
        assert_eq!(d.turns[0].by, "larry");
        assert_eq!(d.turns[0].source, "human");
        assert!(!d.turns[0].at.is_empty(), "the timestamp must render");
    }
}
