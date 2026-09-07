// citrate-quorum — surface registry (QRM-S2D). Maps a nav id to its ported
// surface. Un-registered ids fall through to the honest Placeholder (Rule 1).
// Each surface port adds one line here.
import type { ReactNode } from "react";
import { Dashboard } from "./Dashboard";
import { Ledger } from "./Ledger";
import { Governance } from "./Governance";
import { Meetings } from "./Meetings";
import { Rooms } from "./Rooms";
import { Agents } from "./Agents";
import { Journal } from "./Journal";
import { Calendar } from "./Calendar";
import { Repos } from "./Repos";
import { Wallet } from "./Wallet";
import { Node } from "./Node";
import { Settings } from "./Settings";

export type SurfaceProps = { onGo: (id: string) => void };

export const SURFACES: Record<string, (p: SurfaceProps) => ReactNode> = {
  dashboard: ({ onGo }) => <Dashboard onGo={onGo} />,
  ledger: () => <Ledger />,
  governance: () => <Governance />,
  meetings: () => <Meetings />,
  rooms: () => <Rooms />,
  agents: () => <Agents />,
  journal: () => <Journal />,
  calendar: ({ onGo }) => <Calendar onGo={onGo} />,
  repos: () => <Repos />,
  wallet: () => <Wallet />,
  node: () => <Node />,
  settings: ({ onGo }) => <Settings onGo={onGo} />,
};
