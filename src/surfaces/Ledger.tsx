// citrate-quorum — Ledger surface. Demo beat 4: walk a decision back.
//
// LIVE: the ribbon, one decision as a document, the correlation timeline, and
// Verify — all reads of this tenant's real BLAKE3 evidence chain.
//
// What changed in Phase 0, and why:
//   · Verify used to be a 900ms timer that always ended in "✓ verified". It now
//     replays the chain from genesis AND recomputes the Merkle root from the
//     record's own hash plus its inclusion proof, and reports the two results
//     separately. A verify affordance that cannot fail is worse than none.
//   · The Decision and Correlation tabs used to be hardcoded ids (D-88412,
//     X-7104) that existed in no chain. They now open from a clicked row.
//   · The evidence-pack panel claimed a period, a record count and a set of
//     included artifacts. None of it was built; it says so.
//   · The as-of/time-travel selector did nothing at all. It is gone.
//
// Reads bridge.ledger.query()/stream()/state()/decision()/verifyDecision()/
// correlation().
import { useEffect, useMemo, useState } from "react";
import { bridge } from "../bridge";
import { DomainErrorPlate, useDomain } from "../components/DomainState";
import type { CorrelationView, Decision, DecisionDetail, LedgerState, VerifyDecision } from "../bridge";

type View = "ribbon" | "decision" | "correlation";
const VERDICT_COLOR: Record<string, string> = {
  allow: "var(--ok)", "require-approval": "var(--warn)", deny: "var(--danger)", ungoverned: "var(--danger)",
  // A human refusing is not the same event as policy refusing — it reads as a
  // deliberate act, not a failure.
  rejected: "var(--info)",
};
const HIC_COLOR: Record<string, string> = { "1": "var(--ok)", "2": "var(--info)", "3": "var(--warn)", X: "var(--danger)" };
const hicLabel = (h: string) => (h === "X" ? "HIC-X" : `HIC-${h}`);

/**
 * Verify one decision, for real.
 *
 * Two independent checks, reported separately: the chain replays from genesis
 * (nothing was edited, reordered or removed) and this record proves into the
 * tenant's Merkle root (this specific record is in it). Neither says anything
 * about the chain — collapsing local integrity into on-chain agreement is
 * exactly how a verify button becomes theatre.
 */
function VerifyButton({ id }: { id: string }) {
  const [state, setState] = useState<"idle" | "checking" | "done" | "failed">("idle");
  const [result, setResult] = useState<VerifyDecision | null>(null);
  const [err, setErr] = useState<string | null>(null);

  const run = () => {
    setState("checking");
    bridge.ledger
      .verifyDecision(id)
      .then((v) => { setResult(v); setState("done"); })
      .catch((e: unknown) => { setErr(e instanceof Error ? e.message : String(e)); setState("failed"); });
  };

  const ok = result?.chainIntact === true && result?.included === true;
  const label =
    state === "idle" ? "Verify this"
    : state === "checking" ? "verifying…"
    : state === "failed" ? "✗ could not verify"
    : ok ? "✓ verified"
    : "✗ MISMATCH";
  const color =
    state === "done" ? (ok ? "var(--ok)" : "var(--danger)")
    : state === "failed" ? "var(--danger)"
    : "var(--tx-2)";

  return (
    <span style={{ display: "inline-flex", flexDirection: "column", alignItems: "flex-end", gap: 4 }}>
      <span
        onClick={state === "idle" || state === "failed" ? run : undefined}
        className="mono"
        style={{ fontSize: 9.5, color, border: `1px solid ${color}`, padding: "2px 9px", cursor: state === "checking" ? "default" : "pointer" }}
      >
        {label}
      </span>
      {result && (
        <span className="mono" style={{ fontSize: 9, color: "var(--tx-3)", textAlign: "right", lineHeight: 1.6 }}>
          chain replayed from genesis: {result.chainIntact ? "intact" : "ALTERED"} · {result.records} records
          <br />
          inclusion in the merkle root: {result.included ? `proved (${result.proofLen} siblings)` : "NOT PROVED"}
        </span>
      )}
      {err && <span className="mono" style={{ fontSize: 9, color: "var(--danger)", textAlign: "right" }}>{err}</span>}
    </span>
  );
}

