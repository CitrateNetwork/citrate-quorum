# citrate-quorum

*Part of the **[Citrate Network](https://citrate.ai)** — own the means of computation. · [Docs](https://docs.citrate.ai) · [Run a node](https://citrate.ai/download) · [Contribute → free membership](https://github.com/CitrateNetwork/.github/blob/main/CONTRIBUTING.md)*

> A Tauri desktop app for enterprise agent governance — humans-in-control and AI agents hold auditable meetings, and plain-English governance protocols become on-chain contracts.

## What it is

Citrate Quorum is a governed workspace where humans-in-control (HIC) and heterogeneous AI agents run auditable meetings, execute the "Agentile" loop, and turn plain-English governance protocols into CREATE2 smart contracts deployed on the Citrate chain (**40204**). Every agent action is policy-gated in an adapter before execution, recorded with principal/grant/HIC level, signed only through a single human signature ceremony, and anchored on chain — no agent, sidecar, or remote service ever holds a key.

Meetings run as real MLS groups over the server-blind [citrate-comms](https://github.com/CitrateNetwork/citrate-comms) relay, and safety-critical primitives are shared (not forked) from [citrate-core](https://github.com/CitrateNetwork/citrate-core)'s kit. It is multi-tenant from day one and designed to generate SOC 2 Type 2 control evidence in the customer's own environment. Concept overview: https://docs.citrate.ai/apps.

## Prerequisites

```bash
# Rust stable, rust-version >= 1.80 (rust-toolchain.toml auto-selects stable + rustfmt/clippy)
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
rustup component add rustfmt clippy

# Node.js 18/20+ and npm (npm is the pinned package manager)

# Linux Tauri v2 system deps:
sudo apt-get install -y libwebkit2gtk-4.1-dev build-essential libgtk-3-dev
# scripts/run-dev.sh additionally uses: dbus-run-session gnome-keyring-daemon
```

A full local build needs sibling checkouts of [citrate-core](https://github.com/CitrateNetwork/citrate-core) (`../../citrate-core/kit`) and [citrate-comms](https://github.com/CitrateNetwork/citrate-comms) (`../../citrate-comms`) — both are path dependencies.

## Build from source

```bash
git clone https://github.com/CitrateNetwork/citrate-quorum
cd citrate-quorum

npm install
npm run tauri build            # or: npx tauri build
```

Bundles land under `target/release/bundle/<format>/` — Linux `.deb`/`.rpm`/`.AppImage`, Windows NSIS, macOS `.dmg`/`.app` (app identifier `ai.citrate.quorum`). Builds are currently **unsigned** (code-signing/updater deferred).

The CI gate is a local script (org GitHub Actions are down): `scripts/check.sh` runs cargo fmt/clippy/test/audit + npm typecheck/vitest/build.

```bash
npm test           # vitest (frontend)
scripts/check.sh   # full gate: Rust + frontend
```

## Run locally

```bash
npm run tauri dev
```

- Dev server runs on **`http://localhost:1420`** (fixed, `strictPort`; HMR on 1421 with `TAURI_DEV_HOST`).
- Frontend-only: `npm run dev`. Typecheck: `npm run typecheck`.
- To run the built release binary against an isolated keyring/dbus session: `scripts/run-dev.sh` (needs `dbus-run-session` + `gnome-keyring-daemon`).

Quorum is a desktop app (no HTTP server of its own). It's up when the window opens and the Node surface shows a live block height from chain 40204.

## Connect it locally

Quorum reads chain state live and holds meetings over the comms relay. By default it points at the public testnet (`rpc.citrate.ai`, `comms.citrate.ai`, `auth.citrate.ai`). To wire it to a **local** stack:

1. **citrate-comms relay (required for meetings)** — meetings are real MLS groups; the app dials the relay over `wss://`. Run a local [citrate-comms](https://github.com/CitrateNetwork/citrate-comms) relay, then point Quorum at it:

   ```bash
   export QUORUM_RELAY_URL=wss://localhost:8080     # default: wss://comms.citrate.ai
   ```

   The relay refuses plaintext `ws://`; use `wss://` even locally.

2. **Chain RPC (chain 40204)** — Node status, wallet balances, ledger, meeting anchoring, and governance deploys all read live RPC. The RPC URL comes from the vendored address book, not an env var; override the whole book to point at a local node from [citrate-chain](https://github.com/CitrateNetwork/citrate-chain):

   ```bash
   export QUORUM_ADDRESS_BOOK=./local-addresses.json   # {"chainId":40204,"rpcUrl":"http://127.0.0.1:8545",...}
   ```

3. **Identity / auth** — OIDC RP against [citrate-identity](https://github.com/CitrateNetwork/citrate-identity) at `auth.citrate.ai`. (The Tauri CSP `connect-src` allowlist pins `auth/rpc/comms.citrate.ai`; a local dev build must add local hosts to the allowlist.)

4. **Agent core** — the safety spine (signature ceremony, OS-keyring custody, OIDC, sidecar supervisor) is the shared `citrate-core/kit` path dep — no separate service to run; it's linked in.

For the full multi-repo bring-up see `LOCAL_STACK.md` in [citrate-docs](https://github.com/CitrateNetwork/citrate-docs).

## Configuration

No `.env` file — config is `QUORUM_*` env overrides plus a vendored address book (`src-tauri/src/generated/addresses.json`).

| Variable | Default | Purpose |
|---|---|---|
| `QUORUM_RELAY_URL` | `wss://comms.citrate.ai` | citrate-comms MLS relay endpoint |
| `QUORUM_ADDRESS_BOOK` | vendored `addresses.json` | override the canonical chain address book (chain 40204) |
| `QUORUM_ADDRESS_BOOK_BFR` | vendored `addresses-bfr.json` | override the governance/RBAC book |
| `QUORUM_AGENT_ID` / `QUORUM_PRINCIPAL` / `QUORUM_CLASSIFICATION` / `QUORUM_MODEL_ID` | per-adapter defaults | agent-adapter sidecar config |

The vendored book pins `chainId: 40204`, `rpcUrl: https://rpc.citrate.ai`, `explorerUrl: https://explorer.citrate.ai`.

## Links

- Docs: https://docs.citrate.ai/apps
- Depends on: [citrate-core](https://github.com/CitrateNetwork/citrate-core) (safety kit) · [citrate-comms](https://github.com/CitrateNetwork/citrate-comms) (MLS relay) · [citrate-identity](https://github.com/CitrateNetwork/citrate-identity) (OIDC) · [citrate-chain](https://github.com/CitrateNetwork/citrate-chain) (RPC, chain 40204)
- Contributing (DCO): CONTRIBUTING.md · Security: SECURITY.md · License: LICENSE

## License

Source-available (BUSL-1.1) — free for personal/non-commercial use; commercial use requires a Citrate membership. This is **not** an open-source license.
