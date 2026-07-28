// citrate-quorum — Governance surface (QRM-S2D). Demo beat 1: docs → law.
// Ported from design/CitrateQuorum.dc.html §GOVERNANCE. Three tabs: the 8-step
// authoring Pipeline (Ingest → Interview → Spec → Compile → Simulate → Ceremony
// → Deploy → Bind), the Protocols table, and the Templates catalog. The deploy
// step routes through the real ceremony (2-of-3, CREATE2 shown pre-sign). Reads
// bridge.governance.*.
import { useEffect, useMemo, useState } from "react";
import { bridge } from "../bridge";
import { DomainErrorPlate, useDomain } from "../components/DomainState";
import type {
  IngestFile, InterviewTurn, Simulation, SpecClause,
} from "../bridge";
import { useCeremony } from "../ceremony/Ceremony";
import { LoaderMark } from "../components/LoaderMark";

type Tab = "pipe" | "prot" | "tpl";
const STEPS = ["Ingest", "Interview", "Spec", "Compile", "Simulate", "Ceremony", "Deploy", "Bind"];
const CLS_COLOR: Record<string, string> = { Public: "var(--z-silver)", Proprietary: "var(--info)", CUI: "var(--warn)", ITAR: "var(--danger)" };

export function Governance() {
  const [tab, setTab] = useState<Tab>("pipe");
  const [step, setStep] = useState(1);
  const [ingest, setIngest] = useState<IngestFile[]>([]);
  const [interview, setInterview] = useState<InterviewTurn[]>([]);
  const [clauses, setClauses] = useState<SpecClause[]>([]);
  const [sim, setSim] = useState<Simulation | null>(null);
  const [simState, setSimState] = useState<"idle" | "running" | "done">("idle");
  const [deployed, setDeployed] = useState(false);
  const [specId, setSpecId] = useState<string | null>(null);
  const ceremony = useCeremony();

  useEffect(() => {
    // QRM-S7: the pipeline is per-spec, so the surface picks up the newest
    // draft and reads that. With no drafts these stay empty and each step
    // renders its own empty state rather than a half-populated wizard.
    bridge.governance
      .specs()
      .then((list) => {
        const first = list[0];
        if (!first) return;
        setSpecId(first.id);
        bridge.governance.spec(first.id).then((sp) => {
          setIngest(sp.files);
          setClauses(sp.clauses);
        }).catch(() => {});
        bridge.governance.interview(first.id).then((iv) => setInterview(iv.turns)).catch(() => {});
      })
      .catch(() => {});
    
  }, []);

  // Honest failure (S2D.4/§5.1): this surface's primary read is governance.protocols().
  // A read that cannot succeed must say so and offer a retry, not sit in a
  // loading state forever.
  const primary = useDomain(() => bridge.governance.protocols(), "governance.protocols()");
  // Derived, not mirrored (see Agents.tsx).
  const protocols = primary.state.status === "ready" ? primary.state.data : [];

  const runSim = () => {
    setSimState("running");
    bridge.governance
      .simulate(specId ?? "", { from: "", to: "" })
      .then((s) => { setSim(s); setSimState("done"); })
      .catch(() => setSimState("idle"));
  };

  const openDeploy = async () => {
    const create2 = sim?.create2 ?? "0x9E44d0A17c33B8e2f1a6C90dD24b7E80f1532Aa7";
    const r = await ceremony.request({
      kind: "deploy",
      title: "Deploy SPEC-104 — Line-4 Operating Envelope v4",
      origin: "user",
      // Deploying a governance protocol is chain state: always HIC-1.
      action: { actionClass: "protocol.deploy", classification: "CUI", agent: "user", mandatoryHic1: true },
      create2,
      threshold: 2,
      signers: [{ name: "M. Okonkwo", signed: false }, { name: "J. Whitfield (export-control)", signed: false }],
      rows: [
        { k: "Template", v: "BoundedAutonomy v2.1 · audited" },
        { k: "Spec CID", v: "bafy…104e (SPEC-104)" },
        { k: "Governs", v: "repo.write · pr.open · ci.* · doc.draft · calendar.write · spend" },
        { k: "Tenant", v: "Meridian Aero › … › Line-4 Automation" },
        { k: "Timelock", v: "14 days before effect" },
      ],
    });
    if (r.outcome === "settled") { setDeployed(true); setStep(7); }
  };

  const tabBtn = (t: Tab, label: string) => {
    const on = tab === t;
    return (
      <button key={t} onClick={() => setTab(t)} className="mono" style={{ fontSize: 10, letterSpacing: ".13em", textTransform: "uppercase", background: "transparent", border: "none", cursor: "pointer", padding: "8px 14px", color: on ? "var(--tx-1)" : "var(--tx-3)", borderBottom: `2px solid ${on ? "var(--accent)" : "transparent"}`, marginBottom: -1 }}>{label}</button>
    );
  };

  const summary = useMemo(() => [
    ["Principals", "R. Ortiz (CAIO) · M. Okonkwo (doc/schedule)"],
    ["Spend ceiling", "150 SALT → ceremony (HIC-1)"],
    ["Model egress", "CUI+ local-model only"],
    ["Grant expiry", "45-day max · renewal is a ceremony"],
    ["Escalation", "in-room → CCB after 24h"],
    ["Exceptions", "C4 retaliation — unmapped, T-request filed"],
  ] as [string, string][], []);

  // Placed after EVERY hook: an early return above a useMemo makes the
  // hook run conditionally, and React crashes with "rendered fewer hooks
  // than expected" the moment this read fails.
  if (primary.state.status === "error") {
    return (
      <div style={{ padding: 18 }}>
        <DomainErrorPlate source="governance.protocols()" error={primary.state.error} onRetry={primary.retry} lands="It lands in QRM-S7 (authoring pipeline), on the contracts from QRM-S6." />
      </div>
    );
  }

  return (
    <div style={{ padding: "22px 26px", display: "flex", flexDirection: "column", gap: 16, maxWidth: 1120 }}>
      <div style={{ display: "flex", borderBottom: "1px solid var(--line-2)" }}>
        {tabBtn("pipe", "Pipeline")}{tabBtn("prot", "Protocols")}{tabBtn("tpl", "Templates")}
      </div>

      {tab === "pipe" && (
        <div style={{ display: "flex", flexDirection: "column", gap: 16 }}>
          {/* step rail */}
          <div style={{ display: "flex" }}>
            {STEPS.map((label, i) => {
              const n = i + 1; const active = n === step; const done = n < step;
              const color = active ? "var(--accent-text)" : done ? "var(--ok)" : "var(--tx-3)";
              return (
                <div key={label} onClick={() => n <= step && setStep(n)} style={{ flex: 1, display: "flex", flexDirection: "column", gap: 4, cursor: n <= step ? "pointer" : "default", padding: "6px 8px", borderTop: `2px solid ${active ? "var(--accent)" : done ? "var(--ok)" : "var(--line-2)"}` }}>
                  <span className="mono tabular" style={{ fontSize: 9, color }}>{String(n).padStart(2, "0")}</span>
                  <span className="mono" style={{ fontSize: 9, letterSpacing: ".1em", textTransform: "uppercase", color }}>{label}</span>
                </div>
              );
            })}
          </div>

          {step === 1 && (
            <div style={{ display: "flex", flexDirection: "column", gap: 12 }}>
              <div style={{ fontFamily: "var(--font-display)", fontWeight: 440, fontSize: 22 }}>A stack of documents becomes law</div>
              <div style={{ border: "1.5px dashed var(--line-2)", padding: 26, textAlign: "center", color: "var(--tx-3)", fontSize: 13 }}>Drop board resolutions, legal policy, data exports — PDF, DOCX, MD, CSV, email</div>
              <div className="surface" style={{ display: "flex", flexDirection: "column" }}>
                {ingest.map((f) => (
                  <div key={f.name} style={{ display: "flex", alignItems: "center", gap: 12, padding: "10px 14px", borderBottom: "1px solid var(--line-1)" }}>
                    <span style={{ fontSize: 13, fontWeight: 500, minWidth: 0, flex: 1 }}>{f.name} <span className="mono" style={{ fontSize: 9.5, color: "var(--tx-3)" }}>{f.size}</span></span>
                    <span className="mono" style={{ fontSize: 8.5, letterSpacing: ".1em", textTransform: "uppercase", color: CLS_COLOR[f.class], border: `1px solid ${CLS_COLOR[f.class]}`, padding: "1px 6px" }}>{f.class}</span>
                    <span className="mono" style={{ fontSize: 9.5, color: f.class === "ITAR" ? "var(--danger)" : "var(--tx-3)" }}>{f.note}</span>
                    <span className="mono" style={{ fontSize: 9, color: "var(--tx-3)" }}>{f.prov}</span>
                  </div>
                ))}
              </div>
              <div style={{ display: "flex" }}><div style={{ flex: 1 }} /><button className="btn btn-primary" onClick={() => setStep(2)}>Continue to interview</button></div>
            </div>
          )}

          {step === 2 && (
            <div style={{ display: "grid", gridTemplateColumns: "1fr 340px", gap: 16 }}>
              <div className="surface" style={{ display: "flex", flexDirection: "column" }}>
                <div className="eyebrow" style={{ padding: "10px 14px", borderBottom: "1px solid var(--line-1)" }}>Interview — the harness asks, you answer</div>
                <div style={{ padding: 14, display: "flex", flexDirection: "column", gap: 12 }}>
                  {interview.map((iv, i) => (
                    <div key={i} style={{ display: "flex", flexDirection: "column", gap: 5 }}>
                      <div style={{ display: "flex", gap: 8 }}><span className="mono" style={{ fontSize: 9, color: "var(--info)", border: "1px solid var(--info)", padding: "1px 6px", height: "fit-content", flexShrink: 0 }}>HARNESS</span><span style={{ fontSize: 13, color: "var(--tx-2)" }}>{iv.q}</span></div>
                      <div style={{ display: "flex", gap: 8 }}><span className="mono" style={{ fontSize: 9, color: "var(--accent-text)", border: "1px solid var(--accent-text)", padding: "1px 6px", height: "fit-content", flexShrink: 0 }}>R. ORTIZ</span><span style={{ fontSize: 13 }}>{iv.a}</span></div>
                    </div>
                  ))}
                </div>
              </div>
              <div className="surface" style={{ display: "flex", flexDirection: "column", height: "fit-content", borderTop: "2px solid var(--line-strong)" }}>
                <div className="eyebrow" style={{ padding: "10px 14px", borderBottom: "1px solid var(--line-1)" }}>Structured summary — editable, the artifact</div>
                <div style={{ padding: 14, display: "flex", flexDirection: "column", gap: 8 }}>
                  {summary.map(([k, v]) => (
                    <div key={k} style={{ display: "grid", gridTemplateColumns: "110px 1fr", gap: 8, borderBottom: "1px solid var(--line-1)", paddingBottom: 6 }}>
                      <span className="mono" style={{ fontSize: 9, letterSpacing: ".1em", textTransform: "uppercase", color: "var(--tx-3)" }}>{k}</span>
                      <span style={{ fontSize: 12 }}>{v}</span>
                    </div>
                  ))}
                </div>
                <div style={{ padding: "10px 14px" }}><button className="btn btn-primary btn-sm" onClick={() => setStep(3)} style={{ width: "100%" }}>Draft the spec →</button></div>
              </div>
            </div>
          )}

          {step === 3 && (
            <div style={{ display: "flex", flexDirection: "column", gap: 12 }}>
              <div style={{ display: "flex", alignItems: "center", gap: 12 }}>
                <div style={{ fontFamily: "var(--font-display)", fontWeight: 440, fontSize: 20 }}>SPEC-104 — Line-4 Operating Envelope v4</div>
                <span className="mono" style={{ fontSize: 9, color: "var(--warn)", border: "1px solid var(--warn)", padding: "2px 7px" }}>1 CLAUSE UNMAPPED — DEPLOY BLOCKED</span>
              </div>
              <div style={{ border: "1px solid var(--line-2)" }}>
                <div style={{ display: "grid", gridTemplateColumns: "1fr 1fr", borderBottom: "1px solid var(--line-2)" }}>
                  <div className="mono" style={{ fontSize: 9, letterSpacing: ".13em", textTransform: "uppercase", color: "var(--tx-3)", padding: "8px 14px", borderRight: "1px solid var(--line-2)" }}>Plain English — what counsel reads</div>
                  <div className="mono" style={{ fontSize: 9, letterSpacing: ".13em", textTransform: "uppercase", color: "var(--tx-3)", padding: "8px 14px" }}>Machine-checkable — what the chain enforces</div>
                </div>
                {clauses.map((cl) => (
                  <div key={cl.n} style={{ display: "grid", gridTemplateColumns: "1fr 1fr", borderBottom: "1px solid var(--line-1)", background: cl.ok ? "transparent" : "var(--warn-bg)" }}>
                    <div style={{ padding: "11px 14px", borderRight: "1px solid var(--line-2)" }}>
                      <p style={{ fontSize: 13, lineHeight: 1.55, margin: 0 }}><span className="mono" style={{ color: cl.ok ? "var(--ok)" : "var(--warn)", marginRight: 6 }}>{cl.n}</span>{cl.en}</p>
                      {!cl.ok && (
                        <div style={{ marginTop: 8, border: "1px solid var(--warn)", background: "var(--warn-bg)", padding: "7px 10px", fontSize: 11.5 }}><span className="mono" style={{ fontSize: 9, letterSpacing: ".1em", color: "var(--warn)" }}>NO AUDITED TEMPLATE · </span>{cl.why}</div>
                      )}
                    </div>
                    <div className="mono" style={{ padding: "11px 14px", fontSize: 10.5, lineHeight: 1.6, color: cl.ok ? "var(--tx-2)" : "var(--tx-3)", whiteSpace: "pre-wrap" }}>{cl.gh}{cl.tpl && <span style={{ display: "block", marginTop: 6, fontSize: 8.5, color: "var(--tx-3)" }}>↳ {cl.tpl}</span>}</div>
                  </div>
                ))}
              </div>
              <div style={{ display: "flex", alignItems: "center", gap: 10 }}>
                <span className="mono" style={{ fontSize: 9.5, color: "var(--tx-3)" }}>Clauses lock in scroll; an edit on either side marks the pair dirty. C4 is excluded from compile — flagged, never dropped silently.</span>
                <div style={{ flex: 1 }} /><button className="btn btn-primary" onClick={() => setStep(4)}>Compile (C4 excluded)</button>
              </div>
            </div>
          )}

          {step === 4 && (
            <div style={{ display: "flex", flexDirection: "column", gap: 12, maxWidth: 720 }}>
              <div style={{ fontFamily: "var(--font-display)", fontWeight: 440, fontSize: 20 }}>Compile — audited templates, exact parameters</div>
              <div className="surface" style={{ display: "flex", flexDirection: "column" }}>
                {([["Template", "BoundedAutonomy v2.1"], ["Audit CID", "bafy…70e1 (audited)"], ["Spec", "SPEC-104 · 4 clauses mapped, 1 excluded"], ["spend ceiling", "150 SALT (C2)"], ["egress", "CUI+ local-only (C3)"], ["CREATE2 salt", "keccak(line4 ‖ envelope ‖ v4)"]] as [string, string][]).map(([k, v]) => (
                  <div key={k} style={{ display: "grid", gridTemplateColumns: "170px 1fr", borderBottom: "1px solid var(--line-1)" }}>
                    <span className="mono" style={{ fontSize: 9.5, letterSpacing: ".1em", textTransform: "uppercase", color: "var(--tx-3)", padding: "9px 14px", background: "var(--srf-inset)" }}>{k}</span>
                    <span className="mono" style={{ fontSize: 11, padding: "9px 14px" }}>{v}</span>
                  </div>
                ))}
              </div>
              <div className="surface" style={{ padding: "12px 14px", display: "flex", flexDirection: "column", gap: 6 }}>
                <span className="eyebrow">Diff vs deployed PRT-004 v3</span>
                <span className="mono" style={{ fontSize: 11, color: "var(--ok)" }}>+ single-action ceiling 150 SALT → ceremony (C2)</span>
                <span className="mono" style={{ fontSize: 11, color: "var(--ok)" }}>+ calendar.write standing-grant requirement (C5)</span>
                <span className="mono" style={{ fontSize: 11, color: "var(--danger)" }}>− C4 retaliation protection (unmapped — excluded, T-request filed)</span>
              </div>
              <div style={{ display: "flex" }}><div style={{ flex: 1 }} /><button className="btn btn-primary" onClick={() => setStep(5)}>Simulate against the last 90 days</button></div>
            </div>
          )}

          {step === 5 && (
            <div style={{ display: "flex", flexDirection: "column", gap: 14 }}>
              <div style={{ display: "flex", alignItems: "center", gap: 14 }}>
                <div style={{ fontFamily: "var(--font-display)", fontWeight: 440, fontSize: 20 }}>What this policy would have done</div>
                <span className="mono" style={{ fontSize: 10, color: "var(--tx-3)" }}>{sim?.range ?? "last 90 days"}</span>
                <div style={{ flex: 1 }} />
                {simState === "idle" && <button className="btn btn-primary" onClick={runSim}>Run simulation</button>}
              </div>
              {simState === "running" && (
                <div className="surface" style={{ display: "flex", alignItems: "center", gap: 16, padding: 18 }}>
                  <div style={{ width: 44, height: 44, flexShrink: 0 }}><LoaderMark size={44} /></div>
                  <span className="mono" style={{ fontSize: 11, color: "var(--tx-2)" }}>replaying 14,208 recorded decisions through SPEC-104 · governance.simulate()</span>
                </div>
              )}
              {simState === "done" && sim && (
                <div style={{ display: "flex", flexDirection: "column", gap: 14 }}>
                  <div style={{ display: "grid", gridTemplateColumns: "repeat(3,1fr)", gap: 10 }}>
                    <div className="surface" style={{ padding: 14, borderTop: "2px solid var(--danger)" }}><div className="eyebrow">Would have blocked</div><div className="tabular" style={{ fontFamily: "var(--font-display)", fontSize: 34, fontWeight: 460, color: "var(--danger)" }}>{sim.blocked}</div><div className="mono" style={{ fontSize: 9.5, color: "var(--tx-3)" }}>0.7% of all decisions</div></div>
                    <div className="surface" style={{ padding: 14, borderTop: "2px solid var(--warn)" }}><div className="eyebrow">Would have paused for a human</div><div className="tabular" style={{ fontFamily: "var(--font-display)", fontSize: 34, fontWeight: 460, color: "var(--warn)" }}>{sim.approvals}</div><div className="mono" style={{ fontSize: 9.5, color: "var(--tx-3)" }}>2.2% — median wait modeled 4m</div></div>
                    <div className="surface" style={{ padding: 14, borderTop: "2px solid var(--accent)" }}><div className="eyebrow">Unchanged</div><div className="tabular" style={{ fontFamily: "var(--font-display)", fontSize: 34, fontWeight: 460 }}>{sim.unchanged.toLocaleString("en-US")}</div><div className="mono" style={{ fontSize: 9.5, color: "var(--tx-3)" }}>the policy would not have applied</div></div>
                  </div>
                  <div style={{ display: "grid", gridTemplateColumns: "1fr 1fr", gap: 10 }}>
                    <div className="surface" style={{ padding: 14, display: "flex", flexDirection: "column", gap: 8 }}>
                      <span className="eyebrow">By action class — blocked · paused</span>
                      {sim.byClass.map(([name, blocked, paused]) => {
                        const tot = blocked + paused || 1;
                        return (
                          <div key={name} style={{ display: "flex", alignItems: "center", gap: 10 }}>
                            <span className="mono" style={{ fontSize: 10, width: 110, color: "var(--tx-2)" }}>{name}</span>
                            <div style={{ flex: 1, display: "flex", height: 10, gap: 1 }}>
                              <div style={{ width: `${(blocked / tot) * 100}%`, background: "var(--danger)" }} />
                              <div style={{ width: `${(paused / tot) * 100}%`, background: "var(--warn)" }} />
                            </div>
                            <span className="mono tabular" style={{ fontSize: 10, color: "var(--tx-3)", width: 60, textAlign: "right" }}>{blocked}·{paused}</span>
                          </div>
                        );
                      })}
                    </div>
                    <div className="surface" style={{ padding: 14, display: "flex", flexDirection: "column", gap: 8 }}>
                      <span className="eyebrow">Who this would have inconvenienced</span>
                      {sim.inconvenienced.map(([name, n, note]) => (
                        <div key={name} style={{ display: "flex", alignItems: "baseline", gap: 10, borderBottom: "1px solid var(--line-1)", paddingBottom: 5 }}>
                          <span className="mono" style={{ fontSize: 11, width: 110 }}>{name}</span>
                          <span className="tabular mono" style={{ fontSize: 12 }}>{n}</span>
                          <span style={{ fontSize: 11, color: "var(--tx-3)" }}>{note}</span>
                        </div>
                      ))}
                      <span style={{ fontSize: 11.5, color: "var(--tx-2)", lineHeight: 1.5 }}>No human workflow loses more than 4 pauses/week. The cost lands on agents, where it belongs.</span>
                    </div>
                  </div>
                  <div className="surface" style={{ display: "flex", flexDirection: "column" }}>
                    <div className="eyebrow" style={{ padding: "10px 14px", borderBottom: "1px solid var(--line-1)" }}>Specific decisions, with why</div>
                    {sim.samples.map((sm) => (
                      <div key={sm.id} style={{ display: "grid", gridTemplateColumns: "80px 110px 1fr 140px", gap: 10, padding: "9px 14px", borderBottom: "1px solid var(--line-1)", alignItems: "baseline" }}>
                        <span className="mono" style={{ fontSize: 10, color: "var(--accent-text)" }}>{sm.id}</span>
                        <span className="mono" style={{ fontSize: 10, color: "var(--tx-2)" }}>{sm.agent}</span>
                        <span style={{ fontSize: 12 }}>{sm.action} — <span style={{ color: "var(--tx-3)" }}>{sm.why}</span></span>
                        <span className="mono" style={{ fontSize: 9, letterSpacing: ".08em", textTransform: "uppercase", color: sm.would === "blocked" ? "var(--danger)" : "var(--warn)", textAlign: "right" }}>{sm.would}</span>
                      </div>
                    ))}
                  </div>
                  <div style={{ display: "flex" }}><div style={{ flex: 1 }} /><button className="btn btn-primary" onClick={() => setStep(6)}>Proceed to ceremony — 2 of 3 signers</button></div>
                </div>
              )}
            </div>
          )}

          {step === 6 && (
            <div style={{ display: "flex", flexDirection: "column", gap: 12, maxWidth: 640 }}>
              <div style={{ fontFamily: "var(--font-display)", fontWeight: 440, fontSize: 20 }}>Ceremony — the moment of human authority</div>
              <p style={{ fontSize: 13.5, color: "var(--tx-2)", lineHeight: 1.6, margin: 0 }}>Deploying SPEC-104 needs 2 of 3 signatures. The contract address is CREATE2-derived — <span className="mono">{sim?.create2.slice(0, 6)}…{sim?.create2.slice(-4)}</span> — known now, before anyone signs. What you sign is what deploys.</p>
              <div><button className="btn btn-primary btn-lg" onClick={openDeploy}>Open signature ceremony</button></div>
              {deployed && <div className="cc-stamp mono" style={{ fontSize: 11, color: "var(--ok)", border: "1px solid var(--ok)", background: "var(--ok-bg)", padding: "10px 12px", width: "fit-content" }}>settled — continue to deploy status</div>}
            </div>
          )}

          {step === 7 && (
            <div style={{ display: "flex", flexDirection: "column", gap: 12, maxWidth: 720 }}>
              <div style={{ fontFamily: "var(--font-display)", fontWeight: 440, fontSize: 20 }}>Deployed</div>
              <div className="surface" style={{ display: "flex", flexDirection: "column" }}>
                {([["Protocol", "SPEC-104 — Line-4 Operating Envelope v4"], ["Address", sim?.create2 ?? "0x…"], ["Template", "BoundedAutonomy v2.1 · audited"], ["Spec CID", "bafy…104e"], ["Block", "1,284,067 · anchored"]] as [string, string][]).map(([k, v]) => (
                  <div key={k} style={{ display: "grid", gridTemplateColumns: "150px 1fr", borderBottom: "1px solid var(--line-1)" }}>
                    <span className="mono" style={{ fontSize: 9.5, letterSpacing: ".1em", textTransform: "uppercase", color: "var(--tx-3)", padding: "9px 14px", background: "var(--srf-inset)" }}>{k}</span>
                    <span className="mono" style={{ fontSize: 11, padding: "9px 14px", wordBreak: "break-all", color: k === "Address" ? "var(--accent-text)" : "var(--tx-1)" }}>{v}</span>
                  </div>
                ))}
              </div>
              <div style={{ display: "flex" }}><div style={{ flex: 1 }} /><button className="btn btn-primary" onClick={() => setStep(8)}>Bind — who this now governs</button></div>
            </div>
          )}

          {step === 8 && (
            <div style={{ display: "flex", flexDirection: "column", gap: 12, maxWidth: 820 }}>
              <div style={{ fontFamily: "var(--font-display)", fontWeight: 440, fontSize: 20 }}>Bound — effective permissions, before and after</div>
              <div className="surface" style={{ padding: "12px 14px", display: "flex", flexDirection: "column", gap: 5 }}>
                <span className="eyebrow">Now governs</span>
                <span className="mono" style={{ fontSize: 11 }}>repo.write · pr.open · ci.* · doc.draft · calendar.write · spend — tenant Line-4 Automation</span>
                <span className="mono" style={{ fontSize: 10, color: "var(--ok)" }}>policy reloaded: claude-code ✓ · codex ✓ · devin ✓ · hermes ✓ · windsurf-swe — quarantined, will load on release</span>
              </div>
              <div style={{ display: "grid", gridTemplateColumns: "1fr 1fr", gap: 10 }}>
                <div className="surface" style={{ padding: "12px 14px" }}><span className="eyebrow">Before — v3</span><p className="mono" style={{ fontSize: 10.5, lineHeight: 1.8, margin: "6px 0 0", color: "var(--tx-2)" }}>spend ceiling: budget only<br />calendar.write: ungoverned ⚠<br />CUI egress: advisory</p></div>
                <div className="surface" style={{ padding: "12px 14px", borderTop: "2px solid var(--accent)" }}><span className="eyebrow">After — v4</span><p className="mono" style={{ fontSize: 10.5, lineHeight: 1.8, margin: "6px 0 0" }}>spend &gt; 150 SALT: ceremony<br />calendar.write: standing grant required<br />CUI egress: blocked on chain</p></div>
              </div>
              <div className="mono" style={{ fontSize: 9.5, color: "var(--tx-3)" }}>Read from governance.bind() → ProtocolRegistry → chain 40204</div>
            </div>
          )}
        </div>
      )}

      {tab === "prot" && (
        <div className="surface" style={{ display: "flex", flexDirection: "column" }}>
          <div className="mono" style={{ display: "grid", gridTemplateColumns: "80px 1fr 120px 190px 150px 110px", gap: 10, padding: "8px 16px", fontSize: 9, letterSpacing: ".12em", textTransform: "uppercase", color: "var(--tx-3)", borderBottom: "1px solid var(--line-1)" }}>
            <span>Id</span><span>Protocol</span><span>Version</span><span>Template · audit</span><span>Address</span><span>State</span>
          </div>
          {protocols.map((p) => (
            <div key={p.id} style={{ display: "grid", gridTemplateColumns: "80px 1fr 120px 190px 150px 110px", gap: 10, padding: "10px 16px", borderBottom: "1px solid var(--line-1)", alignItems: "center" }}>
              <span className="mono" style={{ fontSize: 10.5, color: "var(--accent-text)" }}>{p.id}</span>
              <span style={{ fontSize: 13, fontWeight: 500 }}>{p.name} <span style={{ fontSize: 11, color: "var(--tx-3)", fontWeight: 400 }}>governs {p.governs}</span></span>
              <span className="mono" style={{ fontSize: 10.5, color: "var(--tx-2)" }}>{p.version}</span>
              <span className="mono" style={{ fontSize: 9.5, color: p.state === "deprecated" ? "var(--danger)" : "var(--tx-2)" }}>{p.template}<br />{p.audit}</span>
              <span className="mono" style={{ fontSize: 10, color: "var(--tx-2)" }}>{p.addr}</span>
              <span className="mono" style={{ fontSize: 9, letterSpacing: ".08em", textTransform: "uppercase", color: p.state === "live" ? "var(--ok)" : "var(--danger)" }}>{p.state}</span>
            </div>
          ))}
          <div className="mono" style={{ fontSize: 9.5, color: "var(--tx-3)", padding: "8px 16px" }}>Amendment flow: propose → review window → vote → timelock 14d → migrate · plain-English diff at every step</div>
        </div>
      )}

      {tab === "tpl" && (
        <div style={{ display: "grid", gridTemplateColumns: "repeat(3,1fr)", gap: 10 }}>
          {[
            { name: "BoundedAutonomy v2.1", audit: "audited · CID bafy…70e1", desc: "Agent may act unattended within a spend/action budget per window; over-threshold pauses for a signature.", top: "var(--accent)" },
            { name: "EgressControl v1.4", audit: "audited · CID bafy…22a8", desc: "Which model endpoints may serve which classification; CUI+ is local-only.", top: "var(--info)" },
            { name: "ThresholdApproval v1.2", audit: "audited · CID bafy…9c30", desc: "Action class X requires N-of-M signers from role set R.", top: "var(--z-magenta)" },
          ].map((tc) => (
            <div key={tc.name} className="surface" style={{ padding: 14, display: "flex", flexDirection: "column", gap: 7, borderTop: `2px solid ${tc.top}` }}>
              <span style={{ fontSize: 14, fontWeight: 500 }}>{tc.name}</span>
              <span className="mono" style={{ fontSize: 9.5, color: "var(--ok)" }}>{tc.audit}</span>
              <span style={{ fontSize: 12, color: "var(--tx-2)", lineHeight: 1.5 }}>{tc.desc}</span>
            </div>
          ))}
        </div>
      )}
    </div>
  );
}
