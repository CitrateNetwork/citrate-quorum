---
created: 2026-07-22
branch: main
author: Claude Opus 4.8 (1M context), directed by @SaulBuilds
status: active
---

# Audit tier — citrate-quorum

**Tier: T1.**

## Why T1

| Trigger | Present |
|---|---|
| Handles user keys / signs transactions | ✅ inherits the kit's custody + ceremony |
| Money movement | ✅ wallet, staking, seat licensing |
| Identity / authentication | ✅ OIDC RP, upstream IdP federation, entitlements |
| Authorization decisions | ✅ the entire product is an authorization system |
| Binary distribution to end users | ✅ signed installer + updater (S9) |
| Deployed to a customer's production environment | ✅ Fortune-200 private deployment |
| Processes classified / export-controlled material | ✅ in scope (Q13; gate G1) |

## What T1 requires here

- Full internal audit before any customer deployment.
- **External third-party audit** of the governance contracts and the custody /
  ceremony path before release (no federation repo has had one yet — this is the
  first).
- @rule8 security sign-off for: key custody, updater signing keys, gated
  downloads, any export-controlled surface, and the agent adapter sandbox.
- Formal (TLA+) specs with cited invariants for the governance contracts, ratchet-
  enforced so an invariant cannot silently disappear.
- Penetration test and an incident-response runbook before release.
- A vendor-security-assessment response package (SBOM, secure-SDLC evidence,
  subprocessor list) — the customer is the audited entity, we are the vendor.

## What we must never claim

"SOC 2 certified" or "SOC 2 compliant." The accurate claim is: **designed to
generate SOC 2 Type 2 control evidence in the customer's environment.**
