// citrate-quorum — sidebar navigation (QRM-S2D). Grouped Govern → Meet →
// Observe → Operate (design brief §3.1). Each item carries two 1.5px-stroke
// SVG paths (Lucide-derived), the register it renders in (§2.2), and which
// sprint ports it. `built` surfaces render real (sim) data; the rest show an
// honest "being ported" plate (Rule 1).
export type Register = "charter" | "instrument";

export interface NavItem {
  id: string;
  label: string;
  p1: string;
  p2: string;
  register: Register;
  built: boolean;
}
export interface NavGroup {
  label: string;
  items: NavItem[];
}

export const NAV: NavGroup[] = [
  {
    label: "Govern",
    items: [
      { id: "dashboard", label: "Dashboard", register: "instrument", built: true,
        p1: "M3 11 L12 3 L21 11 V20 H14 V14 H10 V20 H3 Z", p2: "M0 0" },
      { id: "governance", label: "Governance", register: "charter", built: false,
        p1: "M12 3 L4 7 V12 C4 16.5 7.5 20.5 12 21 C16.5 20.5 20 16.5 20 12 V7 Z", p2: "M9 12 L11 14 L15 10" },
      { id: "agents", label: "Agents", register: "instrument", built: false,
        p1: "M12 3 A4 4 0 0 1 12 11 A4 4 0 0 1 12 3 Z", p2: "M5 21 V19 A4 4 0 0 1 9 15 H15 A4 4 0 0 1 19 19 V21" },
    ],
  },
  {
    label: "Meet",
    items: [
      { id: "rooms", label: "Rooms", register: "instrument", built: false,
        p1: "M4 5 H16 A2 2 0 0 1 18 7 V15 A2 2 0 0 1 16 17 H9 L5 20 V17 H4 A2 2 0 0 1 2 15 V7 A2 2 0 0 1 4 5 Z", p2: "M20 8 L22 8 V16 L20 16" },
      { id: "meetings", label: "Meetings", register: "charter", built: false,
        p1: "M6 3 H18 A2 2 0 0 1 20 5 V21 L12 17 L4 21 V5 A2 2 0 0 1 6 3 Z", p2: "M8 8 H16 M8 11 H13" },
      { id: "calendar", label: "Calendar", register: "charter", built: false,
        p1: "M4 6 A2 2 0 0 1 6 4 H18 A2 2 0 0 1 20 6 V19 A2 2 0 0 1 18 21 H6 A2 2 0 0 1 4 19 Z", p2: "M4 9 H20 M8 3 V6 M16 3 V6" },
    ],
  },
  {
    label: "Observe",
    items: [
      { id: "ledger", label: "Ledger", register: "instrument", built: false,
        p1: "M5 3 H19 V21 H5 A0 0 0 0 1 5 21 Z", p2: "M8 8 H16 M8 12 H16 M8 16 H13" },
      { id: "journal", label: "Journal", register: "charter", built: false,
        p1: "M6 3 H19 V21 H6 A2 2 0 0 1 4 19 V5 A2 2 0 0 1 6 3 Z", p2: "M9 3 V21 M12.5 8 H16 M12.5 12 H15" },
      { id: "repos", label: "Repos", register: "instrument", built: false,
        p1: "M6 3 A3 3 0 0 0 6 9 H16 A3 3 0 0 1 16 15 M6 9 V21 M6 3 V9", p2: "M6 21 A0 0 0 0 1 6 21 M16 15 A3 3 0 0 0 16 21" },
    ],
  },
  {
    label: "Operate",
    items: [
      { id: "wallet", label: "Wallet", register: "instrument", built: false,
        p1: "M3 7 V17 A2 2 0 0 0 5 19 H19 A2 2 0 0 0 21 17 V9 A2 2 0 0 0 19 7 Z M3 7 A2 2 0 0 1 5 5 H16", p2: "M15.5 13 H17.5" },
      { id: "node", label: "Node", register: "instrument", built: false,
        p1: "M4 5 H20 V11 H4 Z M4 13 H20 V19 H4 Z", p2: "M7 8 H7.01 M7 16 H7.01" },
      { id: "settings", label: "Settings", register: "charter", built: false,
        p1: "M4 7 H20 M4 12 H20 M4 17 H20", p2: "M9 5 V9 M15 10 V14 M8 15 V19" },
    ],
  },
];

/** All items flattened, for routing + title lookup. */
export const NAV_ITEMS: NavItem[] = NAV.flatMap((g) => g.items);
