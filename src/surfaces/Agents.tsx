// citrate-quorum — Agents surface (QRM-S2D). Instrument register. Ported from
// design §AGENTS. The fleet (vendor, HIC, grants, budget, disputes, status) and
// the agent detail: identity + capsules (manifest-hash verified), reputation
// (measured facts with denominators — no single score), the grants envelope
// (issue/revoke via the ceremony), and the kill switch. Reads bridge.agents.*.
import { Fragment, useEffect, useState } from "react";
import { bridge } from "../bridge";
import { DomainErrorPlate, useDomain } from "../components/DomainState";
import type { Agent, Grant } from "../bridge";
import { VENDORS } from "../theme/vendors";
import { useCeremony } from "../ceremony/Ceremony";

const HIC_COLOR: Record<number, string> = { 0: "var(--tx-3)", 1: "var(--ok)", 2: "var(--info)", 3: "var(--warn)" };
const STATUS_COLOR: Record<string, string> = { active: "var(--ok)", probation: "var(--warn)", quarantined: "var(--danger)" };
const REP_LABELS: [keyof Agent["reputation"], string][] = [
  ["dispute", "Dispute rate"], ["contradiction", "Contradiction"], ["escalation", "Escalation"], ["budget", "Budget adherence"], ["grader", "Claim-grader"],
];

