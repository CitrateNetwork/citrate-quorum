// citrate-quorum — escalation toast (QRM-S2D, made live in QRM-S4).
// Ported from design §ESCALATION TOAST. The app's one interrupt: when an agent
// is blocked, it surfaces immediately, anywhere, in amber, and never
// auto-dismisses. Charter register.
//
// It shows a REAL escalation now. Until QRM-S4 its content was the demo beat's
// scripted hermes block — a specific named agent refusing a specific action —
// which is right on the prototype adapter and was a fabricated escalation shown
// to a real operator in the packaged app (Rule 1), so it was made sim-only.
// The live feed exists now, and this reads it.
//
// REVIEW OPENS THE CEREMONY. There is deliberately no approve button here: the
// approval happens under a signature or it does not happen, and a one-click
// "Approve" on a toast would be an HIC-1 bypass wearing a convenient hat.
import { useState } from "react";

import { useCeremony } from "../ceremony/Ceremony";
import { useEscalations } from "./escalations";

export function EscalationToast({ onGo }: { onGo: (id: string) => void }) {
  const { queue, refresh } = useEscalations();
  /** Decisions this operator has waved away for now. Dismissing is not
   *  answering: the escalation stays in the queue, on the Dashboard, and in
   *  the badge. It just stops covering the screen. */
  const [dismissed, setDismissed] = useState<Set<number>>(new Set());
  const ceremony = useCeremony();

  const next = queue.find((p) => !dismissed.has(p.decision));
  if (!next) return null;

  const answer = async () => {
    await ceremony.review(next);
    refresh();
  };

  return (
    <div
      data-register="charter"
      role="alert"
      style={{ color: "var(--tx-1)", position: "fixed", right: 18, bottom: 44, width: 400, zIndex: 90, background: "#ffffff", border: "1px solid var(--warn)", borderLeft: "3px solid var(--warn)", boxShadow: "0 12px 40px rgba(14,15,12,.22)", borderRadius: "var(--r-1)" }}
    >
      <div style={{ display: "flex", alignItems: "center", gap: 10, padding: "12px 14px", borderBottom: "1px solid var(--line-1)" }}>
        <span style={{ width: 8, height: 8, borderRadius: 999, background: "var(--warn)", animation: "ccPulse 1.4s infinite" }} />
        <span className="mono" style={{ fontSize: 10, letterSpacing: ".14em", textTransform: "uppercase", color: "var(--warn)" }}>Agent blocked — needs a human</span>
        <div style={{ flex: 1 }} />
        {queue.length > 1 && (
          <span className="mono tabular" style={{ fontSize: 9.5, color: "var(--tx-3)", border: "1px solid var(--line-2)", padding: "1px 7px", borderRadius: 999 }}>
            {queue.length} waiting
          </span>
        )}
        <button
          onClick={() => setDismissed((s) => new Set(s).add(next.decision))}
          title="Dismiss — the escalation stays in the queue"
          style={{ background: "none", border: "none", cursor: "pointer", color: "var(--tx-3)", fontSize: 14, lineHeight: 1 }}
        >
          ×
        </button>
      </div>
      <div style={{ padding: "12px 14px", display: "flex", flexDirection: "column", gap: 10 }}>
        <div style={{ display: "flex", alignItems: "center", gap: 8, flexWrap: "wrap" }}>
          <span className="mono" style={{ fontSize: 10, color: "var(--z-amber)", border: "1px solid var(--z-amber)", padding: "2px 7px" }}>{next.agent}</span>
          <span className="mono" style={{ fontSize: 10, color: "var(--warn)" }}>HIC-1</span>
          <span className="mono" style={{ fontSize: 10, color: "var(--tx-3)" }}>decision #{next.decision}</span>
        </div>
        <p style={{ fontSize: 13, lineHeight: 1.5, margin: 0, color: "var(--tx-2)" }}>
          <span className="mono">{next.actionClass}</span> at {next.classification}
          {next.cost ? <> · cost <span className="mono tabular">{next.cost}</span></> : null} needs
          your approval before it runs. It has been recorded, and the agent is waiting.
        </p>
        <div style={{ display: "flex", alignItems: "center", gap: 8 }}>
          <button className="btn btn-primary btn-sm" onClick={() => void answer()}>Review &amp; sign</button>
          <button className="btn btn-ghost btn-sm" onClick={() => onGo("ledger")}>Open ledger</button>
        </div>
      </div>
    </div>
  );
}
