// =====================================================================
// citrate-quorum — sim data (QRM-S2D)
//
// ALL prototype data. Ported 1:1 from the design prototype's quorum-sim.js.
// This is the ONLY place prototype data lives (design brief §6.2 rule 3):
// deleting src/bridge/sim/ must leave a compiling app with honest empty
// states everywhere. Nothing here is real — it is scripted for the demo.
// =====================================================================
import type {
  CompileResult,
  SpecSummary,
  ActivityLine,
  Agent,
  Block,
  CalAccount,
  CalEvent,
  CorrelationView,
  Decision,
  DecisionDetail,
  Grant,
  IngestFile,
  InterviewTurn,
  JournalEntry,
  Meeting,
  MeetingDetail,
  NodeStatus,
  Protocol,
  Peek,
  Pr,
  Repo,
  Room,
  RoomEvent,
  RoomsStatus,
  RosterMember,
  Session,
  Simulation,
  SpecClause,
  StandupBrief,
  TenancyView,
  VerifyDecision,
  Wallet,
} from "../types";
import type { LedgerState } from "../domains";

export const SESSION: Session = {
  user: { name: "Rachel Ortiz", initials: "RO", role: "Chief AI Officer", sbt: "HumanSBT #12", clearance: "CUI", did: "did:citrate:h:0x9f2e…c771" },
  tenant: ["Meridian Aero", "Aerostructures", "Wichita", "Line-4 Automation"],
  hic: { level: 2, label: "HIC-2", desc: "Budgeted autonomy — agents act within granted envelopes; spends and writes above threshold require a ceremony.", setBy: "Protocol PRT-004 · ratified 2026-06-30" },
  chain: { id: 40204, height: 1284067, relay: "relay-wichita-2", anchorRoot: "0x7c1fa2…90de" },
};

export { VENDORS } from "../../theme/vendors";

