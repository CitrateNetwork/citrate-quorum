// citrate-quorum — Settings surface. Charter register.
//
// Tenancy is LIVE as of Phase 0: the tree is read from `TenantHierarchy` on
// chain 40204, resolved by name from the BFR address book at runtime. A tree
// that comes back empty says WHY — a contract with no root is a deployment
// state an operator must act on, not an app with no tenants.
//
// The other four tabs describe things that are not built. They previously read
// as configured systems ("Meridian Okta · OIDC · healthy", "SCIM sync · 1,204
// seats", "Gemma 3 4B · governance LoRA v2.3", "Seats licensed 1,500"). None of
// that existed. Each is now a plate that names what is absent and where it
// lands — an unbuilt surface must look unbuilt (Rule 1).
//
// Reads bridge.settings.tenancy(); the vault and signing identity come from the
// shared kit surface (VaultPanel / WalletSetup).
import { useState } from "react";
import { bridge } from "../bridge";
import { Domain, useDomain } from "../components/DomainState";
import { VaultPanel } from "../wallet/VaultPanel";
import { WalletSetup } from "../wallet/WalletSetup";

type Tab = "tenancy" | "identity" | "models" | "license" | "compliance";
const CLS_COLOR: Record<string, string> = { Public: "var(--z-silver)", Proprietary: "var(--info)", CUI: "var(--warn)", ITAR: "var(--danger)" };

/** A plate for something that is not built. Names it, and where it lands. */
function NotBuilt({ what, lands }: { what: string; lands: string }) {
  return (
    <div style={{ border: "1.5px dashed var(--line-2)", padding: "12px 14px", fontSize: 12.5, color: "var(--tx-3)", lineHeight: 1.6 }}>
      {what} <span className="mono">{lands}</span>
    </div>
  );
}

