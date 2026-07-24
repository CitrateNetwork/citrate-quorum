// citrate-quorum — honest not-yet-ported surface plate (QRM-S2D, Rule 1).
// The prototype (design/CitrateQuorum.dc.html) has the full design for every
// surface; they are ported one at a time. Until a surface is ported it says so
// plainly — it never renders fabricated data.
import type { NavItem } from "../shell/nav";

export function Placeholder({ item }: { item: NavItem }) {
  return (
    <div style={{ padding: 48, maxWidth: 640 }}>
      <div className="eyebrow" style={{ marginBottom: 8 }}>
        {item.label} · being ported
      </div>
      <h2
        style={{
          fontFamily: "var(--font-display)",
          fontWeight: 460,
          fontSize: 22,
          margin: "0 0 10px",
        }}
      >
        This surface is not built yet
      </h2>
      <p style={{ color: "var(--tx-2)", fontSize: 14, lineHeight: 1.55 }}>
        The <strong>{item.label}</strong> surface is designed in the prototype
        and is being translated into the app in QRM-S2D, wired through the same
        bridge seam as the Dashboard. Until then it shows this honest plate
        rather than placeholder data.
      </p>
      <p className="mono" style={{ fontSize: 11, color: "var(--tx-3)", marginTop: 14 }}>
        renders in the <strong>{item.register}</strong> register · bridge domain
        ready · surface pending
      </p>
    </div>
  );
}
