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
import { useEffect, useState } from "react";
import { bridge } from "../bridge";
import { Domain, useDomain } from "../components/DomainState";
import { VaultPanel } from "../wallet/VaultPanel";
import { WalletSetup } from "../wallet/WalletSetup";

type Tab = "tenancy" | "identity" | "models" | "license" | "compliance";

/**
 * The operator's clearance, read from chain.
 *
 * The distinction this panel exists to keep: **"nobody has recorded a clearance
 * for you" is not "you are cleared to Public".** Both enforce as Public — that is
 * the fail-closed rule and it does not bend — but they need different fixes, and
 * `ClassificationRegistry.getClearance` answers them identically. The read uses
 * `getRecord` so the surface can tell you which one you are looking at.
 */
function ClearancePanel() {
  const [address, setAddress] = useState<string | null>(null);
  const [tenant, setTenant] = useState<string | null>(null);
  useEffect(() => {
    bridge.wallet.identity().then((i) => setAddress(i.address)).catch(() => {});
    bridge.session.activeTenant().then(setTenant).catch(() => {});
  }, []);

  // The source string carries the arguments deliberately. `useDomain` re-runs on
  // `source`, and the address and tenant arrive from their own async reads AFTER
  // the first render — with a constant source the panel fired once against
  // `null`, reported "no signing identity", and never retried, while the identity
  // sat rendered two panels above it. An honest error plate for a state that had
  // already passed is its own kind of false. Naming the arguments also satisfies
  // Rule 11 better than "settings.clearance()" did.
  const read = useDomain(
    () =>
      address && tenant
        ? bridge.settings.clearance(address, tenant)
        : Promise.reject(new Error(
            address
              ? "no tenant scope is established yet — a clearance is read per tenant"
              : "no signing identity yet — a clearance is recorded against an address",
          )),
    `settings.clearance(${address ?? "…"}, ${tenant ?? "…"})`,
  );

  return (
    <div className="surface" style={{ padding: 16, display: "flex", flexDirection: "column", gap: 10 }}>
      <span className="eyebrow">Clearance</span>
      <Domain read={read} source={`settings.clearance(${address ?? "…"}, ${tenant ?? "…"})`} skeletonRows={2}>
        {(c) => (
          <>
            <div style={{ display: "grid", gridTemplateColumns: "190px 1fr", gap: "8px 12px", fontSize: 13 }}>
              <span className="lbl" style={{ margin: 0 }}>On chain</span>
              <span>
                {c.recorded ? (
                  <>
                    <span className="mono" style={{ fontSize: 8.5, letterSpacing: ".1em", color: CLS_COLOR[c.effective] ?? "var(--tx-2)", border: `1px solid ${CLS_COLOR[c.effective] ?? "var(--line-2)"}`, padding: "1px 6px" }}>{c.effective}</span>
                    {c.foreignNational !== null && (
                      <span className="mono" style={{ fontSize: 10, color: "var(--tx-3)", marginLeft: 8 }}>
                        foreign national: {c.foreignNational ? "yes" : "no"}
                      </span>
                    )}
                  </>
                ) : (
                  <span style={{ color: "var(--warn)" }}>
                    no record — nobody has recorded a clearance for this address. That is not the same as being
                    cleared to Public, and it enforces as Public either way.
                  </span>
                )}
              </span>
              <span className="lbl" style={{ margin: 0 }}>Tenant ceiling</span>
              <span className="mono" style={{ fontSize: 11.5 }}>{c.tenantCeiling ?? "— no node for this tenant"}</span>
              <span className="lbl" style={{ margin: 0 }}>Bounded to</span>
              <span className="mono" style={{ fontSize: 11.5, color: "var(--tx-1)" }}>{c.boundedTo}</span>
              <span className="lbl" style={{ margin: 0 }}>Subject key</span>
              <span className="mono" style={{ fontSize: 9.5, color: "var(--tx-3)", wordBreak: "break-all" }}>{c.subject}</span>
            </div>
            {c.note && (
              <div className="mono" style={{ fontSize: 10.5, color: "var(--warn)", border: "1px solid var(--warn)", background: "var(--warn-bg)", padding: "8px 10px", lineHeight: 1.6 }}>
                {c.note}
              </div>
            )}
            <span className="mono" style={{ fontSize: 9.5, color: "var(--tx-3)", lineHeight: 1.6, wordBreak: "break-word" }}>
              {c.source}
              <br />
              A clearance is written by an HR oracle signer, not by this app — point it at the subject key above.
              The commercial-tier axis is not read here; the enterprise axes are.
            </span>
          </>
        )}
      </Domain>
    </div>
  );
}
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
  // A SECOND counter, deliberately not the same one. `WalletSetup` is keyed on
  // `vaultEpoch`, so having its `onReady` bump that same value remounts it, which
  // fires `onReady` again, which remounts it… — a loop I wrote and the packaged
  // app showed as a clearance panel that never resolved. The identity signal
  // feeds only the panels that consume the address.
  const [identityEpoch, setIdentityEpoch] = useState(0);

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
          {/* `onReady` fires when a key is created, imported, or simply found in
              an unlocked vault. It had no caller anywhere in the app — so the
              clearance panel below, which needs the address, had no way to learn
              it had appeared. Bumping the same epoch remounts that panel. */}
          <WalletSetup key={vaultEpoch} onReady={() => setIdentityEpoch((n) => n + 1)} />
          <NotBuilt
            what="Upstream identity federation — an enterprise IdP over OIDC, SCIM provisioning, and the deprovision SLA that doubles as the agent kill-switch — is not wired. The app has an OIDC client in the shared kit, but no directory is connected, so nothing here can report a federation, a seat count, or an SLA."
            lands="QRM-S8"
          />
          {/* Keyed on the vault epoch, exactly as WalletSetup above is: this
              panel reads the signing identity ONCE on mount, and the identity
              does not exist until the vault is unlocked AND a key is imported.
              Read once, it showed "no signing identity yet" while the address was
              rendered two panels above — a true sentence about a state that had
              already passed. Keyed on BOTH epochs, because the vault unlocking
              and a key arriving are different moments and this panel needs the
              later one. */}
          <ClearancePanel key={`${vaultEpoch}:${identityEpoch}`} />
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
