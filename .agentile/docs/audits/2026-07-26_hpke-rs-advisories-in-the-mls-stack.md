---
created: 2026-07-26T16:10:00Z
branch: feat/qrm-s3-rooms
author: Claude Opus 4.8 (1M context), directed by @SaulBuilds
status: resolved-with-residue
---

# Four hpke-rs advisories, one of them critical, under the federation's MLS stack

> **Owner decision taken 2026-07-26: upgrade OpenMLS (option 1).** Not a
> citrate-quorum bug and not introduced by QRM-S3 — linking the MLS engine is
> what made an existing exposure visible to a gate that runs.
>
> **Correction, same day.** The first version of this document said the exposure
> "includes the deployed relay". **It does not.** `comms-relay` links `comms-core`
> with `default-features = false, features = ["store"]`, so the MLS engine — and
> therefore `hpke-rs` — is not in the relay's dependency graph at all:
>
> ```
> $ cargo tree -p comms-relay -e normal | grep -cE "hpke|openmls"
> 0
> ```
>
> That is the server-blind crate split doing precisely the job it was designed
> for, and it is worth saying out loud: the architecture contained a critical
> advisory to the clients before anyone knew the advisory existed. The exposure
> is **client-side** — the native `comms-client`, and citrate-quorum's Rooms.
> Overstating the blast radius of a security finding is its own kind of
> inaccuracy, so the original sentence is corrected rather than quietly edited.

## What `cargo audit` reports

Surfaced the moment citrate-quorum linked `comms-core` with the `mls` feature:

| ID | Crate | Severity | Title |
|---|---|---|---|
| RUSTSEC-2026-0071 | `hpke-rs` 0.2.0 | **9.3 critical** | Nonce reuse in HPKE context |
| RUSTSEC-2026-0070 | `hpke-rs` 0.2.0 | 8.2 high | Panic when opening or sealing on an export-only context |
| RUSTSEC-2026-0069 | `hpke-rs` 0.2.0 | — | Incorrect length encoding on KDF export |
| RUSTSEC-2026-0072 | `hpke-rs-rust-crypto` 0.2.0 | — | Missing check for an all-zero X25519 shared secret |

All four are fixed in `hpke-rs >= 0.6.0`.

## Why it cannot be fixed by bumping one line

```
hpke-rs v0.2.0
└── openmls_rust_crypto v0.3.0
    └── comms-core v0.0.1  ← only via the `mls` feature
        ├── citrate-quorum          (Rooms)
        ├── comms-client            (the native desktop client)
        └── comms-session

comms-relay links comms-core WITHOUT `mls` → no openmls, no hpke-rs.
```

`citrate-comms` pins `openmls 0.6` / `openmls_rust_crypto 0.3`, and that line of
OpenMLS depends on `hpke-rs 0.2`. The advisory's fix lives behind an OpenMLS
upgrade: current is `openmls 0.8` / `openmls_rust_crypto 0.5`. That is a major
version step across the MLS engine — `add_members`, the provider traits and the
processing API all changed shape — touching the crypto core of a relay that is
live and a client that has shipped. It is a sprint, with its own test pass, not a
dependency bump.

## What the exposure actually is

Stating it precisely, because "critical CVE in your crypto" is the kind of
sentence that gets over- and under-read in the same meeting.

- **Nonce reuse (0071)** is the one that matters for the product's claim. HPKE is
  what MLS uses to seal the group secret to a joiner's KeyPackage. A nonce reuse
  in an AEAD context is a confidentiality break, and confidentiality is exactly
  the property Rooms sells (`server_blindness.rs` proves the relay *stores* no
  plaintext — it does not prove the sealing primitive underneath is sound).
- **The all-zero X25519 check (0072)** is a contributory-behaviour check: without
  it, a peer can force a shared secret an attacker knows.
- **The panic (0070)** is availability, on a context shape MLS does not use here.
- Whether the reuse condition is *reachable* through OpenMLS 0.6's use of
  `hpke-rs` has NOT been established. Nobody should read this document as either
  "we are exploited" or "it does not apply to us". It is unassessed, and the
  honest posture until it is assessed is that a critical advisory sits under the
  primitive our confidentiality claim rests on.

## What was done, and what deliberately was not

**Not done: silencing it.** There is no `audit.toml` ignore in this PR. A
suppressed advisory is invisible the day someone can act on it, and this repo's
gate exists to be believed — `cargo audit` fails, visibly, and the summary line
says why. A gate that is red for a real reason is a gate doing its job.

