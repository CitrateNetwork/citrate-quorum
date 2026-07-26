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
/**
 * A human acting under their own authority — not an agent asking permission.
 *
 * The policy engine binds AGENTS: it answers "which grant lets this agent do
 * this". An operator has no grant, so evaluating them returns `ungoverned` —
 * and HIC-X is "an alert state, never a configuration" (04_HIC_MODEL §2).
 * Running operators through the agent gate showed a red UNGOVERNED plate for
 * issuing a grant, and wrote an ungoverned record for every human act, which
 * inflates the one number the product exists to keep honest.
 *
 * The act is still recorded — by whichever command commits it, naming the
 * human (`record_principal_action` in the backend) — so this is not a bypass.
 * It is the difference between "nobody authorised this" and "a person did it".
 */
function principalAuthority(): GateDecision {
  return {
    decisionId: -1,
    verdict: "approved",
    hic: "1",
    grantId: null,
    reason: "you are acting under your own authority — recorded against your name",
    chainHead: "",
    ungoverned: false,
  };
}

/** Human-origin intents come from the operator; anything else is an agent. */
export function isPrincipalOrigin(origin: string): boolean {
  return origin === "user";
}

export async function runGate(
  evaluate: (a: GovernedAction) => Promise<GateDecision>,
  action: GovernedAction,
  principal: string | undefined,
  /**
   * REQUIRED, and deliberately not defaulted: a default of "user" would mean
   * that forgetting this argument silently skips the agent gate. The safe
   * failure for an omitted origin is a compile error.
   */
  origin: string,
): Promise<GateOutcome> {
  // L-1 makes these HIC-1 regardless; the ceremony IS that control.
  if (isPrincipalOrigin(origin)) {
    return { open: true, gate: principalAuthority() };
  }
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

/**
 * What the ceremony is allowed to say once it settles.
 *
 * Pulled out of the dialog and made pure because this exact sentence was
 * wrong: the fallback asserted "recorded against your name in this tenant's
 * evidence chain" for every intent, including human-origin ones that skip the
 * agent gate and therefore record nothing at that moment. The chain was
 * provably empty while the claim was on screen.
 *
 * The rule, in precedence order:
 *  1. the commit told us what it did — use its words;
 *  2. the gate recorded a decision — name the chain head it produced;
 *  3. there was a commit but it returned nothing — a write did happen;
 *  4. nothing was written — say exactly that, and do not imply otherwise.
 */
export function settledNote(input: {
  commitNote?: string | void;
  chainHead?: string;
  hasCommit: boolean;
}): string {
  if (typeof input.commitNote === "string" && input.commitNote.length > 0) {
    return input.commitNote;
  }
  if (input.chainHead) {
    return `decision recorded · chain head ${input.chainHead.slice(0, 10)}…`;
  }
  if (input.hasCommit) {
    return "committed — recorded against your name in this tenant's evidence chain";
  }
  return "approved by you · this ceremony recorded nothing — the action it authorises is written by whatever you do next";
}
