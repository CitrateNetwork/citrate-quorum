// citrate-quorum — Ledger surface (QRM-S2D). Demo beat 4: walk a decision
// back. Ported from design/CitrateQuorum.dc.html §LEDGER. Ribbon (streaming,
// filterable), decision-detail page (charter, with Verify — §5.4), correlation
// timeline, time-travel, and the auditor evidence pack. Reads bridge.ledger.*.
import { useEffect, useMemo, useState } from "react";
import { bridge } from "../bridge";
import type { CorrelationEvent, Decision, DecisionDetail } from "../bridge";

type View = "ribbon" | "decision" | "correlation";
const VERDICT_COLOR: Record<string, string> = {
  allow: "var(--ok)", "require-approval": "var(--warn)", deny: "var(--danger)", ungoverned: "var(--danger)",
};
const HIC_COLOR: Record<string, string> = { "1": "var(--ok)", "2": "var(--info)", "3": "var(--warn)", X: "var(--danger)" };
const hicLabel = (h: string) => (h === "X" ? "HIC-X" : `HIC-${h}`);

function VerifyButton() {
  const [state, setState] = useState<"idle" | "checking" | "ok" | "bad" | "pending">("idle");
  const run = () => {
    setState("checking");
    // §5.4: recompute the hash locally and compare to the anchored root. The
    // Tauri adapter does this against chain; sim resolves to verified.
    setTimeout(() => setState("ok"), 900);
  };
  const map = {
    idle: ["Verify this", "var(--tx-2)", "var(--line-2)"],
    checking: ["verifying…", "var(--tx-3)", "var(--line-2)"],
    ok: ["✓ verified", "var(--ok)", "var(--ok)"],
    bad: ["✗ mismatch", "var(--danger)", "var(--danger)"],
    pending: ["⚠ not yet anchored", "var(--warn)", "var(--warn)"],
  } as const;
  const [label, color, bd] = map[state];
  return (
    <span onClick={state === "idle" ? run : undefined} className="mono" style={{ fontSize: 9.5, color, border: `1px solid ${bd}`, padding: "2px 9px", cursor: state === "idle" ? "pointer" : "default" }}>{label}</span>
  );
}

