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
/**
 * An agent in the fleet.
 *
 * The first block is what this tenant can KNOW from its own evidence — grants
 * it issued and decisions it recorded. Everything below is the `AgentSBT`
 * registry's to say (vendor, model, sandbox, capsules, reputation), and is
 * OPTIONAL because that registry is a chain read that has not landed. A surface
 * renders an absent field as "—" naming its source; it never invents one.
 */
export interface Agent {
  id: string;
  name: string;
  /** Live (unrevoked) capability grants. */
  grants: number;
  budgetUsed: number;
  budgetCap: number;
  /** Decisions recorded for this agent in this tenant. */
  decisions?: number;
  /** How many of those had no live grant behind them. */
  ungoverned?: number;

  // ---- from the AgentSBT registry (chain) — absent until it is wired ----
  vendor?: string;
  sbt?: string;
  hic?: number;
  lastActive?: string;
  disputeRate?: string;
  status?: AgentStatus;
  did?: string;
  pubkey?: string;
  org?: string;
  model?: string;
  lora?: string;
  transport?: "MCP" | "A2A" | "CLI";
  sandbox?: string;
  egress?: string;
  capsules?: Capsule[];
  reputation?: AgentReputation;
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
  /** A revoked grant stays in the record — it is history, not an absence. */
  revoked?: boolean;
}

/** The terms of a capability grant an operator is about to issue. */
export interface GrantTerms {
  id: string;
  agent: string;
  principal: string;
  tenantScope: string;
  actionClasses: string[];
  classificationCeiling: Classification;
  budgetUnits: number;
  expiresAtMs: number;
  /** "1" | "2" | "3" — the HIC level the grant confers. */
  hic: string;
}

// ---- rooms ----------------------------------------------------------
/**
 * A room is an MLS group on the citrate-comms relay. The relay carries
 * ciphertext and routing metadata; it holds no group secret and can decrypt
 * nothing — proved, not asserted, by `tests/server_blindness.rs`.
 */
export interface Room {
  id: string;
  name: string;
  classification: Classification | string;
  live: boolean;
  members: number;
  started: string | null;
}
/** What this app's relay connection is doing. */
export interface RoomsStatus {
  connected: boolean;
  relayUrl: string;
  relayDomain: string;
  /** The operator seat's relay address, once connected. */
  address: string | null;
  seats: number;
  rooms: number;
  /** Rule 11 + Rule 1: what this subsystem is, and what it does not keep. */
  note: string;
}
/**
 * One decrypted transcript line.
 *
 * `kind` is `text` or `system` — there is deliberately no `speech`: this app has
 * no audio path, so nothing here can have been spoken.
 */
export interface RoomEvent {
  /** Monotonic cursor within this session's transcript. */
  n: number;
  room: string;
  kind: "text" | "system";
  who: string;
  human: boolean;
  text: string;
  t: string;
}
/**
 * A seat in a room. `address` is a RELAY identity — not the chain identity that
 * ratifies minutes, and the surface must not imply otherwise. An agent's seat key
 * is held by this app on its behalf; the agent process holds nothing.
 */
