// citrate-quorum — the app shell (QRM-S2D). Ported from
// design/CitrateQuorum.dc.html §SHELL. Sidebar (evergreen) + main grid
// (topbar 52px / surface / status rail 30px). The surface area is
// register-aware: `data-register` flips per the active surface (§2.2), and
// the ceremony (later) always forces charter. Routes via hash.
import { useEffect, useState } from "react";
import { BRIDGE_MODE, bridge } from "../bridge";
import type { Session } from "../bridge";
import { NAV, NAV_ITEMS, type NavItem } from "./nav";
import { SURFACES } from "../surfaces/registry";
import { Placeholder } from "../surfaces/Placeholder";
import { CommandPalette } from "./CommandPalette";
import { EscalationToast } from "./EscalationToast";
import markWhite from "../assets/brand/citrate_mark_white.svg";
import marqueeWhite from "../assets/brand/citrate_marquee_white.svg";

function routeFromHash(): string {
  const h = window.location.hash.replace(/^#\/?/, "");
  return NAV_ITEMS.some((i) => i.id === h) ? h : "dashboard";
}

const HIC_PILL_COLOR: Record<number, string> = {
  0: "var(--ok)",
  1: "var(--ok)",
  2: "var(--info)",
  3: "var(--warn)",
};

export function Shell() {
  const [route, setRoute] = useState<string>(routeFromHash());
  const [session, setSession] = useState<Session | null>(null);
  const [hicOpen, setHicOpen] = useState(false);
  const [palOpen, setPalOpen] = useState(false);

  useEffect(() => {
    bridge.session.current().then(setSession);
    const onHash = () => setRoute(routeFromHash());
    const onKey = (e: KeyboardEvent) => {
      if ((e.metaKey || e.ctrlKey) && e.key.toLowerCase() === "k") { e.preventDefault(); setPalOpen((v) => !v); }
      if (e.key === "Escape") setPalOpen(false);
    };
    window.addEventListener("hashchange", onHash);
    window.addEventListener("keydown", onKey);
    return () => { window.removeEventListener("hashchange", onHash); window.removeEventListener("keydown", onKey); };
  }, []);

  const go = (id: string) => {
    window.location.hash = `#/${id}`;
    setRoute(id);
  };

  const active: NavItem =
    NAV_ITEMS.find((i) => i.id === route) ?? NAV_ITEMS[0];
  const hicColor = session ? HIC_PILL_COLOR[session.hic.level] : "var(--info)";

  return (
    <div style={{ height: "100vh", display: "grid", gridTemplateColumns: "224px 1fr", minWidth: 0 }}>
      {/* sidebar */}
      <div style={{ background: "var(--deep-evergreen)", color: "#cde7d6", display: "flex", flexDirection: "column", minHeight: 0 }}>
        <div style={{ display: "flex", alignItems: "center", gap: 9, padding: "16px 14px 12px" }}>
          <img src={markWhite} alt="" style={{ width: 22, height: 22 }} />
          <img src={marqueeWhite} alt="Citrate" style={{ height: 12 }} />
          <div style={{ flex: 1 }} />
          <span className="mono" style={{ fontSize: 8.5, letterSpacing: ".18em", color: "rgba(205,231,214,.6)" }}>QUORUM</span>
        </div>
        <div style={{ flex: 1, overflow: "auto", padding: "4px 8px", display: "flex", flexDirection: "column", gap: 2 }}>
          {NAV.map((g) => (
            <div key={g.label}>
              <div className="mono" style={{ fontSize: 9, letterSpacing: ".16em", textTransform: "uppercase", color: "rgba(205,231,214,.45)", padding: "12px 8px 5px" }}>{g.label}</div>
              {g.items.map((it) => {
                const on = it.id === route;
                return (
                  <div key={it.id} onClick={() => go(it.id)} style={{ display: "flex", alignItems: "center", gap: 10, padding: "7px 8px", borderRadius: "var(--r-1)", cursor: "pointer", background: on ? "var(--accent)" : "transparent", color: on ? "var(--ink)" : "#cde7d6", fontWeight: on ? 500 : 400 }}>
                    <svg viewBox="0 0 24 24" width="16" height="16" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round" style={{ flexShrink: 0 }}>
                      <path d={it.p1} /><path d={it.p2} />
                    </svg>
                    <span style={{ fontSize: 13, fontWeight: 500, flex: 1 }}>{it.label}</span>
                    {!it.built && <span style={{ width: 6, height: 6, borderRadius: 999, background: "rgba(205,231,214,.3)" }} title="being ported" />}
                  </div>
                );
              })}
            </div>
          ))}
        </div>
        <div style={{ borderTop: "1px solid rgba(205,231,214,.14)", padding: "12px 14px", display: "flex", flexDirection: "column", gap: 10 }}>
          <div style={{ display: "flex", alignItems: "center", gap: 10 }}>
            <span style={{ width: 30, height: 30, borderRadius: 999, background: "rgba(142,204,9,.18)", border: "1px solid rgba(142,204,9,.5)", display: "inline-flex", alignItems: "center", justifyContent: "center", fontSize: 11, fontWeight: 600, color: "var(--citrate-green)", flexShrink: 0 }}>
              {session?.user.initials ?? "··"}
            </span>
            <div style={{ minWidth: 0 }}>
              <div style={{ fontSize: 12.5, fontWeight: 500, color: "#e9efe7" }}>{session?.user.name ?? "—"}</div>
              <div className="mono" style={{ fontSize: 9, letterSpacing: ".08em", color: "rgba(205,231,214,.55)" }}>{session ? `${session.user.sbt} · ${session.tenant[session.tenant.length - 1]}` : ""}</div>
            </div>
          </div>
          <div style={{ display: "flex", alignItems: "center", gap: 8 }}>
            <span className="mono" style={{ fontSize: 9, letterSpacing: ".12em", textTransform: "uppercase", color: "var(--warn)", border: "1px solid var(--warn)", padding: "1px 6px" }}>{session?.user.clearance ?? "—"}</span>
            <div style={{ flex: 1 }} />
            <span style={{ width: 7, height: 7, borderRadius: 999, background: "var(--citrate-green)" }} />
            <span className="mono" style={{ fontSize: 9, color: "rgba(205,231,214,.55)" }}>{session ? `synced · ${session.chain.height.toLocaleString("en-US")}` : "…"}</span>
          </div>
        </div>
      </div>

      {/* main */}
      <div style={{ display: "grid", gridTemplateRows: "52px 1fr 30px", minWidth: 0, minHeight: 0, background: "var(--srf-0)", color: "var(--tx-1)" }} data-register={active.register}>
        {/* topbar */}
        <div style={{ display: "flex", alignItems: "center", gap: 14, padding: "0 18px", borderBottom: "1px solid var(--line-1)", background: "var(--srf-1)", position: "relative" }}>
          <button className="mono" style={{ fontSize: 10, letterSpacing: ".06em", color: "var(--tx-2)", background: "transparent", border: "1px solid var(--line-2)", borderRadius: "var(--r-1)", padding: "4px 9px", cursor: "pointer", whiteSpace: "nowrap", overflow: "hidden", textOverflow: "ellipsis", maxWidth: 340 }}>
            {session ? session.tenant.join(" › ") : "…"}
          </button>
          <div style={{ fontFamily: "var(--font-display)", fontWeight: 460, fontSize: 17, letterSpacing: "-0.008em" }}>{active.label}</div>
          <div style={{ flex: 1 }} />
          <button className="mono" onClick={() => setPalOpen(true)} style={{ fontSize: 10, color: "var(--tx-3)", background: "transparent", border: "1px solid var(--line-1)", borderRadius: "var(--r-1)", padding: "4px 9px", cursor: "pointer" }}>⌘K</button>
          <button className="mono" onClick={() => setHicOpen((v) => !v)} style={{ fontSize: 10.5, fontWeight: 500, letterSpacing: ".1em", color: hicColor, background: "transparent", border: `1px solid ${hicColor}`, borderRadius: 999, padding: "4px 11px", cursor: "pointer", display: "inline-flex", alignItems: "center", gap: 7 }}>
            <span style={{ width: 7, height: 7, borderRadius: 999, background: hicColor }} />{session?.hic.label ?? "HIC"}
          </button>
          {hicOpen && session && (
            <div data-register="charter" className="surface" style={{ color: "var(--tx-1)", position: "absolute", top: 48, right: 18, width: 340, padding: 16, zIndex: 60, boxShadow: "0 12px 32px rgba(14,15,12,.18)", borderRadius: "var(--r-2)" }}>
              <div className="eyebrow" style={{ marginBottom: 8 }}>{session.hic.label} · budgeted autonomy</div>
              <p style={{ fontSize: 13, lineHeight: 1.55, color: "var(--tx-2)", margin: "0 0 10px" }}>{session.hic.desc}</p>
              <div className="mono" style={{ fontSize: 10, color: "var(--tx-3)", borderTop: "1px solid var(--line-1)", paddingTop: 8 }}>Set by {session.hic.setBy}</div>
            </div>
          )}
        </div>

        {/* surface */}
        <div style={{ minHeight: 0, overflow: "auto", position: "relative" }}>
          {SURFACES[active.id] ? SURFACES[active.id]({ onGo: go }) : <Placeholder item={active} />}
        </div>

        {/* status rail */}
        <div className="mono" style={{ display: "flex", alignItems: "center", gap: 16, padding: "0 18px", borderTop: "1px solid var(--line-1)", background: "var(--srf-1)", fontSize: 10, color: "var(--tx-3)" }}>
          <span>chain 40204 · {session ? session.chain.height.toLocaleString("en-US") : "…"}</span>
          <span>relay {session?.chain.relay ?? "—"}</span>
          <span>anchor {session?.chain.anchorRoot ?? "—"}</span>
          <div style={{ flex: 1 }} />
          {/* Name the adapter actually running. This read "surface: live (sim)"
              unconditionally, so the packaged Tauri build reported "sim". */}
          <span>{active.built ? `surface: live (${BRIDGE_MODE})` : "surface: being ported"}</span>
        </div>
      </div>
      <CommandPalette open={palOpen} onClose={() => setPalOpen(false)} onGo={go} />
      <EscalationToast onGo={go} />
    </div>
  );
}
