//! citrate-quorum — session resolution (WP-S2: identity → clearance).
//!
//! Login flows: OIDC id_token → entitlement claim → **join with on-chain
//! clearance and the tenant ceiling** → an [`EffectiveGrant`] the whole app
//! gates against (`02_ARCHITECTURE.md` §2). This crate is the pure, deterministic
//! **resolution algebra**; the OIDC exchange lives in `citrate-core-kit::oidc`
//! and the chain reads use the `quorum-rbac` selectors — this module takes their
//! *resolved values* and computes the grant. Keeping it pure makes the security
//! properties exhaustively testable (see the adversarial suite in `tests`).
//!
//! ## The two invariants (fail-closed, least-of-ceilings)
//!
//! 1. **Fail closed.** A missing entitlement, an expired claim, an unreadable
//!    chain, or an unknown tenant collapses to [`EffectiveGrant::fail_closed`]
//!    (Public, non-FN). Access is *computed then shown*; nothing is granted by
//!    default.
//! 2. **Least of all ceilings.** The classification ceiling is the **minimum** of
//!    three independent ceilings — the commercial-tier cap, the on-chain
//!    clearance, and the tenant's `classification_max`. A high value on one axis
//!    can never lift a low value on another.
//!
//! ## Why a commercial-tier *cap*, not a grant (the hardened decision)
//!
//! The entitlement `tier` proves a **paid, KYC'd commercial relationship**; it
//! does **not** itself grant clearance — clearance is an enterprise fact that
//! lives on-chain in `ClassificationRegistry`. So the tier only ever *caps*:
//!
//! - unauthenticated / free / public → capped at **Public** (an unpaid or
//!   lapsed principal can never exceed Public regardless of on-chain clearance);
//! - any active paid/KYC'd tier → **no commercial cap** (cap = ITAR), so the real
//!   gate becomes `min(on-chain clearance, tenant ceiling)`.
//!
//! This keeps the two axes honest: paying lifts the commercial gate; it is the
//! chain that says what you are cleared to see. (`09_DECISIONS_LOCKED.md` Q13.)

#![forbid(unsafe_code)]

pub use quorum_tenancy::{Classification, EffectiveGrant};

/// The entitlement claim minted by citrate-identity and carried in the OIDC
/// id_token under `https://citrate.ai/entitlement`. This is the subset the
/// session resolution needs; the kit's `oidc::EntitlementClaim` is the wire form.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Entitlement {
    /// The raw tier string, e.g. `commercial.kyc`, `academic`, `free`. Compared
    /// case-insensitively; unknown tiers fail closed (treated as unpaid).
    pub tier: Option<String>,
    /// Absolute expiry (epoch-ms). `None` means no expiry was asserted — which,
    /// for a paid entitlement, we treat as **expired/absent** and fail closed
    /// (an open-ended paid claim is not a shape citrate-identity mints).
    pub expires_at_ms: Option<i64>,
}

impl Entitlement {
    /// The fail-closed default: no tier, no expiry — resolves to Public.
    pub fn none() -> Self {
        Self {
            tier: None,
            expires_at_ms: None,
        }
    }

    /// Is this entitlement active at `now_ms`? An active entitlement needs a
    /// recognized paid tier AND an expiry strictly in the future. Absent tier,
    /// unknown tier, absent expiry, or a past expiry all read as inactive.
    pub fn is_active(&self, now_ms: i64) -> bool {
        if self.tier_cap() <= Classification::Public {
            // free / public / unknown / absent — not a paid relationship.
            return false;
        }
        match self.expires_at_ms {
            Some(exp) => exp > now_ms,
            None => false,
        }
    }

    /// The classification the commercial tier *caps* access to. Only ever caps —
    /// see the module docs. Recognizes both citrate-identity's raw vocabulary
    /// (`commercial`, `commercial.kyc`, `academic`, `confidential`) and the
    /// core-app-normalized forms (`pilot`, `enterprise`) that the kit may surface.
    pub fn tier_cap(&self) -> Classification {
        let raw = match &self.tier {
            Some(t) => t.trim().to_ascii_lowercase(),
            None => return Classification::Public,
        };
        match raw.as_str() {
            "" | "free" | "public" | "anonymous" => Classification::Public,
            // Any active paid/KYC'd tier lifts the commercial cap entirely; the
            // real gate is then the on-chain clearance ∩ tenant ceiling.
            "commercial" | "commercial.kyc" | "member" | "pilot" | "enterprise"
            | "enterprise.kyc" | "academic" | "confidential" => Classification::Itar,
            // Unknown tier → fail closed (Public). An unrecognized claim must not
            // widen access.
            _ => Classification::Public,
        }
    }
}

