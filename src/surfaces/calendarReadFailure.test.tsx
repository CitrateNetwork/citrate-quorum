// @vitest-environment jsdom
//
// citrate-quorum — Calendar: a failed read must SAY so.
//
// ORIGIN (QA 2026-08-01): the defect was
// `bridge.calendar.events().then(setEvents).catch(() => {})` — a swallowed
// rejection that left an empty grid indistinguishable from a month with no
// meetings. On a governance calendar, "no governed meetings" and "we could not
// find out" are not the same claim.
//
// PORTED 2026-08-26: the surface no longer reads `calendar.*` at all. Its
// primary read is `meetings.list()`, which is live — the external-provider
// domain it used to depend on did not exist, so the whole surface collapsed to
// a "not wired yet" plate while the real governed data sat unused.
//
// The property is unchanged and is the reason this file survives the rewrite: a
// read that fails must be DISTINGUISHABLE from a read that succeeded and found
// nothing. Only the call it is asserted against moved. Deleting the test with
// the call would have thrown away the invariant along with its subject.
//
// Split from calendarHonesty.test.tsx because it needs a DOM and a real commit
// pass — renderToStaticMarkup never runs effects, so it cannot see this branch,
// and a mutant that re-swallowed the error survived it there.
import { describe, expect, it, vi, beforeEach } from "vitest";
import { act } from "react";
import { createRoot } from "react-dom/client";

const list = vi.fn();

vi.mock("../bridge", async () => {
  const actual = await vi.importActual<typeof import("../bridge")>("../bridge");
  return {
    ...actual,
    bridge: { ...actual.bridge, meetings: { ...actual.bridge.meetings, list: () => list() } },
  };
});

// React 19 requires this before `act` is used, or it warns on every call.
(globalThis as unknown as { IS_REACT_ACT_ENVIRONMENT: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

const { Calendar } = await import("./Calendar");

async function mount(): Promise<HTMLElement> {
  const host = document.createElement("div");
  document.body.appendChild(host);
  await act(async () => {
    createRoot(host).render(<Calendar onGo={() => {}} />);
  });
  return host;
}

beforeEach(() => {
  list.mockReset();
  document.body.innerHTML = "";
});

describe("Calendar — a failed read is stated, not swallowed", () => {
  it("names the failing call when meetings.list() rejects", async () => {
    list.mockRejectedValue(new Error("no tenant scope is established"));
    const host = await mount();
    expect(host.textContent).toContain("meetings.list()");
  });

  it("shows the reason, not an empty grid", async () => {
    list.mockRejectedValue(new Error("no tenant scope is established"));
    const host = await mount();
    // The operator must learn WHY. An empty month and a failed read look
    // identical without this.
    expect(host.textContent).toContain("no tenant scope is established");
  });

  it("does NOT claim the surface is unbuilt when a read merely failed", async () => {
    // The old plate said "Not wired yet ... there is nothing real to show yet".
    // This surface IS wired; saying otherwise sends an operator to wait for a
    // sprint instead of fixing their scope.
    list.mockRejectedValue(new Error("no tenant scope is established"));
    const host = await mount();
    expect(host.textContent).not.toContain("Not wired yet");
    expect(host.textContent).not.toMatch(/QRM-S8/);
  });

  it("stays quiet when the read SUCCEEDS but returns nothing (a genuinely empty tenant)", async () => {
    list.mockResolvedValue([]);
    const host = await mount();
    expect(host.textContent).not.toContain("meetings.list() unavailable");
    expect(host.textContent).toContain("None scheduled");
  });
});
