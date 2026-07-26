// citrate-quorum — Node surface. Instrument register.
//
// LIVE as of Phase 0. Every number on this page is a live answer from chain
// 40204, read over the endpoint the canonical address book names, or a local
// read of this tenant's evidence chain. Nothing here is styled to look like
// telemetry we do not have.
//
// What the ported design prototype showed here and this does NOT, because the
// data does not exist to show:
//   · "uptime 99.2% · 30d" and "Validating — live": this app validates nothing
//     and keeps no uptime series. It is an RPC client.
//   · a per-block "blue / anticone" flag and a checkpoint marker: 40204's RPC
//     returns `blueScore`, `selectedParentHash` and `mergeParentHashes`, and
//     nothing that says "checkpoint". The real fields are rendered instead.
//   · a peer LIST: `net_peerCount` returns a count and the public RPC exposes
//     no peer enumeration, so the count is shown as a count.
//   · a live NODE log: this app supervises no node. The log panel shows this
//     app's own RPC activity, which is a real thing and is labelled as one.
//   · "mentor-mentee learning · gov-lora v2.3": no model runtime is wired.
//
// Reads bridge.node.status()/blocks()/activity() and bridge.ledger.state().
import { useCallback, useEffect, useState } from "react";
import { bridge } from "../bridge";
import { Domain, DomainErrorPlate, useDomain } from "../components/DomainState";
import type { ActivityLine, Block, LedgerState } from "../bridge";

const LVL_COLOR: Record<string, string> = { INFO: "var(--ok)", WARN: "var(--warn)", DEBUG: "var(--tx-3)", ERROR: "var(--danger)" };

/** How many blocks the explorer asks for. The backend clamps at 25. */
const BLOCK_COUNT = 12;
/** How often the posture tiles, blocks and activity refresh (ms). */
const REFRESH_MS = 6000;

const num = (n: number) => n.toLocaleString("en-US");

/** Seconds since a block's timestamp, rendered for a human. */
function age(timestamp: number, now: number): string {
  const s = Math.max(0, now - timestamp);
  if (s < 2) return "now";
  if (s < 90) return `${s}s ago`;
  if (s < 5400) return `${Math.round(s / 60)}m ago`;
  return `${Math.round(s / 3600)}h ago`;
}

function Tile({ label, value, note, accent }: { label: string; value: string; note: string; accent?: string }) {
  return (
    <div className="surface" style={{ padding: 16, ...(accent ? { borderTop: `2px solid ${accent}` } : {}) }}>
      <div className="eyebrow">{label}</div>
      <div className="tabular" style={{ fontFamily: "var(--font-display)", fontSize: 24, fontWeight: 460, color: accent ?? "inherit" }}>{value}</div>
      <div className="mono" style={{ fontSize: 9.5, color: "var(--tx-3)" }}>{note}</div>
    </div>
  );
}

