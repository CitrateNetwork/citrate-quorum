//! QRM-S7.5 — SIMULATE: replay real decisions, and be honest about what it cannot say.
//!
//! Stage 5. The executive's proof: take the decisions this tenant actually
//! recorded, run them through the proposed policy, and show what would have
//! changed.
//!
//! # R-B, and the three ways a replay flatters
//!
//! > **R-B.** A replay that only shows blocked-in-hindsight cases is a demo,
//! > not evidence.
//!
//! There are three distinct ways to produce a comforting number, and this module
//! is built to refuse all of them.
//!
//! **1. Reporting only what would have been blocked.** So [`Replay`] carries
//! `allowed` and `unchanged` beside `blocked` and `paused`, and `unchanged` —
//! decisions the policy would not have touched at all — is the honest majority
//! in any real corpus. A run with zero unchanged means the replay corpus is too
//! narrow, not that the policy is powerful.
//!
//! **2. Replaying an empty corpus.** A tenant with no recorded decisions
//! produces zeros, and zeros render exactly like "this policy is harmless".
//! So an empty corpus is reported as [`Replay::corpus_note`] — the simulation
//! says it had nothing to run rather than presenting a clean bill of health.
//!
//! **3. Silently skipping clauses it cannot replay.** This is the subtle one and
//! it bit immediately: `DecisionRecord` does not carry a classification, so a
//! `ClassificationGate` clause **cannot be replayed against this ledger**. A
//! simulation that skipped it would under-report blocks and look like a policy
//! that never refuses anything. Every clause that was not replayed is listed in
//! [`Replay::not_simulated`] with the reason, and the surface must show that
//! list next to the counts — a number produced by half a policy is not a number
//! about that policy.

use crate::compile::Compiled;
use quorum_audit::{DecisionRecord, Verdict};
use quorum_policy::HicLevel;

/// What the proposed policy would have done to one recorded decision.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Outcome {
    /// The policy does not govern this action class at all.
    Unchanged,
    /// The policy applies and permits it — same outcome as recorded.
    Allowed,
    /// The policy would have required a human where none was asked.
    Paused,
    /// The policy would have refused it.
    ///
    /// **Currently unreachable, and that is the honest state.** The only
    /// template this module can replay is `ThresholdApproval`, which pauses
    /// rather than refuses. The clauses that WOULD block — `ClassificationGate`,
    /// `IncidentEscalation` — cannot be replayed against a `DecisionRecord`
    /// (see [`replayable`]). So a simulation today reports `blocked: 0`, and an
    /// executive reading that must see `not_simulated` beside it. Kept in the
    /// model rather than deleted, because deleting it would make the zero look
    /// like a finding instead of a limit.
    #[allow(dead_code)]
    Blocked,
}

#[derive(Clone, Debug)]
pub struct Sample {
    pub agent: String,
    pub action_class: String,
    pub was: String,
    pub would: String,
    pub why: String,
}

/// A clause the replay could not exercise, and why. Not an error — a limit.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NotSimulated {
    pub clause: String,
    pub why: String,
}

#[derive(Clone, Debug, Default)]
pub struct Replay {
    pub from_ms: i64,
    pub to_ms: i64,
    pub total: usize,
    pub blocked: usize,
    pub paused: usize,
    pub allowed: usize,
    pub unchanged: usize,
    pub samples: Vec<Sample>,
    /// Clause numbers actually exercised.
    pub simulated: Vec<String>,
    /// Clauses that could not be exercised, each with a reason.
    pub not_simulated: Vec<NotSimulated>,
    /// Set when there was nothing to replay. Zeros without this note read as a
    /// clean bill of health.
    pub corpus_note: Option<String>,
}

impl Replay {
    /// Did every clause of the policy actually get exercised?
    pub fn is_complete(&self) -> bool {
        self.not_simulated.is_empty() && self.corpus_note.is_none()
    }
}

