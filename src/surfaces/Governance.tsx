// citrate-quorum — Governance surface (QRM-S2D, wired to the real pipeline in
// QRM-S7.8). Three tabs: the 8-step authoring Pipeline (Ingest → Interview →
// Spec → Compile → Simulate → Ceremony → Deploy → Bind), the Protocols table,
// and the Templates catalog.
//
// **What changed in S7.8, and why it had to.** Steps 4 and 6–8 rendered a
// scripted deployment: a hardcoded template name, a hardcoded audit CID
// described as "audited", a fabricated block number labelled "anchored", a
// before/after permissions diff nobody computed, and — worst — a CREATE2
// address literal used as the fallback when the backend returned none. Every
// one of those read as live. They are replaced by the pipeline's own results:
// `deployIntent` supplies the predicted address, `deployComplete` the address
// the chain actually logged, and `bindIntent`/`bindComplete` the before/after
// verdicts read from `PolicyBinding.check`.
//
// Rule 11: every panel below names the bridge call behind it.
import { useEffect, useState } from "react";
import { bridge } from "../bridge";
import { DomainErrorPlate, useDomain } from "../components/DomainState";
import type {
  BindIntent, BindResult, CompileResult, DeployIntent, DeployResult,
  IngestFile, InterviewTurn, Simulation, SpecClause,
} from "../bridge";
import { useCeremony } from "../ceremony/Ceremony";
import { LoaderMark } from "../components/LoaderMark";

type Tab = "pipe" | "prot" | "tpl";
const STEPS = ["Ingest", "Interview", "Spec", "Compile", "Simulate", "Ceremony", "Deploy", "Bind"];
const CLS_COLOR: Record<string, string> = { Public: "var(--z-silver)", Proprietary: "var(--info)", CUI: "var(--warn)", ITAR: "var(--danger)" };
/** Turn order is the backend's topic order — `Topic::ALL`, never re-sorted. */
const TOPIC_OF = ["scope", "principals", "roles", "thresholds", "escalation", "expiry", "exceptions"];

/** A key/value panel. Every value below comes from a bridge result. */
function Rows({ rows, accent }: { rows: [string, string][]; accent?: string }) {
  return (
    <div className="surface" style={{ display: "flex", flexDirection: "column" }}>
      {rows.map(([k, v]) => (
        <div key={k} style={{ display: "grid", gridTemplateColumns: "170px 1fr", borderBottom: "1px solid var(--line-1)" }}>
          <span className="mono" style={{ fontSize: 9.5, letterSpacing: ".1em", textTransform: "uppercase", color: "var(--tx-3)", padding: "9px 14px", background: "var(--srf-inset)" }}>{k}</span>
          <span className="mono" style={{ fontSize: 11, padding: "9px 14px", wordBreak: "break-all", color: k === accent ? "var(--accent-text)" : "var(--tx-1)" }}>{v}</span>
        </div>
      ))}
    </div>
  );
}

/** An error from a pipeline call, shown verbatim. These messages are written to
 *  be read by an operator — truncating them loses the reason. */
function Why({ text }: { text: string }) {
  if (!text) return null;
  return (
    <div className="mono" style={{ fontSize: 11, lineHeight: 1.6, color: "var(--danger)", border: "1px solid var(--danger)", background: "var(--danger-bg)", padding: "10px 12px", whiteSpace: "pre-wrap" }}>{text}</div>
  );
}

