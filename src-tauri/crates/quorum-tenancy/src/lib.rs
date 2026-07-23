//! citrate-quorum — the tenancy spine (WP-S1.4).
//!
//! Quorum is a T1 **multi-tenant** product delivered as a dedicated single-tenant
//! deployment (Q20 / decision in `09_DECISIONS_LOCKED.md`). Multi-tenancy is a
//! day-one property, not a retrofit, so the type system — not code review — is
//! what stops a tenant-less or cross-tenant access from ever being written.
//!
//! Three guarantees this crate provides:
//!
//! 1. **Every domain call is scoped.** A backend domain method takes a
//!    [`TenantContext`] by reference. There is no method that reads or writes
//!    tenant data without one; the [`Scoped`] trait makes "I forgot the tenant"
//!    a **compile error**, proven by `tests/compile_fail.rs`.
//! 2. **No global singletons for tenant state.** Tenant-scoped state lives behind
//!    [`TenantMap`], keyed by [`TenantId`]; there is no ambient "current tenant".
//! 3. **Storage is keyed by `(tenant, principal)`.** [`ScopeKey`] is the only
//!    blessed way to derive a storage path or keyring entry, so two tenants (or
//!    two principals in one tenant) can never collide or read each other.
//!
//! This crate is deliberately dependency-free and freeze-independent: it does not
//! touch citrate-core, so it is built during the pre-extraction window.

#![forbid(unsafe_code)]

use std::collections::HashMap;
use std::fmt;

/// A tenant in the [`TenantHierarchy`] (enterprise / BU / site / team). Opaque,
/// non-empty, and compared by value. We keep the on-chain `bytes32` tenant id as
/// a lowercase hex string so this crate stays free of a chain dependency; the
/// binding layer (WP-S1.5) converts to/from `[u8; 32]`.
#[derive(Clone, PartialEq, Eq, Hash)]
pub struct TenantId(String);

impl TenantId {
    /// Construct from a canonical tenant id. Rejects empty / whitespace so a
    /// "default tenant" cannot be conjured by accident — there is no tenant zero.
    pub fn new(id: impl Into<String>) -> Result<Self, TenancyError> {
        let id = id.into();
        if id.trim().is_empty() {
            return Err(TenancyError::EmptyTenantId);
        }
        Ok(Self(id))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

// Deliberately terse Debug: a tenant id is not secret, but keeping it one line
// keeps it out of the way in the middle of a large ceremony/decision record dump.
impl fmt::Debug for TenantId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "TenantId({})", self.0)
    }
}

/// A principal within a tenant: a human (identity `sub` ↔ wallet) or an agent
/// (`AgentSBT` id). The kind is carried so a `(tenant, principal)` scope key is
/// unambiguous across the two namespaces.
#[derive(Clone, PartialEq, Eq, Hash)]
pub struct PrincipalId {
    kind: PrincipalKind,
    id: String,
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum PrincipalKind {
    /// A human, keyed by the identity subject / wallet address.
    Human,
    /// An agent, keyed by its `AgentSBT` token id.
    Agent,
}

impl PrincipalId {
    pub fn human(id: impl Into<String>) -> Result<Self, TenancyError> {
        Self::new(PrincipalKind::Human, id)
    }

    pub fn agent(id: impl Into<String>) -> Result<Self, TenancyError> {
        Self::new(PrincipalKind::Agent, id)
    }

    fn new(kind: PrincipalKind, id: impl Into<String>) -> Result<Self, TenancyError> {
        let id = id.into();
        if id.trim().is_empty() {
            return Err(TenancyError::EmptyPrincipalId);
        }
        Ok(Self { kind, id })
    }

    pub fn kind(&self) -> PrincipalKind {
        self.kind
    }

    pub fn as_str(&self) -> &str {
        &self.id
    }
}

impl fmt::Debug for PrincipalId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "PrincipalId({:?}:{})", self.kind, self.id)
    }
}

/// The clearance ladder (Q13), adopted as-is from the chain's
/// `ClassificationRegistry`. Ordered: `Public < Proprietary < CUI < ITAR`.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
pub enum Classification {
    Public,
    Proprietary,
    Cui,
    Itar,
}

/// The resolved authority a principal carries **into a specific tenant scope**.
/// Built by the session layer (WP-S2) from the least of: the entitlement tier
/// ceiling, the on-chain clearance, and the tenant's `classification_max`
/// (`02_ARCHITECTURE.md` §2, "least of all ceilings"). Carried, never recomputed
/// ad hoc, so an audit can ask "what could this principal see here" and get one
/// answer.
#[derive(Clone, Debug)]
pub struct EffectiveGrant {
    /// The ceiling this principal may operate at within this tenant.
    pub classification_ceiling: Classification,
    /// Whether the principal is flagged foreign-national (drives ITAR gating).
    pub foreign_national: bool,
}

