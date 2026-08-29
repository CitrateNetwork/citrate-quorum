// citrate-quorum — Calendar surface. The tenant's GOVERNED meetings, on a grid.
//
// ## What this used to be, and why it changed
//
// This surface read `calendar.accounts()` as its primary read — an external
// provider integration that does not exist — so the whole surface collapsed to
// "NOT WIRED YET · It lands in QRM-S8", while `meetings.list()` sat live and
// unused one bridge call away. A calendar in a governance product is not
// primarily a mirror of somebody's Outlook; it is *when this tenant meets under
// governance*, and that data is real, local and already flowing.
//
// So the primary read is now `meetings.list()`. The external-mirroring concept
// is not explained here, not badged, and not promised — it is simply absent
// until it exists. A plate describing a missing feature is a worse answer than
// the feature, and a worse answer than silence.
//
// Removed with it: the "Connected accounts" panel (nothing to connect to), the
// "□ mirrored external" legend (nothing is mirrored), and a disabled "New
// governed meeting" button (Meetings already has a working scheduler — this
// navigates there rather than growing a second one, Rule 9).
//
// Kept from the QA sweep (#56): the month is derived from the clock, never
// pinned; a failed read says so instead of rendering an empty grid that reads
// as "no meetings".
//
// Rule 11: every value below comes from `meetings.list()`.
import { useMemo, useState } from "react";
import { bridge } from "../bridge";
import { DomainErrorPlate, useDomain } from "../components/DomainState";
import type { Meeting } from "../bridge";

const CLS_COLOR: Record<string, string> = { Public: "var(--z-silver)", Proprietary: "var(--info)", CUI: "var(--warn)", ITAR: "var(--danger)" };
const DOWS = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"];
const STATE_COLOR: Record<string, string> = {
  scheduled: "var(--tx-2)",
  "in-progress": "var(--accent-text)",
  awaiting: "var(--warn)",
  ratified: "var(--ok)",
  inquorate: "var(--danger)",
};

/**
 * The day a meeting falls on, or null when its `when` is not a date we can read.
 *
 * The backend stores `when` verbatim and never parses it (see MeetingSchedule),
 * so this surface must not assume it is RFC3339. An unparseable value is
 * SURFACED as undated rather than silently dropped or coerced to today — a
 * meeting that vanishes from the calendar because its timestamp was odd is the
 * failure this function exists to prevent.
 */
export function meetingDay(when: string): Date | null {
  const t = Date.parse(when);
  return Number.isNaN(t) ? null : new Date(t);
}

