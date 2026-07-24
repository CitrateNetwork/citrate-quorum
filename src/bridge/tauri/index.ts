// =====================================================================
// citrate-quorum — Tauri adapter (QRM-S2D · STUB)
//
// Every method throws Unavailable until its domain is wired to a real Rust
// command. This is the honest not-built state (Rule 1): a packaged build
// with an unwired domain shows an honest error, never fabricated data. Each
// domain lights up in its sprint; the SHARED signing surface (config /
// custody / auth / ceremony) is already live in the kit (WP-S1.3).
// =====================================================================
import type { BridgeContract } from "../domains";
import { Unavailable, type Unsubscribe } from "../types";

const na = (op: string) => (): never => {
  throw new Unavailable(op);
};
const naStream =
  (op: string) =>
  (): Unsubscribe => {
    throw new Unavailable(op);
  };

export function createTauriBridge(): BridgeContract {
  return {
    session: { current: na("session.current") },
    wallet: { summary: na("wallet.summary") },
    node: { peers: na("node.peers"), logs: naStream("node.logs"), blocks: na("node.blocks") },
    agents: { list: na("agents.list"), grants: na("agents.grants") },
    rooms: { list: na("rooms.list"), roster: na("rooms.roster"), events: naStream("rooms.events") },
    ledger: {
      query: na("ledger.query"),
      stream: naStream("ledger.stream"),
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
