---
created: 2026-07-26T20:30:00Z
branch: docs/g1-superseded-rooms
author: Claude Opus 4.8 (1M context), directed by @SaulBuilds
status: final
---

# Decision — G1 no longer blocks QRM-S3 (Rooms)

> **Owner sign-off, 2026-07-26, @SaulBuilds:** *"This shouldn't be gated on legal
> opinion... if it is, I supersede it and sign off. We need to finish this. It's
> one of the key features and needs to be finalized and as robust and tested as
> possible."*

## What changed

The federation planset lists **G1 — export-control legal opinion** as a gate
before QRM-S3. That gate is superseded **for the purpose of building Rooms**.
The Rooms surface, the MLS client, transcripts, presence and the kill switch may
be built, merged and shipped without waiting for the written opinion.

## What deliberately did not change

The supersession is scoped to construction. Three things it does not touch:

1. **No export-control compliance claims.** Whether this software satisfies
   ITAR/EAR is a legal conclusion. Nothing we build supplies it, and saying
   otherwise to a customer is a claim the owner cannot underwrite on
   engineering's behalf. `CLAUDE.md` keeps this prohibition verbatim.
2. **No documented handling procedures for controlled technical data.**
3. **MR-4 and the egress allowlist stay requirements.** A room's classification
   ceiling is bounded by the lowest clearance among attested attendees, and a
   controlled classification permits no external model egress unless
   explicitly allowlisted. These are what make a classified room defensible
   under *any* regime, so they are not contingent on knowing which one applies.

The classification ladder remains generic and configuration-driven. Rooms
carries a classification; it does not encode a regime's rules.

## Why the split is the right shape

The original gate conflated two questions that have different owners:

- *May we build it?* — an engineering and product question. The owner's call,
  now answered.
- *May we tell a customer it is compliant?* — a legal question. Still open, and
  no amount of building answers it.

Blocking construction on the second was the error. Removing the first without
also removing the second is the point of this record.

## Blocker that remains, and is not legal

`wss://comms.citrate.ai` — the server-blind MLS relay QRM-S3 depends on — was
**down** when this was written (Caddy up, upstream 502 on `/`, `/health`, and on
a WebSocket upgrade). Memory recorded it as live, so it has failed since. Rooms
cannot be integration-tested until it is back. The client stack
(`comms-client`, `comms-proto`, `comms-core`) exists and is unaffected.