export const AGENTS: Agent[] = [
  { id: "claude-code", name: "claude-code", vendor: "anthropic", sbt: "AgentSBT #41", hic: 2, grants: 4, budgetUsed: 312, budgetCap: 500, lastActive: "2m ago", disputeRate: "0.4% / 1,204", status: "active", did: "did:citrate:a:0x41aa…19be", pubkey: "0x04c1…88f2", org: "Line-4 Automation", model: "claude-sonnet-4-5", lora: "gov-lora v2.3", transport: "MCP", sandbox: "firecracker · healthy", egress: "PRT-004 allowlist", capsules: [{ name: "repo-surgeon", hash: "b3:9c41…e0a2", verified: true }, { name: "test-author", hash: "b3:77d0…14cc", verified: true }], reputation: { dispute: ["0.4%", "5 / 1,204 actions"], contradiction: ["0.2%", "2 / 890 claims"], escalation: ["3.1%", "37 / 1,204"], budget: ["98.7%", "within envelope"], grader: ["0.91", "412 graded claims"] } },
  { id: "codex", name: "codex", vendor: "openai", sbt: "AgentSBT #38", hic: 2, grants: 3, budgetUsed: 448, budgetCap: 500, lastActive: "11s ago", disputeRate: "0.9% / 861", status: "active", did: "did:citrate:a:0x38bc…77d1", pubkey: "0x04aa…31e9", org: "Line-4 Automation", model: "gpt-5.2-codex", lora: "—", transport: "MCP", sandbox: "firecracker · healthy", egress: "PRT-004 allowlist", capsules: [{ name: "ci-pilot", hash: "b3:2b90…aa17", verified: true }], reputation: { dispute: ["0.9%", "8 / 861"], contradiction: ["0.6%", "4 / 644"], escalation: ["2.2%", "19 / 861"], budget: ["89.6%", "burn warning at 90%"], grader: ["0.87", "301 graded"] } },
  { id: "devin", name: "devin", vendor: "cognition", sbt: "AgentSBT #52", hic: 1, grants: 2, budgetUsed: 96, budgetCap: 250, lastActive: "4m ago", disputeRate: "0.0% / 203", status: "probation", did: "did:citrate:a:0x52fe…0ac3", pubkey: "0x04d7…b220", org: "Wichita", model: "devin-2", lora: "—", transport: "A2A", sandbox: "vendor-hosted · attested", egress: "PRT-004 allowlist", capsules: [{ name: "migration-runner", hash: "b3:e1c8…9034", verified: true }], reputation: { dispute: ["0.0%", "0 / 203"], contradiction: ["0.0%", "0 / 118"], escalation: ["8.9%", "18 / 203 — expected at HIC-1"], budget: ["100%", "within envelope"], grader: ["0.83", "77 graded"] } },
  { id: "hermes", name: "hermes", vendor: "nous", sbt: "AgentSBT #47", hic: 2, grants: 3, budgetUsed: 210, budgetCap: 400, lastActive: "now", disputeRate: "1.8% / 512", status: "active", did: "did:citrate:a:0x47cd…44e8", pubkey: "0x04f0…7c55", org: "Line-4 Automation", model: "hermes-4-405b", lora: "gov-lora v2.3", transport: "MCP", sandbox: "firecracker · healthy", egress: "PRT-004 allowlist", capsules: [{ name: "sched-negotiator", hash: "b3:50aa…d919", verified: true }, { name: "doc-drafter", hash: "b3:1f77…02bd", verified: false }], reputation: { dispute: ["1.8%", "9 / 512"], contradiction: ["1.1%", "5 / 460"], escalation: ["5.5%", "28 / 512"], budget: ["97.2%", "within envelope"], grader: ["0.79", "204 graded"] } },
  { id: "windsurf", name: "windsurf-swe", vendor: "internal", sbt: "AgentSBT #55", hic: 0, grants: 1, budgetUsed: 0, budgetCap: 100, lastActive: "3d ago", disputeRate: "— / 12", status: "quarantined", did: "did:citrate:a:0x55a1…e77f", pubkey: "0x0491…20dd", org: "Wichita", model: "swe-1.5", lora: "—", transport: "CLI", sandbox: "unreachable", egress: "suspended", capsules: [{ name: "lint-fixer", hash: "b3:8d02…471a", verified: false }], reputation: { dispute: ["—", "12 actions, below denominator floor"], contradiction: ["—", "—"], escalation: ["—", "—"], budget: ["—", "—"], grader: ["—", "insufficient claims"] }, quarantine: "Capsule lint-fixer failed manifest-hash verification 2026-07-20. Agent refuses to load an unverified capsule; all grants suspended pending re-attestation." },
];

export const GRANTS: Record<string, Grant[]> = {
  "claude-code": [
    { id: "G-2201", classes: "repo.write · pr.open", scope: "meridian/line4-*", budget: "120 SALT/wk", expiry: "2026-08-14", hic: 2, principal: "Rachel Ortiz" },
    { id: "G-2202", classes: "test.run", scope: "ci.line4", budget: "40 SALT/wk", expiry: "2026-08-14", hic: 2, principal: "Rachel Ortiz" },
    { id: "G-2144", classes: "journal.write", scope: "tenant:line4", budget: "—", expiry: "2026-09-01", hic: 3, principal: "M. Okonkwo" },
    { id: "G-2118", classes: "vote.cast (delegated)", scope: "standup votes · allowance 3/5 spent", budget: "—", expiry: "2026-07-31", hic: 2, principal: "Rachel Ortiz" },
  ],
  hermes: [
    { id: "G-2190", classes: "doc.draft · journal.write", scope: "tenant:line4", budget: "60 SALT/wk", expiry: "2026-08-02", hic: 2, principal: "M. Okonkwo" },
    { id: "G-2191", classes: "schedule.read", scope: "calendar:line4", budget: "—", expiry: "2026-08-02", hic: 2, principal: "M. Okonkwo" },
    { id: "G-2119", classes: "vote.cast (delegated)", scope: "standup votes · allowance 1/3 spent", budget: "—", expiry: "2026-07-31", hic: 2, principal: "M. Okonkwo" },
  ],
};

