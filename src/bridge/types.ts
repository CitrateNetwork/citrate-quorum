// =====================================================================
// citrate-quorum — bridge data types (QRM-S2D)
//
// The shapes every surface renders. Ported from the design prototype's
// `// TYPE:` annotations in quorum-sim.js. Our Rust (bridge/tauri) is
// written against THIS contract — see the tauri adapter's Unavailable
// stubs. Do not use `any`; an open question is marked `// TODO(wire):`.
// =====================================================================

/** The typed errors a domain method rejects with (§6.2 of the design brief). */
export class Unavailable extends Error {
  readonly kind = "unavailable" as const;
  constructor(op: string) {
    super(`citrate-quorum: "${op}" is not wired yet (Tauri adapter stub)`);
  }
}
export class Denied extends Error {
  readonly kind = "denied" as const;
  constructor(
    public readonly reason: string,
    public readonly grantedBy?: string,
  ) {
    super(`denied: ${reason}`);
  }
}
export class Failed extends Error {
  readonly kind = "failed" as const;
  constructor(public readonly source: string, message: string) {
    super(`${source}: ${message}`);
  }
}

// ---- session / identity --------------------------------------------
export type Classification = "Public" | "Proprietary" | "CUI" | "ITAR";
export type Connectivity = "live" | "degraded" | "offline";

export interface SessionUser {
  name: string;
  initials: string;
  role: string;
  sbt: string;
  clearance: Classification;
  did: string;
}
export interface HicState {
  level: number;
  label: string;
  desc: string;
  setBy: string;
}
export interface ChainInfo {
  id: number;
  height: number;
  relay: string;
  anchorRoot: string;
}
export interface Session {
  user: SessionUser;
  /** The TenantHierarchy path root→leaf. */
  tenant: string[];
  hic: HicState;
  chain: ChainInfo;
}

export interface Vendor {
  name: string;
  color: string;
}

// ---- agents ---------------------------------------------------------
export type AgentStatus = "active" | "probation" | "quarantined";
export interface Capsule {
  name: string;
  hash: string;
  verified: boolean;
}
/** A measured reputation metric: [value, denominator/explanation]. */
export type RepMetric = [string, string];
export interface AgentReputation {
  dispute: RepMetric;
  contradiction: RepMetric;
  escalation: RepMetric;
  budget: RepMetric;
  grader: RepMetric;
}
export interface Agent {
  id: string;
  name: string;
  vendor: string;
  sbt: string;
  hic: number;
  grants: number;
  budgetUsed: number;
  budgetCap: number;
  lastActive: string;
  disputeRate: string;
  status: AgentStatus;
  did: string;
  pubkey: string;
  org: string;
  model: string;
  lora: string;
  transport: "MCP" | "A2A" | "CLI";
  sandbox: string;
  egress: string;
  capsules: Capsule[];
  reputation: AgentReputation;
  quarantine?: string;
}
export interface Grant {
  id: string;
  classes: string;
  scope: string;
  budget: string;
  expiry: string;
  hic: number;
  principal: string;
}

// ---- rooms ----------------------------------------------------------
export interface Room {
  id: string;
  name: string;
  classification: Classification;
  live: boolean;
  members: number;
  started: string | null;
}
/** RoomEvent kinds drive distinct transcript renderings (design brief §4.2). */
export type RoomEventKind =
  | "speech"
  | "text"
  | "tool"
  | "system"
  | "contradiction"
  | "vote"
  | "vote-cast"
  | "vote-close";
export interface RoomEvent {
  t: number;
  kind: RoomEventKind;
  who?: string;
  human?: boolean;
  text?: string;
  meta?: string;
  cites?: string[];
  tool?: string;
  verdict?: "allow" | "require-approval" | "deny";
  dur?: string;
  params?: string;
  result?: string;
  escalate?: boolean;
  a?: string;
  b?: string;
  fact?: string;
  va?: string;
  vb?: string;
  note?: string;
  choice?: "For" | "Against" | "Abstain";
  weight?: number;
  proof?: string;
}
export interface RosterMember {
  id: string;
  name: string;
  human?: boolean;
  role?: string;
  clearance?: Classification;
  vendor?: string;
  sbt?: string;
  hic?: number | null;
  speaking?: boolean;
}

