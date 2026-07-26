// citrate-quorum — Rooms surface (QRM-S2D). Demo beat 2: the governed standup,
// the "Zoom for agents." Ported from design §ROOMS. Instrument register. Room
// list → consent gate → three-pane in-room (roster / streaming transcript /
// work panel with agenda+approvals+live-vote+minutes). Reads bridge.rooms.list()
// and the bridge.rooms.events transcript stream; humans and agents are peers.
import { useEffect, useMemo, useState } from "react";
import { bridge } from "../bridge";
import { DomainErrorPlate, useDomain } from "../components/DomainState";
import type { RoomEvent, RosterMember } from "../bridge";
import { VENDORS } from "../theme/vendors";

const CLS_COLOR: Record<string, string> = { Public: "var(--z-silver)", Proprietary: "var(--info)", CUI: "var(--warn)", ITAR: "var(--danger)" };
const VERDICT_COLOR: Record<string, string> = { allow: "var(--ok)", "require-approval": "var(--warn)", deny: "var(--danger)" };
function whoColorFrom(roster: RosterMember[], who?: string, human?: boolean) {
  if (human) return "var(--accent-text)";
  const m = roster.find((r) => r.id === who || r.name === who);
  return m?.vendor ? VENDORS[m.vendor]?.color ?? "var(--tx-2)" : "var(--tx-2)";
}