export function Calendar({ onGo }: { onGo: (id: string) => void }) {
  // Which month is on screen. The QA sweep unpinned this from a hardcoded July
  // 2026; it was still stuck on the CURRENT month, so a meeting scheduled for
  // next month was invisible with no way to look.
  const [offsetMonths, setOffsetMonths] = useState(0);

  const primary = useDomain(() => bridge.meetings.list(), "meetings.list()");
  // Memoised on the read state, not derived inline: a fresh `[]` every render
  // makes it a new dependency every render, so the grid below would recompute
  // on every tick of anything. eslint's rules-of-hooks catches exactly this.
  const meetings = useMemo<Meeting[]>(
    () => (primary.state.status === "ready" ? primary.state.data : []),
    [primary.state],
  );

  const { label, cells, undated, isThisMonth } = useMemo(() => {
    const base = new Date();
    const view = new Date(base.getFullYear(), base.getMonth() + offsetMonths, 1);
    const y = view.getFullYear();
    const m = view.getMonth();
    const lead = new Date(y, m, 1).getDay();
    const days = new Date(y, m + 1, 0).getDate();

    const byDay = new Map<number, Meeting[]>();
    const noDate: Meeting[] = [];
    for (const mt of meetings) {
      const d = meetingDay(mt.when);
      if (!d) { noDate.push(mt); continue; }
      if (d.getFullYear() !== y || d.getMonth() !== m) continue;
      const a = byDay.get(d.getDate()) ?? [];
      a.push(mt);
      byDay.set(d.getDate(), a);
    }
    const out: { d: number | null; ms: Meeting[]; today: boolean }[] = [];
    for (let i = 0; i < lead; i++) out.push({ d: null, ms: [], today: false });
    for (let d = 1; d <= days; d++) {
      out.push({
        d,
        ms: byDay.get(d) ?? [],
        today: offsetMonths === 0 && d === base.getDate(),
      });
    }
    while (out.length % 7 !== 0) out.push({ d: null, ms: [], today: false });
    return {
      label: view.toLocaleString("en-US", { month: "long", year: "numeric" }),
      cells: out,
      undated: noDate,
      isThisMonth: offsetMonths === 0,
    };
  }, [meetings, offsetMonths]);

  if (primary.state.status === "error") {
    return (
      <div style={{ padding: 18 }}>
        {/* No `lands` prop: this surface IS wired. A read that failed says so
            and offers a retry; it does not claim the feature is unbuilt. */}
        <DomainErrorPlate source="meetings.list()" error={primary.state.error} onRetry={primary.retry} />
      </div>
    );
  }

  const nav = (label: string, delta: number | "today") => (
    <button
      className="btn btn-ghost btn-sm"
      style={{ fontSize: 10 }}
      onClick={() => setOffsetMonths((n) => (delta === "today" ? 0 : n + delta))}
    >{label}</button>
  );

  return (
    <div style={{ padding: "20px 24px", display: "grid", gridTemplateColumns: "1fr 300px", gap: 16 }}>
      <div className="surface" style={{ display: "flex", flexDirection: "column" }}>
        <div style={{ padding: "10px 16px", borderBottom: "1px solid var(--line-1)", display: "flex", alignItems: "center", gap: 8 }}>
          <span style={{ fontFamily: "var(--font-display)", fontWeight: 460, fontSize: 16 }}>{label}</span>
          {nav("‹", -1)}
          {!isThisMonth && nav("today", "today")}
          {nav("›", 1)}
          <div style={{ flex: 1 }} />
          <span className="mono" style={{ fontSize: 9, color: "var(--tx-3)" }}>
            {meetings.length} governed meeting{meetings.length === 1 ? "" : "s"} in this tenant
          </span>
        </div>
        <div style={{ display: "grid", gridTemplateColumns: "repeat(7,1fr)", borderBottom: "1px solid var(--line-1)" }}>
          {DOWS.map((d) => <span key={d} className="mono" style={{ fontSize: 9, letterSpacing: ".12em", textTransform: "uppercase", color: "var(--tx-3)", padding: "6px 10px" }}>{d}</span>)}
        </div>
        <div style={{ display: "grid", gridTemplateColumns: "repeat(7,1fr)" }}>
          {cells.map((c, i) => (
            <div key={i} style={{ minHeight: 74, borderRight: "1px solid var(--line-1)", borderBottom: "1px solid var(--line-1)", padding: "5px 7px", display: "flex", flexDirection: "column", gap: 3, background: c.d ? (c.today ? "var(--srf-inset)" : "transparent") : "var(--srf-inset)" }}>
              <span className="mono tabular" style={{ fontSize: 9.5, color: c.today ? "var(--accent-text)" : "var(--tx-3)", fontWeight: c.today ? 600 : 400 }}>{c.d ?? ""}</span>
              {c.ms.map((mt) => {
                const col = CLS_COLOR[mt.classification] ?? "var(--accent)";
                return (
                  <button
                    key={mt.id}
                    onClick={() => onGo("meetings")}
                    title={`${mt.name} · ${mt.tpl} · ${mt.classification} · ${mt.state}`}
                    className="mono"
                    style={{ fontSize: 8.5, lineHeight: 1.3, color: col, borderLeft: `2px solid ${col}`, background: "transparent", border: "none", borderLeftWidth: 2, borderLeftStyle: "solid", textAlign: "left", padding: "0 0 0 4px", cursor: "pointer", overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" }}
                  >{mt.name}</button>
                );
              })}
            </div>
          ))}
        </div>
      </div>

      <div style={{ display: "flex", flexDirection: "column", gap: 14 }}>
        <div className="surface" style={{ display: "flex", flexDirection: "column" }}>
          <div style={{ padding: "10px 14px", borderBottom: "1px solid var(--line-1)" }}><span className="eyebrow">This tenant's meetings</span></div>
          {meetings.length === 0 && (
            <div className="mono" style={{ fontSize: 10.5, color: "var(--tx-3)", padding: "12px 14px", lineHeight: 1.6 }}>
              None scheduled. This is the tenant's own record, not a mirror of anyone's calendar.
            </div>
          )}
          {meetings.slice(0, 8).map((mt) => (
            <div key={mt.id} style={{ display: "flex", flexDirection: "column", gap: 3, padding: "9px 14px", borderBottom: "1px solid var(--line-1)" }}>
              <span style={{ fontSize: 12, fontWeight: 500 }}>{mt.name}</span>
              <div style={{ display: "flex", alignItems: "center", gap: 8, flexWrap: "wrap" }}>
                <span className="mono" style={{ fontSize: 8.5, letterSpacing: ".1em", textTransform: "uppercase", color: STATE_COLOR[mt.state] ?? "var(--tx-2)", border: `1px solid ${STATE_COLOR[mt.state] ?? "var(--line-2)"}`, padding: "1px 6px" }}>{mt.state}</span>
                <span className="mono" style={{ fontSize: 8.5, letterSpacing: ".1em", textTransform: "uppercase", color: CLS_COLOR[mt.classification] ?? "var(--tx-2)" }}>{mt.classification}</span>
                <span className="mono" style={{ fontSize: 9, color: "var(--tx-3)" }}>{mt.humans}H · {mt.agents}A</span>
              </div>
            </div>
          ))}
          {/* An unparseable `when` would otherwise vanish from the grid with no
              trace. The backend stores it verbatim, so this surface cannot
              assume a format — it shows what it could not place. */}
          {undated.length > 0 && (
            <div className="mono" style={{ fontSize: 10, color: "var(--warn)", padding: "9px 14px", lineHeight: 1.6, borderTop: "1px solid var(--line-1)" }}>
              {undated.length} meeting(s) carry a date this surface cannot read, so they are not on the grid: {undated.map((m) => m.name).join(", ")}
            </div>
          )}
          <div className="mono" style={{ fontSize: 9, color: "var(--tx-3)", padding: "8px 14px", lineHeight: 1.5 }}>meetings.list()</div>
        </div>

        <div className="surface" style={{ padding: 14, display: "flex", flexDirection: "column", gap: 8 }}>
          <span className="eyebrow">Schedule a governed meeting</span>
          <span style={{ fontSize: 12, color: "var(--tx-2)", lineHeight: 1.55 }}>
            Scheduling collects the template, its quorum rule, the classification and the workspace whose sprint files become the agenda.
          </span>
          {/* Meetings owns the scheduler and it works. A second form here would
              be two implementations of one governed act (Rule 9). */}
          <button className="btn btn-primary btn-sm" style={{ width: "fit-content" }} onClick={() => onGo("meetings")}>
            Open the scheduler
          </button>
        </div>
      </div>
    </div>
  );
}
