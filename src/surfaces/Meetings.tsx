// citrate-quorum — Meetings surface (QRM-S2D). Demo beat 3: minutes as
// evidence. Ported from design §MEETINGS. Charter register. List (planned +
// ratified, templates) and the detail document (agenda frozen at open, minutes,
// dissent first-class, attendance attested, decisions) with the Ratify ceremony.
// Reads bridge.meetings.list()/get(); ratify routes through useCeremony().
import { useState } from "react";
import { bridge } from "../bridge";
import { DomainErrorPlate, useDomain } from "../components/DomainState";
import type { Classification, Meeting, MeetingDetail, MeetingTemplate } from "../bridge";
import { useCeremony } from "../ceremony/Ceremony";

const CLS_COLOR: Record<string, string> = { Public: "var(--z-silver)", Proprietary: "var(--info)", CUI: "var(--warn)", ITAR: "var(--danger)" };
const STATE_COLOR: Record<string, string> = {
  scheduled: "var(--tx-3)", "in-progress": "var(--accent)", awaiting: "var(--warn)", ratified: "var(--ok)", inquorate: "var(--danger)",
};
const STATE_LABEL: Record<string, string> = {
  scheduled: "scheduled", "in-progress": "in progress", awaiting: "awaiting ratification", ratified: "ratified", inquorate: "inquorate",
};

/**
 * The templates an operator can schedule from.
 *
 * `minHumans` is DATA, not a caption: it is sent to the backend and becomes the
 * quorum rule enforced at close. These used to be display-only chips whose
 * captions ("5 signers", "3 humans") described a rule nothing implemented, so
 * the words and the behaviour could not disagree because the behaviour did not
 * exist. Now they cannot disagree because they are the same number.
 */
const TEMPLATES: MeetingTemplate[] = [
  { name: "Standup", minHumans: 2, classification: "Proprietary", note: "2 humans · agents optional" },
  { name: "Change-control", minHumans: 5, classification: "CUI", note: "5 signers · CUI" },
  { name: "Incident review", minHumans: 3, classification: "Proprietary", note: "3 humans · postmortem" },
  { name: "Quarterly governance", minHumans: 7, classification: "CUI", note: "board seat required" },
];

