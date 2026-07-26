// citrate-quorum — Settings surface (QRM-S2D). Charter register. Ported from
// design §SETTINGS. Five tabs: Tenancy (scope tree, ceilings, thresholds),
// Identity (upstream IdP federation + SCIM; the deprovision SLA IS the agent
// kill-switch SLA), Models (bundled Gemma + LoRA, BYO keys, governed egress
// policy — read-only here), License (metering not enforced — honest plate), and
// Compliance. Reads bridge.settings.tenancy().
import { useState } from "react";
import { bridge } from "../bridge";
import { DomainErrorPlate, useDomain } from "../components/DomainState";

type Tab = "tenancy" | "identity" | "models" | "license" | "compliance";
const CLS_COLOR: Record<string, string> = { Public: "var(--z-silver)", Proprietary: "var(--info)", CUI: "var(--warn)", ITAR: "var(--danger)" };

export function Settings({ onGo }: { onGo: (id: string) => void }) {
  const [tab, setTab] = useState<Tab>("tenancy");

  // Honest failure (S2D.4/§5.1): this surface's primary read is settings.tenancy().
  // A read that cannot succeed must say so and offer a retry, not sit in a
  // loading state forever.
  const primary = useDomain(() => bridge.settings.tenancy(), "settings.tenancy()");
  // Derived, not mirrored (see Agents.tsx).
  const tenancy = primary.state.status === "ready" ? primary.state.data : [];
  if (primary.state.status === "error") {
    return (
      <div style={{ padding: 18 }}>
        <DomainErrorPlate source="settings.tenancy()" error={primary.state.error} onRetry={primary.retry} lands="It reads TenantHierarchy on chain and needs a live chain." />
      </div>
    );
  }

  const tabBtn = (t: Tab, label: string) => {
    const on = tab === t;
    return <button key={t} onClick={() => setTab(t)} className="mono" style={{ fontSize: 10, letterSpacing: ".13em", textTransform: "uppercase", background: "transparent", border: "none", cursor: "pointer", padding: "8px 14px", color: on ? "var(--tx-1)" : "var(--tx-3)", borderBottom: `2px solid ${on ? "var(--accent)" : "transparent"}`, marginBottom: -1 }}>{label}</button>;
  };
  const row = (k: string, v: React.ReactNode) => (
    <><span className="lbl" style={{ margin: 0 }}>{k}</span><span>{v}</span></>
  );

  return (
    <div style={{ padding: "22px 26px", display: "flex", flexDirection: "column", gap: 16, maxWidth: 860 }}>
      <div style={{ display: "flex", borderBottom: "1px solid var(--line-2)" }}>
        {tabBtn("tenancy", "Tenancy")}{tabBtn("identity", "Identity")}{tabBtn("models", "Models")}{tabBtn("license", "License")}{tabBtn("compliance", "Compliance")}
      </div>

      {tab === "tenancy" && (
        <div className="surface" style={{ display: "flex", flexDirection: "column" }}>
          <div className="mono" style={{ display: "grid", gridTemplateColumns: "1fr 200px 130px 110px", gap: 10, padding: "8px 16px", fontSize: 9, letterSpacing: ".12em", textTransform: "uppercase", color: "var(--tx-3)", borderBottom: "1px solid var(--line-1)" }}>
            <span>Scope</span><span>Admins</span><span>Class ceiling</span><span>Threshold</span>
          </div>
          {tenancy.map((t) => (
            <div key={t.name} style={{ display: "grid", gridTemplateColumns: "1fr 200px 130px 110px", gap: 10, padding: "9px 16px", borderBottom: "1px solid var(--line-1)", alignItems: "center" }}>
              <span style={{ fontSize: 13, fontWeight: 500, paddingLeft: t.depth * 18 }}>{t.name}</span>
              <span style={{ fontSize: 12, color: "var(--tx-2)" }}>{t.admins}</span>
              <span className="mono" style={{ fontSize: 8.5, letterSpacing: ".1em", color: CLS_COLOR[t.ceiling], border: `1px solid ${CLS_COLOR[t.ceiling]}`, padding: "1px 6px", width: "fit-content" }}>{t.ceiling}</span>
              <span className="mono tabular" style={{ fontSize: 10.5, color: "var(--tx-2)" }}>{t.threshold}</span>
            </div>
          ))}
        </div>
      )}

      {tab === "identity" && (
        <div className="surface" style={{ padding: 16, display: "flex", flexDirection: "column", gap: 10 }}>
          <div style={{ display: "grid", gridTemplateColumns: "190px 1fr", gap: "8px 12px", fontSize: 13 }}>
            {row("Federation", "Meridian Okta · OIDC · healthy")}
            {row("SCIM sync", "last 4m ago · 1,204 seats")}
            {row("Clearance source", "ClassificationRegistry (chain) ← HR export, dual-signed")}
            {row("Deprovision SLA", "15 minutes")}
          </div>
          <div style={{ border: "1px solid var(--warn)", background: "var(--warn-bg)", padding: "10px 12px", fontSize: 12.5, lineHeight: 1.5 }}>Honest note: this SLA <em>is</em> the agent kill-switch SLA. When a human leaves, every grant issued under their authority suspends within the same window.</div>
        </div>
      )}

      {tab === "models" && (
        <div className="surface" style={{ padding: 16, display: "flex", flexDirection: "column", gap: 10 }}>
          <div style={{ display: "grid", gridTemplateColumns: "190px 1fr", gap: "8px 12px", fontSize: 13 }}>
            {row("Bundled model", <span className="mono" style={{ fontSize: 11.5 }}>Gemma 3 4B · governance LoRA v2.3 (rank 16)</span>)}
            {row("Gateway", <span className="mono" style={{ fontSize: 11.5 }}>127.0.0.1:8484 · local</span>)}
            {row("BYO keys", <span className="mono" style={{ fontSize: 11.5 }}>anthropic ●●●● k3B1 · keyring-sealed, shown once</span>)}
          </div>
          <div style={{ border: "1px solid var(--line-2)", background: "var(--srf-inset)", padding: "10px 12px", display: "flex", alignItems: "center", gap: 12 }}>
            <div style={{ flex: 1 }}>
              <div style={{ fontSize: 12.5, fontWeight: 500 }}>Egress policy — read-only here</div>
              <div className="mono" style={{ fontSize: 10, color: "var(--tx-3)" }}>in force: PRT-002 · CUI+ local-only · set by a deployed protocol, not a preference</div>
            </div>
            <button className="btn btn-ghost btn-sm" onClick={() => onGo("governance")}>This is governed — change it in Governance</button>
          </div>
        </div>
      )}

      {tab === "license" && (
        <div className="surface" style={{ padding: 16, display: "flex", flexDirection: "column", gap: 10 }}>
          <div style={{ display: "flex", gap: 24 }}>
            <div><div className="eyebrow">Seats licensed</div><div className="tabular" style={{ fontFamily: "var(--font-display)", fontSize: 28, fontWeight: 460 }}>1,500</div></div>
            <div><div className="eyebrow">Seats active</div><div className="tabular" style={{ fontFamily: "var(--font-display)", fontSize: 28, fontWeight: 460 }}>1,204</div></div>
          </div>
          <div style={{ border: "1.5px dashed var(--line-2)", padding: "12px 14px", fontSize: 12.5, color: "var(--tx-3)" }}>Metering is not enforced yet — counts are informational. Enforcement lands in <span className="mono">QRM-S9</span>. This plate is deliberate: an unbuilt surface must look unbuilt.</div>
        </div>
      )}

      {tab === "compliance" && (
        <div className="surface" style={{ padding: 16, display: "flex", flexDirection: "column", gap: 10 }}>
          <div style={{ display: "grid", gridTemplateColumns: "190px 1fr", gap: "8px 12px", fontSize: 13 }}>
            {row("Evidence export", "auditor packs from Ledger · verify.sh included")}
            {row("Retention", "decisions indefinite (chain) · transcripts 7y · drafts 90d")}
            {row("Consent", "room voice: local transcription, never stored — per-member consent recorded")}
          </div>
        </div>
      )}
    </div>
  );
}
