// citrate-quorum — the pre-ceremony policy gate (QRM-S2).
//
// These assert the security property, not the rendering: an action that cannot
// be gated must be REFUSED. If any of these ever go green-by-bypass, the app
// has stopped being evidence.
import { describe, expect, it, vi } from "vitest";

import type { GateDecision, GovernedAction } from "../bridge";
import { runGate } from "./gate";

const ACTION: GovernedAction = {
  actionClass: "repo.write",
  classification: "Proprietary",
  agent: "claude-code",
};

const decision = (over: Partial<GateDecision> = {}): GateDecision => ({
  verdict: "allow",
  hic: "2",
  grantId: "G-2291",
  reason: "within grant scope, budget, and ceiling",
  chainHead: "0xfeedfacefeedface",
  ungoverned: false,
  ...over,
});

describe("runGate", () => {
  it("opens the ceremony for an allowed action, carrying the recorded verdict", async () => {
    const out = await runGate(async () => decision(), ACTION);
    expect(out.open).toBe(true);
    if (!out.open) throw new Error("unreachable");
    expect(out.gate.grantId).toBe("G-2291");
    expect(out.gate.chainHead).toBe("0xfeedfacefeedface");
  });

  it("refuses a denied action before the ceremony ever opens", async () => {
    const out = await runGate(
      async () => decision({ verdict: "deny", reason: "classification ceiling exceeded" }),
      ACTION,
    );
    expect(out.open).toBe(false);
    if (out.open) throw new Error("unreachable");
    expect(out.result.outcome).toBe("rejected");
    expect(out.result.note).toBe("classification ceiling exceeded");
    // The verdict still comes back — a denial is evidence too.
    expect(out.result.gate?.verdict).toBe("deny");
  });

  it("FAILS CLOSED when the gate is unreachable — never a bypass", async () => {
    const out = await runGate(async () => {
      throw new Error("no active tenant scope — establish one before any governed action");
    }, ACTION);
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
    );
    expect(out.open).toBe(true);
    if (!out.open) throw new Error("unreachable");
    expect(out.gate.hic).toBe("1");
  });

  it("stamps the signing principal when the action does not name one", async () => {
    const evaluate = vi.fn(async () => decision());
    await runGate(evaluate, ACTION, "R. Ortiz");
    expect(evaluate).toHaveBeenCalledWith(expect.objectContaining({ principal: "R. Ortiz" }));
  });

  it("never overwrites a principal the action already names", async () => {
    const evaluate = vi.fn(async () => decision());
    await runGate(evaluate, { ...ACTION, principal: "M. Okonkwo" }, "R. Ortiz");
    expect(evaluate).toHaveBeenCalledWith(expect.objectContaining({ principal: "M. Okonkwo" }));
  });

  it("leaves the principal unset rather than inventing one", async () => {
    const evaluate = vi.fn(async () => decision());
    await runGate(evaluate, ACTION);
    expect(evaluate).toHaveBeenCalledWith(expect.objectContaining({ principal: undefined }));
  });
});
