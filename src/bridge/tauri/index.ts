// =====================================================================
// citrate-quorum — Tauri adapter (QRM-S2 · mostly LIVE)
//
// Domains with a real Rust backend today call it (Rule 1: real data or an
// honest error, never fabricated). LIVE here:
//
//  · policy  — the gate. Evaluates against the tenant's live grants and
//              appends the decision to the tenant's BLAKE3 evidence chain.
//  · ledger  — reads that same chain back, including one decision as a
//              document with a recomputed Merkle inclusion proof.
//  · session  — the active tenant scope (`current()` still needs the IdP).
//  · meetings — the governed meeting record: agenda frozen under a BLAKE3
//              hash, minutes composed from the evidence chain, ratification
//              recorded as a HIC-1 act, and the on-chain anchor row.
//  · node     — chain 40204 over the address book's rpcUrl: height, peer
//              count, client, sync state, recent blocks, and this app's own
//              record of every RPC it issued.
//  · wallet   — the vault's address and its live balances.
//  · settings — the on-chain tenant tree from TenantHierarchy.
//  · rooms    — a real MLS group on the citrate-comms relay; humans and agents
//               as cryptographic peers, and a relay that can decrypt nothing.
//
// Still unavailable, each for a stated reason: governance (the S6/S7 contracts), calendar (OAuth consent), repos (a GitHub
// App), and `session.current` (the OIDC RP). The SHARED signing surface
// (config / custody / auth / ceremony) is live in the kit (WP-S1.3).
//
// The tenant scope lives in the Rust backend, not here: no call below names a
// tenant, and a command with no scope established fails closed with an honest
// error rather than guessing one (Rule 6).
// =====================================================================
import type { BridgeContract } from "../domains";
import {
  Unavailable,
  type ActivityLine,
  type Block,
  type CorrelationEvent,
  type CorrelationView,
  type Decision,
  type DecisionDetail,
  type GateDecision,
  type Agent,
  type GovernedAction,
  type Grant,
  type GrantTerms,
  type JournalEntry,
  type Meeting,
  type MeetingDetail,
  type NodeStatus,
  type Room,
  type RoomEvent,
  type RoomsStatus,
  type RosterMember,
  type StandupBrief,
  type MeetingState,
  type Classification,
  type HicLevel,
  type PendingApproval,
  type ClearanceView,
  type TenancyView,
  type Unsubscribe,
  type VerifyDecision,
  type Wallet,
} from "../types";
import {
  actionApprove,
  agentsKnown,
  grantIssue,
  grantRevoke,
  grantsForAgent,
  operatorGet,
  operatorSet,
  actionEvaluateAndRecord,
  actionReject,
  approvalsPending,
  journalBrief,
  journalList,
  ledgerCorrelation,
  ledgerDecision,
  ledgerRecords,
  ledgerState,
  roomsConnectComplete,
  roomsConnectIntent,
  roomsEvents,
  roomsLeave,
  roomsList,
  roomsOpen,
  roomsRoster,
  roomsSay,
  roomsStatus,
  ledgerVerifyDecision,
  nodeActivity,
  nodeBlocks,
  nodeStatus,
  clearanceOf,
  tenancyTree,
  walletSummary,
  meetingAdmit,
  meetingAnchor,
  meetingClose,
  meetingContentHash,
  meetingGet,
  meetingOpen,
  meetingSchedule,
  meetingRatify,
  meetingsList,
  tenantActive,
  tenantSet,
  walletCreate,
  walletImport,
  walletStatus,
  type DecisionResult,
} from "./commands";

/** The Rust DTO is snake_case; the bridge contract is camelCase. */
const toRoomsStatus = (s: import("./commands").RoomsStatusDto): RoomsStatus => ({
  connected: s.connected,
  relayUrl: s.relay_url,
  relayDomain: s.relay_domain,
  address: s.address,
  seats: s.seats,
  rooms: s.rooms,
  note: s.note,
});
const toRoom = (r: import("./commands").RoomDto): Room => ({
  id: r.id,
  name: r.name,
  classification: r.classification,
  live: r.live,
  members: r.members,
  started: r.started,
});

const toGate = (d: DecisionResult): GateDecision => ({
  decisionId: d.decision_id,
  verdict: d.verdict,
  hic: d.hic as HicLevel,
  grantId: d.grant_id,
  reason: d.reason,
  chainHead: d.chain_head,
  ungoverned: d.ungoverned,
});

