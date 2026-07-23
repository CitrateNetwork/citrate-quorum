//! citrate-quorum — the seat-metering seam (WP-S1.6).
//!
//! Quorum is a T1 product priced **per governed seat** (Q20). Real metering and
//! license enforcement are QRM-S9 work. This crate exists now, in S1, so the
//! seam is present from day one and S9 fills it in **without a refactor** — the
//! rest of the app depends on [`LicenseDomain`], never on a concrete impl.
//!
//! Rule 1 (no mocks) governs the placeholder: the ships-today implementation is
//! [`UnenforcedLicense`], and it is **honest about doing nothing**. It never
//! blocks a user, and [`LicenseStatus::enforced`] is `false` so the Settings
//! surface can state plainly "seat metering is not enforced in this build" rather
//! than imply a license check is happening. A no-op that pretended to enforce
//! would be exactly the kind of fake-live surface the rules forbid.

#![forbid(unsafe_code)]

use std::fmt;

/// A seat is one governed principal (a human or an agent) counted against the
/// deployment's license. The definition of "active" (e.g. acted within N days)
/// is an S9 policy decision; the seam only needs the counts to flow.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct SeatCount(pub u64);

/// A point-in-time view of license posture, surfaced read-only in Settings.
#[derive(Clone, Debug)]
pub struct LicenseStatus {
    /// Seats the deployment is licensed for, if a license is installed.
    pub licensed: Option<SeatCount>,
    /// Seats currently counted active.
    pub active: SeatCount,
    /// Whether metering is actually enforced. **`false` in every S1 build.** The
    /// UI must show this verbatim, not infer enforcement from the presence of
    /// counts.
    pub enforced: bool,
    /// A short, honest human-readable posture for the Settings card.
    pub note: &'static str,
}

/// The outcome of asking whether a new seat may be activated.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum SeatDecision {
    /// The seat may proceed.
    Allow,
    /// The seat is over the licensed count. **Never returned by the S1 no-op.**
    Deny,
}

/// The seam every seat-consuming path depends on. In S9 a real implementation
/// backs this with the installed license and a durable active-seat set; in S1
/// only [`UnenforcedLicense`] exists.
pub trait LicenseDomain {
    /// Current license posture for the Settings surface.
    fn status(&self) -> LicenseStatus;

    /// May `principal_scope` (a `(tenant, principal)` scope key from
    /// `quorum-tenancy`) become an active seat? The S1 no-op always allows.
    fn authorize_seat(&self, principal_scope: &str) -> SeatDecision;

    /// Record that a scope was active (for metering). The S1 no-op discards it.
    fn record_seat_activity(&self, principal_scope: &str);
}

/// The ships-today implementation: counts nothing, blocks nothing, and reports
/// `enforced = false`. Its entire job is to make the seam real while telling the
/// truth about the fact that no license is being enforced yet.
#[derive(Clone, Copy, Debug, Default)]
pub struct UnenforcedLicense;

impl LicenseDomain for UnenforcedLicense {
    fn status(&self) -> LicenseStatus {
        LicenseStatus {
            licensed: None,
            active: SeatCount(0),
            enforced: false,
            note: "Seat metering is not enforced in this build (lands in QRM-S9).",
        }
    }

    fn authorize_seat(&self, _principal_scope: &str) -> SeatDecision {
        // Fail-OPEN by design: an unenforced license must never be the thing that
        // stops a legitimate user. Enforcement, if any, arrives in S9.
        SeatDecision::Allow
    }

    fn record_seat_activity(&self, _principal_scope: &str) {
        // Intentionally discarded until S9 provides a durable meter.
    }
}

impl fmt::Display for LicenseStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.licensed {
            Some(SeatCount(n)) => write!(f, "{}/{} seats", self.active.0, n),
            None => write!(f, "{} seats active · unlicensed", self.active.0),
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn noop_never_blocks_a_seat() {
        let lic = UnenforcedLicense;
        // Whatever the scope, the unenforced license allows it. This is the
        // WP-S1.6 acceptance property: the placeholder never blocks a user.
        for scope in ["t3:bca/a5:sbt-1", "", "t2:x/h1:9"] {
            assert_eq!(lic.authorize_seat(scope), SeatDecision::Allow);
        }
    }

    #[test]
    fn noop_reports_itself_as_unenforced() {
        let s = UnenforcedLicense.status();
        assert!(!s.enforced, "S1 build must report metering as NOT enforced");
        assert!(s.licensed.is_none());
        assert_eq!(s.active, SeatCount(0));
        assert!(
            s.note.contains("not enforced"),
            "the Settings note must be honest about doing nothing"
        );
    }

    #[test]
    fn recording_activity_is_a_harmless_noop() {
        // Must not panic and must not change the reported posture.
        let lic = UnenforcedLicense;
        lic.record_seat_activity("t3:bca/a5:sbt-1");
        assert!(!lic.status().enforced);
    }

    #[test]
    fn status_display_is_honest_when_unlicensed() {
        assert_eq!(
            UnenforcedLicense.status().to_string(),
            "0 seats active · unlicensed"
        );
    }
}
