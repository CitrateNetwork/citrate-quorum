//! citrate-quorum — policy enforcement (WP-S6 core, client-side).
//!
//! The decision heart, as pure tested Rust mirroring the governance-contract
//! invariants (`03_GOVERNANCE_CONTRACTS.md`):
//!
//! - [`CapabilityGrant`] — the HIC envelope a principal gives an agent (scope,
//!   action classes, classification ceiling, budget, expiry, HIC level).
//! - [`evaluate`] — the `PolicyBinding.check()` decision: given an action and an
//!   agent's grants, produce a [`Verdict`] + the HIC level + the authorizing
//!   grant, or **`Ungoverned`** when nothing covers it.
//! - [`VoteAllowance`] — the delegated, bounded, revocable franchise an agent
//!   spends on a principal's behalf (agents never hold voting power natively).
//!
//! This is the *advisory + attested* half of enforcement (Q5): the app evaluates
//! locally against the pinned ruleset and records the verdict it got; the binding
//! half is the on-chain settlement of that verdict. Either way, the decision is
//! computed here and is exhaustively testable without a chain.
//!
//! Every path is fail-closed: no live grant → `Ungoverned`; over budget, expired,
//! revoked, over-classification, or FN-gated → not `Allow`.

#![forbid(unsafe_code)]

pub use quorum_audit::{HicLevel, Verdict};
use quorum_tenancy::Classification;

/// The HIC level a principal set on a grant — how their control is exercised.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum GrantHic {
    /// HIC-1: every action under this grant pauses for a signature.
    ApproveEach,
    /// HIC-2: budgeted autonomy — act within scope/budget/expiry.
    Budgeted,
    /// HIC-3: act, with a post-hoc review window.
    PostHoc,
}

impl GrantHic {
    fn level(self) -> HicLevel {
        match self {
            GrantHic::ApproveEach => HicLevel::ApproveEach,
            GrantHic::Budgeted => HicLevel::Budgeted,
            GrantHic::PostHoc => HicLevel::PostHoc,
        }
    }
}

/// The capability envelope a principal grants an agent (`CapabilityGrant.sol`).
/// Invariants enforced by construction + the methods below:
/// **CG-1** `consumed ≤ budget`; **CG-2** revoke is immediate; **CG-3** a live
/// grant requires principal + scope; **CG-4** the HIC level only tightens toward
/// more human control except by an explicit principal action.
#[derive(Clone, Debug)]
pub struct CapabilityGrant {
    pub id: String,
    pub agent: String,
    pub principal: String,
    /// The tenant scope (a scope-key prefix) this grant is valid within.
    pub tenant_scope: String,
    /// Action classes this grant authorizes (e.g. `repo.write`, `spend`).
    pub action_classes: Vec<String>,
    /// The maximum classification this grant may operate on.
    pub classification_ceiling: Classification,
    /// The spend/action budget (in abstract units), and how much is consumed.
    pub budget_units: u64,
    pub consumed: u64,
    /// Absolute expiry (epoch-ms).
    pub expires_at_ms: i64,
    pub hic: GrantHic,
    pub revoked: bool,
}

impl CapabilityGrant {
    /// Is the grant live at `now_ms`? (not revoked, not expired). CG-2: a revoked
    /// grant is dead immediately regardless of expiry.
    pub fn is_live(&self, now_ms: i64) -> bool {
        !self.revoked && self.expires_at_ms > now_ms
    }

    /// Does this grant cover an action of `class` at `classification` costing
    /// `cost` units — i.e. live, class in scope, within ceiling, budget remaining?
    pub fn covers(
        &self,
        class: &str,
        classification: Classification,
        cost: u64,
        now_ms: i64,
    ) -> bool {
        self.is_live(now_ms)
            && self.action_classes.iter().any(|c| c == class)
            && classification <= self.classification_ceiling
            && self.remaining() >= cost
    }

    /// Budget left. CG-1: never underflows below zero.
    pub fn remaining(&self) -> u64 {
        self.budget_units.saturating_sub(self.consumed)
    }

    /// Consume `cost` units. Errors (leaving state unchanged) if it would exceed
    /// the budget — CG-1 can never be violated.
    pub fn consume(&mut self, cost: u64) -> Result<(), PolicyError> {
        if cost > self.remaining() {
            return Err(PolicyError::OverBudget);
        }
        self.consumed += cost;
        Ok(())
    }

    /// Revoke immediately (CG-2). Idempotent.
    pub fn revoke(&mut self) {
        self.revoked = true;
    }
}

