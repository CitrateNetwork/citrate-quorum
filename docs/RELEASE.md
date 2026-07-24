---
created: 2026-07-24
branch: main
author: Claude Opus 4.8 (1M context), directed by @SaulBuilds
status: active
---

# Release, signing, and updates — the plan and the current (inert) state

> **WP-S1.8 delivered the branding + installer *skeleton*.** This document records
> exactly what is real today and what is deliberately deferred to **QRM-S9** under
> @rule8 (nothing that touches signing keys or an update channel ships before the
> security sign-off). Read the "Current state" table before assuming any signing.

## Current state (S1.8)

| Concern | State today | Lands in |
|---|---|---|
| App identity | `productName: Citrate Quorum`, `identifier: ai.citrate.quorum` | ✅ done |
| Bundle metadata | category, publisher, copyright, license, descriptions, per-platform (linux/macOS/windows) in `src-tauri/tauri.conf.json` | ✅ done |
| Icon set | the shared Citrate mark (federation brand). A quorum-specific icon treatment, if wanted, is a design-team item | ✅ shared brand |
| Local **unsigned** bundle | builds on Linux (`npm run tauri build`) — verified in WP-S1.8 | ✅ done |
| **Code-signing** (macOS notarization, Windows Authenticode) | **NOT configured. No signing identity, no certificate.** | QRM-S9 (@rule8) |
| **Auto-updater** | **NOT wired.** No `tauri-plugin-updater` dependency, no update endpoint, no update pubkey. `bundle.createUpdaterArtifacts: false`. The app has no self-update mechanism. | QRM-S9 (@rule8) |
| Updater signing key | **does not exist in this repo or in CI.** | QRM-S9 (@rule8) |

## Why the updater is inert, not merely disabled

A Tauri auto-updater is only as trustworthy as the key that signs its releases. That
key is an @rule8 secret: minting it, storing its custody, and wiring the update
channel are a security-sign-off deliverable, not skeleton work. So rather than ship
a *disabled-but-present* updater (which invites someone to "just turn it on"), the
skeleton ships **no updater at all** and `createUpdaterArtifacts: false`. There is
nothing to accidentally enable, and there is provably no key material to leak.

A guard in `scripts/check.sh` (`no signing/updater key material`) fails the build if
an updater pubkey, a minisign/tauri signing key file, or a `TAURI_SIGNING_PRIVATE_KEY`
reference ever appears in the tree — so the inert state cannot silently regress.

## The S9 plan (documented now, built then, under @rule8)

1. **Signing identities.** Apple Developer ID + notarization; Windows Authenticode
   (EV or org cert). Custody via the CI secret store, never in the repo.
2. **Update channel.** `tauri-plugin-updater` + a signed release manifest served
   from a controlled endpoint; the update pubkey embedded in the app, the private
   key in CI custody only.
3. **Release ceremony.** A reproducible, signed build with an SBOM, matching the
   T1 audit posture (see `AUDIT_TIER.md`). The updater and signing wiring get their
   own security review before first signed release.
4. **Enterprise packaging.** For the Fortune-200 private deployment, the customer
   may prefer MSI/MDM distribution over an auto-updater; that choice is Q-driven
   and settled with the customer in S9.

## Building a local bundle today (unsigned)

```bash
npm install
npm run tauri build            # or: npx tauri build
# artifacts land in src-tauri/target/release/bundle/<format>/
```

The result is **unsigned** and for local/testing use only. Do not distribute it as
a release — a real release is an S9 signed-ceremony artifact.
