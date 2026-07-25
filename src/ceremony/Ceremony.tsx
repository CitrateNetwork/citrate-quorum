// =====================================================================
import markBlack from "../assets/brand/citrate_mark_black.svg";
// citrate-quorum — the Signature Ceremony (QRM-S2D · gated in QRM-S2)
//
// Ported from design/CitrateQuorum.dc.html §CEREMONY. THE single
// human-in-the-loop signing surface (design brief §3.3). Any surface that
// needs a signature calls useCeremony().request(intent) and awaits the
// outcome — it NEVER performs the action itself and NEVER fabricates a
// settled state. Charter register, always (signing is a document act).
//
// THE POLICY GATE RUNS FIRST (I-3/I-4). `request()` does not open this dialog:
// it calls bridge.policy.evaluate(intent.action), which evaluates the action
// against the tenant's live grants and appends the verdict to the tenant's
// evidence chain. Only then is a signature asked for, and the verdict that was
// recorded is shown alongside what is being signed:
//  · deny            → rejected before the dialog ever opens
//  · require-approval→ opens, flagged HIC-1, this signature IS the approval
//  · ungoverned      → opens, flagged — recorded and surfaced, never silent
//  · gate unreachable→ rejected, fail-closed, with the reason
//
// Properties enforced here, which are HIC security expressed as UI:
//  · decoded intent (key/value rows), never a raw hex blob by default
//  · raw/undecodable calldata is blocked behind an explicit ack
//  · one approval → one signature; a queue is worked head-first
//  · multisig envelopes show who signed, the threshold, and the expiry
//  · phases review → signing → broadcasting → settled | rejected
//
// The sim resolves signing/broadcasting on timers; the Tauri adapter will
// drive the same phases from the real kit ceremony (config/custody/auth/
// ceremony are already live in citrate-core-kit — WP-S1.3).
// =====================================================================
import {
  createContext,
  useCallback,
  useContext,
  useEffect,
  useMemo,
  useRef,
  useState,
  type ReactNode,
} from "react";
import {
  bridge,
  type CeremonyPhase,
  type GateDecision,
  type PendingApproval,
  type SignatureIntent,
} from "../bridge";
import { LoaderMark } from "../components/LoaderMark";
import { runGate, type CeremonyResult } from "./gate";

export type { CeremonyResult } from "./gate";

interface Pending {
  intent: SignatureIntent;
  /** The verdict recorded BEFORE this dialog opened. */
  gate: GateDecision;
  /**
   * Set when this is an agent's escalation being ANSWERED rather than a new
   * action being proposed. Signing approves the existing decision; rejecting
   * refuses it. Either way no second decision is evaluated — the question was
   * already asked and recorded.
   */
  answering?: PendingApproval;
  resolve: (r: CeremonyResult) => void;
}

interface CeremonyApi {
  /**
   * Run the policy gate on the intent's action, record the verdict, and — if
   * the verdict permits — queue the signature request. Resolves when it settles
   * or is rejected. A denied or ungateable action never reaches the dialog.
   */
  request: (intent: SignatureIntent) => Promise<CeremonyResult>;
  /**
   * Open the ceremony on an agent's escalation. This is the human half of
   * HIC-1: an agent stopped and asked, and this is where someone answers.
   * Unlike `request`, it does NOT run the gate — the decision already exists.
   */
  review: (pending: PendingApproval) => Promise<CeremonyResult>;
}

/** The first 10 hex chars of a chain head — enough to recognize, short enough to read. */
const shortHead = (h: string) => (h.length > 12 ? `${h.slice(0, 12)}…` : h);

const VERDICT_STYLE: Record<GateDecision["verdict"], { label: string; color: string }> = {
  allow: { label: "Allowed by policy", color: "var(--ok)" },
  "require-approval": { label: "Requires your approval", color: "var(--warn)" },
  deny: { label: "Denied by policy", color: "var(--danger)" },
  ungoverned: { label: "Ungoverned", color: "var(--danger)" },
  rejected: { label: "Refused by a human", color: "var(--danger)" },
  approved: { label: "Approved by a human", color: "var(--ok)" },
};

