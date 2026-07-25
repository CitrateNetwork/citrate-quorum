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
  type Agent,
  type GovernedAction,
  type Grant,
  type GrantTerms,
  type HicLevel,
  type PendingApproval,
  type Unsubscribe,
} from "../types";
import {
  actionApprove,
  agentsKnown,
  grantIssue,
  grantRevoke,
  grantsForAgent,
  operatorGet,
  operatorSet,
  actionEvaluateAndRecord,
  actionReject,
  approvalsPending,
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

/**
 * An unwired async domain method.
 *
 * It REJECTS rather than throwing synchronously. The domain contract types
 * these as `Promise`-returning, and a synchronous throw from one is a trap: a
 * caller doing `bridge.x.y().then(...)` inside a `useEffect` never gets to
 * `.catch()`, the effect throws, and React unmounts the whole tree — the
 * packaged app white-screens instead of reporting that a domain is unavailable.
 * Rejecting keeps the failure inside the promise chain where callers handle it.
 */
const na =
  (op: string) =>
  (): Promise<never> =>
    Promise.reject(new Unavailable(op));

/** Synchronous domain methods (streams return an Unsubscribe; `node.blocks`
 *  returns an array) have no promise for the failure to live in, so these do
 *  throw. Callers of a synchronous method are expected to guard it. */
const naSync = (op: string) => (): never => {
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
      operator: () => operatorGet(),
      setOperator: (name: string) => operatorSet(name),
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
      // LIVE: records the approval, naming the human (`action_approve`).
      approve: async (decisionId: number, approver: string): Promise<GateDecision> =>
        toGate(await actionApprove(decisionId, approver)),
      // LIVE: the escalations an agent is blocked on (`approvals_pending`).
      pending: async (): Promise<PendingApproval[]> =>
        (await approvalsPending()).map((p) => ({
          decision: p.decision,
          agent: p.agent,
          principal: p.principal,
          actionClass: p.action_class,
          classification: p.classification,
          cost: p.cost,
          correlationId: p.correlation_id,
          requestedAtMs: p.requested_at_ms,
        })),
    },
    wallet: { summary: na("wallet.summary") },
    node: { peers: na("node.peers"), logs: naStream("node.logs"), blocks: naSync("node.blocks") },
    agents: {
      // LIVE: the fleet this tenant has evidence about (`agents_known`). The
      // registry-only fields (vendor, model, capsules, reputation) are simply
      // absent — the surface renders them as "—" naming their source rather
      // than inventing a vendor for an agent we have only seen act.
      list: async (): Promise<Agent[]> =>
        (await agentsKnown()).map((a) => ({
          id: a.id,
          name: a.id,
          grants: a.live_grants,
          budgetUsed: a.consumed,
          budgetCap: a.budget_units,
          decisions: a.decisions,
          ungoverned: a.ungoverned,
        })),
      // LIVE: real capability grants (`grants_for_agent`).
      grants: async (agentId: string): Promise<Grant[]> =>
        (await grantsForAgent(agentId)).map((g) => ({
          id: g.id,
          classes: g.classes,
          scope: g.scope,
          budget: `${g.consumed}/${g.budget_units}`,
          expiry:
            g.expires_at_ms >= Number.MAX_SAFE_INTEGER ? "no expiry" : String(g.expires_at_ms),
          hic: Number(g.hic),
          principal: g.principal,
          revoked: g.revoked,
        })),
      issue: (terms: GrantTerms) =>
        grantIssue({
          id: terms.id,
          agent: terms.agent,
          issued_by: terms.principal,
          principal: terms.principal,
          tenant_scope: terms.tenantScope,
          action_classes: terms.actionClasses,
          classification_ceiling: terms.classificationCeiling,
          budget_units: terms.budgetUnits,
          expires_at_ms: terms.expiresAtMs,
          hic: terms.hic,
        }),
      revoke: (agent: string, grantId: string, revokedBy: string) =>
        grantRevoke(agent, grantId, revokedBy),
    },
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