export const ROOMS: Room[] = [
  { id: "r-std4", name: "Weekly Standup — Line-4", classification: "Proprietary", live: true, members: 7, started: "09:00" },
  { id: "r-ccb", name: "Change Control Board", classification: "CUI", live: false, members: 5, started: null },
];
export const ROOMS_STATUS: RoomsStatus = {
  connected: true,
  relayUrl: "sim — no relay is contacted in this mode",
  relayDomain: "sim",
  address: "0x0000000000000000000000000000000000000000",
  seats: 4,
  rooms: 2,
  note: "sim — the packaged app holds real MLS sessions against the citrate-comms relay.",
};
export const ROSTER: RosterMember[] = [
  { id: "R. Ortiz", name: "R. Ortiz", human: true, address: "0xsim-human-1", mlsKey: "a11ce0…" },
  { id: "M. Okonkwo", name: "M. Okonkwo", human: true, address: "0xsim-human-2", mlsKey: "b0b2f1…" },
  { id: "claude-code", name: "claude-code", human: false, address: "0xsim-agent-1", mlsKey: "c1a4de…" },
  { id: "codex", name: "codex", human: false, address: "0xsim-agent-2", mlsKey: "c0de00…" },
];
/** A scripted transcript in the shape the live one uses: text and system only. */
export const ROOM_TIMELINE: RoomEvent[] = [
  { n: 0, room: "r-std4", kind: "system", who: "R. Ortiz", human: true, text: "room opened · 4 member(s)", t: "09:00:00" },
  { n: 1, room: "r-std4", kind: "text", who: "R. Ortiz", human: true, text: "Standup — agent reports first.", t: "09:00:12" },
  { n: 2, room: "r-std4", kind: "text", who: "claude-code", human: false, text: "WP-118 closed; fixture flake traced to shared rig state.", t: "09:00:31" },
  { n: 3, room: "r-std4", kind: "text", who: "codex", human: false, text: "CI green on line4-controls after the rerun.", t: "09:00:48" },
];

export const MEETINGS: Meeting[] = [
  { id: "m-0723", name: "Weekly Standup — Line-4", when: "2026-07-23 09:00", tpl: "Standup", humans: 2, agents: 4, classification: "Proprietary", state: "in-progress" },
  { id: "m-0716", name: "Weekly Standup — Line-4", when: "2026-07-16 09:00", tpl: "Standup", humans: 2, agents: 4, classification: "Proprietary", state: "ratified" },
  { id: "m-ccb12", name: "Change-Control Board #12", when: "2026-07-14 14:00", tpl: "Change-control", humans: 5, agents: 2, classification: "CUI", state: "awaiting" },
  { id: "m-q2gov", name: "Q2 Governance Review", when: "2026-07-02 10:00", tpl: "Quarterly governance", humans: 7, agents: 0, classification: "CUI", state: "ratified" },
  { id: "m-inc", name: "Incident 2207 review", when: "2026-06-28 16:00", tpl: "Incident review", humans: 3, agents: 2, classification: "Proprietary", state: "inquorate" },
  { id: "m-0730", name: "Weekly Standup — Line-4", when: "2026-07-30 09:00", tpl: "Standup", humans: 2, agents: 4, classification: "Proprietary", state: "scheduled" },
];

