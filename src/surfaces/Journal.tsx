// citrate-quorum — Journal surface (QRM-S2D). Charter register. Ported from
// design §JOURNAL. Journals & retros (human + agent, attributed) and the
// standup brief — what an agent will say, reviewable before the meeting. Reads
// bridge.journal.list()/brief().
import { useEffect, useState } from "react";
import { bridge } from "../bridge";
import { DomainErrorPlate, useDomain } from "../components/DomainState";
import type { StandupBrief } from "../bridge";

export function Journal() {
  const [brief, setBrief] = useState<StandupBrief | null>(null);
  useEffect(() => {
    
    bridge.journal.brief("claude-code", "m-0723").then(setBrief).catch(() => {});
  }, []);

  // Honest failure (S2D.4/§5.1): this surface's primary read is journal.list().
  // A read that cannot succeed must say so and offer a retry, not sit in a
  // loading state forever.
  const primary = useDomain(() => bridge.journal.list(), "journal.list()");
  // Derived, not mirrored (see Agents.tsx).
  const entries = primary.state.status === "ready" ? primary.state.data : [];
  if (primary.state.status === "error") {
    return (
      <div style={{ padding: 18 }}>
        <DomainErrorPlate source="journal.list()" error={primary.state.error} onRetry={primary.retry} lands="It lands in QRM-S5 (meetings + minutes)." />
      </div>
    );
  }
  return (
    <div style={{ padding: "20px 24px", display: "grid", gridTemplateColumns: "1fr 340px", gap: 16, maxWidth: 1080 }}>
      <div className="surface" style={{ display: "flex", flexDirection: "column", height: "fit-content" }}>
        <div style={{ padding: "10px 16px", borderBottom: "1px solid var(--line-1)" }}><span className="eyebrow">Journals &amp; retros — human and agent, attributed</span></div>
        {entries.map((j) => {
          const c = j.human ? "var(--accent-text)" : "var(--info)";
          return (
            <div key={j.id} style={{ display: "flex", gap: 12, padding: "12px 16px", borderBottom: "1px solid var(--line-1)" }}>
              <span className="mono tabular" style={{ fontSize: 10, color: "var(--tx-3)", width: 74, flexShrink: 0, paddingTop: 2 }}>{j.date}</span>
              <div style={{ minWidth: 0 }}>
                <div style={{ display: "flex", alignItems: "center", gap: 8 }}>
                  <span className="mono" style={{ fontSize: 9.5, color: c, border: `1px solid ${c}`, padding: "1px 7px" }}>{j.who}</span>
                  <span className="mono" style={{ fontSize: 8.5, letterSpacing: ".1em", textTransform: "uppercase", color: "var(--tx-3)" }}>{j.kind}</span>
                </div>
                <p style={{ fontSize: 13, lineHeight: 1.55, margin: "5px 0 0" }}>{j.text}</p>
              </div>
            </div>
          );
        })}
        <div className="mono" style={{ fontSize: 9.5, color: "var(--tx-3)", padding: "8px 16px" }}>journal.list() → local memory graph · agent entries are signed by their AgentSBT key</div>
      </div>
      <div className="surface" style={{ display: "flex", flexDirection: "column", height: "fit-content", borderTop: "2px solid var(--line-strong)" }}>
        <div style={{ padding: "12px 16px", borderBottom: "1px solid var(--line-1)", display: "flex", flexDirection: "column", gap: 3 }}>
          <span className="eyebrow">Standup brief — {brief?.agent ?? "…"}</span>
          <span className="mono" style={{ fontSize: 9.5, color: "var(--tx-3)" }}>what the agent will say · review before the meeting</span>
        </div>
        {brief?.sections.map(([k, v]) => (
          <div key={k} style={{ display: "flex", flexDirection: "column", gap: 2, padding: "9px 16px", borderBottom: "1px solid var(--line-1)" }}>
            <span className="mono" style={{ fontSize: 9, letterSpacing: ".1em", textTransform: "uppercase", color: "var(--tx-3)" }}>{k}</span>
            <span style={{ fontSize: 12.5 }}>{v}</span>
          </div>
        ))}
        <div style={{ padding: "10px 16px", display: "flex", gap: 8 }}><button className="btn btn-primary btn-sm">Approve for meeting</button><button className="btn btn-ghost btn-sm">Edit</button></div>
      </div>
    </div>
  );
}
