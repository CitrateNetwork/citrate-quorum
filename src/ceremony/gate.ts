// =====================================================================
// citrate-quorum — the pre-ceremony policy gate (QRM-S2).
//
// The decision of whether a signature is even asked for. Split out of
// Ceremony.tsx so the security property — that an action which cannot be
// gated is REFUSED, not waved through — is unit-testable without a DOM.
//
// I-3/I-4: every governed action is evaluated and recorded BEFORE it executes
// or is signed. The verdicts are not advisory:
//   deny             → refused here; the ceremony never opens
//   gate unreachable → refused here, fail-closed, with the reason
//   otherwise        → the ceremony opens, carrying the recorded verdict
//
// `ungoverned` deliberately OPENS the ceremony. An action with no live grant
// is not blocked — it is recorded as ungoverned and surfaced to the human, who
// decides in full knowledge. Never silently allowed, never silently dropped.
// =====================================================================
import type { GateDecision, GovernedAction } from "../bridge";

export interface CeremonyResult {
  outcome: "settled" | "rejected";
  /** A short human-readable settlement note (chain head / decision id). */
  note?: string;
  /** What the policy gate recorded. Absent only if the gate was unreachable. */
  gate?: GateDecision;
}

export type GateOutcome =
  | { open: true; gate: GateDecision }
  | { open: false; result: CeremonyResult };

/**
 * Run the gate for one action.
 *
 * @param evaluate  the bridge's policy.evaluate — evaluates AND records.
 * @param action    the proposed governed action.
 * @param principal the signing human, stamped on when the action does not
 *                  already name one. Undefined leaves the record without a
 *                  principal rather than inventing one.
 */
export async function runGate(
  evaluate: (a: GovernedAction) => Promise<GateDecision>,
  action: GovernedAction,
  principal?: string,
): Promise<GateOutcome> {
  let gate: GateDecision;
  try {
    gate = await evaluate({ ...action, principal: action.principal ?? principal });
  } catch (e) {
    // Fail closed: an unreachable gate is a refusal, not a bypass. The most
    // common cause is no tenant scope established yet.
    return {
      open: false,
      result: {
        outcome: "rejected",
        note: `policy gate unavailable — ${e instanceof Error ? e.message : String(e)}`,
      },
    };
  }
  if (gate.verdict === "deny") {
    // Already recorded by evaluate(); the dialog never opens.
    return { open: false, result: { outcome: "rejected", note: gate.reason, gate } };
  }
  return { open: true, gate };
}
