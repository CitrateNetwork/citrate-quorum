// citrate-quorum — Wallet surface.
//
// LIVE as of Phase 0: the address is the vault's own signing identity and the
// balances are live reads from chain 40204 (`eth_getBalance` for the native
// currency, ERC-20 `balanceOf` for booked tokens).
//
// What the ported design prototype showed here and this does NOT:
//   · a transaction history ("grant budget draw", "validator reward", …). This
//     app runs no transaction index and 40204's RPC cannot enumerate an
//     address's past. An empty table would have implied "no activity"; the note
//     says what is actually true.
//   · "Paymaster budget 64%", "Tier-2 revenue 30d", staking bond/APR/rewards.
//     No contract on the frozen book reports any of those for this identity.
//   · agent spend bars ("claude-code 312/500"). Agent budgets are real and live
//     on the Agents surface, read from the grants that hold them — they were
//     never wallet balances.
//   · a Send panel and a Stake panel. Both opened a ceremony that moved no
//     money and then reported success. Sending needs a transfer intent the
//     ceremony can sign — the same path minutes registration already uses — and
//     that is its own work package, not a plate.
//
// Reads bridge.wallet.summary().
import { bridge } from "../bridge";
import { Domain, useDomain } from "../components/DomainState";

export function Wallet() {
  const primary = useDomain(() => bridge.wallet.summary(), "wallet.summary()");

  return (
    <div style={{ padding: 18, display: "flex", flexDirection: "column", gap: 14, maxWidth: 1000 }}>
      <Domain
        read={primary}
        source="wallet.summary()"
        lands="It needs a signing identity in an unlocked vault (Settings → Identity) and a reachable chain."
        skeletonRows={4}
      >
        {(w) => {
          const native = w.tokens.find((t) => t.native);
          return (
            <>
              <div style={{ display: "grid", gridTemplateColumns: "repeat(3,1fr)", gap: 10 }}>
                <div className="surface" style={{ padding: 16 }}>
                  <div className="eyebrow">Balance</div>
                  <div className="tabular" style={{ fontFamily: "var(--font-display)", fontSize: 26, fontWeight: 460, wordBreak: "break-all" }}>
                    {native?.balance ?? "—"} <span style={{ fontSize: 13, color: "var(--tx-3)" }}>SALT</span>
                  </div>
                  <div className="mono" style={{ fontSize: 9.5, color: "var(--tx-3)" }}>eth_getBalance · exact, to the wei</div>
                </div>
                <div className="surface" style={{ padding: 16 }}>
                  <div className="eyebrow">Chain</div>
                  <div className="tabular" style={{ fontFamily: "var(--font-display)", fontSize: 26, fontWeight: 460 }}>{w.chainId}</div>
                  <div className="mono" style={{ fontSize: 9.5, color: "var(--tx-3)", overflow: "hidden", textOverflow: "ellipsis" }}>{w.rpcUrl}</div>
                </div>
                <div className="surface" style={{ padding: 16 }}>
                  <div className="eyebrow">Key custody</div>
                  <div style={{ fontFamily: "var(--font-display)", fontSize: 17, fontWeight: 460, lineHeight: 1.3 }}>vault-held</div>
                  <div className="mono" style={{ fontSize: 9.5, color: "var(--tx-3)", lineHeight: 1.5 }}>{w.keyStore}</div>
                </div>
              </div>

              <div className="surface" style={{ display: "flex", alignItems: "center", gap: 12, padding: "11px 16px", flexWrap: "wrap" }}>
                <span className="eyebrow">Account</span>
                <span className="mono" style={{ fontSize: 11.5, color: "var(--tx-2)", wordBreak: "break-all" }}>{w.address}</span>
                <div style={{ flex: 1 }} />
                <span className="mono" style={{ fontSize: 9.5, color: "var(--tx-3)" }}>
                  this is the key that signs every ratification
                </span>
              </div>

              <div className="surface" style={{ display: "flex", flexDirection: "column" }}>
                <div style={{ padding: "10px 14px", borderBottom: "1px solid var(--line-1)" }}><span className="eyebrow">Balances</span></div>
                {w.tokens.map((tk) => (
                  <div key={tk.symbol} style={{ display: "grid", gridTemplateColumns: "80px 1fr auto", gap: 10, alignItems: "baseline", padding: "10px 14px", borderBottom: "1px solid var(--line-1)" }}>
                    <span className="mono" style={{ fontSize: 11, fontWeight: 500 }}>{tk.symbol}</span>
                    <span style={{ fontSize: 11, color: "var(--tx-3)", minWidth: 0, overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" }} title={tk.source}>
                      {tk.name}
                    </span>
                    <span className="mono tabular" style={{ fontSize: 12, wordBreak: "break-all", textAlign: "right" }}>{tk.balance}</span>
                  </div>
                ))}
                {w.notes.map((n) => (
                  <div key={n} className="mono" style={{ fontSize: 10, color: "var(--warn)", padding: "8px 14px", borderBottom: "1px solid var(--line-1)", lineHeight: 1.6 }}>
                    {n}
                  </div>
                ))}
                <div className="mono" style={{ fontSize: 9, color: "var(--tx-3)", padding: "8px 14px", lineHeight: 1.6 }}>
                  wallet.summary() · {w.source}
                  <br />
                  Only tokens named in the frozen address book are read. A tenant's own tokens need a per-tenant
                  token list, which does not exist yet — so none is guessed at.
                </div>
              </div>

              <div className="surface" style={{ padding: "14px 16px", display: "flex", flexDirection: "column", gap: 8 }}>
                <span className="eyebrow">Activity</span>
                <span style={{ fontSize: 12.5, color: "var(--tx-2)", lineHeight: 1.6 }}>{w.activityNote}</span>
              </div>

              <div style={{ border: "1.5px dashed var(--line-2)", padding: "12px 14px", fontSize: 12.5, color: "var(--tx-3)", lineHeight: 1.6 }}>
                Sending and staking are not wired. Moving money from this identity needs a transfer intent the
                SignatureCeremony can sign — the same path on-chain minutes registration already uses — and it lands
                with its own work package. Until then this surface reads; it does not spend. The vault holds the key
                either way.
              </div>
            </>
          );
        }}
      </Domain>
    </div>
  );
}
