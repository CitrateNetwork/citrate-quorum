// citrate-quorum — the pre-ceremony policy gate (QRM-S2).
//
// These assert the security property, not the rendering: an action that cannot
// be gated must be REFUSED. If any of these ever go green-by-bypass, the app
// has stopped being evidence.
import { describe, expect, it, vi } from "vitest";

import type { GateDecision, GovernedAction } from "../bridge";
import { runGate, settledNote } from "./gate";

/** The gate exists for agent-origin actions; every case below is one. */
const AGENT_ORIGIN = "agent:claude-code";

const ACTION: GovernedAction = {
  actionClass: "repo.write",
  classification: "Proprietary",
  agent: "claude-code",
};

const decision = (over: Partial<GateDecision> = {}): GateDecision => ({
  decisionId: 0,
  verdict: "allow",
  hic: "2",
  grantId: "G-2291",
  reason: "within grant scope, budget, and ceiling",
  chainHead: "0xfeedfacefeedface",
  ungoverned: false,
  ...over,
});

describe("runGate — a human acting directly", () => {
  it("does NOT run the agent gate on the operator's own action", async () => {
    const evaluate = vi.fn(async () => decision());
    const out = await runGate(evaluate, ACTION, "R. Ortiz", "user");
    expect(evaluate).not.toHaveBeenCalled();
    expect(out.open).toBe(true);
    if (!out.open) throw new Error("unreachable");
    // The operator IS the authority — not an alert. HIC-X is an alert state,
    // never a description of someone doing their job.
    expect(out.gate.ungoverned).toBe(false);
    expect(out.gate.verdict).toBe("approved");
    expect(out.gate.hic).toBe("1");
  });

  it("still gates anything that did not come from the operator", async () => {
    const evaluate = vi.fn(async () => decision({ verdict: "ungoverned", ungoverned: true }));
    await runGate(evaluate, ACTION, undefined, "agent:hermes");
    expect(evaluate).toHaveBeenCalled();
  });
});

describe("runGate", () => {
  it("opens the ceremony for an allowed action, carrying the recorded verdict", async () => {
    const out = await runGate(async () => decision(), ACTION, undefined, AGENT_ORIGIN);
    expect(out.open).toBe(true);
    if (!out.open) throw new Error("unreachable");
    expect(out.gate.grantId).toBe("G-2291");
    expect(out.gate.chainHead).toBe("0xfeedfacefeedface");
  });

  it("refuses a denied action before the ceremony ever opens", async () => {
    const out = await runGate(
      async () => decision({ verdict: "deny", reason: "classification ceiling exceeded" }),
      ACTION,
      undefined,
      AGENT_ORIGIN,
    );
    expect(out.open).toBe(false);
    if (out.open) throw new Error("unreachable");
    expect(out.result.outcome).toBe("rejected");
    expect(out.result.note).toBe("classification ceiling exceeded");
    // The verdict still comes back — a denial is evidence too.
    expect(out.result.gate?.verdict).toBe("deny");
  });

  it("FAILS CLOSED when the gate is unreachable — never a bypass", async () => {
    const out = await runGate(
      async () => {
        throw new Error("no active tenant scope — establish one before any governed action");
      },
      ACTION,
      undefined,
      AGENT_ORIGIN,
    );
    expect(out.open).toBe(false);
    if (out.open) throw new Error("unreachable");
    expect(out.result.outcome).toBe("rejected");
    expect(out.result.note).toContain("policy gate unavailable");
    expect(out.result.note).toContain("no active tenant scope");
    // Nothing was recorded, so nothing is claimed.
    expect(out.result.gate).toBeUndefined();
  });

  it("opens for an UNGOVERNED action rather than dropping it (Rule 5)", async () => {
    const out = await runGate(
      async () =>
        decision({
          verdict: "ungoverned",
          hic: "X",
          grantId: null,
          ungoverned: true,
          reason: "no live grant covers this agent",
        }),
      ACTION,
      undefined,
      AGENT_ORIGIN,
    );
    expect(out.open).toBe(true);
    if (!out.open) throw new Error("unreachable");
    expect(out.gate.ungoverned).toBe(true);
    expect(out.gate.hic).toBe("X");
  });

  it("opens for require-approval — that signature IS the HIC-1 approval", async () => {
    const out = await runGate(
      async () => decision({ verdict: "require-approval", hic: "1" }),
      ACTION,
      undefined,
      AGENT_ORIGIN,
    );
    expect(out.open).toBe(true);
    if (!out.open) throw new Error("unreachable");
    expect(out.gate.hic).toBe("1");
  });

  it("stamps the signing principal when the action does not name one", async () => {
    const evaluate = vi.fn(async () => decision());
    await runGate(evaluate, ACTION, "R. Ortiz", AGENT_ORIGIN);
    expect(evaluate).toHaveBeenCalledWith(expect.objectContaining({ principal: "R. Ortiz" }));
  });

  it("never overwrites a principal the action already names", async () => {
    const evaluate = vi.fn(async () => decision());
    await runGate(evaluate, { ...ACTION, principal: "M. Okonkwo" }, "R. Ortiz", AGENT_ORIGIN);
    expect(evaluate).toHaveBeenCalledWith(expect.objectContaining({ principal: "M. Okonkwo" }));
  });

  it("leaves the principal unset rather than inventing one", async () => {
    const evaluate = vi.fn(async () => decision());
    await runGate(evaluate, ACTION, undefined, AGENT_ORIGIN);
    expect(evaluate).toHaveBeenCalledWith(expect.objectContaining({ principal: undefined }));
  });
});

describe("settledNote — the ceremony may only claim what happened", () => {
  it("uses the commit's own words when it reported them", () => {
    expect(settledNote({ commitNote: "grant G-1 revoked", hasCommit: true })).toBe(
      "grant G-1 revoked",
    );
  });

  it("names the chain head when the gate actually recorded a decision", () => {
    expect(settledNote({ chainHead: "0xfeedfacefeedface", hasCommit: false })).toContain(
      "decision recorded",
    );
  });

  it("claims a record only when a commit ran", () => {
    expect(settledNote({ hasCommit: true })).toContain("recorded against your name");
  });

  it("does NOT claim a record when nothing was written", () => {
    // The regression this exists for: a human-origin intent skips the agent
    // gate, so there is no chain head and nothing has been recorded — yet the
    // dialog used to say "recorded against your name in this tenant's evidence
    // chain" anyway, with a provably empty chain behind it.
    const note = settledNote({ hasCommit: false });
    expect(note).not.toContain("recorded against your name");
    expect(note).toContain("recorded nothing");
  });

  it("prefers the commit's words over a stale chain head", () => {
    expect(
      settledNote({ commitNote: "ratified", chainHead: "0xabc", hasCommit: true }),
    ).toBe("ratified");
  });
})