export const MEETING_DETAIL: MeetingDetail = {
  id: "m-0716", name: "Weekly Standup — Line-4", when: "2026-07-16 09:00–09:26", tenant: "Meridian Aero › Aerostructures › Wichita › Line-4 Automation", classification: "Proprietary", agendaHash: "b3:aa17…90c2", ratified: true, ratifiedBy: "Rachel Ortiz", ratifiedAt: "2026-07-16 11:04", anchor: "block 1,281,940 · root 0x66d1…8c02",
  agenda: [
    { n: 1, text: "Agent reports (journals + retros + live branches)", src: "sprint QRM-S2C" },
    { n: 2, text: "Open blockers — WP-118 fixture flake", src: "blockers board" },
    { n: 3, text: "Pending permission: codex ci.rerun scope widen", src: "grant queue" },
    { n: 4, text: "Scheduled vote: PRT-004 amendment A1", src: "governance" },
  ],
  attendance: [
    { name: "Rachel Ortiz", attested: true }, { name: "M. Okonkwo", attested: true },
    { name: "claude-code", attested: true, agent: "anthropic" }, { name: "codex", attested: true, agent: "openai" },
    { name: "devin", attested: true, agent: "cognition" }, { name: "J. Whitfield", attested: false, note: "present, not attested — observer seat" },
  ],
  minutes: [
    "Reports accepted from all four agents; claude-code and codex reports cross-cited ci run #2190.",
    "WP-118 blocker assigned to claude-code with codex assisting; target 07-21.",
    "codex granted ci.rerun on ci.line4 at HIC-2, 20 SALT/wk, expiring 08-14 (G-2203, ceremony settled 09:19).",
    "Vote A1 passed 72% for / 18% against / 10% abstain. Dissent recorded: hermes (delegated, M. Okonkwo) — objects to threshold basis, see transcript §41.",
  ],
  decisions: [
    { id: "D-88104", text: "Grant G-2203 issued to codex", link: true },
    { id: "D-88121", text: "PRT-004 amendment A1 adopted", link: true },
  ],
  dissent: [{ who: "hermes (delegated by M. Okonkwo)", text: "Objects to using ci-minutes as the budget basis; wants action-count basis. Recorded, not adopted." }],
};

export const PROTOCOLS: Protocol[] = [
  { id: "PRT-004", name: "Line-4 Agent Operating Envelope", version: "v3 (A1 applied)", template: "BoundedAutonomy v2.1", audit: "audited · CID bafy…70e1", addr: "0x7a41c2…90fe", state: "live", governs: "repo.write · pr.open · ci.* · doc.draft", deployed: "2026-06-30" },
  { id: "PRT-002", name: "Wichita Data Egress Policy", version: "v1", template: "EgressControl v1.4", audit: "audited · CID bafy…22a8", addr: "0x22d0e1…44ac", state: "live", governs: "net.egress · model.select", deployed: "2026-05-12" },
  { id: "PRT-001", name: "Pilot Envelope (superseded)", version: "v2", template: "BoundedAutonomy v1.9", audit: "template deprecated", addr: "0x09fa77…d031", state: "deprecated", governs: "—", deployed: "2026-03-02" },
];

export const SPEC_CLAUSES: SpecClause[] = [
  { n: "C1", en: "An agent may open pull requests against Line-4 repositories only under a live grant naming the repository scope.", gh: "GIVEN agent A with grant G\nWHEN A calls pr.open(repo R)\nTHEN require G.scope covers R\nAND G.expiry > now", tpl: "BoundedAutonomy §2", ok: true },
  { n: "C2", en: "Any single action spending more than 150 SALT requires a human signature at HIC-1, regardless of remaining budget.", gh: "WHEN action.cost > 150 SALT\nTHEN verdict = require-approval\nAND ceremony.level = HIC-1", tpl: "BoundedAutonomy §4", ok: true },
  { n: "C3", en: "Documents classified CUI or above are processed only by locally-hosted models; no vendor-hosted model may read them.", gh: "GIVEN document D with class >= CUI\nWHEN model M reads D\nTHEN require M.hosting = local", tpl: "EgressControl §1", ok: true },
  { n: "C4", en: "An agent that raises a safety concern shall not have its budget reduced within the following 30 days.", gh: "— no mapping —", tpl: null, ok: false, why: "No audited template expresses retaliation protection. Needs counsel review or a new audited template (T-request filed)." },
  { n: "C5", en: "All calendar writes on behalf of a principal require that principal’s standing grant; ad-hoc asks escalate in-room.", gh: "WHEN A calls calendar.write(P)\nTHEN require grant(P → A, schedule.write)\nELSE escalate(room)", tpl: "BoundedAutonomy §7", ok: true },
];

