// =====================================================================
// citrate-quorum — Tauri adapter (QRM-S2 · partially LIVE)
//
// Domains with a real Rust backend today call it (Rule 1: real data or an
// honest error, never fabricated). LIVE here:
//
//  · policy  — the gate. Evaluates against the tenant's live grants and
//              appends the decision to the tenant's BLAKE3 evidence chain.
//  · ledger  — reads that same chain back.
//  · session — the active tenant scope (`current()` still needs the IdP).
//
// Every other domain throws Unavailable until its sprint wires it (rooms need
// the comms relay, governance needs the chain, calendar needs OAuth, …). The
// SHARED signing surface (config / custody / auth / ceremony) is already live
// in the kit (WP-S1.3).
//
// The tenant scope lives in the Rust backend, not here: no call below names a
// tenant, and a command with no scope established fails closed with an honest
// error rather than guessing one (Rule 6).
// =====================================================================
import type { BridgeContract } from "../domains";
import {
  Unavailable,
  type Decision,
  type GateDecision,
  type GovernedAction,
  type HicLevel,
  type Unsubscribe,
} from "../types";
import {
  actionEvaluateAndRecord,
  actionReject,
  ledgerRecords,
  tenantActive,
  tenantSet,
  type DecisionResult,
} from "./commands";

/** The Rust DTO is snake_case; the bridge contract is camelCase. */
const toGate = (d: DecisionResult): GateDecision => ({
  decisionId: d.decision_id,
  verdict: d.verdict,
  hic: d.hic as HicLevel,
  grantId: d.grant_id,
  reason: d.reason,
  chainHead: d.chain_head,
  ungoverned: d.ungoverned,
});

const na = (op: string) => (): never => {
  throw new Unavailable(op);
};
const naStream =
  (op: string) =>
  (): Unsubscribe => {
    throw new Unavailable(op);
  };

/** Poll interval for the streaming ledger ribbon (ms). The chain is append-only
 *  and low-rate; polling is honest and simple until a push channel lands. */
const LEDGER_POLL_MS = 4000;

export function createTauriBridge(): BridgeContract {
  return {
    session: {
      current: na("session.current"),
      // LIVE: backend-owned tenant scope.
      activeTenant: () => tenantActive(),
      setActiveTenant: (tenant: string) => tenantSet(tenant),
    },
    policy: {
      // LIVE: quorum-policy evaluate → quorum-audit DecisionRecord → the
      // tenant's hash chain. `action_evaluate_and_record` in backend.rs.
      evaluate: async (action: GovernedAction): Promise<GateDecision> =>
        toGate(
          await actionEvaluateAndRecord({
            agent: action.agent,
            principal: action.principal ?? null,
            class: action.actionClass,
            classification: action.classification,
            cost: action.cost ?? 0,
            hic1_cost_threshold: action.hic1CostThreshold ?? 0,
            mandatory_hic1: action.mandatoryHic1 ?? false,
            // Stays empty until the agent adapters (S4) supply real tool
            // parameters to commit to. An empty hash claims nothing.
            params_hash: "",
            model_id: action.modelId ?? "",
            correlation_id: action.correlationId ?? "",
          }),
        ),
      // LIVE: refunds the charge and records the refusal (`action_reject`).
      reject: async (decisionId: number): Promise<GateDecision> =>
        toGate(await actionReject(decisionId)),
    },
    wallet: { summary: na("wallet.summary") },
    node: { peers: na("node.peers"), logs: naStream("node.logs"), blocks: na("node.blocks") },
    agents: { list: na("agents.list"), grants: na("agents.grants") },
    rooms: { list: na("rooms.list"), roster: na("rooms.roster"), events: naStream("rooms.events") },
    ledger: {
      // LIVE: the real per-tenant hash chain.
      query: () => ledgerRecords(),
      // LIVE: poll the chain, emit rows appended since the last poll.
      stream: (onEvent: (e: Decision) => void): Unsubscribe => {
        let seen = 0;
        let stopped = false;
        const tick = async (): Promise<void> => {
          if (stopped) return;
          try {
            const rows = await ledgerRecords();
            for (let i = seen; i < rows.length; i++) onEvent(rows[i]);
            seen = rows.length;
          } catch {
            // A transient backend error (or no tenant scope yet) must not kill
            // the stream; the next tick retries. Errors surface through query().
          }
        };
        void tick();
        const handle = setInterval(() => void tick(), LEDGER_POLL_MS);
        return () => {
          stopped = true;
          clearInterval(handle);
        };
      },
      // Not yet: single-decision detail + correlation need the on-chain anchor
      // proof + the meeting/PR join, which land with the chain and rooms wiring.
      decision: na("ledger.decision"),
      correlation: na("ledger.correlation"),
    },
    meetings: { list: na("meetings.list"), get: na("meetings.get") },
    governance: {
      protocols: na("governance.protocols"),
      clauses: na("governance.clauses"),
      simulate: na("governance.simulate"),
      ingest: na("governance.ingest"),
      interview: na("governance.interview"),
    },
    journal: { list: na("journal.list"), brief: na("journal.brief") },
    calendar: { accounts: na("calendar.accounts"), events: na("calendar.events") },
    repos: { list: na("repos.list"), prs: na("repos.prs"), peek: na("repos.peek") },
    settings: { tenancy: na("settings.tenancy") },
  };
}
