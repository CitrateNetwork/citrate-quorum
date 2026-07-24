// citrate-quorum — frontend entry (QRM-S2D). Onboarding (sign-in → clearance →
// HIC tour) gates the Shell; the Shell hosts the 12 surfaces, the ceremony
// overlay, the command palette, and the escalation toast. All read through the
// bridge seam (sim on web, tauri in the packaged app).
import { StrictMode, useState } from "react";
import { createRoot } from "react-dom/client";
import "./styles/index.css";
import { Shell } from "./shell/Shell";
import { CeremonyProvider } from "./ceremony/Ceremony";
import { Onboarding } from "./onboarding/Onboarding";

function App() {
  const [entered, setEntered] = useState(() => {
    try { return localStorage.getItem("quorum-entered") === "1"; } catch { return false; }
  });
  const enter = () => {
    try { localStorage.setItem("quorum-entered", "1"); } catch { /* ignore */ }
    setEntered(true);
  };
  if (!entered) return <Onboarding onDone={enter} />;
  return (
    <CeremonyProvider>
      <Shell />
    </CeremonyProvider>
  );
}

createRoot(document.getElementById("root")!).render(
  <StrictMode>
    <App />
  </StrictMode>,
);
