// citrate-quorum — Calendar surface (QRM-S2D). Charter register. Ported from
// design §CALENDAR. Month grid (governed meetings distinct from mirrored
// external events), connected accounts with truthful read-only/two-way badges,
// a governed-field sync conflict, and scheduling. Reads bridge.calendar.*.
import { useEffect, useMemo, useState } from "react";
import { bridge } from "../bridge";
import type { CalAccount, CalEvent } from "../bridge";

const CLS_COLOR: Record<string, string> = { Public: "var(--z-silver)", Proprietary: "var(--info)", CUI: "var(--warn)", ITAR: "var(--danger)" };
const DOWS = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"];

export function Calendar() {
  const [accounts, setAccounts] = useState<CalAccount[]>([]);
  const [events, setEvents] = useState<CalEvent[]>([]);
  useEffect(() => { bridge.calendar.accounts().then(setAccounts); bridge.calendar.events().then(setEvents); }, []);

  // July 2026 starts on a Wednesday (offset 3); 31 days.
  const cells = useMemo(() => {
    const byDay = new Map<number, CalEvent[]>();
    events.forEach((e) => { const a = byDay.get(e.d) ?? []; a.push(e); byDay.set(e.d, a); });
    const out: { d: number | null; evs: CalEvent[] }[] = [];
    for (let i = 0; i < 3; i++) out.push({ d: null, evs: [] });
    for (let d = 1; d <= 31; d++) out.push({ d, evs: byDay.get(d) ?? [] });
    while (out.length % 7 !== 0) out.push({ d: null, evs: [] });
    return out;
  }, [events]);

  return (
    <div style={{ padding: "20px 24px", display: "grid", gridTemplateColumns: "1fr 300px", gap: 16 }}>
      <div className="surface" style={{ display: "flex", flexDirection: "column" }}>
        <div style={{ padding: "10px 16px", borderBottom: "1px solid var(--line-1)", display: "flex", alignItems: "center", gap: 10 }}>
          <span style={{ fontFamily: "var(--font-display)", fontWeight: 460, fontSize: 16 }}>July 2026</span>
          <div style={{ flex: 1 }} />
          <span className="mono" style={{ fontSize: 9, color: "var(--accent-text)" }}>■ governed</span>
          <span className="mono" style={{ fontSize: 9, color: "var(--tx-3)" }}>□ mirrored external</span>
        </div>
        <div style={{ display: "grid", gridTemplateColumns: "repeat(7,1fr)", borderBottom: "1px solid var(--line-1)" }}>
          {DOWS.map((d) => <span key={d} className="mono" style={{ fontSize: 9, letterSpacing: ".12em", textTransform: "uppercase", color: "var(--tx-3)", padding: "6px 10px" }}>{d}</span>)}
        </div>
        <div style={{ display: "grid", gridTemplateColumns: "repeat(7,1fr)" }}>
          {cells.map((c, i) => (
            <div key={i} style={{ minHeight: 74, borderRight: "1px solid var(--line-1)", borderBottom: "1px solid var(--line-1)", padding: "5px 7px", display: "flex", flexDirection: "column", gap: 3, background: c.d ? "transparent" : "var(--srf-inset)" }}>
              <span className="mono tabular" style={{ fontSize: 9.5, color: "var(--tx-3)" }}>{c.d ?? ""}</span>
              {c.evs.map((e, j) => {
                const col = e.gov ? (e.cls ? CLS_COLOR[e.cls] : "var(--accent)") : "var(--tx-3)";
                return <span key={j} className="mono" style={{ fontSize: 8.5, lineHeight: 1.3, color: col, borderLeft: `2px solid ${col}`, paddingLeft: 4, overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" }}>{e.time} {e.name}</span>;
              })}
            </div>
          ))}
        </div>
      </div>
      <div style={{ display: "flex", flexDirection: "column", gap: 14 }}>
        <div className="surface" style={{ display: "flex", flexDirection: "column" }}>
          <div style={{ padding: "10px 14px", borderBottom: "1px solid var(--line-1)" }}><span className="eyebrow">Connected accounts</span></div>
          {accounts.map((a) => (
            <div key={a.name} style={{ display: "flex", flexDirection: "column", gap: 3, padding: "10px 14px", borderBottom: "1px solid var(--line-1)" }}>
              <span style={{ fontSize: 12, fontWeight: 500 }}>{a.name}</span>
              <div style={{ display: "flex", alignItems: "center", gap: 8 }}>
                <span className="mono" style={{ fontSize: 8.5, letterSpacing: ".1em", textTransform: "uppercase", color: a.mode === "two-way" ? "var(--accent-text)" : "var(--tx-2)", border: `1px solid ${a.mode === "two-way" ? "var(--accent-text)" : "var(--line-2)"}`, padding: "1px 6px" }}>{a.mode}</span>
                <span className="mono" style={{ fontSize: 9, color: "var(--tx-3)" }}>synced {a.last}</span>
              </div>
            </div>
          ))}
          <div className="mono" style={{ fontSize: 9, color: "var(--tx-3)", padding: "8px 14px", lineHeight: 1.5 }}>Badges are truthful: read-only means Quorum never writes back. calendar.syncStatus()</div>
        </div>
        <div className="surface" style={{ padding: 14, display: "flex", flexDirection: "column", gap: 8, border: "1px solid var(--warn)" }}>
          <span className="eyebrow" style={{ color: "var(--warn)" }}>Sync conflict</span>
          <span style={{ fontSize: 12.5, lineHeight: 1.5 }}>An Outlook edit moved <span style={{ fontWeight: 500 }}>CCB #13</span> to 15:00 — but its time is a governed field, frozen by agenda. The external copy was not applied.</span>
          <div style={{ display: "flex", gap: 6 }}><button className="btn btn-ghost btn-sm">Keep governed time</button><button className="btn btn-ghost btn-sm">Propose amendment</button></div>
        </div>
        <div className="surface" style={{ padding: 14, display: "flex", flexDirection: "column", gap: 6 }}>
          <span className="eyebrow">Schedule a governed meeting</span>
          <span className="mono" style={{ fontSize: 10, color: "var(--tx-3)", lineHeight: 1.7 }}>collects: template · required roles · agents to call · classification · recurrence</span>
          <button className="btn btn-primary btn-sm" style={{ width: "fit-content" }}>New governed meeting</button>
        </div>
      </div>
    </div>
  );
}
