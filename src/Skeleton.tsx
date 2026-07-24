// citrate-quorum — the honest skeleton shell (WP-S1.3). Replaced by the design
// prototype (QRM-S2D). It states plainly what is and isn't built (Rule 1), and
// proves the Tauri backend + the shared citrate-core-kit signing spine are live
// by calling the one honest status command.
import { useEffect, useState } from "react";

type SkeletonStatus = {
  app: string;
  kit_linked: boolean;
  license_enforced: boolean;
  license_note: string;
  surfaces_built: boolean;
};

// The planned surfaces, in the sidebar order from the design brief (§3.1).
const SURFACES = [
  "Dashboard", "Governance", "Agents",
  "Rooms", "Meetings", "Calendar",
  "Ledger", "Journal", "Repos",
  "Wallet", "Node", "Settings",
];

export function Skeleton() {
  const [status, setStatus] = useState<SkeletonStatus | null>(null);
  const [err, setErr] = useState<string | null>(null);

  useEffect(() => {
    // Detect Tauri without a hard dependency on the API in the web/dev path.
    const isTauri =
      typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
    if (!isTauri) {
      setErr("running in a browser (no Tauri backend) — this is the dev shell");
      return;
    }
    import("@tauri-apps/api/core")
      .then(({ invoke }) => invoke<SkeletonStatus>("quorum_skeleton_status"))
      .then(setStatus)
      .catch((e) => setErr(String(e)));
  }, []);

  return (
    <main data-register="charter" style={{ minHeight: "100vh", padding: "48px 40px", maxWidth: 880, margin: "0 auto" }}>
      <div className="eyebrow" style={{ marginBottom: 8 }}>citrate-quorum · QRM-S1</div>
      <h1 style={{ fontFamily: "var(--font-display)", fontWeight: 460, fontSize: 34, margin: "0 0 6px" }}>
        Governance for agents
      </h1>
      <p style={{ color: "var(--tx-2)", fontSize: 15, lineHeight: 1.5, maxWidth: 620 }}>
        This is the app skeleton. The backend and the shared signing spine
        (citrate-core-kit) are wired; the governance surfaces are not built yet.
        The real interface arrives from the design prototype and is integrated in
        QRM-S2D. Nothing below is fabricated — it states exactly what is real.
      </p>

      <section className="surface" style={{ padding: 18, marginTop: 24 }}>
        <div className="lbl">backend status</div>
        {status ? (
          <ul className="mono" style={{ fontSize: 13, lineHeight: 1.8, listStyle: "none", padding: 0, margin: 0 }}>
            <li>app · {status.app}</li>
            <li>shared kit linked · {status.kit_linked ? "yes" : "no"}</li>
            <li>seat metering enforced · {status.license_enforced ? "yes" : "no (honest — WP-S1.6)"}</li>
            <li>governance surfaces built · {status.surfaces_built ? "yes" : "no (arriving QRM-S2D+)"}</li>
          </ul>
        ) : (
          <div className="mono" style={{ fontSize: 13, color: "var(--tx-3)" }}>{err ?? "querying backend…"}</div>
        )}
      </section>

      <section style={{ marginTop: 24 }}>
        <div className="lbl" style={{ marginBottom: 10 }}>planned surfaces (not built)</div>
        <div style={{ display: "grid", gridTemplateColumns: "repeat(3, 1fr)", gap: 8 }}>
          {SURFACES.map((s) => (
            <div key={s} className="mono" style={{
              fontSize: 12, padding: "10px 12px", border: "1px solid var(--line-1)",
              borderRadius: "var(--r-1)", color: "var(--tx-3)", background: "var(--srf-1)",
            }}>
              {s} · unbuilt
            </div>
          ))}
        </div>
      </section>
    </main>
  );
}
