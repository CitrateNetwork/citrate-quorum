// =====================================================================
// citrate-quorum — sim adapter (QRM-S2D)
//
// Backs the BridgeContract with scripted prototype data (./data). Streams
// replay a timeline; interactive methods resolve after a small delay so
// surfaces exercise their real loading states. The Tauri adapter
// (bridge/tauri) mirrors this signature with real Rust commands.
// =====================================================================
import type { BridgeContract } from "../domains";
import { Unavailable } from "../types";
import type {
  Decision,
  GateDecision,
  GovernedAction,
  PendingApproval,
  Unsubscribe,
} from "../types";
import * as D from "./data";

const delay = <T>(ms: number, v: T): Promise<T> =>
  new Promise((r) => setTimeout(() => r(v), ms));

/**
 * The prototype's tenant scope. Null until the onboarding flow establishes it,
 * mirroring the Tauri adapter (where the scope is backend state) so the same
 * flow runs in both modes.
 */
let simTenant: string | null = null;
/** The prototype signs in as its scripted user. */
let simOperator: string | null = D.SESSION.user.name;

/**
 * The SCRIPTED policy gate — the sim's stand-in for `quorum-policy`.
 *
 * This is prototype data, not the engine: the real verdicts come from
 * `evaluate()` in `quorum-policy` via the Tauri adapter. The rules here are the
 * narrowest set that keeps the demo beats coherent — an agent with no scripted
 * grant is ungoverned, cost over its threshold escalates to HIC-1, and anything
 * mandatory-HIC-1 escalates regardless.
 */
const SIM_GRANTED: Record<string, { grantId: string; hic: "1" | "2" | "3" }> = {
  "claude-code": { grantId: "G-2291", hic: "2" },
  "sbt-41": { grantId: "G-2291", hic: "2" },
  codex: { grantId: "G-2288", hic: "2" },
  hermes: { grantId: "G-2301", hic: "3" },
  user: { grantId: "G-1000", hic: "1" },
};
/** Escalations the scripted gate has raised and no one has answered yet. */
const simPending = new Map<number, PendingApproval>();
let simChainSeq = 0x4a71;
let simDecisionSeq = 0;
/** Decisions the scripted gate has issued, so `reject` has something to name. */
const simIssued = new Map<number, GateDecision>();

function simGate(a: GovernedAction): GateDecision {
  const head = `0x${(simChainSeq++ * 2654435761).toString(16).padStart(16, "0").slice(0, 16)}…`;
  const decisionId = simDecisionSeq++;
  const g = SIM_GRANTED[a.agent];
  if (!g) {
    return {
      decisionId,
      verdict: "ungoverned",
      hic: "X",
      grantId: null,
      reason: "no live grant covers this agent",
      chainHead: head,
      ungoverned: true,
    };
  }
  const overCost = (a.hic1CostThreshold ?? 0) > 0 && (a.cost ?? 0) > (a.hic1CostThreshold ?? 0);
  if (a.mandatoryHic1 || overCost) {
    return {
      decisionId,
      verdict: "require-approval",
      hic: "1",
      grantId: g.grantId,
      reason: overCost ? "cost over the grant's HIC-1 threshold" : "action class always requires HIC-1",
      chainHead: head,
      ungoverned: false,
    };
  }
  return {
    decisionId,
    verdict: "allow",
    hic: g.hic,
    grantId: g.grantId,
    reason: "within grant scope, budget, and ceiling",
    chainHead: head,
    ungoverned: false,
  };
}

