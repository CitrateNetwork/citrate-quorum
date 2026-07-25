// citrate-quorum — Wallet surface (QRM-S2D). Instrument register. Ported from
// design §WALLET. Balances, account (Send/Receive/Stake), Send (ceremony-gated;
// >150 SALT forces the HIC-1 warning per PRT-004 C2), Receive (address + QR),
// Staking (bond/unbond ceremonies), tokens, attributed activity linking to
// decision ids, agent spend. Reads bridge.wallet.summary(); signs via ceremony.
import { useEffect, useMemo, useState } from "react";
import { bridge } from "../bridge";
import { DomainErrorPlate, useDomain } from "../components/DomainState";
import type { Wallet as WalletData } from "../bridge";
import { useCeremony } from "../ceremony/Ceremony";

const DIR_COLOR: Record<string, string> = { in: "var(--ok)", out: "var(--tx-2)", stake: "var(--info)", gas: "var(--tx-3)" };
const ST_COLOR: Record<string, string> = { settled: "var(--tx-3)", rejected: "var(--danger)", pending: "var(--warn)" };

export function Wallet() {
  const [w, setW] = useState<WalletData | null>(null);
  const [panel, setPanel] = useState<"" | "send" | "recv" | "stake">("");
  const [to, setTo] = useState(""); const [amt, setAmt] = useState("");
  const ceremony = useCeremony();

  const over = useMemo(() => parseFloat(amt || "0") > 150, [amt]);

  // Honest failure (S2D.4/§5.1): this surface's primary read is wallet.summary().
  // A read that cannot succeed must say so and offer a retry, not sit in a
  // loading state forever.
  const primary = useDomain(() => bridge.wallet.summary(), "wallet.summary()");
  useEffect(() => {
    if (primary.state.status === "ready") setW(primary.state.data);
  }, [primary.state]);
  if (primary.state.status === "error") {
    return (
      <div style={{ padding: 18 }}>
        <DomainErrorPlate source="wallet.summary()" error={primary.state.error} onRetry={primary.retry} lands="It needs a live chain." />
      </div>
    );
  }
  const send = async () => {
    const r = await ceremony.request({
      kind: "transfer", title: "Send SALT", origin: "user",
      // PRT-004 C2: over 150 SALT escalates to HIC-1. The gate decides that
      // from the cost + threshold — the UI does not pre-judge it.
      action: { actionClass: "spend", classification: "Public", agent: "user", cost: parseFloat(amt || "0"), hic1CostThreshold: 150 },
      rows: [{ k: "To", v: to || "0x…" }, { k: "Amount", v: `${amt || "0"} SALT` }, ...(over ? [{ k: "Policy", v: "PRT-004 C2 · over 150 SALT → HIC-1" }] : [])],
      cost: `${amt || "0"} SALT`,
    });
    if (r.outcome === "settled") setPanel("");
  };
  const stakeOp = async (kind: string) => {
    await ceremony.request({ kind: "stake", title: kind, origin: "user",
      action: { actionClass: "stake", classification: "Public", agent: "user", mandatoryHic1: true },
      rows: [{ k: "Vault", v: "MembershipStakeVault" }, { k: "Effect", v: kind }] });
  };

  if (!w) return <div style={{ padding: 18 }} className="mono">wallet.summary()…</div>;

  return (
    <div style={{ padding: 18, display: "flex", flexDirection: "column", gap: 14, maxWidth: 1060 }}>
      <div style={{ display: "grid", gridTemplateColumns: "repeat(4,1fr)", gap: 10 }}>
        <div className="surface" style={{ padding: 16 }}><div className="eyebrow">Balance</div><div className="tabular" style={{ fontFamily: "var(--font-display)", fontSize: 28, fontWeight: 460 }}>{w.tokens[0].balance} <span style={{ fontSize: 13, color: "var(--tx-3)" }}>SALT</span></div><div className="mono" style={{ fontSize: 9.5, color: "var(--tx-3)" }}>{w.keyStore}</div></div>
        <div className="surface" style={{ padding: 16 }}><div className="eyebrow">Staked</div><div className="tabular" style={{ fontFamily: "var(--font-display)", fontSize: 28, fontWeight: 460 }}>{w.staking.bonded}</div><div className="mono" style={{ fontSize: 9.5, color: "var(--tx-3)" }}>validator bond · {w.staking.apr} · unbonding {w.staking.period}</div></div>
        <div className="surface" style={{ padding: 16, borderTop: "2px solid var(--warn)" }}><div className="eyebrow">Paymaster budget</div><div className="tabular" style={{ fontFamily: "var(--font-display)", fontSize: 28, fontWeight: 460 }}>64%</div><div className="mono" style={{ fontSize: 9.5, color: "var(--tx-3)" }}>agent gas sponsored · resets 08-01</div></div>
        <div className="surface" style={{ padding: 16 }}><div className="eyebrow">Tier-2 revenue · 30d</div><div className="tabular" style={{ fontFamily: "var(--font-display)", fontSize: 28, fontWeight: 460, color: "var(--accent-text)" }}>{w.staking.rewards30d}</div><div className="mono" style={{ fontSize: 9.5, color: "var(--tx-3)" }}>verifiable inference + rewards</div></div>
      </div>

      <div className="surface" style={{ display: "flex", alignItems: "center", gap: 12, padding: "11px 16px" }}>
        <span className="eyebrow">Account</span>
        <span className="mono" style={{ fontSize: 11, color: "var(--tx-2)", wordBreak: "break-all" }}>{w.address}</span>
        <div style={{ flex: 1 }} />
        <button className="btn btn-primary btn-sm" onClick={() => setPanel(panel === "send" ? "" : "send")}>Send</button>
        <button className="btn btn-ghost btn-sm" onClick={() => setPanel(panel === "recv" ? "" : "recv")}>Receive</button>
        <button className="btn btn-ghost btn-sm" onClick={() => setPanel(panel === "stake" ? "" : "stake")}>Stake</button>
      </div>

      {panel === "send" && (
        <div className="surface" style={{ padding: 16, display: "flex", flexDirection: "column", gap: 12, borderTop: "2px solid var(--line-strong)", maxWidth: 560 }}>
          <span className="eyebrow">Send — every transfer is a ceremony</span>
          <div><div className="lbl">To</div>
            <select className="input" value={to} onChange={(e) => setTo(e.target.value)} style={{ width: "100%" }}>
              <option value="">Choose a contact or paste an address…</option>
              {w.contacts.map((c) => <option key={c.addr} value={c.addr}>{c.name} · {c.addr}</option>)}
            </select>
          </div>
          <div style={{ display: "grid", gridTemplateColumns: "1fr 130px", gap: 8 }}>
            <div><div className="lbl">Amount</div><input className="input" value={amt} onChange={(e) => setAmt(e.target.value)} placeholder="0.00" style={{ width: "100%" }} /></div>
            <div><div className="lbl">Token</div><select className="input" style={{ width: "100%" }}><option>SALT</option><option>L4-CRED</option></select></div>
          </div>
          {over && <div className="mono" style={{ fontSize: 10, color: "var(--warn)", border: "1px solid var(--warn)", background: "var(--warn-bg)", padding: "7px 10px" }}>Over 150 SALT — PRT-004 C2 forces HIC-1: this send will require your signature in a full ceremony.</div>}
          <div style={{ display: "flex", gap: 8 }}><button className="btn btn-primary btn-sm" onClick={send} disabled={!to || !amt}>Review &amp; sign</button><button className="btn btn-ghost btn-sm" onClick={() => setPanel("")}>Cancel</button></div>
        </div>
      )}
      {panel === "recv" && (
        <div className="surface" style={{ padding: 16, display: "flex", gap: 16, alignItems: "center", borderTop: "2px solid var(--line-strong)", maxWidth: 560 }}>
          <div style={{ width: 96, height: 96, flexShrink: 0, display: "grid", gridTemplateColumns: "repeat(8,1fr)", gap: 2, padding: 8, border: "1px solid var(--line-2)" }}>
            {Array.from({ length: 64 }, (_, i) => <span key={i} style={{ background: (i * 2654435761 % 3) ? "var(--ink)" : "transparent" }} />)}
          </div>
          <div style={{ display: "flex", flexDirection: "column", gap: 6, minWidth: 0 }}>
            <span className="eyebrow">Receive</span>
            <span className="mono" style={{ fontSize: 11, wordBreak: "break-all" }}>{w.address}</span>
            <span className="mono" style={{ fontSize: 9.5, color: "var(--tx-3)" }}>chain 40204 · citrate testnet · SALT + tenant tokens</span>
            <button className="btn btn-ghost btn-sm" onClick={() => setPanel("")} style={{ width: "fit-content" }}>Close</button>
          </div>
        </div>
      )}
      {panel === "stake" && (
        <div className="surface" style={{ padding: 16, display: "flex", flexDirection: "column", gap: 10, borderTop: "2px solid var(--line-strong)", maxWidth: 560 }}>
          <span className="eyebrow">Staking — validator bond</span>
          <div style={{ display: "grid", gridTemplateColumns: "150px 1fr", gap: "6px 12px", fontSize: 12.5 }}>
            <span className="lbl" style={{ margin: 0 }}>Bonded</span><span className="mono tabular" style={{ fontSize: 12 }}>{w.staking.bonded} SALT · MembershipStakeVault</span>
            <span className="lbl" style={{ margin: 0 }}>Rewards · 30d</span><span className="mono tabular" style={{ fontSize: 12, color: "var(--accent-text)" }}>{w.staking.rewards30d} SALT · {w.staking.apr} APR</span>
            <span className="lbl" style={{ margin: 0 }}>Unbonding</span><span className="mono tabular" style={{ fontSize: 12 }}>{w.staking.unbonding} · period {w.staking.period}</span>
            <span className="lbl" style={{ margin: 0 }}>Slashing</span><span style={{ fontSize: 12, color: "var(--tx-2)" }}>{w.staking.slashable}</span>
          </div>
          <div style={{ display: "flex", gap: 8 }}><button className="btn btn-primary btn-sm" onClick={() => stakeOp("Bond more")}>Bond more — ceremony</button><button className="btn btn-ghost btn-sm" onClick={() => stakeOp("Begin unbonding")}>Begin unbonding — ceremony</button><button className="btn btn-ghost btn-sm" onClick={() => setPanel("")}>Close</button></div>
        </div>
      )}

      <div style={{ display: "grid", gridTemplateColumns: "300px 1fr", gap: 14, alignItems: "start" }}>
        <div className="surface" style={{ display: "flex", flexDirection: "column" }}>
          <div style={{ padding: "10px 14px", borderBottom: "1px solid var(--line-1)" }}><span className="eyebrow">Tokens</span></div>
          {w.tokens.map((tk) => (
            <div key={tk.sym} style={{ display: "flex", alignItems: "baseline", gap: 10, padding: "10px 14px", borderBottom: "1px solid var(--line-1)" }}>
              <span className="mono" style={{ fontSize: 11, fontWeight: 500, width: 64 }}>{tk.sym}</span>
              <span style={{ fontSize: 11, color: "var(--tx-3)", flex: 1, minWidth: 0, overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" }}>{tk.name}</span>
              <span className="mono tabular" style={{ fontSize: 12 }}>{tk.balance}</span>
            </div>
          ))}
          <div className="mono" style={{ fontSize: 9, color: "var(--tx-3)", padding: "8px 14px", lineHeight: 1.5 }}>wallet.summary() · tenant tokens are chain-native, custody in the same keyring</div>
        </div>
        <div className="surface" style={{ display: "flex", flexDirection: "column" }}>
          <div style={{ padding: "10px 14px", borderBottom: "1px solid var(--line-1)" }}><span className="eyebrow">Activity — every movement, attributed</span></div>
          {w.txs.map((tx) => (
            <div key={tx.hash} style={{ display: "grid", gridTemplateColumns: "52px 160px 1fr 110px 70px", gap: 10, padding: "8px 14px", borderBottom: "1px solid var(--line-1)", alignItems: "baseline" }}>
              <span className="mono" style={{ fontSize: 8.5, letterSpacing: ".08em", textTransform: "uppercase", color: DIR_COLOR[tx.dir] }}>{tx.dir}</span>
              <span style={{ fontSize: 12 }}>{tx.kind}</span>
              <span className="mono" style={{ fontSize: 10, color: "var(--tx-3)", minWidth: 0, overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" }}>{tx.counterparty}{tx.decision && <a href="#/ledger" style={{ color: "var(--info)" }}> · {tx.decision}</a>}</span>
              <span className="mono tabular" style={{ fontSize: 11.5, textAlign: "right", color: tx.amount.startsWith("+") ? "var(--ok)" : "var(--tx-1)" }}>{tx.amount} <span style={{ color: "var(--tx-3)", fontSize: 9 }}>{tx.token}</span></span>
              <span className="mono" style={{ fontSize: 9, color: ST_COLOR[tx.status], textAlign: "right" }}>{tx.status}</span>
            </div>
          ))}
          <div className="mono" style={{ fontSize: 9, color: "var(--tx-3)", padding: "8px 14px" }}>Agent budget draws and paymaster gas link to their decision id — spend is governance, not just accounting</div>
        </div>
      </div>
      <div className="surface" style={{ padding: "14px 16px", display: "flex", flexDirection: "column", gap: 6 }}>
        <span className="eyebrow">Agent spend — under grants</span>
        <span className="mono" style={{ fontSize: 11, lineHeight: 1.9, color: "var(--tx-2)" }}>claude-code 312 / 500 · codex 448 / 500 ⚠ · devin 96 / 250 · hermes 210 / 400</span>
      </div>
    </div>
  );
}
