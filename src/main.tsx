// citrate-quorum — frontend entry (WP-S1.3 SKELETON PLACEHOLDER).
//
// This is a deliberately minimal, honest "under construction" shell. The real
// surfaces (Rooms, Meetings, Governance, Agents, Ledger, Journal, Calendar,
// Repos, Wallet, Node, Settings) arrive from the DESIGN PROTOTYPE (see the
// planset's 10_DESIGN_BRIEF.md) and are integrated in QRM-S2D. The design
// prototype REPLACES this file and everything under src/shell, src/surfaces,
// src/bridge, src/components. Nothing here fabricates governance data (Rule 1).
import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import "./styles/index.css";
import { Skeleton } from "./Skeleton";

createRoot(document.getElementById("root")!).render(
  <StrictMode>
    <Skeleton />
  </StrictMode>,
);
