// =====================================================================
// citrate-quorum — sim adapter (QRM-S2D)
//
// Backs the BridgeContract with scripted prototype data (./data). Streams
// replay a timeline; interactive methods resolve after a small delay so
// surfaces exercise their real loading states. The Tauri adapter
// (bridge/tauri) mirrors this signature with real Rust commands.
// =====================================================================
import type { BridgeContract } from "../domains";
import type { RoomEvent, Decision, LogLine, Unsubscribe } from "../types";
import * as D from "./data";

const delay = <T>(ms: number, v: T): Promise<T> =>
  new Promise((r) => setTimeout(() => r(v), ms));

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
    session: { current: () => delay(120, D.SESSION) },
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
    },
    rooms: {
      list: () => delay(150, D.ROOMS),
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