export interface RosterMember {
  id: string;
  name: string;
  human: boolean;
  address: string;
  /** MLS signature public key, truncated: what actually distinguishes two seats. */
  mlsKey: string;
}
/** What opening a room needs. */
export interface RoomOpen {
  name: string;
  classification: Classification;
  /** Agent ids to admit — each gets its own app-held seat. */
  agents: string[];
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
/**
 * A meeting template. `minHumans` is the quorum rule the backend enforces at
 * close — it is data, not a caption, so the chips a user picks from and the
 * rule that decides quorate/inquorate cannot drift apart.
 */
export interface MeetingTemplate {
  name: string;
  minHumans: number;
  /** The classification this template defaults to. */
  classification: Classification;
  /** What the template means, for the operator choosing it. */
  note: string;
}

export interface MeetingSchedule {
  id: string;
  name: string;
  /** RFC3339. The backend stores it verbatim and never parses it. */
  when: string;
  template: string;
  minHumans: number;
  classification: Classification;
  /**
   * Directory whose `.agentile/sprints/active/{'*'}/SCOPE.md` files become the
   * agenda. Omitted means an empty agenda that says it was not generated —
   * never invented items.
   */
  workspace?: string;
}

export interface MeetingAdmit {
  id: string;
  name: string;
  /** Set for an agent; omitted for a human. Only humans count toward quorum. */
  vendor?: string;
  attested: boolean;
  /** Unknown clearance fails closed to Public (MR-4). */
  clearance?: Classification;
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
/**
 * `deny` = the rules said no; `rejected` = the human said no.
 * `allow` = the rules said yes; `approved` = the human said yes.
 * Keeping each pair distinct is what lets an auditor see who actually decided.
 */
export type Verdict =
  | "allow"
  | "require-approval"
  | "deny"
  | "ungoverned"
  | "rejected"
  | "approved";
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
/**
 * One decision, as a document.
 *
 * Every field is read back out of the tenant's evidence chain. The three hashes
 * are what make the document checkable by someone who does not trust the app
 * rendering it: `contentHash` is the record's own hash (the Merkle leaf),
 * `entryHash` is `BLAKE3(prev_head ++ record)` (its position in the chain), and
 * `merkleRoot` is what an anchor commits to. `included` is the result of
 * actually recomputing the root from the leaf and its proof — not a claim.
 */
export interface DecisionDetail {
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
  chainPos: string;
  entryHash: string;
  contentHash: string;
  chainHead: string;
  merkleRoot: string;
  proofLen: number;
  included: boolean;
  /** Rule 11: where this document was read from. */
  source: string;
}

/** What the Verify affordance actually checked, when it was clicked. */
export interface VerifyDecision {
  /** The whole chain replayed from genesis and every stored hash matched. */
  chainIntact: boolean;
  /** This record's content hash + its proof recomputed the Merkle root. */
  included: boolean;
  records: number;
  entryHash: string;
  merkleRoot: string;
  proofLen: number;
}

export interface CorrelationEvent {
  t: string;
  kind: "meeting" | "grant" | "action" | "pr";
  text: string;
  link: string;
}
/** A correlation timeline plus what was searched — and what was not. */
export interface CorrelationView {
  events: CorrelationEvent[];
  source: string;
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
/**
 * One balance. `balance` is a decimal string in whole units, computed exactly —
 * a balance is money, and no float ever touches it.
 */
export interface Token {
  symbol: string;
  name: string;
  balance: string;
  native: boolean;
  /** Rule 11: the exact call this number came from. */
  source: string;
}
/**
 * The signing identity and what it holds on chain.
 *
 * There is deliberately no movement history: this app runs no transaction index
 * and 40204's RPC cannot enumerate an address's past. `activityNote` says that
 * out loud rather than letting an empty table imply "no activity".
 */
export interface Wallet {
  address: string;
  chainId: number;
  rpcUrl: string;
  keyStore: string;
  source: string;
  tokens: Token[];
  /** Reads that did NOT succeed, named. Never rendered as a zero balance. */
  notes: string[];
  activityNote: string;
}

// ---- node -----------------------------------------------------------
/** Where the chain is, from the endpoint the address book names. */
export interface NodeStatus {
  rpcUrl: string;
  book: string;
  chainId: number;
  height: number;
  /** `net_peerCount` from the endpoint. `null` = it did not answer — not zero. */
  peers: number | null;
  client: string | null;
  /** `false` = fully synced. `null` = the node did not answer. */
  syncing: boolean | null;
  latencyMs: number;
  baseFeeWei: string | null;
  blueScore: number | null;
}
/**
 * One line of what this app did against the chain.
 *
 * NOT a node log: citrate-quorum supervises no node, so it has none to stream.
 * This is its own RPC record — method, endpoint, outcome, measured round trip.
 */
export interface ActivityLine {
  t: string;
  lvl: "INFO" | "WARN" | "DEBUG" | "ERROR";
  module: string;
  msg: string;
}
/** A block, carrying only fields chain 40204's RPC actually returns. */
export interface Block {
  height: number;
  hash: string;
  txs: number;
  proposer: string;
  gasUsed: number;
  gasLimit: number;
  /** Epoch seconds. */
  timestamp: number;
  /** GhostDAG blue score, when the node reports one. */
  blueScore: number | null;
  /** How many merge parents this block absorbed. */
  mergeParents: number;
}

// ---- settings -------------------------------------------------------
export interface TenancyNode {
  depth: number;
  name: string;
  admins: string;
  ceiling: Classification | string;
  threshold: string;
  /** The node's on-chain id. */
  id: string;
}
/**
 * A principal's clearance, as the chain has it.
 *
 * `recorded: false` with `effective: "Public"` means **nobody has said** — not
 * that they are cleared to Public. The registry's own `getClearance` collapses
 * those two; this does not, because they need different fixes.
 */
export interface ClearanceView {
  effective: string;
  recorded: boolean;
  foreignNational: boolean | null;
  tenantCeiling: string | null;
  /** The least of the axes that answered — what a room is actually bounded by. */
  boundedTo: string;
  /** The bytes32 key the registry was asked about, so an HR oracle can match it. */
  subject: string;
  source: string;
  note: string | null;
}

/** The tenant tree, plus where it came from — or why it is empty. */
export interface TenancyView {
  rows: TenancyNode[];
  source: string;
  /** Set when the tree is empty for a reason an operator must act on. */
  note: string | null;
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
  verdict: Verdict;
  hic: HicLevel;
  grantId: string | null;
  reason: string;
  /** The tenant's evidence-chain head after this decision was appended. */
  chainHead: string;
  ungoverned: boolean;
}

/**
 * An escalation waiting on a human: an agent asked to do something at HIC-1 and
 * stopped. It is already recorded — approving or rejecting it answers a question
 * that has been asked, rather than proposing a new action.
 */
export interface PendingApproval {
  /** The escalated decision's index in the tenant's chain. */
  decision: number;
  agent: string;
  principal: string | null;
  actionClass: string;
  classification: Classification;
  cost: number;
  correlationId: string;
  requestedAtMs: number;
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
  /**
   * A chain transaction this approval also authorises, run through the KIT's
   * signing path after `commit` succeeds.
   *
   * `prepare` returns the id of a PENDING kit ceremony (built by a Rust command
   * that assembles the tx and signs nothing). The ceremony then calls
   * `sign_and_broadcast` on that id — the only command that signs, gated on an
   * unlocked vault, single-use, and refusing any tx whose `from` is not the
   * vault's own address.
   *
   * A FAILED chain write does not undo `commit`: the governance act really
   * happened locally. It is reported as its own outcome instead of being folded
   * into success or into failure.
   */
  chainTx?: {
    prepare: () => Promise<{ id: string; rawUnverified?: boolean }>;
    label: string;
  };
  /**
   * The write this signature authorises, run BY the ceremony.
   *
   * Callers used to await `request()` and then perform the write themselves.
   * But `request()` resolves when the operator DISMISSES the dialog, and the
   * dialog reaches "On record" well before that — so the ceremony announced a
   * record that did not exist yet, and a write that then failed left the
   * operator believing it had succeeded. One caller even swallowed the error.
   *
   * Supplying `commit` closes that window: the ceremony awaits it between
   * signing and settling, a rejection returns the dialog to review with the
   * reason shown, and only a successful commit is allowed to say "on record".
   * Return a string to become the settled note (e.g. a chain head).
   *
   * This mirrors what answering an escalation already does — see `onSign`.
   */
  commit?: () => Promise<string | void>;
  /**
   * A signature this approval produces, and what to do with it.
   *
   * `prepare` returns the id of a PENDING kit ceremony staged by a Rust command
   * that signed nothing. The ceremony then calls `sign_approve` on that id — the
   * only path that signs — and hands the hex to `apply`.
   *
   * Used by the relay login: the operator signs an EIP-191 SIWE message with the
   * vault key, so their room seat IS their wallet address. One approval per
   * session; every message after it is signed by the MLS member key.
   */
  signature?: {
    prepare: () => Promise<{ id: string; rawUnverified?: boolean }>;
    apply: (sigHex: string) => Promise<string | void>;
    label: string;
  };
}

/** A stream subscription: register a listener, get an unsubscribe fn (§6.4). */
export type Unsubscribe = () => void;
export type Subscribe<E> = (onEvent: (e: E) => void) => Unsubscribe;