export function Agents() {
  const [fleet, setFleet] = useState<Agent[]>([]);
  const [sel, setSel] = useState<Agent | null>(null);
  const [grants, setGrants] = useState<Grant[]>([]);
  const [revoked, setRevoked] = useState<Set<string>>(new Set());
  const ceremony = useCeremony();


  // Honest failure (S2D.4/§5.1): this surface's primary read is agents.list().
  // A read that cannot succeed must say so and offer a retry, not sit in a
  // loading state forever.
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
    bridge.agents.grants(a.id).then(setGrants).catch(() => {});
  };

  const revokeGrant = async (g: Grant) => {
    if (!sel) return;
    const r = await ceremony.request({
      kind: "revoke", title: `Revoke grant ${g.id} — ${sel.name}`, origin: "user",
      // Revoking a capability always requires a human at HIC-1 (Rule 5).
      action: { actionClass: "grant.revoke", classification: "Proprietary", agent: "user", mandatoryHic1: true },
      rows: [{ k: "Grant", v: `${g.id} · ${g.classes}` }, { k: "Agent", v: `${sel.name} · ${sel.sbt}` }, { k: "Scope", v: g.scope }, { k: "Effect", v: "immediate — the capability is gone at the next checkpoint" }],
    });
    if (r.outcome === "settled") setRevoked((s) => new Set(s).add(g.id));
  };

  const killAll = async () => {
    if (!sel) return;
    const r = await ceremony.request({
      kind: "revoke", title: `Revoke ALL grants — ${sel.name}`, origin: "user",
      action: { actionClass: "grant.revoke-all", classification: "Proprietary", agent: "user", mandatoryHic1: true },
      rows: [{ k: "Agent", v: `${sel.name} · ${sel.sbt}` }, { k: "Grants revoked", v: `${grants.length} live grants` }, { k: "Keeps", v: "identity + history" }, { k: "Loses", v: "every capability" }, { k: "Running actions", v: "abort at the next checkpoint (<25s)" }],
    });
    if (r.outcome === "settled") setRevoked(new Set(grants.map((g) => g.id)));
  };

  const issueGrant = async () => {
    if (!sel) return;
    await ceremony.request({
      kind: "grant", title: `Issue grant — ${sel.name}`, origin: "user",
      action: { actionClass: "grant.issue", classification: "Proprietary", agent: "user", mandatoryHic1: true },
      rows: [{ k: "Agent", v: `${sel.name} · ${sel.sbt}` }, { k: "Action classes", v: "(configured in the grant form)" }, { k: "HIC level", v: "HIC-2 · budgeted autonomy" }, { k: "Expiry", v: "45-day maximum" }],
    });
  };

  const liveGrants = grants.filter((g) => !revoked.has(g.id));

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
              <span style={{ display: "flex", alignItems: "center", gap: 7, minWidth: 0 }}><span style={{ width: 8, height: 8, borderRadius: 999, background: VENDORS[a.vendor]?.color, flexShrink: 0 }} /><span style={{ fontSize: 13, fontWeight: 500 }}>{a.name}</span></span>
              <span className="mono" style={{ fontSize: 10, color: "var(--tx-3)" }}>{a.sbt}</span>
              <span className="mono" style={{ fontSize: 10, color: HIC_COLOR[a.hic] }}>HIC-{a.hic}</span>
              <span className="mono tabular" style={{ fontSize: 11 }}>{a.grants}</span>
              <span style={{ display: "flex", alignItems: "center", gap: 8 }}><div style={{ flex: 1, height: 7, background: "var(--srf-inset)", border: "1px solid var(--line-1)" }}><div style={{ height: "100%", width: `${pct}%`, background: bc }} /></div><span className="mono tabular" style={{ fontSize: 9.5, color: "var(--tx-3)" }}>{a.budgetUsed}/{a.budgetCap}</span></span>
              <span className="mono tabular" style={{ fontSize: 10, color: "var(--tx-3)" }}>{a.disputeRate}</span>
              <span className="mono" style={{ fontSize: 9, letterSpacing: ".08em", textTransform: "uppercase", color: STATUS_COLOR[a.status] }}>{a.status}</span>
            </div>
          );
        })}
        <div className="mono" style={{ fontSize: 9.5, color: "var(--tx-3)", padding: "8px 14px" }}>Read from agents.list() → AgentRegistry + GrantRegistry · lifecycle: register → probation HIC-0/1 → promotion, each step a ceremony</div>
      </div>

      {sel && (
        <div style={{ display: "grid", gridTemplateColumns: "1fr 1fr", gap: 14 }}>
          <div style={{ display: "flex", flexDirection: "column", gap: 14 }}>
            <div className="surface" style={{ padding: 16, display: "flex", flexDirection: "column", gap: 10 }}>
              <div style={{ display: "flex", alignItems: "center", gap: 10 }}>
                <span style={{ width: 10, height: 10, borderRadius: 999, background: VENDORS[sel.vendor]?.color }} />
                <span style={{ fontFamily: "var(--font-display)", fontWeight: 460, fontSize: 20 }}>{sel.name}</span>
                <span className="mono" style={{ fontSize: 9.5, color: "var(--tx-3)" }}>{VENDORS[sel.vendor]?.name} · {sel.sbt}</span>
                <div style={{ flex: 1 }} />
                <span className="mono" style={{ fontSize: 9, letterSpacing: ".08em", textTransform: "uppercase", color: STATUS_COLOR[sel.status], border: `1px solid ${STATUS_COLOR[sel.status]}`, padding: "2px 8px" }}>{sel.status}</span>
              </div>
              {sel.quarantine && <div style={{ border: "1px solid var(--danger)", background: "var(--danger-bg)", padding: "10px 12px", fontSize: 12.5, lineHeight: 1.5 }}>{sel.quarantine}</div>}
              <div style={{ display: "grid", gridTemplateColumns: "120px 1fr", gap: "6px 10px", fontSize: 12 }}>
                {([["DID", sel.did], ["Pubkey", sel.pubkey], ["Model", `${sel.model} · LoRA ${sel.lora}`], ["Adapter", `${sel.transport} · ${sel.sandbox} · egress ${sel.egress}`]] as [string, string][]).map(([k, v]) => (
                  <Fragment key={k}>
                    <span className="mono" style={{ fontSize: 9, letterSpacing: ".1em", textTransform: "uppercase", color: "var(--tx-3)", paddingTop: 2 }}>{k}</span>
                    <span className="mono" style={{ fontSize: 11 }}>{v}</span>
                  </Fragment>
                ))}
              </div>
              <div style={{ display: "flex", flexDirection: "column", gap: 5, borderTop: "1px solid var(--line-1)", paddingTop: 10 }}>
                <span className="eyebrow">Capsules — manifest-hash verified</span>
                {sel.capsules.map((cp) => (
                  <div key={cp.name} className="mono" style={{ display: "flex", gap: 10, fontSize: 10.5, alignItems: "baseline" }}><span>{cp.name}</span><span style={{ color: "var(--tx-3)" }}>{cp.hash}</span><span style={{ color: cp.verified ? "var(--ok)" : "var(--danger)" }}>{cp.verified ? "verified" : "UNVERIFIED — refuses to load"}</span></div>
                ))}
              </div>
            </div>
            <div className="surface" style={{ padding: 16, display: "flex", flexDirection: "column", gap: 8 }}>
              <span className="eyebrow">Reputation — measured facts, with denominators</span>
              {REP_LABELS.map(([key, label]) => {
                const [v, d] = sel.reputation[key];
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
                <button className="btn btn-primary btn-sm" onClick={issueGrant}>Issue grant</button>
              </div>
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
              <div className="mono" style={{ fontSize: 9.5, color: "var(--tx-3)", padding: "8px 14px" }}>agents.grants() → GrantRegistry · revocation is immediate and needs a signature</div>
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
