// citrate-quorum — onboarding (QRM-S2D). Ported from design §ONBOARDING.
// Sign in with the federated IdP → clearance resolves from chain (nothing
// granted by default, access is computed then shown) → a 4-screen HIC tour →
// the app. Plus the fail-closed reduced-access state. Charter register.
import { useEffect, useState } from "react";
import { LoaderMark } from "../components/LoaderMark";

type Phase = "sign" | "resolve" | "tour" | "reduced";

const RESOLVE_STEPS = [
  "Meridian Okta — OIDC id_token verified",
  "Entitlement claim: commercial.kyc · orgId meridian",
  "On-chain clearance: ClassificationRegistry → CUI (non-FN)",
  "EffectiveGrant = least of (tier ceiling, clearance, tenant max) → CUI",
];

const TOUR = [
  { badge: "HIC-1", color: "var(--ok)", title: "Approve each", body: "For anything that touches chain state, money, keys, or a capability grant, a human signs that specific action in a ceremony. No auto-approve, no 'approve latest'. One approval yields exactly one signature." },
  { badge: "HIC-2", color: "var(--info)", title: "Budgeted autonomy", body: "For routine work, agents act unattended inside a bounded envelope — scope, budget, expiry. Every act is recorded; exhausting the budget or leaving scope escalates to HIC-1. This is where most work lives." },
  { badge: "HIC-3", color: "var(--warn)", title: "Post-hoc review", body: "For bulk mechanical work, agents act with a mandatory review window before effects become final. Fast, still accountable." },
  { badge: "HIC-X", color: "var(--danger)", title: "Ungoverned is an alert", body: "An action with no live grant behind it is never silently allowed and never silently dropped — it is flagged ungoverned and surfaced. That is the difference between a dashboard and evidence." },
];

