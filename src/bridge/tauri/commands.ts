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

/** The human at the keyboard, or null if nobody has been named. */
export function operatorGet(): Promise<string | null> {
  return invoke<string | null>("operator_get");
}

/** Name the human at the keyboard. Rejects an empty name. */
export function operatorSet(operator: string): Promise<void> {
  return invoke<void>("operator_set", { operator });
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

export interface LedgerStateDto {
  head: string;
  merkle_root: string;
  records: number;
  ungoverned: number;
  intact: boolean;
  tenant: string;
}
/** Head, root, counts and integrity in ONE read, so they describe the same
 *  chain at the same instant. */
export function ledgerState(): Promise<LedgerStateDto> {
  return invoke<LedgerStateDto>("ledger_state");
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

/** The Rust shape of one decision document (snake_case on the wire). */
export interface DecisionDetailDto {
  id: string;
  what: string;
  when: string;
  principal: string;
  agent: string;
  grant: string;
  protocol: string;
  verdict: string;
  hic: string;
  reason: string;
  model: string;
  params: string;
  correlation: string;
  chain_pos: string;
  entry_hash: string;
  content_hash: string;
  chain_head: string;
  merkle_root: string;
  proof_len: number;
  included: boolean;
  source: string;
}
export interface VerifyDecisionDto {
  chain_intact: boolean;
  included: boolean;
  records: number;
  entry_hash: string;
  merkle_root: string;
  proof_len: number;
}
export interface CorrelationViewDto {
  events: { t: string; kind: string; text: string; link: string }[];
  source: string;
}

/** One decision as a document — a local read of the tenant's evidence chain. */
export function ledgerDecision(id: string): Promise<DecisionDetailDto> {
  return invoke<DecisionDetailDto>("ledger_decision", { id });
}
/** Replay the chain and re-prove this record's inclusion. Real work, on click. */
export function ledgerVerifyDecision(id: string): Promise<VerifyDecisionDto> {
  return invoke<VerifyDecisionDto>("ledger_verify_decision", { id });
}
/** Everything recorded under one correlation id. */
export function ledgerCorrelation(corr: string): Promise<CorrelationViewDto> {
  return invoke<CorrelationViewDto>("ledger_correlation", { corr });
}

// ---- live chain reads (Phase 0) -------------------------------------
//
// `node_status` / `node_blocks` / `tenancy_tree` / `wallet_summary` all make
// real RPC calls to the endpoint the address book names. They read; none of
// them signs. `node_activity` returns this app's own record of those calls.

export interface ChainStatusDto {
  rpc_url: string;
  book: string;
  chain_id: number;
  height: number;
  peers: number | null;
  client: string | null;
  syncing: boolean | null;
  latency_ms: number;
  base_fee_wei: string | null;
  blue_score: number | null;
}
export interface BlockRowDto {
  height: number;
  hash: string;
  txs: number;
  proposer: string;
  gas_used: number;
  gas_limit: number;
  timestamp: number;
  blue_score: number | null;
  merge_parents: number;
}
export interface ActivityLineDto {
  t: string;
  lvl: string;
  module: string;
  msg: string;
}
export interface TenancyViewDto {
  rows: {
    depth: number;
    name: string;
    admins: string;
    ceiling: string;
    threshold: string;
    id: string;
  }[];
  source: string;
  note: string | null;
}
export interface WalletSummaryDto {
  address: string;
  chain_id: number;
  rpc_url: string;
  key_store: string;
  source: string;
  tokens: { symbol: string; name: string; balance: string; native: boolean; source: string }[];
  notes: string[];
  activity_note: string;
}

export function nodeStatus(): Promise<ChainStatusDto> {
  return invoke<ChainStatusDto>("node_status");
}
export function nodeBlocks(count: number): Promise<BlockRowDto[]> {
  return invoke<BlockRowDto[]>("node_blocks", { count });
}
export function nodeActivity(): Promise<ActivityLineDto[]> {
  return invoke<ActivityLineDto[]>("node_activity");
}
export function tenancyTree(): Promise<TenancyViewDto> {
  return invoke<TenancyViewDto>("tenancy_tree");
}
export function walletSummary(): Promise<WalletSummaryDto> {
  return invoke<WalletSummaryDto>("wallet_summary");
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
  /** The human issuing it. The backend refuses an anonymous grant. */
  issued_by: string;
  principal: string;
  tenant_scope: string;
  action_classes: string[];
  classification_ceiling: Classification;
  budget_units: number;
  expires_at_ms: number;
  hic: string; // "1" | "2" | "3"
}
export interface AgentRow {
  id: string;
  live_grants: number;
  budget_units: number;
  consumed: number;
  decisions: number;
  ungoverned: number;
}
/** Agents this tenant has evidence about — granted, or seen acting. */
export function agentsKnown(): Promise<AgentRow[]> {
  return invoke<AgentRow[]>("agents_known");
}

export interface GrantRow {
  id: string;
  agent: string;
  principal: string;
  scope: string;
  classes: string;
  ceiling: Classification;
  budget_units: number;
  consumed: number;
  hic: string;
  expires_at_ms: number;
  revoked: boolean;
}
/** The capability grants one agent holds, revoked ones included. */
export function grantsForAgent(agent: string): Promise<GrantRow[]> {
  return invoke<GrantRow[]>("grants_for_agent", { agent });
}

export function grantIssue(input: GrantInput): Promise<void> {
  return invoke<void>("grant_issue", { input });
}
export function grantRevoke(
  agent: string,
  grantId: string,
  revokedBy: string,
): Promise<boolean> {
  return invoke<boolean>("grant_revoke", { agent, grantId, revokedBy });
}

// ---- the kit signing path (shared surface) --------------------------
//
// `sign_and_broadcast` is the ONLY command that produces a real, broadcast
// 40204 transaction. It consumes a pending ceremony bound to `id` — no
// auto-approve, no "approve latest" — signs with the vault key, and fails
// closed if the vault is locked.

export interface CeremonyViewDto {
  id: string;
  origin: string;
  chainId: number;
  rawUnverified?: boolean;
}
export interface BroadcastResultDto {
  txHash: string;
  blockNumber: number | null;
}

export function signAndBroadcast(id: string, rawAck: boolean): Promise<BroadcastResultDto> {
  return invoke<BroadcastResultDto>("sign_and_broadcast", { id, rawAck });
}
export function signReject(id: string): Promise<void> {
  return invoke<void>("sign_reject", { id });
}
/** Build the minutes-registration tx as a PENDING ceremony. Signs nothing. */
export function meetingRegisterIntent(id: string): Promise<CeremonyViewDto> {
  return invoke<CeremonyViewDto>("meeting_register_intent", { id });
}

// ---- custody vault (shared kit surface) ----------------------------
//
// The vault is a SHARED kit surface (config / custody / auth / ceremony), not
// a quorum domain, so it is not part of the BridgeContract — there is no sim
// analogue of an OS keyring and faking one would be worse than none.

export interface CustodyStatusDto {
  initialized: boolean;
  unlocked: boolean;
  autolockMins: number;
  keyringStatus: string;
}

export function custodyStatus(): Promise<CustodyStatusDto> {
  return invoke<CustodyStatusDto>("custody_status");
}
/** Create the vault. The passphrase is zeroized in Rust after use. */
export function custodyInit(passphrase: string): Promise<void> {
  return invoke<void>("custody_init", { passphrase });
}
export function custodyUnlock(passphrase: string): Promise<void> {
  return invoke<void>("custody_unlock", { passphrase });
}
export function custodyLock(): Promise<void> {
  return invoke<void>("custody_lock");
}

// ---- wallet setup (QRM-S6) -----------------------------------------

export interface WalletStatusDto {
  exists: boolean;
  address: string | null;
  reason: string | null;
}
/**
 * The response to creation — the ONLY place a recovery phrase crosses this
 * boundary, once, at generation. It is never obtainable again: there is no
 * command that reads it back. Callers must display-and-drop, never persist.
 */
export interface WalletCreatedDto {
  address: string;
  mnemonic: string;
}

export function walletStatus(): Promise<WalletStatusDto> {
  return invoke<WalletStatusDto>("wallet_status");
}
export function walletCreate(): Promise<WalletCreatedDto> {
  return invoke<WalletCreatedDto>("wallet_create");
}
export function walletImport(mnemonic: string): Promise<WalletStatusDto> {
  return invoke<WalletStatusDto>("wallet_import", { mnemonic });
}

// ---- meetings (QRM-S5) ---------------------------------------------

export interface MeetingRowDto {
  id: string;
  name: string;
  when: string;
  tpl: string;
  humans: number;
  agents: number;
  classification: string;
  state: string;
}

export interface MeetingDetailDto {
  id: string;
  name: string;
  when: string;
  tenant: string;
  classification: string;
  state: string;
  agenda_hash: string | null;
  agenda_source: string;
  agenda_skipped: number;
  ratified: boolean;
  ratified_by: string | null;
  ratified_at: number | null;
  content_hash: string;
  quorate: boolean;
  min_humans: number;
  attested_humans: number;
  agenda: { n: number; text: string; src: string }[];
  attendance: {
    name: string;
    attested: boolean;
    agent: string | null;
    note: string | null;
  }[];
  minutes: string[];
  decisions: { id: string; text: string; link: boolean }[];
  dissent: { who: string; text: string }[];
}

/**
 * The on-chain anchor state of a meeting's minutes.
 *
 * Four distinct answers, deliberately not collapsed: a chain that could not be
 * reached is NOT the same as a meeting that is not anchored, and a record whose
 * hash disagrees with ours is an integrity alarm, not an absence.
 */
export type AnchorState =
  | { state: "anchored"; block: number; ratifier: string; contract: string }
  | { state: "not-anchored"; contract: string; reason: string }
  | { state: "mismatch"; contract: string; on_chain: string }
  | { state: "unavailable"; reason: string }
  | { state: "unreachable"; contract: string; reason: string };

/** MeetingRegistry.verifyMinutes/getMinutes via eth_call. */
export function meetingAnchor(id: string): Promise<AnchorState> {
  return invoke<AnchorState>("meeting_anchor", { id });
}

export function meetingsList(): Promise<MeetingRowDto[]> {
  return invoke<MeetingRowDto[]>("meetings_list");
}
export function meetingGet(id: string): Promise<MeetingDetailDto> {
  return invoke<MeetingDetailDto>("meeting_get", { id });
}
export interface ScheduleMeetingInput {
  id: string;
  name: string;
  when: string;
  template: string;
  min_humans: number;
  classification: string;
  workspace?: string;
}
export function meetingSchedule(input: ScheduleMeetingInput): Promise<void> {
  return invoke<void>("meeting_schedule", { input });
}
export function meetingAdmit(input: {
  id: string;
  name: string;
  vendor?: string;
  attested: boolean;
  clearance?: string;
}): Promise<void> {
  return invoke<void>("meeting_admit", input);
}
export function meetingOpen(id: string): Promise<string> {
  return invoke<string>("meeting_open", { id });
}
export function meetingClose(id: string): Promise<string> {
  return invoke<string>("meeting_close", { id });
}
/** The hash the ceremony must display. Reads only — it signs nothing. */
export function meetingContentHash(id: string): Promise<string> {
  return invoke<string>("meeting_content_hash", { id });
}
/** Record a ratification the ceremony already took. */
export function meetingRatify(
  id: string,
  by: string,
  expectHash: string,
): Promise<DecisionResult> {
  return invoke<DecisionResult>("meeting_ratify", { id, by, expectHash });
}

// ---- journals + standup briefs (QRM-S5.7) --------------------------

export interface JournalEntryDto {
  id: string;
  date: string;
  who: string;
  kind: string;
  text: string;
  human: boolean | null;
}
export interface JournalListDto {
  entries: JournalEntryDto[];
  /** Rule 11: where these came from, or why there are none. */
  source: string;
}
export interface StandupBriefDto {
  agent: string;
  meeting: string;
  sections: [string, string][];
  source: string;
}

export function journalList(): Promise<JournalListDto> {
  return invoke<JournalListDto>("journal_list");
}
export function journalBrief(agent: string, meeting: string): Promise<StandupBriefDto> {
  return invoke<StandupBriefDto>("journal_brief", { agent, meeting });
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
