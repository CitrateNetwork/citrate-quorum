// citrate-quorum — Journal surface (QRM-S2D). Charter register. Ported from
// design §JOURNAL. Journals & retros (human + agent, attributed) and the
// standup brief — what an agent will say, reviewable before the meeting. Reads
// bridge.journal.list()/brief().
import { useEffect, useState } from "react";
import { bridge } from "../bridge";
import { DomainErrorPlate, useDomain } from "../components/DomainState";
import type { JournalEntry, StandupBrief } from "../bridge";

export function Journal() {
  const [entries, setEntries] = useState<JournalEntry[]>([]);
  const [brief, setBrief] = useState<StandupBrief | null>(null);
  const [briefNote, setBriefNote] = useState("");

  // The agent and the meeting used to be hardcoded as "claude-code" and
  // "m-0723" — ids that were true of the design fixture and exist in no real
  // tenant, so against live data the brief silently failed and rendered "…".
  // Both now come from what this tenant actually has.
  useEffect(() => {
    (async () => {
      try {
        const [agents, meetings] = await Promise.all([
          bridge.agents.list(),
          bridge.meetings.list(),
        ]);
        if (agents.length === 0 || meetings.length === 0) {
          setBriefNote(
            agents.length === 0
              ? "no agent has acted in this tenant yet — there is nobody to brief"
              : "no meeting scheduled yet — a brief is written for a specific meeting",
          );
          return;
        }
        setBrief(await bridge.journal.brief(agents[0].id, meetings[0].id));
      } catch (e) {
        setBriefNote(e instanceof Error ? e.message : String(e));
      }
    })();
  }, []);

  // Honest failure (S2D.4/§5.1): this surface's primary read is journal.list().
  // A read that cannot succeed must say so and offer a retry, not sit in a
  // loading state forever.
  const primary = useDomain(() => bridge.journal.list(), "journal.list()");
  useEffect(() => {
    if (primary.state.status === "ready") setEntries(primary.state.data);
  }, [primary.state]);
  if (primary.state.status === "error") {
    return (
      <div style={{ padding: 18 }}>
        <DomainErrorPlate source="journal.list()" error={primary.state.error} onRetry={primary.retry} lands="It lands in QRM-S5 (meetings + minutes)." />
      </div>
    );
  }
  return (
    <div style={{ padding: "20px 24px", display: "grid", gridTemplateColumns: "1fr 340px", gap: 16, maxWidth: 1080 }}>
      <div className="surface" style={{ display: "flex", flexDirection: "column", height: "fit-content" }}>
        <div style={{ padding: "10px 16px", borderBottom: "1px solid var(--line-1)" }}><span className="eyebrow">Journals &amp; retros — human and agent, attributed</span></div>
        {entries.map((j) => {
          const c = j.human ? "var(--accent-text)" : "var(--info)";
          return (
            <div key={j.id} style={{ display: "flex", gap: 12, padding: "12px 16px", borderBottom: "1px solid var(--line-1)" }}>
              <span className="mono tabular" style={{ fontSize: 10, color: "var(--tx-3)", width: 74, flexShrink: 0, paddingTop: 2 }}>{j.date}</span>
              <div style={{ minWidth: 0 }}>
                <div style={{ display: "flex", alignItems: "center", gap: 8 }}>
                  <span className="mono" style={{ fontSize: 9.5, color: c, border: `1px solid ${c}`, padding: "1px 7px" }}>{j.who}</span>
                  <span className="mono" style={{ fontSize: 8.5, letterSpacing: ".1em", textTransform: "uppercase", color: "var(--tx-3)" }}>{j.kind}</span>
                </div>
                <p style={{ fontSize: 13, lineHeight: 1.55, margin: "5px 0 0" }}>{j.text}</p>
              </div>
            </div>
          );
        })}
        {entries.length === 0 && (
          <div className="mono" style={{ fontSize: 10.5, lineHeight: 1.6, color: "var(--tx-3)", padding: "12px 16px" }}>
            No journals or retros. Quorum reads them from the workspace named when a meeting is
            scheduled — this register is empty because none were found, not because a read failed.
          </div>
        )}
        {/* Rule 11. This claimed "local memory graph · agent entries are signed
            by their AgentSBT key": there is no memory graph wired, and nothing
            signs a journal entry. Entries are markdown files on disk. */}
        <div className="mono" style={{ fontSize: 9.5, color: "var(--tx-3)", padding: "8px 16px", lineHeight: 1.6 }}>
          journal.list() → .agentile/docs/journals/*.md and sprints/completed/*/RETRO.md in this
          tenant's workspace. Authorship is the file's frontmatter, verbatim — not a signature.
        </div>
      </div>
      <div className="surface" style={{ display: "flex", flexDirection: "column", height: "fit-content", borderTop: "2px solid var(--line-strong)" }}>
        <div style={{ padding: "12px 16px", borderBottom: "1px solid var(--line-1)", display: "flex", flexDirection: "column", gap: 3 }}>
          <span className="eyebrow">Standup brief{brief ? ` — ${brief.agent}` : ""}</span>
          <span className="mono" style={{ fontSize: 9.5, color: "var(--tx-3)" }}>
            {brief ? `for ${brief.meeting} · review before the meeting` : "what the agent will say · review before the meeting"}
          </span>
        </div>
        {!brief && briefNote && (
          <div className="mono" style={{ fontSize: 10.5, lineHeight: 1.6, color: "var(--tx-3)", padding: "10px 16px" }}>
            {briefNote}
          </div>
        )}
        {brief?.sections.map(([k, v]) => (
          <div key={k} style={{ display: "flex", flexDirection: "column", gap: 2, padding: "9px 16px", borderBottom: "1px solid var(--line-1)" }}>
            <span className="mono" style={{ fontSize: 9, letterSpacing: ".1em", textTransform: "uppercase", color: "var(--tx-3)" }}>{k}</span>
            <span style={{ fontSize: 12.5 }}>{v}</span>
          </div>
        ))}
        {/* Approving a brief is a governed act (it decides what an agent says
            on the record) and editing one is an amendment to an artifact. Both
            need the ceremony and neither is built, so they are disabled rather
            than offered as buttons that do nothing. */}
        <div style={{ padding: "10px 16px", display: "flex", gap: 8 }}>
          <button className="btn btn-primary btn-sm" disabled title="Brief approval is a governed act and is not built yet.">Approve for meeting — not yet wired</button>
          <button className="btn btn-ghost btn-sm" disabled title="Editing a brief amends an artifact; not built yet.">Edit</button>
        </div>
      </div>
    </div>
  );
}