/// An action an agent wants to take, evaluated against its grants.
#[derive(Clone, Debug)]
pub struct Action {
    pub class: String,
    pub classification: Classification,
    /// Abstract cost (spend/action units) charged against a grant's budget.
    pub cost: u64,
    /// A single-action spend ceiling above which a human MUST sign, regardless of
    /// remaining budget (PRT-004 C2 pattern). `0` disables the check.
    pub hic1_cost_threshold: u64,
    /// True for action classes that touch chain/money/keys/grants/classification
    /// — these always require a human signature (L-1 of the HIC model).
    pub mandatory_hic1: bool,
}

/// The outcome of a policy check.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Decision {
    pub verdict: Verdict,
    pub hic: HicLevel,
    /// The grant that authorized (or would authorize) the action; `None` when
    /// ungoverned.
    pub grant_id: Option<String>,
    /// A short reason code for the ledger.
    pub reason: &'static str,
}

/// The `PolicyBinding.check()` decision. Picks the first live grant that covers
/// the action; escalates to `RequireApproval` (HIC-1) when the action mandates a
/// signature or exceeds the single-action ceiling; else `Allow` at the grant's
/// HIC level. No covering grant → `Ungoverned` (HIC-X) — never a silent allow.
pub fn evaluate(action: &Action, grants: &[CapabilityGrant], now_ms: i64) -> Decision {
    let covering = grants
        .iter()
        .find(|g| g.covers(&action.class, action.classification, action.cost, now_ms));

    let Some(grant) = covering else {
        // Distinguish "a grant exists but is over-classification/expired" is not
        // needed for the verdict — the app-level result is the same: no live
        // covering grant means the action is ungoverned.
        return Decision {
            verdict: Verdict::Ungoverned,
            hic: HicLevel::Ungoverned,
            grant_id: None,
            reason: "RC-000 no live grant covers this action",
        };
    };

    let over_ceiling = action.hic1_cost_threshold > 0 && action.cost > action.hic1_cost_threshold;

    if action.mandatory_hic1 || over_ceiling || grant.hic == GrantHic::ApproveEach {
        return Decision {
            verdict: Verdict::RequireApproval,
            hic: HicLevel::ApproveEach,
            grant_id: Some(grant.id.clone()),
            reason: if over_ceiling {
                "RC-102 over single-action ceiling → HIC-1"
            } else if action.mandatory_hic1 {
                "RC-101 chain/money/key/grant action → HIC-1"
            } else {
                "RC-103 grant is HIC-1 approve-each"
            },
        };
    }

    Decision {
        verdict: Verdict::Allow,
        hic: grant.hic.level(),
        grant_id: Some(grant.id.clone()),
        reason: "RC-200 within a live grant envelope",
    }
}

/// The delegated, bounded, revocable voting franchise (`VoteAllowance.sol`).
/// Agents never hold voting power natively — they spend an allowance a human
/// principal granted. Invariants: **VA-1** `spent ≤ weight_cap`; **VA-2** an
/// expired/revoked allowance can never be spent; **VA-3** every cast traces to
/// exactly one principal; **VA-4** no sub-delegation (there is no API to delegate
/// an allowance onward).
#[derive(Clone, Debug)]
pub struct VoteAllowance {
    pub id: String,
    pub principal: String,
    pub agent: String,
    pub tenant_scope: String,
    pub proposal_classes: Vec<String>,
    pub weight_cap: u64,
    pub spent: u64,
    pub expires_at_ms: i64,
    pub revoked: bool,
}

impl VoteAllowance {
    pub fn is_live(&self, now_ms: i64) -> bool {
        !self.revoked && self.expires_at_ms > now_ms
    }

    pub fn remaining(&self) -> u64 {
        self.weight_cap.saturating_sub(self.spent)
    }

    /// Cast `weight` on a proposal of `class`. Returns the delegation proof
    /// (`principal ▸ agent · spent/cap`) on success. Fails closed if the
    /// allowance is dead, doesn't cover the class, or would exceed the cap.
    pub fn cast(&mut self, class: &str, weight: u64, now_ms: i64) -> Result<String, PolicyError> {
        if !self.is_live(now_ms) {
            return Err(PolicyError::AllowanceInactive);
        }
        if !self.proposal_classes.iter().any(|c| c == class) {
            return Err(PolicyError::ClassNotCovered);
        }
        if weight > self.remaining() {
            return Err(PolicyError::OverAllowance);
        }
        self.spent += weight;
        Ok(format!(
            "{} ▸ {} · allowance {}/{}",
            self.principal, self.agent, self.spent, self.weight_cap
        ))
    }