export function Settings({ onGo }: { onGo: (id: string) => void }) {
  const [tab, setTab] = useState<Tab>("tenancy");
  // Bumped whenever the vault's state changes. Remounting WalletSetup is the
  // simplest correct way to make it re-read the identity: a vault that just
  // unlocked turns "no signing identity" into a real address.
  const [vaultEpoch, setVaultEpoch] = useState(0);

  const primary = useDomain(() => bridge.settings.tenancy(), "settings.tenancy()");

  const tabBtn = (t: Tab, label: string) => {
    const on = tab === t;
    return <button key={t} onClick={() => setTab(t)} className="mono" style={{ fontSize: 10, letterSpacing: ".13em", textTransform: "uppercase", background: "transparent", border: "none", cursor: "pointer", padding: "8px 14px", color: on ? "var(--tx-1)" : "var(--tx-3)", borderBottom: `2px solid ${on ? "var(--accent)" : "transparent"}`, marginBottom: -1 }}>{label}</button>;
  };

  return (
    <div style={{ padding: "22px 26px", display: "flex", flexDirection: "column", gap: 16, maxWidth: 900 }}>
      <div style={{ display: "flex", borderBottom: "1px solid var(--line-2)" }}>
        {tabBtn("tenancy", "Tenancy")}{tabBtn("identity", "Identity")}{tabBtn("models", "Models")}{tabBtn("license", "License")}{tabBtn("compliance", "Compliance")}
      </div>

      {tab === "tenancy" && (
        <Domain
          read={primary}
          source="settings.tenancy()"
          lands="It reads TenantHierarchy on chain and needs a reachable chain."
          skeletonRows={4}
        >
          {(t) => (
            <div style={{ display: "flex", flexDirection: "column", gap: 12 }}>
              {t.note && (
                <div style={{ border: "1px solid var(--warn)", background: "var(--warn-bg)", padding: "12px 14px", fontSize: 12.5, lineHeight: 1.6 }}>
                  <div className="mono" style={{ fontSize: 9.5, letterSpacing: ".13em", textTransform: "uppercase", color: "var(--warn)", marginBottom: 6 }}>
                    Nothing to show, and here is why
                  </div>
                  {t.note}
                </div>
              )}
              {t.rows.length > 0 && (
                <div className="surface" style={{ display: "flex", flexDirection: "column" }}>
                  <div className="mono" style={{ display: "grid", gridTemplateColumns: "1fr 240px 130px 110px", gap: 10, padding: "8px 16px", fontSize: 9, letterSpacing: ".12em", textTransform: "uppercase", color: "var(--tx-3)", borderBottom: "1px solid var(--line-1)" }}>
                    <span>Scope</span><span>Admins</span><span>Class ceiling</span><span>Threshold</span>
                  </div>
                  {t.rows.map((r) => (
                    <div key={r.id} style={{ display: "grid", gridTemplateColumns: "1fr 240px 130px 110px", gap: 10, padding: "9px 16px", borderBottom: "1px solid var(--line-1)", alignItems: "center" }}>
                      <span style={{ fontSize: 13, fontWeight: 500, paddingLeft: r.depth * 18 }} title={r.id}>{r.name}</span>
                      <span className="mono" style={{ fontSize: 10, color: "var(--tx-2)", overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" }} title={r.admins}>{r.admins}</span>
                      <span className="mono" style={{ fontSize: 8.5, letterSpacing: ".1em", color: CLS_COLOR[r.ceiling] ?? "var(--danger)", border: `1px solid ${CLS_COLOR[r.ceiling] ?? "var(--danger)"}`, padding: "1px 6px", width: "fit-content" }}>{r.ceiling}</span>
                      <span className="mono tabular" style={{ fontSize: 10.5, color: "var(--tx-2)" }}>{r.threshold}</span>
                    </div>
                  ))}
                </div>
              )}
              <div className="mono" style={{ fontSize: 9.5, color: "var(--tx-3)", lineHeight: 1.6, wordBreak: "break-word" }}>
                settings.tenancy() → {t.source}
              </div>
            </div>
          )}
        </Domain>
      )}

      {tab === "identity" && (
        <div style={{ display: "flex", flexDirection: "column", gap: 16 }}>
          <VaultPanel onChange={() => setVaultEpoch((n) => n + 1)} />
          <WalletSetup key={vaultEpoch} />
          <NotBuilt
            what="Upstream identity federation — an enterprise IdP over OIDC, SCIM provisioning, and the deprovision SLA that doubles as the agent kill-switch — is not wired. The app has an OIDC client in the shared kit, but no directory is connected, so nothing here can report a federation, a seat count, or an SLA."
            lands="QRM-S8"
          />
          <div className="surface" style={{ padding: 16, display: "flex", flexDirection: "column", gap: 8 }}>
            <span className="eyebrow">Clearance</span>
            <span style={{ fontSize: 12.5, color: "var(--tx-2)", lineHeight: 1.6 }}>
              A person's clearance ceiling is resolved fail-closed as the least of their commercial tier, their
              on-chain clearance and their tenant's <span className="mono">classification_max</span>. The tenant half
              of that is live on the Tenancy tab. <span className="mono">ClassificationRegistry</span> is deployed but
              this app does not read it yet, so the on-chain half is not in force.
            </span>
          </div>
        </div>
      )}

      {tab === "models" && (
        <div style={{ display: "flex", flexDirection: "column", gap: 12 }}>
          <NotBuilt
            what="No model runtime is bundled with or wired into this app: no local gateway, no governance LoRA, no BYO provider keys. Agents reach the app from outside, through the keyless agent bridge, and are governed at that seam — which is why nothing here has to trust a model."
            lands="QRM-S8"
          />
          <div className="surface" style={{ padding: 16, display: "flex", alignItems: "center", gap: 12 }}>
            <div style={{ flex: 1 }}>
              <div style={{ fontSize: 12.5, fontWeight: 500 }}>Egress policy is governance, not a preference</div>
              <div className="mono" style={{ fontSize: 10, color: "var(--tx-3)", lineHeight: 1.6 }}>
                When it exists it will be set by a deployed protocol, so it is changed in Governance and read-only here.
                No protocol contract is deployed yet.
              </div>
            </div>
            <button className="btn btn-ghost btn-sm" onClick={() => onGo("governance")}>Governance</button>
          </div>
        </div>
      )}

      {tab === "license" && (
        <NotBuilt
          what="Seat metering is not enforced and no seat count is collected, so no number is shown here. The license seam exists in the backend and reports itself unenforced rather than pretending to a count."
          lands="QRM-S9"
        />
      )}

      {tab === "compliance" && (
        <div style={{ display: "flex", flexDirection: "column", gap: 12 }}>
          <div className="surface" style={{ padding: 16, display: "flex", flexDirection: "column", gap: 8 }}>
            <span className="eyebrow">What is real today</span>
            <span style={{ fontSize: 12.5, color: "var(--tx-2)", lineHeight: 1.6 }}>
              Every governed action is recorded to a per-tenant BLAKE3 hash chain that can be replayed from genesis,
              and any single decision can be proved into that chain's Merkle root — see Ledger → Verify. Ratified
              minutes are committed on chain in <span className="mono">MeetingRegistry</span>.
            </span>
          </div>
          <NotBuilt
            what="Auditor evidence export (a signed pack with the records, the roots and an offline verify script), retention policy enforcement, and recorded per-member consent are not built."
            lands="QRM-S9"
          />
          <div style={{ border: "1px solid var(--line-2)", background: "var(--srf-inset)", padding: "12px 14px", fontSize: 12.5, color: "var(--tx-2)", lineHeight: 1.6 }}>
            This product is designed to generate SOC 2 Type 2 control evidence in your environment. It is not itself
            certified, and it cannot be: the audited entity is the organisation running it.
          </div>
        </div>
      )}
    </div>
  );
}
