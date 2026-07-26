// citrate-quorum — the custody vault (QRM-S6).
//
// Every key this app holds is sealed in an OS-keyring-backed envelope that is
// locked by a passphrase. Nothing can be sealed into a LOCKED vault, so this is
// the first thing an operator does: without it there is no signing identity,
// and without that nothing can be ratified on chain.
//
// ## What this surface is careful about
//
//  · The passphrase is `type="password"`, lives in component state only, and is
//    cleared immediately after the call — success or failure.
//  · Initialising warns that the passphrase cannot be recovered, because it
//    cannot: it is the seal, not a login.
//  · A failed unlock says only that it failed. Custody deliberately gives no
//    locked-vs-absent oracle, and this surface does not reintroduce one by
//    guessing a friendlier reason.
//  · Autolock and the keyring backend are shown as facts read from the vault,
//    not as reassurance.
import { useCallback, useEffect, useState } from "react";
import {
  custodyInit,
  custodyLock,
  custodyStatus,
  custodyUnlock,
  type CustodyStatusDto,
} from "../bridge/tauri/commands";

export function VaultPanel({ onChange }: { onChange?: () => void }) {
  const [status, setStatus] = useState<CustodyStatusDto | null>(null);
  const [pass, setPass] = useState("");
  const [confirm, setConfirm] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");
  const [loadError, setLoadError] = useState("");

  const read = useCallback(async () => {
    try {
      setStatus(await custodyStatus());
    } catch (e) {
      setLoadError(e instanceof Error ? e.message : String(e));
    }
  }, []);

  useEffect(() => {
    // `read` is async: the setState happens after an await, not synchronously
    // in the effect body. The compiler rule cannot see through the promise, and
    // there is no derived form of "ask the vault what state it is in".
    // eslint-disable-next-line react-hooks/set-state-in-effect -- see above
    void read();
  }, [read]);

  /** Run an action, then ALWAYS drop the passphrase from memory. */
  const withPass = async (fn: () => Promise<void>) => {
    setBusy(true);
    setError("");
    try {
      await fn();
      await read();
      onChange?.();
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      // Cleared on the failure path too — a retry should retype it, not reuse
      // a value still sitting in a React state cell.
      setPass("");
      setConfirm("");
      setBusy(false);
    }
  };

  const box: React.CSSProperties = {
    border: "1px solid var(--line-2)", padding: "16px 18px",
    display: "flex", flexDirection: "column", gap: 12, maxWidth: 640,
  };
  const field: React.CSSProperties = { padding: "7px 9px", fontSize: 13 };

  if (loadError) {
    return (
      <div className="surface" style={{ ...box, borderTop: "2px solid var(--danger)" }}>
        <span className="eyebrow" style={{ color: "var(--danger)" }}>Vault unreadable</span>
        <span className="mono" style={{ fontSize: 11, lineHeight: 1.6 }}>{loadError}</span>
      </div>
    );
  }
  if (!status) {
    return <div className="mono" style={{ fontSize: 11, color: "var(--tx-3)" }}>reading the vault…</div>;
  }

  const facts = (
    <div className="mono" style={{ fontSize: 9.5, color: "var(--tx-3)", lineHeight: 1.7 }}>
      keyring: {status.keyringStatus} · autolock: {status.autolockMins} min
    </div>
  );

  // ---- not initialised: set the passphrase that seals everything ----
  if (!status.initialized) {
    const tooShort = pass.length > 0 && pass.length < 12;
    const mismatch = confirm.length > 0 && pass !== confirm;
    return (
      <div className="surface" style={{ ...box, borderTop: "2px solid var(--warn)" }}>
        <span className="eyebrow" style={{ color: "var(--warn)" }}>The vault is not set up</span>
        <span style={{ fontSize: 13, lineHeight: 1.6 }}>
          Choose a passphrase. It seals the envelope that holds your signing key — it is not a
          login, and it <strong>cannot be recovered or reset</strong>. If you lose it, the key
          sealed under it is gone and only your 24-word recovery phrase can restore the identity.
        </span>
        <input type="password" autoComplete="new-password" placeholder="passphrase (12+ characters)"
          value={pass} onChange={(e) => setPass(e.target.value)} style={field} />
        <input type="password" autoComplete="new-password" placeholder="confirm passphrase"
          value={confirm} onChange={(e) => setConfirm(e.target.value)} style={field} />
        {tooShort && <span className="mono" style={{ fontSize: 10.5, color: "var(--warn)" }}>use at least 12 characters</span>}
        {mismatch && <span className="mono" style={{ fontSize: 10.5, color: "var(--warn)" }}>the two entries do not match</span>}
        <button className="btn btn-primary btn-sm" style={{ width: "fit-content" }}
          disabled={busy || pass.length < 12 || pass !== confirm}
          onClick={() =>
            withPass(async () => {
              // `custody_init` seals the envelope but leaves it LOCKED, so
              // creating a vault used to drop the operator straight onto an
              // unlock prompt for the passphrase they had just typed — and
              // this panel clears it, so they had to retype it. Unlock in the
              // same action: they demonstrably know it a moment ago.
              await custodyInit(pass);
              await custodyUnlock(pass);
            })
          }>
          {busy ? "sealing…" : "Create the vault"}
        </button>
        {error && <span className="mono" style={{ fontSize: 11, color: "var(--danger)" }}>{error}</span>}
        {facts}
      </div>
    );
  }

  // ---- initialised but locked ----
  if (!status.unlocked) {
    return (
      <div className="surface" style={{ ...box, borderTop: "2px solid var(--warn)" }}>
        <span className="eyebrow" style={{ color: "var(--warn)" }}>The vault is locked</span>
        <span style={{ fontSize: 13, lineHeight: 1.6 }}>
          Nothing can be signed, and no key can be created or imported, while it is locked.
        </span>
        <input type="password" autoComplete="current-password" placeholder="passphrase"
          value={pass} onChange={(e) => setPass(e.target.value)} style={field}
          onKeyDown={(e) => { if (e.key === "Enter" && pass) void withPass(() => custodyUnlock(pass)); }} />
        <button className="btn btn-primary btn-sm" style={{ width: "fit-content" }}
          disabled={busy || !pass}
          onClick={() => withPass(() => custodyUnlock(pass))}>
          {busy ? "unlocking…" : "Unlock"}
        </button>
        {/* Deliberately unelaborated. Custody gives no locked-vs-absent oracle;
            inventing a friendlier reason here would hand one back. */}
        {error && <span className="mono" style={{ fontSize: 11, color: "var(--danger)" }}>{error}</span>}
        {facts}
      </div>
    );
  }

  // ---- unlocked ----
  return (
    <div className="surface" style={{ ...box, borderTop: "2px solid var(--ok)" }}>
      <div style={{ display: "flex", alignItems: "center", gap: 10 }}>
        <span className="eyebrow" style={{ color: "var(--ok)" }}>Vault unlocked</span>
        <div style={{ flex: 1 }} />
        <button className="btn btn-ghost btn-sm" disabled={busy}
          onClick={() => withPass(() => custodyLock())}>Lock now</button>
      </div>
      <span className="mono" style={{ fontSize: 10, color: "var(--tx-3)", lineHeight: 1.7 }}>
        It re-locks by itself after {status.autolockMins} minutes idle. A locked vault cannot
        sign, which is the intended behaviour, not a fault.
      </span>
      {error && <span className="mono" style={{ fontSize: 11, color: "var(--danger)" }}>{error}</span>}
      {facts}
    </div>
  );
}
