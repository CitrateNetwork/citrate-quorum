// citrate-quorum — the signing identity (QRM-S6).
//
// `MeetingRegistry` records `msg.sender` as the ratifier and the ceremony
// refuses to sign unless the transaction's `from` is this vault's own address.
// So this key IS the operator's on-chain identity, and everything they ratify
// is attributed to it. Until it exists, quorum can sign nothing.
//
// ## The recovery phrase
//
// Showing it breaks the kit's rule that no invoke command returns secret
// material. That exception was an explicit owner decision, and this surface
// carries the other half of it — the kit's contract is *display-and-drop, never
// persist*:
//
//  · it arrives once, from `wallet_create`, and no command can read it back;
//  · it is BLURRED until deliberately revealed, so it cannot be captured by a
//    shoulder, a screen-share, or a screenshot taken without intent;
//  · it lives in component state only — never localStorage, never a store,
//    never a log — and is cleared the moment the operator confirms;
//  · there is no copy button. A clipboard survives the app, syncs between
//    devices, and is readable by anything else running. Typing it into a
//    password manager is slower and is the point.
import { useEffect, useState } from "react";
import { bridge } from "../bridge";

type Mode = "loading" | "none" | "created" | "importing" | "ready";

export function WalletSetup({ onReady }: { onReady?: (address: string) => void }) {
  const [mode, setMode] = useState<Mode>("loading");
  const [address, setAddress] = useState("");
  const [reason, setReason] = useState("");
  const [error, setError] = useState("");
  // The phrase. Component state only, and deliberately never lifted.
  const [phrase, setPhrase] = useState("");
  const [revealed, setRevealed] = useState(false);
  const [saved, setSaved] = useState(false);
  const [importPhrase, setImportPhrase] = useState("");

  useEffect(() => {
    bridge.wallet.identity()
      .then((s) => {
        if (s.exists && s.address) {
          setAddress(s.address);
          setMode("ready");
        } else {
          setReason(s.reason ?? "");
          setMode("none");
        }
      })
      .catch((e) => {
        setError(e instanceof Error ? e.message : String(e));
        setMode("none");
      });
  }, []);

  const create = async () => {
    setError("");
    try {
      const w = await bridge.wallet.createIdentity();
      setAddress(w.address);
      setPhrase(w.mnemonic);
      setRevealed(false);
      setSaved(false);
      setMode("created");
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    }
  };

  const doImport = async () => {
    setError("");
    try {
      const s = await bridge.wallet.importIdentity(importPhrase.trim());
      // Drop the typed phrase immediately — it has done its job.
      setImportPhrase("");
      setAddress(s.address ?? "");
      setMode("ready");
      if (s.address) onReady?.(s.address);
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    }
  };

  /** Confirm the phrase is stored, and drop it from memory. */
  const confirmSaved = () => {
    setPhrase("");
    setRevealed(false);
    setMode("ready");
    onReady?.(address);
  };

  const box: React.CSSProperties = {
    border: "1px solid var(--line-2)", padding: "16px 18px",
    display: "flex", flexDirection: "column", gap: 12, maxWidth: 640,
  };

  if (mode === "loading") {
    return <div className="mono" style={{ fontSize: 11, color: "var(--tx-3)" }}>reading the vault…</div>;
  }

  if (mode === "ready") {
    return (
      <div className="surface" style={box}>
        <span className="eyebrow">Signing identity</span>
        <span className="mono" style={{ fontSize: 12 }}>{address}</span>
        <span className="mono" style={{ fontSize: 9.5, color: "var(--tx-3)", lineHeight: 1.6 }}>
          Every meeting you ratify is attributed to this address on chain. It is sealed in your
          OS keychain and never leaves this machine.
        </span>
      </div>
    );
  }

  if (mode === "created") {
    return (
      <div className="surface" style={{ ...box, borderTop: "2px solid var(--warn)" }}>
        <span className="eyebrow" style={{ color: "var(--warn)" }}>
          Write this down now — it is shown once and never again
        </span>
        <span style={{ fontSize: 13, lineHeight: 1.6 }}>
          These 24 words are the only way to recover your signing identity. Quorum cannot show
          them to you a second time, and no support process can recover them — there is no copy
          anywhere else.
        </span>

        {/* Blurred until deliberately revealed. */}
        <div
          onClick={() => setRevealed(true)}
          title={revealed ? "" : "Click to reveal — make sure nobody is watching your screen"}
          style={{
            position: "relative", cursor: revealed ? "default" : "pointer",
            border: "1px solid var(--line-2)", background: "var(--srf-inset)", padding: "14px 16px",
          }}
        >
          <div
            className="mono"
            style={{
              fontSize: 13, lineHeight: 1.9, wordSpacing: 4,
              filter: revealed ? "none" : "blur(7px)",
              userSelect: revealed ? "text" : "none",
              transition: "filter .12s ease",
            }}
          >
            {phrase}
          </div>
          {!revealed && (
            <div style={{
              position: "absolute", inset: 0, display: "flex", alignItems: "center",
              justifyContent: "center", pointerEvents: "none",
            }}>
              <span className="mono" style={{ fontSize: 10.5, letterSpacing: ".1em", textTransform: "uppercase", color: "var(--tx-2)" }}>
                Click to reveal
              </span>
            </div>
          )}
        </div>

        <div className="mono" style={{ fontSize: 10, lineHeight: 1.8, color: "var(--tx-2)" }}>
          <div style={{ marginBottom: 4, color: "var(--tx-1)" }}>Where to put it</div>
          · Your OS keychain or a password manager — macOS Keychain Access, GNOME Keyring / KDE
          Wallet, 1Password, Bitwarden. Store it as a secure note, not a password field.<br />
          · Or on paper, somewhere you would keep a passport.<br />
          <span style={{ color: "var(--danger)" }}>
            · Not in a screenshot, a photo, a chat, an email, a text file, or a note-taking app
            that syncs.
          </span>
        </div>

        <span className="mono" style={{ fontSize: 9.5, color: "var(--tx-3)" }}>
          address {address}
        </span>

        <label style={{ display: "flex", alignItems: "flex-start", gap: 8, fontSize: 12.5 }}>
          <input type="checkbox" checked={saved} onChange={(e) => setSaved(e.target.checked)} style={{ marginTop: 3 }} />
          <span>I have stored the 24 words somewhere I can get them back.</span>
        </label>

        <button className="btn btn-primary btn-sm" disabled={!saved} onClick={confirmSaved} style={{ width: "fit-content" }}>
          Continue — clear it from this screen
        </button>
        {error && <span className="mono" style={{ fontSize: 11, color: "var(--danger)" }}>{error}</span>}
      </div>
    );
  }

  if (mode === "importing") {
    return (
      <div className="surface" style={box}>
        <span className="eyebrow">Import an existing identity</span>
        <span style={{ fontSize: 13, lineHeight: 1.6 }}>
          Paste the 24-word recovery phrase. It is sealed into this machine's OS keychain and is
          not stored anywhere else, nor sent anywhere.
        </span>
        <textarea
          value={importPhrase}
          onChange={(e) => setImportPhrase(e.target.value)}
          rows={3}
          spellCheck={false}
          autoComplete="off"
          placeholder="word word word …"
          className="mono"
          style={{ fontSize: 12, padding: 10, resize: "vertical" }}
        />
        <div style={{ display: "flex", gap: 8 }}>
          <button className="btn btn-primary btn-sm" onClick={doImport} disabled={!importPhrase.trim()}>Import</button>
          <button className="btn btn-ghost btn-sm" onClick={() => { setImportPhrase(""); setError(""); setMode("none"); }}>Cancel</button>
        </div>
        {error && <span className="mono" style={{ fontSize: 11, color: "var(--danger)" }}>{error}</span>}
      </div>
    );
  }

  // mode === "none"
  return (
    <div className="surface" style={box}>
      <span className="eyebrow">No signing identity yet</span>
      <span style={{ fontSize: 13, lineHeight: 1.6 }}>
        Quorum cannot sign anything until this machine holds a key. Ratifying minutes attributes
        them to its address on chain, so this is the identity an auditor will see.
      </span>
      {/* The raw vault error is true but useless on its own — "custody vault
          locked or unavailable" tells an operator nothing about what to do.
          Creation needs an UNLOCKED vault, so say that. */}
      {reason && (
        <div className="mono" style={{ fontSize: 10, color: "var(--tx-3)", lineHeight: 1.7 }}>
          {/[Ll]ocked|[Dd]enied|[Cc]ustody/.test(reason) ? (
            <>
              Your custody vault is locked, and a key can only be sealed into an unlocked vault.
              Unlock it first — creating or importing below will fail until you do.
            </>
          ) : (
            <>No key is stored on this machine yet.</>
          )}
          <br />
          <span style={{ color: "var(--tx-3)" }}>vault says: {reason}</span>
        </div>
      )}
      <div style={{ display: "flex", gap: 8 }}>
        <button className="btn btn-primary btn-sm" onClick={create}>Create a new identity</button>
        <button className="btn btn-ghost btn-sm" onClick={() => { setError(""); setMode("importing"); }}>Import an existing one</button>
      </div>
      {error && <span className="mono" style={{ fontSize: 11, color: "var(--danger)" }}>{error}</span>}
    </div>
  );
}