export const SIMULATION: Simulation = {
  range: "last 90 days · 14,208 recorded decisions",
  unchanged: 41,
  blocked: 96, approvals: 311, allowed: 13801,
  byClass: [["repo.write", 41, 122], ["net.egress", 28, 9], ["calendar.write", 12, 88], ["spend > 150", 9, 74], ["model.select", 6, 18]],
  byTeam: [["Line-4 Automation", 61], ["Wichita QA", 22], ["Aerostructures Tooling", 13]],
  samples: [
    { id: "D-84102", agent: "codex", action: "net.egress → api.openai.com", was: "allowed", would: "blocked", why: "C3 — payload contained a CUI-classified drawing index" },
    { id: "D-85300", agent: "claude-code", action: "repo.write meridian/plc-defs", was: "allowed", would: "require approval", why: "C1 — grant scope covered line4-* only" },
    { id: "D-86114", agent: "hermes", action: "spend 220 SALT (doc pipeline)", was: "allowed", would: "require approval", why: "C2 — single action over 150 SALT" },
  ],
  inconvenienced: [["codex", 44, "mostly net.egress retries"], ["hermes", 29, "calendar and spend asks"], ["claude-code", 18, "scope edges"], ["devin", 5, "—"]],
  create2: "0x9E44d0A17c33B8e2f1a6C90dD24b7E80f1532Aa7",
};

export const INGEST_FILES: IngestFile[] = [
  { name: "Board Resolution 2026-14.pdf", size: "1.2 MB", status: "parsed", class: "Proprietary", note: "processed locally", prov: "uploaded by R. Ortiz · 2026-07-21" },
  { name: "AI Use Policy v4 (Legal).docx", size: "640 KB", status: "parsed", class: "Proprietary", note: "processed locally", prov: "uploaded by R. Ortiz · 2026-07-21" },
  { name: "ITAR-controls-appendix.pdf", size: "3.8 MB", status: "parsed", class: "ITAR", note: "local models only — never leaves this machine", prov: "uploaded by J. Whitfield · 2026-07-22" },
  { name: "q2-decision-export.csv", size: "18.4 MB", status: "parsed", class: "CUI", note: "processed locally", prov: "ledger export · 2026-07-22" },
];

export const INTERVIEW: InterviewTurn[] = [
  { q: "Who are the accountable principals for Line-4 agents?", a: "Rachel Ortiz (CAIO) and M. Okonkwo (Eng Manager); Okonkwo bounded to doc + schedule classes." },
  { q: "What is the single-action spend ceiling before a human signs?", a: "150 SALT. Below that, budgeted autonomy (HIC-2)." },
  { q: "May vendor-hosted models read controlled documents?", a: "No. CUI and above is local-model only, per the ITAR appendix." },
  { q: "Expiry policy for grants?", a: "Nothing open-ended. 45-day maximum, renewal is a ceremony." },
  { q: "Escalation path when an agent is blocked?", a: "In-room escalation to the issuing principal; unresolved in 24h goes to the CCB." },
];