/**
 * An unwired async domain method.
 *
 * It REJECTS rather than throwing synchronously. The domain contract types
 * these as `Promise`-returning, and a synchronous throw from one is a trap: a
 * caller doing `bridge.x.y().then(...)` inside a `useEffect` never gets to
 * `.catch()`, the effect throws, and React unmounts the whole tree — the
 * packaged app white-screens instead of reporting that a domain is unavailable.
 * Rejecting keeps the failure inside the promise chain where callers handle it.
 */
const na =
  (op: string) =>
  (): Promise<never> =>
    Promise.reject(new Unavailable(op));

/** Poll interval for the streaming ledger ribbon (ms). The chain is append-only
 *  and low-rate; polling is honest and simple until a push channel lands. */
const LEDGER_POLL_MS = 4000;

export function createTauriBridge(): BridgeContract {
  return {
    session: {
      current: na("session.current"),
      // LIVE: backend-owned tenant scope.
      activeTenant: () => tenantActive(),
      setActiveTenant: (tenant: string) => tenantSet(tenant),
      operator: () => operatorGet(),
      setOperator: (name: string) => operatorSet(name),
    },
    policy: {
      // LIVE: quorum-policy evaluate → quorum-audit DecisionRecord → the
      // tenant's hash chain. `action_evaluate_and_record` in backend.rs.
      evaluate: async (action: GovernedAction): Promise<GateDecision> =>
        toGate(
          await actionEvaluateAndRecord({
            agent: action.agent,
            principal: action.principal ?? null,
            class: action.actionClass,
            classification: action.classification,
            cost: action.cost ?? 0,
            hic1_cost_threshold: action.hic1CostThreshold ?? 0,
            mandatory_hic1: action.mandatoryHic1 ?? false,
            // Stays empty until the agent adapters (S4) supply real tool
            // parameters to commit to. An empty hash claims nothing.
            params_hash: "",
            model_id: action.modelId ?? "",
            correlation_id: action.correlationId ?? "",
          }),
        ),
      // LIVE: refunds the charge and records the refusal (`action_reject`).
      reject: async (decisionId: number): Promise<GateDecision> =>
        toGate(await actionReject(decisionId)),
      // LIVE: records the approval, naming the human (`action_approve`).
      approve: async (decisionId: number, approver: string): Promise<GateDecision> =>
        toGate(await actionApprove(decisionId, approver)),
      // LIVE: the escalations an agent is blocked on (`approvals_pending`).
      pending: async (): Promise<PendingApproval[]> =>
        (await approvalsPending()).map((p) => ({
          decision: p.decision,
          agent: p.agent,
          principal: p.principal,
          actionClass: p.action_class,
          classification: p.classification,
          cost: p.cost,
          correlationId: p.correlation_id,
          requestedAtMs: p.requested_at_ms,
        })),
    },
    wallet: {
      // LIVE (Phase 0): the vault's address + live balances from the endpoint
      // the address book names. `eth_getBalance` for the native currency,
      // ERC-20 `balanceOf` for booked tokens.
      summary: async (): Promise<Wallet> => {
        const w = await walletSummary();
        return {
          address: w.address,
          chainId: w.chain_id,
          rpcUrl: w.rpc_url,
          keyStore: w.key_store,
          source: w.source,
          tokens: w.tokens,
          notes: w.notes,
          activityNote: w.activity_note,
        };
      },
      // LIVE (QRM-S6): the signing identity in the OS-keyring vault.
      identity: () => walletStatus(),
      createIdentity: () => walletCreate(),
      importIdentity: (mnemonic: string) => walletImport(mnemonic),
    },
    // LIVE (Phase 0): chain 40204 over the book's rpcUrl. `activity` is this
    // app's own RPC record — see NodeDomain for why there is no peer list and
    // no node log stream.
    node: {
      status: async (): Promise<NodeStatus> => {
        const s = await nodeStatus();
        return {
          rpcUrl: s.rpc_url,
          book: s.book,
          chainId: s.chain_id,
          height: s.height,
          peers: s.peers,
          client: s.client,
          syncing: s.syncing,
          latencyMs: s.latency_ms,
          baseFeeWei: s.base_fee_wei,
          blueScore: s.blue_score,
        };
      },
      blocks: async (count: number): Promise<Block[]> =>
        (await nodeBlocks(count)).map((b) => ({
          height: b.height,
          hash: b.hash,
          txs: b.txs,
          proposer: b.proposer,
          gasUsed: b.gas_used,
          gasLimit: b.gas_limit,
          timestamp: b.timestamp,
          blueScore: b.blue_score,
          mergeParents: b.merge_parents,
        })),
      activity: async (): Promise<ActivityLine[]> =>
        (await nodeActivity()).map((l) => ({
          t: l.t,
          // The Rust side emits INFO/ERROR; anything else is passed through
          // rather than coerced into a level it did not claim.
          lvl: l.lvl as ActivityLine["lvl"],
          module: l.module,
          msg: l.msg,
        })),
    },
    agents: {
      // LIVE: the fleet this tenant has evidence about (`agents_known`). The
      // registry-only fields (vendor, model, capsules, reputation) are simply
      // absent — the surface renders them as "—" naming their source rather
      // than inventing a vendor for an agent we have only seen act.
      list: async (): Promise<Agent[]> =>
        (await agentsKnown()).map((a) => ({
          id: a.id,
          name: a.id,
          grants: a.live_grants,
          budgetUsed: a.consumed,
          budgetCap: a.budget_units,
          decisions: a.decisions,
          ungoverned: a.ungoverned,
        })),
      // LIVE: real capability grants (`grants_for_agent`).
      grants: async (agentId: string): Promise<Grant[]> =>
        (await grantsForAgent(agentId)).map((g) => ({
          id: g.id,
          classes: g.classes,
          scope: g.scope,
          budget: `${g.consumed}/${g.budget_units}`,
          // Epoch milliseconds told an operator nothing and overflowed the
          // column. A grant's expiry is a date a human has to reason about.
          expiry:
            g.expires_at_ms >= Number.MAX_SAFE_INTEGER
              ? "no expiry"
              : new Date(g.expires_at_ms).toISOString().slice(0, 10),
          hic: Number(g.hic),
          principal: g.principal,
          revoked: g.revoked,
        })),
      issue: (terms: GrantTerms) =>
        grantIssue({
          id: terms.id,
          agent: terms.agent,
          issued_by: terms.principal,
          principal: terms.principal,
          tenant_scope: terms.tenantScope,
          action_classes: terms.actionClasses,
          classification_ceiling: terms.classificationCeiling,
          budget_units: terms.budgetUnits,
          expires_at_ms: terms.expiresAtMs,
          hic: terms.hic,
        }),
      revoke: (agent: string, grantId: string, revokedBy: string) =>
        grantRevoke(agent, grantId, revokedBy),
    },
    // LIVE (QRM-S3): a real MLS group on the citrate-comms relay. The relay
    // carries ciphertext and routing metadata and can decrypt nothing —
    // src-tauri/tests/server_blindness.rs proves it against a real relay store,
    // with a negative control so the proof cannot pass by searching nothing.
    rooms: {
      status: async (): Promise<RoomsStatus> => toRoomsStatus(await roomsStatus()),
      connectIntent: async (operator: string) => {
        const i = await roomsConnectIntent(operator);
        return {
          ceremonyId: i.ceremony_id,
          address: i.address,
          relayUrl: i.relay_url,
          siwe: i.siwe,
        };
      },
      connectComplete: async (ceremonyId: string, signatureHex: string): Promise<RoomsStatus> =>
        toRoomsStatus(await roomsConnectComplete(ceremonyId, signatureHex)),
      list: async (): Promise<Room[]> => (await roomsList()).map(toRoom),
      open: async (input, operator: string): Promise<Room> =>
        toRoom(await roomsOpen(operator, input.name, input.classification, input.agents)),
      roster: async (roomId: string): Promise<RosterMember[]> =>
        (await roomsRoster(roomId)).map((m) => ({
          id: m.id,
          name: m.name,
          human: m.human,
          address: m.address,
          mlsKey: m.mls_key,
        })),
      say: async (roomId: string, principal: string, text: string): Promise<void> => {
        await roomsSay(roomId, principal, text);
      },
      events: async (since: number): Promise<RoomEvent[]> =>
        (await roomsEvents(since)).map((e) => ({
          n: e.n,
          room: e.room,
          // The Rust side emits `text` or `system` and nothing else — there is no
          // audio path, so no line here can claim to have been spoken.
          kind: e.kind === "system" ? "system" : "text",
          who: e.who,
          human: e.human,
          text: e.text,
          t: e.t,
        })),
      leave: (roomId: string) => roomsLeave(roomId),
    },
    ledger: {
      // LIVE: the real per-tenant hash chain.
      query: () => ledgerRecords(),
      state: async () => {
        const s = await ledgerState();
        return {
          head: s.head,
          merkleRoot: s.merkle_root,
          records: s.records,
          ungoverned: s.ungoverned,
          intact: s.intact,
          tenant: s.tenant,
        };
      },
      // LIVE: poll the chain, emit rows appended since the last poll.
      stream: (onEvent: (e: Decision) => void): Unsubscribe => {
        let seen = 0;
        let stopped = false;
        const tick = async (): Promise<void> => {
          if (stopped) return;
          try {
            const rows = await ledgerRecords();
            for (let i = seen; i < rows.length; i++) onEvent(rows[i]);
            seen = rows.length;
          } catch {
            // A transient backend error (or no tenant scope yet) must not kill
            // the stream; the next tick retries. Errors surface through query().
          }
        };
        void tick();
        const handle = setInterval(() => void tick(), LEDGER_POLL_MS);
        return () => {
          stopped = true;
          clearInterval(handle);
        };
      },
      // LIVE (Phase 0): one decision as a document, read back out of the same
      // chain the ribbon reads — including the inclusion proof, recomputed
      // here rather than asserted.
      decision: async (id: string): Promise<DecisionDetail> => {
        const d = await ledgerDecision(id);
        return {
          id: d.id,
          what: d.what,
          when: d.when,
          principal: d.principal,
          agent: d.agent,
          grant: d.grant,
          protocol: d.protocol,
          verdict: d.verdict,
          hic: d.hic,
          reason: d.reason,
          model: d.model,
          params: d.params,
          correlation: d.correlation,
          chainPos: d.chain_pos,
          entryHash: d.entry_hash,
          contentHash: d.content_hash,
          chainHead: d.chain_head,
          merkleRoot: d.merkle_root,
          proofLen: d.proof_len,
          included: d.included,
          source: d.source,
        };
      },
      verifyDecision: async (id: string): Promise<VerifyDecision> => {
        const v = await ledgerVerifyDecision(id);
        return {
          chainIntact: v.chain_intact,
          included: v.included,
          records: v.records,
          entryHash: v.entry_hash,
          merkleRoot: v.merkle_root,
          proofLen: v.proof_len,
        };
      },
      correlation: async (corr: string): Promise<CorrelationView> => {
        const c = await ledgerCorrelation(corr);
        return {
          events: c.events.map((e) => ({
            t: e.t,
            kind: e.kind as CorrelationEvent["kind"],
            text: e.text,
            link: e.link,
          })),
          source: c.source,
        };
      },
    },
    // LIVE (QRM-S5): the governed meeting record. `list`/`get` read the
    // tenant's durable meeting store; an empty tenant returns an empty list,
    // which the surface renders as an honest empty register rather than a
    // fabricated one.
    meetings: {
      list: async (): Promise<Meeting[]> =>
        (await meetingsList()).map((m) => ({
          id: m.id,
          name: m.name,
          when: m.when,
          tpl: m.tpl,
          humans: m.humans,
          agents: m.agents,
          classification: m.classification as Classification,
          state: m.state as MeetingState,
        })),
      get: async (id: string): Promise<MeetingDetail> => {
        const d = await meetingGet(id);
        return {
          id: d.id,
          name: d.name,
          when: d.when,
          tenant: d.tenant,
          classification: d.classification as Classification,
          // A scheduled meeting has no frozen hash. Saying so beats rendering
          // an empty string that reads like a value.
          agendaHash: d.agenda_hash ?? "not frozen — the agenda is still open",
          ratified: d.ratified,
          ratifiedBy: d.ratified_by ?? undefined,
          ratifiedAt: d.ratified_at ? new Date(d.ratified_at).toISOString() : undefined,
          // The anchor is read separately (`anchorState`) because it makes an
          // eth_call. `get()` stays a local read, and the surface fills this
          // row in once the chain answers.
          anchor: undefined,
          agenda: d.agenda,
          attendance: d.attendance.map((a) => ({
            name: a.name,
            attested: a.attested,
            agent: a.agent ?? undefined,
            note: a.note ?? undefined,
          })),
          minutes: d.minutes,
          decisions: d.decisions,
          dissent: d.dissent,
        };
      },
      contentHash: (id: string) => meetingContentHash(id),
      // LIVE (QRM-S6): MeetingRegistry.verifyMinutes via eth_call, the contract
      // resolved by name from the canonical address book at runtime (rule 8).
      anchorState: async (id: string): Promise<string> => {
        const a = await meetingAnchor(id);
        switch (a.state) {
          case "anchored":
            return `anchored — block ${a.block}, registered by ${a.ratifier} in MeetingRegistry ${a.contract}`;
          case "not-anchored":
            // The chain WAS asked. That is a different fact from not asking.
            return `not anchored — ${a.reason} (checked MeetingRegistry ${a.contract})`;
          case "mismatch":
            // The loudest state: the local minutes and the registered
            // commitment disagree. Never soften this into "not anchored".
            return `MISMATCH — the chain holds a different minutes hash for this meeting (${a.on_chain}). The local record and the on-chain commitment disagree.`;
          case "unavailable":
            return `not anchored — ${a.reason}`;
          case "unreachable":
            return `unknown — ${a.reason}. This is NOT the same as "not anchored": the chain could not be asked.`;
        }
      },
      schedule: (input) =>
        meetingSchedule({
          id: input.id,
          name: input.name,
          when: input.when,
          template: input.template,
          min_humans: input.minHumans,
          classification: input.classification,
          workspace: input.workspace,
        }),
      admit: (input) =>
        meetingAdmit({
          id: input.id,
          name: input.name,
          vendor: input.vendor,
          attested: input.attested,
          clearance: input.clearance,
        }),
      open: (id: string) => meetingOpen(id),
      close: (id: string) => meetingClose(id),
      ratify: async (id: string, by: string, expectHash: string) => {
        await meetingRatify(id, by, expectHash);
      },
    },
    // QRM-S7 reshaped this domain to the authoring pipeline. Every method is
    // still Unavailable — the shape landed before the implementation, on
    // purpose, so the surface can be written against the real contract instead
    // of against a placeholder that would have to be unpicked later.
    governance: {
      specs: na("governance.specs"),
      protocols: na("governance.protocols"),
      spec: na("governance.spec"),
      compile: na("governance.compile"),
      simulate: na("governance.simulate"),
      ingest: na("governance.ingest"),
      interview: na("governance.interview"),
      deployIntent: na("governance.deployIntent"),
      deployComplete: na("governance.deployComplete"),
      bind: na("governance.bind"),
    },
    // LIVE (QRM-S5.7): journals and retros read from the tenant's workspace
    // `.agentile` files; the brief adds the live governance state (what this
    // agent is blocked on, and what authority it holds).
    journal: {
      list: async (): Promise<JournalEntry[]> =>
        (await journalList()).entries.map((e) => ({
          id: e.id,
          date: e.date,
          who: e.who,
          kind: e.kind as JournalEntry["kind"],
          text: e.text,
          // `null` from Rust means "not decided", which is not the same as
          // `false`. Passing it through as undefined keeps the surface from
          // rendering an agent as a human.
          human: e.human ?? undefined,
        })),
      brief: async (agentId: string, meetingId: string): Promise<StandupBrief> => {
        const b = await journalBrief(agentId, meetingId);
        return { agent: b.agent, meeting: b.meeting, sections: b.sections };
      },
    },
    calendar: { accounts: na("calendar.accounts"), events: na("calendar.events") },
    repos: { list: na("repos.list"), prs: na("repos.prs"), peek: na("repos.peek") },
    // LIVE (Phase 0): TenantHierarchy on chain, resolved by name from the BFR
    // address book. A tree that is empty because the contract has no root says
    // exactly that — it does not render as "no tenants".
    settings: {
      clearance: async (address: string, tenant: string): Promise<ClearanceView> => {
        const c = await clearanceOf(address, tenant);
        return {
          effective: c.effective,
          recorded: c.recorded,
          foreignNational: c.foreign_national,
          tenantCeiling: c.tenant_ceiling,
          boundedTo: c.bounded_to,
          subject: c.subject,
          source: c.source,
          note: c.note,
        };
      },
      tenancy: async (): Promise<TenancyView> => {
        const t = await tenancyTree();
        return {
          rows: t.rows.map((r) => ({
            depth: r.depth,
            name: r.name,
            admins: r.admins,
            ceiling: r.ceiling,
            threshold: r.threshold,
            id: r.id,
          })),
          source: t.source,
          note: t.note,
        };
      },
    },
  };
}
