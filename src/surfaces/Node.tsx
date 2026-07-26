// citrate-quorum — Node surface (QRM-S2D). Instrument register. Ported from
// design §NODE. Live posture tiles, the block explorer (GhostDAG blue/non-blue
// rows, checkpoint markers, block detail), the live log stream (level filter),
// peers, local hash chain, mentor-mentee learning. Reads bridge.node.blocks()/
// logs()/peers() and bridge.session for chain height.
import { useEffect, useState } from "react";
import { bridge } from "../bridge";
import { DomainErrorPlate, useDomain } from "../components/DomainState";
import type { Block, LogLine, Session } from "../bridge";

const LVL_COLOR: Record<string, string> = { INFO: "var(--ok)", WARN: "var(--warn)", DEBUG: "var(--tx-3)", ERROR: "var(--danger)" };

export function Node() {
  const [session, setSession] = useState<Session | null>(null);
  const [blocks, setBlocks] = useState<Block[]>([]);
  const [logs, setLogs] = useState<LogLine[]>([]);
  const [sel, setSel] = useState<Block | null>(null);
  const [lvl, setLvl] = useState<string>("all");

  useEffect(() => {
    bridge.session.current().then((s) => { setSession(s); setBlocks(bridge.node.blocks(s.chain.height)); }).catch(() => {});
    
    // Streams throw synchronously when unwired (they return an Unsubscribe,
    // so there is no promise to reject). An unguarded subscribe in an effect
    // takes the whole tree down; the surface's error plate covers the reason.
    let unsub: (() => void) | undefined;
    try {
      unsub = bridge.node.logs((l) => setLogs((p) => [l, ...p].slice(0, 40)));
    } catch {
      /* no log stream — peers/blocks still render */
    }
    return unsub;
  }, []);

  // Honest failure (S2D.4/§5.1): this surface's primary read is node.peers().
  // A read that cannot succeed must say so and offer a retry, not sit in a
  // loading state forever.
  const primary = useDomain(() => bridge.node.peers(), "node.peers()");
  // Derived, not mirrored (see Agents.tsx).
  const peers = primary.state.status === "ready" ? primary.state.data : [];
  if (primary.state.status === "error") {
    return (
      <div style={{ padding: 18 }}>
        <DomainErrorPlate source="node.peers()" error={primary.state.error} onRetry={primary.retry} lands="It needs a live node connection." />
      </div>
    );
  }

  const chainH = session ? session.chain.height.toLocaleString("en-US") : "…";
  const shownLogs = logs.filter((l) => lvl === "all" || l.lvl === lvl);

  return (
    <div style={{ padding: 18, display: "flex", flexDirection: "column", gap: 14, maxWidth: 1100 }}>
      <div style={{ display: "grid", gridTemplateColumns: "repeat(4,1fr)", gap: 10 }}>
        <div className="surface" style={{ padding: 16 }}><div className="eyebrow">Height</div><div className="tabular" style={{ fontFamily: "var(--font-display)", fontSize: 26, fontWeight: 460 }}>{chainH}</div><div className="mono" style={{ fontSize: 9.5, color: "var(--tx-3)" }}>GhostDAG k=18</div></div>
        <div className="surface" style={{ padding: 16 }}><div className="eyebrow">Checkpoint</div><div className="tabular" style={{ fontFamily: "var(--font-display)", fontSize: 26, fontWeight: 460 }}>~25s</div><div className="mono" style={{ fontSize: 9.5, color: "var(--tx-3)" }}>50 blocks · BFT 67%</div></div>
        <div className="surface" style={{ padding: 16, borderTop: "2px solid var(--accent)" }}><div className="eyebrow">Validating</div><div className="tabular" style={{ fontFamily: "var(--font-display)", fontSize: 26, fontWeight: 460, color: "var(--accent)" }}>live</div><div className="mono" style={{ fontSize: 9.5, color: "var(--tx-3)" }}>uptime 99.2% · 30d</div></div>
        <div className="surface" style={{ padding: 16 }}><div className="eyebrow">Relay</div><div className="tabular" style={{ fontFamily: "var(--font-display)", fontSize: 22, fontWeight: 460 }}>{session?.chain.relay ?? "—"}</div><div className="mono" style={{ fontSize: 9.5, color: "var(--tx-3)" }}>rooms E2E · relay sees ciphertext</div></div>
      </div>
      <div style={{ display: "grid", gridTemplateColumns: "minmax(0,1fr) 280px", gap: 14, alignItems: "start" }}>
        <div style={{ display: "flex", flexDirection: "column", gap: 14, minWidth: 0 }}>
          <div className="surface" style={{ display: "flex", flexDirection: "column" }}>
            <div style={{ padding: "10px 14px", borderBottom: "1px solid var(--line-1)", display: "flex", alignItems: "center", gap: 10 }}>
              <span className="eyebrow">Explorer — recent blocks</span>
              <span style={{ width: 7, height: 7, borderRadius: 999, background: "var(--accent)", animation: "ccPulse 1.6s infinite" }} />
            </div>
            <div className="mono" style={{ display: "grid", gridTemplateColumns: "76px minmax(70px,1fr) 34px minmax(80px,110px) 64px 56px", gap: 8, padding: "6px 14px", fontSize: 9, letterSpacing: ".12em", textTransform: "uppercase", color: "var(--tx-3)", borderBottom: "1px solid var(--line-1)" }}>
              <span>Height</span><span>Hash</span><span>Txs</span><span>Proposer</span><span>Gas</span><span>Age</span>
            </div>
            {blocks.map((b) => (
              <div key={b.height} onClick={() => setSel(b)} className="mono tabular" style={{ display: "grid", gridTemplateColumns: "76px minmax(70px,1fr) 34px minmax(80px,110px) 64px 56px", gap: 8, padding: "6px 14px", fontSize: 10.5, borderBottom: "1px solid var(--line-1)", cursor: "pointer", background: b.blue ? "transparent" : "var(--danger-bg)" }}>
                <span style={{ whiteSpace: "nowrap" }}>{b.height}{b.checkpoint && <span style={{ color: "var(--accent-text)" }}> ●</span>}</span>
                <span style={{ color: "var(--tx-2)", overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" }}>{b.hash}</span>
                <span>{b.txs}</span>
                <span style={{ color: "var(--tx-2)", overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" }}>{b.proposer}</span>
                <span style={{ color: "var(--tx-3)" }}>{b.gas}</span>
                <span style={{ color: "var(--tx-3)", whiteSpace: "nowrap" }}>{b.age}</span>
              </div>
            ))}
            <div className="mono" style={{ fontSize: 9, color: "var(--tx-3)", padding: "7px 14px" }}>● checkpoint block · red rows are non-blue (GhostDAG anticone) — recorded, not discarded · node.blocks()</div>
          </div>
          {sel && (
            <div className="surface" style={{ display: "flex", flexDirection: "column", borderTop: "2px solid var(--line-strong)" }}>
              <div style={{ padding: "10px 14px", borderBottom: "1px solid var(--line-1)", display: "flex", alignItems: "center", gap: 10 }}>
                <span className="eyebrow">Block {sel.height}</span><div style={{ flex: 1 }} /><span onClick={() => setSel(null)} className="mono" style={{ fontSize: 10, color: "var(--tx-3)", cursor: "pointer" }}>close ×</span>
              </div>
              {([["Hash", sel.hash], ["Proposer", sel.proposer], ["Txs", String(sel.txs)], ["Gas used", sel.gas], ["Blue (GhostDAG)", sel.blue ? "yes" : "no — anticone, recorded"], ["Checkpoint", sel.checkpoint ? "yes · BFT-final" : "no"]] as [string, string][]).map(([k, v]) => (
                <div key={k} style={{ display: "grid", gridTemplateColumns: "130px 1fr", borderBottom: "1px solid var(--line-1)" }}>
                  <span className="mono" style={{ fontSize: 9, letterSpacing: ".1em", textTransform: "uppercase", color: "var(--tx-3)", padding: "7px 14px", background: "var(--srf-inset)" }}>{k}</span>
                  <span className="mono" style={{ fontSize: 10.5, padding: "7px 14px", wordBreak: "break-all" }}>{v}</span>
                </div>
              ))}
            </div>
          )}
          <div className="surface" style={{ display: "flex", flexDirection: "column" }}>
            <div style={{ padding: "10px 14px", borderBottom: "1px solid var(--line-1)", display: "flex", alignItems: "center", gap: 10 }}>
              <span className="eyebrow">Logs — live</span>
              <span style={{ width: 7, height: 7, borderRadius: 999, background: "var(--accent)", animation: "ccPulse 1.6s infinite" }} />
              <div style={{ flex: 1 }} />
              {["all", "INFO", "WARN"].map((l) => (
                <span key={l} onClick={() => setLvl(l)} className="mono" style={{ fontSize: 9, letterSpacing: ".1em", textTransform: "uppercase", color: lvl === l ? "var(--tx-1)" : "var(--tx-3)", border: `1px solid ${lvl === l ? "var(--tx-2)" : "var(--line-2)"}`, padding: "1px 7px", cursor: "pointer" }}>{l}</span>
              ))}
            </div>
            <div style={{ maxHeight: 260, overflow: "auto", background: "var(--srf-inset)", padding: "6px 0" }}>
              {shownLogs.map((lg, i) => (
                <div key={i} className="mono" style={{ display: "grid", gridTemplateColumns: "64px 44px 80px minmax(0,1fr)", gap: 8, padding: "2px 14px", fontSize: 10, lineHeight: 1.7 }}>
                  <span className="tabular" style={{ color: "var(--tx-3)" }}>{lg.t}</span>
                  <span style={{ color: LVL_COLOR[lg.lvl] }}>{lg.lvl}</span>
                  <span style={{ color: "var(--info)" }}>{lg.mod}</span>
                  <span style={{ color: "var(--tx-2)", whiteSpace: "nowrap", overflow: "hidden", textOverflow: "ellipsis" }}>{lg.msg}</span>
                </div>
              ))}
            </div>
            <div className="mono" style={{ fontSize: 9, color: "var(--tx-3)", padding: "7px 14px" }}>node.logs() stream · tail -f semantics · production: ring buffer 10k lines, full logs on disk</div>
          </div>
        </div>
        <div style={{ display: "flex", flexDirection: "column", gap: 14 }}>
          <div className="surface" style={{ display: "flex", flexDirection: "column" }}>
            <div style={{ padding: "10px 14px", borderBottom: "1px solid var(--line-1)" }}><span className="eyebrow">Peers</span></div>
            {peers.map((p) => (
              <div key={p.id} style={{ display: "flex", alignItems: "center", gap: 8, padding: "8px 14px", borderBottom: "1px solid var(--line-1)" }}>
                <span style={{ width: 7, height: 7, borderRadius: 999, background: p.ok ? "var(--ok)" : "var(--danger)", flexShrink: 0 }} />
                <span className="mono" style={{ fontSize: 10.5, flex: 1, minWidth: 0, overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" }}>{p.id}</span>
                <span className="mono" style={{ fontSize: 8.5, letterSpacing: ".08em", textTransform: "uppercase", color: "var(--tx-3)" }}>{p.kind}</span>
                <span className="mono tabular" style={{ fontSize: 9.5, color: p.ok ? "var(--tx-3)" : "var(--danger)", width: 38, textAlign: "right" }}>{p.latency}</span>
              </div>
            ))}
          </div>
          <div className="surface" style={{ padding: 14, display: "flex", flexDirection: "column", gap: 6 }}>
            <span className="eyebrow">Local hash chain</span>
            <span className="mono" style={{ fontSize: 10.5, color: "var(--tx-2)", lineHeight: 1.7 }}>#48,214 · head b3:22e0…91cf<br />anchored → root {session?.chain.anchorRoot} @ 1,284,051<br />advances even offline</span>
          </div>
          <div className="surface" style={{ padding: 14, display: "flex", flexDirection: "column", gap: 6 }}>
            <span className="eyebrow">Learning — mentor-mentee</span>
            <span className="mono" style={{ fontSize: 10.5, color: "var(--tx-2)", lineHeight: 1.7 }}>gov-lora v2.3 · rank 16 · dim 768<br />last delta: checkpoint 25,681<br />next exchange in ~18s</span>
          </div>
        </div>
      </div>
    </div>
  );
}
