// =====================================================================
// citrate-quorum — Tauri adapter (QRM-S2 · partially LIVE)
//
// Domains with a real Rust backend today call it (Rule 1: real data or an
// honest error, never fabricated). The audit LEDGER is live — it reads the
// per-tenant BLAKE3 HashChain the policy→audit pipeline writes. Every other
// domain still throws Unavailable until its sprint wires it (rooms need the
// comms relay, governance needs the chain, calendar needs OAuth, …). The
// SHARED signing surface (config / custody / auth / ceremony) is already live
// in the kit (WP-S1.3).
//
// Active tenant: the ledger is tenant-scoped (Rule 6). Until the session flow
// resolves a tenant (needs the live IdP), it is unset and the ledger fails
// closed with an honest error rather than guessing a tenant.
// =====================================================================
import type { BridgeContract } from "../domains";
import { Failed, Unavailable, type Decision, type Unsubscribe } from "../types";
import { ledgerRecords } from "./commands";

const na = (op: string) => (): never => {
  throw new Unavailable(op);
};
const naStream =
  (op: string) =>
  (): Unsubscribe => {
    throw new Unavailable(op);
  };

// The tenant whose data the packaged app is currently showing. Set by the
// session flow once it resolves (session_resolve). Null until then.
let activeTenant: string | null = null;
/** Set the active tenant scope. Called by the session/onboarding flow. */
export function setActiveTenant(tenant: string | null): void {
  activeTenant = tenant;
}
function requireTenant(op: string): string {
  if (!activeTenant) {
    throw new Failed(op, "no active tenant — sign in to resolve a tenant scope first");
  }
  return activeTenant;
}

/** Poll interval for the streaming ledger ribbon (ms). The chain is append-only
 *  and low-rate; polling is honest and simple until a push channel lands. */
const LEDGER_POLL_MS = 4000;

export function createTauriBridge(): BridgeContract {
  return {
    session: { current: na("session.current") },
    wallet: { summary: na("wallet.summary") },
    node: { peers: na("node.peers"), logs: naStream("node.logs"), blocks: na("node.blocks") },
    agents: { list: na("agents.list"), grants: na("agents.grants") },
    rooms: { list: na("rooms.list"), roster: na("rooms.roster"), events: naStream("rooms.events") },
    ledger: {
      // LIVE: the real per-tenant hash chain.
      query: () => ledgerRecords(requireTenant("ledger.query")),
      // LIVE: poll the chain, emit rows appended since the last poll.
      stream: (onEvent: (e: Decision) => void): Unsubscribe => {
        const tenant = requireTenant("ledger.stream");
        let seen = 0;
        let stopped = false;
        const tick = async (): Promise<void> => {
          if (stopped) return;
          try {
            const rows = await ledgerRecords(tenant);
            for (let i = seen; i < rows.length; i++) onEvent(rows[i]);
            seen = rows.length;
          } catch {
            // A transient backend error must not kill the stream; the next
            // tick retries. (Errors surface through query() for the UI.)
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
