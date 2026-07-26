// citrate-quorum — command palette (QRM-S2D, ⌘K). Ported from design
// §COMMAND PALETTE. Jump to a surface/room/protocol/agent/decision, or run a
// command. Charter register. Destructive commands are confirmed, never
// one-keystroke.
import { useEffect, useMemo, useRef, useState } from "react";
import { NAV_ITEMS } from "./nav";

interface Entry { kind: string; label: string; color: string; go: string; danger?: boolean }

export function CommandPalette({ open, onClose, onGo }: { open: boolean; onClose: () => void; onGo: (id: string) => void }) {
  const [q, setQ] = useState("");
  const inputRef = useRef<HTMLInputElement>(null);
  // Deliberate: the palette must open empty, not showing the previous query.
  // eslint-disable-next-line react-hooks/set-state-in-effect -- see above
  useEffect(() => { if (open) { setQ(""); setTimeout(() => inputRef.current?.focus(), 0); } }, [open]);

  const entries = useMemo<Entry[]>(() => [
    ...NAV_ITEMS.map((n) => ({ kind: "go", label: n.label, color: "var(--info)", go: n.id })),
    { kind: "room", label: "Weekly Standup — Line-4 (live)", color: "var(--accent-text)", go: "rooms" },
    { kind: "protocol", label: "PRT-004 — Line-4 Operating Envelope", color: "var(--z-magenta)", go: "governance" },
    { kind: "agent", label: "claude-code · AgentSBT #41", color: "var(--z-cyan)", go: "agents" },
    { kind: "decision", label: "D-88412 — vote cast, PRT-004 A2", color: "var(--warn)", go: "ledger" },
    { kind: "command", label: "Revoke all — for an agent", color: "var(--danger)", go: "agents", danger: true },
    { kind: "command", label: "Start a meeting", color: "var(--ok)", go: "meetings" },
  ], []);

  const results = entries.filter((e) => e.label.toLowerCase().includes(q.toLowerCase()) || e.kind.includes(q.toLowerCase()));

  if (!open) return null;
  return (
    <div onClick={onClose} style={{ position: "fixed", inset: 0, background: "rgba(14,15,12,.35)", zIndex: 110, display: "flex", justifyContent: "center", paddingTop: "12vh" }}>
      <div data-register="charter" onClick={(e) => e.stopPropagation()} style={{ color: "var(--tx-1)", width: 560, height: "fit-content", maxHeight: "60vh", background: "#ffffff", borderRadius: "var(--r-2)", boxShadow: "0 24px 64px rgba(14,15,12,.3)", display: "flex", flexDirection: "column", overflow: "hidden" }}>
        <input ref={inputRef} className="mono" value={q} onChange={(e) => setQ(e.target.value)} placeholder="Jump to a room, protocol, agent, decision — or run a command" style={{ border: "none", outline: "none", padding: "14px 16px", fontSize: 13, borderBottom: "1px solid var(--line-1)", background: "transparent", color: "var(--tx-1)" }} />
        <div style={{ overflow: "auto" }}>
          {results.map((r, i) => (
            <div key={i} onClick={() => { onGo(r.go); onClose(); }} style={{ display: "flex", alignItems: "center", gap: 10, padding: "9px 16px", cursor: "pointer", borderBottom: "1px solid var(--line-1)" }}>
              <span className="mono" style={{ fontSize: 9, letterSpacing: ".12em", textTransform: "uppercase", color: r.color, border: `1px solid ${r.color}`, padding: "1px 6px", flexShrink: 0 }}>{r.kind}</span>
              <span style={{ fontSize: 13, flex: 1 }}>{r.label}</span>
              {r.danger && <span className="mono" style={{ fontSize: 9, color: "var(--danger)" }}>CONFIRMED ACTION</span>}
            </div>
          ))}
        </div>
      </div>
    </div>
  );
}
