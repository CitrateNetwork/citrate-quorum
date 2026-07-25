//! citrate-quorum — the meetings domain (WP-S5.1).
//!
//! A meeting here is a **governed object**, not a calendar entry. The normative
//! lifecycle is the planset's `02_ARCHITECTURE.md` §3.1:
//!
//! ```text
//! schedule → agenda drafted → agendaHash frozen at open
//!   → attendance attested → segments → minutes composed
//!   → HIC reviews + signs (ceremony) → hash on chain
//! ```
//!
//! This crate owns the part of that which is decidable locally and can be
//! tested: the state machine, the agenda freeze, the classification rule, and
//! quorum. It deliberately holds **no** I/O — the store, the `SignatureCeremony`
//! and the chain reader live outside and call in.
//!
//! ## The four rules this crate exists to enforce
//!
//! 1. **The agenda freezes at open.** A meeting cannot leave `Scheduled`
//!    without an agenda hash, and once frozen the agenda is immutable. A
//!    meeting whose agenda can change after the fact cannot be evidence of
//!    anything — the hash in the minutes would describe a document that no
//!    longer exists.
//! 2. **MR-4 classification monotonicity** (planset Q13): a meeting's
//!    classification may not exceed the lowest clearance among its *attested*
//!    attendees. Stated the other way round, admitting someone lowers the
//!    ceiling — which is why the check runs on attest, not only on create.
//! 3. **Quorum is a precondition of ratification**, not a label. An inquorate
//!    meeting cannot be ratified at all.
//! 4. **A ratified meeting is immutable.** Ratification is a human signature
//!    over a specific content hash; anything that could change afterwards was
//!    not what was signed.

use quorum_tenancy::Classification;

// ---- identifiers ----------------------------------------------------

/// A meeting's stable id. Opaque to this crate — the caller mints it.
pub type MeetingId = String;

// ---- the lifecycle --------------------------------------------------

/// Where a meeting sits in §3.1. The frontend contract (frozen in S2D) uses the
/// same five names, so the mapping is one-to-one and needs no translation
/// table that could drift.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum MeetingState {
    /// Created, agenda still mutable, not yet opened.
    Scheduled,
    /// Opened. The agenda is frozen and `agenda_hash` is set.
    InProgress,
    /// Closed with minutes, waiting on a human signature.
    AwaitingRatification,
    /// Signed. Immutable from here.
    Ratified,
    /// Closed without quorum. A terminal state that can never be ratified.
    Inquorate,
}

impl MeetingState {
    /// The wire name. Matches `MeetingState` in `src/bridge/types.ts`.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Scheduled => "scheduled",
            Self::InProgress => "in-progress",
            Self::AwaitingRatification => "awaiting",
            Self::Ratified => "ratified",
            Self::Inquorate => "inquorate",
        }
    }

    /// Parse the wire name back. Used only when reloading the store.
    /// Named `from_wire`, not `from_str`, so it cannot be confused with the
    /// `FromStr` trait method (clippy::should_implement_trait).
    pub fn from_wire(s: &str) -> Option<Self> {
        Some(match s {
            "scheduled" => Self::Scheduled,
            "in-progress" => Self::InProgress,
            "awaiting" => Self::AwaitingRatification,
            "ratified" => Self::Ratified,
            "inquorate" => Self::Inquorate,
            _ => return None,
        })
    }

    /// Terminal states accept no further transitions.
    pub fn is_terminal(self) -> bool {
        matches!(self, Self::Ratified | Self::Inquorate)
    }
}

// ---- the agenda -----------------------------------------------------

/// One numbered agenda line, carrying the artifact it was derived from.
///
/// `src` is not decoration: Rule 11 requires every rendered value to be able to
/// name its origin, and an agenda item whose source cannot be stated is exactly
/// the kind of plausible-looking invention this product cannot ship.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct AgendaItem {
    pub n: u32,
    pub text: String,
    pub src: String,
}

