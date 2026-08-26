// citrate-quorum — Calendar surface (QRM-S2D). Charter register. Ported from
// design §CALENDAR. Month grid (governed meetings distinct from mirrored
// external events), connected accounts with truthful read-only/two-way badges,
// a governed-field sync conflict, and scheduling. Reads bridge.calendar.*.
import { useEffect, useMemo, useState } from "react";
import { bridge } from "../bridge";
import { DomainErrorPlate, useDomain } from "../components/DomainState";
import type { CalEvent } from "../bridge";

const CLS_COLOR: Record<string, string> = { Public: "var(--z-silver)", Proprietary: "var(--info)", CUI: "var(--warn)", ITAR: "var(--danger)" };
const DOWS = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"];

export function Calendar() {
  const [events, setEvents] = useState<CalEvent[]>([]);
  // A failed events read used to be swallowed, leaving an empty grid that looked
  // like "no meetings this month" rather than "we could not find out" (Rule 1).
  const [eventsFailed, setEventsFailed] = useState(false);
  useEffect(() => {
    bridge.calendar.events()
      .then((e) => { setEvents(e); setEventsFailed(false); })
      .catch(() => { setEvents([]); setEventsFailed(true); });
  }, []);

  // The grid was pinned to July 2026 — header, a 3-day leading offset and 31
  // days, all literals — and was already showing the wrong month by 2026-08-01.
  // Derived from the clock so it cannot go stale again.
  const { label, cells } = useMemo(() => {
    const now = new Date();
    const y = now.getFullYear();
    const m = now.getMonth();
    const offset = new Date(y, m, 1).getDay();
    const days = new Date(y, m + 1, 0).getDate();
    const byDay = new Map<number, CalEvent[]>();
    events.forEach((e) => { const a = byDay.get(e.d) ?? []; a.push(e); byDay.set(e.d, a); });
    const out: { d: number | null; evs: CalEvent[] }[] = [];
    for (let i = 0; i < offset; i++) out.push({ d: null, evs: [] });
    for (let d = 1; d <= days; d++) out.push({ d, evs: byDay.get(d) ?? [] });
    while (out.length % 7 !== 0) out.push({ d: null, evs: [] });
    return { label: now.toLocaleString("en-US", { month: "long", year: "numeric" }), cells: out };
  }, [events]);

  // Honest failure (S2D.4/§5.1): this surface's primary read is calendar.accounts().
  // A read that cannot succeed must say so and offer a retry, not sit in a
  // loading state forever.
  const primary = useDomain(() => bridge.calendar.accounts(), "calendar.accounts()");
  // Derived, not mirrored (see Agents.tsx).
  const accounts = primary.state.status === "ready" ? primary.state.data : [];
  if (primary.state.status === "error") {
    return (
      <div style={{ padding: 18 }}>
        <DomainErrorPlate source="calendar.accounts()" error={primary.state.error} onRetry={primary.retry} lands="It lands in QRM-S8 (calendar + repos)." />
      </div>
    );
  }

  return (
    <div style={{ padding: "20px 24px", display: "grid", gridTemplateColumns: "1fr 300px", gap: 16 }}>
      <div className="surface" style={{ display: "flex", flexDirection: "column" }}>
        <div style={{ padding: "10px 16px", borderBottom: "1px solid var(--line-1)", display: "flex", alignItems: "center", gap: 10 }}>
          <span style={{ fontFamily: "var(--font-display)", fontWeight: 460, fontSize: 16 }}>{label}</span>
          <div style={{ flex: 1 }} />
          {eventsFailed && (
            <span className="mono" style={{ fontSize: 9, color: "var(--warn)" }}>
              calendar.events() unavailable — this grid is empty because the read failed, not because the month is
            </span>
          )}
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
        {/*
          A "Sync conflict" plate lived here, reading: "An Outlook edit moved CCB
          #13 to 15:00 — but its time is a governed field, frozen by agenda."
          Static JSX. No state behind it, no condition in front of it. It rendered
          against a brand-new empty tenant and slipped every ratchet because it
          carries no thousands separator and no percentage.

          Governed-field conflict detection is real and belongs here — it is
          exactly what makes a governed calendar worth having — but it needs
          calendar.syncStatus() (QRM-S8). Until that lands, an invented incident
          is worse than no incident: an operator would go looking for CCB #13.
        */}
        <div className="surface" style={{ padding: 14, display: "flex", flexDirection: "column", gap: 6 }}>
          <span className="eyebrow">Schedule a governed meeting</span>
          <span className="mono" style={{ fontSize: 10, color: "var(--tx-3)", lineHeight: 1.7 }}>collects: template · required roles · agents to call · classification · recurrence</span>
          <button
            className="btn btn-primary btn-sm"
            style={{ width: "fit-content" }}
            disabled
            title="Scheduling a governed meeting is a governed act; it lands in QRM-S8 (calendar + repos)."
          >
            New governed meeting
          </button>
        </div>
      </div>
    </div>
  );
}