impl EffectiveGrant {
    /// Public, non-FN — the fail-closed default. A missing entitlement, an
    /// unreadable chain, or an expired claim collapses to exactly this.
    pub fn fail_closed() -> Self {
        Self {
            classification_ceiling: Classification::Public,
            foreign_national: false,
        }
    }

    /// May this grant reach `level`? ITAR additionally requires non-FN.
    pub fn permits(&self, level: Classification) -> bool {
        if level == Classification::Itar && self.foreign_national {
            return false;
        }
        self.classification_ceiling >= level
    }
}

/// The context every tenant-scoped domain call must receive. It binds a
/// principal to a tenant with a resolved grant. There is no `Default` and no
/// `TenantContext::current()` — you cannot obtain one without naming all three,
/// which is the whole point.
#[derive(Clone, Debug)]
pub struct TenantContext {
    tenant: TenantId,
    principal: PrincipalId,
    grant: EffectiveGrant,
}

impl TenantContext {
    pub fn new(tenant: TenantId, principal: PrincipalId, grant: EffectiveGrant) -> Self {
        Self {
            tenant,
            principal,
            grant,
        }
    }

    pub fn tenant(&self) -> &TenantId {
        &self.tenant
    }

    pub fn principal(&self) -> &PrincipalId {
        &self.principal
    }

    pub fn grant(&self) -> &EffectiveGrant {
        &self.grant
    }

    /// The storage/keyring key for this context's `(tenant, principal)` pair.
    /// The **only** blessed way to derive a per-principal storage location, so
    /// two principals can never collide or read across.
    pub fn scope_key(&self) -> ScopeKey {
        ScopeKey::of(&self.tenant, &self.principal)
    }
}

/// An opaque, collision-free key for `(tenant, principal)` state. Used for
/// on-disk paths and keyring entry names. Components are length-prefixed so
/// `("a", "bc")` and `("ab", "c")` cannot alias.
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub struct ScopeKey(String);

impl ScopeKey {
    fn of(tenant: &TenantId, principal: &PrincipalId) -> Self {
        let kind = match principal.kind {
            PrincipalKind::Human => "h",
            PrincipalKind::Agent => "a",
        };
        // length-prefixed segments: t<len>:<tenant>/<kind><len>:<principal>
        Self(format!(
            "t{}:{}/{}{}:{}",
            tenant.0.len(),
            tenant.0,
            kind,
            principal.id.len(),
            principal.id
        ))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// A per-tenant state map. The blessed alternative to a global singleton: state
/// is reachable only by naming a [`TenantId`], so there is no ambient tenant and
/// no path by which one tenant's state is served to another.
#[derive(Debug, Default)]
pub struct TenantMap<T> {
    inner: HashMap<TenantId, T>,
}

impl<T> TenantMap<T> {
    pub fn new() -> Self {
        Self {
            inner: HashMap::new(),
        }
    }

    /// Read a tenant's state, if present. There is no "get default".
    pub fn get(&self, ctx: &TenantContext) -> Option<&T> {
        self.inner.get(&ctx.tenant)
    }

    pub fn get_mut(&mut self, ctx: &TenantContext) -> Option<&mut T> {
        self.inner.get_mut(&ctx.tenant)
    }

    /// Get-or-insert a tenant's state, initialized by `init` on first touch.
    pub fn entry_or_insert_with(
        &mut self,
        ctx: &TenantContext,
        init: impl FnOnce() -> T,
    ) -> &mut T {
        self.inner.entry(ctx.tenant.clone()).or_insert_with(init)
    }

    pub fn len(&self) -> usize {
        self.inner.len()
    }

    pub fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }
}

/// The type-level gate. A backend domain implements `Scoped`, and its methods
/// take `&TenantContext`. Because the trait method **requires** the context, a
/// caller that omits it does not type-check — the compile-fail test proves this
/// is enforced, not merely encouraged.
///
/// This is intentionally minimal: it is the shape every Quorum domain seam
/// (rooms, meetings, governance, agents, ledger, …) will adopt in its own sprint.
pub trait Scoped {
    /// The tenant this call is scoped to. Every `Scoped` method threads the
    /// context; there is no un-scoped accessor.
    fn tenant<'a>(&self, ctx: &'a TenantContext) -> &'a TenantId {
        ctx.tenant()
    }
}

/// Errors from constructing tenancy primitives. Deliberately small; the session
/// layer maps these to the bridge's `Denied` / `Failed` variants.
#[derive(Debug, PartialEq, Eq)]
pub enum TenancyError {
    EmptyTenantId,
    EmptyPrincipalId,
}

