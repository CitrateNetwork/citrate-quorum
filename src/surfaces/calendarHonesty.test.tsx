// citrate-quorum — Calendar surface honesty (QA 2026-08-01). Rule 1.
//
// THE FIRST SURFACE TEST IN THIS REPO. All 51 existing frontend tests live in
// bridge/, ceremony/ and shell/; the 14 surfaces (3,742 lines) had none. The
// wiring-contract ratchets in scripts/check.sh cover part of this ground, and
// they say so honestly — the "no fabricated stats" scan comments that it
// "CANNOT catch every invented value (a bare 4 is indistinguishable from a real
// one) ... The proof is running the packaged app against an empty tenant and
// reading what it claims." These tests are that proof, in CI form.
//
// What they pin, all found on Calendar:
//
//   1. A FABRICATED INCIDENT. The surface rendered a "Sync conflict" plate
//      reading "An Outlook edit moved CCB #13 to 15:00 — but its time is a
//      governed field". Static JSX. No state behind it, no condition in front of
//      it. It rendered against a brand-new empty tenant, and it slipped every
//      ratchet because it contains no thousands separator and no percentage.
//
//   2. A HARDCODED MONTH. The grid was pinned to July 2026 — header, a 3-day
//      leading offset, and 31 days, all literals. It was already showing the
//      wrong month on the day this test was written.
//
//   3. ENABLED-BUT-INERT BUTTONS. Three buttons with no handler. Journal.tsx
//      already shows this repo's correct pattern for an unbuilt action:
//      `disabled` plus a `title` saying why. Calendar deviated from it.
//
// SCOPE, STATED HONESTLY: in the packaged app `calendar.accounts()` is an `na()`
// stub, so the surface hits its error plate and returns before any of this
// renders. The fabrication is therefore dev/demo-facing today, NOT shipped to a
// customer. It is fixed anyway because the bridge is explicitly designed so each
// domain flips live independently ("flipping a domain sim→live changes one
// adapter, zero surfaces") — the moment `accounts()` lands while `events()`
// stays stubbed, this becomes user-facing with no code change to warn anyone.
//
// These render via renderToStaticMarkup, which does not run effects, so useDomain
// sits in `loading` — which is precisely the state that rendered the fabrication.
import { describe, expect, it } from "vitest";
import { renderToStaticMarkup } from "react-dom/server";
import { Calendar } from "./Calendar";

const html = () => renderToStaticMarkup(<Calendar />);

describe("Calendar honesty — no fabricated incident (Rule 1)", () => {
  it("NEVER renders the invented Outlook sync conflict", () => {
    const out = html();
    expect(out).not.toContain("An Outlook edit moved");
    expect(out).not.toContain("CCB #13");
  });

  it("does not announce a sync conflict with nothing behind it", () => {
    // The plate itself is only honest if a real conflict drives it. Until
    // calendar.syncStatus() is wired there is nothing to drive it with.
    expect(html()).not.toContain("Sync conflict");
  });
});

describe("Calendar honesty — the month is real, not pinned", () => {
  it("does not hardcode July 2026", () => {
    expect(html()).not.toContain("July 2026");
  });

  it("names the month the user is actually in", () => {
    const now = new Date();
    const label = now.toLocaleString("en-US", { month: "long", year: "numeric" });
    expect(html()).toContain(label);
  });

  it("lays the grid out for THIS month — correct leading offset and day count", () => {
    const now = new Date();
    const y = now.getFullYear();
    const m = now.getMonth();
    const offset = new Date(y, m, 1).getDay();
    const days = new Date(y, m + 1, 0).getDate();
    const out = html();
    // Pull each day cell's rendered value, in order. Blanks render as "".
    const rendered = [...out.matchAll(/class="mono tabular"[^>]*>([^<]*)</g)].map((m) => m[1]);

    // Leading blanks — the padding before the 1st — equal the real weekday
    // offset. Counting ALL blanks would also sweep up the trailing padding that
    // squares the grid to whole weeks, which is layout, not a claim.
    const firstDay = rendered.indexOf("1");
    expect(firstDay).toBe(offset);

    // The month's real days appear, in order, and nothing beyond it does.
    expect(rendered.slice(firstDay, firstDay + days)).toEqual(
      Array.from({ length: days }, (_, i) => String(i + 1)),
    );
    expect(rendered).not.toContain(String(days + 1));

    // Whole weeks.
    expect(rendered.length % 7).toBe(0);
  });
});

describe("Calendar honesty — an unbuilt action looks unbuilt", () => {
  it("does not ship enabled buttons that do nothing", () => {
    const out = html();
    // Every button this surface renders is for an action QRM-S8 has not built.
    // Follow Journal.tsx: disabled, with a title that says why.
    for (const label of ["Keep governed time", "Propose amendment", "New governed meeting"]) {
      const idx = out.indexOf(label);
      if (idx === -1) continue; // removed entirely is also honest
      const tag = out.lastIndexOf("<button", idx);
      expect(out.slice(tag, idx), `"${label}" must be disabled while unbuilt`).toContain("disabled");
    }
  });
});
