// @vitest-environment jsdom
//
// citrate-quorum — Calendar: a failed events read must SAY so (QA 2026-08-01).
//
// Split from calendarHonesty.test.tsx because it needs a DOM and a real commit
// pass. That file uses renderToStaticMarkup, which never runs effects — so it
// could not see this branch at all, and a mutant that re-swallowed the error
// survived it. An untested fix is the thing this whole QA pass keeps finding, so
// it is not one I get to ship.
//
// The defect: `bridge.calendar.events().then(setEvents).catch(() => {})`. In the
// packaged app `calendar.events()` is an `na()` stub that rejects, so the grid
// rendered empty and indistinguishable from a month with no meetings. On a
// governance calendar, "no governed meetings" and "we could not find out" are
// not the same claim.
//
// The repo's own vitest environment is `node` (no config sets otherwise), hence
// the docblock above rather than a global change.
import { describe, expect, it, vi, beforeEach } from "vitest";
import { act } from "react";
import { createRoot } from "react-dom/client";

const events = vi.fn();
const accounts = vi.fn();

vi.mock("../bridge", async () => {
  const actual = await vi.importActual<typeof import("../bridge")>("../bridge");
  return {
    ...actual,
    bridge: {
      ...actual.bridge,
      calendar: { events: () => events(), accounts: () => accounts() },
    },
  };
});

// React 19 requires this to be set before `act` is used, or it warns on every
// call that the environment does not support it.
(globalThis as unknown as { IS_REACT_ACT_ENVIRONMENT: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

const { Calendar } = await import("./Calendar");

async function mount(): Promise<HTMLElement> {
  const host = document.createElement("div");
  document.body.appendChild(host);
  await act(async () => {
    createRoot(host).render(<Calendar />);
  });
  return host;
}

beforeEach(() => {
  events.mockReset();
  accounts.mockReset();
  document.body.innerHTML = "";
  // Keep the surface past its early error return so the grid renders.
  accounts.mockResolvedValue([]);
});

describe("Calendar — a failed events read is stated, not swallowed", () => {
  it("names the failing call when calendar.events() rejects", async () => {
    events.mockRejectedValue(new Error("unavailable: calendar.events"));
    const host = await mount();
    expect(host.textContent).toContain("calendar.events() unavailable");
  });

  it("says the grid is empty because the READ failed, not because the month is", async () => {
    events.mockRejectedValue(new Error("unavailable: calendar.events"));
    const host = await mount();
    // The distinction is the whole point of the fix.
    expect(host.textContent).toContain("not because the month is");
  });

  it("stays silent when the read SUCCEEDS but returns nothing (a genuinely empty month)", async () => {
    events.mockResolvedValue([]);
    const host = await mount();
    expect(host.textContent).not.toContain("calendar.events() unavailable");
  });
});
