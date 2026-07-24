// citrate-quorum — bridge runtime selection (QRM-S2D). `tauri` inside the
// packaged app, else `sim`. Detected once, at the boundary.
export function detectMode(): "sim" | "tauri" {
  if (typeof window !== "undefined" && "__TAURI_INTERNALS__" in window) {
    return "tauri";
  }
  return "sim";
}
export const BRIDGE_MODE: "sim" | "tauri" = detectMode();
