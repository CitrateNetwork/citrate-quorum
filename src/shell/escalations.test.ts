// citrate-quorum — the escalation notification's content boundary.
//
// An OS notification leaves the application's control: it can be written to a
// system log, rendered on a lock screen, mirrored to a paired phone, or read by
// any process with notification access. An escalation's details — the agent,
// the tool, the parameters, the classification of the work — are exactly the
// material this product is careful about.
//
// So the notice says how many agents are waiting and NOTHING else. These pin
// that, because "I remembered not to put the tool name in" is not a control.
import { describe, expect, it } from "vitest";

import { escalationNotice } from "./escalations";

describe("escalationNotice", () => {
  it("says how many are waiting, in words a human reads at a glance", () => {
    expect(escalationNotice(1).body).toBe("An agent is waiting for your approval.");
    expect(escalationNotice(3).body).toBe("3 agents are waiting for your approval.");
    expect(escalationNotice(1).title).toBe("Citrate Quorum");
  });

  it("leaks NO detail about the escalated action", () => {
    // Everything a real escalation carries. None of it may reach the OS.
    const forbidden = [
      "codex",
      "claude-code",
      "shell.exec",
      "repo.write",
      "spend",
      "CUI",
      "ITAR",
      "Proprietary",
      "decision",
      "R. Ortiz",
      "line-4-automation",
    ];
    for (const count of [1, 2, 47]) {
      const { title, body } = escalationNotice(count);
      const text = `${title} ${body}`.toLowerCase();
      for (const secret of forbidden) {
        expect(
          text.includes(secret.toLowerCase()),
          `the notification must not carry "${secret}" — it leaves the app's control`,
        ).toBe(false);
      }
    }
  });

  it("carries only a count, so the number is the only thing that varies", () => {
    // If two different escalations ever produced different text beyond the
    // count, something about them had leaked into it.
    const a = escalationNotice(2).body;
    const b = escalationNotice(2).body;
    expect(a).toBe(b);
    expect(escalationNotice(2).body).not.toBe(escalationNotice(5).body);
  });
});