/// The agenda, plus whether it has been frozen.
///
/// The hash covers the items only — not the freeze flag — so that recomputing
/// it from the rendered agenda in the minutes reproduces the same value. That
/// is what makes "verify this" possible offline.
#[derive(Clone, Default, Debug)]
pub struct Agenda {
    items: Vec<AgendaItem>,
    /// How many candidate lines the generator saw but could not parse. Carried
    /// so a silently-thin agenda is visible rather than looking complete
    /// (sprint risk R-C).
    pub skipped: usize,
}

impl Agenda {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn items(&self) -> &[AgendaItem] {
        &self.items
    }

    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    pub fn len(&self) -> usize {
        self.items.len()
    }

    /// Append an item, renumbering from 1 so `n` is always positional.
    pub fn push(&mut self, text: impl Into<String>, src: impl Into<String>) {
        let n = self.items.len() as u32 + 1;
        self.items.push(AgendaItem {
            n,
            text: text.into(),
            src: src.into(),
        });
    }

    /// BLAKE3 over a canonical encoding of the items.
    ///
    /// Length-prefixed rather than delimiter-joined: with a plain separator, an
    /// item containing the separator could be re-split into two different
    /// agendas that hash identically. Length prefixes make the encoding
    /// unambiguous, so the hash commits to exactly one reading.
    pub fn hash(&self) -> [u8; 32] {
        let mut h = blake3::Hasher::new();
        h.update(b"quorum.agenda.v1");
        h.update(&(self.items.len() as u64).to_le_bytes());
        for it in &self.items {
            h.update(&it.n.to_le_bytes());
            for field in [it.text.as_bytes(), it.src.as_bytes()] {
                h.update(&(field.len() as u64).to_le_bytes());
                h.update(field);
            }
        }
        *h.finalize().as_bytes()
    }
}

// ---- attendance -----------------------------------------------------

/// Someone present. `attested` distinguishes "we recorded them as present" from
/// "they proved it" — the planset's attendance-attested requirement.
///
/// `clearance` is what the attendee is cleared to, and it is what MR-4 reads.
/// It is `None` when unknown, which the classification rule treats as the
/// fail-closed `Public` rather than as "no constraint".
#[derive(Clone, Debug)]
pub struct Attendee {
    pub name: String,
    pub attested: bool,
    /// `Some(vendor)` for an agent, `None` for a human.
    pub agent: Option<String>,
    pub note: Option<String>,
    pub clearance: Option<Classification>,
}

impl Attendee {
    pub fn human(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            attested: false,
            agent: None,
            note: None,
            clearance: None,
        }
    }

    pub fn agent(name: impl Into<String>, vendor: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            attested: false,
            agent: Some(vendor.into()),
            note: None,
            clearance: None,
        }
    }

    pub fn attested(mut self) -> Self {
        self.attested = true;
        self
    }

    pub fn cleared_to(mut self, c: Classification) -> Self {
        self.clearance = Some(c);
        self
    }

    pub fn is_human(&self) -> bool {
        self.agent.is_none()
    }

    /// The clearance MR-4 should use. Unknown collapses to `Public` — the same
    /// fail-closed default `EffectiveGrant::fail_closed()` uses, so an
    /// unreadable clearance narrows the room rather than widening it.
    pub fn effective_clearance(&self) -> Classification {
        self.clearance.unwrap_or(Classification::Public)
    }
}

// ---- minutes --------------------------------------------------------

/// A decision referenced by the minutes. `link` records whether it resolves to
/// a record in this tenant's evidence chain — an unresolvable decision id is
/// rendered as such rather than quietly presented as a live link.
#[derive(Clone, Debug)]
pub struct MinuteDecision {
    pub id: String,
    pub text: String,
    pub link: bool,
}

/// Recorded disagreement. First-class because a governance record that drops
/// dissent is a record of consensus that did not happen.
#[derive(Clone, Debug)]
pub struct Dissent {
    pub who: String,
    pub text: String,
}