/// Which templates this module knows how to replay against a `DecisionRecord`.
///
/// Deliberately explicit. A template absent from here is reported in
/// `not_simulated` rather than quietly contributing nothing.
fn replayable(template: &str) -> Result<(), &'static str> {
    match template {
        "ThresholdApproval" => Ok(()),
        "SegregationOfDuties" => Err(
            "the ledger records one actor per decision, not proposer/approver/executor \
             separately, so role disjointness cannot be replayed against it",
        ),
        "ClassificationGate" => Err(
            "DecisionRecord does not carry the classification of the material an \
             action touched, so a clearance ceiling cannot be replayed against this \
             ledger",
        ),
        "IncidentEscalation" => Err(
            "replaying an SLA needs the contradiction ledger's history alongside the \
             decision history; this replay reads decisions only",
        ),
        "TimeBoundedElevation" => Err(
            "the ledger does not record whether the actor held a live elevation at the \
             time, so an elevation window cannot be replayed",
        ),
        other => {
            let _ = other;
            Err("no replay rule exists for this template")
        }
    }
}

/// Replay `decisions` through `policy`.
///
/// `scope` is the set of action classes the policy governs, taken from the
/// spec's structural clauses. A decision outside it is [`Outcome::Unchanged`].
pub fn replay(policy: &Compiled, decisions: &[DecisionRecord], scope: &[String]) -> Replay {
    let mut out = Replay::default();

    // Which clauses can actually be exercised.
    let mut threshold: Option<u32> = None;
    for m in &policy.mapped {
        match replayable(&m.template_name) {
            Ok(()) => {
                out.simulated.push(m.clause.clone());
                if m.template_name == "ThresholdApproval" {
                    threshold = m.params.get("threshold").and_then(|v| v.parse().ok());
                }
            }
            Err(why) => out.not_simulated.push(NotSimulated {
                clause: m.clause.clone(),
                why: why.to_string(),
            }),
        }
    }
    // Unmapped clauses were never compiled, so they cannot be replayed either —
    // and an executive reading a count needs to know the policy was incomplete
    // before it was simulated.
    for u in &policy.unmapped {
        out.not_simulated.push(NotSimulated {
            clause: u.clause.clone(),
            why: format!("clause did not compile, so it was not replayed: {}", u.why),
        });
    }

    if decisions.is_empty() {
        out.corpus_note = Some(
            "this tenant has no recorded decisions in range, so there was nothing to \
             replay — these zeros mean 'no evidence', not 'no impact'"
                .to_string(),
        );
        return out;
    }

    out.total = decisions.len();
    out.from_ms = decisions.iter().map(|d| d.timestamp_ms).min().unwrap_or(0);
    out.to_ms = decisions.iter().map(|d| d.timestamp_ms).max().unwrap_or(0);

    for d in decisions {
        let (outcome, why) = evaluate(d, scope, threshold);
        match outcome {
            Outcome::Unchanged => out.unchanged += 1,
            Outcome::Allowed => out.allowed += 1,
            Outcome::Paused => out.paused += 1,
            Outcome::Blocked => out.blocked += 1,
        }
        // Sample the cases that CHANGE, since those are what an operator needs
        // to look at — but the counts above include everything.
        if outcome != Outcome::Unchanged && out.samples.len() < 12 {
            out.samples.push(Sample {
                agent: d.agent.clone(),
                action_class: d.action_class.clone(),
                was: format!("{:?}", d.verdict),
                would: format!("{outcome:?}"),
                why: why.to_string(),
            });
        }
    }
    out
}

fn evaluate(
    d: &DecisionRecord,
    scope: &[String],
    threshold: Option<u32>,
) -> (Outcome, &'static str) {
    if !scope.iter().any(|s| s == &d.action_class) {
        return (Outcome::Unchanged, "outside the policy's action classes");
    }
    let Some(n) = threshold else {
        return (
            Outcome::Allowed,
            "the policy governs this class but no replayable clause constrains it",
        );
    };
    if n == 0 {
        return (Outcome::Allowed, "threshold of zero constrains nothing");
    }
    // The action already paused for a human: the policy asks for the same thing.
    if d.hic == HicLevel::ApproveEach || d.verdict == Verdict::RequireApproval {
        return (Outcome::Allowed, "already required a human, as this policy does");
    }
    // It ran unattended, and the policy would have required approval.
    (
        Outcome::Paused,
        "ran unattended; this policy would have required approval first",
    )
}