**Not done: bumping OpenMLS blind.** A crypto-engine major upgrade landed at the
end of a sprint, untested against the deployed relay, would be a worse outcome
than a known-and-named exposure.

**Done: making it visible where the decision gets made.** This file, a line in
the QRM-S3 SCOPE, and the PR body.

## The decision

One of:

1. **Upgrade OpenMLS in citrate-comms** (0.6 → 0.8, `openmls_rust_crypto` 0.3 →
   0.5) as its own work package, with the relay's e2e suite and a redeploy. This
   is the real fix and the one this document recommends.
2. **Assess reachability first** — determine whether OpenMLS 0.6 can reach the
   nonce-reuse condition, and record the analysis. Cheaper, and it either
   downgrades the urgency with evidence or confirms it.
3. **Accept and document with an expiry** — an `audit.toml` ignore with a date
   and this file linked, so the acceptance is explicit and time-boxed rather than
   silent.

Doing nothing is not on the list: it is under every client that joins a room.

**Decision, 2026-07-26 (@SaulBuilds): option 1 — upgrade.**

## Outcome (2026-07-26)

**Option 1 executed** — citrate-comms PR #43, OpenMLS 0.6 → 0.8. `hpke-rs` is now
0.6.1 and all four advisories above are gone, including the 9.3 critical. No
source changes were needed and both live relay tests pass on 0.8.

### The residue, stated as a trade rather than a win

`hpke-rs 0.6.1` pulls `libcrux`, which carries six advisories of its own:

| ID | Crate | Severity | Fixed in | Linked by the app? |
|---|---|---|---|---|
| RUSTSEC-2026-0207 | libcrux-sha3 0.0.8 | 8.2 high | ≥ 0.0.10 | **yes** |
| RUSTSEC-2026-0208 | libcrux-sha3 0.0.8 | 8.2 high | ≥ 0.0.10 | **yes** |
| RUSTSEC-2026-0212 | libcrux-secrets 0.0.5 | 8.2 high | ≥ 0.0.6 | **yes** (aarch64 — this box and the DGX) |
| RUSTSEC-2026-0209 | libcrux-aesgcm 0.0.7 | 6.3 med | *no fix* | no (lockfile only) |
| RUSTSEC-2026-0211 | libcrux-aesgcm 0.0.7 | 6.3 med | *no fix* | no (lockfile only) |
| RUSTSEC-2026-0124 | libcrux-chacha20poly1305 0.0.7 | 8.2 high | ≥ 0.0.8 | no (lockfile only) |

**They cannot be raised from this repository.** Cargo treats `0.0.x` releases as
mutually incompatible, so adding a newer `libcrux-sha3` to a manifest puts BOTH
versions in the graph and `hpke-rs` keeps using 0.0.8 — tried, verified, reverted.
The fix needs `hpke-rs >= 0.7`, which `openmls_rust_crypto 0.5.1` does not accept
(`^0.6`). That is upstream's release cadence.

So the severity profile improved — a critical confidentiality break traded for
panics, a SHAKE correctness bug, and a constant-time swap on aarch64 — and the
MLS stack is **not clean**. Anyone quoting this work should quote that sentence
with it.

### Why there is now an audit.toml, having argued against one

The original position was "no ignore file: a suppressed advisory is invisible the
day someone can act on it." That was right while the fix was in our hands. It is
wrong now: the remaining six are upstream-blocked, so a permanently red gate
stops being a signal and every push needs `--no-verify`, which is how a team
learns to ignore its own gate.

`.cargo/audit.toml` accepts the six **with names, reasons and an expiry**, and
`scripts/check_audit_ignores.py` runs in the gate and fails when:

- the expiry passes (a fresh decision, not renewal by reflex),
- an ignored advisory stops being reported (stale ignore — delete it),
- or `hpke-rs >= 0.7` appears in the lock (**the blocker lifted; these are now
  fixable**).

The blocker is encoded as a testable condition rather than described in prose,
which is the difference between an accepted risk and a forgotten one. All four
failure branches were negative-controlled; the script found its own first bug
(`cargo audit` drops ignored advisories from its report entirely, so asking the
normal run whether an ignore is still needed always answers "no").

## Related

- `.agentile/sprints/active/sprint-qrm-s3/SCOPE.md` — where this was found.
- citrate-comms PR #42 — the crate split that made quorum link the MLS stack.
- R11 in the planset risk register: *"No third-party audit anywhere in the
  federation yet."* This is the first advisory-level finding to land against the
  crypto stack, and it argues R11's mitigation should not wait for QRM-S9.