export function createSimBridge(): BridgeContract {
  return {
    session: {
      current: () => delay(120, D.SESSION),
      activeTenant: () => delay(40, simTenant),
      setActiveTenant: (tenant: string) => {
        simTenant = tenant;
        return delay(40, undefined);
      },
      operator: () => delay(40, simOperator),
      setOperator: (name: string) => {
        simOperator = name;
        return delay(40, undefined);
      },
    },
    policy: {
      evaluate: (a: GovernedAction) => {
        const d = simGate(a);
        simIssued.set(d.decisionId, d);
        if (d.verdict === "require-approval") {
          simPending.set(d.decisionId, {
            decision: d.decisionId,
            agent: a.agent,
            principal: a.principal ?? null,
            actionClass: a.actionClass,
            classification: a.classification,
            cost: a.cost ?? 0,
            correlationId: a.correlationId ?? "",
            requestedAtMs: 0,
          });
        }
        return delay(220, d);
      },
      // The prototype has no budget to refund; it records the refusal so the
      // shape of the flow matches the real adapter.
      reject: (decisionId: number) =>
        delay(180, {
          ...(simIssued.get(decisionId) ?? simGate({ actionClass: "unknown", classification: "Public", agent: "user" })),
          decisionId: simDecisionSeq++,
          verdict: "rejected" as const,
          hic: "1" as const,
          reason: "RC-300 refused by the human it was escalated to",
        }).then((r) => {
          simPending.delete(decisionId);
          return r;
        }),
      approve: (decisionId: number, approver: string) =>
        delay(180, {
          ...(simIssued.get(decisionId) ??
            simGate({ actionClass: "unknown", classification: "Public", agent: "user" })),
          decisionId: simDecisionSeq++,
          verdict: "approved" as const,
          hic: "1" as const,
          reason: `RC-301 approved by ${approver}`,
        }).then((r) => {
          simPending.delete(decisionId);
          return r;
        }),
      pending: () => delay(120, [...simPending.values()]),
    },
    wallet: {
      summary: () => delay(150, D.WALLET),
      identity: () =>
        delay(60, { exists: false, address: null, reason: "the sim has no custody vault" }),
      // Deliberately refuses. A sim that handed back a plausible 24-word phrase
      // would eventually be saved by someone as if it protected something.
      createIdentity: () =>
        Promise.reject(new Unavailable("wallet.createIdentity (the sim has no vault, and a fake recovery phrase is worse than none)")),
      importIdentity: () => Promise.reject(new Unavailable("wallet.importIdentity (the sim has no vault)")),
    },
    node: {
      status: () => delay(120, D.NODE_STATUS),
      blocks: (count: number) =>
        delay(150, Array.from({ length: count }, (_, i) => D.mkBlock(i, D.NODE_STATUS.height))),
      activity: () => delay(100, Array.from({ length: 12 }, (_, i) => D.mkActivity(i))),
    },
    agents: {
      list: () => delay(200, D.AGENTS),
      grants: (id: string) => delay(150, D.GRANTS[id] ?? []),
      // The prototype has no store to write to; the flow's shape is what
      // matters here, and the real adapter is what proves it.
      issue: () => delay(150, undefined),
      revoke: () => delay(150, true),
    },
    rooms: {
      status: () => delay(80, D.ROOMS_STATUS),
      // The sim has no relay and no vault, so there is nothing to sign a login
      // with. Refusing beats handing back a fake ceremony id.
      connectIntent: () => Promise.reject(new Unavailable("rooms.connectIntent (the sim has no relay)")),
      connectComplete: () => Promise.reject(new Unavailable("rooms.connectComplete (the sim has no relay)")),
      list: () => delay(150, D.ROOMS),
      // The sim has no relay and no MLS group. Opening a room that exists only
      // in a fixture would teach an operator the wrong thing about what this
      // product does, so it says so instead.
      open: () => Promise.reject(new Unavailable("rooms.open (the sim has no relay)")),
      roster: () => delay(120, D.ROSTER),
      say: () => Promise.reject(new Unavailable("rooms.say (the sim has no relay)")),
      events: (since: number) => delay(120, D.ROOM_TIMELINE.filter((e) => e.n >= since)),
      leave: () => delay(80, undefined),
    },
    ledger: {
      query: () =>
        delay(250, Array.from({ length: 40 }, (_, i) => D.mkDecision(i))),
      state: () => delay(80, D.LEDGER_STATE),
      stream: (onEvent: (e: Decision) => void): Unsubscribe => {
        let i = 1;
        const iv = setInterval(() => onEvent(D.mkDecision(++i)), 4200);
        return () => clearInterval(iv);
      },
      decision: () => delay(180, D.DECISION_DETAIL),
      verifyDecision: () => delay(400, D.VERIFY_DECISION),
      correlation: () => delay(160, D.CORRELATION),
    },
    meetings: {
      list: () => delay(150, D.MEETINGS),
      get: () => delay(180, D.MEETING_DETAIL),
      // The sim has no evidence chain to hash, so it echoes the fixture's own
      // agenda hash rather than inventing a content hash that looks real. The
      // honest tauri path computes a true BLAKE3 over the minutes.
      contentHash: () => delay(60, `sim — no content hash (fixture ${D.MEETING_DETAIL.agendaHash})`),
      anchorState: () => delay(80, "sim — no chain is consulted in this mode"),
      ratify: () => delay(120, undefined as void),
      // The sim's register is a frozen fixture. Rather than pretend to
      // schedule into it, these say plainly that only the real backend keeps
      // a meeting record — a scheduled meeting that vanishes on reload would
      // teach an operator the wrong thing about what this product stores.
      schedule: () => Promise.reject(new Unavailable("meetings.schedule (sim has no meeting store)")),
      admit: () => Promise.reject(new Unavailable("meetings.admit (sim has no meeting store)")),
      open: () => Promise.reject(new Unavailable("meetings.open (sim has no meeting store)")),
      close: () => Promise.reject(new Unavailable("meetings.close (sim has no meeting store)")),
    },
    governance: {
      protocols: () => delay(150, D.PROTOCOLS),
      clauses: () => delay(150, D.SPEC_CLAUSES),
      simulate: () => delay(1400, D.SIMULATION),
      ingest: () => delay(150, D.INGEST_FILES),
      interview: () => delay(150, D.INTERVIEW),
    },
    journal: {
      list: () => delay(150, D.JOURNAL),
      brief: () => delay(150, D.BRIEF),
    },
    calendar: {
      accounts: () => delay(120, D.CAL_ACCOUNTS),
      events: () => delay(150, D.CAL_EVENTS),
    },
    repos: {
      list: () => delay(150, D.REPOS),
      prs: () => delay(150, D.PRS),
      peek: () => delay(140, D.PEEK),
    },
    settings: { tenancy: () => delay(150, D.TENANCY) },
  };
}