// ── Tauri seam ──────────────────────────────────────────────────────────

use serde::Serialize;

#[derive(Serialize, Debug, Clone)]
pub struct SampleDto {
    pub id: String,
    pub agent: String,
    pub action: String,
    pub was: String,
    pub would: String,
    pub why: String,
}

#[derive(Serialize, Debug, Clone)]
pub struct NotSimulatedDto {
    pub clause: String,
    pub why: String,
}

#[derive(Serialize, Debug, Clone)]
pub struct SimulationDto {
    pub range: String,
    pub total: usize,
    pub blocked: usize,
    pub approvals: usize,
    pub allowed: usize,
    pub unchanged: usize,
    pub samples: Vec<SampleDto>,
    pub simulated: Vec<String>,
    /// Clauses the replay could not exercise. The surface MUST show this beside
    /// the counts — a number produced by half a policy is not a number about
    /// that policy.
    pub not_simulated: Vec<NotSimulatedDto>,
    pub complete: bool,
    pub corpus_note: Option<String>,
}

pub fn to_dto(r: &Replay) -> SimulationDto {
    SimulationDto {
        range: if r.total == 0 {
            "no decisions in range".to_string()
        } else {
            format!(
                "{} — {}",
                crate::backend::iso_utc(r.from_ms),
                crate::backend::iso_utc(r.to_ms)
            )
        },
        total: r.total,
        blocked: r.blocked,
        approvals: r.paused,
        allowed: r.allowed,
        unchanged: r.unchanged,
        samples: r
            .samples
            .iter()
            .enumerate()
            .map(|(i, s)| SampleDto {
                id: format!("s-{i}"),
                agent: s.agent.clone(),
                action: s.action_class.clone(),
                was: s.was.clone(),
                would: s.would.clone(),
                why: s.why.clone(),
            })
            .collect(),
        simulated: r.simulated.clone(),
        not_simulated: r
            .not_simulated
            .iter()
            .map(|n| NotSimulatedDto {
                clause: n.clause.clone(),
                why: n.why.clone(),
            })
            .collect(),
        complete: r.is_complete(),
        corpus_note: r.corpus_note.clone(),
    }
}