const Ctx = createContext<CeremonyApi | null>(null);

/** Surfaces call this to route a signed action through the one ceremony. */
export function useCeremony(): CeremonyApi {
  const api = useContext(Ctx);
  if (!api) throw new Error("useCeremony must be used within <CeremonyProvider>");
  return api;
}

const ORIGIN_COLOR: Record<string, string> = {
  user: "var(--accent-text)",
  "agent:claude-code": "var(--z-cyan)",
  "agent:codex": "var(--z-indigo)",
  "agent:hermes": "var(--z-amber)",
  "agent:devin": "var(--z-magenta)",
};

const PHASES: { key: CeremonyPhase; label: string }[] = [
  { key: "review", label: "Review" },
  { key: "signing", label: "Signing" },
  { key: "broadcasting", label: "Broadcasting" },
  { key: "settled", label: "On record" },
];

export function CeremonyProvider({ children }: { children: ReactNode }) {
  const [queue, setQueue] = useState<Pending[]>([]);
  // The ceremony is the human-identity boundary, so it — not each surface —
  // stamps the principal onto the action. Unresolved (no IdP) leaves it unset,
  // and the record honestly carries no principal rather than a guessed one.
  const principal = useRef<string | undefined>(undefined);
  useEffect(() => {
    // The IdP names you when it can; otherwise the operator named at sign-in
    // does. One of them must, because an approval is recorded against a human
    // and the backend refuses an anonymous one.
    Promise.resolve()
      .then(() => bridge.session.current())
      .then((s) => {
        principal.current = s.user.name;
      })
      .catch(() =>
        Promise.resolve()
          .then(() => bridge.session.operator())
          .then((op) => {
            principal.current = op ?? undefined;
          })
          .catch(() => {
            principal.current = undefined;
          }),
      );
  }, []);
  const [phase, setPhase] = useState<CeremonyPhase>("review");
  const [ack, setAck] = useState(false);
  const [settledNote, setSettledNote] = useState<string>("");
  /** A commit the backend refused. Shown instead of an "On record" stamp. */
  const [commitError, setCommitError] = useState<string>("");
  const timers = useRef<ReturnType<typeof setTimeout>[]>([]);

  const head = queue[0] ?? null;

  const request = useCallback(async (intent: SignatureIntent): Promise<CeremonyResult> => {
    // I-3: the gate runs BEFORE this dialog opens. No signature is ever asked
    // for on an action the policy engine has not ruled on and recorded.
    const outcome = await runGate(
      (a) => bridge.policy.evaluate(a),
      intent.action,
      principal.current,
      intent.origin,
    );
    if (!outcome.open) return outcome.result;
    const { gate } = outcome;
    return new Promise<CeremonyResult>((resolve) => {
      setQueue((q) => [...q, { intent, gate, resolve }]);
    });
  }, []);

  /**
   * Answer an agent's escalation. The decision is already on the chain, so this
   * opens the dialog directly on it — no second evaluation, no second record
   * for the same act. Signing approves it; rejecting refuses it.
   */
  const review = useCallback(async (pa: PendingApproval): Promise<CeremonyResult> => {
    const gate: GateDecision = {
      decisionId: pa.decision,
      verdict: "require-approval",
      hic: "1",
      grantId: null,
      reason: "escalated by an agent — awaiting your decision",
      chainHead: "",
      ungoverned: false,
    };
    const intent: SignatureIntent = {
      kind: "grant",
      title: `${pa.agent} — approve ${pa.actionClass}?`,
      origin: `agent:${pa.agent}`,
      action: {
        actionClass: pa.actionClass,
        classification: pa.classification,
        agent: pa.agent,
        principal: pa.principal ?? undefined,
        cost: pa.cost,
        correlationId: pa.correlationId,
      },
      rows: [
        { k: "Agent", v: pa.agent },
        { k: "Action", v: pa.actionClass },
        { k: "Classification", v: pa.classification },
        ...(pa.cost ? [{ k: "Cost", v: String(pa.cost) }] : []),
        { k: "On behalf of", v: pa.principal ?? "— no principal resolved" },
        { k: "Correlation", v: pa.correlationId || "—" },
        { k: "Decision", v: `#${pa.decision} — already recorded, awaiting you` },
      ],
    };
    return new Promise<CeremonyResult>((resolve) => {
      setQueue((q) => [...q, { intent, gate, answering: pa, resolve }]);
    });
  }, []);

  const api = useMemo<CeremonyApi>(() => ({ request, review }), [request, review]);

  const clearTimers = () => {
    timers.current.forEach(clearTimeout);
    timers.current = [];
  };

  const finish = (outcome: "settled" | "rejected", note?: string) => {
    if (!head) return;
    if (outcome === "rejected" && head.answering) {
      // A refusal is an act of governance: it is recorded and it refunds what
      // the decision charged. Failing to record it must be visible.
      void bridge.policy.reject(head.gate.decisionId).catch((e: unknown) => {
        setCommitError(e instanceof Error ? e.message : String(e));
      });
    }
    head.resolve({ outcome, note, gate: head.gate });
    clearTimers();
    setQueue((q) => q.slice(1));
    setPhase("review");
    setAck(false);
    setSettledNote("");
    setCommitError("");
  };

  const onSign = async () => {
    if (!head) return;
    if (head.intent.rawUnverified && !ack) return; // Sign is gated on the ack
    setCommitError("");
    setPhase("signing");

    // Answering an escalation COMMITS here, before anything claims to be on
    // record. Previously the approval was fired at close and its failure
    // swallowed: the operator saw "On record" while the agent stayed blocked
    // and the queue never cleared. A stamp that can be wrong is worse than no
    // stamp.
    if (head.answering) {
      try {
        await bridge.policy.approve(head.gate.decisionId, principal.current ?? "");
      } catch (e) {
        setPhase("review");
        setCommitError(e instanceof Error ? e.message : String(e));
        return;
      }
    }
    // signing → broadcasting → settled. Broadcasting compresses the real
    // checkpoint-finality wait (~25s / 50 blocks, BFT 67%); the Tauri adapter
    // keeps this loader and reports real progress (DESIGN_NOTES TODO(wire)).
    timers.current.push(
      setTimeout(() => setPhase("broadcasting"), 1000),
      setTimeout(() => {
        setPhase("settled");
        // The real thing that happened: the gate appended this decision to the
        // tenant's evidence chain, at this head. Nothing claims an anchor —
        // there is no chain to anchor to yet (see the sub-line below).
        // A human acting directly has no head to quote here: the act is
        // recorded by the command that commits it, not by the gate.
        setSettledNote(
          head.gate.chainHead
            ? `decision recorded · chain head ${shortHead(head.gate.chainHead)}`
            : "recorded against your name in this tenant's evidence chain",
        );
      }, 2400),
    );
  };

  const onReject = () => finish("rejected");
  const onNext = () => {
    // head resolves on close/next; advancing settles the current one.
    finish("settled", settledNote);
  };

  if (!head) {
    return <Ctx.Provider value={api}>{children}</Ctx.Provider>;
  }

  const intent = head.intent;
  const gate = head.gate;
  const gateStyle = VERDICT_STYLE[gate.verdict];
  const originColor = ORIGIN_COLOR[intent.origin] ?? "var(--tx-2)";
  const phaseIdx = PHASES.findIndex((p) => p.key === (phase === "rejected" ? "review" : phase));
  const inReview = phase === "review";
  const isBusy = phase === "signing" || phase === "broadcasting";
  const isSettled = phase === "settled";
  const signDisabled = Boolean(intent.rawUnverified) && !ack;
  const signersMet =
    intent.signers && intent.threshold
      ? intent.signers.filter((s) => s.signed).length + 1 >= intent.threshold
      : true;

  return (
    <Ctx.Provider value={api}>
      {children}
      <div
        style={{ position: "fixed", inset: 0, background: "rgba(14,15,12,.5)", zIndex: 100, display: "flex", alignItems: "center", justifyContent: "center", padding: 24 }}
        role="dialog"
        aria-modal="true"
        aria-label="Signature ceremony"
      >
        <div data-register="charter" style={{ width: 640, maxHeight: "90vh", overflow: "auto", background: "#ffffff", color: "var(--tx-1)", borderRadius: 12, boxShadow: "0 24px 64px rgba(14,15,12,.35)", display: "flex", flexDirection: "column" }}>
          {/* header */}
          <div style={{ display: "flex", alignItems: "center", gap: 10, padding: "16px 20px", borderBottom: "2px solid var(--line-strong)" }}>
            <img src={markBlack} alt="" style={{ width: 20, height: 20 }} />
            <span className="mono" style={{ fontSize: 11, fontWeight: 500, letterSpacing: ".16em", textTransform: "uppercase" }}>Signature ceremony</span>
            {queue.length > 1 && (
              <span className="mono tabular" style={{ fontSize: 9.5, color: "var(--tx-3)", border: "1px solid var(--line-2)", padding: "1px 7px", borderRadius: 999 }}>queue {queue.length}</span>
            )}
            <div style={{ flex: 1 }} />
            <span className="mono" style={{ fontSize: 9.5, letterSpacing: ".1em", textTransform: "uppercase", color: originColor, border: `1px solid ${originColor}`, padding: "2px 8px" }}>{intent.origin}</span>
          </div>
          {/* phase strip */}
          <div style={{ display: "flex", borderBottom: "1px solid var(--line-1)" }}>
            {PHASES.map((p, i) => {
              const active = i === phaseIdx;
              const done = i < phaseIdx;
              return (
                <div key={p.key} className="mono" style={{ flex: 1, textAlign: "center", fontSize: 9, letterSpacing: ".13em", textTransform: "uppercase", padding: "8px 4px", color: active ? "var(--tx-1)" : done ? "var(--ok)" : "var(--tx-3)", borderBottom: `2px solid ${active ? "var(--accent)" : done ? "var(--ok)" : "transparent"}` }}>{p.label}</div>
              );
            })}
          </div>
          {/* body */}
          <div style={{ padding: "18px 20px", display: "flex", flexDirection: "column", gap: 14 }}>
            <div style={{ fontFamily: "var(--font-display)", fontWeight: 440, fontSize: 20, letterSpacing: "-0.008em" }}>{intent.title}</div>

            {/* The policy verdict — recorded BEFORE this dialog opened, read
                back from the decision that was appended to the chain. */}
            <div style={{ border: `1px solid ${gateStyle.color}`, background: gate.ungoverned ? "var(--danger-bg)" : "var(--srf-inset)" }}>
              <div style={{ display: "flex", alignItems: "center", gap: 10, padding: "9px 12px", borderBottom: "1px solid var(--line-1)" }}>
                <span className="mono" style={{ fontSize: 9.5, letterSpacing: ".13em", textTransform: "uppercase", color: gateStyle.color }}>{gateStyle.label}</span>
                <span className="mono" style={{ fontSize: 9.5, letterSpacing: ".1em", padding: "1px 7px", border: `1px solid ${gateStyle.color}`, color: gateStyle.color }}>HIC-{gate.hic}</span>
                <div style={{ flex: 1 }} />
                <span className="mono" style={{ fontSize: 10, color: "var(--tx-3)" }}>{gate.grantId ?? "no grant"}</span>
              </div>
              <div style={{ padding: "9px 12px", display: "flex", flexDirection: "column", gap: 4 }}>
                <span style={{ fontSize: 12.5, lineHeight: 1.5 }}>
                  {gate.ungoverned
                    ? "No live grant covers this action. It has been recorded as ungoverned and surfaced — signing it does not make it governed."
                    : gate.reason}
                </span>
                <span className="mono" style={{ fontSize: 9.5, color: "var(--tx-3)" }}>
                  {intent.action.actionClass} · {intent.action.classification}
                  {/* An answered escalation was recorded earlier, so this
                      dialog has no head of its own to quote — say nothing
                      rather than "recorded at " with a blank after it. */}
                  {gate.chainHead
                    ? ` · recorded at ${shortHead(gate.chainHead)}`
                    : gate.decisionId >= 0
                      ? ` · decision #${gate.decisionId}, already on the chain`
                      : " · recorded when you sign"}
                </span>
              </div>
            </div>

            {intent.rawUnverified && (
              <div style={{ border: "1px solid var(--danger)", background: "var(--danger-bg)", padding: "12px 14px", display: "flex", flexDirection: "column", gap: 8 }}>
                <span className="mono" style={{ fontSize: 10, letterSpacing: ".13em", textTransform: "uppercase", color: "var(--danger)" }}>Calldata could not be decoded</span>
                <p style={{ fontSize: 13, lineHeight: 1.5, margin: 0 }}>This request carries raw calldata that matches no known contract ABI. Quorum cannot tell you what it does. Signing it is signing something unread.</p>
                <label style={{ display: "flex", alignItems: "flex-start", gap: 8, fontSize: 12.5, cursor: "pointer" }}>
                  <input type="checkbox" checked={ack} onChange={(e) => setAck(e.target.checked)} style={{ marginTop: 2 }} disabled={!inReview} />
                  <span>I understand this calldata is raw and unverified, and I accept responsibility for what it executes.</span>
                </label>
              </div>
            )}

            {/* decoded intent rows */}
            <div style={{ border: "1px solid var(--line-1)" }}>
              {intent.rows.map((r, i) => (
                <div key={i} style={{ display: "grid", gridTemplateColumns: "150px 1fr", borderBottom: i < intent.rows.length - 1 ? "1px solid var(--line-1)" : "none" }}>
                  <span className="mono" style={{ fontSize: 9.5, letterSpacing: ".12em", textTransform: "uppercase", color: "var(--tx-3)", padding: "8px 12px", background: "var(--srf-inset)" }}>{r.k}</span>
                  <span className="mono" style={{ fontSize: 11.5, padding: "8px 12px", wordBreak: "break-all" }}>{r.v}</span>
                </div>
              ))}
              {intent.create2 && (
                <div style={{ display: "grid", gridTemplateColumns: "150px 1fr", borderTop: "1px solid var(--line-1)" }}>
                  <span className="mono" style={{ fontSize: 9.5, letterSpacing: ".12em", textTransform: "uppercase", color: "var(--accent-text)", padding: "8px 12px", background: "var(--accent-wash)" }}>Address (CREATE2)</span>
                  <span className="mono" style={{ fontSize: 11.5, padding: "8px 12px", wordBreak: "break-all", color: "var(--accent-text)" }}>{intent.create2}</span>
                </div>
              )}
            </div>

            {/* multisig envelope */}
            {intent.signers && intent.threshold && (
              <div style={{ display: "flex", flexDirection: "column", border: "1px solid var(--line-1)" }}>
                <div style={{ display: "flex", alignItems: "center", gap: 10, padding: "8px 12px", background: "var(--srf-inset)", borderBottom: "1px solid var(--line-1)" }}>
                  <span className="mono" style={{ fontSize: 9.5, letterSpacing: ".12em", textTransform: "uppercase", color: "var(--tx-3)" }}>Signer envelope · {intent.threshold}-of-{intent.signers.length + 1}</span>
                </div>
                {[{ name: "You (Rachel Ortiz)", signed: !inReview, you: true }, ...intent.signers.map((s) => ({ ...s, you: false }))].map((sg, i) => (
                  <div key={i} style={{ display: "flex", alignItems: "center", gap: 10, padding: "8px 12px", borderBottom: "1px solid var(--line-1)", background: sg.you ? "var(--srf-1)" : "transparent" }}>
                    <span style={{ width: 8, height: 8, borderRadius: 999, background: sg.signed ? "var(--ok)" : "var(--line-2)", flexShrink: 0 }} />
                    <span style={{ fontSize: 13, fontWeight: sg.you ? 600 : 400 }}>{sg.name}</span>
                    {sg.you && <span className="mono" style={{ fontSize: 9, letterSpacing: ".1em", color: "var(--accent-text)" }}>YOU</span>}
                    <div style={{ flex: 1 }} />
                    <span className="mono" style={{ fontSize: 10, color: sg.signed ? "var(--ok)" : "var(--tx-3)" }}>{sg.signed ? "signed" : "awaiting"}</span>
                  </div>
                ))}
              </div>
            )}

            {commitError && (
              <div style={{ border: "1px solid var(--danger)", background: "var(--danger-bg)", padding: "12px 14px", display: "flex", flexDirection: "column", gap: 6 }}>
                <span className="mono" style={{ fontSize: 10, letterSpacing: ".13em", textTransform: "uppercase", color: "var(--danger)" }}>Not recorded</span>
                <span style={{ fontSize: 13, lineHeight: 1.5 }}>
                  This was not written to the evidence chain, so nothing about it has changed.
                </span>
                <span className="mono" style={{ fontSize: 10.5, color: "var(--tx-2)" }}>{commitError}</span>
              </div>
            )}
            {isSettled && (
              <div className="cc-stamp" style={{ display: "flex", alignItems: "center", gap: 12, border: "1px solid var(--ok)", background: "var(--ok-bg)", padding: 14 }}>
                <span style={{ width: 28, height: 28, borderRadius: 999, border: "1.5px solid var(--ok)", color: "var(--ok)", display: "inline-flex", alignItems: "center", justifyContent: "center" }}>
                  <svg viewBox="0 0 24 24" width="15" height="15" fill="none" stroke="currentColor" strokeWidth="2.4" strokeLinecap="round"><path d="M5 12 L10 17 L19 8" /></svg>
                </span>
                <div>
                  <div style={{ fontSize: 14, fontWeight: 500 }}>On record</div>
                  <div className="mono" style={{ fontSize: 10.5, color: "var(--tx-2)" }}>{settledNote}</div>
                  {/* Rule 1: say exactly how far this went. The decision is in
                      the local evidence chain; nothing is on chain yet. */}
                  <div className="mono" style={{ fontSize: 9.5, color: "var(--tx-3)", marginTop: 2 }}>
                    local evidence chain only — signature + on-chain anchor land with QRM-S6
                  </div>
                </div>
              </div>
            )}
            {isBusy && (
              <div style={{ display: "flex", alignItems: "center", gap: 14, border: "1px solid var(--line-1)", padding: "12px 14px" }}>
                <div style={{ width: 34, height: 34, flexShrink: 0 }}><LoaderMark size={34} /></div>
                <span className="mono" style={{ fontSize: 11, color: "var(--tx-2)" }}>{phase === "signing" ? "signing — keystore in OS keyring" : "broadcasting · awaiting checkpoint finality"}</span>
              </div>
            )}
          </div>
          {/* footer */}
          <div style={{ display: "flex", alignItems: "center", gap: 10, padding: "14px 20px", borderTop: "1px solid var(--line-1)" }}>
            <span className="mono" style={{ fontSize: 9, letterSpacing: ".1em", textTransform: "uppercase", color: "var(--tx-3)" }}>One approval · one signature · nothing is remembered</span>
            <div style={{ flex: 1 }} />
            {inReview && (
              <>
                <button className="btn btn-ghost" onClick={onReject}>Reject</button>
                <button className="btn btn-primary" onClick={() => void onSign()} disabled={signDisabled || !signersMet}>Sign</button>
              </>
            )}
            {commitError && (
              <div style={{ border: "1px solid var(--danger)", background: "var(--danger-bg)", padding: "12px 14px", display: "flex", flexDirection: "column", gap: 6 }}>
                <span className="mono" style={{ fontSize: 10, letterSpacing: ".13em", textTransform: "uppercase", color: "var(--danger)" }}>Not recorded</span>
                <span style={{ fontSize: 13, lineHeight: 1.5 }}>
                  This was not written to the evidence chain, so nothing about it has changed.
                </span>
                <span className="mono" style={{ fontSize: 10.5, color: "var(--tx-2)" }}>{commitError}</span>
              </div>
            )}
            {isSettled && (
              <button className="btn btn-primary" onClick={onNext}>{queue.length > 1 ? `Next in queue (${queue.length - 1})` : "Close"}</button>
            )}
          </div>
        </div>
      </div>
    </Ctx.Provider>
  );
}
