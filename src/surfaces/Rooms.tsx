// citrate-quorum — Rooms surface (QRM-S3). LIVE.
//
// A room here is a real MLS group on the citrate-comms relay. Humans and agents
// are members of the same group; the relay carries ciphertext and routing
// metadata and can decrypt nothing — proved by `src-tauri/tests/server_blindness.rs`
// against a real relay store, with a negative control so the proof cannot pass by
// searching an empty directory.
//
// What the ported design prototype showed here and this does NOT, because it does
// not exist:
//   · a push-to-talk button and "transcribing locally". There is no audio path at
//     all. Voice (the stt-worker sidecar) is planset WP5 and is not built; a mic
//     button that did nothing would be the worst kind of claim to make in a room
//     that may carry controlled data.
//   · a consent gate about audio, for the same reason.
//   · a frozen agenda with a "✓ verified" hash, a live vote with delegated
//     weights, a contradiction between two attested sources, and draft minutes.
//     Those are Meetings (live, on its own surface) and Governance (not built);
//     rendering them here as room furniture invented four features at once.
//   · a fixed room title, roster and "since 09:00". Rooms are opened by the
//     operator and the roster is whoever actually joined.
//
// Reads bridge.rooms.status/list/roster/events and writes through open/say/leave.
import { useCallback, useEffect, useMemo, useState } from "react";
import { bridge } from "../bridge";
import { Domain, useDomain } from "../components/DomainState";
import { useCeremony } from "../ceremony/Ceremony";
import type { Classification, Room, RoomEvent, RoomsStatus, RosterMember } from "../bridge";

const CLS_COLOR: Record<string, string> = {
  Public: "var(--z-silver)",
  Proprietary: "var(--info)",
  CUI: "var(--warn)",
  ITAR: "var(--danger)",
};
const CLASSES: Classification[] = ["Public", "Proprietary", "CUI", "ITAR"];
/** How often the transcript is drained from the relay (ms). */
const POLL_MS = 1500;

