// citrate-quorum — the Tauri adapter's mapping seam (QRM-S2).
//
// The Rust DTOs are snake_case; the bridge contract is camelCase. A typo here
// is silent — the field lands `undefined` and a surface renders a blank where a
// verdict should be. These pin the mapping, and pin that no call names a tenant
// (the scope is backend-owned, Rule 6).
import { beforeEach, describe, expect, it, vi } from "vitest";

const invoke = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({ invoke: (...a: unknown[]) => invoke(...a) }));

const { createTauriBridge } = await import("./index");

describe("tauri adapter — policy gate", () => {
  beforeEach(() => {
    invoke.mockReset();
  });

  it("maps the recorded decision onto the bridge contract", async () => {
    invoke.mockResolvedValue({
      decision_id: 7,
      verdict: "require-approval",
      hic: "1",
      grant_id: "G-2291",
      reason: "cost over the grant's HIC-1 threshold",
      chain_head: "0xfeedfacefeedface",
      ungoverned: false,
    });

    const gate = await createTauriBridge().policy.evaluate({
      actionClass: "spend",
      classification: "Public",
      agent: "user",
      cost: 220,
      hic1CostThreshold: 150,
    });

    expect(gate).toEqual({
      decisionId: 7,
      verdict: "require-approval",
      hic: "1",
      grantId: "G-2291",
      reason: "cost over the grant's HIC-1 threshold",
      chainHead: "0xfeedfacefeedface",
      ungoverned: false,
    });
  });

  it("sends the action to Rust in the shape the command expects", async () => {
    invoke.mockResolvedValue({
      decision_id: 0,
      verdict: "allow",
      hic: "2",
      grant_id: "G-1",
      reason: "ok",
      chain_head: "0x00",
      ungoverned: false,
    });

    await createTauriBridge().policy.evaluate({
      actionClass: "repo.write",
      classification: "Proprietary",
      agent: "claude-code",
      principal: "R. Ortiz",
    });

    const [command, args] = invoke.mock.calls[0] as [string, { input: Record<string, unknown> }];
    expect(command).toBe("action_evaluate_and_record");
    expect(args.input).toMatchObject({
      class: "repo.write",
      classification: "Proprietary",
      agent: "claude-code",
      principal: "R. Ortiz",
      cost: 0,
      hic1_cost_threshold: 0,
      mandatory_hic1: false,
    });
    // The frontend must not be able to name the tenant it writes to (Rule 6).
    expect(args.input).not.toHaveProperty("tenant");
  });

  it("routes a human refusal to the refund command with the right decision", async () => {
    invoke.mockResolvedValue({
      decision_id: 8,
      verdict: "rejected",
      hic: "1",
      grant_id: "G-2291",
      reason: "RC-300 refused by the human it was escalated to",
      chain_head: "0xbeef",
      ungoverned: false,
    });
    const gate = await createTauriBridge().policy.reject(7);
    expect(invoke).toHaveBeenCalledWith("action_reject", { decisionId: 7 });
    expect(gate.verdict).toBe("rejected");
    expect(gate.decisionId).toBe(8);
  });

  it("propagates a backend refusal instead of swallowing it (fail closed)", async () => {
    // Tauri rejects with the Rust `Err(String)` verbatim, not an Error.
    const refusal = "no active tenant scope — establish one before any governed action";
    invoke.mockImplementation(async () => {
      throw refusal;
    });
    let caught: unknown;
    try {
      await createTauriBridge().policy.evaluate({
        actionClass: "spend",
        classification: "Public",
        agent: "user",
      });
    } catch (e) {
      caught = e;
    }
    expect(caught).toBe(refusal);
  });
});

