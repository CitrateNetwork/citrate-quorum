// =====================================================================
// citrate-quorum — the BRIDGE (QRM-S2D)
//
// The one seam between surfaces and backend. `import { bridge } from
// "./bridge"`. `sim` on web/dev, `tauri` in the packaged app. Surfaces
// never know which adapter is underneath; flipping a domain sim→live
// changes one adapter, zero surfaces.
// =====================================================================
import { BRIDGE_MODE } from "./mode";
import type { BridgeContract } from "./domains";
import { createSimBridge } from "./sim";
import { createTauriBridge } from "./tauri";

export type { BridgeContract } from "./domains";
export * from "./types";

export const bridge: BridgeContract =
  BRIDGE_MODE === "tauri" ? createTauriBridge() : createSimBridge();

export { BRIDGE_MODE };
