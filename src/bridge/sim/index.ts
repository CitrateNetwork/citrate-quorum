// =====================================================================
// citrate-quorum — sim adapter (QRM-S2D)
//
// Backs the BridgeContract with scripted prototype data (./data). Streams
// replay a timeline; interactive methods resolve after a small delay so
// surfaces exercise their real loading states. The Tauri adapter
// (bridge/tauri) mirrors this signature with real Rust commands.
// =====================================================================
import type { BridgeContract } from "../domains";
import type {
  RoomEvent,
  Decision,
  GateDecision,
  GovernedAction,
  LogLine,
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

/** Replay a timestamped script once; `loop` re-runs it after the last event. */
function timeline<E extends { t: number }>(script: E[], loop: boolean) {
  return (onEvent: (e: E) => void): Unsubscribe => {
    let stopped = false;
    const timers: ReturnType<typeof setTimeout>[] = [];
    const run = () =>
      script.forEach((ev) =>
        timers.push(setTimeout(() => !stopped && onEvent(ev), ev.t)),
      );
    run();
    let interval: ReturnType<typeof setInterval> | null = null;
    if (loop)
      interval = setInterval(
        () => !stopped && run(),
        script[script.length - 1].t + 6000,
      );
    return () => {
      stopped = true;
      timers.forEach(clearTimeout);
      if (interval) clearInterval(interval);
    };
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
    wallet: { summary: () => delay(150, D.WALLET) },
    node: {
      peers: () => delay(120, D.NODE_PEERS),
      logs: (onEvent: (e: LogLine) => void): Unsubscribe => {
        let i = 40;
        const iv = setInterval(() => onEvent(D.mkLog((++i * 7919) % 1000)), 2600);
        return () => clearInterval(iv);
      },
      blocks: (height: number) =>
        Array.from({ length: 12 }, (_, i) => D.mkBlock(i, height)),
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
      list: () => delay(150, D.ROOMS),
      roster: () => delay(120, D.ROSTER),
      events: timeline<RoomEvent>(D.ROOM_TIMELINE, false),
    },
    ledger: {
      query: () =>
        delay(250, Array.from({ length: 40 }, (_, i) => D.mkDecision(i))),
      stream: (onEvent: (e: Decision) => void): Unsubscribe => {
        let i = 1;
        const iv = setInterval(() => onEvent(D.mkDecision(++i)), 4200);
        return () => clearInterval(iv);
      },
      decision: () => delay(180, D.DECISION_DETAIL),
      correlation: () => delay(160, D.CORRELATION),
    },
    meetings: {
      list: () => delay(150, D.MEETINGS),
      get: () => delay(180, D.MEETING_DETAIL),
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