/// Everything the resolution needs about the on-chain and tenant side. The
/// caller fills this from `ClassificationRegistry.getClearance` (clearance +
/// foreign-national flag) and `TenantHierarchy.getNode` (`classification_max`).
/// **`None` on any field means "could not read" and forces fail-closed** — an
/// unreadable chain never grants access.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ClearanceInputs {
    /// The principal's on-chain clearance, or `None` if unread / no record.
    pub on_chain_clearance: Option<Classification>,
    /// The foreign-national flag from `ClassificationRegistry`. `None` (unread)
    /// is treated as FN=true (the conservative default: assume FN until proven
    /// otherwise, so an unread flag cannot open an ITAR gate).
    pub foreign_national: Option<bool>,
    /// The tenant's `classification_max`, or `None` if the tenant node is
    /// unknown / unread.
    pub tenant_ceiling: Option<Classification>,
}

impl ClearanceInputs {
    /// The fail-closed default: nothing read.
    pub fn unread() -> Self {
        Self {
            on_chain_clearance: None,
            foreign_national: None,
            tenant_ceiling: None,
        }
    }
}

/// Resolve the [`EffectiveGrant`] a principal carries into a tenant scope.
///
/// This is the **whole authorization decision** for classification, in one pure
/// function. It is the least of three ceilings, with a fail-closed default and an
/// ITAR-requires-non-FN rule folded into [`EffectiveGrant::permits`].
///
/// - `entitlement` — the OIDC entitlement claim (commercial axis).
/// - `clearance` — the on-chain + tenant inputs (enterprise axis).
/// - `now_ms` — the current time, injected (no ambient clock), so expiry is
///   deterministic and testable.
pub fn resolve_effective_grant(
    entitlement: &Entitlement,
    clearance: &ClearanceInputs,
    now_ms: i64,
) -> EffectiveGrant {
    // (1) An inactive/expired/absent entitlement caps at Public no matter what
    //     the chain says — an unpaid or lapsed principal is Public.
    let tier_cap = if entitlement.is_active(now_ms) {
        entitlement.tier_cap()
    } else {
        Classification::Public
    };

    // (2) Any unread chain input fails closed on that axis.
    let clearance_ceiling = clearance
        .on_chain_clearance
        .unwrap_or(Classification::Public);
    let tenant_ceiling = clearance.tenant_ceiling.unwrap_or(Classification::Public);

    // (3) The least of all ceilings.
    let ceiling = tier_cap.min(clearance_ceiling).min(tenant_ceiling);

    // (4) FN flag: unread → assume FN (conservative), so an unread flag cannot
    //     open the ITAR gate. Only a definite `Some(false)` clears FN.
    let foreign_national = clearance.foreign_national != Some(false);

    EffectiveGrant {
        classification_ceiling: ceiling,
        foreign_national,
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    const NOW: i64 = 1_800_000_000_000; // a fixed "now" for the suite
    const FUTURE: i64 = NOW + 86_400_000;
    const PAST: i64 = NOW - 86_400_000;

    fn paid(tier: &str, exp: i64) -> Entitlement {
        Entitlement {
            tier: Some(tier.into()),
            expires_at_ms: Some(exp),
        }
    }
    fn cleared(c: Classification, fn_flag: bool, ceil: Classification) -> ClearanceInputs {
        ClearanceInputs {
            on_chain_clearance: Some(c),
            foreign_national: Some(fn_flag),
            tenant_ceiling: Some(ceil),
        }
    }

    #[test]
    fn happy_path_cui_member_in_cui_tenant_gets_cui() {
        let g = resolve_effective_grant(
            &paid("commercial.kyc", FUTURE),
            &cleared(Classification::Cui, false, Classification::Cui),
            NOW,
        );
        assert_eq!(g.classification_ceiling, Classification::Cui);
        assert!(g.permits(Classification::Cui));
        assert!(!g.permits(Classification::Itar));
    }

    // ---- fail-closed: the four collapse-to-Public paths -----------------

    #[test]
    fn no_entitlement_fails_closed_to_public_even_with_itar_clearance() {
        // The adversary has ITAR on-chain clearance but no paid entitlement.
        let g = resolve_effective_grant(
            &Entitlement::none(),
            &cleared(Classification::Itar, false, Classification::Itar),
            NOW,
        );
        assert_eq!(g.classification_ceiling, Classification::Public);
        assert!(!g.permits(Classification::Proprietary));
    }

    #[test]
    fn expired_claim_collapses_to_public() {
        let g = resolve_effective_grant(
            &paid("commercial.kyc", PAST), // expired yesterday
            &cleared(Classification::Cui, false, Classification::Cui),
            NOW,
        );
        assert_eq!(g.classification_ceiling, Classification::Public);
    }

    #[test]
    fn unreadable_chain_fails_closed() {
        // Active paid tier, but the chain could not be read on any axis.
        let g =
            resolve_effective_grant(&paid("enterprise", FUTURE), &ClearanceInputs::unread(), NOW);
        assert_eq!(g.classification_ceiling, Classification::Public);
        // FN unread ⇒ assumed FN.
        assert!(g.foreign_national);
    }

    #[test]
    fn unknown_tenant_ceiling_fails_closed() {
        let g = resolve_effective_grant(
            &paid("commercial.kyc", FUTURE),
            &ClearanceInputs {
                on_chain_clearance: Some(Classification::Cui),
                foreign_national: Some(false),
                tenant_ceiling: None, // tenant node unread
            },
            NOW,
        );
        assert_eq!(g.classification_ceiling, Classification::Public);
    }

    // ---- adversarial: forged tier, ceiling bypass, FN flag (WP6) ---------

    #[test]
    fn forged_or_unknown_tier_does_not_widen_access() {
        // A principal presents a made-up tier string. It must NOT be honored.
        let g = resolve_effective_grant(
            &paid("superuser", FUTURE),
            &cleared(Classification::Itar, false, Classification::Itar),
            NOW,
        );
        assert_eq!(g.classification_ceiling, Classification::Public);
    }

    #[test]
    fn tenant_ceiling_caps_a_higher_clearance() {
        // Principal is cleared to ITAR on-chain, but the tenant scope maxes at
        // CUI — the tenant ceiling wins (least of ceilings).
        let g = resolve_effective_grant(
            &paid("enterprise", FUTURE),
            &cleared(Classification::Itar, false, Classification::Cui),
            NOW,
        );
        assert_eq!(g.classification_ceiling, Classification::Cui);
        assert!(!g.permits(Classification::Itar));
    }

    #[test]
    fn on_chain_clearance_caps_a_permissive_tenant() {
        // Tenant permits ITAR, but the principal is only cleared to Proprietary.
        let g = resolve_effective_grant(
            &paid("enterprise", FUTURE),
            &cleared(Classification::Proprietary, false, Classification::Itar),
            NOW,
        );
        assert_eq!(g.classification_ceiling, Classification::Proprietary);
    }

    #[test]
    fn foreign_national_is_blocked_from_itar_but_reaches_below() {
        // Everything says ITAR, but the principal is a foreign national.
        let g = resolve_effective_grant(
            &paid("enterprise", FUTURE),
            &cleared(Classification::Itar, true, Classification::Itar),
            NOW,
        );
        assert_eq!(g.classification_ceiling, Classification::Itar);
        assert!(!g.permits(Classification::Itar)); // FN gate
        assert!(g.permits(Classification::Cui)); // but everything below
    }

    #[test]
    fn unread_fn_flag_is_treated_as_foreign_national() {
        // The FN flag could not be read; ITAR must NOT open.
        let g = resolve_effective_grant(
            &paid("enterprise", FUTURE),
            &ClearanceInputs {
                on_chain_clearance: Some(Classification::Itar),
                foreign_national: None, // unread
                tenant_ceiling: Some(Classification::Itar),
            },
            NOW,
        );
        assert!(g.foreign_national);
        assert!(!g.permits(Classification::Itar));
    }

    #[test]
    fn academic_tier_is_a_paid_relationship_gated_by_chain() {
        // Academic lifts the commercial cap; the chain then gates to Proprietary.
        let g = resolve_effective_grant(
            &paid("academic", FUTURE),
            &cleared(Classification::Proprietary, false, Classification::Cui),
            NOW,
        );
        assert_eq!(g.classification_ceiling, Classification::Proprietary);
    }

    #[test]
    fn free_tier_is_capped_at_public_regardless_of_clearance() {
        let g = resolve_effective_grant(
            &paid("free", FUTURE),
            &cleared(Classification::Cui, false, Classification::Cui),
            NOW,
        );
        assert_eq!(g.classification_ceiling, Classification::Public);
    }

    #[test]
    fn expiry_boundary_is_strict() {
        // exp == now is NOT active (strictly future).
        let e = paid("commercial.kyc", NOW);
        assert!(!e.is_active(NOW));
        let e2 = paid("commercial.kyc", NOW + 1);
        assert!(e2.is_active(NOW));
    }
}
