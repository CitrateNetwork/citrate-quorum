// citrate-quorum — Agents surface (QRM-S2D). Instrument register. Ported from
// design §AGENTS. The fleet (vendor, HIC, grants, budget, disputes, status) and
// the agent detail: identity + capsules (manifest-hash verified), reputation
// (measured facts with denominators — no single score), the grants envelope
// (issue/revoke via the ceremony), and the kill switch. Reads bridge.agents.*.
import { Fragment, useEffect, useState } from "react";
import { bridge } from "../bridge";
import { DomainErrorPlate, useDomain } from "../components/DomainState";
import type { Agent, Classification, Grant } from "../bridge";
import { VENDORS } from "../theme/vendors";
import { useCeremony } from "../ceremony/Ceremony";

const HIC_COLOR: Record<number, string> = { 0: "var(--tx-3)", 1: "var(--ok)", 2: "var(--info)", 3: "var(--warn)" };
const STATUS_COLOR: Record<string, string> = { active: "var(--ok)", probation: "var(--warn)", quarantined: "var(--danger)" };

/**
 * The action classes the shipped adapters emit (`action_class` in
 * quorum-adapter). Offered as suggestions, NOT as a closed set: an unknown tool
 * is governed under `tool.<name>`, so the field stays free text. The point is
 * that a grant covering `shel.exec` governs nothing, silently, and the operator
 * should be able to see that before they sign.
 */
const KNOWN_CLASSES = [
  "shell.exec",
  "repo.write",
  "repo.read",
  "net.fetch",
  "agent.spawn",
  "spend",
] as const;

/** A readable, sortable, collision-resistant default. `G-1784947152936` was
 *  none of those. */
function defaultGrantId(now: Date): string {
  const p = (n: number) => String(n).padStart(2, "0");
  return `G-${now.getUTCFullYear()}${p(now.getUTCMonth() + 1)}${p(now.getUTCDate())}-${p(now.getUTCHours())}${p(now.getUTCMinutes())}${p(now.getUTCSeconds())}`;
}

/** Registry-only fields are absent until the AgentSBT read lands. An em dash is
 *  the honest render; a plausible value is not (Rule 1). */
const orDash = (v: string | number | undefined | null) =>
  v === undefined || v === null || v === "" ? "—" : String(v);
const REP_LABELS: [keyof NonNullable<Agent["reputation"]>, string][] = [
  ["dispute", "Dispute rate"], ["contradiction", "Contradiction"], ["escalation", "Escalation"], ["budget", "Budget adherence"], ["grader", "Claim-grader"],
];