// ---- the meeting ----------------------------------------------------

/// The template a meeting was scheduled from. Carries the quorum rule, which is
/// the only part this crate needs to decide anything.
#[derive(Clone, Debug)]
pub struct Template {
    pub name: String,
    /// Attested humans required for the meeting to be quorate.
    pub min_humans: usize,
}

impl Template {
    pub fn new(name: impl Into<String>, min_humans: usize) -> Self {
        Self {
            name: name.into(),
            min_humans,
        }
    }
}

#[derive(Clone, Debug)]
pub struct Meeting {
    pub id: MeetingId,
    pub name: String,
    /// Scheduled start, RFC3339. Kept as a string: this crate never does date
    /// arithmetic, and parsing it would invite a timezone bug for no gain.
    pub when: String,
    pub tenant: String,
    pub template: Template,
    pub classification: Classification,
    state: MeetingState,
    pub agenda: Agenda,
    agenda_hash: Option<[u8; 32]>,
    pub attendance: Vec<Attendee>,
    pub minutes: Vec<String>,
    pub decisions: Vec<MinuteDecision>,
    pub dissent: Vec<Dissent>,
    pub ratified_by: Option<String>,
    pub ratified_at: Option<u64>,
}

impl Meeting {
    pub fn schedule(
        id: impl Into<MeetingId>,
        name: impl Into<String>,
        when: impl Into<String>,
        tenant: impl Into<String>,
        template: Template,
        classification: Classification,
    ) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            when: when.into(),
            tenant: tenant.into(),
            template,
            classification,
            state: MeetingState::Scheduled,
            agenda: Agenda::new(),
            agenda_hash: None,
            attendance: Vec::new(),
            minutes: Vec::new(),
            decisions: Vec::new(),
            dissent: Vec::new(),
            ratified_by: None,
            ratified_at: None,
        }
    }

    pub fn state(&self) -> MeetingState {
        self.state
    }

    /// The frozen agenda hash. `None` until the meeting opens — a scheduled
    /// meeting has no hash because its agenda can still change.
    pub fn agenda_hash(&self) -> Option<[u8; 32]> {
        self.agenda_hash
    }

    /// Attested humans present.
    pub fn attested_humans(&self) -> usize {
        self.attendance
            .iter()
            .filter(|a| a.is_human() && a.attested)
            .count()
    }

    /// Is the quorum rule satisfied right now?
    pub fn is_quorate(&self) -> bool {
        self.attested_humans() >= self.template.min_humans
    }

    /// MR-4: the highest classification this room may carry, given who has
    /// attested. An empty room is `Public` — you cannot hold a CUI meeting by
    /// yourself with nobody attested.
    pub fn classification_ceiling(&self) -> Classification {
        self.attendance
            .iter()
            .filter(|a| a.attested)
            .map(|a| a.effective_clearance())
            .min()
            .unwrap_or(Classification::Public)
    }

    /// Add an attendee.
    ///
    /// Refuses if the meeting is past `InProgress` (you cannot join a meeting
    /// that has closed) and — the MR-4 half — if admitting this person would
    /// drop the ceiling below the meeting's classification. The alternative,
    /// silently downgrading the meeting, would reclassify material that has
    /// already been discussed.
    pub fn admit(&mut self, who: Attendee) -> Result<(), MeetingError> {
        if self.state.is_terminal() || self.state == MeetingState::AwaitingRatification {
            return Err(MeetingError::Closed(self.state));
        }
        let cleared_to = who.effective_clearance();
        if who.attested && cleared_to < self.classification {
            return Err(MeetingError::ClearanceBelowClassification {
                who: who.name,
                cleared_to,
                meeting: self.classification,
            });
        }
        self.attendance.push(who);
        Ok(())
    }

    /// Open the meeting: freeze the agenda and record its hash.
    ///
    /// An empty agenda is allowed — a meeting can legitimately open with
    /// nothing scheduled — but it still freezes, so "the agenda was empty" is
    /// itself a committed fact rather than an absence someone can fill in later.
    pub fn open(&mut self) -> Result<[u8; 32], MeetingError> {
        if self.state != MeetingState::Scheduled {
            return Err(MeetingError::BadTransition {
                from: self.state,
                to: MeetingState::InProgress,
            });
        }
        let h = self.agenda.hash();
        self.agenda_hash = Some(h);
        self.state = MeetingState::InProgress;
        Ok(h)
    }

    /// Add an agenda item. Refused once the agenda is frozen — this is rule 1,
    /// and it is the whole reason `agenda` cannot simply be a public `Vec`.
    pub fn add_agenda_item(
        &mut self,
        text: impl Into<String>,
        src: impl Into<String>,
    ) -> Result<(), MeetingError> {
        if self.agenda_hash.is_some() {
            return Err(MeetingError::AgendaFrozen);
        }
        self.agenda.push(text, src);
        Ok(())
    }

    /// Close the meeting with its minutes.
    ///
    /// Quorum is evaluated **here**, at close, against who actually attested —
    /// not at schedule time against who was invited. A meeting that nobody
    /// turned up to goes to `Inquorate` and can never be ratified.
    pub fn close(&mut self, minutes: Vec<String>) -> Result<MeetingState, MeetingError> {
        if self.state != MeetingState::InProgress {
            return Err(MeetingError::BadTransition {
                from: self.state,
                to: MeetingState::AwaitingRatification,
            });
        }
        self.minutes = minutes;
        self.state = if self.is_quorate() {
            MeetingState::AwaitingRatification
        } else {
            MeetingState::Inquorate
        };
        Ok(self.state)
    }

    /// The content hash a human actually signs.
    ///
    /// Covers the agenda hash, the minutes, the decisions, the dissent and the
    /// attested attendance — everything the minutes assert. It deliberately
    /// excludes `ratified_by`/`ratified_at`, which are produced *by* the
    /// signature and so cannot be inside what is signed.
    pub fn content_hash(&self) -> [u8; 32] {
        let mut h = blake3::Hasher::new();
        h.update(b"quorum.minutes.v1");
        for field in [
            self.id.as_bytes(),
            self.name.as_bytes(),
            self.when.as_bytes(),
        ] {
            h.update(&(field.len() as u64).to_le_bytes());
            h.update(field);
        }
        h.update(&[self.classification as u8]);
        h.update(self.agenda_hash.as_ref().unwrap_or(&[0u8; 32]));

        h.update(&(self.minutes.len() as u64).to_le_bytes());
        for m in &self.minutes {
            h.update(&(m.len() as u64).to_le_bytes());
            h.update(m.as_bytes());
        }
        h.update(&(self.decisions.len() as u64).to_le_bytes());
        for d in &self.decisions {
            for field in [d.id.as_bytes(), d.text.as_bytes()] {
                h.update(&(field.len() as u64).to_le_bytes());
                h.update(field);
            }
        }
        h.update(&(self.dissent.len() as u64).to_le_bytes());
        for d in &self.dissent {
            for field in [d.who.as_bytes(), d.text.as_bytes()] {
                h.update(&(field.len() as u64).to_le_bytes());
                h.update(field);
            }
        }
        let attested: Vec<&Attendee> = self.attendance.iter().filter(|a| a.attested).collect();
        h.update(&(attested.len() as u64).to_le_bytes());
        for a in attested {
            h.update(&(a.name.len() as u64).to_le_bytes());
            h.update(a.name.as_bytes());
        }
        *h.finalize().as_bytes()
    }

    /// Ratify. The caller must already have obtained a human signature through
    /// the `SignatureCeremony` over `content_hash()` — this crate cannot sign
    /// and must never be able to (Rule 3).
    ///
    /// `expect_hash` is the hash the human was actually shown. If the meeting
    /// changed between the ceremony opening and this call, the hashes differ
    /// and ratification is refused rather than recording a signature over
    /// something nobody saw.
    pub fn ratify(
        &mut self,
        by: impl Into<String>,
        at: u64,
        expect_hash: [u8; 32],
    ) -> Result<(), MeetingError> {
        if self.state == MeetingState::Inquorate {
            return Err(MeetingError::Inquorate {
                attested: self.attested_humans(),
                required: self.template.min_humans,
            });
        }
        if self.state != MeetingState::AwaitingRatification {
            return Err(MeetingError::BadTransition {
                from: self.state,
                to: MeetingState::Ratified,
            });
        }
        let actual = self.content_hash();
        if actual != expect_hash {
            return Err(MeetingError::MinutesChanged);
        }
        let who = by.into();
        if who.trim().is_empty() {
            return Err(MeetingError::AnonymousRatifier);
        }
        self.ratified_by = Some(who);
        self.ratified_at = Some(at);
        self.state = MeetingState::Ratified;
        Ok(())
    }

    /// Restore a meeting's decided fields when reloading from the store.
    /// Separate from the transition methods on purpose: reloading is not a
    /// transition, and routing it through `open`/`close` would recompute a
    /// hash rather than restore the one that was signed.
    pub fn restore(
        &mut self,
        state: MeetingState,
        agenda_hash: Option<[u8; 32]>,
        ratified_by: Option<String>,
        ratified_at: Option<u64>,
    ) {
        self.state = state;
        self.agenda_hash = agenda_hash;
        self.ratified_by = ratified_by;
        self.ratified_at = ratified_at;
    }
}