export function Governance() {
  const [tab, setTab] = useState<Tab>("pipe");
  const [step, setStep] = useState(1);
  const [ingest, setIngest] = useState<IngestFile[]>([]);
  const [interview, setInterview] = useState<InterviewTurn[]>([]);
  const [clauses, setClauses] = useState<SpecClause[]>([]);
  const [sim, setSim] = useState<Simulation | null>(null);
  const [simState, setSimState] = useState<"idle" | "running" | "done">("idle");
  const [specId, setSpecId] = useState<string | null>(null);
  const [compiled, setCompiled] = useState<CompileResult | null>(null);
  const [intent, setIntent] = useState<DeployIntent | null>(null);
  const [deployRes, setDeployRes] = useState<DeployResult | null>(null);
  const [bindIntent, setBindIntent] = useState<BindIntent | null>(null);
  const [bindRes, setBindRes] = useState<BindResult | null>(null);
  const [busy, setBusy] = useState("");
  const [err, setErr] = useState("");
  // Step 1 and 2 had no inputs at all: a drop zone that accepted nothing and a
  // transcript with no way to add a turn. The pipeline could be READ from the
  // surface and finished from step 6, but never STARTED — which is the exit
  // gate ("a doc dump becomes a deployed protocol"), so it had to be built.
  const [paths, setPaths] = useState("");
  const [refused, setRefused] = useState<{ name: string; why: string }[]>([]);
  const [pending, setPending] = useState<string | null>(null);
  const [outstanding, setOutstanding] = useState<string[]>([]);
  const [answer, setAnswer] = useState("");
  // Which already-answered topic is being corrected, and with what.
  const [revising, setRevising] = useState<string | null>(null);
  const [revision, setRevision] = useState("");
  const [operator, setOperator] = useState<string | null>(null);
  const ceremony = useCeremony();

  useEffect(() => {
    // QRM-S7: the pipeline is per-spec, so the surface picks up the newest
    // draft and reads that. With no drafts these stay empty and each step
    // renders its own empty state rather than a half-populated wizard.
    bridge.governance
      .specs()
      .then((list) => {
        const first = list[0];
        if (!first) return;
        setSpecId(first.id);
        bridge.governance.spec(first.id).then((sp) => {
          setIngest(sp.files);
          setClauses(sp.clauses);
        }).catch(() => {});
        bridge.governance.interview(first.id).then((iv) => {
          setInterview(iv.turns);
          setPending(iv.pending);
          setOutstanding(iv.outstanding);
        }).catch(() => {});
      })
      .catch(() => {});
    // Whoever this installation is signed in as. The interview transcript used
    // to label every answer "R. ORTIZ" regardless.
    bridge.session.operator().then(setOperator).catch(() => {});
  }, []);

  // Honest failure (S2D.4/§5.1): this surface's primary read is governance.protocols().
  // A read that cannot succeed must say so and offer a retry, not sit in a
  // loading state forever.
  const primary = useDomain(() => bridge.governance.protocols(), "governance.protocols()");
  // Derived, not mirrored (see Agents.tsx).
  const protocols = primary.state.status === "ready" ? primary.state.data : [];

  /**
   * Stage 1. Read the operator's chosen files from disk and classify them.
   *
   * A file whose marking cannot be determined comes back in `refused` and is
   * SHOWN — the marking is what decides which model may read the content, so a
   * document accepted with an assumed classification is the one failure this
   * step exists to prevent.
   */
  const runIngest = async () => {
    const list = paths.split("\n").map((s) => s.trim()).filter(Boolean);
    if (!list.length) return;
    setErr(""); setBusy("reading and classifying");
    try {
      const r = await bridge.governance.ingest(specId ? { paths: list, specId } : { paths: list });
      setSpecId(r.specId);
      setIngest(r.files);
      setRefused(r.refused);
      const iv = await bridge.governance.interview(r.specId);
      setInterview(iv.turns); setPending(iv.pending); setOutstanding(iv.outstanding);
    } catch (e) {
      setErr(e instanceof Error ? e.message : String(e));
    } finally {
      setBusy("");
    }
  };

  /**
   * Correct an answer already given.
   *
   * The live run needed this: an answer that names a person where the template
   * needs a duration cannot be typed, so the clause is unmapped and the spec
   * cannot deploy — permanently, because a first answer used to be final. The
   * prior answer is superseded, not erased.
   */
  const reviseTopic = async (topic: string) => {
    if (!specId || !revision.trim()) return;
    setErr("");
    try {
      const iv = await bridge.governance.interview(specId, revision.trim(), topic);
      setInterview(iv.turns); setPending(iv.pending); setOutstanding(iv.outstanding);
      setRevising(null); setRevision("");
      const sp = await bridge.governance.spec(specId);
      setClauses(sp.clauses);
      // A correction usually exists BECAUSE the compile refused, so re-run it
      // rather than leaving a stale refusal on screen.
      if (compiled) setCompiled(await bridge.governance.compile(specId));
    } catch (e) {
      setErr(e instanceof Error ? e.message : String(e));
    }
  };

  /** Stage 2. One turn. The backend stamps WHO answered — not this field. */
  const answerTopic = async () => {
    if (!specId || !answer.trim()) return;
    setErr("");
    try {
      const iv = await bridge.governance.interview(specId, answer.trim());
      setInterview(iv.turns); setPending(iv.pending); setOutstanding(iv.outstanding);
      setAnswer("");
      // Re-read the spec. The clauses ARE the answers — a spec read once at
      // mount is a spec from before the interview, which rendered step 3 as an
      // empty clause table beside a spec that existed and had clauses.
      const sp = await bridge.governance.spec(specId);
      setClauses(sp.clauses);
    } catch (e) {
      setErr(e instanceof Error ? e.message : String(e));
    }
  };

  /** Map the spec onto the audited template set the registry actually holds. */
  const runCompile = async () => {
    if (!specId) return;
    setErr("");
    try {
      setCompiled(await bridge.governance.compile(specId));
    } catch (e) {
      setCompiled(null);
      setErr(e instanceof Error ? e.message : String(e));
    }
  };

  const runSim = () => {
    setSimState("running");
    bridge.governance
      .simulate(specId ?? "", { from: "", to: "" })
      .then((s) => { setSim(s); setSimState("done"); })
      .catch(() => setSimState("idle"));
  };

  /**
   * Build the deploy intent. **Signs nothing.** Called when the operator opens
   * step 6, so the predicted address exists before the ceremony does — GF-1 is
   * "approve a known address", and an address fetched after the dialog opened
   * would be approved sight-unseen.
   */
  const prepareDeploy = async () => {
    if (!specId) return;
    setErr(""); setBusy("building the deploy intent — signing nothing");
    try {
      setIntent(await bridge.governance.deployIntent(specId));
    } catch (e) {
      setIntent(null);
      setErr(e instanceof Error ? e.message : String(e));
    } finally {
      setBusy("");
    }
  };

  /**
   * Run the ceremony on the intent, then ask the chain what it built.
   *
   * The ceremony's `chainTx` is what signs and broadcasts — the single signing
   * path (rule 3). `deployComplete` signs nothing: it takes the hash that path
   * produced and REJECTS if the address the factory logged is not the one shown
   * below. So reaching step 7 is itself the proof the addresses agreed.
   */
  const openDeploy = async () => {
    if (!intent) return;
    setErr("");
    const r = await ceremony.request({
      kind: "deploy",
      title: `Deploy ${intent.templateName} — ${intent.specId}`,
      origin: "user",
      // Deploying a governance protocol is chain state: always HIC-1.
      action: { actionClass: "protocol.deploy", classification: intent.classification, agent: "user", mandatoryHic1: true },
      create2: intent.predictedAddress,
      rows: [
        { k: "Template", v: `${intent.templateName} · ${intent.templateId.slice(0, 10)}…` },
        { k: "Audit CID", v: "devnet CID — UNAUDITED (D-2)" },
        { k: "Spec", v: `${intent.specId} · ${intent.specCID}` },
        { k: "Tenant", v: `${intent.tenantName} · ${intent.tenantId.slice(0, 10)}…` },
        { k: "Classification", v: intent.classification },
        { k: "Approvers", v: intent.approvers.length ? intent.approvers.join(", ") : "none named" },
        { k: "Will bind to", v: intent.actionClass ?? "nothing — the spec names no scope" },
      ],
      chainTx: {
        label: "deployed through GovernanceProtocolFactory",
        // The kit ceremony was created by `deployIntent`; this hands back its
        // id rather than building a second one, so the transaction that gets
        // signed is the one whose address was predicted above.
        prepare: async () => ({ id: intent.ceremonyId }),
      },
    });
    if (r.outcome !== "settled") return;
    if (!r.chain) {
      setErr("the ceremony settled but broadcast no transaction, so there is nothing to verify. Nothing was deployed.");
      return;
    }
    setBusy("checking what the chain deployed");
    try {
      setDeployRes(await bridge.governance.deployComplete(intent.ceremonyId, r.chain.txHash));
      setStep(7);
    } catch (e) {
      setErr(e instanceof Error ? e.message : String(e));
    } finally {
      setBusy("");
    }
  };

  /** Read what `check` says today, before any binding. Signs nothing. */
  const prepareBind = async () => {
    if (!deployRes?.actionClass) return;
    setErr(""); setBusy("reading PolicyBinding.check — signing nothing");
    try {
      setBindIntent(await bridge.governance.bindIntent(deployRes.address, deployRes.actionClass));
    } catch (e) {
      setBindIntent(null);
      setErr(e instanceof Error ? e.message : String(e));
    } finally {
      setBusy("");
    }
  };

  const openBind = async () => {
    if (!bindIntent) return;
    setErr("");
    const r = await ceremony.request({
      kind: "deploy",
      title: `Bind ${bindIntent.actionClass} — ${bindIntent.tenantName}`,
      origin: "user",
      action: { actionClass: "protocol.bind", classification: deployRes?.classification ?? "Public", agent: "user", mandatoryHic1: true },
      rows: [
        { k: "Protocol", v: bindIntent.protocol },
        { k: "Action class", v: `${bindIntent.actionClass} · ${bindIntent.actionClassId.slice(0, 10)}…` },
        { k: "Tenant", v: bindIntent.tenantName },
        { k: "Verdict today", v: `${bindIntent.before.verdict} / ${bindIntent.before.reason}${bindIntent.before.ungoverned ? " — nobody has bound anything" : ""}` },
      ],
      chainTx: {
        label: "bound through PolicyBinding",
        prepare: async () => ({ id: bindIntent.ceremonyId }),
      },
    });
    if (r.outcome !== "settled") return;
    if (!r.chain) {
      setErr("the ceremony settled but broadcast no transaction. Nothing is bound.");
      return;
    }
    setBusy("re-reading PolicyBinding.check");
    try {
      setBindRes(await bridge.governance.bindComplete(bindIntent.ceremonyId, r.chain.txHash));
    } catch (e) {
      setErr(e instanceof Error ? e.message : String(e));
    } finally {
      setBusy("");
    }
  };

  const tabBtn = (t: Tab, label: string) => {
    const on = tab === t;
    return (
      <button key={t} onClick={() => setTab(t)} className="mono" style={{ fontSize: 10, letterSpacing: ".13em", textTransform: "uppercase", background: "transparent", border: "none", cursor: "pointer", padding: "8px 14px", color: on ? "var(--tx-1)" : "var(--tx-3)", borderBottom: `2px solid ${on ? "var(--accent)" : "transparent"}`, marginBottom: -1 }}>{label}</button>
    );
  };

  // The structured summary IS the interview's answers. It was six hardcoded
  // rows describing a company that does not exist — beside a panel showing the
  // operator's real answers, which made the fabricated half look like the
  // system's own conclusions about them.
  const summary: [string, string][] = interview
    .filter((iv) => iv.a)
    .map((iv) => [iv.q, iv.a]);

  // Placed after EVERY hook: an early return above a useMemo makes the
  // hook run conditionally, and React crashes with "rendered fewer hooks
  // than expected" the moment this read fails.
  if (primary.state.status === "error") {
    return (
      <div style={{ padding: 18 }}>
        {/* No `lands` prop, deliberately. Passing it makes the plate announce
            "Not wired yet" and explain that "there is nothing real to show" —
            which was true until S7.8 and is now a false statement about a wired
            surface. It also buried the actual error: a real install showed this
            plate saying the surface was unbuilt when what had really happened
            was `no tenant scope is established`. Without `lands`, the plate says
            the read failed and shows the reason, which is the useful sentence. */}
        <DomainErrorPlate source="governance.protocols()" error={primary.state.error} onRetry={primary.retry} />
      </div>
    );
  }

  return (
    <div style={{ padding: "22px 26px", display: "flex", flexDirection: "column", gap: 16, maxWidth: 1120 }}>
      <div style={{ display: "flex", borderBottom: "1px solid var(--line-2)" }}>
        {tabBtn("pipe", "Pipeline")}{tabBtn("prot", "Protocols")}{tabBtn("tpl", "Templates")}
      </div>

      {tab === "pipe" && (
        <div style={{ display: "flex", flexDirection: "column", gap: 16 }}>
          {/* step rail */}
          <div style={{ display: "flex" }}>
            {STEPS.map((label, i) => {
              const n = i + 1; const active = n === step; const done = n < step;
              const color = active ? "var(--accent-text)" : done ? "var(--ok)" : "var(--tx-3)";
              return (
                <div key={label} onClick={() => n <= step && setStep(n)} style={{ flex: 1, display: "flex", flexDirection: "column", gap: 4, cursor: n <= step ? "pointer" : "default", padding: "6px 8px", borderTop: `2px solid ${active ? "var(--accent)" : done ? "var(--ok)" : "var(--line-2)"}` }}>
                  <span className="mono tabular" style={{ fontSize: 9, color }}>{String(n).padStart(2, "0")}</span>
                  <span className="mono" style={{ fontSize: 9, letterSpacing: ".1em", textTransform: "uppercase", color }}>{label}</span>
                </div>
              );
            })}
          </div>

          {step === 1 && (
            <div style={{ display: "flex", flexDirection: "column", gap: 12 }}>
              <div style={{ fontFamily: "var(--font-display)", fontWeight: 440, fontSize: 22 }}>A stack of documents becomes law</div>
              <span className="mono" style={{ fontSize: 9.5, color: "var(--tx-3)" }}>governance.ingest() — read from this machine, never uploaded</span>
              {/* A path field, not a drop zone. `governance.ingest` takes absolute
                  paths and reads them locally; there is no file-dialog plugin in
                  this app and adding one is a dependency plus a capability grant.
                  The prototype's "drop your documents here" panel accepted nothing
                  and called nothing — it was the only thing on this step. */}
              <textarea
                value={paths}
                onChange={(e) => setPaths(e.target.value)}
                spellCheck={false}
                placeholder={"/absolute/path/to/policy.md\none path per line — .md .txt .csv .json .yaml and friends"}
                style={{ fontFamily: "var(--font-mono)", fontSize: 11.5, lineHeight: 1.6, padding: "10px 12px", minHeight: 76, background: "var(--srf-inset)", color: "var(--tx-1)", border: "1px solid var(--line-2)", resize: "vertical" }}
              />
              <div style={{ display: "flex", alignItems: "center", gap: 12 }}>
                <button className="btn btn-primary" onClick={runIngest} disabled={!paths.trim() || busy !== ""}>Ingest</button>
                {busy && <span className="mono" style={{ fontSize: 10.5, color: "var(--tx-3)" }}>{busy}…</span>}
                {specId && <span className="mono" style={{ fontSize: 10.5, color: "var(--tx-3)" }}>spec {specId}</span>}
              </div>
              <Why text={err} />

              {ingest.length > 0 && (
                <div className="surface" style={{ display: "flex", flexDirection: "column" }}>
                  {ingest.map((f) => (
                    <div key={f.name} style={{ display: "flex", alignItems: "center", gap: 12, padding: "10px 14px", borderBottom: "1px solid var(--line-1)" }}>
                      <span style={{ fontSize: 13, fontWeight: 500, minWidth: 0, flex: 1 }}>{f.name} <span className="mono" style={{ fontSize: 9.5, color: "var(--tx-3)" }}>{f.size}</span></span>
                      <span className="mono" style={{ fontSize: 8.5, letterSpacing: ".1em", textTransform: "uppercase", color: CLS_COLOR[f.class], border: `1px solid ${CLS_COLOR[f.class]}`, padding: "1px 6px" }}>{f.class}</span>
                      <span className="mono" style={{ fontSize: 9.5, color: f.class === "ITAR" ? "var(--danger)" : "var(--tx-3)" }}>{f.note}</span>
                      <span className="mono" style={{ fontSize: 9, color: "var(--tx-3)" }}>{f.prov}</span>
                    </div>
                  ))}
                </div>
              )}

              {/* A refusal is the product working. An unmarked document has no
                  classification, and classification is what decides which model
                  may read it — so it is shown, named, and not quietly dropped. */}
              {refused.map((r) => (
                <div key={r.name} style={{ border: "1px solid var(--warn)", background: "var(--warn-bg)", padding: "10px 12px", fontSize: 11.5, lineHeight: 1.55 }}>
                  <span className="mono" style={{ fontSize: 9, letterSpacing: ".1em", color: "var(--warn)" }}>REFUSED · {r.name} · </span>{r.why}
                </div>
              ))}

              <div style={{ display: "flex" }}><div style={{ flex: 1 }} /><button className="btn btn-primary" onClick={() => setStep(2)} disabled={!specId}>Continue to interview</button></div>
            </div>
          )}

          {step === 2 && (
            <div style={{ display: "grid", gridTemplateColumns: "1fr 340px", gap: 16 }}>
              <div className="surface" style={{ display: "flex", flexDirection: "column" }}>
                <div className="eyebrow" style={{ padding: "10px 14px", borderBottom: "1px solid var(--line-1)" }}>Interview — the harness asks, you answer</div>
                <div style={{ padding: 14, display: "flex", flexDirection: "column", gap: 12 }}>
                  {interview.map((iv, i) => (
                    <div key={i} style={{ display: "flex", flexDirection: "column", gap: 5 }}>
                      <div style={{ display: "flex", gap: 8 }}><span className="mono" style={{ fontSize: 9, color: "var(--info)", border: "1px solid var(--info)", padding: "1px 6px", height: "fit-content", flexShrink: 0 }}>HARNESS</span><span style={{ fontSize: 13, color: "var(--tx-2)" }}>{iv.q}</span></div>
                      {/* The answering label was the constant "R. ORTIZ" — a name
                          from the design prototype's fictional customer, printed
                          over whoever actually answered. It is the operator this
                          installation is signed in as, which is also the only
                          name the backend will record. */}
                      <div style={{ display: "flex", gap: 8, alignItems: "baseline" }}>
                        <span className="mono" style={{ fontSize: 9, color: "var(--accent-text)", border: "1px solid var(--accent-text)", padding: "1px 6px", height: "fit-content", flexShrink: 0, textTransform: "uppercase" }}>{operator ?? "operator"}</span>
                        <span style={{ fontSize: 13, flex: 1 }}>{iv.a}</span>
                        {iv.a && TOPIC_OF[i] && (
                          <button className="btn btn-ghost btn-sm" style={{ fontSize: 10 }} onClick={() => { setRevising(TOPIC_OF[i]); setRevision(iv.a); }}>Correct</button>
                        )}
                      </div>
                      {revising === TOPIC_OF[i] && (
                        <div style={{ display: "flex", flexDirection: "column", gap: 6, paddingLeft: 8, borderLeft: "2px solid var(--accent)" }}>
                          <textarea
                            value={revision}
                            onChange={(e) => setRevision(e.target.value)}
                            style={{ fontFamily: "var(--font-sans)", fontSize: 12.5, lineHeight: 1.6, padding: "8px 10px", minHeight: 54, background: "var(--srf-inset)", color: "var(--tx-1)", border: "1px solid var(--line-2)", resize: "vertical" }}
                          />
                          <div style={{ display: "flex", gap: 8, alignItems: "center" }}>
                            <button className="btn btn-primary btn-sm" onClick={() => reviseTopic(TOPIC_OF[i])} disabled={!revision.trim()}>Save correction</button>
                            <button className="btn btn-ghost btn-sm" onClick={() => { setRevising(null); setRevision(""); }}>Cancel</button>
                            {/* An interview is evidence. Correcting it is allowed;
                                pretending the first answer never happened is not. */}
                            <span className="mono" style={{ fontSize: 9.5, color: "var(--tx-3)" }}>the previous answer is kept, superseded</span>
                          </div>
                        </div>
                      )}
                    </div>
                  ))}
                  {interview.length === 0 && (
                    <span className="mono" style={{ fontSize: 11, color: "var(--tx-3)" }}>Nothing asked yet. Ingest a document first — the interview is per-spec.</span>
                  )}
                </div>

                {pending && (
                  <div style={{ borderTop: "1px solid var(--line-1)", padding: 14, display: "flex", flexDirection: "column", gap: 8 }}>
                    <div style={{ display: "flex", gap: 8 }}>
                      <span className="mono" style={{ fontSize: 9, color: "var(--info)", border: "1px solid var(--info)", padding: "1px 6px", height: "fit-content", flexShrink: 0 }}>HARNESS</span>
                      <span style={{ fontSize: 13, color: "var(--tx-1)" }}>{pending}</span>
                    </div>
                    <textarea
                      value={answer}
                      onChange={(e) => setAnswer(e.target.value)}
                      placeholder="in your own words — recorded against your name"
                      style={{ fontFamily: "var(--font-sans)", fontSize: 12.5, lineHeight: 1.6, padding: "9px 11px", minHeight: 62, background: "var(--srf-inset)", color: "var(--tx-1)", border: "1px solid var(--line-2)", resize: "vertical" }}
                    />
                    <div style={{ display: "flex", alignItems: "center", gap: 10 }}>
                      <button className="btn btn-primary btn-sm" onClick={answerTopic} disabled={!answer.trim()}>Answer</button>
                      {/* Skipping is a real outcome, not a failure: an unanswered
                          topic produces NO clause, so the policy simply does not
                          cover it. That is honest, and better than an answer
                          somebody typed to move the wizard along. */}
                      <button className="btn btn-ghost btn-sm" onClick={() => setStep(3)}>Stop here — the rest goes uncovered</button>
                      <span className="mono" style={{ fontSize: 9.5, color: "var(--tx-3)" }}>{outstanding.length} topic(s) still unanswered</span>
                    </div>
                    <Why text={err} />
                  </div>
                )}
                {!pending && interview.length > 0 && (
                  <div style={{ borderTop: "1px solid var(--line-1)", padding: 14 }}>
                    <span className="mono" style={{ fontSize: 10.5, color: "var(--ok)" }}>every topic answered</span>
                  </div>
                )}
              </div>

              <div className="surface" style={{ display: "flex", flexDirection: "column", height: "fit-content", borderTop: "2px solid var(--line-strong)" }}>
                <div className="eyebrow" style={{ padding: "10px 14px", borderBottom: "1px solid var(--line-1)" }}>Structured summary — your answers, nothing added</div>
                <div style={{ padding: 14, display: "flex", flexDirection: "column", gap: 8 }}>
                  {summary.length === 0 && <span className="mono" style={{ fontSize: 11, color: "var(--tx-3)" }}>No answers yet.</span>}
                  {summary.map(([k, v]) => (
                    <div key={k} style={{ display: "grid", gridTemplateColumns: "110px 1fr", gap: 8, borderBottom: "1px solid var(--line-1)", paddingBottom: 6 }}>
                      <span className="mono" style={{ fontSize: 9, letterSpacing: ".1em", textTransform: "uppercase", color: "var(--tx-3)" }}>{k}</span>
                      <span style={{ fontSize: 12 }}>{v}</span>
                    </div>
                  ))}
                </div>
                <div style={{ padding: "10px 14px" }}><button className="btn btn-primary btn-sm" onClick={() => setStep(3)} style={{ width: "100%" }} disabled={!specId}>Draft the spec →</button></div>
              </div>
            </div>
          )}

          {step === 3 && (
            <div style={{ display: "flex", flexDirection: "column", gap: 12 }}>
              <div style={{ display: "flex", alignItems: "center", gap: 12 }}>
                <div style={{ fontFamily: "var(--font-display)", fontWeight: 440, fontSize: 20 }}>{specId ?? "No draft yet"}</div>
                {/* The unmapped count is COMPILE's answer, not a label. It was
                    the constant "1 CLAUSE UNMAPPED" beside whatever the spec
                    actually contained. */}
                {clauses.some((c) => !c.ok) && (
                  <span className="mono" style={{ fontSize: 9, color: "var(--warn)", border: "1px solid var(--warn)", padding: "2px 7px" }}>
                    {clauses.filter((c) => !c.ok).length} CLAUSE(S) WITHOUT A TEMPLATE — DEPLOY BLOCKED
                  </span>
                )}
              </div>
              <div style={{ border: "1px solid var(--line-2)" }}>
                <div style={{ display: "grid", gridTemplateColumns: "1fr 1fr", borderBottom: "1px solid var(--line-2)" }}>
                  <div className="mono" style={{ fontSize: 9, letterSpacing: ".13em", textTransform: "uppercase", color: "var(--tx-3)", padding: "8px 14px", borderRight: "1px solid var(--line-2)" }}>Plain English — what counsel reads</div>
                  <div className="mono" style={{ fontSize: 9, letterSpacing: ".13em", textTransform: "uppercase", color: "var(--tx-3)", padding: "8px 14px" }}>Machine-checkable — what the chain enforces</div>
                </div>
                {clauses.map((cl) => (
                  <div key={cl.n} style={{ display: "grid", gridTemplateColumns: "1fr 1fr", borderBottom: "1px solid var(--line-1)", background: cl.ok ? "transparent" : "var(--warn-bg)" }}>
                    <div style={{ padding: "11px 14px", borderRight: "1px solid var(--line-2)" }}>
                      <p style={{ fontSize: 13, lineHeight: 1.55, margin: 0 }}><span className="mono" style={{ color: cl.ok ? "var(--ok)" : "var(--warn)", marginRight: 6 }}>{cl.n}</span>{cl.en}</p>
                      {!cl.ok && (
                        <div style={{ marginTop: 8, border: "1px solid var(--warn)", background: "var(--warn-bg)", padding: "7px 10px", fontSize: 11.5 }}><span className="mono" style={{ fontSize: 9, letterSpacing: ".1em", color: "var(--warn)" }}>NO AUDITED TEMPLATE · </span>{cl.why}</div>
                      )}
                    </div>
                    <div className="mono" style={{ padding: "11px 14px", fontSize: 10.5, lineHeight: 1.6, color: cl.ok ? "var(--tx-2)" : "var(--tx-3)", whiteSpace: "pre-wrap" }}>{cl.gh}{cl.tpl && <span style={{ display: "block", marginTop: 6, fontSize: 8.5, color: "var(--tx-3)" }}>↳ {cl.tpl}</span>}</div>
                  </div>
                ))}
              </div>
              <div style={{ display: "flex", alignItems: "center", gap: 10 }}>
                <span className="mono" style={{ fontSize: 9.5, color: "var(--tx-3)" }}>Each clause carries its Gherkin and its typed parameters, rendered from the same values — they cannot disagree. A clause that maps to no audited template is flagged, never dropped silently.</span>
                <div style={{ flex: 1 }} /><button className="btn btn-primary" onClick={() => setStep(4)}>Compile against the audited templates</button>
              </div>
            </div>
          )}

          {step === 4 && (
            <div style={{ display: "flex", flexDirection: "column", gap: 12, maxWidth: 820 }}>
              <div style={{ fontFamily: "var(--font-display)", fontWeight: 440, fontSize: 20 }}>Compile — audited templates, or a refusal</div>
              <span className="mono" style={{ fontSize: 9.5, color: "var(--tx-3)" }}>governance.compile() → GovernanceTemplateRegistry on chain 40204</span>
              {!compiled && <button className="btn btn-primary" style={{ width: "fit-content" }} onClick={runCompile}>Compile this spec</button>}
              {compiled && (
                <>
                  <span className="mono" style={{ fontSize: 9.5, letterSpacing: ".08em", textTransform: "uppercase", width: "fit-content", padding: "2px 7px", border: `1px solid ${compiled.deployable ? "var(--ok)" : "var(--warn)"}`, color: compiled.deployable ? "var(--ok)" : "var(--warn)" }}>
                    {compiled.deployable ? `${compiled.mapped.length} clause(s) mapped — deployable` : `${compiled.unmapped.length} clause(s) unmapped — DEPLOY BLOCKED`}
                  </span>
                  {compiled.mapped.map((m) => (
                    <div key={m.clause} className="surface" style={{ padding: "10px 14px", display: "flex", flexDirection: "column", gap: 4 }}>
                      <span className="mono" style={{ fontSize: 10.5, color: "var(--ok)" }}>clause {m.clause} → template {m.templateId.slice(0, 14)}…</span>
                      <span className="mono" style={{ fontSize: 10.5, color: "var(--tx-2)" }}>
                        {Object.entries(m.params).map(([k, v]) => `${k} = ${v}`).join(" · ") || "no parameters"}
                      </span>
                    </div>
                  ))}
                  {/* A waiver is shown, not hidden. "This topic imposes no rule"
                      is a decision a reader should see, and seeing the words the
                      human wrote is how they tell it from a clause that was
                      dropped. */}
                  {compiled.waived.map((w) => (
                    <div key={w.clause} className="surface" style={{ padding: "10px 14px", display: "flex", gap: 10, alignItems: "baseline" }}>
                      <span className="mono" style={{ fontSize: 9, letterSpacing: ".1em", textTransform: "uppercase", color: "var(--tx-3)", border: "1px solid var(--line-2)", padding: "1px 6px" }}>waived</span>
                      <span className="mono" style={{ fontSize: 10.5, color: "var(--tx-2)" }}>clause {w.clause} · {w.topic} — you wrote &ldquo;{w.said}&rdquo;, so no protocol is deployed for it</span>
                    </div>
                  ))}
                  {/* R-A: an unmapped clause is a hard output, never a warning
                      that can be clicked through. It blocks the deploy. */}
                  {compiled.unmapped.map((u) => (
                    <div key={u.clause} style={{ border: "1px solid var(--warn)", background: "var(--warn-bg)", padding: "10px 12px", fontSize: 11.5, lineHeight: 1.55 }}>
                      <span className="mono" style={{ fontSize: 9, letterSpacing: ".1em", color: "var(--warn)" }}>CLAUSE {u.clause} MAPS TO NOTHING · </span>{u.why}
                    </div>
                  ))}
                  <div style={{ display: "flex" }}><div style={{ flex: 1 }} /><button className="btn btn-primary" onClick={() => setStep(5)}>Simulate against recorded decisions</button></div>
                </>
              )}
            </div>
          )}
          {step === 5 && (
            <div style={{ display: "flex", flexDirection: "column", gap: 14 }}>
              <div style={{ display: "flex", alignItems: "center", gap: 14 }}>
                <div style={{ fontFamily: "var(--font-display)", fontWeight: 440, fontSize: 20 }}>What this policy would have done</div>
                <span className="mono" style={{ fontSize: 10, color: "var(--tx-3)" }}>{sim?.range ?? "last 90 days"}</span>
                <div style={{ flex: 1 }} />
                {simState === "idle" && <button className="btn btn-primary" onClick={runSim}>Run simulation</button>}
              </div>
              {simState === "running" && (
                <div className="surface" style={{ display: "flex", alignItems: "center", gap: 16, padding: 18 }}>
                  <div style={{ width: 44, height: 44, flexShrink: 0 }}><LoaderMark size={44} /></div>
                  <span className="mono" style={{ fontSize: 11, color: "var(--tx-2)" }}>replaying this tenant’s recorded decisions · governance.simulate()</span>
                </div>
              )}
              {simState === "done" && sim && (
                <div style={{ display: "flex", flexDirection: "column", gap: 14 }}>
                  <div style={{ display: "grid", gridTemplateColumns: "repeat(3,1fr)", gap: 10 }}>
                    <div className="surface" style={{ padding: 14, borderTop: "2px solid var(--danger)" }}><div className="eyebrow">Would have blocked</div><div className="tabular" style={{ fontFamily: "var(--font-display)", fontSize: 34, fontWeight: 460, color: "var(--danger)" }}>{sim.blocked}</div><div className="mono" style={{ fontSize: 9.5, color: "var(--tx-3)" }}>of {(sim.blocked + sim.approvals + sim.allowed + sim.unchanged).toLocaleString("en-US")} replayed</div></div>
                    <div className="surface" style={{ padding: 14, borderTop: "2px solid var(--warn)" }}><div className="eyebrow">Would have paused for a human</div><div className="tabular" style={{ fontFamily: "var(--font-display)", fontSize: 34, fontWeight: 460, color: "var(--warn)" }}>{sim.approvals}</div><div className="mono" style={{ fontSize: 9.5, color: "var(--tx-3)" }}>would have required a human</div></div>
                    <div className="surface" style={{ padding: 14, borderTop: "2px solid var(--accent)" }}><div className="eyebrow">Unchanged</div><div className="tabular" style={{ fontFamily: "var(--font-display)", fontSize: 34, fontWeight: 460 }}>{sim.unchanged.toLocaleString("en-US")}</div><div className="mono" style={{ fontSize: 9.5, color: "var(--tx-3)" }}>the policy would not have applied</div></div>
                  </div>
                  <div style={{ display: "grid", gridTemplateColumns: "1fr 1fr", gap: 10 }}>
                    <div className="surface" style={{ padding: 14, display: "flex", flexDirection: "column", gap: 8 }}>
                      <span className="eyebrow">By action class — blocked · paused</span>
                      {sim.byClass.map(([name, blocked, paused]) => {
                        const tot = blocked + paused || 1;
                        return (
                          <div key={name} style={{ display: "flex", alignItems: "center", gap: 10 }}>
                            <span className="mono" style={{ fontSize: 10, width: 110, color: "var(--tx-2)" }}>{name}</span>
                            <div style={{ flex: 1, display: "flex", height: 10, gap: 1 }}>
                              <div style={{ width: `${(blocked / tot) * 100}%`, background: "var(--danger)" }} />
                              <div style={{ width: `${(paused / tot) * 100}%`, background: "var(--warn)" }} />
                            </div>
                            <span className="mono tabular" style={{ fontSize: 10, color: "var(--tx-3)", width: 60, textAlign: "right" }}>{blocked}·{paused}</span>
                          </div>
                        );
                      })}
                    </div>
                    <div className="surface" style={{ padding: 14, display: "flex", flexDirection: "column", gap: 8 }}>
                      <span className="eyebrow">Who this would have inconvenienced</span>
                      {sim.inconvenienced.map(([name, n, note]) => (
                        <div key={name} style={{ display: "flex", alignItems: "baseline", gap: 10, borderBottom: "1px solid var(--line-1)", paddingBottom: 5 }}>
                          <span className="mono" style={{ fontSize: 11, width: 110 }}>{name}</span>
                          <span className="tabular mono" style={{ fontSize: 12 }}>{n}</span>
                          <span style={{ fontSize: 11, color: "var(--tx-3)" }}>{note}</span>
                        </div>
                      ))}
                      {sim.inconvenienced.length === 0 && <span className="mono" style={{ fontSize: 10.5, color: "var(--tx-3)", lineHeight: 1.5 }}>The replay does not attribute decisions to people — the ledger records a principal per decision, but this breakdown is not computed. Nothing is shown rather than a plausible list.</span>}
                    </div>
                  </div>
                  <div className="surface" style={{ display: "flex", flexDirection: "column" }}>
                    <div className="eyebrow" style={{ padding: "10px 14px", borderBottom: "1px solid var(--line-1)" }}>Specific decisions, with why</div>
                    {sim.samples.map((sm) => (
                      <div key={sm.id} style={{ display: "grid", gridTemplateColumns: "80px 110px 1fr 140px", gap: 10, padding: "9px 14px", borderBottom: "1px solid var(--line-1)", alignItems: "baseline" }}>
                        <span className="mono" style={{ fontSize: 10, color: "var(--accent-text)" }}>{sm.id}</span>
                        <span className="mono" style={{ fontSize: 10, color: "var(--tx-2)" }}>{sm.agent}</span>
                        <span style={{ fontSize: 12 }}>{sm.action} — <span style={{ color: "var(--tx-3)" }}>{sm.why}</span></span>
                        <span className="mono" style={{ fontSize: 9, letterSpacing: ".08em", textTransform: "uppercase", color: sm.would === "blocked" ? "var(--danger)" : "var(--warn)", textAlign: "right" }}>{sm.would}</span>
                      </div>
                    ))}
                  </div>
                  <div style={{ display: "flex" }}><div style={{ flex: 1 }} /><button className="btn btn-primary" onClick={() => setStep(6)}>Proceed to ceremony</button></div>
                </div>
              )}
            </div>
          )}

          {step === 6 && (
            <div style={{ display: "flex", flexDirection: "column", gap: 12, maxWidth: 760 }}>
              <div style={{ fontFamily: "var(--font-display)", fontWeight: 440, fontSize: 20 }}>Ceremony — the moment of human authority</div>
              <span className="mono" style={{ fontSize: 9.5, color: "var(--tx-3)" }}>governance.deployIntent() → GovernanceProtocolFactory.predict() on chain 40204</span>
              <Why text={err} />
              {busy && <span className="mono" style={{ fontSize: 10.5, color: "var(--tx-3)" }}>{busy}…</span>}
              {!intent && !busy && (
                <div><button className="btn btn-primary btn-lg" onClick={prepareDeploy} disabled={!specId}>Build the deploy intent</button></div>
              )}
              {intent && (
                <>
                  <p style={{ fontSize: 13.5, color: "var(--tx-2)", lineHeight: 1.6, margin: 0 }}>
                    The contract address is CREATE2-derived and known <em>now</em>, before anyone signs — the factory&apos;s own <span className="mono">predict()</span> answered it, and the same call was dry-run as your wallet to prove it will not revert. What you sign is what deploys.
                  </p>
                  <Rows
                    accent="Address it will have"
                    rows={[
                      ["Address it will have", intent.predictedAddress],
                      ["Template", `${intent.templateName} · ${intent.templateId}`],
                      ["Audit", "devnet CID — UNAUDITED. Not an external audit (D-2)."],
                      ["Tenant", `${intent.tenantName} · ${intent.tenantId}`],
                      ["Classification", intent.classification],
                      ["Spec", `${intent.specId} · ${intent.specCID}`],
                      ["Approvers it will require", intent.approvers.length ? intent.approvers.join(" · ") : "none — the spec named no principals"],
                      ["Will be bound to", intent.actionClass ?? "nothing — the spec names no scope, so BIND cannot run"],
                      ["Salt", intent.salt],
                      ["Read from", intent.source],
                    ]}
                  />
                  {/* The ceremony cannot ABI-decode a factory call and is right
                      not to invent a friendlier summary. This is the line it
                      will show, repeated verbatim beside the detail above. */}
                  <div className="mono" style={{ fontSize: 10, color: "var(--tx-3)", lineHeight: 1.6 }}>
                    The ceremony will describe this only as “{intent.ceremonyAction}” — it does not ABI-decode, and it refuses to invent a friendlier summary. The rows above are what you are actually approving.
                  </div>
                  <div><button className="btn btn-primary btn-lg" onClick={openDeploy}>Open signature ceremony</button></div>
                </>
              )}
            </div>
          )}

          {step === 7 && (
            <div style={{ display: "flex", flexDirection: "column", gap: 12, maxWidth: 760 }}>
              <div style={{ fontFamily: "var(--font-display)", fontWeight: 440, fontSize: 20 }}>Deployed</div>
              <span className="mono" style={{ fontSize: 9.5, color: "var(--tx-3)" }}>governance.deployComplete() → the factory&apos;s ProtocolDeployed log · eth_getCode</span>
              <Why text={err} />
              {!deployRes && <span className="mono" style={{ fontSize: 11, color: "var(--tx-3)" }}>Nothing has been deployed from this spec yet.</span>}
              {deployRes && (
                <>
                  {/* Reaching here IS the address check: deployComplete rejects
                      on a mismatch, so there is no "matched: false" to render. */}
                  <div className="mono" style={{ fontSize: 10.5, color: "var(--ok)", border: "1px solid var(--ok)", background: "var(--ok-bg)", padding: "8px 12px", width: "fit-content" }}>
                    the address the chain logged matches the one that was approved
                  </div>
                  <Rows
                    accent="Address"
                    rows={[
                      ["Address", deployRes.address],
                      ["Approved as", deployRes.predictedAddress],
                      ["Code on chain", `${deployRes.codeSize} bytes`],
                      ["Transaction", deployRes.txHash],
                      ["Block", deployRes.block === null ? "not yet reported" : String(deployRes.block)],
                      ["Template", deployRes.templateName],
                      ["Tenant", `${deployRes.tenantName} · ${deployRes.tenantId}`],
                      ["Classification", deployRes.classification],
                      ["Spec", deployRes.specId],
                      ["Governs", "nothing yet — a deployed protocol is unbound until step 8"],
                      ["Read from", deployRes.source],
                    ]}
                  />
                  <div style={{ display: "flex" }}><div style={{ flex: 1 }} /><button className="btn btn-primary" onClick={() => setStep(8)}>Bind — who this now governs</button></div>
                </>
              )}
            </div>
          )}

          {step === 8 && (
            <div style={{ display: "flex", flexDirection: "column", gap: 12, maxWidth: 860 }}>
              <div style={{ fontFamily: "var(--font-display)", fontWeight: 440, fontSize: 20 }}>Bind — what the policy answers, before and after</div>
              <span className="mono" style={{ fontSize: 9.5, color: "var(--tx-3)" }}>governance.bindIntent()/bindComplete() → PolicyBinding.check + bind on chain 40204</span>
              <Why text={err} />
              {busy && <span className="mono" style={{ fontSize: 10.5, color: "var(--tx-3)" }}>{busy}…</span>}
              {!deployRes && <span className="mono" style={{ fontSize: 11, color: "var(--tx-3)" }}>Deploy a protocol first — there is nothing to bind.</span>}
              {deployRes && !deployRes.actionClass && (
                <div style={{ border: "1px solid var(--warn)", background: "var(--warn-bg)", padding: "10px 12px", fontSize: 11.5, lineHeight: 1.55 }}>
                  This spec names no scope clause, so there is no action class to bind to. Binding to a guessed one would govern something nobody named. Answer the scope question in the interview and redeploy.
                </div>
              )}
              {deployRes?.actionClass && !bindIntent && !busy && (
                <div><button className="btn btn-primary btn-lg" onClick={prepareBind}>Read what the policy says today</button></div>
              )}
              {bindIntent && !bindRes && (
                <>
                  <Rows rows={[
                    ["Protocol", bindIntent.protocol],
                    ["Action class", `${bindIntent.actionClass} · ${bindIntent.actionClassId}`],
                    ["Tenant", `${bindIntent.tenantName} · ${bindIntent.tenantId}`],
                    ["Verdict today", `${bindIntent.before.verdict} / ${bindIntent.before.reason}`],
                    ["Meaning", bindIntent.before.ungoverned
                      ? "UNGOVERNED — nobody has bound anything to this action class. Quorum records that and alerts on it; it is not approval."
                      : "a protocol already answers for this action class"],
                    ["Read from", bindIntent.source],
                  ]} />
                  <div><button className="btn btn-primary btn-lg" onClick={openBind}>Open signature ceremony</button></div>
                </>
              )}
              {bindRes && (
                <>
                  <div style={{ display: "grid", gridTemplateColumns: "1fr 1fr", gap: 10 }}>
                    <div className="surface" style={{ padding: "12px 14px" }}>
                      <span className="eyebrow">Before</span>
                      <p className="mono" style={{ fontSize: 11, lineHeight: 1.8, margin: "6px 0 0", color: "var(--tx-2)" }}>
                        {bindRes.before.verdict} / {bindRes.before.reason}<br />
                        required signers: {bindRes.before.requiredSigners}<br />
                        {bindRes.before.ungoverned ? "ungoverned ⚠" : "governed"}
                      </p>
                    </div>
                    <div className="surface" style={{ padding: "12px 14px", borderTop: "2px solid var(--accent)" }}>
                      <span className="eyebrow">After</span>
                      <p className="mono" style={{ fontSize: 11, lineHeight: 1.8, margin: "6px 0 0" }}>
                        {bindRes.after.verdict} / {bindRes.after.reason}<br />
                        required signers: {bindRes.after.requiredSigners}<br />
                        {bindRes.after.ungoverned ? "ungoverned ⚠" : "governed"}
                      </p>
                    </div>
                  </div>
                  {/* A bind transaction that succeeds while leaving the verdict
                      unchanged has not started governing anything. Saying so is
                      the difference between "it worked" and "it took effect". */}
                  <div className="mono" style={{ fontSize: 10.5, padding: "8px 12px", width: "fit-content", border: `1px solid ${bindRes.changed ? "var(--ok)" : "var(--warn)"}`, background: bindRes.changed ? "var(--ok-bg)" : "var(--warn-bg)", color: bindRes.changed ? "var(--ok)" : "var(--warn)" }}>
                    {bindRes.changed
                      ? "the policy now answers differently — the binding took effect"
                      : "the verdict did not change. The transaction succeeded, but nothing about what this governs is different."}
                  </div>
                  <Rows rows={[
                    ["Protocol", bindRes.protocol],
                    ["Action class", bindRes.actionClass],
                    ["Protocols bound to it", String(bindRes.protocolCount)],
                    ["Transaction", bindRes.txHash],
                    ["Block", bindRes.block === null ? "not yet reported" : String(bindRes.block)],
                    ["Read from", bindRes.source],
                  ]} />
                  {/* S6.4's enforcement table, unchanged by this sprint. */}
                  <div className="mono" style={{ fontSize: 10, color: "var(--tx-3)", lineHeight: 1.6, borderTop: "1px solid var(--line-1)", paddingTop: 8 }}>
                    This binding is <strong>advisory</strong>. PolicyBinding has no on-chain caller: nothing is prevented by it. What it changes is that quorum&apos;s own gate can now ask a real question and record a real verdict instead of <span className="mono">ungoverned</span>.
                  </div>
                </>
              )}
            </div>
          )}
        </div>
      )}

      {tab === "prot" && (
        <div className="surface" style={{ display: "flex", flexDirection: "column" }}>
          <div className="mono" style={{ display: "grid", gridTemplateColumns: "80px 1fr 120px 190px 150px 110px", gap: 10, padding: "8px 16px", fontSize: 9, letterSpacing: ".12em", textTransform: "uppercase", color: "var(--tx-3)", borderBottom: "1px solid var(--line-1)" }}>
            <span>Id</span><span>Protocol</span><span>Version</span><span>Template · audit</span><span>Address</span><span>State</span>
          </div>
          {protocols.map((p) => (
            <div key={p.id} style={{ display: "grid", gridTemplateColumns: "80px 1fr 120px 190px 150px 110px", gap: 10, padding: "10px 16px", borderBottom: "1px solid var(--line-1)", alignItems: "center" }}>
              <span className="mono" style={{ fontSize: 10.5, color: "var(--accent-text)" }}>{p.id}</span>
              <span style={{ fontSize: 13, fontWeight: 500 }}>{p.name} <span style={{ fontSize: 11, color: "var(--tx-3)", fontWeight: 400 }}>governs {p.governs}</span></span>
              <span className="mono" style={{ fontSize: 10.5, color: "var(--tx-2)" }}>{p.version}</span>
              <span className="mono" style={{ fontSize: 9.5, color: p.state === "deprecated" ? "var(--danger)" : "var(--tx-2)" }}>{p.template}<br />{p.audit}</span>
              <span className="mono" style={{ fontSize: 10, color: "var(--tx-2)" }}>{p.addr}</span>
              {/* `unbound` is amber, not green: the protocol exists and governs
                  nothing. Only a confirmed binding earns the OK colour. */}
              <span className="mono" style={{ fontSize: 9, letterSpacing: ".08em", textTransform: "uppercase", color: p.state === "bound" ? "var(--ok)" : p.state === "unbound" ? "var(--warn)" : "var(--danger)" }}>{p.state}</span>
            </div>
          ))}
          <div className="mono" style={{ fontSize: 9.5, color: "var(--tx-3)", padding: "8px 16px" }}>Amendment flow: propose → review window → vote → timelock 14d → migrate · plain-English diff at every step</div>
        </div>
      )}

      {tab === "tpl" && (
        <div style={{ display: "grid", gridTemplateColumns: "repeat(3,1fr)", gap: 10 }}>
          {[
            { name: "BoundedAutonomy v2.1", audit: "audited · CID bafy…70e1", desc: "Agent may act unattended within a spend/action budget per window; over-threshold pauses for a signature.", top: "var(--accent)" },
            { name: "EgressControl v1.4", audit: "audited · CID bafy…22a8", desc: "Which model endpoints may serve which classification; CUI+ is local-only.", top: "var(--info)" },
            { name: "ThresholdApproval v1.2", audit: "audited · CID bafy…9c30", desc: "Action class X requires N-of-M signers from role set R.", top: "var(--z-magenta)" },
          ].map((tc) => (
            <div key={tc.name} className="surface" style={{ padding: 14, display: "flex", flexDirection: "column", gap: 7, borderTop: `2px solid ${tc.top}` }}>
              <span style={{ fontSize: 14, fontWeight: 500 }}>{tc.name}</span>
              <span className="mono" style={{ fontSize: 9.5, color: "var(--ok)" }}>{tc.audit}</span>
              <span style={{ fontSize: 12, color: "var(--tx-2)", lineHeight: 1.5 }}>{tc.desc}</span>
            </div>
          ))}
        </div>
      )}
    </div>
  );
}
