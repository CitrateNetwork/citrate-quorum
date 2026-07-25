// citrate-quorum — Dashboard surface (QRM-S2D, honesty pass S2D.4).
// Ported from design/CitrateQuorum.dc.html §DASHBOARD. Instrument register.
//
// EVERY TILE STATES ITS SOURCE (Rule 11) AND SHOWS NOTHING IT CANNOT READ
// (Rule 1). The prototype shipped plausible constants here — "1,204 actions
// recorded", "62% budget burn", "1 open contradiction" — which rendered
// unchanged against a brand-new empty tenant in the packaged app. A dashboard
// that invents its own numbers is the exact failure this product exists to
// prevent, so tiles now derive from a real read or show an em dash naming the
// call that would fill them.
import { useEffect, useState } from "react";
import { bridge } from "../bridge";
import type { Decision, PendingApproval, Session } from "../bridge";
import { DomainErrorPlate, useDomain } from "../components/DomainState";
import { useCeremony } from "../ceremony/Ceremony";
import { useEscalations } from "../shell/escalations";

const VERDICT_COLOR: Record<string, string> = {
  allow: "var(--ok)",
  "require-approval": "var(--warn)",
  deny: "var(--danger)",
  ungoverned: "var(--danger)",
};
const HIC_COLOR: Record<string, string> = {
  "1": "var(--ok)",
  "2": "var(--info)",
  "3": "var(--warn)",
  X: "var(--danger)",
};

/** How often the dashboard re-reads its counts and the escalation queue. The
 *  chain is append-only and low-rate; polling is honest and simple. */
const COUNT_REFRESH_MS = 4000;

/** The risk strip's slots and the read each one waits on (Rule 11). */
const RISK_SLOTS = [
  { label: "Contradictions", source: "ContradictionLedger · QRM-S6" },
  { label: "Expiring grants", source: "agents.grants() · QRM-S4" },
  { label: "Budget", source: "agents.grants() · QRM-S4" },
  { label: "Templates", source: "governance.protocols() · QRM-S7" },
] as const;

interface Tile {
  label: string;
  /** null = no live source for this number yet; the tile shows an em dash. */
  value: string | null;
  /** What the number means when present, or which call would supply it. */
  sub: string;
  color: string;
  top: string;
}