// ---- meetings -------------------------------------------------------
export type MeetingState =
  | "scheduled"
  | "in-progress"
  | "awaiting"
  | "ratified"
  | "inquorate";
export interface Meeting {
  id: string;
  name: string;
  when: string;
  tpl: string;
  humans: number;
  agents: number;
  classification: Classification;
  state: MeetingState;
}
export interface MeetingDetail {
  id: string;
  name: string;
  when: string;
  tenant: string;
  classification: Classification;
  agendaHash: string;
  ratified: boolean;
  ratifiedBy?: string;
  ratifiedAt?: string;
  anchor?: string;
  agenda: { n: number; text: string; src: string }[];
  attendance: {
    name: string;
    attested: boolean;
    agent?: string;
    note?: string;
  }[];
  minutes: string[];
  decisions: { id: string; text: string; link: boolean }[];
  dissent: { who: string; text: string }[];
}

// ---- governance -----------------------------------------------------
export type ProtocolState = "live" | "deprecated";
export interface Protocol {
  id: string;
  name: string;
  version: string;
  template: string;
  audit: string;
  addr: string;
  state: ProtocolState;
  governs: string;
  deployed: string;
}
export interface SpecClause {
  n: string;
  en: string;
  gh: string;
  tpl: string | null;
  ok: boolean;
  why?: string;
}
export interface Simulation {
  range: string;
  blocked: number;
  approvals: number;
  allowed: number;
  byClass: [string, number, number][];
  byTeam: [string, number][];
  samples: {
    id: string;
    agent: string;
    action: string;
    was: string;
    would: string;
    why: string;
  }[];
  inconvenienced: [string, number, string][];
  create2: string;
}
export interface IngestFile {
  name: string;
  size: string;
  status: string;
  class: Classification;
  note: string;
  prov: string;
}
export interface InterviewTurn {
  q: string;
  a: string;
}

// ---- ledger ---------------------------------------------------------
/** `deny` = the rules said no. `rejected` = the human said no. Different facts. */
export type Verdict = "allow" | "require-approval" | "deny" | "ungoverned" | "rejected";
export interface Decision {
  id: string;
  time: string;
  principal: string;
  agent: string;
  cls: string;
  verdict: Verdict;
  hic: string;
  corr: string;
}
export interface DecisionDetail {
  id: string;
  what: string;
  when: string;
  principal: string;
  agent: string;
  grant: string;
  protocol: string;
  verdict: string;
  reason: string;
  model: string;
  params: string;
  chainPos: string;
  anchor: string;
  proof: string;
}
export interface CorrelationEvent {
  t: string;
  kind: "meeting" | "grant" | "action" | "pr";
  text: string;
  link: string;
}

// ---- journal --------------------------------------------------------
export interface JournalEntry {
  id: string;
  date: string;
  who: string;
  human?: boolean;
  kind: "note" | "journal" | "retro";
  text: string;
}
export interface StandupBrief {
  agent: string;
  meeting: string;
  sections: [string, string][];
}

// ---- calendar -------------------------------------------------------
export interface CalAccount {
  name: string;
  mode: "two-way" | "read-only";
  last: string;
  ok: boolean;
}
export interface CalEvent {
  d: number;
  name: string;
  gov: boolean;
  cls?: Classification;
  time: string;
}

// ---- repos ----------------------------------------------------------
export interface Repo {
  id: string;
  name: string;
  branches: number | null;
  prs: number | null;
  checks: string | null;
  agents: string | null;
  last: string | null;
  denied?: Classification;
  deniedWho?: string;
}
export interface Pr {
  id: string;
  repo: string;
  title: string;
  by: string;
  agent: boolean;
  checks: string;
  age: string;
}
export interface Peek {
  ref: string;
  hash: string;
  lines: [number, string][];
}

