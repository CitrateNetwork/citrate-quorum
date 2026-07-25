// =====================================================================
// citrate-quorum — the escalation feed (QRM-S4, notification path).
//
// One poller for "which agents are blocked waiting on a human", shared by
// everything that needs to know: the toast that interrupts, the badge in the
// shell chrome, and the Dashboard's queue. Before this each of those would
// have polled independently, which is three answers to one question and three
// chances to disagree.
//
// ## Why a notification at all
//
// An escalation is recorded, durable, and visible on the Dashboard. None of
// that helps if nobody is looking at the Dashboard. An agent that stops and
// asks at 2am waits until someone opens the app — so the queue needs to reach
// out, not just sit there.
//
// ## What it deliberately does not do
//
// **It never approves.** Every affordance here opens the ceremony; the
// approval happens there, under a signature, or it does not happen. A
// notification with an "Approve" button would be an HIC-1 bypass wearing a
// convenient hat.
//
// **The OS notification carries no detail** — see `notifyOs`.
// =====================================================================
import {
  createContext,
  useCallback,
  useContext,
  useEffect,
  useRef,
  useState,
  type ReactNode,
} from "react";

import { bridge, type PendingApproval } from "../bridge";

/** How often the feed re-reads the queue. The chain is low-rate; polling is
 *  honest and simple, and matches the ledger ribbon's cadence. */
const POLL_MS = 4000;

interface EscalationFeed {
  /** Escalations waiting on a human, oldest first. */
  queue: PendingApproval[];
  /** Re-read now — call after answering one so the UI does not lag a tick. */
  refresh: () => void;
}

const Ctx = createContext<EscalationFeed>({ queue: [], refresh: () => {} });

export function useEscalations(): EscalationFeed {
  return useContext(Ctx);
}

/**
 * Tell the operating system an agent is waiting — and tell it *nothing else*.
 *
 * An OS notification leaves the application's control: it can be written to a
 * system log, rendered on a lock screen, mirrored to a paired phone, or read
 * by any process with notification access. An escalation's details are exactly
 * the material this product is careful about — the agent, the tool, the
 * parameters, and the classification of the work.
 *
 * So the notification says how many agents are waiting and nothing more. The
 * detail stays behind the operator's session, in the app. A count is enough to
 * make someone look, which is the entire job.
 *
 * Best-effort by construction: if the platform has no notification service, or
 * permission is refused, the in-app toast still fires and nothing breaks.
 */
export function escalationNotice(count: number): { title: string; body: string } {
  return {
    title: "Citrate Quorum",
    body:
      count === 1
        ? "An agent is waiting for your approval."
        : `${count} agents are waiting for your approval.`,
  };
}

async function notifyOs(count: number): Promise<void> {
  try {
    const mod = await import("@tauri-apps/plugin-notification");
    let granted = await mod.isPermissionGranted();
    if (!granted) {
      granted = (await mod.requestPermission()) === "granted";
    }
    if (!granted) return;
    // No `actions`: a notification you can approve from is an HIC-1 bypass.
    mod.sendNotification(escalationNotice(count));
  } catch {
    // No notification service, no permission, or not running in Tauri at all.
    // The in-app toast is the guaranteed channel; this one is a courtesy.
  }
}

export function EscalationProvider({ children }: { children: ReactNode }) {
  const [queue, setQueue] = useState<PendingApproval[]>([]);
  const [tick, setTick] = useState(0);
  /** Decisions we have already announced, so a standing queue does not
   *  re-notify every four seconds. */
  const announced = useRef<Set<number>>(new Set());
  /** Suppress the notification for the queue that already existed at startup:
   *  those were escalated while the app was closed and are visible the moment
   *  it opens. Interrupting someone about what is already on their screen
   *  trains them to dismiss interruptions. */
  const primed = useRef(false);

  useEffect(() => {
    const iv = setInterval(() => setTick((t) => t + 1), POLL_MS);
    return () => clearInterval(iv);
  }, []);

  useEffect(() => {
    let live = true;
    Promise.resolve()
      .then(() => bridge.policy.pending())
      .then((q) => {
        if (!live) return;
        setQueue(q);
        const fresh = q.filter((p) => !announced.current.has(p.decision));
        q.forEach((p) => announced.current.add(p.decision));
        // Forget answered ones so a decision id cannot silently go stale.
        const liveIds = new Set(q.map((p) => p.decision));
        announced.current.forEach((id) => {
          if (!liveIds.has(id)) announced.current.delete(id);
        });
        if (!primed.current) {
          primed.current = true;
          return;
        }
        if (fresh.length > 0) void notifyOs(q.length);
      })
      .catch(() => {
        // No tenant scope yet, or the backend is unavailable. An empty queue is
        // the honest render; the surfaces report the failure themselves.
        if (live) setQueue([]);
      });
    return () => {
      live = false;
    };
  }, [tick]);

  const refresh = useCallback(() => setTick((t) => t + 1), []);

  return <Ctx.Provider value={{ queue, refresh }}>{children}</Ctx.Provider>;
}
