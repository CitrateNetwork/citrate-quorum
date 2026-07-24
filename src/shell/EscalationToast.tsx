// citrate-quorum — escalation toast (QRM-S2D). Ported from design
// §ESCALATION TOAST. The app's one interrupt: when an agent is blocked, it
// surfaces immediately, anywhere, in amber, and never auto-dismisses. Charter
// register. Fires ~14s after mount (the standup's hermes calendar.write block).
import { useEffect, useState } from "react";

export function EscalationToast({ afterSeconds = 14, onGo }: { afterSeconds?: number; onGo: (id: string) => void }) {
  const [open, setOpen] = useState(false);
  useEffect(() => {
    const t = setTimeout(() => setOpen(true), afterSeconds * 1000);
    return () => clearTimeout(t);
  }, [afterSeconds]);
  if (!open) return null;
  return (
    <div data-register="charter" style={{ color: "var(--tx-1)", position: "fixed", right: 18, bottom: 44, width: 400, zIndex: 90, background: "#ffffff", border: "1px solid var(--warn)", borderLeft: "3px solid var(--warn)", boxShadow: "0 12px 40px rgba(14,15,12,.22)", borderRadius: "var(--r-1)" }}>
      <div style={{ display: "flex", alignItems: "center", gap: 10, padding: "12px 14px", borderBottom: "1px solid var(--line-1)" }}>
        <span style={{ width: 8, height: 8, borderRadius: 999, background: "var(--warn)", animation: "ccPulse 1.4s infinite" }} />
        <span className="mono" style={{ fontSize: 10, letterSpacing: ".14em", textTransform: "uppercase", color: "var(--warn)" }}>Agent blocked — needs a human</span>
        <div style={{ flex: 1 }} />
        <button onClick={() => setOpen(false)} style={{ background: "none", border: "none", cursor: "pointer", color: "var(--tx-3)", fontSize: 14, lineHeight: 1 }}>×</button>
      </div>
      <div style={{ padding: "12px 14px", display: "flex", flexDirection: "column", gap: 10 }}>
        <div style={{ display: "flex", alignItems: "center", gap: 8 }}>
          <span className="mono" style={{ fontSize: 10, color: "var(--z-amber)", border: "1px solid var(--z-amber)", padding: "2px 7px" }}>hermes · Nous · AgentSBT #47</span>
          <span className="mono" style={{ fontSize: 9.5, color: "var(--info)" }}>HIC-2</span>
        </div>
        <p style={{ fontSize: 13, lineHeight: 1.5, margin: 0, color: "var(--tx-2)" }}><span className="mono">calendar.write</span> refused — no live grant carries <span className="mono">schedule.write</span>. hermes asks for a bounded grant to book the probation review.</p>
        <div style={{ display: "flex", gap: 8 }}>
          <button className="btn btn-primary btn-sm" onClick={() => { onGo("rooms"); setOpen(false); }}>Open in room</button>
          <button className="btn btn-ghost btn-sm" onClick={() => { onGo("agents"); setOpen(false); }}>Review grant</button>
        </div>
      </div>
    </div>
  );
}
