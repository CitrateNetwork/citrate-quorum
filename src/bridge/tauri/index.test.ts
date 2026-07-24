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
    expect(() => createTauriBridge().rooms.list()).toThrow(/rooms\.list/);
  });
});