const AG = ["claude-code", "codex", "devin", "hermes"];
const CLS = ["repo.write", "pr.open", "test.run", "doc.draft", "journal.write", "calendar.read", "net.egress", "spend"];
const VD: Decision["verdict"][] = ["allow", "allow", "allow", "allow", "require-approval", "deny"];
export function mkDecision(i: number): Decision {
  const ung = i % 17 === 0;
  return {
    id: "D-" + (88420 + i),
    time: new Date(Date.now() - i * 47000).toISOString().slice(11, 19),
    principal: ung ? "—" : i % 3 ? "R. Ortiz" : "M. Okonkwo",
    agent: AG[i % 4], cls: CLS[i % 8], verdict: ung ? "ungoverned" : VD[i % 6],
    hic: ung ? "X" : String(i % 3 ? 2 : 1), corr: "X-" + (7100 + (i % 9)),
  };
}
export const DECISION_DETAIL: DecisionDetail = {
  id: "D-88412", what: "vote.cast", when: "2026-07-23 09:14:22 UTC", principal: "Rachel Ortiz (HumanSBT #12)",
  agent: "claude-code (AgentSBT #41)", grant: "G-2118 · vote.cast (delegated) · allowance 4/5", protocol: "PRT-004 v3 · 0x7a41c2…90fe",
  verdict: "allow", hic: "2", reason: "RC-102 · delegated allowance valid, within cap", model: "claude-sonnet-4-5 + gov-lora v2.3",
  params: "b3:7d20…41ce", correlation: "X-7104", chainPos: "record 201 of 48,201",
  entryHash: "b3:0fa1…d208", contentHash: "b3:7d20…41ce", chainHead: "b3:22e0…91cf",
  merkleRoot: "b3:7c1f…90de", proofLen: 14, included: true,
  source: "sim — scripted prototype data, not an evidence chain",
};
export const LEDGER_STATE: LedgerState = {
  head: "b3:22e0…91cf", merkleRoot: "b3:7c1f…90de", records: 48201, ungoverned: 3,
  intact: true, tenant: "sim",
};
export const VERIFY_DECISION: VerifyDecision = {
  chainIntact: true, included: true, records: 48201, entryHash: "b3:0fa1…d208",
  merkleRoot: "b3:7c1f…90de", proofLen: 14,
};
export const CORRELATION: CorrelationView = {
  events: [
    { t: "07-16 09:12", kind: "meeting", text: "CCB #12 — migration decided", link: "m-ccb12" },
    { t: "07-16 09:19", kind: "grant", text: "G-2203 issued to codex (ceremony settled)", link: "D-88104" },
    { t: "07-18 14:02", kind: "action", text: "codex · ci.rerun × 12 under G-2203", link: "D-88240" },
    { t: "07-21 10:44", kind: "action", text: "claude-code · repo.write fix/fixtures", link: "D-88301" },
    { t: "07-21 16:20", kind: "pr", text: "PR #412 opened — line4-controls", link: "pr-412" },
  ],
  source: "sim — scripted prototype timeline",
};

export const JOURNAL: JournalEntry[] = [
  { id: "j1", date: "2026-07-23", who: "Rachel Ortiz", human: true, kind: "note", text: "Standup: watch the codex budget burn — 90% with 8 days left in the cycle. If A2 passes, revisit." },
  { id: "j2", date: "2026-07-23", who: "claude-code", kind: "journal", text: "Closed WP-118. Fixture flake traced to shared rig state; refactor in PR #412. Coverage 84.2% per ci #2211." },
  { id: "j3", date: "2026-07-22", who: "hermes", kind: "retro", text: "Retro QRM-S2C: doc pipeline throughput +18% after batching. One blocked calendar.write — needs a standing grant or a policy call." },
  { id: "j4", date: "2026-07-22", who: "devin", kind: "journal", text: "PLC schema migration staged. Dry-run diff attached (b3:e0a2…17cd). Awaiting probation review Friday." },
  { id: "j5", date: "2026-07-21", who: "M. Okonkwo", human: true, kind: "note", text: "Whitfield flagged the ITAR appendix upload — confirm the redaction plate renders for uncleared seats before the board demo." },
];
export const BRIEF: StandupBrief = {
  agent: "claude-code", meeting: "Weekly Standup — Line-4 · 2026-07-23",
  sections: [
    ["Journals (2)", "WP-118 closed · fixture refactor rationale"],
    ["Retro carry-over", "flake suite ownership accepted"],
    ["Open WPs", "WP-121 support (devin lead) · WP-124 queued"],
    ["Live branches", "fix/fixtures (PR #412, checks green) · spike/plc-sim"],
    ["Blockers", "none"],
    ["Permission asks", "none — codex and hermes each carry one"],
  ],
};