export function Agents() {
  const [fleet, setFleet] = useState<Agent[]>([]);
  const [sel, setSel] = useState<Agent | null>(null);
  const [grants, setGrants] = useState<Grant[]>([]);
  const [revoked, setRevoked] = useState<Set<string>>(new Set());
  const [refresh, setRefresh] = useState(0);
  /** The human at the keyboard. A revocation is recorded against them. */
  const [operator, setOperator] = useState("");
  const [formOpen, setFormOpen] = useState(false);
  const [formError, setFormError] = useState("");
  const [form, setForm] = useState({
    id: "",
    agent: "",
    principal: "",
    scope: "",
    classes: "repo.write",
    ceiling: "Public" as Classification,
    budget: 100,
    days: 45,
    hic: "2",
  });
  const ceremony = useCeremony();


  // Honest failure (S2D.4/§5.1): this surface's primary read is agents.list().
  // A read that cannot succeed must say so and offer a retry, not sit in a
  // loading state forever.
  useEffect(() => {
    Promise.resolve()
      .then(() => bridge.session.operator())
      .then((op) => {
        setOperator(op ?? "");
        // Prefill the issuer: it is the same human, and retyping your own name
        // on every grant invites a typo into the evidence.
        if (op) setForm((f) => (f.principal ? f : { ...f, principal: op }));
      })
      .catch(() => setOperator(""));
  }, []);

  // Re-read after any issue/revoke: the backend is the record, not this state.
  useEffect(() => {
    if (refresh === 0) return;
    bridge.agents.list().then(setFleet).catch(() => {});
    if (sel) bridge.agents.grants(sel.id).then(setGrants).catch(() => {});
    // `sel` is intentionally not a dep: this fires on an explicit change only.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [refresh]);

  const primary = useDomain(() => bridge.agents.list(), "agents.list()");
  useEffect(() => {
    if (primary.state.status === "ready") setFleet(primary.state.data);
  }, [primary.state]);
  if (primary.state.status === "error") {
    return (
      <div style={{ padding: 18 }}>
        <DomainErrorPlate source="agents.list()" error={primary.state.error} onRetry={primary.retry} lands="It lands in QRM-S4 (agent adapter layer)." />
      </div>
    );
  }

  const open = (a: Agent) => {
    setSel(a); setRevoked(new Set());
    setForm((f) => ({ ...f, agent: a.id }));
    bridge.agents.grants(a.id).then(setGrants).catch(() => {});
  };

  const revokeGrant = async (g: Grant) => {
    if (!sel) return;
    const r = await ceremony.request({
      kind: "revoke", title: `Revoke grant ${g.id} — ${sel.name}`, origin: "user",
      // Revoking a capability always requires a human at HIC-1 (Rule 5).
      action: { actionClass: "grant.revoke", classification: "Proprietary", agent: "user", mandatoryHic1: true },
      rows: [{ k: "Grant", v: `${g.id} · ${g.classes}` }, { k: "Agent", v: `${sel.name} · ${orDash(sel.sbt)}` }, { k: "Scope", v: g.scope }, { k: "Effect", v: "immediate — the capability is gone at the next checkpoint" }],
    });
    if (r.outcome === "settled") {
      setRevoked((s) => new Set(s).add(g.id));
      // The named operator, not a form field that happens to be filled in:
      // "operator" as a principal is a placeholder, and a revocation attributed
      // to a placeholder is not evidence of who did it.
      await bridge.agents.revoke(sel.id, g.id, operator).catch(() => {});
      setRefresh((n) => n + 1);
    }
  };

  const killAll = async () => {
    if (!sel) return;
    const r = await ceremony.request({
      kind: "revoke", title: `Revoke ALL grants — ${sel.name}`, origin: "user",
      action: { actionClass: "grant.revoke-all", classification: "Proprietary", agent: "user", mandatoryHic1: true },
      rows: [{ k: "Agent", v: `${sel.name} · ${orDash(sel.sbt)}` }, { k: "Grants revoked", v: `${liveGrants.length} live grants` }, { k: "Keeps", v: "identity + history" }, { k: "Loses", v: "every capability" }, { k: "Running actions", v: "abort at the next checkpoint (<25s)" }],
    });
    if (r.outcome === "settled") {
      setRevoked(new Set(liveGrants.map((g) => g.id)));
      for (const g of liveGrants) {
        await bridge.agents.revoke(sel.id, g.id, operator).catch(() => {});
      }
      setRefresh((n) => n + 1);
    }
  };

  /**
   * Issue a capability grant. L-1 makes this HIC-1, so the terms go to the
   * ceremony FIRST and the grant is only written if a human signs — the
   * ceremony is not a confirmation dialog after the fact.
   */
  const issueGrant = async () => {
    if (!sel) return;
    const agent = form.agent.trim() || sel.id;
    const classes = form.classes.split(/[,\s]+/).filter(Boolean);
    if (!classes.length || !form.principal.trim()) {
      setFormError("An action class and the issuing principal are both required.");
      return;
    }
    setFormError("");
    const expiresAtMs = Date.now() + form.days * 86_400_000;
    const grantId = form.id.trim() || defaultGrantId(new Date());
    const r = await ceremony.request({
      kind: "grant",
      title: `Issue grant — ${agent}`,
      origin: "user",
      action: { actionClass: "grant.issue", classification: form.ceiling, agent: "user", mandatoryHic1: true },
      rows: [
        { k: "Grant", v: grantId },
        { k: "Agent", v: agent },
        { k: "Action classes", v: classes.join(" · ") },
        { k: "Ceiling", v: form.ceiling },
        { k: "Budget", v: `${form.budget} units` },
        { k: "HIC level", v: `HIC-${form.hic}` },
        { k: "Expires", v: `${form.days} days` },
        { k: "Issued by", v: form.principal },
      ],
    });
    if (r.outcome !== "settled") return;
    try {
      await bridge.agents.issue({
        id: grantId,
        agent,
        principal: form.principal.trim(),
        tenantScope: form.scope.trim(),
        actionClasses: classes,
        classificationCeiling: form.ceiling,
        budgetUnits: form.budget,
        expiresAtMs,
        hic: form.hic,
      });
      setFormOpen(false);
      setRefresh((n) => n + 1);
    } catch (e) {
      setFormError(e instanceof Error ? e.message : String(e));
    }
  };

  // A grant revoked in an earlier session comes back from the backend with
  // `revoked: true`. Filtering only on this session's optimistic set showed it
  // as live — an operator reading a capability that no longer exists.
  /** What the classes field will actually become. Shown live under the field. */
  const parsedClasses = form.classes.split(/[,\s]+/).filter(Boolean);

  const isRevoked = (g: Grant) => g.revoked === true || revoked.has(g.id);
  const liveGrants = grants.filter((g) => !isRevoked(g));
  const revokedGrants = grants.filter(isRevoked);

  return (
    <div style={{ padding: 18, display: "flex", flexDirection: "column", gap: 14 }}>
      <div className="surface" style={{ display: "flex", flexDirection: "column" }}>
        <div className="mono" style={{ display: "grid", gridTemplateColumns: "150px 90px 60px 60px 150px 100px 110px", gap: 10, padding: "8px 14px", fontSize: 9, letterSpacing: ".12em", textTransform: "uppercase", color: "var(--tx-3)", borderBottom: "1px solid var(--line-1)" }}>
          <span>Agent</span><span>SBT</span><span>HIC</span><span>Grants</span><span>Budget · SALT</span><span>Disputes</span><span>Status</span>
        </div>
        {fleet.map((a) => {
          const pct = a.budgetCap ? (a.budgetUsed / a.budgetCap) * 100 : 0;
          const bc = pct >= 90 ? "var(--warn)" : "var(--accent)";
          return (
            <div key={a.id} onClick={() => open(a)} style={{ display: "grid", gridTemplateColumns: "150px 90px 60px 60px 150px 100px 110px", gap: 10, padding: "10px 14px", borderBottom: "1px solid var(--line-1)", cursor: "pointer", alignItems: "center", background: sel?.id === a.id ? "var(--srf-1)" : "transparent" }}>
              <span style={{ display: "flex", alignItems: "center", gap: 7, minWidth: 0 }}><span style={{ width: 8, height: 8, borderRadius: 999, background: a.vendor ? VENDORS[a.vendor]?.color : "var(--line-2)", flexShrink: 0 }} /><span style={{ fontSize: 13, fontWeight: 500 }}>{a.name}</span></span>
              <span className="mono" style={{ fontSize: 10, color: "var(--tx-3)" }}>{orDash(a.sbt)}</span>
              <span className="mono" style={{ fontSize: 10, color: a.hic === undefined ? "var(--tx-3)" : HIC_COLOR[a.hic] }}>{a.hic === undefined ? "—" : `HIC-${a.hic}`}</span>
              <span className="mono tabular" style={{ fontSize: 11 }}>{a.grants}</span>
              <span style={{ display: "flex", alignItems: "center", gap: 8 }}><div style={{ flex: 1, height: 7, background: "var(--srf-inset)", border: "1px solid var(--line-1)" }}><div style={{ height: "100%", width: `${pct}%`, background: bc }} /></div><span className="mono tabular" style={{ fontSize: 9.5, color: "var(--tx-3)" }}>{a.budgetUsed}/{a.budgetCap}</span></span>
              <span className="mono tabular" style={{ fontSize: 10, color: "var(--tx-3)" }}>{orDash(a.disputeRate)}</span>
              <span className="mono" style={{ fontSize: 9, letterSpacing: ".08em", textTransform: "uppercase", color: a.status ? STATUS_COLOR[a.status] : "var(--tx-3)" }}>{orDash(a.status)}</span>
            </div>
          );
        })}
        <div className="mono" style={{ fontSize: 9.5, color: "var(--tx-3)", padding: "8px 14px" }}>
          Agent · Grants · Budget read from agents.list() → this tenant&apos;s grants + decision records. SBT · HIC · Disputes · Status come from the AgentSBT registry (a chain read) and show — until it is wired.
        </div>
      </div>

      {sel && (
        <div style={{ display: "grid", gridTemplateColumns: "1fr 1fr", gap: 14 }}>
          <div style={{ display: "flex", flexDirection: "column", gap: 14 }}>
            <div className="surface" style={{ padding: 16, display: "flex", flexDirection: "column", gap: 10 }}>
              <div style={{ display: "flex", alignItems: "center", gap: 10 }}>
                <span style={{ width: 10, height: 10, borderRadius: 999, background: sel.vendor ? VENDORS[sel.vendor]?.color : "var(--line-2)" }} />
                <span style={{ fontFamily: "var(--font-display)", fontWeight: 460, fontSize: 20 }}>{sel.name}</span>
                <span className="mono" style={{ fontSize: 9.5, color: "var(--tx-3)" }}>{sel.vendor ? VENDORS[sel.vendor]?.name : "vendor —"} · {orDash(sel.sbt)}</span>
                <div style={{ flex: 1 }} />
                <span className="mono" style={{ fontSize: 9, letterSpacing: ".08em", textTransform: "uppercase", color: sel.status ? STATUS_COLOR[sel.status] : "var(--tx-3)", border: `1px solid ${sel.status ? STATUS_COLOR[sel.status] : "var(--line-2)"}`, padding: "2px 8px" }}>{orDash(sel.status)}</span>
              </div>
              {sel.quarantine && <div style={{ border: "1px solid var(--danger)", background: "var(--danger-bg)", padding: "10px 12px", fontSize: 12.5, lineHeight: 1.5 }}>{sel.quarantine}</div>}
              <div style={{ display: "grid", gridTemplateColumns: "120px 1fr", gap: "6px 10px", fontSize: 12 }}>
                {([
                  ["DID", orDash(sel.did)],
                  ["Pubkey", orDash(sel.pubkey)],
                  ["Model", sel.model ? `${sel.model} · LoRA ${orDash(sel.lora)}` : "—"],
                  ["Adapter", sel.transport ? `${sel.transport} · ${orDash(sel.sandbox)} · egress ${orDash(sel.egress)}` : "—"],
                ] as [string, string][]).map(([k, v]) => (
                  <Fragment key={k}>
                    <span className="mono" style={{ fontSize: 9, letterSpacing: ".1em", textTransform: "uppercase", color: "var(--tx-3)", paddingTop: 2 }}>{k}</span>
                    <span className="mono" style={{ fontSize: 11 }}>{v}</span>
                  </Fragment>
                ))}
              </div>
              <div style={{ display: "flex", flexDirection: "column", gap: 5, borderTop: "1px solid var(--line-1)", paddingTop: 10 }}>
                <span className="eyebrow">Capsules — manifest-hash verified</span>
                {!sel.capsules && (
                  <span className="mono" style={{ fontSize: 10, color: "var(--tx-3)" }}>
                    — CapsuleRegistry is a chain read; it lands with the registry
                  </span>
                )}
                {(sel.capsules ?? []).map((cp) => (
                  <div key={cp.name} className="mono" style={{ display: "flex", gap: 10, fontSize: 10.5, alignItems: "baseline" }}><span>{cp.name}</span><span style={{ color: "var(--tx-3)" }}>{cp.hash}</span><span style={{ color: cp.verified ? "var(--ok)" : "var(--danger)" }}>{cp.verified ? "verified" : "UNVERIFIED — refuses to load"}</span></div>
                ))}
              </div>
            </div>
            <div className="surface" style={{ padding: 16, display: "flex", flexDirection: "column", gap: 8 }}>
              <span className="eyebrow">Reputation — measured facts, with denominators</span>
              {!sel.reputation && (
                <span className="mono" style={{ fontSize: 10, color: "var(--tx-3)" }}>
                  — measured over disputes, contradictions and budget adherence; needs the
                  AgentSBT registry and a dispute history to divide by
                </span>
              )}
              {(sel.reputation ? REP_LABELS : []).map(([key, label]) => {
                const [v, d] = sel.reputation![key];
                return (
                  <div key={key} style={{ display: "grid", gridTemplateColumns: "150px 70px 1fr", gap: 10, borderBottom: "1px solid var(--line-1)", paddingBottom: 5, alignItems: "baseline" }}>
                    <span className="mono" style={{ fontSize: 9.5, letterSpacing: ".08em", textTransform: "uppercase", color: "var(--tx-3)" }}>{label}</span>
                    <span className="mono tabular" style={{ fontSize: 13 }}>{v}</span>
                    <span className="mono" style={{ fontSize: 9.5, color: "var(--tx-3)" }}>{d}</span>
                  </div>
                );
              })}
              <span style={{ fontSize: 11, color: "var(--tx-3)" }}>No single score. A number without its denominator is an opinion.</span>
            </div>
          </div>
          <div style={{ display: "flex", flexDirection: "column", gap: 14 }}>
            <div className="surface" style={{ display: "flex", flexDirection: "column", borderTop: `2px solid ${liveGrants.length ? "var(--accent)" : "var(--line-2)"}` }}>
              <div style={{ display: "flex", alignItems: "center", padding: "10px 14px", borderBottom: "1px solid var(--line-1)", gap: 10 }}>
                <span className="eyebrow">Grants — the envelope</span><div style={{ flex: 1 }} />
                <button className="btn btn-primary btn-sm" onClick={() => setFormOpen((o) => !o)}>
                  {formOpen ? "Cancel" : "Issue grant"}
                </button>
              </div>
              {formOpen && (
                <div style={{ padding: "12px 14px", display: "flex", flexDirection: "column", gap: 10, borderBottom: "1px solid var(--line-1)", background: "var(--srf-inset)" }}>
                  <span className="mono" style={{ fontSize: 9.5, letterSpacing: ".12em", textTransform: "uppercase", color: "var(--tx-3)" }}>
                    New capability grant — signed in a ceremony (L-1)
                  </span>
                  <div style={{ display: "grid", gridTemplateColumns: "1fr 1fr", gap: 8 }}>
                    <div><div className="lbl">Agent</div>
                      <input className="input" style={{ width: "100%" }} value={form.agent} onChange={(e) => setForm({ ...form, agent: e.target.value })} placeholder="agent id" /></div>
                    <div><div className="lbl">Issued by</div>
                      <input className="input" style={{ width: "100%" }} value={form.principal} onChange={(e) => setForm({ ...form, principal: e.target.value })} placeholder="your name — recorded" /></div>
                    <div style={{ gridColumn: "1 / -1" }}><div className="lbl">Action classes</div>
                      <input className="input" style={{ width: "100%" }} list="quorum-action-classes" value={form.classes} onChange={(e) => setForm({ ...form, classes: e.target.value })} placeholder="repo.write shell.exec" />
                      <datalist id="quorum-action-classes">
                        {KNOWN_CLASSES.map((k) => <option key={k} value={k} />)}
                      </datalist>
                      {/* Show what will actually be granted. A typo here is
                          silent: a grant covering `shel.exec` governs nothing,
                          and the agent just keeps coming back ungoverned. */}
                      <div className="mono" style={{ fontSize: 9.5, color: "var(--tx-3)", marginTop: 4 }}>
                        {parsedClasses.length === 0
                          ? "governs nothing yet — name at least one action class"
                          : <>governs {parsedClasses.map((cl) => (
                              <span key={cl} style={{ color: (KNOWN_CLASSES as readonly string[]).includes(cl) ? "var(--ok)" : "var(--warn)" }}>{cl}{" "}</span>
                            ))}{parsedClasses.some((cl) => !(KNOWN_CLASSES as readonly string[]).includes(cl)) && "— amber classes are not ones the shipped adapters emit; check the spelling"}</>}
                      </div>
                    </div>
                    <div><div className="lbl">Classification ceiling</div>
                      <select className="input" style={{ width: "100%" }} value={form.ceiling} onChange={(e) => setForm({ ...form, ceiling: e.target.value as Classification })}>
                        <option>Public</option><option>Proprietary</option><option>CUI</option><option>ITAR</option>
                      </select></div>
                    <div><div className="lbl">HIC level</div>
                      <select className="input" style={{ width: "100%" }} value={form.hic} onChange={(e) => setForm({ ...form, hic: e.target.value })}>
                        <option value="1">HIC-1 · approve each</option>
                        <option value="2">HIC-2 · budgeted autonomy</option>
                        <option value="3">HIC-3 · post-hoc review</option>
                      </select></div>
                    <div><div className="lbl">Budget (units)</div>
                      <input className="input" style={{ width: "100%" }} type="number" value={form.budget} onChange={(e) => setForm({ ...form, budget: Number(e.target.value) })} /></div>
                    <div><div className="lbl">Expires in (days)</div>
                      <input className="input" style={{ width: "100%" }} type="number" value={form.days} onChange={(e) => setForm({ ...form, days: Number(e.target.value) })} /></div>
                  </div>
                  <span className="mono" style={{ fontSize: 9.5, color: "var(--tx-3)" }}>
                    Every governed action charges at least one unit; the envelope depletes and returns to you.
                  </span>
                  {formError && (
                    <span className="mono" style={{ fontSize: 10.5, color: "var(--danger)" }}>{formError}</span>
                  )}
                  <div>
                    <button className="btn btn-primary btn-sm" onClick={() => void issueGrant()}>Review &amp; sign</button>
                  </div>
                </div>
              )}
              {liveGrants.length === 0 ? (
                <div style={{ padding: "16px 14px", fontSize: 12.5, color: "var(--tx-3)" }}>No live grants. This agent can observe and speak, and nothing else.</div>
              ) : (
                liveGrants.map((g) => (
                  <div key={g.id} style={{ display: "flex", flexDirection: "column", gap: 4, padding: "10px 14px", borderBottom: "1px solid var(--line-1)" }}>
                    <div style={{ display: "flex", alignItems: "center", gap: 8 }}>
                      <span className="mono" style={{ fontSize: 10, color: "var(--accent-text)" }}>{g.id}</span>
                      <span className="mono" style={{ fontSize: 11 }}>{g.classes}</span>
                      <div style={{ flex: 1 }} />
                      <span className="mono" style={{ fontSize: 9, color: HIC_COLOR[g.hic] }}>HIC-{g.hic}</span>
                      <span onClick={() => revokeGrant(g)} className="mono" style={{ fontSize: 9, letterSpacing: ".08em", textTransform: "uppercase", color: "var(--danger)", cursor: "pointer", border: "1px solid var(--danger)", padding: "1px 7px" }}>Revoke</span>
                    </div>
                    <div className="mono" style={{ fontSize: 9.5, color: "var(--tx-3)" }}>scope {g.scope} · budget {g.budget} · expires {g.expiry} · issued by {g.principal}</div>
                  </div>
                ))
              )}
              {/* A revoked grant stays on screen. It is history — the record of
                  a capability this agent once held and who took it away — and
                  hiding it makes the envelope look like it was never wider. */}
              {revokedGrants.map((g) => (
                <div key={g.id} style={{ display: "flex", flexDirection: "column", gap: 4, padding: "10px 14px", borderBottom: "1px solid var(--line-1)", opacity: 0.66 }}>
                  <div style={{ display: "flex", alignItems: "center", gap: 8 }}>
                    <span className="mono" style={{ fontSize: 10, color: "var(--tx-3)", textDecoration: "line-through" }}>{g.id}</span>
                    <span className="mono" style={{ fontSize: 11, color: "var(--tx-3)", textDecoration: "line-through" }}>{g.classes}</span>
                    <div style={{ flex: 1 }} />
                    <span className="mono" style={{ fontSize: 9, letterSpacing: ".08em", textTransform: "uppercase", color: "var(--danger)", border: "1px solid var(--danger)", padding: "1px 7px" }}>Revoked</span>
                  </div>
                  <div className="mono" style={{ fontSize: 9.5, color: "var(--tx-3)" }}>scope {g.scope} · budget {g.budget} · issued by {g.principal}</div>
                </div>
              ))}
              <div className="mono" style={{ fontSize: 9.5, color: "var(--tx-3)", padding: "8px 14px" }}>agents.grants() → GrantRegistry · revocation is immediate and needs a signature · revoked grants stay listed as history</div>
            </div>
            <div className="surface" style={{ padding: "14px 16px", display: "flex", alignItems: "center", gap: 14, border: "1px solid var(--danger)" }}>
              <div style={{ flex: 1 }}>
                <div style={{ fontSize: 13.5, fontWeight: 500, color: "var(--danger)" }}>Kill switch</div>
                <div style={{ fontSize: 11.5, color: "var(--tx-3)", lineHeight: 1.5 }}>Revokes every live grant for this agent. It keeps its identity and its history; it loses every capability. Running actions abort at the next checkpoint (&lt;25s).</div>
              </div>
              <button className="btn btn-danger" onClick={killAll}>Revoke all — {sel.name}</button>
            </div>
          </div>
        </div>
      )}
    </div>
  );
}
