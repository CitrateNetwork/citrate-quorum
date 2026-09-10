---
created: 2026-07-26T14:40:00Z
branch: feat/qrm-phase0-live-surfaces
author: Claude Opus 4.8 (1M context), directed by @SaulBuilds
status: final
---

# The panel that only existed because something failed

> I made a read succeed, and a signing flow three surfaces away stopped working.
> Nothing about signing changed. The vault setup panel had been reachable only
> as a consolation prize for a failing read, and I took the failure away.

Phase 0 was supposed to be additive: four dark surfaces, four live reads. One of
them was `settings.tenancy()`, which had rejected since the day it was written
because `TenantHierarchy` was not deployed. Now it resolves.

The Settings surface had a thoughtful little accommodation for that failure.
When the tenancy read errored it rendered the error plate **and, underneath it,
the custody vault panel and the signing-identity panel** — with a comment
explaining why, and the comment was right: neither of those depends on tenancy,
and hiding them behind that failure would make the vault unreachable in exactly
the state where an operator needs to set it up.

What the comment did not say, because nobody knew it, was that this had become
the *only* path anyone actually used. The vault panels also live on the Identity
tab, but the default tab is Tenancy, and Tenancy always failed, so the panels
were always right there on arrival. `verify_meetings.py` clicked them at
coordinates measured off that view. It had never once clicked the Identity tab.

So the run went: tenancy resolves → the error plate is gone → the vault panel is
one tab away → the script types a passphrase into empty space → the vault stays
locked → ratification still succeeds, because that is local → the on-chain
registration fails → 16 of 17.

The app was honest about all of it. The ceremony's own words were "registered in
MeetingRegistry FAILED — wallet: custody vault locked or unavailable", which is
exactly what happened and exactly why. The failure took ten minutes to find
because the app said so plainly. That part worked.

The lesson is not "update the coordinates". It is that **an error path is a
render path, and a render path acquires users.** The moment a surface draws
something useful in a failure state, that draw is load-bearing for whoever
found it there — and the day the failure goes away, so does their flow, silently
and at a distance. This one was a script. It could as easily have been an
operator whose muscle memory for setting up a vault was "open Settings, type".

Two things came out of it, and only one is the fix.

The fix: the script now clicks the Identity tab deliberately, and says in a
comment why it has to — so the next person does not rediscover that the panels
used to be somewhere else.

The other thing is bigger. Chasing this turned up that the packaged drive was
not repeatable at all: wiping the app-data directory leaves `custody-master-key`
and `custody-generation` in the shared `ai.citrate.core` keyring, so the second
run of any custody flow hits the anti-rollback guard and reports "custody
envelope corrupt or tampered" — correct behaviour, useless test. Every past run
of this script either happened to be the first, or quietly passed for a reason
nobody checked. It now re-execs itself under a private D-Bus session with its
own empty keyring, which costs one `execvpe` and makes the custody half of the
verification mean something. Deleting a developer's real keyring entries would
have been the other way to get a clean run, and it would have destroyed the
citrate-core vault sealed under the same service.

Both findings came from running the packaged binary, and neither is visible to
any gate. `scripts/check.sh` was 20/0/1 green through all of it.