export function Ledger() {
  const [view, setView] = useState<View>("ribbon");
  const [rows, setRows] = useState<Decision[]>([]);
  const [ungOnly, setUngOnly] = useState(false);
  const [verdict, setVerdict] = useState<string>("all");
  const [detail, setDetail] = useState<DecisionDetail | null>(null);
  const [detailErr, setDetailErr] = useState<string | null>(null);
  const [corr, setCorr] = useState<CorrelationView | null>(null);
  const [corrId, setCorrId] = useState<string>("");
  const [chain, setChain] = useState<LedgerState | null>(null);
  const [exportOpen, setExportOpen] = useState(false);

  useEffect(() => {
    let unsub: (() => void) | undefined;
    try {
      unsub = bridge.ledger.stream((d) =>
        setRows((p) => (p.some((r) => r.id === d.id) ? p : [d, ...p].slice(0, 60))),
      );
    } catch {
      /* the query below already populated the table */
    }
    return unsub;
  }, []);

  // Honest failure (S2D.4/§5.1): this surface's primary read is ledger.query().
  const primary = useDomain(() => bridge.ledger.query(), "ledger.query()");
  // Deliberate, unlike the other surfaces: `rows` is SEEDED from the query and
  // then appended to by the live decision stream above, so it has two writers
  // and cannot be a derived constant.
  useEffect(() => {
    // eslint-disable-next-line react-hooks/set-state-in-effect -- see above
    if (primary.state.status === "ready") setRows(primary.state.data);
  }, [primary.state]);

  // The chain's own state, refreshed alongside the ribbon.
  useEffect(() => {
    if (primary.state.status !== "ready") return;
    let live = true;
    bridge.ledger.state().then((s) => { if (live) setChain(s); }).catch(() => {
      /* the strip is a decoration; the ribbon's own error plate covers a dead backend */
    });
    return () => { live = false; };
  }, [primary.state]);

  const openDecision = (id: string) => {
    setDetail(null);
    setDetailErr(null);
    setView("decision");
    bridge.ledger
      .decision(id)
      .then(setDetail)
      .catch((e: unknown) => setDetailErr(e instanceof Error ? e.message : String(e)));
  };
  const openCorrelation = (id: string) => {
    if (!id) return;
    setCorr(null);
    setCorrId(id);
    setView("correlation");
    bridge.ledger.correlation(id).then(setCorr).catch(() => setCorr({ events: [], source: `correlation ${id} could not be read` }));
  };

  const filtered = useMemo(
    () =>
      rows.filter((r) => (ungOnly ? r.verdict === "ungoverned" : true)).filter((r) => (verdict === "all" ? true : r.verdict === verdict)),
    [rows, ungOnly, verdict],
  );

  // Placed after EVERY hook: an early return above a useMemo makes the
  // hook run conditionally, and React crashes with "rendered fewer hooks
  // than expected" the moment this read fails.
  if (primary.state.status === "error") {
    return (
      <div style={{ padding: 18 }}>
        <DomainErrorPlate source="ledger.query()" error={primary.state.error} onRetry={primary.retry} />
      </div>
    );
  }

  const tabBtn = (v: View, label: string, onClick: () => void, disabled = false) => {
    const on = view === v;
    return (
      <button key={v} onClick={onClick} disabled={disabled} className="mono" style={{ fontSize: 10, letterSpacing: ".12em", textTransform: "uppercase", background: on ? "var(--accent)" : "var(--srf-1)", color: on ? "var(--ink)" : "var(--tx-2)", border: `1px solid ${on ? "var(--accent)" : "var(--line-2)"}`, padding: "5px 12px", cursor: disabled ? "not-allowed" : "pointer", borderRadius: "var(--r-1)", opacity: disabled ? 0.5 : 1 }}>{label}</button>
    );
  };

  return (
    <div style={{ padding: 18, display: "flex", flexDirection: "column", gap: 12 }}>
      <div style={{ display: "flex", alignItems: "center", gap: 10, flexWrap: "wrap" }}>
        {tabBtn("ribbon", "Ribbon", () => setView("ribbon"))}
        {tabBtn("decision", detail ? `Decision ${detail.id}` : "Decision", () => setView("decision"), !detail)}
        {tabBtn("correlation", corrId ? `Correlation ${corrId}` : "Correlation", () => setView("correlation"), !corrId)}
        <div style={{ flex: 1 }} />
        <button className="btn btn-ghost btn-sm" onClick={() => setExportOpen((v) => !v)}>Evidence pack…</button>
      </div>

      {chain && (
        <div className="mono" style={{ fontSize: 9.5, color: "var(--tx-3)", lineHeight: 1.7, wordBreak: "break-all" }}>
          tenant {chain.tenant} · {chain.records} record{chain.records === 1 ? "" : "s"} · head {chain.head}
          <br />
          merkle root {chain.merkleRoot} ·{" "}
          <span style={{ color: chain.intact ? "var(--ok)" : "var(--danger)" }}>
            {chain.intact ? "chain replays intact" : "CHAIN REPLAY FAILED"}
          </span>
          {chain.ungoverned > 0 && <span style={{ color: "var(--danger)" }}> · {chain.ungoverned} ungoverned</span>}
        </div>
      )}

      {exportOpen && (
        <div className="surface" style={{ maxWidth: 620, padding: 16, display: "flex", flexDirection: "column", gap: 10, borderTop: "2px solid var(--line-strong)" }}>
          <span className="eyebrow">Auditor evidence pack — not built</span>
          <span style={{ fontSize: 13, color: "var(--tx-2)", lineHeight: 1.6 }}>
            The pack — decision records for a period, grant lineage, ratified minutes, the anchored roots, and an
            offline <span className="mono">verify.sh</span> a board member can run without this app — is not
            implemented, so nothing is generated here. The pieces it would bundle are real and readable today: the
            ribbon below, each decision's document, and each decision's inclusion proof. It lands in{" "}
            <span className="mono">QRM-S9</span>.
          </span>
          <div><button className="btn btn-ghost btn-sm" onClick={() => setExportOpen(false)}>Close</button></div>
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
              <option value="all">All verdicts</option><option value="allow">allow</option><option value="require-approval">require-approval</option><option value="approved">approved</option><option value="rejected">rejected</option><option value="deny">deny</option><option value="ungoverned">ungoverned</option>
            </select>
          </div>
          <div className="mono" style={{ display: "grid", gridTemplateColumns: "70px 90px 110px 120px 1fr 60px 100px 90px", gap: 8, padding: "6px 14px", fontSize: 9, letterSpacing: ".12em", textTransform: "uppercase", color: "var(--tx-3)", borderBottom: "1px solid var(--line-1)" }}>
            <span>Time</span><span>Decision</span><span>Principal</span><span>Agent</span><span>Action class</span><span>HIC</span><span>Verdict</span><span>Corr</span>
          </div>
          <div style={{ maxHeight: 460, overflow: "auto" }}>
            {filtered.length === 0 && (
              <div style={{ padding: "18px 14px", fontSize: 12.5, color: "var(--tx-3)", lineHeight: 1.6 }}>
                {rows.length === 0
                  ? "This tenant's evidence chain holds no records yet. It fills as governed actions are evaluated — nothing is pre-populated."
                  : "No record matches this filter."}
              </div>
            )}
            {filtered.map((r) => (
              <div key={r.id} className="mono tabular" style={{ display: "grid", gridTemplateColumns: "70px 90px 110px 120px 1fr 60px 100px 90px", gap: 8, padding: "8px 14px", fontSize: 11, borderBottom: "1px solid var(--line-1)", background: r.verdict === "ungoverned" ? "var(--danger-bg)" : "transparent" }}>
                <span style={{ color: "var(--tx-3)" }}>{r.time}</span>
                <span onClick={() => openDecision(r.id)} style={{ cursor: "pointer", textDecoration: "underline" }}>{r.id}</span>
                <span style={{ color: "var(--tx-2)", overflow: "hidden", textOverflow: "ellipsis" }}>{r.principal}</span>
                <span style={{ overflow: "hidden", textOverflow: "ellipsis" }}>{r.agent}</span>
                <span style={{ color: "var(--tx-2)", overflow: "hidden", textOverflow: "ellipsis" }}>{r.cls}</span>
                <span style={{ color: HIC_COLOR[r.hic] ?? "var(--tx-2)" }}>{hicLabel(r.hic)}</span>
                <span style={{ color: VERDICT_COLOR[r.verdict] ?? "var(--tx-2)" }}>{r.verdict}</span>
                <span
                  onClick={() => openCorrelation(r.corr)}
                  style={{ color: r.corr ? "var(--info)" : "var(--tx-3)", cursor: r.corr ? "pointer" : "default", overflow: "hidden", textOverflow: "ellipsis" }}
                  title={r.corr}
                >
                  {r.corr || "—"}
                </span>
              </div>
            ))}
          </div>
          <div className="mono" style={{ fontSize: 9.5, color: "var(--tx-3)", padding: "7px 14px", lineHeight: 1.6 }}>
            ledger.query() + ledger.stream() → the tenant's local BLAKE3 evidence chain. It is not read from a
            contract: the chain is local, durable and works offline, and what goes on chain is a commitment to it.
          </div>
        </div>
      )}

      {view === "decision" && (
        <div data-register="charter" style={{ background: "var(--srf-0)", color: "var(--tx-1)", margin: -6, padding: 6 }}>
          {detailErr && (
            <div className="mono" style={{ fontSize: 11, color: "var(--danger)", padding: 12 }}>ledger.decision() — {detailErr}</div>
          )}
          {!detail && !detailErr && <div className="mono" style={{ fontSize: 11, color: "var(--tx-3)", padding: 12 }}>reading…</div>}
          {detail && (
            <div className="surface" style={{ maxWidth: 800, borderTop: "2px solid var(--line-strong)" }}>
              <div style={{ padding: "20px 26px", borderBottom: "1px solid var(--line-1)", display: "flex", flexDirection: "column", gap: 8 }}>
                <div style={{ display: "flex", alignItems: "flex-start", gap: 12 }}>
                  <span className="mono" style={{ fontSize: 11, color: "var(--accent-text)" }}>{detail.id}</span>
                  <div style={{ fontFamily: "var(--font-display)", fontWeight: 440, fontSize: 20 }}>{detail.what}</div>
                  <div style={{ flex: 1 }} />
                  <VerifyButton id={detail.id} />
                </div>
                <span className="mono" style={{ fontSize: 10, color: "var(--tx-3)" }}>{detail.when} · one action, as a document — print it for the board</span>
              </div>
              <div style={{ padding: "6px 0" }}>
                {([
                  ["Principal", detail.principal], ["Agent", detail.agent], ["Grant", detail.grant],
                  ["Verdict", `${detail.verdict} · ${hicLabel(detail.hic)}`], ["Reason", detail.reason],
                  ["Protocol", detail.protocol], ["Model", detail.model], ["Params hash", detail.params],
                  ["Correlation", detail.correlation || "— none"],
                  ["Position", detail.chainPos],
                  ["Entry hash", detail.entryHash],
                  ["Record hash", detail.contentHash],
                  ["Chain head", detail.chainHead],
                  ["Merkle root", detail.merkleRoot],
                  ["Inclusion", detail.included
                    ? `proved into the root with ${detail.proofLen} sibling hash${detail.proofLen === 1 ? "" : "es"}`
                    : "NOT PROVED — this record did not recompute the root"],
                ] as [string, string][]).map(([k, v]) => (
                  <div key={k} style={{ display: "grid", gridTemplateColumns: "180px 1fr", padding: "0 26px" }}>
                    <span className="mono" style={{ fontSize: 9.5, letterSpacing: ".1em", textTransform: "uppercase", color: "var(--tx-3)", padding: "8px 0" }}>{k}</span>
                    <span className="mono" style={{ fontSize: 11.5, padding: "8px 0", borderBottom: "1px solid var(--line-1)", wordBreak: "break-all", color: k === "Inclusion" && !detail.included ? "var(--danger)" : "inherit" }}>{v}</span>
                  </div>
                ))}
              </div>
              <div className="mono" style={{ fontSize: 9.5, color: "var(--tx-3)", padding: "10px 26px", borderTop: "1px solid var(--line-1)", lineHeight: 1.6 }}>
                ledger.decision() · {detail.source}
                <br />
                Verify recomputes both hashes locally: the chain from genesis, and this record into the Merkle root.
                Whether the chain agrees is a separate question, asked on the meeting that anchored it.
              </div>
            </div>
          )}
        </div>
      )}

      {view === "correlation" && (
        <div className="surface" style={{ maxWidth: 800, display: "flex", flexDirection: "column" }}>
          <div style={{ padding: "12px 16px", borderBottom: "1px solid var(--line-1)", display: "flex", alignItems: "center", gap: 10 }}>
            <span className="eyebrow">Correlation {corrId} — walk the decision back</span>
          </div>
          <div style={{ padding: 16, display: "flex", flexDirection: "column" }}>
            {!corr && <span className="mono" style={{ fontSize: 11, color: "var(--tx-3)" }}>reading…</span>}
            {corr?.events.map((cr, i) => (
              <div key={`${cr.link}-${i}`} style={{ display: "flex", gap: 14 }}>
                <div style={{ display: "flex", flexDirection: "column", alignItems: "center" }}>
                  <span style={{ width: 9, height: 9, borderRadius: 999, background: "var(--srf-2)", border: "1.5px solid var(--info)", flexShrink: 0, marginTop: 3 }} />
                  {i < corr.events.length - 1 && <span style={{ width: 1, flex: 1, background: "var(--line-2)", minHeight: 22 }} />}
                </div>
                <div style={{ paddingBottom: 14 }}>
                  <div style={{ display: "flex", gap: 10, alignItems: "baseline" }}>
                    <span className="mono tabular" style={{ fontSize: 10, color: "var(--tx-3)" }}>{cr.t}</span>
                    <span className="mono" style={{ fontSize: 8.5, letterSpacing: ".1em", textTransform: "uppercase", color: "var(--info)", border: "1px solid var(--info)", padding: "1px 6px" }}>{cr.kind}</span>
                    <span onClick={() => openDecision(cr.link)} className="mono" style={{ fontSize: 10, color: "var(--info)", cursor: "pointer", textDecoration: "underline" }}>{cr.link}</span>
                  </div>
                  <div style={{ fontSize: 13, marginTop: 3 }}>{cr.text}</div>
                </div>
              </div>
            ))}
          </div>
          <div className="mono" style={{ fontSize: 9.5, color: "var(--tx-3)", padding: "8px 16px", borderTop: "1px solid var(--line-1)", lineHeight: 1.6 }}>
            ledger.correlation() · {corr?.source ?? ""}
          </div>
        </div>
      )}
    </div>
  );
}