function RosterRow({ m }: { m: RosterMember }) {
  const color = m.human ? "var(--accent)" : "var(--info)";
  const initials = m.name
    .split(/[\s-]+/)
    .map((w) => w[0])
    .join("")
    .slice(0, 2)
    .toUpperCase();
  return (
    <div style={{ display: "flex", alignItems: "center", gap: 8, padding: 6, borderRadius: "var(--r-1)" }} title={`relay identity ${m.address}`}>
      <span style={{ width: 24, height: 24, borderRadius: 999, background: "var(--srf-2)", border: `1px solid ${color}`, color, display: "inline-flex", alignItems: "center", justifyContent: "center", fontSize: 9, fontWeight: 600, flexShrink: 0 }}>{initials}</span>
      <div style={{ minWidth: 0, flex: 1 }}>
        <div style={{ fontSize: 12, fontWeight: 500, overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" }}>{m.name}</div>
        <div className="mono" style={{ fontSize: 8.5, color: "var(--tx-3)" }}>
          {m.human ? "human" : "agent · key held by this app"} · mls {m.mlsKey}
        </div>
      </div>
    </div>
  );
}

export function Rooms() {
  const [status, setStatus] = useState<RoomsStatus | null>(null);
  const [statusErr, setStatusErr] = useState<string | null>(null);
  const [active, setActive] = useState<string | null>(null);
  const [roster, setRoster] = useState<RosterMember[]>([]);
  const [events, setEvents] = useState<RoomEvent[]>([]);
  /** null while the transcript is draining; a reason when it is not. */
  const [drainDown, setDrainDown] = useState<string | null>(null);
  const [rooms, setRooms] = useState<Room[]>([]);
  const [draft, setDraft] = useState("");
  const [opening, setOpening] = useState(false);
  const [form, setForm] = useState({ name: "", classification: "Proprietary" as Classification, agents: "" });
  const [operator, setOperator] = useState<string>("");
  const [busy, setBusy] = useState<string | null>(null);

  const ceremony = useCeremony();
  const primary = useDomain(() => bridge.rooms.status(), "rooms.status()");

  useEffect(() => {
    bridge.session.operator().then((o) => setOperator(o ?? "")).catch(() => {});
  }, []);

  const refreshRooms = useCallback(() => {
    bridge.rooms.list().then(setRooms).catch(() => {
      /* the status plate above carries the connection's own failure */
    });
  }, []);

  useEffect(() => {
    if (primary.state.status === "ready") {
      // eslint-disable-next-line react-hooks/set-state-in-effect -- seeded once from the read, then owned here
      setStatus(primary.state.data);
      refreshRooms();
    }
  }, [primary.state, refreshRooms]);

  // Drain the relay on a beat. Polled, not pushed: same reason the ledger ribbon
  // polls — no event plumbing, and a room is not a high-rate source.
  useEffect(() => {
    if (!status?.connected) return;
    let live = true;
    const tick = () => {
      bridge.rooms
        .events(0)
        .then((all) => {
          if (!live) return;
          setEvents(all);
          setDrainDown(null);
        })
        .catch((e: unknown) => {
          // A transient drain failure must not CLEAR the transcript — the lines
          // already decrypted are still true. But it must stop the panel
          // claiming the transcript is live, because it is not: the relay
          // connection can be up while the drain fails, and a pulsing dot over a
          // frozen transcript is the one thing a room must never show.
          if (live) setDrainDown(e instanceof Error ? e.message : String(e));
        });
    };
    tick();
    const iv = setInterval(tick, POLL_MS);
    return () => {
      live = false;
      clearInterval(iv);
    };
  }, [status?.connected]);

  useEffect(() => {
    if (!active) return;
    let live = true;
    bridge.rooms.roster(active).then((r) => live && setRoster(r)).catch(() => {});
    return () => {
      live = false;
    };
  }, [active, events.length]);

  // Connecting is a governed act, not a toggle: the operator signs a SIWE
  // message with the vault key, through the one ceremony, and the seat the relay
  // grants IS their wallet address. One approval per session — MLS signs every
  // message after that with the member's own credential key.
  const connect = async () => {
    setBusy("opening the relay socket…");
    try {
      const intent = await bridge.rooms.connectIntent(operator || "operator");
      setBusy(null);
      const r = await ceremony.request({
        kind: "raw",
        title: "Sign in to the comms relay",
        // "user": the operator is acting under their OWN authority, so this
        // skips the agent policy gate and is recorded as an approved HIC-1 act
        // naming them. Sending it through the agent gate would record every
        // human relay login as ungoverned and inflate the product's own alarm
        // — the modelling fix from QRM-S4.3, applied here.
        origin: "user",
        action: {
          actionClass: "rooms.connect",
          classification: "Public",
          agent: operator || "operator",
          principal: operator || "operator",
          mandatoryHic1: true,
        },
        rows: [
          { k: "Relay", v: intent.relayUrl },
          { k: "Signing as", v: intent.address },
          { k: "Effect", v: "authenticates this machine to the relay as you, for this session. Room messages after this are signed by the member key, not by you." },
        ],
        signature: {
          label: "relay login",
          prepare: async () => ({ id: intent.ceremonyId }),
          apply: async (sigHex: string) => {
            setStatus(await bridge.rooms.connectComplete(intent.ceremonyId, sigHex));
            return `seat ${intent.address.slice(0, 10)}… authenticated`;
          },
        },
      });
      if (r.outcome !== "settled") setStatusErr("the relay login was not completed");
      else setStatusErr(null);
    } catch (e) {
      setStatusErr(e instanceof Error ? e.message : String(e));
    } finally {
      setBusy(null);
    }
  };

  const open = async () => {
    setBusy("opening the room — each agent seat logs in and joins…");
    try {
      const agents = form.agents.split(",").map((a) => a.trim()).filter(Boolean);
      const room = await bridge.rooms.open(
        { name: form.name || "Untitled room", classification: form.classification, agents },
        operator || "operator",
      );
      setStatusErr(null);
      setOpening(false);
      setActive(room.id);
      refreshRooms();
      bridge.rooms.status().then(setStatus).catch(() => {});
    } catch (e) {
      setStatusErr(e instanceof Error ? e.message : String(e));
    } finally {
      setBusy(null);
    }
  };

  const say = async () => {
    if (!active || !draft.trim()) return;
    const text = draft;
    setDraft("");
    try {
      await bridge.rooms.say(active, operator || "operator", text);
    } catch (e) {
      setStatusErr(e instanceof Error ? e.message : String(e));
    }
  };

  const shown = useMemo(() => events.filter((e) => !active || e.room === active), [events, active]);

  return (
    <div style={{ padding: 18, display: "flex", flexDirection: "column", gap: 14, maxWidth: 1100 }}>
      <Domain
        read={primary}
        source="rooms.status()"
        lands="It needs the citrate-comms relay."
        skeletonRows={2}
      >
        {() => (
          <>
            <div className="surface" style={{ padding: "12px 16px", display: "flex", alignItems: "center", gap: 12, flexWrap: "wrap" }}>
              <span style={{ width: 8, height: 8, borderRadius: 999, background: status?.connected ? "var(--accent)" : "var(--line-2)", animation: status?.connected ? "ccPulse 1.6s infinite" : "none" }} />
              <span className="eyebrow">{status?.connected ? "Connected" : "Not connected"}</span>
              <span className="mono" style={{ fontSize: 10.5, color: "var(--tx-2)" }}>{status?.relayUrl}</span>
              {status?.address && (
                <span className="mono" style={{ fontSize: 9.5, color: "var(--tx-3)" }} title="relay identity — not the chain identity that ratifies minutes">
                  seat {status.address.slice(0, 10)}… · {status.seats} seat{status.seats === 1 ? "" : "s"}
                </span>
              )}
              <div style={{ flex: 1 }} />
              {!status?.connected && (
                <button className="btn btn-primary btn-sm" onClick={connect} disabled={Boolean(busy)}>Sign in to the relay</button>
              )}
              {status?.connected && (
                <button className="btn btn-ghost btn-sm" onClick={() => setOpening((v) => !v)} disabled={Boolean(busy)}>Open a room…</button>
              )}
            </div>
            {busy && <div className="mono" style={{ fontSize: 10.5, color: "var(--tx-3)" }}>{busy}</div>}
            {statusErr && (
              <div className="mono" style={{ fontSize: 10.5, color: "var(--danger)", border: "1px solid var(--danger)", background: "var(--danger-bg)", padding: "8px 12px", lineHeight: 1.6 }}>
                {statusErr}
              </div>
            )}

            {opening && (
              <div className="surface" style={{ padding: 16, display: "flex", flexDirection: "column", gap: 10, borderTop: "2px solid var(--line-strong)", maxWidth: 620 }}>
                <span className="eyebrow">Open a room</span>
                <div><div className="lbl">Name</div><input className="input" style={{ width: "100%" }} value={form.name} onChange={(e) => setForm({ ...form, name: e.target.value })} placeholder="Weekly standup" /></div>
                <div>
                  <div className="lbl">Classification</div>
                  <select className="input" style={{ width: "100%" }} value={form.classification} onChange={(e) => setForm({ ...form, classification: e.target.value as Classification })}>
                    {CLASSES.map((c) => <option key={c} value={c}>{c}</option>)}
                  </select>
                </div>
                <div>
                  <div className="lbl">Agents to admit (comma separated)</div>
                  <input className="input" style={{ width: "100%" }} value={form.agents} onChange={(e) => setForm({ ...form, agents: e.target.value })} placeholder="claude-code, codex" />
                  <div className="mono" style={{ fontSize: 9.5, color: "var(--tx-3)", marginTop: 4, lineHeight: 1.6 }}>
                    Each agent gets its own seat, with a key held by THIS app and sealed in the vault — the agent
                    process never holds one. To the relay an agent seat is indistinguishable from a human's.
                    <br />
                    <strong>MR-4:</strong> an agent is admitted only if a live capability grant clears it to this
                    room's classification. A revoked or expired grant stops clearing it immediately, and the room
                    opens or is refused as a whole — a partly-admitted room would be worse than none.
                  </div>
                </div>
                <div style={{ display: "flex", gap: 8 }}>
                  <button className="btn btn-primary btn-sm" onClick={open} disabled={Boolean(busy)}>Open</button>
                  <button className="btn btn-ghost btn-sm" onClick={() => setOpening(false)}>Cancel</button>
                </div>
              </div>
            )}

            {rooms.length === 0 && !opening && (
              <div style={{ border: "1.5px dashed var(--line-2)", padding: "12px 14px", fontSize: 12.5, color: "var(--tx-3)", lineHeight: 1.6 }}>
                No room is open in this session. Rooms are not listed from the relay — it has no directory to ask,
                by design, since a directory of who meets whom is metadata this product does not need to centralise.
              </div>
            )}

            {rooms.length > 0 && (
              <div style={{ display: "grid", gridTemplateColumns: "260px minmax(0,1fr)", gap: 14, alignItems: "start" }}>
                <div style={{ display: "flex", flexDirection: "column", gap: 10 }}>
                  {rooms.map((rm) => (
                    <div key={rm.id} className="surface" onClick={() => setActive(rm.id)} style={{ padding: "12px 14px", cursor: "pointer", display: "flex", flexDirection: "column", gap: 6, borderLeft: active === rm.id ? "2px solid var(--accent)" : "2px solid transparent" }}>
                      <span style={{ fontSize: 13.5, fontWeight: 500 }}>{rm.name}</span>
                      <div style={{ display: "flex", alignItems: "center", gap: 8 }}>
                        <span className="mono" style={{ fontSize: 8.5, letterSpacing: ".12em", textTransform: "uppercase", color: CLS_COLOR[rm.classification] ?? "var(--tx-3)", border: `1px solid ${CLS_COLOR[rm.classification] ?? "var(--line-2)"}`, padding: "1px 6px" }}>{rm.classification}</span>
                        <span className="mono tabular" style={{ fontSize: 9.5, color: "var(--tx-3)" }}>{rm.members} member{rm.members === 1 ? "" : "s"}{rm.started ? ` · ${rm.started}` : ""}</span>
                      </div>
                      <span className="mono" style={{ fontSize: 8.5, color: "var(--tx-3)", wordBreak: "break-all" }}>mls group {rm.id.slice(0, 16)}…</span>
                    </div>
                  ))}
                  {active && (
                    <div className="surface" style={{ display: "flex", flexDirection: "column" }}>
                      <div style={{ padding: "10px 12px", borderBottom: "1px solid var(--line-1)" }}><span className="eyebrow">Roster</span></div>
                      <div style={{ padding: 6 }}>
                        {roster.map((m) => <RosterRow key={m.id} m={m} />)}
                      </div>
                    </div>
                  )}
                </div>

                <div className="surface" style={{ display: "flex", flexDirection: "column", minHeight: 320 }}>
                  <div style={{ padding: "10px 14px", borderBottom: "1px solid var(--line-1)", display: "flex", alignItems: "center", gap: 10 }}>
                    <span className="eyebrow">Transcript</span>
                    {drainDown === null ? (
                      <span style={{ width: 7, height: 7, borderRadius: 999, background: "var(--accent)", animation: "ccPulse 1.6s infinite" }} />
                    ) : (
                      <span className="mono" style={{ fontSize: 9, color: "var(--warn)" }} title={drainDown}>not live — rooms.events() is failing</span>
                    )}
                    <div style={{ flex: 1 }} />
                    <span className="mono" style={{ fontSize: 9, color: "var(--tx-3)" }}>decrypted in this process</span>
                  </div>
                  <div style={{ flex: 1, overflow: "auto", padding: "12px 16px", display: "flex", flexDirection: "column", gap: 10, maxHeight: 360 }}>
                    {shown.length === 0 && (
                      <span className="mono" style={{ fontSize: 10.5, color: "var(--tx-3)" }}>nothing said yet</span>
                    )}
                    {shown.map((ev) => {
                      if (ev.kind === "system") {
                        return (
                          <div key={ev.n} className="mono" style={{ borderTop: "1px solid var(--line-2)", borderBottom: "1px solid var(--line-2)", padding: "6px 2px", fontSize: 10.5, color: "var(--tx-2)", display: "flex", gap: 10 }}>
                            <span className="tabular" style={{ color: "var(--tx-3)" }}>{ev.t}</span>
                            <span>{ev.text}</span>
                          </div>
                        );
                      }
                      const c = ev.human ? "var(--accent-text)" : "var(--info)";
                      return (
                        <div key={ev.n} style={{ display: "flex", gap: 10 }}>
                          <span className="mono" style={{ fontSize: 9.5, color: c, border: `1px solid ${c}`, padding: "2px 7px", height: "fit-content", flexShrink: 0 }}>{ev.who}</span>
                          <div style={{ minWidth: 0 }}>
                            <p style={{ fontSize: 13, lineHeight: 1.5, margin: 0 }}>{ev.text}</p>
                            <span className="mono tabular" style={{ fontSize: 9, color: "var(--tx-3)" }}>{ev.t}</span>
                          </div>
                        </div>
                      );
                    })}
                  </div>
                  <div style={{ display: "flex", gap: 8, padding: "10px 14px", borderTop: "1px solid var(--line-1)" }}>
                    <input
                      className="input"
                      style={{ flex: 1 }}
                      value={draft}
                      onChange={(e) => setDraft(e.target.value)}
                      onKeyDown={(e) => { if (e.key === "Enter") void say(); }}
                      placeholder={active ? "Message the room" : "Select a room"}
                      disabled={!active}
                    />
                    <button className="btn btn-primary btn-sm" onClick={say} disabled={!active || !draft.trim()}>Send</button>
                  </div>
                </div>
              </div>
            )}

            <div className="mono" style={{ fontSize: 9.5, color: "var(--tx-3)", lineHeight: 1.7 }}>
              rooms.status() / list() / roster() / events() · {status?.note}
            </div>
            <div className="surface" style={{ padding: "12px 14px", display: "flex", flexDirection: "column", gap: 6 }}>
              <span className="eyebrow">Who may be in this room</span>
              <span style={{ fontSize: 12.5, color: "var(--tx-2)", lineHeight: 1.6 }}>
                An <strong>agent</strong> is admitted only if a live capability grant clears it to the room's
                classification (MR-4), and a room's classification never drops once it is open. The operator's own
                clearance is <strong>not verified</strong>: it would be the least of their commercial tier, their
                on-chain clearance (<span className="mono">ClassificationRegistry</span> — deployed, not read yet)
                and their tenant's ceiling (<span className="mono">TenantHierarchy</span> — deployed with no root).
                Until one of those is live, the room's classification is the operator's own declaration, recorded
                as such.
              </span>
            </div>
            <div style={{ border: "1.5px dashed var(--line-2)", padding: "12px 14px", fontSize: 12.5, color: "var(--tx-3)", lineHeight: 1.6 }}>
              Not built, and deliberately not implied anywhere above: <strong>voice</strong> (there is no audio path,
              so nothing here can have been spoken), <strong>a durable transcript</strong> (messages live in memory
              for this session and are never written to disk — encrypted-at-rest storage is the ENCRYPT program's
              work), <strong>AgentSBT-attested seats</strong> (a seat carries the agent id this tenant already has
              evidence about; it claims no on-chain attestation), and <strong>classification-bounded admission</strong>
              (MR-4 monotonicity — a room carries a classification but does not yet refuse a member below it).
            </div>
          </>
        )}
      </Domain>
    </div>
  );
}
