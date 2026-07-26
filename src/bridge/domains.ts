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
  Agent,
  Block,
  CalAccount,
  CalEvent,
  CorrelationEvent,
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
  LogLine,
  Meeting,
  MeetingAdmit,
  MeetingDetail,
  MeetingSchedule,
  NodePeer,
  Protocol,
  Repo,
  Room,
  RoomEvent,
  RosterMember,
  Session,
  Simulation,
  SpecClause,
  StandupBrief,
  Subscribe,
  TenancyNode,
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

export interface NodeDomain {
  peers(): Promise<NodePeer[]>;
  /** Live log stream. */
  logs: Subscribe<LogLine>;
  /** The most recent blocks below `height` (GhostDAG rows). */
  blocks(height: number): Block[];
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

export interface RoomsDomain {
  list(): Promise<Room[]>;
  /** The members of a room (humans + agents, cryptographic peers). */
  roster(roomId: string): Promise<RosterMember[]>;
  /** The live transcript stream for a room. */
  events: Subscribe<RoomEvent>;
  // TODO(wire): open/join/post/invite; stt.start() consent + local transcription.
}

export interface LedgerDomain {
  query(): Promise<Decision[]>;
  /** The streaming decision ribbon (new rows arrive over time). */
  stream: Subscribe<Decision>;
  decision(id: string): Promise<DecisionDetail>;
  correlation(id: string): Promise<CorrelationEvent[]>;
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
  tenancy(): Promise<TenancyNode[]>;
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