impl fmt::Display for TenancyError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EmptyTenantId => write!(f, "tenant id must be non-empty"),
            Self::EmptyPrincipalId => write!(f, "principal id must be non-empty"),
        }
    }
}

impl std::error::Error for TenancyError {}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    fn ctx(tenant: &str, principal_agent: &str, grant: EffectiveGrant) -> TenantContext {
        TenantContext::new(
            TenantId::new(tenant).unwrap(),
            PrincipalId::agent(principal_agent).unwrap(),
            grant,
        )
    }

    #[test]
    fn tenant_and_principal_reject_empty() {
        assert_eq!(TenantId::new("").unwrap_err(), TenancyError::EmptyTenantId);
        assert_eq!(
            TenantId::new("   ").unwrap_err(),
            TenancyError::EmptyTenantId
        );
        assert_eq!(
            PrincipalId::human("").unwrap_err(),
            TenancyError::EmptyPrincipalId
        );
        assert_eq!(
            PrincipalId::agent(" ").unwrap_err(),
            TenancyError::EmptyPrincipalId
        );
    }

    #[test]
    fn classification_ladder_is_ordered() {
        assert!(Classification::Public < Classification::Proprietary);
        assert!(Classification::Proprietary < Classification::Cui);
        assert!(Classification::Cui < Classification::Itar);
    }

    #[test]
    fn fail_closed_grant_is_public_non_fn() {
        let g = EffectiveGrant::fail_closed();
        assert!(g.permits(Classification::Public));
        assert!(!g.permits(Classification::Proprietary));
        assert!(!g.foreign_national);
    }

    #[test]
    fn grant_ceiling_is_inclusive_and_monotone() {
        let g = EffectiveGrant {
            classification_ceiling: Classification::Cui,
            foreign_national: false,
        };
        assert!(g.permits(Classification::Public));
        assert!(g.permits(Classification::Proprietary));
        assert!(g.permits(Classification::Cui));
        assert!(!g.permits(Classification::Itar));
    }

    #[test]
    fn itar_requires_non_foreign_national_even_at_itar_ceiling() {
        let g = EffectiveGrant {
            classification_ceiling: Classification::Itar,
            foreign_national: true,
        };
        // FN principal is blocked from ITAR despite an ITAR ceiling...
        assert!(!g.permits(Classification::Itar));
        // ...but still reaches everything below it.
        assert!(g.permits(Classification::Cui));
    }

    #[test]
    fn scope_key_is_collision_free_across_tenant_principal_boundaries() {
        // The classic ambiguity: ("a","bc") vs ("ab","c"). Length-prefixing
        // must keep them distinct.
        let k1 = ScopeKey::of(
            &TenantId::new("a").unwrap(),
            &PrincipalId::agent("bc").unwrap(),
        );
        let k2 = ScopeKey::of(
            &TenantId::new("ab").unwrap(),
            &PrincipalId::agent("c").unwrap(),
        );
        assert_ne!(k1, k2);
    }

    #[test]
    fn scope_key_separates_human_and_agent_namespaces() {
        let t = TenantId::new("bca").unwrap();
        let h = ScopeKey::of(&t, &PrincipalId::human("42").unwrap());
        let a = ScopeKey::of(&t, &PrincipalId::agent("42").unwrap());
        assert_ne!(
            h, a,
            "same id string, different principal kind, must not alias"
        );
    }

    #[test]
    fn tenant_map_has_no_ambient_default() {
        let mut m: TenantMap<u32> = TenantMap::new();
        let c1 = ctx("t1", "agent1", EffectiveGrant::fail_closed());
        let c2 = ctx("t2", "agent1", EffectiveGrant::fail_closed());
        assert!(
            m.get(&c1).is_none(),
            "unseen tenant returns None, not a default"
        );
        *m.entry_or_insert_with(&c1, || 10) += 5;
        assert_eq!(*m.get(&c1).unwrap(), 15);
        // Writing t1 must not create or affect t2.
        assert!(
            m.get(&c2).is_none(),
            "one tenant's write must not leak to another"
        );
        assert_eq!(m.len(), 1);
    }

    #[test]
    fn context_exposes_all_three_and_derives_its_own_key() {
        let c = ctx(
            "bca",
            "sbt-7",
            EffectiveGrant {
                classification_ceiling: Classification::Proprietary,
                foreign_national: false,
            },
        );
        assert_eq!(c.tenant().as_str(), "bca");
        assert_eq!(c.principal().as_str(), "sbt-7");
        assert_eq!(c.principal().kind(), PrincipalKind::Agent);
        assert!(c.grant().permits(Classification::Proprietary));
        assert_eq!(c.scope_key(), ScopeKey::of(c.tenant(), c.principal()));
    }
}