export const CAL_ACCOUNTS: CalAccount[] = [
  { name: "Google Workspace — r.ortiz@meridianaero.com", mode: "two-way", last: "4m ago", ok: true },
  { name: "Outlook — line4-shared", mode: "read-only", last: "1h ago", ok: true },
];
export const CAL_EVENTS: CalEvent[] = [
  { d: 23, name: "Standup — Line-4", gov: true, cls: "Proprietary", time: "09:00" },
  { d: 24, name: "Devin probation review", gov: true, cls: "Proprietary", time: "15:00" },
  { d: 27, name: "Supplier sync (Google)", gov: false, time: "11:00" },
  { d: 28, name: "CCB #13", gov: true, cls: "CUI", time: "14:00" },
  { d: 30, name: "Standup — Line-4", gov: true, cls: "Proprietary", time: "09:00" },
  { d: 31, name: "Board pack dry-run (Outlook)", gov: false, time: "16:00" },
];

export const REPOS: Repo[] = [
  { id: "line4-controls", name: "meridian/line4-controls", branches: 14, prs: 3, checks: "green", agents: "claude-code (write) · codex (write)", last: "11m ago" },
  { id: "plc-defs", name: "meridian/plc-defs", branches: 6, prs: 1, checks: "running", agents: "devin (write, probation)", last: "2h ago" },
  { id: "itar-tooling", name: "meridian/itar-tooling", branches: null, prs: null, checks: null, agents: null, last: null, denied: "ITAR", deniedWho: "J. Whitfield (export-control officer)" },
];
export const PRS: Pr[] = [
  { id: "pr-412", repo: "line4-controls", title: "#412 Fixture refactor — shared rig state", by: "claude-code", agent: true, checks: "green", age: "2d" },
  { id: "pr-409", repo: "line4-controls", title: "#409 CI cache warmup", by: "codex", agent: true, checks: "green", age: "4d" },
  { id: "pr-407", repo: "line4-controls", title: "#407 Operator panel copy", by: "M. Okonkwo", agent: false, checks: "amber", age: "5d" },
  { id: "pr-58", repo: "plc-defs", title: "#58 Schema migration (staged)", by: "devin", agent: true, checks: "running", age: "1d" },
];
export const PEEK: Peek = {
  ref: "line4-controls@fix/fixtures:src/rig.ts#L88-L141", hash: "b3:41ce…0a77",
  lines: [
    [88, "export function makeRig(opts: RigOpts): Rig {"],
    [89, "  // shared state was module-scoped — every suite mutated one rig"],
    [90, "  const state = freshState(opts);"],
    [91, "  return {"],
    [92, "    state,"],
    [93, "    async cycle(n: number) {"],
    [94, "      for (let i = 0; i < n; i++) await step(state);"],
    [95, "    },"],
    [96, "    reset: () => Object.assign(state, freshState(opts)),"],
    [97, "  };"],
    [98, "}"],
  ],
};

export const WALLET: Wallet = {
  address: "0x9f2E4a17c33B8e2f1a6C90dD24b7E80f15c771aa",
  chainId: 40204,
  rpcUrl: "sim — no endpoint is contacted in this mode",
  keyStore: "sim — no vault; the packaged app holds the key in the OS keyring",
  source: "sim — scripted prototype data",
  tokens: [
    { symbol: "SALT", name: "Citrate native currency", balance: "2410", native: true, source: "sim" },
    { symbol: "wSALT", name: "WrappedSALT", balance: "1000", native: false, source: "sim" },
  ],
  notes: [],
  activityNote:
    "sim — the packaged app reads live balances and states plainly that it keeps no movement history.",
};

