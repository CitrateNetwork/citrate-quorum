// =====================================================================
// citrate-quorum — bridge domain contracts (QRM-S2D · S2D.3 freeze target)
//
// One typed interface per domain. Every method returns a Promise; streams
// use the Subscribe<E> shape. The sim adapter (bridge/sim) backs these with
// scripted prototype data; the tauri adapter (bridge/tauri) backs them with
// real Rust commands. SURFACES DEPEND ONLY ON THIS FILE — flipping a domain
// sim→live changes one adapter, zero surfaces.
//
// This is the interface our Rust is written against. Changing it after the
// S2D.3 freeze costs backend rework — owner decision, not a PR comment.
// =====================================================================
import type {
  ActivityLine,
  Agent,
  Block,
  CalAccount,
  CalEvent,
  CorrelationView,
  Decision,
  DecisionDetail,
  GateDecision,
  GovernedAction,
  Grant,
  GrantTerms,
  PendingApproval,
  IngestFile,
  InterviewTurn,
  JournalEntry,
  Meeting,
  MeetingAdmit,
  MeetingDetail,
  MeetingSchedule,
  NodeStatus,
  Protocol,
  Repo,
  Room,
  RoomEvent,
  RoomOpen,
  RoomsStatus,
  RosterMember,
  Session,
  Simulation,
  SpecClause,
  StandupBrief,
  Subscribe,
  TenancyView,
  VerifyDecision,
  Wallet,
} from "./types";

export interface SessionDomain {
  current(): Promise<Session>;
  /** The active tenant scope, or null if none is established yet. */
  activeTenant(): Promise<string | null>;
  /** Establish the tenant scope everything else is keyed by (Rule 6). */
  setActiveTenant(tenant: string): Promise<void>;
  /**
   * The human at the keyboard. Approvals are recorded against them, so until
   * one is named nothing can be approved — which is why the session flow asks
   * when the identity provider cannot say.
   */
  operator(): Promise<string | null>;
  setOperator(name: string): Promise<void>;
}

/**
 * The policy gate — the seam through which every governed action passes before
 * it executes or is signed (I-3/I-4).
 *
 * `evaluate` is not a preview: it evaluates the action against the tenant's live
 * grants AND appends the resulting decision to the tenant's evidence chain. An
 * action with no covering grant comes back `ungoverned` — recorded and surfaced,
 * never silently allowed and never silently dropped (Rule 5).
 */
export interface PolicyDomain {
  evaluate(action: GovernedAction): Promise<GateDecision>;
  /**
   * The human refused. Refunds what the decision charged its grant — exactly
   * once — and records the refusal as its own decision, because "the person
   * said no" is evidence in a way that a missing record is not.
   */
  reject(decisionId: number): Promise<GateDecision>;
  /**
   * The human approved an escalation. Records who approved it; the charge stays
   * spent, because the action now goes ahead.
   */
  approve(decisionId: number, approver: string): Promise<GateDecision>;
  /** Escalations still waiting on a human — the ceremony queue's contents. */
  pending(): Promise<PendingApproval[]>;
}

export interface WalletDomain {
  summary(): Promise<Wallet>;
  /** The signing identity's public status. Never carries secret material. */
  identity(): Promise<{ exists: boolean; address: string | null; reason: string | null }>;
  /**
   * Create the signing identity and return its recovery phrase ONCE.
   *
   * The only method on this contract that returns secret material, under an
   * explicit owner decision. There is no method that reads it back. Callers
   * must display-and-drop: never persist it, never log it, never lift it out
   * of component state.
   */
  createIdentity(): Promise<{ address: string; mnemonic: string }>;
  /** Adopt an existing identity from a phrase. Returns the address only. */
  importIdentity(mnemonic: string): Promise<{ exists: boolean; address: string | null; reason: string | null }>;
}

/**
 * The chain this app reads.
 *
 * **Contract change, QRM Phase 0.** This domain was frozen at S2D.3 as
 * `peers(): Promise<NodePeer[]>`, `logs: Subscribe<LogLine>` and a SYNCHRONOUS
 * `blocks(height): Block[]`. None of the three survived contact with a real
 * chain, and the shape was changed rather than faked:
 *
 * - `blocks` cannot be synchronous — every row is an `eth_getBlockByNumber`.
 * - There is no peer LIST to return: `net_peerCount` gives a count, and 40204's
 *   public RPC exposes no peer enumeration. A list of invented peer rows is
 *   exactly the defect this repo keeps finding, so the count moved into
 *   `status()` and `peers()` is gone.
 * - There are no node logs to stream: this app supervises no node. `activity()`
 *   returns the app's OWN record of every RPC it issued, which is a real thing
 *   an operator can use to tell "the chain is down" from "it never asked".
 */
export interface NodeDomain {
  /** Height, peer count, client, sync state, measured latency. */
  status(): Promise<NodeStatus>;
  /** The most recent blocks, newest first (the backend clamps `count`). */
  blocks(count: number): Promise<Block[]>;
  /** What this app has asked the chain, newest first. */
  activity(): Promise<ActivityLine[]>;
}

export interface AgentsDomain {
  list(): Promise<Agent[]>;
  grants(agentId: string): Promise<Grant[]>;
  /**
   * Issue a capability grant. L-1 makes this an HIC-1 act, so callers route it
   * through the ceremony first and only commit on approval. `issuedBy` names
   * the human — a grant that appeared from nowhere is what an auditor hunts for.
   */
  issue(terms: GrantTerms): Promise<void>;
  /** Revoke a grant immediately (CG-2), naming who revoked it. */
  revoke(agent: string, grantId: string, revokedBy: string): Promise<boolean>;
}