// ---- wallet ---------------------------------------------------------
export type TxDir = "in" | "out" | "stake" | "gas";
export interface Token {
  sym: string;
  name: string;
  balance: string;
  fiat: string;
  native?: boolean;
}
export interface Tx {
  hash: string;
  dir: TxDir;
  kind: string;
  counterparty: string;
  amount: string;
  token: string;
  time: string;
  status: "settled" | "rejected" | "pending";
  decision?: string;
}
export interface Wallet {
  address: string;
  keyStore: string;
  tokens: Token[];
  txs: Tx[];
  staking: Record<string, string>;
  contacts: { name: string; addr: string; sbt: string }[];
}

// ---- node -----------------------------------------------------------
export interface NodePeer {
  id: string;
  kind: string;
  latency: string;
  dir: string;
  ok: boolean;
}
export interface LogLine {
  t: string;
  lvl: "INFO" | "WARN" | "DEBUG" | "ERROR";
  mod: string;
  msg: string;
}
export interface Block {
  height: number;
  hash: string;
  txs: number;
  blue: boolean;
  proposer: string;
  gas: string;
  age: string;
  checkpoint: boolean;
}

// ---- settings -------------------------------------------------------
export interface TenancyNode {
  depth: number;
  name: string;
  admins: string;
  ceiling: Classification;
  threshold: string;
}

// ---- ceremony (the one signing component, design brief §3.3) --------
export type CeremonyPhase =
  | "review"
  | "signing"
  | "broadcasting"
  | "settled"
  | "rejected";
export type IntentKind =
  | "raw"
  | "ratify"
  | "deploy"
  | "grant"
  | "revoke"
  | "transfer"
  | "stake";
/** The HIC level in force for a decision. "X" is ungoverned. */
export type HicLevel = "0" | "1" | "2" | "3" | "X";

/**
 * A proposed governed action — the input to the policy gate.
 *
 * There is deliberately no tenant field: the tenant scope is backend-owned, and
 * the frontend cannot name the chain it writes to (Rule 6).
 */
export interface GovernedAction {
  /** Action class, e.g. `repo.write`, `spend`, `grant.revoke`, `protocol.deploy`. */
  actionClass: string;
  classification: Classification;
  /** The acting agent's id — or the principal's, when a human acts directly. */
  agent: string;
  principal?: string;
  /** Cost in the action's own units (SALT for spend, 0 for most). */
  cost?: number;
  /** Above this cost the action escalates to HIC-1. 0 disables the threshold. */
  hic1CostThreshold?: number;
  /** Force HIC-1 regardless of grant or cost (chain state, money, keys, grants). */
  mandatoryHic1?: boolean;
  correlationId?: string;
  modelId?: string;
}

/**
 * What the policy gate actually recorded. Every field is read back from the
 * evidence chain the decision was appended to — none of it is inferred by the UI.
 */
export interface GateDecision {
  /** This decision's index in the tenant's chain — the handle `policy.reject`
   *  refers back to when the human refuses. */
  decisionId: number;
  verdict: "allow" | "require-approval" | "deny" | "ungoverned" | "rejected";
  hic: HicLevel;
  grantId: string | null;
  reason: string;
  /** The tenant's evidence-chain head after this decision was appended. */
  chainHead: string;
  ungoverned: boolean;
}

export interface SignatureIntent {
  kind: IntentKind;
  title: string;
  origin: string;
  /**
   * The governed action this signature enacts. Required: the ceremony runs the
   * policy gate on it *before* opening (I-3/I-4), so an intent that cannot say
   * what it does cannot be signed.
   */
  action: GovernedAction;
  rows: { k: string; v: string }[];
  /** Present for a protocol deploy — shown BEFORE signing (GF-1). */
  create2?: string;
  /** Multisig: the required signers + threshold. */
  signers?: { name: string; signed: boolean }[];
  threshold?: number;
  /** Undecodable calldata forces an explicit ack before Sign enables. */
  rawUnverified?: boolean;
  cost?: string;
}

/** A stream subscription: register a listener, get an unsubscribe fn (§6.4). */
export type Unsubscribe = () => void;
export type Subscribe<E> = (onEvent: (e: E) => void) => Unsubscribe;
