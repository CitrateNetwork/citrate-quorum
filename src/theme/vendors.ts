// citrate-quorum — agent-vendor branding (QRM-S2D). Categorical colors + names
// for the agent vendors, used across surfaces (Rooms roster, Agents fleet,
// transcript chips). This is DESIGN CONFIG, not sim data — it is stable
// branding, so it lives in the theme, not the bridge.
export interface Vendor { name: string; color: string }
export const VENDORS: Record<string, Vendor> = {
  anthropic: { name: "Anthropic", color: "var(--z-cyan)" },
  openai: { name: "OpenAI", color: "var(--z-indigo)" },
  cognition: { name: "Cognition", color: "var(--z-magenta)" },
  nous: { name: "Nous", color: "var(--z-amber)" },
  internal: { name: "Internal", color: "var(--z-silver)" },
};