export function Node() {
  const [blocks, setBlocks] = useState<Block[]>([]);
  const [blocksErr, setBlocksErr] = useState<string | null>(null);
  const [activity, setActivity] = useState<ActivityLine[]>([]);
  const [chain, setChain] = useState<LedgerState | null>(null);
  const [chainErr, setChainErr] = useState<string | null>(null);
  const [sel, setSel] = useState<Block | null>(null);
  const [lvl, setLvl] = useState<string>("all");
  const [nowSec, setNowSec] = useState(() => Math.floor(Date.now() / 1000));

  // The primary read. Everything else on the page decorates it, and each of
  // those failing independently must not take the page down.
  const primary = useDomain(() => bridge.node.status(), "node.status()");

  // Every write here happens in a promise callback, never synchronously: the
  // first call runs inside an effect, and a synchronous setState there
  // cascades renders (react-hooks/set-state-in-effect). The clock the block
  // ages are measured against is therefore stamped when the blocks arrive,
  // which is also the more correct instant to measure them from.
  const refresh = useCallback(() => {
    bridge.node
      .blocks(BLOCK_COUNT)
      .then((b) => { setNowSec(Math.floor(Date.now() / 1000)); setBlocks(b); setBlocksErr(null); })
      .catch((e: unknown) => setBlocksErr(e instanceof Error ? e.message : String(e)));
    bridge.node.activity().then(setActivity).catch(() => {
      /* the activity log is diagnostics; its own failure must not blank the page */
    });
    bridge.ledger
      .state()
      .then((s) => { setChain(s); setChainErr(null); })
      .catch((e: unknown) => setChainErr(e instanceof Error ? e.message : String(e)));
  }, []);

  useEffect(() => {
    refresh();
    const iv = setInterval(refresh, REFRESH_MS);
    return () => clearInterval(iv);
  }, [refresh]);

  // Placed after every hook: an early return above one makes it run
  // conditionally, and React crashes the moment that path is taken.
  if (primary.state.status === "error") {
    return (
      <div style={{ padding: 18 }}>
        <DomainErrorPlate
          source="node.status()"
          error={primary.state.error}
          onRetry={primary.retry}
          lands="It reads chain 40204 over the endpoint in the address book."
        />
      </div>
    );
  }

  const shown = activity.filter((l) => lvl === "all" || l.lvl === lvl);

  return (
    <div style={{ padding: 18, display: "flex", flexDirection: "column", gap: 14, maxWidth: 1100 }}>
      <Domain read={primary} source="node.status()" skeletonRows={2}>
        {(s) => (
          <>
            <div style={{ display: "grid", gridTemplateColumns: "repeat(4,1fr)", gap: 10 }}>
              <Tile label="Height" value={num(s.height)} note={s.blueScore != null ? `blue score ${num(s.blueScore)}` : "blueScore not reported"} />
              <Tile
                label="Peers"
                value={s.peers == null ? "—" : num(s.peers)}
                note={s.peers == null ? "net_peerCount not answered — not zero peers" : "net_peerCount, reported by the endpoint"}
              />
              <Tile
                label="Sync"
                value={s.syncing == null ? "—" : s.syncing ? "syncing" : "in sync"}
                note={s.client ?? "web3_clientVersion not answered"}
                accent={s.syncing ? "var(--warn)" : undefined}
              />
              <Tile label="Round trip" value={`${num(s.latencyMs)} ms`} note={`chain ${s.chainId} · eth_blockNumber`} />
            </div>
            <div className="mono" style={{ fontSize: 9.5, color: "var(--tx-3)", lineHeight: 1.7 }}>
              endpoint {s.rpcUrl} · {s.book}
              {s.baseFeeWei && <> · head base fee {s.baseFeeWei} wei</>}
              <br />
              This app runs no node. It reads the chain over JSON-RPC, and every tile above is one call.
            </div>
          </>
        )}
      </Domain>

      <div style={{ display: "grid", gridTemplateColumns: "minmax(0,1fr) 300px", gap: 14, alignItems: "start" }}>
        <div style={{ display: "flex", flexDirection: "column", gap: 14, minWidth: 0 }}>
          <div className="surface" style={{ display: "flex", flexDirection: "column" }}>
            <div style={{ padding: "10px 14px", borderBottom: "1px solid var(--line-1)", display: "flex", alignItems: "center", gap: 10 }}>
              <span className="eyebrow">Explorer — recent blocks</span>
              <div style={{ flex: 1 }} />
              <span className="mono" style={{ fontSize: 9, color: "var(--tx-3)" }}>eth_getBlockByNumber × {BLOCK_COUNT}</span>
            </div>
            <div className="mono" style={{ display: "grid", gridTemplateColumns: "76px minmax(70px,1fr) 34px minmax(80px,110px) 74px 56px", gap: 8, padding: "6px 14px", fontSize: 9, letterSpacing: ".12em", textTransform: "uppercase", color: "var(--tx-3)", borderBottom: "1px solid var(--line-1)" }}>
              <span>Height</span><span>Hash</span><span>Txs</span><span>Proposer</span><span>Gas used</span><span>Age</span>
            </div>
            {blocksErr && (
              <div className="mono" style={{ fontSize: 10.5, color: "var(--danger)", padding: "10px 14px", lineHeight: 1.6 }}>
                node.blocks() did not return: {blocksErr}
              </div>
            )}
            {!blocksErr && blocks.length === 0 && (
              <div className="mono" style={{ fontSize: 10.5, color: "var(--tx-3)", padding: "10px 14px" }}>reading…</div>
            )}
            {blocks.map((b) => (
              <div key={b.height} onClick={() => setSel(b)} className="mono tabular" style={{ display: "grid", gridTemplateColumns: "76px minmax(70px,1fr) 34px minmax(80px,110px) 74px 56px", gap: 8, padding: "6px 14px", fontSize: 10.5, borderBottom: "1px solid var(--line-1)", cursor: "pointer" }}>
                <span style={{ whiteSpace: "nowrap" }}>{b.height}{b.mergeParents > 0 && <span style={{ color: "var(--accent-text)" }}> ⑂{b.mergeParents}</span>}</span>
                <span style={{ color: "var(--tx-2)", overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" }}>{b.hash}</span>
                <span>{b.txs}</span>
                <span style={{ color: "var(--tx-2)", overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" }}>{b.proposer}</span>
                <span style={{ color: "var(--tx-3)" }}>{num(b.gasUsed)}</span>
                <span style={{ color: "var(--tx-3)", whiteSpace: "nowrap" }}>{age(b.timestamp, nowSec)}</span>
              </div>
            ))}
            <div className="mono" style={{ fontSize: 9, color: "var(--tx-3)", padding: "7px 14px", lineHeight: 1.6 }}>
              ⑂n = the block merged n anticone parents (GhostDAG mergeParentHashes) · node.blocks()
            </div>
          </div>

          {sel && (
            <div className="surface" style={{ display: "flex", flexDirection: "column", borderTop: "2px solid var(--line-strong)" }}>
              <div style={{ padding: "10px 14px", borderBottom: "1px solid var(--line-1)", display: "flex", alignItems: "center", gap: 10 }}>
                <span className="eyebrow">Block {sel.height}</span><div style={{ flex: 1 }} /><span onClick={() => setSel(null)} className="mono" style={{ fontSize: 10, color: "var(--tx-3)", cursor: "pointer" }}>close ×</span>
              </div>
              {([
                ["Hash", sel.hash],
                ["Proposer", sel.proposer],
                ["Txs", String(sel.txs)],
                ["Gas used", `${num(sel.gasUsed)} of ${num(sel.gasLimit)}`],
                ["Blue score", sel.blueScore == null ? "— not reported by this node" : num(sel.blueScore)],
                ["Merge parents", sel.mergeParents === 0 ? "0 — extended the selected chain only" : String(sel.mergeParents)],
                ["Timestamp", `${sel.timestamp} · ${age(sel.timestamp, nowSec)}`],
              ] as [string, string][]).map(([k, v]) => (
                <div key={k} style={{ display: "grid", gridTemplateColumns: "130px 1fr", borderBottom: "1px solid var(--line-1)" }}>
                  <span className="mono" style={{ fontSize: 9, letterSpacing: ".1em", textTransform: "uppercase", color: "var(--tx-3)", padding: "7px 14px", background: "var(--srf-inset)" }}>{k}</span>
                  <span className="mono" style={{ fontSize: 10.5, padding: "7px 14px", wordBreak: "break-all" }}>{v}</span>
                </div>
              ))}
            </div>
          )}

          <div className="surface" style={{ display: "flex", flexDirection: "column" }}>
            <div style={{ padding: "10px 14px", borderBottom: "1px solid var(--line-1)", display: "flex", alignItems: "center", gap: 10 }}>
              <span className="eyebrow">RPC activity — what this app asked</span>
              <div style={{ flex: 1 }} />
              {["all", "INFO", "ERROR"].map((l) => (
                <span key={l} onClick={() => setLvl(l)} className="mono" style={{ fontSize: 9, letterSpacing: ".1em", textTransform: "uppercase", color: lvl === l ? "var(--tx-1)" : "var(--tx-3)", border: `1px solid ${lvl === l ? "var(--tx-2)" : "var(--line-2)"}`, padding: "1px 7px", cursor: "pointer" }}>{l}</span>
              ))}
            </div>
            <div style={{ maxHeight: 260, overflow: "auto", background: "var(--srf-inset)", padding: "6px 0" }}>
              {shown.length === 0 && (
                <div className="mono" style={{ fontSize: 10, color: "var(--tx-3)", padding: "6px 14px" }}>
                  no calls recorded yet
                </div>
              )}
              {shown.map((lg, i) => (
                <div key={`${lg.t}-${i}`} className="mono" style={{ display: "grid", gridTemplateColumns: "64px 48px 150px minmax(0,1fr)", gap: 8, padding: "2px 14px", fontSize: 10, lineHeight: 1.7 }}>
                  <span className="tabular" style={{ color: "var(--tx-3)" }}>{lg.t}</span>
                  <span style={{ color: LVL_COLOR[lg.lvl] ?? "var(--tx-2)" }}>{lg.lvl}</span>
                  <span style={{ color: "var(--info)", overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" }}>{lg.module}</span>
                  <span style={{ color: "var(--tx-2)", whiteSpace: "nowrap", overflow: "hidden", textOverflow: "ellipsis" }}>{lg.msg}</span>
                </div>
              ))}
            </div>
            <div className="mono" style={{ fontSize: 9, color: "var(--tx-3)", padding: "7px 14px", lineHeight: 1.6 }}>
              node.activity() · this is NOT a node log — citrate-quorum supervises no node. It is this app's own
              record of every JSON-RPC call it made, kept in memory (most recent 400).
            </div>
          </div>
        </div>

        <div style={{ display: "flex", flexDirection: "column", gap: 14 }}>
          <div className="surface" style={{ padding: 14, display: "flex", flexDirection: "column", gap: 6 }}>
            <span className="eyebrow">Local evidence chain</span>
            {chainErr && <span className="mono" style={{ fontSize: 10.5, color: "var(--warn)", lineHeight: 1.6 }}>ledger.state() — {chainErr}</span>}
            {!chainErr && chain && (
              <span className="mono" style={{ fontSize: 10.5, color: "var(--tx-2)", lineHeight: 1.8, wordBreak: "break-all" }}>
                {num(chain.records)} record{chain.records === 1 ? "" : "s"} · tenant {chain.tenant}
                <br />head {chain.head}
                <br />merkle root {chain.merkleRoot}
                <br />
                <span style={{ color: chain.intact ? "var(--ok)" : "var(--danger)" }}>
                  {chain.intact ? "replayed from genesis — intact" : "REPLAY FAILED — this chain has been altered"}
                </span>
                {chain.ungoverned > 0 && (
                  <>
                    <br /><span style={{ color: "var(--danger)" }}>{chain.ungoverned} ungoverned</span>
                  </>
                )}
                <br />advances even offline — it needs no chain
              </span>
            )}
          </div>
          <div className="surface" style={{ padding: 14, display: "flex", flexDirection: "column", gap: 6 }}>
            <span className="eyebrow">Anchoring</span>
            <span style={{ fontSize: 12.5, color: "var(--tx-2)", lineHeight: 1.55 }}>
              Ratified minutes are registered in <span className="mono">MeetingRegistry</span> on chain — see a meeting's
              anchor row. Periodic anchoring of the whole evidence chain's Merkle root to{" "}
              <span className="mono">AnchorRegistry</span> is not wired yet.
            </span>
          </div>
        </div>
      </div>
    </div>
  );
}