/**
 * Rooms — a real MLS group on the citrate-comms relay (QRM-S3).
 *
 * **Contract change from the S2D.3 freeze.** The frozen shape was
 * `list/roster/events` with `events` as a push stream. It is now a polled
 * `events(since)` for the same reason the ledger ribbon polls — no event
 * plumbing, and the relay is not a high-rate source — and it gained the
 * operations a room actually needs: connect, open, say, leave.
 */
export interface RoomsDomain {
  /** What this app's relay connection is doing. Never dials by itself. */
  status(): Promise<RoomsStatus>;
  /** Connect the operator's seat. Idempotent. */
  connect(operator: string): Promise<RoomsStatus>;
  list(): Promise<Room[]>;
  /** Open a room, admitting an app-held seat for each named agent. */
  open(input: RoomOpen, operator: string): Promise<Room>;
  /** The members of a room — humans and agents, cryptographic peers. */
  roster(roomId: string): Promise<RosterMember[]>;
  /** Send, as `principal`'s seat. */
  say(roomId: string, principal: string, text: string): Promise<void>;
  /** Drain the relay and return the transcript from cursor `since`. */
  events(since: number): Promise<RoomEvent[]>;
  leave(roomId: string): Promise<void>;
}

/** The evidence chain's own state — head, root, counts, integrity. */
export interface LedgerState {
  head: string;
  merkleRoot: string;
  records: number;
  ungoverned: number;
  intact: boolean;
  tenant: string;
}

export interface LedgerDomain {
  query(): Promise<Decision[]>;
  /** All five facts in one read, so they describe the same chain at once. */
  state(): Promise<LedgerState>;
  /** The streaming decision ribbon (new rows arrive over time). */
  stream: Subscribe<Decision>;
  decision(id: string): Promise<DecisionDetail>;
  /**
   * Re-verify one decision on demand: replay the chain from genesis AND
   * recompute the Merkle root from this record plus its inclusion proof. Both
   * are local checks over evidence we hold — neither says anything about the
   * chain, which is reported separately, because "my copy is intact" and "the
   * world agrees with my copy" are different claims.
   */
  verifyDecision(id: string): Promise<VerifyDecision>;
  correlation(id: string): Promise<CorrelationView>;
  // TODO(wire): asOf(ts) time-travel snapshot pagination; exportPack(range).
}

export interface MeetingsDomain {
  list(): Promise<Meeting[]>;
  get(id: string): Promise<MeetingDetail>;
  /**
   * The content hash a ratifier signs. Read this BEFORE opening the ceremony,
   * display it, and hand the same value to `ratify` — the backend re-checks it
   * so a signature is never recorded over minutes that moved in between.
   */
  contentHash(id: string): Promise<string>;
  /** Record a ratification the ceremony has already taken. Signs nothing. */
  ratify(id: string, by: string, expectHash: string): Promise<void>;
  /** Create a meeting, generating its agenda from `workspace`'s sprint files. */
  schedule(input: MeetingSchedule): Promise<void>;
  /**
   * Admit an attendee. Rejects when their clearance is below the meeting's
   * classification — admitting them would reclassify what has been discussed
   * (MR-4). The caller must surface that refusal, not swallow it.
   */
  admit(input: MeetingAdmit): Promise<void>;
  /**
   * Whether the chain holds this meeting's minutes hash. A read — it neither
   * signs nor sends. Returns a human-readable line for the anchor row.
   */
  anchorState(id: string): Promise<string>;
  /** Open the meeting: freezes the agenda and returns its hash. */
  open(id: string): Promise<string>;
  /** Close it, composing the minutes. Returns the resulting state. */
  close(id: string): Promise<string>;
}

export interface GovernanceDomain {
  protocols(): Promise<Protocol[]>;
  clauses(): Promise<SpecClause[]>;
  simulate(): Promise<Simulation>;
  ingest(): Promise<IngestFile[]>;
  interview(): Promise<InterviewTurn[]>;
  // TODO(wire): deploy(specId) routes through the ceremony; interview streaming.
}

export interface JournalDomain {
  list(): Promise<JournalEntry[]>;
  brief(agentId: string, meetingId: string): Promise<StandupBrief>;
}

export interface CalendarDomain {
  accounts(): Promise<CalAccount[]>;
  events(): Promise<CalEvent[]>;
  // TODO(wire): connect(provider) OAuth loopback; syncStatus() truthful badges.
}

export interface ReposDomain {
  list(): Promise<Repo[]>;
  prs(): Promise<import("./types").Pr[]>;
  peek(ref: string): Promise<import("./types").Peek>;
}

export interface SettingsDomain {
  /** The on-chain tenant tree, plus where it came from or why it is empty. */
  tenancy(): Promise<TenancyView>;
  // TODO(wire): identity federation status; models; license.seats metering source.
}

/** The whole bridge — one field per domain. */
export interface BridgeContract {
  session: SessionDomain;
  policy: PolicyDomain;
  wallet: WalletDomain;
  node: NodeDomain;
  agents: AgentsDomain;
  rooms: RoomsDomain;
  ledger: LedgerDomain;
  meetings: MeetingsDomain;
  governance: GovernanceDomain;
  journal: JournalDomain;
  calendar: CalendarDomain;
  repos: ReposDomain;
  settings: SettingsDomain;
}
