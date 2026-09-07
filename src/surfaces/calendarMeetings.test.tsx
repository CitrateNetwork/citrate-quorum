// @vitest-environment jsdom
//
// citrate-quorum — Calendar renders the tenant's real governed meetings.
//
// Written with the rewrite that replaced `calendar.accounts()`/`events()` — an
// external-provider integration that does not exist — with `meetings.list()`,
// which is live. What these pin is the part that can regress silently: a
// meeting that is real but does not appear.
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
(globalThis as unknown as { IS_REACT_ACT_ENVIRONMENT: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

const { Calendar, meetingDay } = await import("./Calendar");

const meeting = (over: Record<string, unknown> = {}) => ({
  id: "m-1", name: "Weekly CCB", when: new Date().toISOString(),
  tpl: "Change Control Board", humans: 2, agents: 1,
  classification: "CUI", state: "scheduled", ...over,
});

async function mount(): Promise<HTMLElement> {
  const host = document.createElement("div");
  document.body.appendChild(host);
  await act(async () => { createRoot(host).render(<Calendar onGo={() => {}} />); });
  return host;
}

beforeEach(() => { list.mockReset(); document.body.innerHTML = ""; });

describe("Calendar — real governed meetings", () => {
  it("renders a meeting scheduled this month", async () => {
    list.mockResolvedValue([meeting({ name: "Weekly CCB" })]);
    const host = await mount();
    expect(host.textContent).toContain("Weekly CCB");
    expect(host.textContent).toContain("1 governed meeting in this tenant");
  });

  it("carries the meeting's real classification and state, not a default", async () => {
    list.mockResolvedValue([meeting({ classification: "ITAR", state: "ratified" })]);
    const host = await mount();
    expect(host.textContent).toContain("ITAR");
    expect(host.textContent).toContain("ratified");
  });

  /**
   * The backend stores `when` VERBATIM and never parses it, so this surface
   * cannot assume a format. A meeting whose date will not parse must be shown
   * as unplaceable rather than dropped — a real meeting silently missing from
   * the calendar is the worst outcome this surface has.
   */
  it("surfaces a meeting whose date it cannot read instead of dropping it", async () => {
    list.mockResolvedValue([meeting({ id: "m-2", name: "Odd Date", when: "next tuesday-ish" })]);
    const host = await mount();
    expect(meetingDay("next tuesday-ish")).toBeNull();
    expect(host.textContent).toContain("Odd Date");
    expect(host.textContent).toContain("cannot read");
  });

  it("does not invent an external-provider concept it has no source for", async () => {
    list.mockResolvedValue([meeting()]);
    const host = await mount();
    // The old surface badged "mirrored external" and listed "Connected
    // accounts" against a domain that rejects. Nothing mirrors anything.
    expect(host.textContent).not.toContain("mirrored external");
    expect(host.textContent).not.toContain("Connected accounts");
    expect(host.textContent).not.toContain("Outlook");
  });

  it("an empty tenant reads as empty, and says whose record it is", async () => {
    list.mockResolvedValue([]);
    const host = await mount();
    expect(host.textContent).toContain("None scheduled");
    expect(host.textContent).toContain("0 governed meetings");
  });
});

describe("meetingDay", () => {
  it("reads an RFC3339 timestamp", () => {
    expect(meetingDay("2026-08-26T15:00:00Z")?.getUTCDate()).toBe(26);
  });
  it("returns null rather than coercing an unreadable value to today", () => {
    expect(meetingDay("")).toBeNull();
    expect(meetingDay("soon")).toBeNull();
  });
});