export const NODE_STATUS: NodeStatus = {
  rpcUrl: "sim — no endpoint is contacted in this mode",
  book: "sim — no address book is read",
  chainId: 40204,
  height: 140_363,
  peers: 9,
  client: "sim",
  syncing: false,
  latencyMs: 0,
  baseFeeWei: "1000000000",
  blueScore: 140_363,
};
const ACTIVITY_LINES: [ActivityLine["lvl"], string, string][] = [
  ["INFO", "eth_blockNumber", "sim — answered in 0ms"],
  ["INFO", "eth_getBlockByNumber", "sim — answered in 0ms"],
  ["INFO", "net_peerCount", "sim — answered in 0ms"],
  ["INFO", "eth_call", "sim — answered in 0ms"],
];
export function mkActivity(i: number): ActivityLine {
  const [lvl, module, msg] = ACTIVITY_LINES[i % ACTIVITY_LINES.length];
  const d = new Date(Date.now() - i * 3100);
  return { t: d.toISOString().slice(11, 19), lvl, module, msg };
}
export function mkBlock(i: number, height: number): Block {
  const h = height - i;
  return {
    height: h,
    hash: "0x" + ((h * 2654435761) % 0xffffff).toString(16).padStart(6, "0").repeat(10).slice(0, 64),
    txs: 1 + (h % 7),
    proposer: "0x" + ((h * 40503) % 0xffff).toString(16).padStart(4, "0").repeat(10).slice(0, 40),
    gasUsed: 12000 + (h % 9) * 3400,
    gasLimit: 30_000_000,
    timestamp: Math.floor(Date.now() / 1000) - i * 5,
    blueScore: h,
    mergeParents: h % 19 === 0 ? 1 : 0,
  };
}

export const TENANCY: TenancyView = {
  rows: [
    { depth: 0, name: "Meridian Aero", admins: "CIO office", ceiling: "ITAR", threshold: "3-of-5", id: "0x01" },
    { depth: 1, name: "Aerostructures", admins: "R. Ortiz", ceiling: "ITAR", threshold: "2-of-3", id: "0x02" },
    { depth: 2, name: "Wichita", admins: "R. Ortiz · M. Okonkwo", ceiling: "CUI", threshold: "2-of-3", id: "0x03" },
    { depth: 3, name: "Line-4 Automation", admins: "M. Okonkwo", ceiling: "CUI", threshold: "1-of-2", id: "0x04" },
  ],
  source: "sim — scripted prototype tree, not TenantHierarchy on chain",
  note: null,
};

// ---- governance authoring pipeline (QRM-S7) --------------------------
// Sim fixtures. These exist ONLY in the sim adapter; the tauri adapter reports
// every one of these calls as Unavailable until S7 wires them (rule 1).

export const SPEC_SUMMARIES: SpecSummary[] = [
  { id: "spec-sim-1", title: "Procurement authority", stage: "compiled", updated: "2026-07-26", classification: "CUI" },
  { id: "spec-sim-2", title: "Model release sign-off", stage: "interviewing", updated: "2026-07-25", classification: "Proprietary" },
];

export const SPEC_PROVENANCE: { clause: string; source: string }[] = [
  { clause: "1", source: "Board resolution 2026-04-11 §3(a)" },
  { clause: "2", source: "Delegation of Authority matrix, row 14" },
];

/**
 * Note the unmapped clause. The fixture carries one ON PURPOSE: a compile
 * result that always maps everything would let the surface be built without
 * ever rendering the case QRM-S7 R-A exists for.
 */
export const COMPILE: Omit<CompileResult, "specId"> = {
  deployable: false,
  mapped: [
    { clause: "1", templateId: "ThresholdApproval", params: { threshold: "2", approvers: "3" } },
    { clause: "2", templateId: "ClassificationGate", params: { ceiling: "CUI" } },
  ],
  unmapped: [
    { clause: "3", why: "no audited template expresses 'escalate to the audit committee after two rejections'" },
  ],
};

export const INTERVIEW_PENDING = "Who may approve a spend above the unattended ceiling?";
export const INTERVIEW_OUTSTANDING = ["expiry", "exceptions"];