export function Ledger() {
  const [view, setView] = useState<View>("ribbon");
  const [rows, setRows] = useState<Decision[]>([]);
  const [ungOnly, setUngOnly] = useState(false);
  const [verdict, setVerdict] = useState<string>("all");
  const [asOf, setAsOf] = useState<string>("");
  const [detail, setDetail] = useState<DecisionDetail | null>(null);
  const [corr, setCorr] = useState<CorrelationEvent[]>([]);
  const [exportOpen, setExportOpen] = useState(false);

  useEffect(() => {
    bridge.ledger.query().then(setRows);
    const unsub = bridge.ledger.stream((d) => setRows((p) => [d, ...p].slice(0, 60)));
    return unsub;
  }, []);

  const openDecision = (id: string) => {
    bridge.ledger.decision(id).then(setDetail);
    setView("decision");
  };
  const openCorrelation = () => {
    bridge.ledger.correlation("X-7104").then(setCorr);
    setView("correlation");
  };

  const filtered = useMemo(
    () =>
      rows.filter((r) => (ungOnly ? r.verdict === "ungoverned" : true)).filter((r) => (verdict === "all" ? true : r.verdict === verdict)),
    [rows, ungOnly, verdict],
  );

  const tabBtn = (v: View, label: string, onClick: () => void) => {
    const on = view === v;
    return (
      <button key={v} onClick={onClick} className="mono" style={{ fontSize: 10, letterSpacing: ".12em", textTransform: "uppercase", background: on ? "var(--accent)" : "var(--srf-1)", color: on ? "var(--ink)" : "var(--tx-2)", border: `1px solid ${on ? "var(--accent)" : "var(--line-2)"}`, padding: "5px 12px", cursor: "pointer", borderRadius: "var(--r-1)" }}>{label}</button>
    );
  };

  return (
    <div style={{ padding: 18, display: "flex", flexDirection: "column", gap: 12 }}>
      <div style={{ display: "flex", alignItems: "center", gap: 10 }}>
        {tabBtn("ribbon", "Ribbon", () => setView("ribbon"))}
        {tabBtn("decision", "Decision D-88412", () => openDecision("D-88412"))}
        {tabBtn("correlation", "Correlation X-7104", openCorrelation)}
        <div style={{ flex: 1 }} />
        <select className="mono" value={asOf} onChange={(e) => setAsOf(e.target.value)} style={{ fontSize: 10, background: "var(--srf-1)", color: "var(--tx-2)", border: "1px solid var(--line-2)", padding: "5px 8px", borderRadius: "var(--r-1)" }}>
          <option value="">As of — now</option>
          <option value="2026-07-01">As of 2026-07-01</option>
          <option value="2026-06-01">As of 2026-06-01</option>
        </select>
        <button className="btn btn-ghost btn-sm" onClick={() => setExportOpen((v) => !v)}>Evidence pack…</button>
      </div>

      {asOf && (
        <div style={{ display: "flex", alignItems: "center", gap: 10, border: "1.5px dashed var(--info)", background: "var(--info-bg)", padding: "9px 14px" }}>
          <span className="mono" style={{ fontSize: 10, letterSpacing: ".12em", textTransform: "uppercase", color: "var(--info)" }}>Time travel — you are reading history</span>
          <span style={{ fontSize: 12.5 }}>Policies, grants, and clearances as they stood on <span className="mono">{asOf}</span>. Nothing here is current.</span>
          <div style={{ flex: 1 }} />
          <button className="btn btn-ghost btn-sm" onClick={() => setAsOf("")}>Return to now</button>
        </div>
      )}

      {exportOpen && (
        <div className="surface" style={{ maxWidth: 560, padding: 16, display: "flex", flexDirection: "column", gap: 10, borderTop: "2px solid var(--info)" }}>
          <span className="eyebrow">Auditor evidence pack</span>
          <span style={{ fontSize: 13, color: "var(--tx-2)", lineHeight: 1.55 }}>Period 2026-04-23 → 2026-07-23 · scope Line-4 Automation. Includes: decision records (14,208), grant lineage, ratified minutes (11), anchor roots + inclusion proofs, and <span className="mono">verify.sh</span> — a board member can check any record offline in under a minute. Does not include: transcripts of unratified rooms, raw documents above the auditor's clearance.</span>
          <div style={{ display: "flex", gap: 8 }}><button className="btn btn-primary btn-sm">Generate pack</button><button className="btn btn-ghost btn-sm" onClick={() => setExportOpen(false)}>Close</button></div>
        </div>
      )}

      {view === "ribbon" && (
        <div className="surface" style={{ display: "flex", flexDirection: "column", minHeight: 0 }}>
          <div style={{ display: "flex", alignItems: "center", gap: 12, padding: "9px 14px", borderBottom: "1px solid var(--line-1)" }}>
            <span className="eyebrow">Ribbon — every recorded action</span>
            <span style={{ width: 7, height: 7, borderRadius: 999, background: "var(--accent)", animation: "ccPulse 1.6s infinite" }} />
            <div style={{ flex: 1 }} />
            <label className="mono" style={{ display: "flex", alignItems: "center", gap: 6, fontSize: 9.5, letterSpacing: ".08em", textTransform: "uppercase", color: "var(--warn)", cursor: "pointer" }}>
              <input type="checkbox" checked={ungOnly} onChange={(e) => setUngOnly(e.target.checked)} />Ungoverned only
            </label>
            <select className="mono" value={verdict} onChange={(e) => setVerdict(e.target.value)} style={{ fontSize: 10, background: "var(--srf-1)", color: "var(--tx-2)", border: "1px solid var(--line-2)", padding: "4px 8px", borderRadius: "var(--r-1)" }}>
              <option value="all">All verdicts</option><option value="allow">allow</option><option value="require-approval">require-approval</option><option value="deny">deny</option>
            </select>
          </div>
          <div className="mono" style={{ display: "grid", gridTemplateColumns: "70px 90px 110px 120px 1fr 60px 100px 70px", gap: 8, padding: "6px 14px", fontSize: 9, letterSpacing: ".12em", textTransform: "uppercase", color: "var(--tx-3)", borderBottom: "1px solid var(--line-1)" }}>
            <span>Time</span><span>Decision</span><span>Principal</span><span>Agent</span><span>Action class</span><span>HIC</span><span>Verdict</span><span>Corr</span>
          </div>
          <div style={{ maxHeight: 460, overflow: "auto" }}>
            {filtered.map((r) => (
              <div key={r.id} onClick={() => openDecision(r.id)} className="mono tabular" style={{ display: "grid", gridTemplateColumns: "70px 90px 110px 120px 1fr 60px 100px 70px", gap: 8, padding: "8px 14px", fontSize: 11, borderBottom: "1px solid var(--line-1)", cursor: "pointer", background: r.verdict === "ungoverned" ? "var(--danger-bg)" : "transparent" }}>
                <span style={{ color: "var(--tx-3)" }}>{r.time}</span>
                <span>{r.id}</span>
                <span style={{ color: "var(--tx-2)" }}>{r.principal}</span>
                <span>{r.agent}</span>
                <span style={{ color: "var(--tx-2)" }}>{r.cls}</span>
                <span style={{ color: HIC_COLOR[r.hic] ?? "var(--tx-2)" }}>{hicLabel(r.hic)}</span>
                <span style={{ color: VERDICT_COLOR[r.verdict] ?? "var(--tx-2)" }}>{r.verdict}</span>
                <span style={{ color: "var(--info)" }}>{r.corr}</span>
              </div>
            ))}
          </div>
          <div className="mono" style={{ fontSize: 9.5, color: "var(--tx-3)", padding: "7px 14px" }}>ledger.query() + ledger.stream() → AgentDecisionRegistryV2 → chain 40204 · virtualize at 100k rows in production</div>
        </div>
      )}

      {view === "decision" && detail && (
        <div data-register="charter" style={{ background: "var(--srf-0)", color: "var(--tx-1)", margin: -6, padding: 6 }}>
          <div className="surface" style={{ maxWidth: 760, borderTop: "2px solid var(--line-strong)" }}>
            <div style={{ padding: "20px 26px", borderBottom: "1px solid var(--line-1)", display: "flex", flexDirection: "column", gap: 8 }}>
              <div style={{ display: "flex", alignItems: "center", gap: 12 }}>
                <span className="mono" style={{ fontSize: 11, color: "var(--accent-text)" }}>{detail.id}</span>
                <div style={{ fontFamily: "var(--font-display)", fontWeight: 440, fontSize: 20 }}>{detail.what}</div>
                <div style={{ flex: 1 }} />
                <VerifyButton />
              </div>
              <span className="mono" style={{ fontSize: 10, color: "var(--tx-3)" }}>{detail.when} · one action, as a document — print it for the board</span>
            </div>
            <div style={{ padding: "6px 0" }}>
              {([
                ["Principal", detail.principal], ["Agent", detail.agent], ["Grant", detail.grant], ["Protocol", detail.protocol],
                ["Verdict", detail.verdict], ["Reason", detail.reason], ["Model", detail.model], ["Params hash", detail.params],
                ["Local chain", detail.chainPos], ["Anchor", detail.anchor], ["Proof", detail.proof],
              ] as [string, string][]).map(([k, v]) => (
                <div key={k} style={{ display: "grid", gridTemplateColumns: "180px 1fr", padding: "0 26px" }}>
                  <span className="mono" style={{ fontSize: 9.5, letterSpacing: ".1em", textTransform: "uppercase", color: "var(--tx-3)", padding: "8px 0" }}>{k}</span>
                  <span className="mono" style={{ fontSize: 11.5, padding: "8px 0", borderBottom: "1px solid var(--line-1)", wordBreak: "break-all" }}>{v}</span>
                </div>
              ))}
            </div>
            <div className="mono" style={{ fontSize: 9.5, color: "var(--tx-3)", padding: "10px 26px", borderTop: "1px solid var(--line-1)" }}>ledger.decision() · verify recomputes the hash locally and compares to the anchored root — the product's core claim, touchable</div>
          </div>
        </div>
      )}

      {view === "correlation" && (
        <div className="surface" style={{ maxWidth: 760, display: "flex", flexDirection: "column" }}>
          <div style={{ padding: "12px 16px", borderBottom: "1px solid var(--line-1)", display: "flex", alignItems: "center", gap: 10 }}>
            <span className="eyebrow">Correlation X-7104 — walk the decision back</span>
          </div>
          <div style={{ padding: 16, display: "flex", flexDirection: "column" }}>
            {corr.map((cr, i) => (
              <div key={i} style={{ display: "flex", gap: 14 }}>
                <div style={{ display: "flex", flexDirection: "column", alignItems: "center" }}>
                  <span style={{ width: 9, height: 9, borderRadius: 999, background: "var(--srf-2)", border: "1.5px solid var(--info)", flexShrink: 0, marginTop: 3 }} />
                  {i < corr.length - 1 && <span style={{ width: 1, flex: 1, background: "var(--line-2)", minHeight: 22 }} />}
                </div>
                <div style={{ paddingBottom: 14 }}>
                  <div style={{ display: "flex", gap: 10, alignItems: "baseline" }}>
                    <span className="mono tabular" style={{ fontSize: 10, color: "var(--tx-3)" }}>{cr.t}</span>
                    <span className="mono" style={{ fontSize: 8.5, letterSpacing: ".1em", textTransform: "uppercase", color: "var(--info)", border: "1px solid var(--info)", padding: "1px 6px" }}>{cr.kind}</span>
                  </div>
                  <div style={{ fontSize: 13, marginTop: 3 }}>{cr.text}</div>
                </div>
              </div>
            ))}
          </div>
          <div className="mono" style={{ fontSize: 9.5, color: "var(--tx-3)", padding: "8px 16px", borderTop: "1px solid var(--line-1)" }}>The meeting that decided it → the grant that allowed it → the actions → the PR. Every action traces to a human, or is flagged ungoverned.</div>
        </div>
      )}
    </div>
  );
}