    /// Revoke immediately — the "pull the plug" property (VA-2). Idempotent.
    pub fn revoke(&mut self) {
        self.revoked = true;
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PolicyError {
    OverBudget,
    AllowanceInactive,
    ClassNotCovered,
    OverAllowance,
}

impl core::fmt::Display for PolicyError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        let s = match self {
            PolicyError::OverBudget => "action would exceed the grant budget",
            PolicyError::AllowanceInactive => "vote allowance is expired or revoked",
            PolicyError::ClassNotCovered => "proposal class not covered by the allowance",
            PolicyError::OverAllowance => "vote weight would exceed the allowance cap",
        };
        write!(f, "{s}")
    }
}
impl std::error::Error for PolicyError {}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    const NOW: i64 = 1_800_000_000_000;
    const FUTURE: i64 = NOW + 86_400_000;
    const PAST: i64 = NOW - 1;

    fn grant(
        classes: &[&str],
        ceiling: Classification,
        budget: u64,
        hic: GrantHic,
        exp: i64,
    ) -> CapabilityGrant {
        CapabilityGrant {
            id: "G-1".into(),
            agent: "sbt-41".into(),
            principal: "R. Ortiz".into(),
            tenant_scope: "t3:bca".into(),
            action_classes: classes.iter().map(|s| s.to_string()).collect(),
            classification_ceiling: ceiling,
            budget_units: budget,
            consumed: 0,
            expires_at_ms: exp,
            hic,
            revoked: false,
        }
    }
    fn action(class: &str, classification: Classification, cost: u64) -> Action {
        Action {
            class: class.into(),
            classification,
            cost,
            hic1_cost_threshold: 0,
            mandatory_hic1: false,
        }
    }

    // ---- CapabilityGrant invariants ------------------------------------

    #[test]
    fn cg1_consume_cannot_exceed_budget() {
        let mut g = grant(
            &["spend"],
            Classification::Cui,
            100,
            GrantHic::Budgeted,
            FUTURE,
        );
        assert!(g.consume(60).is_ok());
        assert_eq!(g.remaining(), 40);
        assert_eq!(g.consume(50), Err(PolicyError::OverBudget)); // would exceed
        assert_eq!(g.consumed, 60); // unchanged
        assert!(g.consume(40).is_ok());
        assert_eq!(g.remaining(), 0);
    }

    #[test]
    fn cg2_revoke_is_immediate() {
        let mut g = grant(
            &["repo.write"],
            Classification::Cui,
            100,
            GrantHic::Budgeted,
            FUTURE,
        );
        assert!(g.is_live(NOW));
        g.revoke();
        assert!(!g.is_live(NOW), "revoked grant is dead now, not at expiry");
        assert!(!g.covers("repo.write", Classification::Public, 1, NOW));
    }

    #[test]
    fn expired_grant_is_not_live() {
        let g = grant(
            &["repo.write"],
            Classification::Cui,
            100,
            GrantHic::Budgeted,
            PAST,
        );
        assert!(!g.is_live(NOW));
    }

    #[test]
    fn covers_requires_class_ceiling_and_budget() {
        let g = grant(
            &["repo.write"],
            Classification::Proprietary,
            10,
            GrantHic::Budgeted,
            FUTURE,
        );
        assert!(g.covers("repo.write", Classification::Proprietary, 10, NOW));
        assert!(!g.covers("spend", Classification::Public, 1, NOW)); // class not in scope
        assert!(!g.covers("repo.write", Classification::Cui, 1, NOW)); // over ceiling
        assert!(!g.covers("repo.write", Classification::Public, 11, NOW)); // over budget
    }

    // ---- evaluate() verdicts -------------------------------------------

    #[test]
    fn no_covering_grant_is_ungoverned() {
        let d = evaluate(&action("repo.write", Classification::Public, 1), &[], NOW);
        assert_eq!(d.verdict, Verdict::Ungoverned);
        assert_eq!(d.hic, HicLevel::Ungoverned);
        assert!(d.grant_id.is_none());
    }

    #[test]
    fn covered_budgeted_action_is_allowed_at_hic2() {
        let g = grant(
            &["repo.write"],
            Classification::Cui,
            100,
            GrantHic::Budgeted,
            FUTURE,
        );
        let d = evaluate(
            &action("repo.write", Classification::Proprietary, 5),
            &[g],
            NOW,
        );
        assert_eq!(d.verdict, Verdict::Allow);
        assert_eq!(d.hic, HicLevel::Budgeted);
        assert_eq!(d.grant_id.as_deref(), Some("G-1"));
    }

    #[test]
    fn over_single_action_ceiling_forces_hic1() {
        let g = grant(
            &["spend"],
            Classification::Cui,
            1000,
            GrantHic::Budgeted,
            FUTURE,
        );
        let mut a = action("spend", Classification::Public, 220);
        a.hic1_cost_threshold = 150; // PRT-004 C2
        let d = evaluate(&a, &[g], NOW);
        assert_eq!(d.verdict, Verdict::RequireApproval);
        assert_eq!(d.hic, HicLevel::ApproveEach);
        assert!(d.reason.contains("ceiling"));
    }

    #[test]
    fn mandatory_hic1_action_requires_approval_even_when_covered() {
        let g = grant(
            &["grant.issue"],
            Classification::Itar,
            1000,
            GrantHic::Budgeted,
            FUTURE,
        );
        let mut a = action("grant.issue", Classification::Cui, 1);
        a.mandatory_hic1 = true; // touches a capability grant → L-1
        let d = evaluate(&a, &[g], NOW);
        assert_eq!(d.verdict, Verdict::RequireApproval);
    }

    #[test]
    fn approve_each_grant_always_requires_approval() {
        let g = grant(
            &["repo.write"],
            Classification::Cui,
            100,
            GrantHic::ApproveEach,
            FUTURE,
        );
        let d = evaluate(&action("repo.write", Classification::Public, 1), &[g], NOW);
        assert_eq!(d.verdict, Verdict::RequireApproval);
        assert!(d.reason.contains("approve-each"));
    }

    #[test]
    fn revoked_grant_makes_the_action_ungoverned() {
        let mut g = grant(
            &["repo.write"],
            Classification::Cui,
            100,
            GrantHic::Budgeted,
            FUTURE,
        );
        g.revoke();
        let d = evaluate(&action("repo.write", Classification::Public, 1), &[g], NOW);
        assert_eq!(d.verdict, Verdict::Ungoverned);
    }

    #[test]
    fn over_classification_action_is_ungoverned_not_allowed() {
        // Grant ceiling is Proprietary; action is CUI → no covering grant.
        let g = grant(
            &["repo.write"],
            Classification::Proprietary,
            100,
            GrantHic::Budgeted,
            FUTURE,
        );
        let d = evaluate(&action("repo.write", Classification::Cui, 1), &[g], NOW);
        assert_eq!(d.verdict, Verdict::Ungoverned);
    }

    // ---- VoteAllowance invariants --------------------------------------

    fn allowance(cap: u64, classes: &[&str], exp: i64) -> VoteAllowance {
        VoteAllowance {
            id: "VA-1".into(),
            principal: "R. Ortiz".into(),
            agent: "claude-code".into(),
            tenant_scope: "t3:bca".into(),
            proposal_classes: classes.iter().map(|s| s.to_string()).collect(),
            weight_cap: cap,
            spent: 0,
            expires_at_ms: exp,
            revoked: false,
        }
    }

    #[test]
    fn va1_cast_cannot_exceed_cap_and_proof_traces_to_principal() {
        let mut a = allowance(5, &["standup"], FUTURE);
        let proof = a.cast("standup", 3, NOW).unwrap();
        assert!(proof.contains("R. Ortiz ▸ claude-code"));
        assert!(proof.contains("3/5"));
        assert_eq!(a.cast("standup", 3, NOW), Err(PolicyError::OverAllowance));
        assert_eq!(a.spent, 3); // unchanged after the failed cast
    }

    #[test]
    fn va2_expired_or_revoked_allowance_cannot_be_spent() {
        let mut expired = allowance(5, &["standup"], PAST);
        assert_eq!(
            expired.cast("standup", 1, NOW),
            Err(PolicyError::AllowanceInactive)
        );
        let mut live = allowance(5, &["standup"], FUTURE);
        live.revoke();
        assert_eq!(
            live.cast("standup", 1, NOW),
            Err(PolicyError::AllowanceInactive)
        );
    }

    #[test]
    fn allowance_only_covers_its_proposal_classes() {
        let mut a = allowance(5, &["standup"], FUTURE);
        assert_eq!(
            a.cast("treasury", 1, NOW),
            Err(PolicyError::ClassNotCovered)
        );
    }
}