function RosterRow({ m }: { m: RosterMember }) {
  const color = m.vendor ? VENDORS[m.vendor]?.color ?? "var(--tx-2)" : "var(--accent)";
  const initials = m.name.split(" ").map((w) => w[0]).join("").slice(0, 2).toUpperCase();
  return (
    <div style={{ display: "flex", alignItems: "center", gap: 8, padding: "6px 6px", borderRadius: "var(--r-1)" }} title={m.sbt ?? m.role ?? ""}>
      <span style={{ width: 24, height: 24, borderRadius: 999, background: m.human ? "rgba(142,204,9,.15)" : "var(--srf-2)", border: `1px solid ${color}`, color, display: "inline-flex", alignItems: "center", justifyContent: "center", fontSize: 9, fontWeight: 600, flexShrink: 0 }}>{initials}</span>
      <div style={{ minWidth: 0, flex: 1 }}>
        <div style={{ fontSize: 12, fontWeight: 500, overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" }}>{m.name}</div>
        <div className="mono" style={{ fontSize: 8.5, color: "var(--tx-3)", overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" }}>{m.human ? m.role : `${VENDORS[m.vendor!]?.name} · ${m.sbt}`}</div>
      </div>
      {m.hic != null && <span className="mono" style={{ fontSize: 8, color: "var(--tx-3)", flexShrink: 0 }}>HIC-{m.hic}</span>}
    </div>
  );
}

export function Rooms() {
  const [view, setView] = useState<"list" | "consent" | "room">("list");
  const [events, setEvents] = useState<RoomEvent[]>([]);
  const [roster, setRoster] = useState<RosterMember[]>([]);

  useEffect(() => { bridge.rooms.roster("r-std4").then(setRoster).catch(() => {}); }, []);

  useEffect(() => {
    if (view !== "room") return;
    // Deliberate: entering a room must clear the previous room's transcript
    // before this room's stream is subscribed. Showing another room's events
    // for even one frame would be a classification leak, not a cosmetic bug.
    // eslint-disable-next-line react-hooks/set-state-in-effect -- see above
    setEvents([]);
    let unsub: (() => void) | undefined;
    try {
      unsub = bridge.rooms.events((e) => setEvents((p) => [...p, e]));
    } catch {
      /* no transcript stream until the relay is wired (QRM-S3) */
    }
    return unsub;
  }, [view]);

  // Aggregate the live vote from the streamed cast/close events.
  const vote = useMemo(() => {
    const open = events.find((e) => e.kind === "system" && (e.text ?? "").startsWith("Vote opened"));
    if (!open) return null;
    const casts = events.filter((e) => e.kind === "vote-cast");
    const close = events.find((e) => e.kind === "vote-close");
    const forW = casts.filter((c) => c.choice === "For").reduce((a, c) => a + (c.weight ?? 0), 0);
    const abstW = casts.filter((c) => c.choice === "Abstain").reduce((a, c) => a + (c.weight ?? 0), 0);
    const tot = casts.reduce((a, c) => a + (c.weight ?? 0), 0) || 100;
    return { q: open.text!.replace("Vote opened — ", ""), casts, forW, abstW, tot, result: close?.text ?? null };
  }, [events]);

  // Honest failure (S2D.4/§5.1): this surface's primary read is rooms.list().
  // A read that cannot succeed must say so and offer a retry, not sit in a
  // loading state forever.
  const primary = useDomain(() => bridge.rooms.list(), "rooms.list()");
  // Derived, not mirrored (see Agents.tsx).
  const rooms = primary.state.status === "ready" ? primary.state.data : [];
  if (primary.state.status === "error") {
    return (
      <div style={{ padding: 18 }}>
        <DomainErrorPlate source="rooms.list()" error={primary.state.error} onRetry={primary.retry} lands="It lands in QRM-S3 (rooms), which is gated on the G1 export-control legal opinion." />
      </div>
    );
  }

  const whoColor = (who?: string, human?: boolean) => whoColorFrom(roster, who, human);
  const escalation = events.find((e) => e.kind === "tool" && e.escalate);
  const minutesN = events.filter((e) => ["system", "tool", "contradiction", "vote-close"].includes(e.kind)).length;

  if (view === "list") {
    return (
      <div style={{ padding: 18, display: "flex", flexDirection: "column", gap: 10 }}>
        {rooms.map((rm) => (
          <div key={rm.id} className="surface" onClick={() => rm.live && setView("consent")} style={{ display: "flex", alignItems: "center", gap: 14, padding: "14px 16px", cursor: rm.live ? "pointer" : "default" }}>
            <span style={{ width: 8, height: 8, borderRadius: 999, background: rm.live ? "var(--accent)" : "var(--line-2)", animation: rm.live ? "ccPulse 1.5s infinite" : "none" }} />
            <span style={{ fontSize: 14, fontWeight: 500, flex: 1 }}>{rm.name}</span>
            <span className="mono" style={{ fontSize: 9, letterSpacing: ".12em", textTransform: "uppercase", color: CLS_COLOR[rm.classification], border: `1px solid ${CLS_COLOR[rm.classification]}`, padding: "2px 7px" }}>{rm.classification}</span>
            <span className="mono tabular" style={{ fontSize: 10.5, color: "var(--tx-3)" }}>{rm.members} members{rm.started ? ` · since ${rm.started}` : ""}</span>
          </div>
        ))}
        <div className="mono" style={{ fontSize: 9.5, color: "var(--tx-3)" }}>Read from rooms.list() → relay-wichita-2 · rooms are E2E-encrypted; the relay sees ciphertext</div>
      </div>
    );
  }

  if (view === "consent") {
    return (
      <div style={{ flex: 1, display: "flex", alignItems: "center", justifyContent: "center", padding: 24 }}>
        <div className="surface" style={{ width: 420, padding: 22, display: "flex", flexDirection: "column", gap: 12, borderTop: "2px solid var(--info)" }}>
          <span className="eyebrow">Before you join</span>
          <div style={{ fontSize: 15, fontWeight: 500 }}>Audio is transcribed locally</div>
          <p style={{ fontSize: 13, lineHeight: 1.55, color: "var(--tx-2)", margin: 0 }}>Push-to-talk speech is transcribed on this machine and posted as attributed text. Raw audio is never stored and never leaves this device. The transcript is part of the room record.</p>
          <div style={{ display: "flex", gap: 8 }}>
            <button className="btn btn-primary btn-sm" onClick={() => setView("room")}>I consent — join with voice</button>
            <button className="btn btn-ghost btn-sm" onClick={() => setView("room")}>Join text-only</button>
          </div>
        </div>
      </div>
    );
  }

  return (
    <div style={{ height: "100%", display: "grid", gridTemplateColumns: "216px 1fr 288px", minHeight: 0 }}>
      {/* roster */}
      <div style={{ borderRight: "1px solid var(--line-1)", display: "flex", flexDirection: "column", minHeight: 0, background: "var(--srf-1)" }}>
        <div style={{ padding: "12px 12px 8px", display: "flex", flexDirection: "column", gap: 2 }}>
          <span style={{ fontSize: 13.5, fontWeight: 500 }}>Weekly Standup — Line-4</span>
          <div style={{ display: "flex", alignItems: "center", gap: 6 }}>
            <span className="mono" style={{ fontSize: 8.5, letterSpacing: ".12em", textTransform: "uppercase", color: "var(--info)", border: "1px solid var(--info)", padding: "1px 5px" }}>Proprietary</span>
            <span className="mono" style={{ fontSize: 9, color: "var(--tx-3)" }}>since 09:00</span>
          </div>
        </div>
        <div style={{ flex: 1, overflow: "auto", padding: "4px 8px", display: "flex", flexDirection: "column", gap: 2 }}>
          {roster.map((m) => <RosterRow key={m.id} m={m} />)}
        </div>
        <div style={{ padding: 10, display: "flex", flexDirection: "column", gap: 6, borderTop: "1px solid var(--line-1)" }}>
          <button className="btn btn-ghost btn-sm" style={{ width: "100%" }}>Call an agent…</button>
          <button className="btn btn-danger btn-sm" style={{ width: "100%" }}>Revoke all — this room</button>
        </div>
      </div>

      {/* transcript */}
      <div style={{ display: "flex", flexDirection: "column", minHeight: 0 }}>
        <div style={{ flex: 1, overflow: "auto", padding: "14px 18px", display: "flex", flexDirection: "column", gap: 10 }}>
          {events.map((ev, i) => {
            if (ev.kind === "system" || ev.kind === "vote-close") {
              return (
                <div key={i} className="mono" style={{ borderTop: "1px solid var(--line-2)", borderBottom: "1px solid var(--line-2)", padding: "7px 2px", display: "flex", alignItems: "baseline", gap: 10 }}>
                  <span style={{ fontSize: 10.5, letterSpacing: ".06em", color: ev.kind === "vote-close" ? "var(--ok)" : "var(--tx-2)" }}>{ev.text}</span>
                  {ev.meta && <span style={{ fontSize: 9, color: "var(--tx-3)" }}>{ev.meta}</span>}
                </div>
              );
            }
            if (ev.kind === "speech" || ev.kind === "text") {
              const c = whoColor(ev.who, ev.human);
              return (
                <div key={i} style={{ display: "flex", gap: 10 }}>
                  <span className="mono" style={{ fontSize: 9.5, color: c, border: `1px solid ${c}`, padding: "2px 7px", height: "fit-content", flexShrink: 0 }}>{ev.who}</span>
                  <div style={{ minWidth: 0 }}>
                    <p style={{ fontSize: 13, lineHeight: 1.5, margin: 0, fontStyle: ev.kind === "speech" ? "italic" : "normal" }}>{ev.text}</p>
                    {ev.cites?.map((cite, j) => (
                      <span key={j} className="mono" style={{ display: "inline-block", fontSize: 9.5, color: "var(--info)", border: "1px solid var(--info)", padding: "1px 7px", margin: "5px 6px 0 0", cursor: "pointer" }}>{cite}</span>
                    ))}
                  </div>
                </div>
              );
            }
            if (ev.kind === "tool") {
              const vc = VERDICT_COLOR[ev.verdict ?? "allow"];
              return (
                <div key={i} className="mono" style={{ display: "flex", alignItems: "center", gap: 10, border: `1px solid ${ev.escalate ? "var(--warn)" : "var(--line-2)"}`, background: ev.escalate ? "var(--warn-bg)" : "var(--srf-1)", padding: "6px 10px", fontSize: 10.5 }}>
                  <span style={{ color: whoColor(ev.who) }}>{ev.who}</span>
                  <span>{ev.tool}</span>
                  <span style={{ color: vc, textTransform: "uppercase", letterSpacing: ".08em", fontSize: 9 }}>{ev.verdict}</span>
                  <span style={{ color: "var(--tx-3)" }}>{ev.dur}</span>
                  <div style={{ flex: 1 }} />
                  <span style={{ color: "var(--tx-3)", fontSize: 9.5 }}>{ev.result}</span>
                </div>
              );
            }
            if (ev.kind === "contradiction") {
              return (
                <div key={i} style={{ border: "1.5px solid var(--warn)", padding: "12px 14px", display: "flex", flexDirection: "column", gap: 8, background: "var(--warn-bg)" }}>
                  <span className="mono" style={{ fontSize: 9.5, letterSpacing: ".14em", textTransform: "uppercase", color: "var(--warn)" }}>Contradiction — two attested sources disagree</span>
                  <span style={{ fontSize: 13, fontWeight: 500 }}>{ev.fact}</span>
                  <div style={{ display: "grid", gridTemplateColumns: "1fr 1fr", gap: 8 }}>
                    <div className="mono" style={{ fontSize: 10.5, border: "1px solid var(--line-2)", padding: "7px 9px", background: "var(--srf-0)" }}><span style={{ color: "var(--z-cyan)" }}>{ev.a}</span><br />{ev.va}</div>
                    <div className="mono" style={{ fontSize: 10.5, border: "1px solid var(--line-2)", padding: "7px 9px", background: "var(--srf-0)" }}><span style={{ color: "var(--z-indigo)" }}>{ev.b}</span><br />{ev.vb}</div>
                  </div>
                  <span style={{ fontSize: 11.5, color: "var(--tx-2)" }}>{ev.note}</span>
                  <div style={{ display: "flex", gap: 6 }}><button className="btn btn-ghost btn-sm">Escalate</button><button className="btn btn-ghost btn-sm">Resolve</button><button className="btn btn-ghost btn-sm">Withdraw</button></div>
                </div>
              );
            }
            if (ev.kind === "vote-cast") {
              return (
                <div key={i} className="mono" style={{ fontSize: 10, color: "var(--tx-3)", paddingLeft: 12 }}>▸ <span style={{ color: whoColor(ev.who, ev.human) }}>{ev.who}</span> voted <span style={{ color: "var(--tx-1)" }}>{ev.choice}</span> · weight {ev.weight}{ev.proof ? ` · ${ev.proof}` : ""}</div>
              );
            }
            return null;
          })}
        </div>
        <div style={{ display: "flex", gap: 8, padding: "10px 18px", borderTop: "1px solid var(--line-1)", alignItems: "center" }}>
          <input className="input" placeholder="Message the room — @agent to address one" style={{ flex: 1 }} />
          <button className="btn btn-ghost" title="Push to talk — hold">🎙 Hold to talk</button>
          <span className="mono" style={{ fontSize: 8.5, letterSpacing: ".1em", textTransform: "uppercase", color: "var(--tx-3)" }}>transcribing locally</span>
        </div>
      </div>

      {/* work panel */}
      <div style={{ borderLeft: "1px solid var(--line-1)", overflow: "auto", display: "flex", flexDirection: "column", background: "var(--srf-1)" }}>
        <div style={{ padding: "12px 14px", borderBottom: "1px solid var(--line-1)", display: "flex", flexDirection: "column", gap: 8 }}>
          <div style={{ display: "flex", alignItems: "center", gap: 8 }}><span className="eyebrow">Agenda</span><div style={{ flex: 1 }} /><span className="mono" style={{ fontSize: 9, color: "var(--ok)", border: "1px solid var(--ok)", padding: "1px 7px" }}>✓ verified</span></div>
          <span className="mono" style={{ fontSize: 9.5, color: "var(--tx-3)" }}>b3:aa17…90c2 · frozen at open</span>
          {["Agent reports", "hermes calendar ask", "coverage question", "PRT-004 A2 vote"].map((t, i) => (
            <div key={i} style={{ display: "flex", gap: 8, fontSize: 12, color: "var(--tx-2)" }}><span className="mono tabular" style={{ color: "var(--tx-3)" }}>{i + 1}</span><span>{t}</span></div>
          ))}
        </div>
        <div style={{ padding: "12px 14px", borderBottom: "1px solid var(--line-1)", display: "flex", flexDirection: "column", gap: 8 }}>
          <span className="eyebrow">Approvals pending</span>
          {escalation ? (
            <div style={{ border: "1px solid var(--warn)", background: "var(--warn-bg)", padding: "8px 10px", cursor: "pointer", display: "flex", flexDirection: "column", gap: 3 }}>
              <span className="mono" style={{ fontSize: 9.5, color: "var(--warn)" }}>{escalation.who} · {escalation.tool}</span>
              <span style={{ fontSize: 11.5, color: "var(--tx-2)" }}>bounded grant requested — review →</span>
            </div>
          ) : (
            <span style={{ fontSize: 12, color: "var(--tx-3)" }}>None. Asks land here the moment an agent is blocked.</span>
          )}
        </div>
        {vote && (
          <div style={{ padding: "12px 14px", borderBottom: "1px solid var(--line-1)", display: "flex", flexDirection: "column", gap: 8 }}>
            <div style={{ display: "flex", alignItems: "center", gap: 8 }}><span className="eyebrow">Live vote</span>{!vote.result && <span style={{ width: 6, height: 6, borderRadius: 999, background: "var(--accent)", animation: "ccPulse 1.4s infinite" }} />}</div>
            <span style={{ fontSize: 12.5, lineHeight: 1.45 }}>{vote.q}</span>
            <div style={{ display: "flex", flexDirection: "column", gap: 5 }}>
              <div style={{ display: "flex", alignItems: "center", gap: 8 }}><span className="mono" style={{ fontSize: 9.5, width: 52, color: "var(--tx-3)" }}>FOR</span><div style={{ flex: 1, height: 8, background: "var(--srf-inset)", border: "1px solid var(--line-1)" }}><div style={{ height: "100%", width: `${(vote.forW / vote.tot) * 100}%`, background: "var(--accent)" }} /></div><span className="mono tabular" style={{ fontSize: 10 }}>{vote.forW}</span></div>
              <div style={{ display: "flex", alignItems: "center", gap: 8 }}><span className="mono" style={{ fontSize: 9.5, width: 52, color: "var(--tx-3)" }}>ABSTAIN</span><div style={{ flex: 1, height: 8, background: "var(--srf-inset)", border: "1px solid var(--line-1)" }}><div style={{ height: "100%", width: `${(vote.abstW / vote.tot) * 100}%`, background: "var(--z-silver)" }} /></div><span className="mono tabular" style={{ fontSize: 10 }}>{vote.abstW}</span></div>
            </div>
            {vote.casts.filter((c) => c.proof).map((c, i) => (
              <div key={i} className="mono" style={{ fontSize: 9.5, color: "var(--tx-3)", display: "flex", flexDirection: "column", gap: 1, borderLeft: "2px solid var(--line-2)", paddingLeft: 8 }}>
                <span><span style={{ color: whoColor(c.who, c.human) }}>{c.who}</span> · {c.choice} · w{c.weight}</span>
                <span style={{ display: "flex", alignItems: "center", gap: 6 }}>{c.proof} <span style={{ color: "var(--danger)", cursor: "pointer" }}>revoke allowance</span></span>
              </div>
            ))}
            {vote.result && <div className="cc-stamp mono" style={{ fontSize: 10.5, color: "var(--ok)", border: "1px solid var(--ok)", background: "var(--ok-bg)", padding: "6px 9px" }}>{vote.result}</div>}
          </div>
        )}
        <div style={{ padding: "12px 14px", display: "flex", flexDirection: "column", gap: 8 }}>
          <div style={{ display: "flex", alignItems: "center", gap: 8 }}><span className="eyebrow">Minutes</span><span className="mono" style={{ fontSize: 8.5, letterSpacing: ".1em", color: "var(--warn)", border: "1px solid var(--warn)", padding: "1px 6px" }}>DRAFT</span></div>
          <span style={{ fontSize: 12, color: "var(--tx-2)", lineHeight: 1.5 }}>Drafting live from the transcript — {minutesN} record events so far. Ratification happens on Meetings after the room closes.</span>
          <a href="#/meetings" className="btn btn-ghost btn-sm" style={{ width: "fit-content" }}>Open meeting record →</a>
        </div>
      </div>
    </div>
  );
}
