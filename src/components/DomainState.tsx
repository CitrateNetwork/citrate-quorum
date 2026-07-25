// =====================================================================
// citrate-quorum — the shared domain-read state machine (QRM-S2D.4/S2D.6).
//
// The design brief (§5.1) requires every surface to ship all seven states, and
// Rule 1 requires that an unbuilt backend LOOK unbuilt. Before this, a surface
// whose domain was unavailable sat in its loading state forever — the packaged
// app showed `wallet.summary()…` indefinitely with no way to know it never
// would. A permanent "loading" is a lie told slowly.
//
// `useDomain` is the one place that turns a bridge read into an honest state:
//
//   loading → skeletons matching the layout
//   ready   → the data
//   error   → NAMES the failing call and offers a retry (§5.1: never
//             "Something went wrong")
//
// Empty is the surface's own business — only it knows what "empty" means — but
// it can only reach empty by way of `ready`, so an empty list is now provably
// an empty result rather than a read that quietly failed.
//
// Defensive on purpose: `load()` is invoked inside the promise chain so that a
// synchronous throw is captured as a rejection. The bridge's unwired methods
// reject (fixed in #12), but a surface must not white-screen if one ever throws
// again.
// =====================================================================
import { useCallback, useEffect, useState, type ReactNode } from "react";

export type DomainState<T> =
  | { status: "loading" }
  | { status: "ready"; data: T }
  | { status: "error"; error: Error };

export interface DomainRead<T> {
  state: DomainState<T>;
  retry: () => void;
}

/**
 * Read one domain method into an honest state.
 *
 * @param load   the bridge call, e.g. `() => bridge.wallet.summary()`
 * @param source the call's name for the error plate + the Rule 11 note, e.g.
 *               `"wallet.summary()"`. Stated, never guessed from the stack.
 */
export function useDomain<T>(load: () => Promise<T>, source: string): DomainRead<T> {
  const [state, setState] = useState<DomainState<T>>({ status: "loading" });
  const [attempt, setAttempt] = useState(0);

  useEffect(() => {
    let live = true;
    setState({ status: "loading" });
    // Wrapping in Promise.resolve().then keeps a synchronous throw inside the
    // chain instead of blowing up the effect.
    Promise.resolve()
      .then(load)
      .then((data) => {
        if (live) setState({ status: "ready", data });
      })
      .catch((e: unknown) => {
        if (live) {
          setState({
            status: "error",
            error: e instanceof Error ? e : new Error(String(e)),
          });
        }
      });
    return () => {
      live = false;
    };
    // `load` is a fresh closure each render; `source` identifies the call and
    // `attempt` drives retries. Re-running on every render would loop.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [source, attempt]);

  const retry = useCallback(() => setAttempt((a) => a + 1), []);
  return { state, retry };
}

/**
 * The error plate. Names the call that failed and what it said, and offers a
 * retry — §5.1's Error rule, which explicitly forbids "Something went wrong".
 */
export function DomainErrorPlate({
  source,
  error,
  onRetry,
  lands,
}: {
  source: string;
  error: Error;
  onRetry: () => void;
  /** When the domain simply is not wired yet, a sentence saying what would
   *  light it — "It lands in QRM-S3 (rooms)." Named from the planset, never
   *  guessed. */
  lands?: string;
}) {
  return (
    <div
      style={{
        border: "1px solid var(--warn)",
        background: "var(--warn-bg)",
        padding: "14px 16px",
        display: "flex",
        flexDirection: "column",
        gap: 8,
        maxWidth: 620,
      }}
    >
      <span
        className="mono"
        style={{ fontSize: 9.5, letterSpacing: ".13em", textTransform: "uppercase", color: "var(--warn)" }}
      >
        {lands ? "Not wired yet" : "This read failed"}
      </span>
      <span style={{ fontSize: 13.5, lineHeight: 1.5 }}>
        {lands
          ? `This surface reads ${source}. ${lands} Nothing is shown here because there is nothing real to show yet.`
          : `${source} did not return.`}
      </span>
      <span className="mono" style={{ fontSize: 10.5, color: "var(--tx-2)", wordBreak: "break-word" }}>
        {error.message}
      </span>
      <div>
        <button className="btn btn-ghost btn-sm" onClick={onRetry}>
          Retry
        </button>
      </div>
    </div>
  );
}

/** Loading skeleton rows that match a table-ish layout (§5.1: never a spinner). */
export function SkeletonRows({ rows = 5, height = 28 }: { rows?: number; height?: number }) {
  return (
    <div style={{ display: "flex", flexDirection: "column", gap: 6, padding: 4 }} aria-busy="true">
      {Array.from({ length: rows }, (_, i) => (
        <div
          key={i}
          style={{
            height,
            background: "var(--srf-inset)",
            border: "1px solid var(--line-1)",
            opacity: 1 - i * 0.12,
          }}
        />
      ))}
    </div>
  );
}

/**
 * Render a domain read: skeletons while loading, an honest plate on failure,
 * the surface's own content when ready.
 */
export function Domain<T>({
  read,
  source,
  lands,
  skeletonRows,
  children,
}: {
  read: DomainRead<T>;
  source: string;
  lands?: string;
  skeletonRows?: number;
  children: (data: T) => ReactNode;
}) {
  if (read.state.status === "loading") return <SkeletonRows rows={skeletonRows} />;
  if (read.state.status === "error") {
    return (
      <DomainErrorPlate source={source} error={read.state.error} onRetry={read.retry} lands={lands} />
    );
  }
  return <>{children(read.state.data)}</>;
}
