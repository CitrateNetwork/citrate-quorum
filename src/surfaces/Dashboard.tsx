// citrate-quorum — Dashboard surface (QRM-S2D). Instrument register.
// Ported from design/CitrateQuorum.dc.html §DASHBOARD. Reads real (sim) data
// through the bridge: session posture, the streaming decision ribbon, the
// "needs you" queue, and the risk strip. Every number states its source
// (Rule 11) via the ribbon footer.
import { useEffect, useState } from "react";
import { bridge } from "../bridge";
import type { Decision, Session } from "../bridge";

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

interface Tile {
  label: string;
  value: string;
  sub: string;
  color: string;
  top: string;
}

export function Dashboard({ onGo }: { onGo: (id: string) => void }) {
  const [session, setSession] = useState<Session | null>(null);
  const [ribbon, setRibbon] = useState<Decision[]>([]);

  useEffect(() => {
    bridge.session.current().then(setSession);
    bridge.ledger.query().then((rows) => setRibbon(rows.slice(0, 8)));
    // Live stream: prepend new decisions, cap the visible window.
    const unsub = bridge.ledger.stream((d) =>
      setRibbon((prev) => [d, ...prev].slice(0, 8)),
    );
    return unsub;
  }, []);

  const ungoverned = ribbon.filter((r) => r.verdict === "ungoverned").length;
  const pending = ribbon.filter((r) => r.verdict === "require-approval").length;

  const tiles: Tile[] = [
    { label: "Governed today", value: "1,204", sub: "actions recorded", color: "var(--tx-1)", top: "var(--accent)" },
    { label: "Pending approvals", value: String(pending), sub: "in your ceremony queue", color: pending ? "var(--warn)" : "var(--tx-1)", top: pending ? "var(--warn)" : "var(--line-2)" },
    { label: "Ungoverned", value: String(ungoverned), sub: "no live grant — review", color: ungoverned ? "var(--danger)" : "var(--tx-1)", top: ungoverned ? "var(--danger)" : "var(--line-2)" },
    { label: "Active agents", value: "4", sub: "1 probation · 1 quarantined", color: "var(--tx-1)", top: "var(--info)" },
    { label: "Budget burn", value: "62%", sub: "of weekly envelope", color: "var(--tx-1)", top: "var(--info)" },
    { label: "Next meeting", value: "09:00", sub: "Standup — Line-4", color: "var(--tx-1)", top: "var(--line-2)" },
  ];

  const risks = [
    { label: "Contradictions", text: "1 open — Line-4 coverage (84.2% vs 78.9%)", color: "var(--warn)" },
    { label: "Expiring grants", text: "3 grants expire within 7 days", color: "var(--warn)" },
    { label: "Budget", text: "codex at 90% of its weekly envelope", color: "var(--warn)" },
    { label: "Deprecated template", text: "PRT-001 uses a deprecated template", color: "var(--danger)" },
  ];

  return (
    <div style={{ padding: 18, display: "flex", flexDirection: "column", gap: 16 }}>
      {/* stat tiles */}
      <div style={{ display: "grid", gridTemplateColumns: "repeat(6,1fr)", gap: 10 }}>
        {tiles.map((st) => (
          <div key={st.label} className="surface" style={{ padding: "12px 14px", display: "flex", flexDirection: "column", gap: 6, borderTop: `2px solid ${st.top}` }}>
            <span className="mono" style={{ fontSize: 9, letterSpacing: ".13em", textTransform: "uppercase", color: "var(--tx-3)" }}>{st.label}</span>
            <span className="tabular" style={{ fontFamily: "var(--font-display)", fontWeight: 460, fontSize: 24, color: st.color }}>{st.value}</span>
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
        {pending === 0 ? (
          <div style={{ padding: "22px 14px", display: "flex", alignItems: "center", gap: 12 }}>
            <span style={{ width: 22, height: 22, borderRadius: 999, background: "var(--ok-bg)", border: "1px solid var(--ok)", color: "var(--ok)", display: "inline-flex", alignItems: "center", justifyContent: "center" }}>
              <svg viewBox="0 0 24 24" width="12" height="12" fill="none" stroke="currentColor" strokeWidth="2.4" strokeLinecap="round"><path d="M5 12 L10 17 L19 8" /></svg>
            </span>
            <span style={{ fontSize: 13.5, color: "var(--tx-2)" }}>
              Nothing is waiting on you. Every agent is inside its envelope — this is the state the system is designed to hold.
            </span>
          </div>
        ) : (
          ribbon.filter((r) => r.verdict === "require-approval").map((r) => (
            <div key={r.id} onClick={() => onGo("ledger")} style={{ display: "flex", alignItems: "center", gap: 12, padding: "9px 14px", borderBottom: "1px solid var(--line-1)", cursor: "pointer" }}>
              <span className="mono" style={{ fontSize: 9, letterSpacing: ".12em", textTransform: "uppercase", color: "var(--warn)", border: "1px solid var(--warn)", padding: "2px 7px", flexShrink: 0 }}>APPROVE</span>
              <span style={{ fontSize: 13, flex: 1 }}>{r.agent} · {r.cls}</span>
              <span className="mono" style={{ fontSize: 10, color: "var(--tx-3)" }}>{r.time}</span>
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

      {/* risk strip */}
      <div style={{ display: "grid", gridTemplateColumns: "repeat(4,1fr)", gap: 10 }}>
        {risks.map((rk) => (
          <div key={rk.label} className="surface" style={{ padding: "12px 14px", display: "flex", flexDirection: "column", gap: 6, borderTop: `2px solid ${rk.color}` }}>
            <span className="mono" style={{ fontSize: 9, letterSpacing: ".13em", textTransform: "uppercase", color: rk.color }}>{rk.label}</span>
            <span style={{ fontSize: 12.5, lineHeight: 1.45, color: "var(--tx-2)" }}>{rk.text}</span>
          </div>
        ))}
      </div>
    </div>
  );
}