export function Onboarding({ onDone }: { onDone: () => void }) {
  const [phase, setPhase] = useState<Phase>("sign");
  const [checks, setChecks] = useState(0);
  const [tourIdx, setTourIdx] = useState(0);

  useEffect(() => {
    if (phase !== "resolve") return;
    setChecks(0);
    const timers = RESOLVE_STEPS.map((_, i) => setTimeout(() => setChecks(i + 1), 500 + i * 550));
    const done = setTimeout(() => setPhase("tour"), 500 + RESOLVE_STEPS.length * 550 + 400);
    return () => { timers.forEach(clearTimeout); clearTimeout(done); };
  }, [phase]);

  if (phase === "sign") {
    return (
      <div data-register="charter" style={{ height: "100vh", display: "flex", flexDirection: "column", background: "var(--srf-0)", overflow: "auto" }}>
        <div className="lattice-dots" style={{ flex: 1, display: "flex", flexDirection: "column", alignItems: "center", justifyContent: "center", gap: 24, padding: "48px 24px", textAlign: "center" }}>
          <div style={{ width: 160, height: 160 }}><LoaderMark size={160} /></div>
          <div style={{ display: "flex", alignItems: "center", gap: 10 }}>
            <img src="/src/assets/brand/citrate_marquee_black.svg" alt="Citrate" style={{ height: 18 }} />
            <span className="mono" style={{ fontSize: 11, letterSpacing: ".18em", textTransform: "uppercase", color: "var(--tx-2)", border: "1px solid var(--line-2)", padding: "2px 8px" }}>Quorum</span>
          </div>
          <div style={{ maxWidth: 560, display: "flex", flexDirection: "column", gap: 12, alignItems: "center" }}>
            <div style={{ fontFamily: "var(--font-display)", fontWeight: 380, fontSize: 44, lineHeight: 1.08, letterSpacing: "-0.02em" }}>A ledger you can hold a meeting inside.</div>
            <p style={{ fontSize: 15.5, lineHeight: 1.55, color: "var(--tx-2)", margin: 0, maxWidth: 480 }}>Humans in control and agents from every vendor, meeting under governance that is written in plain English and enforced on chain.</p>
          </div>
          <button className="btn btn-primary btn-lg" onClick={() => setPhase("resolve")}>Continue with Meridian SSO · Okta</button>
          <div className="eyebrow">Federated identity · clearance resolves from chain · chain 40204</div>
          <a href="#" onClick={(e) => { e.preventDefault(); setPhase("reduced"); }} style={{ fontSize: 12 }}>View the reduced-access state</a>
        </div>
      </div>
    );
  }

  if (phase === "resolve") {
    return (
      <div data-register="charter" style={{ height: "100vh", display: "flex", alignItems: "center", justifyContent: "center", padding: 32, background: "var(--srf-0)" }}>
        <div className="surface" style={{ width: 460, padding: 28, display: "flex", flexDirection: "column", gap: 18 }}>
          <div style={{ display: "flex", alignItems: "center", gap: 16 }}>
            <div style={{ width: 44, height: 44, flexShrink: 0 }}><LoaderMark size={44} /></div>
            <div>
              <div style={{ fontSize: 15, fontWeight: 500 }}>Resolving your clearance</div>
              <div className="mono" style={{ fontSize: 10.5, color: "var(--tx-3)" }}>nothing is granted by default — access is computed, then shown</div>
            </div>
          </div>
          <div style={{ display: "flex", flexDirection: "column", gap: 9, borderTop: "1px solid var(--line-1)", paddingTop: 14 }}>
            {RESOLVE_STEPS.map((label, i) => {
              const done = i < checks;
              return (
                <div key={i} style={{ display: "flex", alignItems: "center", gap: 10 }}>
                  <span style={{ width: 18, height: 18, borderRadius: 999, display: "inline-flex", alignItems: "center", justifyContent: "center", background: done ? "var(--ok-bg)" : "var(--srf-inset)", border: `1px solid ${done ? "var(--ok)" : "var(--line-2)"}`, color: done ? "var(--ok)" : "var(--tx-3)", flexShrink: 0 }}>
                    <svg viewBox="0 0 24 24" width="11" height="11" fill="none" stroke="currentColor" strokeWidth="2.6" strokeLinecap="round"><path d="M5 12 L10 17 L19 8" /></svg>
                  </span>
                  <span className="mono" style={{ fontSize: 11.5, color: done ? "var(--tx-1)" : "var(--tx-3)" }}>{label}</span>
                </div>
              );
            })}
          </div>
        </div>
      </div>
    );
  }

  if (phase === "tour") {
    const t = TOUR[tourIdx];
    const last = tourIdx === TOUR.length - 1;
    return (
      <div data-register="charter" style={{ height: "100vh", display: "flex", alignItems: "center", justifyContent: "center", padding: 32, background: "var(--srf-0)" }}>
        <div className="surface cc-fade-up" style={{ width: 560, padding: 34, display: "flex", flexDirection: "column", gap: 18, borderTop: "2px solid var(--line-strong)" }}>
          <div className="eyebrow">Human in control · {tourIdx + 1} of 4</div>
          <div style={{ display: "flex", alignItems: "center", gap: 12 }}>
            <span className="mono" style={{ fontSize: 13, fontWeight: 500, padding: "4px 10px", border: `1px solid ${t.color}`, color: t.color }}>{t.badge}</span>
            <div style={{ fontFamily: "var(--font-display)", fontWeight: 420, fontSize: 26, letterSpacing: "-0.01em" }}>{t.title}</div>
          </div>
          <p style={{ fontSize: 15, lineHeight: 1.6, color: "var(--tx-2)", margin: 0 }}>{t.body}</p>
          <div style={{ display: "flex", alignItems: "center", gap: 14, borderTop: "1px solid var(--line-1)", paddingTop: 16 }}>
            <div style={{ display: "flex", gap: 6 }}>
              {TOUR.map((_, i) => <span key={i} style={{ width: 8, height: 8, borderRadius: 999, background: i <= tourIdx ? "var(--accent)" : "transparent", border: "1px solid var(--line-2)" }} />)}
            </div>
            <div style={{ flex: 1 }} />
            <button className="btn btn-primary" onClick={() => (last ? onDone() : setTourIdx((i) => i + 1))}>{last ? "Enter Quorum" : "Next"}</button>
          </div>
        </div>
      </div>
    );
  }

  // reduced (fail-closed)
  return (
    <div data-register="charter" style={{ height: "100vh", display: "flex", alignItems: "center", justifyContent: "center", padding: 32, background: "var(--srf-0)" }}>
      <div className="surface" style={{ width: 520, padding: 30, display: "flex", flexDirection: "column", gap: 14, borderTop: "2px solid var(--warn)" }}>
        <div className="eyebrow" style={{ color: "var(--warn)" }}>Access changed</div>
        <div style={{ fontFamily: "var(--font-display)", fontWeight: 420, fontSize: 26 }}>Your clearance was reduced</div>
        <p style={{ fontSize: 14.5, lineHeight: 1.6, color: "var(--tx-2)", margin: 0 }}>Your CUI entitlement expired on 2026-07-20 and has not been renewed by your identity provider. Quorum fails closed: you are signed in at <span className="mono">PUBLIC</span> clearance. Rooms, documents, and decisions above Public show a redaction plate naming what is required.</p>
        <div className="mono" style={{ fontSize: 11, color: "var(--tx-3)", border: "1px solid var(--line-1)", padding: "10px 12px", background: "var(--srf-inset)" }}>Who can restore it — Meridian IdP admin (SCIM group aero-cui) · deprovision SLA 15 min · source: identity federation, not this app</div>
        <div><button className="btn btn-ghost" onClick={() => setPhase("sign")}>Back to sign in</button></div>
      </div>
    </div>
  );
}