export function Dashboard({ onGo }: { onGo: (id: string) => void }) {
  const [session, setSession] = useState<Session | null>(null);
  const [ribbon, setRibbon] = useState<Decision[]>([]);
  /** The full ledger read backs the counts; the stream keeps them moving. */
  const ledger = useDomain(() => bridge.ledger.query(), "ledger.query()");
  /**
   * The REAL ceremony queue: escalations an agent is blocked on right now.
   * Counting `require-approval` rows in the ledger was wrong — those include
   * escalations that have already been answered, so the number only ever grew.
   */
  // One poller for the whole app (`shell/escalations`): the toast, the shell
  // badge and this queue must never disagree about who is blocked.
  const { queue, refresh: refreshQueue } = useEscalations();
  const [tick, setTick] = useState(0);
  /**
   * The counts are a LIVE view, not a snapshot at mount. They were read once
   * and never again, so the packaged app sat showing "Governed 0" while an
   * agent's decision was already on the chain — stale in a way that reads
   * exactly like wrong.
   */
  const [rows, setRows] = useState<Decision[] | null>(null);
  const ceremony = useCeremony();

  useEffect(() => {
    const iv = setInterval(() => setTick((t) => t + 1), COUNT_REFRESH_MS);
    return () => clearInterval(iv);
  }, []);

  useEffect(() => {
    // Session posture is optional chrome here — its absence must not blank the
    // dashboard, so a failure just leaves the block-height note unresolved.
    Promise.resolve()
      .then(() => bridge.session.current())
      .then(setSession)
      .catch(() => setSession(null));
  }, []);

  useEffect(() => {
    if (ledger.state.status !== "ready") return;
    setRibbon(ledger.state.data.slice(0, 8));
    let unsub: (() => void) | undefined;
    try {
      unsub = bridge.ledger.stream((d) =>
        // Dedupe: a decision can arrive from both the query and the stream, and
        // the same record must never render twice.
        setRibbon((prev) => (prev.some((p) => p.id === d.id) ? prev : [d, ...prev].slice(0, 8))),
      );
    } catch {
      // No stream is survivable — the query above already populated the ribbon.
    }
    return unsub;
  }, [ledger.state]);

  useEffect(() => {
    let live = true;
    // Refreshed on the same beat as the escalation feed, so a decision recorded
    // by an agent shows up in the counts without the operator reloading.
    Promise.resolve()
      .then(() => bridge.ledger.query())
      .then((r) => live && setRows(r))
      .catch(() => {
        /* the error plate below already reports a failed read */
      });
    return () => {
      live = false;
    };
  }, [tick, ledger.state]);

  /** Answer an escalation in the one ceremony, then refresh immediately. */
  const answer = async (pa: PendingApproval) => {
    await ceremony.review(pa);
    refreshQueue();
    setTick((t) => t + 1);
  };

  const all = rows ?? (ledger.state.status === "ready" ? ledger.state.data : []);
  const ungoverned = all.filter((r) => r.verdict === "ungoverned").length;
  const pending = queue.length;
  const known = rows !== null || ledger.state.status === "ready";

  const tiles: Tile[] = [
    // Real, from the tenant's own evidence chain.
    { label: "Governed", value: known ? String(all.length) : null, sub: "decisions recorded · ledger.query()", color: "var(--tx-1)", top: "var(--accent)" },
    { label: "Pending approvals", value: String(pending), sub: "agents blocked · policy.pending()", color: pending ? "var(--warn)" : "var(--tx-1)", top: pending ? "var(--warn)" : "var(--line-2)" },
    { label: "Ungoverned", value: known ? String(ungoverned) : null, sub: "no live grant — review", color: ungoverned ? "var(--danger)" : "var(--tx-1)", top: ungoverned ? "var(--danger)" : "var(--line-2)" },
    // No live source yet. An em dash and the call that would fill it — never a
    // plausible number (Rule 1).
    { label: "Active agents", value: null, sub: "needs agents.list() · QRM-S4", color: "var(--tx-1)", top: "var(--info)" },
    { label: "Budget burn", value: null, sub: "needs agents.grants() · QRM-S4", color: "var(--tx-1)", top: "var(--info)" },
    { label: "Next meeting", value: null, sub: "needs meetings.list() · QRM-S5", color: "var(--tx-1)", top: "var(--line-2)" },
  ];

  return (
    <div style={{ padding: 18, display: "flex", flexDirection: "column", gap: 16 }}>
      {/* stat tiles */}
      <div style={{ display: "grid", gridTemplateColumns: "repeat(6,1fr)", gap: 10 }}>
        {tiles.map((st) => (
          <div key={st.label} className="surface" style={{ padding: "12px 14px", display: "flex", flexDirection: "column", gap: 6, borderTop: `2px solid ${st.top}` }}>
            <span className="mono" style={{ fontSize: 9, letterSpacing: ".13em", textTransform: "uppercase", color: "var(--tx-3)" }}>{st.label}</span>
            <span
              className="tabular"
              style={{ fontFamily: "var(--font-display)", fontWeight: 460, fontSize: 24, color: st.value === null ? "var(--tx-3)" : st.color }}
              title={st.value === null ? "no live source for this number yet" : undefined}
            >
              {st.value ?? "—"}
            </span>
            <span className="mono" style={{ fontSize: 9.5, color: "var(--tx-3)" }}>{st.sub}</span>
          </div>
        ))}
      </div>

      {/* needs you */}
      <div className="surface" style={{ display: "flex", flexDirection: "column" }}>
        <div style={{ display: "flex", alignItems: "center", gap: 10, padding: "10px 14px", borderBottom: "1px solid var(--line-1)" }}>
          <span className="eyebrow">Needs you</span>
          <span className="mono tabular" style={{ fontSize: 10, color: pending ? "var(--warn)" : "var(--tx-3)" }}>{pending}</span>
        </div>
        {ledger.state.status === "error" ? (
          <div style={{ padding: 14 }}>
            <DomainErrorPlate source="ledger.query()" error={ledger.state.error} onRetry={ledger.retry} />
          </div>
        ) : pending === 0 && known ? (
          <div style={{ padding: "22px 14px", display: "flex", alignItems: "center", gap: 12 }}>
            <span style={{ width: 22, height: 22, borderRadius: 999, background: "var(--ok-bg)", border: "1px solid var(--ok)", color: "var(--ok)", display: "inline-flex", alignItems: "center", justifyContent: "center" }}>
              <svg viewBox="0 0 24 24" width="12" height="12" fill="none" stroke="currentColor" strokeWidth="2.4" strokeLinecap="round"><path d="M5 12 L10 17 L19 8" /></svg>
            </span>
            <span style={{ fontSize: 13.5, color: "var(--tx-2)" }}>
              Nothing is waiting on you. Every agent is inside its envelope — this is the state the system is designed to hold.
            </span>
          </div>
        ) : !known ? (
          <div className="mono" style={{ padding: "18px 14px", fontSize: 11, color: "var(--tx-3)" }}>reading ledger.query()…</div>
        ) : (
          queue.map((pa) => (
            <div
              key={pa.decision}
              onClick={() => void answer(pa)}
              role="button"
              tabIndex={0}
              onKeyDown={(e) => { if (e.key === "Enter" || e.key === " ") void answer(pa); }}
              style={{ display: "flex", alignItems: "center", gap: 12, padding: "9px 14px", borderBottom: "1px solid var(--line-1)", cursor: "pointer" }}
            >
              <span className="mono" style={{ fontSize: 9, letterSpacing: ".12em", textTransform: "uppercase", color: "var(--warn)", border: "1px solid var(--warn)", padding: "2px 7px", flexShrink: 0 }}>APPROVE</span>
              <span style={{ fontSize: 13, flex: 1 }}>
                {pa.agent} · {pa.actionClass}
                {pa.cost ? ` · ${pa.cost}` : ""}
              </span>
              <span className="mono" style={{ fontSize: 10, color: "var(--tx-3)" }}>#{pa.decision}</span>
            </div>
          ))
        )}
      </div>

      {/* live decisions ribbon */}
      <div className="surface" style={{ display: "flex", flexDirection: "column" }}>
        <div style={{ display: "flex", alignItems: "center", gap: 10, padding: "10px 14px", borderBottom: "1px solid var(--line-1)" }}>
          <span className="eyebrow">Live decisions</span>
          <span style={{ width: 7, height: 7, borderRadius: 999, background: "var(--accent)", animation: "ccPulse 1.6s infinite" }} />
          <div style={{ flex: 1 }} />
          <a href="#/ledger" onClick={(e) => { e.preventDefault(); onGo("ledger"); }} className="mono" style={{ fontSize: 10, letterSpacing: ".1em", textTransform: "uppercase" }}>Open ledger →</a>
        </div>
        <div className="mono" style={{ display: "grid", gridTemplateColumns: "70px 90px 110px 120px 1fr 60px 78px", gap: 8, padding: "6px 14px", fontSize: 9, letterSpacing: ".12em", textTransform: "uppercase", color: "var(--tx-3)", borderBottom: "1px solid var(--line-1)" }}>
          <span>Time</span><span>Decision</span><span>Principal</span><span>Agent</span><span>Action class</span><span>HIC</span><span>Verdict</span>
        </div>
        {ribbon.map((r) => (
          <div key={r.id} className="mono tabular" style={{ display: "grid", gridTemplateColumns: "70px 90px 110px 120px 1fr 60px 78px", gap: 8, padding: "8px 14px", fontSize: 11, borderBottom: "1px solid var(--line-1)", background: r.verdict === "ungoverned" ? "var(--danger-bg)" : "transparent" }}>
            <span style={{ color: "var(--tx-3)" }}>{r.time}</span>
            <span>{r.id}</span>
            <span style={{ color: "var(--tx-2)" }}>{r.principal}</span>
            <span>{r.agent}</span>
            <span style={{ color: "var(--tx-2)" }}>{r.cls}</span>
            <span style={{ color: HIC_COLOR[r.hic] ?? "var(--tx-2)" }}>{r.hic === "X" ? "HIC-X" : `HIC-${r.hic}`}</span>
            <span style={{ color: VERDICT_COLOR[r.verdict] ?? "var(--tx-2)" }}>{r.verdict}</span>
          </div>
        ))}
        <div className="mono" style={{ fontSize: 9.5, letterSpacing: ".08em", color: "var(--tx-3)", padding: "7px 14px" }}>
          read from ledger.stream() → AgentDecisionRegistryV2 → chain 40204 @ block {session ? session.chain.height.toLocaleString("en-US") : "…"}
        </div>
      </div>

      {/* risk strip — the four signals it will carry, each naming the read that
          will produce it. Empty until those land; a risk board that invents its
          own risks is worse than no risk board. */}
      <div style={{ display: "grid", gridTemplateColumns: "repeat(4,1fr)", gap: 10 }}>
        {RISK_SLOTS.map((rk) => (
          <div key={rk.label} className="surface" style={{ padding: "12px 14px", display: "flex", flexDirection: "column", gap: 6, borderTop: "2px solid var(--line-2)" }}>
            <span className="mono" style={{ fontSize: 9, letterSpacing: ".13em", textTransform: "uppercase", color: "var(--tx-3)" }}>{rk.label}</span>
            <span style={{ fontSize: 12.5, lineHeight: 1.45, color: "var(--tx-3)" }}>No signal yet</span>
            <span className="mono" style={{ fontSize: 9, color: "var(--tx-3)" }}>{rk.source}</span>
          </div>
        ))}
      </div>
    </div>
  );
}