/// `governance.simulate` — stage 5 of the authoring pipeline.
///
/// Replays the tenant's REAL recorded decisions. There is no synthetic corpus
/// and no fallback: a tenant with no decisions gets a stated note, because a
/// fabricated replay would be the most persuasive lie this product could tell.
#[tauri::command]
pub fn governance_simulate(
    app: tauri::AppHandle,
    spec_id: String,
) -> Result<SimulationDto, String> {
    use tauri::Manager;
    let root = app
        .path()
        .app_data_dir()
        .map_err(|e| format!("no app data dir: {e}"))?;

    let book = crate::addresses::AddressBook::load().map_err(|e| e.to_string())?;
    let registry = book
        .get("GovernanceTemplateRegistry")
        .ok_or("GovernanceTemplateRegistry is not in the address book")?;
    let rpc = crate::chain::Rpc::from_book(&book)?;
    let catalog = crate::compile::catalog_from_chain(&rpc, registry)?;

    let interview = crate::interview::load(&root, &spec_id);
    let spec = crate::spec::draft(&spec_id, "Untitled policy", &interview);
    let compiled = crate::compile::compile(&spec, &catalog);

    // The action classes the policy governs come from the spec's structural
    // clauses — the same ones S7.7 will bind.
    let scope: Vec<String> = compiled
        .structural
        .iter()
        .filter(|s| s.kind == "scope")
        .flat_map(|s| {
            s.value
                .split([',', ';'])
                .map(|t| t.trim().to_string())
                .filter(|t| !t.is_empty())
                .collect::<Vec<_>>()
        })
        .collect();

    let store = crate::store::EvidenceStore::open(crate::store::evidence_dir(&root))
        .map_err(|e| format!("evidence store: {e}"))?;
    // The tenant scope lives in the backend, never in a caller's argument
    // (the bridge's own rule: no frontend call names a tenant). A command with
    // no scope established fails closed rather than replaying someone else's
    // decisions.
    let tenant = store
        .load_scope()
        .ok_or("no tenant scope is established — establish one before simulating, \
                so the replay reads this tenant's decisions and no one else's")?;
    let tid = quorum_tenancy::TenantId::new(&tenant).map_err(|e| e.to_string())?;
    let chain = store
        .load_chain(&tid)
        .map_err(|e| format!("cannot read the decision history: {e}"))?;
    let decisions: Vec<quorum_audit::DecisionRecord> = chain.records().cloned().collect();

    Ok(to_dto(&replay(&compiled, &decisions, &scope)))
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use crate::compile::{Compiled, Mapped, Unmapped};
    use std::collections::BTreeMap;

    fn rec(action: &str, hic: HicLevel, verdict: Verdict, ts: i64) -> DecisionRecord {
        DecisionRecord {
            agent: "agent-1".into(),
            principal: Some("larry".into()),
            grant_id: Some("g-1".into()),
            action_class: action.into(),
            params_hash: [0u8; 32],
            verdict,
            hic,
            model_id: "m".into(),
            correlation_id: "c".into(),
            timestamp_ms: ts,
        }
    }

    fn policy_with_threshold(n: &str) -> Compiled {
        Compiled {
            spec_id: "spec-1".into(),
            mapped: vec![Mapped {
                clause: "1".into(),
                template_id: "0x01".into(),
                template_name: "ThresholdApproval".into(),
                params: BTreeMap::from([("threshold".to_string(), n.to_string())]),
            }],
            ..Default::default()
        }
    }

    // ── R-B: the three ways a replay flatters ───────────────────────

    /// A replay reports what it would have ALLOWED and what it would not have
    /// touched, not only what it would have blocked. `unchanged` is the honest
    /// majority in any real corpus.
    #[test]
    fn a_replay_reports_allowed_and_unchanged_not_only_blocked() {
        let decisions = vec![
            rec("repo.write", HicLevel::Budgeted, Verdict::Allow, 100),
            rec("repo.write", HicLevel::ApproveEach, Verdict::Allow, 200),
            rec("calendar.write", HicLevel::Budgeted, Verdict::Allow, 300),
            rec("mail.send", HicLevel::Budgeted, Verdict::Allow, 400),
        ];
        let r = replay(
            &policy_with_threshold("2"),
            &decisions,
            &["repo.write".to_string()],
        );
        assert_eq!(r.total, 4);
        assert_eq!(r.paused, 1, "the unattended repo.write would now pause");
        assert_eq!(r.allowed, 1, "the one that already asked a human is unchanged in outcome");
        assert_eq!(r.unchanged, 2, "two actions are outside the policy entirely");
        assert_eq!(r.blocked + r.paused + r.allowed + r.unchanged, r.total);
    }

    /// **An empty corpus is not a clean bill of health.** Zeros render exactly
    /// like "this policy is harmless", so the replay says it had nothing to run.
    #[test]
    fn an_empty_corpus_says_so_rather_than_reporting_zeros() {
        let r = replay(&policy_with_threshold("2"), &[], &["repo.write".to_string()]);
        assert_eq!(r.total, 0);
        let note = r.corpus_note.as_ref().expect("must carry a note");
        assert!(note.contains("no evidence"), "{note}");
        assert!(!r.is_complete(), "a replay with no corpus is not complete");
    }

    /// **The subtle one.** A clause the replay cannot exercise is listed with a
    /// reason, never silently skipped — a skipped ClassificationGate would
    /// under-report blocks and make the policy look like it never refuses.
    #[test]
    fn a_clause_that_cannot_be_replayed_is_named_not_skipped() {
        let mut p = policy_with_threshold("2");
        p.mapped.push(Mapped {
            clause: "2".into(),
            template_id: "0x02".into(),
            template_name: "ClassificationGate".into(),
            params: BTreeMap::new(),
        });

        let r = replay(&p, &[rec("repo.write", HicLevel::Budgeted, Verdict::Allow, 1)], &["repo.write".into()]);
        assert_eq!(r.simulated, vec!["1".to_string()]);
        let ns = r.not_simulated.iter().find(|n| n.clause == "2").expect("named");
        assert!(ns.why.contains("classification"), "{}", ns.why);
        assert!(!r.is_complete(), "an unreplayable clause makes the run incomplete");
    }

    /// A clause that never compiled cannot be replayed either, and an executive
    /// reading a count needs to know the policy was incomplete BEFORE it ran.
    #[test]
    fn an_unmapped_clause_is_reported_as_not_simulated() {
        let mut p = policy_with_threshold("2");
        p.unmapped.push(Unmapped {
            clause: "3".into(),
            why: "cannot read an approval count".into(),
        });
        let r = replay(&p, &[rec("repo.write", HicLevel::Budgeted, Verdict::Allow, 1)], &["repo.write".into()]);
        let ns = r.not_simulated.iter().find(|n| n.clause == "3").expect("named");
        assert!(ns.why.contains("did not compile"), "{}", ns.why);
        assert!(!r.is_complete());
    }

    // ── The replay itself ───────────────────────────────────────────

    /// An action outside the policy's scope is `unchanged` — the policy would
    /// not have applied at all. This is the count that keeps the others honest.
    #[test]
    fn actions_outside_scope_are_unchanged() {
        let r = replay(
            &policy_with_threshold("2"),
            &[rec("mail.send", HicLevel::Budgeted, Verdict::Allow, 1)],
            &["repo.write".to_string()],
        );
        assert_eq!(r.unchanged, 1);
        assert_eq!(r.paused, 0);
        assert!(r.samples.is_empty(), "unchanged cases are not samples");
    }

    /// An action that already paused for a human is not counted as a change.
    /// Counting it would inflate the policy's apparent effect.
    #[test]
    fn an_action_that_already_asked_a_human_is_not_a_change() {
        let r = replay(
            &policy_with_threshold("2"),
            &[rec("repo.write", HicLevel::ApproveEach, Verdict::Allow, 1)],
            &["repo.write".to_string()],
        );
        assert_eq!(r.allowed, 1);
        assert_eq!(r.paused, 0);
    }

    /// Samples are drawn only from decisions the policy would CHANGE — that is
    /// what an operator needs to inspect — while the counts cover everything.
    #[test]
    fn samples_cover_changes_while_counts_cover_everything() {
        let decisions = vec![
            rec("repo.write", HicLevel::Budgeted, Verdict::Allow, 1),
            rec("mail.send", HicLevel::Budgeted, Verdict::Allow, 2),
        ];
        let r = replay(&policy_with_threshold("2"), &decisions, &["repo.write".into()]);
        assert_eq!(r.samples.len(), 1);
        assert_eq!(r.samples[0].action_class, "repo.write");
        assert_eq!(r.total, 2);
    }

    /// **The limit, pinned.** No replayable template can produce a block today,
    /// so `blocked` is always 0 — and that is a property of what this replay can
    /// exercise, not a finding about the policy. If a future rule can block,
    /// this test should fail and be updated deliberately.
    #[test]
    fn no_replayable_clause_can_block_today_and_the_reason_is_reported() {
        let mut p = policy_with_threshold("2");
        p.mapped.push(Mapped {
            clause: "2".into(),
            template_id: "0x02".into(),
            template_name: "ClassificationGate".into(),
            params: BTreeMap::new(),
        });
        let decisions = vec![
            rec("repo.write", HicLevel::Budgeted, Verdict::Allow, 1),
            rec("repo.write", HicLevel::ApproveEach, Verdict::Allow, 2),
        ];
        let r = replay(&p, &decisions, &["repo.write".to_string()]);
        assert_eq!(r.blocked, 0, "nothing replayable can block");
        assert!(
            r.not_simulated.iter().any(|n| n.why.contains("classification")),
            "the clause that WOULD block must be named as unreplayable: {:?}",
            r.not_simulated
        );
        assert!(!r.is_complete());
    }

    #[test]
    fn the_dto_surfaces_incompleteness_and_the_corpus_note() {
        let d = to_dto(&replay(&policy_with_threshold("2"), &[], &["repo.write".into()]));
        assert!(!d.complete);
        assert!(d.corpus_note.is_some());
        assert_eq!(d.range, "no decisions in range");
        assert_eq!(d.total, 0);
    }
}
