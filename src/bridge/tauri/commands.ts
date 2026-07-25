// =====================================================================
// citrate-quorum — typed wrappers over the Rust backend commands (QRM-S2).
//
// One thin function per `#[tauri::command]` in `src-tauri/src/backend.rs`.
// These are the ONLY place the frontend names a Rust command (Rule 11:
// every surface names its command; every command names its source). The
// tauri adapter (./index.ts) composes these into the bridge contract.
//
// Nothing here fabricates: each call round-trips to real Rust that reads the
// real per-tenant HashChain / policy engine. An empty chain returns [].
//
// NOTE: no wrapper takes a tenant. The tenant scope is backend-owned state
// (`tenant_set`); the frontend cannot name the chain it reads or writes, so a
// bug here cannot cross a tenant boundary (Rule 6).
// =====================================================================
import { invoke } from "@tauri-apps/api/core";

import type { Classification, Decision, Verdict } from "../types";

// ---- the tenant scope ----------------------------------------------

/** The active tenant scope, or null if none is established yet. */
export function tenantActive(): Promise<string | null> {
  return invoke<string | null>("tenant_active");
}

/** Establish the active tenant scope. Rejects a malformed tenant id. */
export function tenantSet(tenant: string): Promise<void> {
  return invoke<void>("tenant_set", { tenant });
}

// ---- ledger / audit spine ------------------------------------------

/** The Ledger rows for the active tenant — the real BLAKE3 hash-chained records. */
export function ledgerRecords(): Promise<Decision[]> {
  return invoke<Decision[]>("ledger_records");
}

/** The tenant's chain head — the value that commits to the whole history. */
export function ledgerHead(): Promise<string> {
  return invoke<string>("ledger_head");
}

/** The Merkle root that an `AnchorRegistry` anchor will commit to. */
export function ledgerMerkleRoot(): Promise<string> {
  return invoke<string>("ledger_merkle_root");
}

/** Recompute the whole chain and confirm it was not tampered with. */
export function ledgerVerify(): Promise<boolean> {
  return invoke<boolean>("ledger_verify");
}

/** The count of `ungoverned` actions — the honest headline gap (Rule 5). */
export function ledgerUngovernedCount(): Promise<number> {
  return invoke<number>("ledger_ungoverned_count");
}

// ---- the governed-action pipeline ----------------------------------

export interface ActionInput {
  agent: string;
  principal?: string | null;
  class: string;
  classification: Classification;
  cost: number;
  hic1_cost_threshold?: number;
  mandatory_hic1?: boolean;
  params_hash?: string;
  model_id?: string;
  correlation_id?: string;
}
export interface DecisionResult {
  decision_id: number;
  verdict: Verdict;
  hic: string;
  grant_id: string | null;
  reason: string;
  chain_head: string;
  ungoverned: boolean;
}

/** Evaluate an action against its agent's grants and record the verdict to the
 *  tenant's chain. This is the HIC evidence loop: policy → decision → chain. */
export function actionEvaluateAndRecord(input: ActionInput): Promise<DecisionResult> {
  return invoke<DecisionResult>("action_evaluate_and_record", { input });
}

/** Refund a refused decision's charge (once) and record the refusal. */
export function actionReject(decisionId: number): Promise<DecisionResult> {
  return invoke<DecisionResult>("action_reject", { decisionId });
}

/** Approve an escalation, naming the human who gave the approval. */
export function actionApprove(decisionId: number, approver: string): Promise<DecisionResult> {
  return invoke<DecisionResult>("action_approve", { decisionId, approver });
}

/** The Rust shape of a queued escalation (snake_case on the wire). */
export interface PendingApprovalRow {
  decision: number;
  agent: string;
  principal: string | null;
  action_class: string;
  classification: Classification;
  cost: number;
  correlation_id: string;
  requested_at_ms: number;
}

/** Escalations awaiting a human in the active tenant. */
export function approvalsPending(): Promise<PendingApprovalRow[]> {
  return invoke<PendingApprovalRow[]>("approvals_pending");
}

// ---- capability grants ---------------------------------------------

export interface GrantInput {
  id: string;
  agent: string;
  principal: string;
  tenant_scope: string;
  action_classes: string[];
  classification_ceiling: Classification;
  budget_units: number;
  expires_at_ms: number;
  hic: string; // "1" | "2" | "3"
}
export function grantIssue(input: GrantInput): Promise<void> {
  return invoke<void>("grant_issue", { input });
}
export function grantRevoke(agent: string, grantId: string): Promise<boolean> {
  return invoke<boolean>("grant_revoke", { agent, grantId });
}

// ---- vote allowances (the delegated voting franchise) --------------

export interface AllowanceInput {
  id: string;
  principal: string;
  agent: string;
  tenant_scope: string;
  proposal_classes: string[];
  weight_cap: number;
  expires_at_ms: number;
}
export function allowanceIssue(input: AllowanceInput): Promise<void> {
  return invoke<void>("allowance_issue", { input });
}
export function allowanceRevoke(allowanceId: string): Promise<boolean> {
  return invoke<boolean>("allowance_revoke", { allowanceId });
}
/** Cast a delegated vote; resolves to the delegation proof, rejects fail-closed. */
export function voteCast(allowanceId: string, proposalClass: string, weight: number): Promise<string> {
  return invoke<string>("vote_cast", { allowanceId, proposalClass, weight });
}

// ---- session resolution --------------------------------------------

export interface EffectiveGrantResult {
  classification_ceiling: Classification;
  foreign_national: boolean;
}
/** Resolve the fail-closed least-of-ceilings grant from the commercial tier +
 *  on-chain clearance + tenant classification_max. */
export function sessionResolve(args: {
  tier?: string | null;
  expiresAtMs?: number | null;
  onChainClearance?: Classification | null;
  foreignNational?: boolean | null;
  tenantCeiling?: Classification | null;
}): Promise<EffectiveGrantResult> {
  return invoke<EffectiveGrantResult>("session_resolve", {
    tier: args.tier ?? null,
    expiresAtMs: args.expiresAtMs ?? null,
    onChainClearance: args.onChainClearance ?? null,
    foreignNational: args.foreignNational ?? null,
    tenantCeiling: args.tenantCeiling ?? null,
  });
}