// ---- errors ---------------------------------------------------------

#[derive(Clone, PartialEq, Eq, Debug)]
pub enum MeetingError {
    BadTransition {
        from: MeetingState,
        to: MeetingState,
    },
    AgendaFrozen,
    Closed(MeetingState),
    ClearanceBelowClassification {
        who: String,
        cleared_to: Classification,
        meeting: Classification,
    },
    Inquorate {
        attested: usize,
        required: usize,
    },
    MinutesChanged,
    AnonymousRatifier,
}

impl std::fmt::Display for MeetingError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::BadTransition { from, to } => write!(
                f,
                "a meeting cannot go from {} to {}",
                from.as_str(),
                to.as_str()
            ),
            Self::AgendaFrozen => write!(
                f,
                "the agenda was frozen when the meeting opened and cannot change"
            ),
            Self::Closed(s) => write!(f, "the meeting is {} — nobody can be admitted", s.as_str()),
            Self::ClearanceBelowClassification {
                who,
                cleared_to,
                meeting,
            } => write!(
                f,
                "{who} is cleared to {cleared_to:?} but the meeting is {meeting:?} \
                 — admitting them would reclassify material already discussed (MR-4)"
            ),
            Self::Inquorate { attested, required } => write!(
                f,
                "inquorate: {attested} attested of {required} required — \
                 an inquorate meeting can never be ratified"
            ),
            Self::MinutesChanged => write!(
                f,
                "the minutes changed after the ceremony opened — refusing to record \
                 a signature over something the signer did not see"
            ),
            Self::AnonymousRatifier => {
                write!(f, "ratification must name the human who signed it")
            }
        }
    }
}

