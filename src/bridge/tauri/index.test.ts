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
    // `rooms` used to be the example here; it is live as of QRM-S3. Governance is
    // the honest stand-in now — and when it lands, this must move again rather
    // than be deleted: the property under test is that an unwired domain SAYS so.
    await expect(createTauriBridge().governance.protocols()).rejects.toThrow(/governance\.protocols/);
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

  it("does not fabricate an anchor on the local read", async () => {
    // `get()` is a LOCAL read. The anchor is a chain read behind its own
    // command, so this must leave the field empty rather than guess.
    invoke.mockResolvedValue(detail());
    const d = await createTauriBridge().meetings.get("m-1");
    expect(d.anchor).toBeUndefined();
    expect(d.ratified).toBe(true);
  });

  it("never reports an unreachable chain as unanchored", async () => {
    // The distinction the anchor row exists for: "we asked and the answer is
    // no" is a different fact from "we could not ask".
    invoke.mockResolvedValue({
      state: "unreachable",
      contract: "0x7cef",
      reason: "connection refused",
    });
    const line = await createTauriBridge().meetings.anchorState("m-1");
    expect(line).toContain("unknown");
    expect(line).toContain('NOT the same as "not anchored"');
  });

  it("renders a hash disagreement as a MISMATCH, not as absence", async () => {
    invoke.mockResolvedValue({ state: "mismatch", contract: "0x7cef", on_chain: "0xdead" });
    const line = await createTauriBridge().meetings.anchorState("m-1");
    expect(line).toContain("MISMATCH");
    expect(line).toContain("0xdead");
  });

  it("reports a real anchor with its block and registering key", async () => {
    invoke.mockResolvedValue({
      state: "anchored", block: 108899, ratifier: "0x4fab", contract: "0x7cef",
    });
    const line = await createTauriBridge().meetings.anchorState("m-1");
    expect(line).toContain("block 108899");
    expect(line).toContain("0x4fab");
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

describe("tauri adapter — live chain reads (Phase 0)", () => {
  beforeEach(() => {
    invoke.mockReset();
  });

  it("maps node status, keeping a missing answer as null rather than zero", async () => {
    // `peers: null` means the endpoint did not answer net_peerCount. Coercing
    // that to 0 would render "no peers" — a claim the node never made.
    invoke.mockResolvedValue({
      rpc_url: "https://rpc.citrate.ai",
      book: "vendored src/generated/addresses.json, chain 40204, 51 contracts",
      chain_id: 40204,
      height: 140363,
      peers: null,
      client: null,
      syncing: false,
      latency_ms: 41,
      base_fee_wei: "1000000000",
      blue_score: 140363,
    });
    const s = await createTauriBridge().node.status();
    expect(s).toEqual({
      rpcUrl: "https://rpc.citrate.ai",
      book: "vendored src/generated/addresses.json, chain 40204, 51 contracts",
      chainId: 40204,
      height: 140363,
      peers: null,
      client: null,
      syncing: false,
      latencyMs: 41,
      baseFeeWei: "1000000000",
      blueScore: 140363,
    });
    expect(invoke).toHaveBeenCalledWith("node_status");
  });

  it("maps block rows onto the fields 40204 actually returns", async () => {
    invoke.mockResolvedValue([
      {
        height: 140363,
        hash: "0x531a",
        txs: 0,
        proposer: "0x25b7",
        gas_used: 0,
        gas_limit: 30000000,
        timestamp: 1785076305,
        blue_score: 140363,
        merge_parents: 2,
      },
    ]);
    const [b] = await createTauriBridge().node.blocks(12);
    expect(b).toEqual({
      height: 140363,
      hash: "0x531a",
      txs: 0,
      proposer: "0x25b7",
      gasUsed: 0,
      gasLimit: 30000000,
      timestamp: 1785076305,
      blueScore: 140363,
      mergeParents: 2,
    });
    expect(invoke).toHaveBeenCalledWith("node_blocks", { count: 12 });
  });

  it("carries the wallet's un-read tokens through as notes, not as zero balances", async () => {
    invoke.mockResolvedValue({
      address: "0x4fAB",
      chain_id: 40204,
      rpc_url: "https://rpc.citrate.ai",
      key_store: "citrate-core-kit custody vault · OS keyring available",
      source: "vendored book",
      tokens: [{ symbol: "SALT", name: "Citrate native currency", balance: "9999464.745500887910960821", native: true, source: "eth_getBalance" }],
      notes: ["WrappedSALT at 0xaa91 did not answer: execution reverted"],
      activity_note: "No movement history: this app runs no transaction index.",
    });
    const w = await createTauriBridge().wallet.summary();
    expect(w.tokens[0].balance).toBe("9999464.745500887910960821");
    expect(w.notes).toEqual(["WrappedSALT at 0xaa91 did not answer: execution reverted"]);
    expect(w.activityNote).toContain("no transaction index");
    expect(w.chainId).toBe(40204);
  });

  it("keeps an uninitialised tenant tree's reason instead of an empty table", async () => {
    invoke.mockResolvedValue({
      rows: [],
      source: "TenantHierarchy.getNode/getChildren at 0x2dc5…",
      note: "TenantHierarchy is deployed at 0x2dc5… but holds no root node: root() is the zero word",
    });
    const t = await createTauriBridge().settings.tenancy();
    expect(t.rows).toEqual([]);
    expect(t.note).toContain("no root node");
    expect(invoke).toHaveBeenCalledWith("tenancy_tree");
  });

  it("maps a decision document, proof result included", async () => {
    invoke.mockResolvedValue({
      id: "D-90001",
      what: "repo.write",
      when: "2026-07-26 14:33:02 UTC",
      principal: "R. Ortiz",
      agent: "sbt-41",
      grant: "G-1",
      protocol: "— none deployed",
      verdict: "allow",
      hic: "2",
      reason: "allowed by grant G-1 at 2",
      model: "— not stated by the caller",
      params: "— no params committed to",
      correlation: "X-7104",
      chain_pos: "record 1 of 1",
      entry_hash: "b3:aa",
      content_hash: "b3:bb",
      chain_head: "b3:aa",
      merkle_root: "b3:cc",
      proof_len: 0,
      included: true,
      source: "the tenant's BLAKE3 evidence chain, record 0",
    });
    const d = await createTauriBridge().ledger.decision("D-90001");
    expect(d.entryHash).toBe("b3:aa");
    expect(d.contentHash).toBe("b3:bb");
    expect(d.merkleRoot).toBe("b3:cc");
    expect(d.included).toBe(true);
    expect(d.proofLen).toBe(0);
    expect(invoke).toHaveBeenCalledWith("ledger_decision", { id: "D-90001" });
  });

  it("keeps the two verify checks separate", async () => {
    // A chain that replays intact says nothing about whether THIS record is in
    // it. Folding them into one boolean is how a verify button becomes theatre.
    invoke.mockResolvedValue({
      chain_intact: true,
      included: false,
      records: 12,
      entry_hash: "b3:aa",
      merkle_root: "b3:cc",
      proof_len: 4,
    });
    const v = await createTauriBridge().ledger.verifyDecision("D-90003");
    expect(v.chainIntact).toBe(true);
    expect(v.included).toBe(false);
    expect(v.records).toBe(12);
    expect(invoke).toHaveBeenCalledWith("ledger_verify_decision", { id: "D-90003" });
  });

  it("maps the correlation timeline and keeps its source line", async () => {
    invoke.mockResolvedValue({
      events: [{ t: "14:33:02", kind: "action", text: "repo.write · allow · R. Ortiz", link: "D-90001" }],
      source: "1 record(s) carrying correlation X-7104. Pull requests are not in this timeline",
    });
    const c = await createTauriBridge().ledger.correlation("X-7104");
    expect(c.events[0].link).toBe("D-90001");
    expect(c.source).toContain("not in this timeline");
    expect(invoke).toHaveBeenCalledWith("ledger_correlation", { corr: "X-7104" });
  });

  it("reads the chain's five facts in one call so they describe one instant", async () => {
    invoke.mockResolvedValue({
      head: "b3:aa",
      merkle_root: "b3:cc",
      records: 12,
      ungoverned: 2,
      intact: true,
      tenant: "bca",
    });
    const s = await createTauriBridge().ledger.state();
    expect(s).toEqual({
      head: "b3:aa",
      merkleRoot: "b3:cc",
      records: 12,
      ungoverned: 2,
      intact: true,
      tenant: "bca",
    });
    expect(invoke).toHaveBeenCalledWith("ledger_state");
  });
});

describe("tauri adapter — rooms (QRM-S3)", () => {
  beforeEach(() => {
    invoke.mockReset();
  });

  it("maps the relay status and keeps its honesty note", async () => {
    invoke.mockResolvedValue({
      connected: true,
      relay_url: "wss://comms.citrate.ai",
      relay_domain: "comms.citrate.ai",
      address: "0xabc",
      seats: 3,
      rooms: 1,
      note: "The relay carries ciphertext and routing metadata only",
    });
    const s = await createTauriBridge().rooms.status();
    expect(s).toEqual({
      connected: true,
      relayUrl: "wss://comms.citrate.ai",
      relayDomain: "comms.citrate.ai",
      address: "0xabc",
      seats: 3,
      rooms: 1,
      note: "The relay carries ciphertext and routing metadata only",
    });
    expect(invoke).toHaveBeenCalledWith("rooms_status");
  });

  it("carries a seat's MLS key through — it is what distinguishes two seats", async () => {
    invoke.mockResolvedValue([
      { id: "R. Ortiz", name: "R. Ortiz", human: true, address: "0x11", mls_key: "a11ce0…" },
      { id: "claude-code", name: "claude-code", human: false, address: "0x22", mls_key: "c1a4de…" },
    ]);
    const r = await createTauriBridge().rooms.roster("g1");
    expect(r[0].mlsKey).toBe("a11ce0…");
    expect(r[1].human).toBe(false);
    expect(invoke).toHaveBeenCalledWith("rooms_roster", { room: "g1" });
  });

  it("never lets a transcript line claim to have been spoken", async () => {
    // There is no audio path. A `speech` kind reaching a surface would be a claim
    // this product cannot make, so the mapping collapses anything that is not
    // `system` to `text` rather than passing an unknown kind through.
    invoke.mockResolvedValue([
      { n: 0, room: "g1", kind: "system", who: "R. Ortiz", human: true, text: "room opened", t: "09:00:00" },
      { n: 1, room: "g1", kind: "speech", who: "R. Ortiz", human: true, text: "hello", t: "09:00:01" },
    ]);
    const evs = await createTauriBridge().rooms.events(0);
    expect(evs.map((e) => e.kind)).toEqual(["system", "text"]);
    expect(invoke).toHaveBeenCalledWith("rooms_events", { since: 0 });
  });

  it("sends the room, the speaking principal and the text", async () => {
    invoke.mockResolvedValue(7);
    await createTauriBridge().rooms.say("g1", "R. Ortiz", "standup starts now");
    expect(invoke).toHaveBeenCalledWith("rooms_say", {
      room: "g1",
      principal: "R. Ortiz",
      text: "standup starts now",
    });
  });

  it("opens a room with the agents it was given", async () => {
    invoke.mockResolvedValue({
      id: "g1",
      name: "Standup",
      classification: "Proprietary",
      live: true,
      members: 3,
      started: "09:00:00",
    });
    const room = await createTauriBridge().rooms.open(
      { name: "Standup", classification: "Proprietary", agents: ["claude-code", "codex"] },
      "R. Ortiz",
    );
    expect(room.members).toBe(3);
    expect(invoke).toHaveBeenCalledWith("rooms_open", {
      operator: "R. Ortiz",
      name: "Standup",
      classification: "Proprietary",
      agents: ["claude-code", "codex"],
    });
  });
});
