// citrate-quorum — Repos surface (QRM-S2D). Instrument register. Ported from
// design §REPOS. Connected repos (with the ITAR redaction plate — a denied
// surface names the classification + who can grant it, never a silent omission),
// open PRs (agent-authored flagged), and the hash-pinned Peek citation. Reads
// bridge.repos.list()/prs()/peek().
import { useEffect, useState } from "react";
import { bridge } from "../bridge";
import { DomainErrorPlate, useDomain } from "../components/DomainState";
import type { Peek, Pr, Repo } from "../bridge";

const CHECK_COLOR: Record<string, string> = { green: "var(--ok)", running: "var(--warn)", amber: "var(--warn)", red: "var(--danger)" };

export function Repos() {
  const [repos, setRepos] = useState<Repo[]>([]);
  const [prs, setPrs] = useState<Pr[]>([]);
  const [peek, setPeek] = useState<Peek | null>(null);
  useEffect(() => { bridge.repos.prs().then(setPrs).catch(() => {}); bridge.repos.peek("").then(setPeek).catch(() => {}); }, []);

  // Honest failure (S2D.4/§5.1): this surface's primary read is repos.list().
  // A read that cannot succeed must say so and offer a retry, not sit in a
  // loading state forever.
  const primary = useDomain(() => bridge.repos.list(), "repos.list()");
  useEffect(() => {
    if (primary.state.status === "ready") setRepos(primary.state.data);
  }, [primary.state]);
  if (primary.state.status === "error") {
    return (
      <div style={{ padding: 18 }}>
        <DomainErrorPlate source="repos.list()" error={primary.state.error} onRetry={primary.retry} lands="It lands in QRM-S8 (calendar + repos)." />
      </div>
    );
  }
  return (
    <div style={{ padding: 18, display: "grid", gridTemplateColumns: "1fr 1fr", gap: 14 }}>
      <div style={{ display: "flex", flexDirection: "column", gap: 14 }}>
        <div className="surface" style={{ display: "flex", flexDirection: "column" }}>
          <div style={{ padding: "10px 14px", borderBottom: "1px solid var(--line-1)" }}><span className="eyebrow">Connected repositories</span></div>
          {repos.map((r) => (
            <div key={r.id} style={{ display: "flex", flexDirection: "column", gap: 4, padding: "11px 14px", borderBottom: "1px solid var(--line-1)" }}>
              {r.denied ? (
                <>
                  <div style={{ display: "flex", alignItems: "center", gap: 10 }}>
                    <span className="mono" style={{ fontSize: 12, color: "var(--tx-3)" }}>{r.name}</span>
                    <span className="mono" style={{ fontSize: 8.5, letterSpacing: ".1em", color: "var(--danger)", border: "1px solid var(--danger)", padding: "1px 6px" }}>{r.denied}</span>
                  </div>
                  <div style={{ border: "1.5px dashed var(--danger)", padding: "9px 11px", fontSize: 11.5, color: "var(--tx-2)", lineHeight: 1.5 }}>Redacted — requires {r.denied} clearance. Your seat is cleared to CUI. Grantable by: {r.deniedWho}. A silent omission would be indistinguishable from "nothing here" — that is why this plate exists.</div>
                </>
              ) : (
                <>
                  <div style={{ display: "flex", alignItems: "center", gap: 10 }}>
                    <span className="mono" style={{ fontSize: 12, fontWeight: 500 }}>{r.name}</span>
                    <span style={{ width: 7, height: 7, borderRadius: 999, background: CHECK_COLOR[r.checks ?? ""] ?? "var(--line-2)" }} />
                    <span className="mono" style={{ fontSize: 9, color: "var(--tx-3)" }}>checks {r.checks}</span>
                    <div style={{ flex: 1 }} />
                    <span className="mono tabular" style={{ fontSize: 9.5, color: "var(--tx-3)" }}>{r.branches} branches · {r.prs} PRs · {r.last}</span>
                  </div>
                  <span className="mono" style={{ fontSize: 9.5, color: "var(--tx-3)" }}>write access: {r.agents}</span>
                </>
              )}
            </div>
          ))}
        </div>
        <div className="surface" style={{ display: "flex", flexDirection: "column" }}>
          <div style={{ padding: "10px 14px", borderBottom: "1px solid var(--line-1)" }}><span className="eyebrow">Open pull requests</span></div>
          {prs.map((pr) => (
            <div key={pr.id} style={{ display: "flex", alignItems: "center", gap: 10, padding: "10px 14px", borderBottom: "1px solid var(--line-1)" }}>
              <span style={{ width: 7, height: 7, borderRadius: 999, background: CHECK_COLOR[pr.checks] ?? "var(--line-2)", flexShrink: 0 }} />
              <span style={{ fontSize: 12.5, flex: 1, minWidth: 0, overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" }}>{pr.title}</span>
              {pr.agent ? <span className="mono" style={{ fontSize: 8.5, letterSpacing: ".08em", color: "var(--info)", border: "1px solid var(--info)", padding: "1px 6px" }}>AGENT · {pr.by}</span> : <span className="mono" style={{ fontSize: 8.5, color: "var(--tx-3)" }}>{pr.by}</span>}
              <span className="mono tabular" style={{ fontSize: 9.5, color: "var(--tx-3)" }}>{pr.age}</span>
            </div>
          ))}
          <div className="mono" style={{ fontSize: 9.5, color: "var(--tx-3)", padding: "8px 14px" }}>repos.prs() · agent-authored PRs are flagged and trace to their grant</div>
        </div>
      </div>
      <div className="surface" style={{ display: "flex", flexDirection: "column", height: "fit-content" }}>
        <div style={{ padding: "10px 14px", borderBottom: "1px solid var(--line-1)", display: "flex", alignItems: "center", gap: 10 }}>
          <span className="eyebrow">Peek — the citation component</span>
          <div style={{ flex: 1 }} />
          <span className="mono" style={{ fontSize: 9, color: "var(--ok)", border: "1px solid var(--ok)", padding: "1px 7px" }}>✓ verified</span>
        </div>
        <div className="mono" style={{ fontSize: 10, color: "var(--info)", padding: "9px 14px", borderBottom: "1px solid var(--line-1)" }}>{peek?.ref} · pinned {peek?.hash}</div>
        <div style={{ padding: "10px 0", background: "var(--srf-inset)" }}>
          {peek?.lines.map(([n, code]) => (
            <div key={n} className="mono" style={{ display: "grid", gridTemplateColumns: "44px 1fr", fontSize: 10.5, lineHeight: 1.7 }}>
              <span className="tabular" style={{ color: "var(--tx-3)", textAlign: "right", paddingRight: 12 }}>{n}</span>
              <span style={{ whiteSpace: "pre", color: code.trimStart().startsWith("//") ? "var(--tx-3)" : "var(--tx-1)" }}>{code}</span>
            </div>
          ))}
        </div>
        <div className="mono" style={{ fontSize: 9.5, color: "var(--tx-3)", padding: "8px 14px" }}>repos.peek(ref) — hash-pinned so the citation cannot drift after the meeting. Opens inside rooms without navigating away.</div>
      </div>
    </div>
  );
}