export function Meetings() {
  const [detail, setDetail] = useState<MeetingDetail | null>(null);
  const [selectedState, setSelectedState] = useState<string>("ratified");
  const [ratifyError, setRatifyError] = useState("");
  const [operator, setOperator] = useState("");
  const [formOpen, setFormOpen] = useState(false);
  const [form, setForm] = useState({
    name: "", when: "", template: TEMPLATES[0].name, workspace: "",
  });
  const [formError, setFormError] = useState("");
  const [admitName, setAdmitName] = useState("");
  const [admitVendor, setAdmitVendor] = useState("");
  const [admitClearance, setAdmitClearance] = useState<Classification>("CUI");
  const [actionError, setActionError] = useState("");
  const ceremony = useCeremony();

  // Ratification must name the human who signed it; the backend refuses an
  // anonymous ratifier outright.
  useEffect(() => {
    bridge.session.operator().then((o) => setOperator(o ?? "")).catch(() => {});
  }, []);


  // Honest failure (S2D.4/§5.1): this surface's primary read is meetings.list().
  // A read that cannot succeed must say so and offer a retry, not sit in a
  // loading state forever.
  const primary = useDomain(() => bridge.meetings.list(), "meetings.list()");
  // Derived, not mirrored (see Agents.tsx).
  const meetings = primary.state.status === "ready" ? primary.state.data : [];
  if (primary.state.status === "error") {
    return (
      <div style={{ padding: 18 }}>
        <DomainErrorPlate source="meetings.list()" error={primary.state.error} onRetry={primary.retry} lands="It lands in QRM-S5 (meetings + minutes)." />
      </div>
    );
  }

  const open = (m: Meeting) => {
    setSelectedState(m.state);
    bridge.meetings.get(m.id).then(setDetail).catch(() => {});
  };

  /** Re-read the register and, if a meeting is open, its detail + state. */
  const refresh = async (id?: string) => {
    const rows = await bridge.meetings.list();
    setMeetings(rows);
    if (id) {
      const row = rows.find((r) => r.id === id);
      if (row) setSelectedState(row.state);
      setDetail(await bridge.meetings.get(id));
    }
  };

  const schedule = async () => {
    setFormError("");
    const tpl = TEMPLATES.find((x) => x.name === form.template) ?? TEMPLATES[0];
    if (!form.name.trim() || !form.when.trim()) {
      setFormError("a meeting needs a name and a time");
      return;
    }
    try {
      await bridge.meetings.schedule({
        // The id must identify one meeting in this tenant; the backend refuses
        // a duplicate. Derived from the time it was created rather than the
        // name, so scheduling two standups does not collide.
        id: `m-${Date.now().toString(36)}`,
        name: form.name.trim(),
        when: form.when.trim(),
        template: tpl.name,
        minHumans: tpl.minHumans,
        classification: tpl.classification,
        // Empty means no source: the backend records an empty agenda that says
        // it was not generated, rather than inventing items.
        workspace: form.workspace.trim() || undefined,
      });
      setForm({ name: "", when: "", template: TEMPLATES[0].name, workspace: "" });
      setFormOpen(false);
      await refresh();
    } catch (e) {
      setFormError(e instanceof Error ? e.message : String(e));
    }
  };

  const admit = async () => {
    if (!detail || !admitName.trim()) return;
    setActionError("");
    try {
      await bridge.meetings.admit({
        id: detail.id,
        name: admitName.trim(),
        vendor: admitVendor.trim() || undefined,
        // Attested, because this IS the attestation: an operator recording
        // someone as present. An unattested row would not count for quorum or
        // for MR-4, which is a different act than the one this button performs.
        attested: true,
        clearance: admitClearance,
      });
      setAdmitName("");
      setAdmitVendor("");
      await refresh(detail.id);
    } catch (e) {
      // MR-4 refusals land here and MUST be shown — an under-cleared attendee
      // silently dropped would leave the operator believing they were admitted.
      setActionError(e instanceof Error ? e.message : String(e));
    }
  };

  const lifecycle = async (step: "open" | "close") => {
    if (!detail) return;
    setActionError("");
    try {
      if (step === "open") await bridge.meetings.open(detail.id);
      else await bridge.meetings.close(detail.id);
      await refresh(detail.id);
    } catch (e) {
      setActionError(e instanceof Error ? e.message : String(e));
    }
  };

  const ratify = async () => {
    if (!detail) return;
    setRatifyError("");
    // The hash the signer is shown must be the hash the backend records
    // against. Read it first, display it, and pass the SAME value back — if the
    // minutes move in between, the backend refuses rather than recording a
    // signature over something nobody saw.
    let contentHash: string;
    try {
      contentHash = await bridge.meetings.contentHash(detail.id);
    } catch (e) {
      setRatifyError(e instanceof Error ? e.message : String(e));
      return;
    }

    const r = await ceremony.request({
      kind: "ratify",
      title: `Ratify minutes — ${detail.name}`,
      origin: "user",
      action: { actionClass: "meeting.ratify", classification: detail.classification, agent: "user", mandatoryHic1: true },
      rows: [
        { k: "Meeting", v: detail.name },
        { k: "When", v: detail.when },
        { k: "Agenda hash", v: detail.agendaHash },
        // Counted, not asserted. This row read "1 decision · 1 dissent
        // recorded" as a literal, which was true of the design mock and of
        // nothing else.
        { k: "Minutes", v: `${detail.minutes.length} items · ${detail.decisions.length} decisions · ${detail.dissent.length} dissent recorded` },
        { k: "You are signing", v: contentHash },
        // The old row promised "anchors the minutes on chain". Nothing anchors
        // anything yet — AnchorRegistry is not resolvable in this build — and a
        // ceremony that misstates its own effect is the worst place to be
        // imprecise.
        { k: "Effect", v: "records your signature over the hash above, in this tenant's evidence chain. It does NOT anchor on chain — see the anchor row." },
      ],
      // The write runs INSIDE the ceremony, before it can claim to be on
      // record. Doing it after `request()` resolved meant it ran on dismiss —
      // after the dialog had already said the record existed.
      commit: async () => {
        await bridge.meetings.ratify(detail.id, operator, contentHash);
        return "ratified · minutes hash-chained locally, not anchored on chain";
      },
    });
    if (r.outcome !== "settled") return;
    // Re-read rather than setting a local flag: the backend is the record, and
    // a surface that decides for itself that it is ratified is exactly the
    // "stamped On record while the write failed" bug from S4.
    try {
      await refresh(detail.id);
    } catch (e) {
      setRatifyError(e instanceof Error ? e.message : String(e));
    }
  };

  if (!detail) {
    return (
      <div style={{ padding: "22px 26px", display: "flex", flexDirection: "column", gap: 16, maxWidth: 1080 }}>
        <div style={{ display: "flex", alignItems: "center", gap: 12 }}>
          <span className="eyebrow">The record — every meeting, planned and ratified</span>
          <div style={{ flex: 1 }} />
          <button className="btn btn-primary btn-sm" onClick={() => setFormOpen((v) => !v)}>
            {formOpen ? "Cancel" : "Schedule a meeting"}
          </button>
        </div>

        {formOpen && (
          <div className="surface" style={{ padding: 16, display: "flex", flexDirection: "column", gap: 12 }}>
            <span className="eyebrow">Schedule — the template sets the quorum rule</span>
            <div style={{ display: "grid", gridTemplateColumns: "1fr 1fr", gap: 10 }}>
              <label style={{ display: "flex", flexDirection: "column", gap: 4 }}>
                <span className="mono" style={{ fontSize: 9, letterSpacing: ".1em", textTransform: "uppercase", color: "var(--tx-3)" }}>Name</span>
                <input value={form.name} onChange={(e) => setForm((f) => ({ ...f, name: e.target.value }))}
                  placeholder="Weekly Standup" style={{ padding: "7px 9px", fontSize: 13 }} />
              </label>
              <label style={{ display: "flex", flexDirection: "column", gap: 4 }}>
                <span className="mono" style={{ fontSize: 9, letterSpacing: ".1em", textTransform: "uppercase", color: "var(--tx-3)" }}>When (RFC3339)</span>
                <input value={form.when} onChange={(e) => setForm((f) => ({ ...f, when: e.target.value }))}
                  placeholder="2026-07-30T09:00:00Z" style={{ padding: "7px 9px", fontSize: 13 }} />
              </label>
            </div>
            <div style={{ display: "flex", flexDirection: "column", gap: 6 }}>
              <span className="mono" style={{ fontSize: 9, letterSpacing: ".1em", textTransform: "uppercase", color: "var(--tx-3)" }}>Template</span>
              <div style={{ display: "flex", gap: 8, flexWrap: "wrap" }}>
                {TEMPLATES.map((tpl) => {
                  const on = form.template === tpl.name;
                  return (
                    <button key={tpl.name} onClick={() => setForm((f) => ({ ...f, template: tpl.name }))}
                      className="mono" style={{ fontSize: 10, cursor: "pointer", background: "transparent", color: on ? "var(--tx-1)" : "var(--tx-2)", border: `1px solid ${on ? "var(--accent)" : "var(--line-2)"}`, padding: "5px 10px" }}>
                      {tpl.name} <span style={{ color: "var(--tx-3)" }}>· {tpl.note} · {tpl.classification}</span>
                    </button>
                  );
                })}
              </div>
            </div>
            <label style={{ display: "flex", flexDirection: "column", gap: 4 }}>
              <span className="mono" style={{ fontSize: 9, letterSpacing: ".1em", textTransform: "uppercase", color: "var(--tx-3)" }}>Workspace (optional) — the agenda is generated from its active sprint files</span>
              <input value={form.workspace} onChange={(e) => setForm((f) => ({ ...f, workspace: e.target.value }))}
                placeholder="/home/you/Projects/citrate-quorum" className="mono" style={{ padding: "7px 9px", fontSize: 12 }} />
              <span className="mono" style={{ fontSize: 9.5, color: "var(--tx-3)", lineHeight: 1.5 }}>
                Left empty, the meeting opens with an empty agenda that says it was not generated — never with invented items.
              </span>
            </label>
            <div style={{ display: "flex", alignItems: "center", gap: 10 }}>
              <button className="btn btn-primary btn-sm" onClick={schedule}>Schedule</button>
              {formError && <span className="mono" style={{ fontSize: 11, color: "var(--danger)" }}>{formError}</span>}
            </div>
          </div>
        )}
        <div className="surface" style={{ display: "flex", flexDirection: "column" }}>
          <div className="mono" style={{ display: "grid", gridTemplateColumns: "1fr 150px 130px 110px 120px 150px", gap: 10, padding: "8px 16px", fontSize: 9, letterSpacing: ".12em", textTransform: "uppercase", color: "var(--tx-3)", borderBottom: "1px solid var(--line-1)" }}>
            <span>Meeting</span><span>When</span><span>Template</span><span>Attendees</span><span>Class</span><span>State</span>
          </div>
          {meetings.map((m) => (
            <div key={m.id} onClick={() => open(m)} style={{ display: "grid", gridTemplateColumns: "1fr 150px 130px 110px 120px 150px", gap: 10, padding: "10px 16px", borderBottom: "1px solid var(--line-1)", cursor: "pointer", alignItems: "center" }}>
              <span style={{ fontSize: 13, fontWeight: 500 }}>{m.name}</span>
              <span className="mono tabular" style={{ fontSize: 10.5, color: "var(--tx-2)" }}>{m.when}</span>
              <span style={{ fontSize: 12, color: "var(--tx-2)" }}>{m.tpl}</span>
              <span className="mono tabular" style={{ fontSize: 10.5, color: "var(--tx-2)" }}>{m.humans}h · {m.agents}a</span>
              <span className="mono" style={{ fontSize: 8.5, letterSpacing: ".1em", textTransform: "uppercase", color: CLS_COLOR[m.classification], border: `1px solid ${CLS_COLOR[m.classification]}`, padding: "1px 6px", width: "fit-content" }}>{m.classification}</span>
              <span className="mono" style={{ fontSize: 9, letterSpacing: ".08em", textTransform: "uppercase", color: STATE_COLOR[m.state] }}>{STATE_LABEL[m.state]}</span>
            </div>
          ))}
          {meetings.length === 0 && (
            <div className="mono" style={{ fontSize: 11, lineHeight: 1.6, color: "var(--tx-3)", padding: "14px 16px" }}>
              No meetings in this tenant yet. This register is empty because nothing has been
              scheduled — not because a read failed.
            </div>
          )}
        </div>
        {/* Rule 11: this line names where the rows actually come from. It used
            to claim "MinutesRegistry + relay archive" — neither of which
            exists. MinutesRegistry is a QRM-S6 contract that is not written,
            and there is no relay archive until Rooms (QRM-S3). */}
        <div className="mono" style={{ fontSize: 9.5, color: "var(--tx-3)", lineHeight: 1.6 }}>
          Read from meetings.list() → this tenant's local meeting store. Minutes are hash-chained
          locally and verifiable offline; they are not on a chain — MeetingRegistry and
          AnchorRegistry land with the governance contracts (QRM-S6).
        </div>
      </div>
    );
  }

  // Derived from the RECORD, not from a local flag. The flag was set at open
  // and never updated by a successful ratification, so the document went on
  // showing "Awaiting ratification" after it had been signed — the surface
  // disagreeing with the evidence it is supposed to display.
  const isRatified = detail.ratified || selectedState === "ratified";
  const isUnratified = !isRatified && (selectedState === "awaiting" || selectedState === "in-progress");

  return (
    <div style={{ padding: "22px 26px", display: "flex", flexDirection: "column", maxWidth: 1080 }}>
      <a href="#/meetings" onClick={(e) => { e.preventDefault(); setDetail(null); }} className="mono" style={{ fontSize: 10, letterSpacing: ".1em", textTransform: "uppercase", marginBottom: 14 }}>← All meetings</a>
      <div className="surface" style={{ padding: 0, borderTop: "2px solid var(--line-strong)" }}>
        <div style={{ padding: "22px 26px", borderBottom: "1px solid var(--line-1)", display: "flex", flexDirection: "column", gap: 10 }}>
          <div style={{ display: "flex", alignItems: "center", gap: 12 }}>
            <div style={{ fontFamily: "var(--font-display)", fontWeight: 440, fontSize: 24, letterSpacing: "-0.012em" }}>{detail.name}</div>
            <div style={{ flex: 1 }} />
            {isRatified && <span className="cc-stamp mono" style={{ fontSize: 9.5, letterSpacing: ".12em", textTransform: "uppercase", color: "var(--ok)", border: "1.5px solid var(--ok)", padding: "4px 10px", transform: "rotate(-2deg)" }}>Ratified</span>}
            {isUnratified && <span className="mono" style={{ fontSize: 9.5, letterSpacing: ".12em", textTransform: "uppercase", color: "var(--warn)", border: "1px solid var(--warn)", padding: "4px 10px" }}>Awaiting ratification</span>}
          </div>
          <div className="mono" style={{ fontSize: 10.5, color: "var(--tx-2)", display: "flex", gap: 16, flexWrap: "wrap" }}>
            <span>{detail.when}</span><span>{detail.tenant}</span><span style={{ color: CLS_COLOR[detail.classification] }}>{detail.classification.toUpperCase()}</span>
          </div>
          <div style={{ display: "flex", alignItems: "center", gap: 10 }}>
            <span className="mono" style={{ fontSize: 10, color: "var(--tx-3)" }}>agenda {detail.agendaHash}</span>
            {isRatified && <span className="mono" style={{ fontSize: 10, color: "var(--tx-3)" }}>anchor: {detail.anchor ?? "unknown"}</span>}
          </div>
        </div>
        <div style={{ display: "grid", gridTemplateColumns: "1fr 300px" }}>
          <div style={{ padding: "20px 26px", display: "flex", flexDirection: "column", gap: 18, borderRight: "1px solid var(--line-1)" }}>
            <div>
              <div className="lbl">Agenda — frozen at open</div>
              {detail.agenda.map((a) => (
                <div key={a.n} style={{ display: "flex", gap: 10, padding: "6px 0", borderBottom: "1px solid var(--line-1)", alignItems: "baseline" }}>
                  <span className="mono tabular" style={{ fontSize: 10, color: "var(--tx-3)" }}>{a.n}</span>
                  <span style={{ fontSize: 13, flex: 1 }}>{a.text}</span>
                  <span className="mono" style={{ fontSize: 9, color: "var(--tx-3)" }}>{a.src}</span>
                </div>
              ))}
              {/* This used to read "A changed agenda creates a linked successor
                  meeting — this one has none." There is no successor-meeting
                  feature; the sentence described behaviour that does not exist
                  anywhere in the product. What IS true is the freeze rule. */}
              <div className="mono" style={{ fontSize: 9, color: "var(--tx-3)", marginTop: 6, lineHeight: 1.6 }}>
                {detail.agendaHash.startsWith("not frozen")
                  ? "Not frozen yet — opening the meeting commits this agenda under a BLAKE3 hash."
                  : "Frozen at open. Recompute this hash from the items above to verify it offline."}
              </div>
            </div>
            <div>
              <div className="lbl">Minutes</div>
              {detail.minutes.map((m, i) => (<p key={i} style={{ fontSize: 13.5, lineHeight: 1.6, margin: "0 0 8px" }}>{m}</p>))}
            </div>
            {detail.dissent.length > 0 && (
              <div>
                <div className="lbl">Dissent — first-class in the record</div>
                {detail.dissent.map((d, i) => (
                  <div key={i} style={{ borderLeft: "2px solid var(--warn)", padding: "6px 12px" }}>
                    <span className="mono" style={{ fontSize: 10, color: "var(--z-amber)" }}>{d.who}</span>
                    <p style={{ fontSize: 13, lineHeight: 1.5, margin: "3px 0 0", color: "var(--tx-2)" }}>{d.text}</p>
                  </div>
                ))}
              </div>
            )}
            {/* The lifecycle, driven from the UI. Each button is shown only in
                the state where it is legal — the backend enforces the same
                transitions, so a visible-but-doomed control would be a lie
                about what the record permits. */}
            {(selectedState === "scheduled" || selectedState === "in-progress") && (
              <div style={{ display: "flex", alignItems: "center", gap: 10, border: "1px solid var(--line-2)", padding: "12px 14px" }}>
                <span style={{ fontSize: 13, flex: 1 }}>
                  {selectedState === "scheduled"
                    ? "Opening freezes the agenda under a BLAKE3 hash. It cannot change afterwards."
                    : "Closing composes the minutes from the governed actions recorded since it opened, and decides quorum."}
                </span>
                <button className="btn btn-primary btn-sm" onClick={() => lifecycle(selectedState === "scheduled" ? "open" : "close")}>
                  {selectedState === "scheduled" ? "Open — freeze the agenda" : "Close — compose minutes"}
                </button>
              </div>
            )}
            {actionError && (
              <div className="mono" style={{ fontSize: 11, lineHeight: 1.5, color: "var(--danger)", border: "1px solid var(--danger)", padding: "10px 12px" }}>
                refused — {actionError}
              </div>
            )}
            {isUnratified && (
              <div style={{ display: "flex", alignItems: "center", gap: 12, border: "1px solid var(--warn)", background: "var(--warn-bg)", padding: "12px 14px" }}>
                <span style={{ fontSize: 13, flex: 1 }}>These minutes are a draft. Ratifying records your signature over their hash in this tenant's evidence chain, which a board member can verify offline. It does not anchor them on chain yet — that lands with the governance contracts.</span>
                <button className="btn btn-primary" onClick={ratify}>Ratify — sign</button>
              </div>
            )}
            {/* A refused ratification must say so. The S4 bug this guards
                against stamped "On record" while the write had failed. */}
            {ratifyError && (
              <div className="mono" style={{ fontSize: 11, lineHeight: 1.5, color: "var(--danger)", border: "1px solid var(--danger)", padding: "10px 12px" }}>
                ratification refused — {ratifyError}
              </div>
            )}
          </div>
          <div style={{ padding: "20px 22px", display: "flex", flexDirection: "column", gap: 16, background: "var(--srf-1)" }}>
            <div>
              <div className="lbl">Attendance — attested</div>
              {detail.attendance.map((at) => (
                <div key={at.name} style={{ display: "flex", alignItems: "center", gap: 8, padding: "4px 0" }}>
                  <span style={{ width: 7, height: 7, borderRadius: 999, background: at.attested ? "var(--ok)" : "var(--tx-3)", flexShrink: 0 }} />
                  <span style={{ fontSize: 12.5, flex: 1, color: at.attested ? "var(--tx-1)" : "var(--tx-3)" }}>{at.name}</span>
                  <span className="mono" style={{ fontSize: 8.5, color: "var(--tx-3)" }}>{at.agent ?? (at.note ? "observer" : "human")}</span>
                </div>
              ))}
              {detail.attendance.length === 0 && (
                <span className="mono" style={{ fontSize: 10.5, color: "var(--tx-3)" }}>nobody admitted yet</span>
              )}

              {/* Admission closes when the meeting does — the backend refuses
                  it, and offering a control that can only fail is worse than
                  not offering it. */}
              {(selectedState === "scheduled" || selectedState === "in-progress") && (
                <div style={{ display: "flex", flexDirection: "column", gap: 6, marginTop: 10, paddingTop: 10, borderTop: "1px solid var(--line-1)" }}>
                  <span className="mono" style={{ fontSize: 9, letterSpacing: ".1em", textTransform: "uppercase", color: "var(--tx-3)" }}>Attest an attendee</span>
                  <input value={admitName} onChange={(e) => setAdmitName(e.target.value)} placeholder="name" style={{ padding: "6px 8px", fontSize: 12 }} />
                  <input value={admitVendor} onChange={(e) => setAdmitVendor(e.target.value)} placeholder="vendor (leave empty for a human)" className="mono" style={{ padding: "6px 8px", fontSize: 11 }} />
                  <select value={admitClearance} onChange={(e) => setAdmitClearance(e.target.value as Classification)} className="mono" style={{ padding: "6px 8px", fontSize: 11 }}>
                    {(["Public", "Proprietary", "CUI", "ITAR"] as Classification[]).map((c) => (
                      <option key={c} value={c}>cleared to {c}</option>
                    ))}
                  </select>
                  <button className="btn btn-ghost btn-sm" onClick={admit} disabled={!admitName.trim()}>Attest present</button>
                  <span className="mono" style={{ fontSize: 9.5, color: "var(--tx-3)", lineHeight: 1.5 }}>
                    Only humans count toward quorum ({detail.attendance.filter((a) => !a.agent).length} of {TEMPLATES.find((x) => x.name === (meetings.find((m) => m.id === detail.id)?.tpl ?? ""))?.minHumans ?? "?"} required).
                    Someone cleared below {detail.classification} is refused — MR-4.
                  </span>
                </div>
              )}
            </div>
            <div>
              <div className="lbl">Decisions</div>
              {detail.decisions.map((d) => (
                <div key={d.id} style={{ display: "flex", gap: 8, padding: "5px 0", alignItems: "baseline" }}>
                  <span className="mono" style={{ fontSize: 10, color: "var(--accent-text)" }}>{d.id}</span>
                  <span style={{ fontSize: 12, color: "var(--tx-2)" }}>{d.text}</span>
                </div>
              ))}
            </div>
            {/* Rule 11 again: this claimed "MinutesRegistry", a QRM-S6
                contract that is not written. The detail comes from the same
                local store as the register. */}
            <div className="mono" style={{ fontSize: 9, color: "var(--tx-3)", borderTop: "1px solid var(--line-1)", paddingTop: 10, lineHeight: 1.6 }}>Read from meetings.get() → this tenant's local meeting store · print-ready under @media print</div>
          </div>
        </div>
      </div>
    </div>
  );
}