impl std::error::Error for MeetingError {}

pub mod agenda_source;

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    fn std_template() -> Template {
        Template::new("Standup", 2)
    }

    fn scheduled() -> Meeting {
        Meeting::schedule(
            "m-1",
            "Weekly Standup",
            "2026-07-23T09:00:00Z",
            "citrate",
            std_template(),
            Classification::Proprietary,
        )
    }

    // ---- rule 1: the agenda freezes at open -------------------------

    #[test]
    fn agenda_is_mutable_before_open_and_frozen_after() {
        let mut m = scheduled();
        m.add_agenda_item("Agent reports", "sprint-qrm-s5/SCOPE.md")
            .unwrap();
        assert!(m.agenda_hash().is_none(), "no hash until it opens");

        let h = m.open().unwrap();
        assert_eq!(m.agenda_hash(), Some(h));
        assert_eq!(
            m.add_agenda_item("slipped in later", "nowhere"),
            Err(MeetingError::AgendaFrozen),
        );
    }

    #[test]
    fn the_frozen_hash_matches_a_recomputation_from_the_items() {
        let mut m = scheduled();
        m.add_agenda_item("one", "a").unwrap();
        m.add_agenda_item("two", "b").unwrap();
        let frozen = m.open().unwrap();
        // This is the "verify this" property: anyone holding the rendered
        // agenda can recompute the hash without the app.
        let mut replica = Agenda::new();
        replica.push("one", "a");
        replica.push("two", "b");
        assert_eq!(replica.hash(), frozen);
    }

    #[test]
    fn agenda_hash_is_unambiguous_across_field_boundaries() {
        // Without length prefixes these two agendas would hash identically:
        // "ab" + "c" and "a" + "bc" concatenate to the same bytes.
        let mut x = Agenda::new();
        x.push("ab", "c");
        let mut y = Agenda::new();
        y.push("a", "bc");
        assert_ne!(x.hash(), y.hash());
    }

    #[test]
    fn a_meeting_cannot_open_twice() {
        let mut m = scheduled();
        m.open().unwrap();
        assert_eq!(
            m.open(),
            Err(MeetingError::BadTransition {
                from: MeetingState::InProgress,
                to: MeetingState::InProgress,
            })
        );
    }

    // ---- rule 2: MR-4 classification monotonicity -------------------

    #[test]
    fn admitting_someone_under_cleared_is_refused() {
        let mut m = Meeting::schedule(
            "m-cui",
            "CCB",
            "2026-07-23T14:00:00Z",
            "citrate",
            std_template(),
            Classification::Cui,
        );
        let err = m
            .admit(
                Attendee::human("J. Whitfield")
                    .attested()
                    .cleared_to(Classification::Proprietary),
            )
            .unwrap_err();
        assert!(matches!(
            err,
            MeetingError::ClearanceBelowClassification { .. }
        ));
    }

    #[test]
    fn an_unattested_observer_does_not_lower_the_ceiling() {
        // Present but not attested: recorded, and not counted by MR-4. This is
        // the observer seat.
        let mut m = Meeting::schedule(
            "m-cui",
            "CCB",
            "2026-07-23T14:00:00Z",
            "citrate",
            std_template(),
            Classification::Cui,
        );
        m.admit(Attendee::human("Observer")).unwrap();
        m.admit(
            Attendee::human("R. Ortiz")
                .attested()
                .cleared_to(Classification::Cui),
        )
        .unwrap();
        assert_eq!(m.classification_ceiling(), Classification::Cui);
    }

    #[test]
    fn unknown_clearance_fails_closed_to_public() {
        let a = Attendee::human("unknown");
        assert_eq!(a.effective_clearance(), Classification::Public);
    }

    #[test]
    fn an_empty_room_has_a_public_ceiling() {
        let m = scheduled();
        assert_eq!(m.classification_ceiling(), Classification::Public);
    }

    // ---- rule 3: quorum gates ratification --------------------------

    #[test]
    fn closing_without_quorum_is_inquorate_and_unratifiable() {
        let mut m = scheduled();
        m.admit(
            Attendee::human("R. Ortiz")
                .attested()
                .cleared_to(Classification::Cui),
        )
        .unwrap();
        m.open().unwrap();
        assert_eq!(
            m.close(vec!["only one human".into()]).unwrap(),
            MeetingState::Inquorate
        );

        let h = m.content_hash();
        assert!(matches!(
            m.ratify("R. Ortiz", 1, h).unwrap_err(),
            MeetingError::Inquorate {
                attested: 1,
                required: 2
            }
        ));
    }

    #[test]
    fn agents_do_not_count_toward_a_human_quorum() {
        let mut m = scheduled();
        m.admit(
            Attendee::human("R. Ortiz")
                .attested()
                .cleared_to(Classification::Cui),
        )
        .unwrap();
        for a in ["claude-code", "codex", "devin"] {
            m.admit(
                Attendee::agent(a, "vendor")
                    .attested()
                    .cleared_to(Classification::Cui),
            )
            .unwrap();
        }
        assert_eq!(m.attested_humans(), 1);
        assert!(!m.is_quorate(), "three agents are not a second human");
    }

    // ---- rule 4: a ratified meeting is immutable --------------------

    fn quorate_and_closed() -> Meeting {
        let mut m = scheduled();
        for who in ["R. Ortiz", "M. Okonkwo"] {
            m.admit(
                Attendee::human(who)
                    .attested()
                    .cleared_to(Classification::Cui),
            )
            .unwrap();
        }
        m.add_agenda_item("Agent reports", "sprint-qrm-s5/SCOPE.md")
            .unwrap();
        m.open().unwrap();
        m.close(vec!["Reports accepted.".into()]).unwrap();
        m
    }

    #[test]
    fn ratification_records_the_signer_and_locks_the_meeting() {
        let mut m = quorate_and_closed();
        let h = m.content_hash();
        m.ratify("R. Ortiz", 1_753_460_000, h).unwrap();

        assert_eq!(m.state(), MeetingState::Ratified);
        assert_eq!(m.ratified_by.as_deref(), Some("R. Ortiz"));
        assert!(m.state().is_terminal());
        // A second signature over the same minutes is refused.
        assert!(matches!(
            m.ratify("M. Okonkwo", 2, h).unwrap_err(),
            MeetingError::BadTransition { .. }
        ));
    }

    #[test]
    fn ratifying_minutes_that_changed_since_the_ceremony_is_refused() {
        let mut m = quorate_and_closed();
        let shown_to_the_human = m.content_hash();
        // Something edits the minutes after the ceremony opened.
        m.minutes.push("...and one more thing".into());
        assert_eq!(
            m.ratify("R. Ortiz", 1, shown_to_the_human),
            Err(MeetingError::MinutesChanged)
        );
        assert_eq!(m.state(), MeetingState::AwaitingRatification);
    }

    #[test]
    fn ratification_must_name_a_human() {
        let mut m = quorate_and_closed();
        let h = m.content_hash();
        assert_eq!(m.ratify("   ", 1, h), Err(MeetingError::AnonymousRatifier));
    }

    #[test]
    fn nobody_can_be_admitted_after_the_meeting_closes() {
        let mut m = quorate_and_closed();
        assert!(matches!(
            m.admit(Attendee::human("latecomer")).unwrap_err(),
            MeetingError::Closed(MeetingState::AwaitingRatification)
        ));
    }

    // ---- content hash -----------------------------------------------

    #[test]
    fn the_content_hash_covers_dissent() {
        // Dropping dissent from the signed content would let a record of
        // consensus be signed for a meeting that had none.
        let mut a = quorate_and_closed();
        let before = a.content_hash();
        a.dissent.push(Dissent {
            who: "hermes".into(),
            text: "objects to the threshold basis".into(),
        });
        assert_ne!(before, a.content_hash());
    }

    #[test]
    fn the_content_hash_covers_the_agenda() {
        let mut a = quorate_and_closed();
        let with_agenda = a.content_hash();
        a.agenda_hash = Some([0u8; 32]);
        assert_ne!(with_agenda, a.content_hash());
    }

    #[test]
    fn state_names_round_trip() {
        for s in [
            MeetingState::Scheduled,
            MeetingState::InProgress,
            MeetingState::AwaitingRatification,
            MeetingState::Ratified,
            MeetingState::Inquorate,
        ] {
            assert_eq!(MeetingState::from_wire(s.as_str()), Some(s));
        }
        assert_eq!(MeetingState::from_wire("nonsense"), None);
    }
}