describe("tauri adapter — the escalation queue", () => {
  beforeEach(() => {
    invoke.mockReset();
  });

  it("maps a queued escalation onto the bridge contract", async () => {
    invoke.mockResolvedValue([
      {
        decision: 3,
        agent: "claude-code",
        principal: "R. Ortiz",
        action_class: "spend",
        classification: "Public",
        cost: 220,
        correlation_id: "X-7104",
        requested_at_ms: 1000,
      },
    ]);
    const queue = await createTauriBridge().policy.pending();
    expect(invoke).toHaveBeenCalledWith("approvals_pending");
    expect(queue).toEqual([
      {
        decision: 3,
        agent: "claude-code",
        principal: "R. Ortiz",
        actionClass: "spend",
        classification: "Public",
        cost: 220,
        correlationId: "X-7104",
        requestedAtMs: 1000,
      },
    ]);
  });

  it("names the approving human on the wire", async () => {
    invoke.mockResolvedValue({
      decision_id: 4,
      verdict: "approved",
      hic: "1",
      grant_id: "G-1",
      reason: "RC-301 approved by R. Ortiz for claude-code",
      chain_head: "0xabc",
      ungoverned: false,
    });
    const gate = await createTauriBridge().policy.approve(3, "R. Ortiz");
    expect(invoke).toHaveBeenCalledWith("action_approve", {
      decisionId: 3,
      approver: "R. Ortiz",
    });
    expect(gate.verdict).toBe("approved");
    // The approval is its own decision, not a mutation of the escalation.
    expect(gate.decisionId).toBe(4);
  });

  it("an empty queue is an empty array, never a thrown error", async () => {
    invoke.mockResolvedValue([]);
    await expect(createTauriBridge().policy.pending()).resolves.toEqual([]);
  });
});

describe("tauri adapter — tenant scope + ledger", () => {
  beforeEach(() => {
    invoke.mockReset();
  });

  it("reads and sets the backend-owned scope", async () => {
    invoke.mockResolvedValue(null);
    expect(await createTauriBridge().session.activeTenant()).toBeNull();
    expect(invoke).toHaveBeenCalledWith("tenant_active");

    invoke.mockReset();
    invoke.mockResolvedValue(undefined);
    await createTauriBridge().session.setActiveTenant("line-4-automation");
    expect(invoke).toHaveBeenCalledWith("tenant_set", { tenant: "line-4-automation" });
  });

  it("queries the ledger without naming a tenant", async () => {
    invoke.mockResolvedValue([]);
    expect(await createTauriBridge().ledger.query()).toEqual([]);
    expect(invoke).toHaveBeenCalledWith("ledger_records");
  });

  it("reports an unwired domain as unavailable rather than empty", async () => {
    await expect(createTauriBridge().rooms.list()).rejects.toThrow(/rooms\.list/);
  });

  it("REJECTS unwired async domains instead of throwing synchronously", async () => {
    // A synchronous throw from a Promise-typed method escapes `.then(...)` in a
    // useEffect and unmounts the React tree — the packaged app white-screens.
    // Every unwired async method must fail inside the promise.
    const b = createTauriBridge();
    const calls: Array<() => unknown> = [
      () => b.session.current(),
      () => b.wallet.summary(),
      () => b.agents.list(),
      () => b.rooms.list(),
      () => b.meetings.list(),
      () => b.governance.protocols(),
      () => b.journal.list(),
      () => b.calendar.accounts(),
      () => b.repos.list(),
      () => b.settings.tenancy(),
    ];
    for (const call of calls) {
      let threw = false;
      let result: unknown;
      try {
        result = call();
      } catch {
        threw = true;
      }
      expect(threw).toBe(false);
      await expect(result as Promise<unknown>).rejects.toBeInstanceOf(Error);
    }
  });
});

