// citrate-quorum — frontend entry (QRM-S2D).
// The design prototype (design/CitrateQuorum.dc.html) is being integrated
// surface-by-surface through the bridge seam (src/bridge). The Shell renders
// the sidebar/topbar/status-rail and routes to surfaces; ported surfaces read
// real (sim) data, un-ported ones show an honest plate (Rule 1).
import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import "./styles/index.css";
import { Shell } from "./shell/Shell";

createRoot(document.getElementById("root")!).render(
  <StrictMode>
    <Shell />
  </StrictMode>,
);
