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
  Grant,
  IngestFile,
  InterviewTurn,
  JournalEntry,
  LogLine,
  Meeting,
  MeetingDetail,
  NodePeer,
  Protocol,
  Repo,
  Room,
  RoomEvent,
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
}

export interface WalletDomain {
  summary(): Promise<Wallet>;
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
  // TODO(wire): issueGrant / revoke route through the ceremony (signing.request).
}

export interface RoomsDomain {
  list(): Promise<Room[]>;
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
  // TODO(wire): ratify(id) routes through the ceremony; scheduling form.
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