describe("tauri adapter — meetings (QRM-S5)", () => {
  beforeEach(() => {
    invoke.mockReset();
  });

  const detail = (over: Record<string, unknown> = {}) => ({
    id: "m-1",
    name: "Weekly Standup",
    when: "2026-07-23T09:00:00Z",
    tenant: "acme",
    classification: "Proprietary",
    state: "ratified",
    agenda_hash: "0xabc123",
    agenda_source: "generated from sprint-qrm-s5",
    agenda_skipped: 0,
    ratified: true,
    ratified_by: "R. Ortiz",
    ratified_at: 1753460000000,
    content_hash: "0xdeadbeef",
    anchor: { anchored: false, reference: null, reason: "not anchored — no chain address book" },
    quorate: true,
    min_humans: 2,
    attested_humans: 2,
    agenda: [{ n: 1, text: "S5.1", src: "sprint-qrm-s5/SCOPE.md" }],
    attendance: [{ name: "R. Ortiz", attested: true, agent: null, note: null }],
    minutes: ["Reports accepted."],
    decisions: [{ id: "D-1", text: "allow — ci.rerun", link: true }],
    dissent: [],
    ...over,
  });

  it("carries the unavailable anchor REASON, so unanchored never reads as anchored", async () => {
    // The R-A risk: `anchor` left undefined simply does not render, and a
    // ratified-but-unanchored meeting then looks identical to an anchored one.
    invoke.mockResolvedValue(detail());
    const d = await createTauriBridge().meetings.get("m-1");
    expect(d.anchor).toBe("not anchored — no chain address book");
    expect(d.ratified).toBe(true);
  });

  it("prefers the reference once something actually anchors", async () => {
    invoke.mockResolvedValue(
      detail({ anchor: { anchored: true, reference: "block 1284067 · root 0x66d1", reason: "" } }),
    );
    const d = await createTauriBridge().meetings.get("m-1");
    expect(d.anchor).toBe("block 1284067 · root 0x66d1");
  });

  it("says an unfrozen agenda is unfrozen rather than rendering a blank hash", async () => {
    invoke.mockResolvedValue(detail({ agenda_hash: null, state: "scheduled", ratified: false }));
    const d = await createTauriBridge().meetings.get("m-1");
    expect(d.agendaHash).toBe("not frozen — the agenda is still open");
  });

  it("maps the register without naming a tenant (Rule 6)", async () => {
    invoke.mockResolvedValue([
      {
        id: "m-1",
        name: "Weekly Standup",
        when: "2026-07-23T09:00:00Z",
        tpl: "Standup",
        humans: 2,
        agents: 3,
        classification: "Proprietary",
        state: "ratified",
      },
    ]);
    const rows = await createTauriBridge().meetings.list();
    expect(rows[0]).toMatchObject({ id: "m-1", humans: 2, agents: 3, state: "ratified" });
    // No argument at all — the tenant scope is backend-owned (Rule 6).
    expect(invoke).toHaveBeenCalledWith("meetings_list");
  });

  it("hands ratify the exact hash it was given", async () => {
    invoke.mockResolvedValue({ decision_id: 4 });
    await createTauriBridge().meetings.ratify("m-1", "R. Ortiz", "0xdeadbeef");
    expect(invoke).toHaveBeenCalledWith("meeting_ratify", {
      id: "m-1",
      by: "R. Ortiz",
      expectHash: "0xdeadbeef",
    });
  });
});

describe("tauri adapter — journals + briefs (QRM-S5.7)", () => {
  beforeEach(() => {
    invoke.mockReset();
  });

  it("passes an undecided authorship through as undefined, never false", async () => {
    // Rust sends `null` for "we did not decide". Coercing that to `false`
    // would render an agent as a human in an attributed record.
    invoke.mockResolvedValue({
      entries: [
        { id: "j1", date: "2026-07-25", who: "Claude Opus 4.8, directed by @SaulBuilds", kind: "journal", text: "Running it is the test", human: null },
      ],
      source: "6 journals + 4 retros",
    });
    const [e] = await createTauriBridge().journal.list();
    expect(e.human).toBeUndefined();
    expect(e.who).toBe("Claude Opus 4.8, directed by @SaulBuilds");
  });

  it("maps a brief's sections in order", async () => {
    invoke.mockResolvedValue({
      agent: "sbt-41",
      meeting: "Weekly Standup",
      sections: [["Waiting on a human", "nothing pending"], ["Since last time", "3 entries"]],
      source: "6 journals + 4 retros",
    });
    const b = await createTauriBridge().journal.brief("sbt-41", "m-1");
    expect(b.sections[0][0]).toBe("Waiting on a human");
    expect(invoke).toHaveBeenCalledWith("journal_brief", { agent: "sbt-41", meeting: "m-1" });
  });
});
