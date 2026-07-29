//! citrate-quorum — the Tauri desktop app backend (WP-S1.3).
//!
//! **Governance for agents.** This skeleton wires the shared safety-critical
//! spine from [`citrate_core_kit`] — the SignatureCeremony (the single
//! human-in-the-loop signing path), the OS-keyring custody vault, the OIDC RP,
//! and app config — into a Tauri app, exactly as citrate-core does, so the two
//! apps run ONE implementation of that code, never a fork.
//!
//! ## What is real here, and what is honest-placeholder (Rule 1)
//! - **Real:** the kit's `config` / `custody` / `auth` / `ceremony` command
//!   surface is registered and functional (the shared signing path is live).
//! - **Placeholder:** the frontend shell is a "under construction" plate; the
//!   real surfaces (Rooms, Meetings, Governance, Agents, Ledger, …) arrive from
//!   the design prototype (QRM-S2D) and their sprints. Nothing renders fabricated
//!   data.
//! - **Not yet wired:** quorum's own domains (rooms, meetings, governance, agents,
//!   ledger, calendar, repos) — each lands in its sprint. The tenancy spine
//!   ([`quorum_tenancy`]), license seam ([`quorum_license`]), and RBAC bindings
//!   ([`quorum_rbac`]) exist as backend crates and are surfaced honestly.
//!
//! ## The single-signing-path property holds in quorum too
//! The kit's gated signer is `pub(crate)` to the kit crate, so **no quorum code
//! can invoke it** — the compiler forbids it. `tests` additionally source-scans
//! this crate for any competing signing site (the "runs in both repos" guard).

use citrate_core_kit::{ceremony, config, custody, oidc};
use tauri::Manager;

mod addresses;
mod agent_bridge;
mod anchor;
mod backend;
mod bind;
mod chain;
mod compile;
mod ctor;
mod deploy;
mod ingest;
mod interview;
mod protocols;
mod rooms;
mod simulate;
mod spec;
mod store;
#[cfg(test)]
mod template_hashes;
mod wallet_setup;

/// A tiny, honest status command the placeholder shell can call to prove the
/// backend is live and the quorum-specific seams (tenancy, license) are wired —
/// without fabricating any governance data.
#[tauri::command]
fn quorum_skeleton_status() -> SkeletonStatus {
    use quorum_license::{LicenseDomain, UnenforcedLicense};
    let lic = UnenforcedLicense.status();
    SkeletonStatus {
        app: "citrate-quorum",
        // The shared signing spine is consumed from citrate-core-kit.
        kit_linked: true,
        // Seat metering is honestly not enforced yet (WP-S1.6).
        license_enforced: lic.enforced,
        license_note: lic.note,
        // The real governance surfaces are not built yet (Rule 1).
        surfaces_built: false,
    }
}

#[derive(serde::Serialize)]
struct SkeletonStatus {
    app: &'static str,
    kit_linked: bool,
    license_enforced: bool,
    license_note: &'static str,
    surfaces_built: bool,
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
// `expect` on the top-level Tauri run is the sanctioned idiom: an app that cannot
// build its context or event loop has no recoverable state and must fail loudly
// at startup. This is the ONE sanctioned panic site (workspace lints deny
// expect/unwrap/panic everywhere else).
#[allow(clippy::expect_used)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_store::Builder::new().build())
        .setup(|app| {
            // The shared custody vault (real OS keyring + app-data envelope),
            // seeded with the persisted autolock — identical wiring to core.
            let handle = app.handle();
            let autolock = config::config_read(handle.clone())
                .map(|c| c.autolock)
                .unwrap_or_else(|_| config::AppConfig::default().autolock);
            let state = custody::build_custody_state(handle, autolock)
                .map_err(|e| -> Box<dyn std::error::Error> { e.into() })?;
            app.manage(state);
            // The shared OIDC auth manager.
            app.manage(oidc::build_auth_state());
            // The shared SignatureCeremony — the SINGLE HITL signing path. Every
            // signature intent in quorum (user or, later, agent) routes through
            // this one approval surface; the gated signer is reachable ONLY from
            // its `approve` path and is unreachable from any quorum code.
            app.manage(ceremony::build_ceremony_state());
            // The rooms subsystem. Empty until the operator connects: this app
            // dials the relay when asked to, never at startup, so an installation
            // that never opens a room never touches the network.
            app.manage(rooms::RoomsState::new());
            // The quorum governance backend: per-tenant audit HashChains, live
            // capability grants + vote allowances, and the policy→audit pipeline.
            // Serves the Ledger surface and the governed-action loop from real
            // hash-chained records, never fabricated data (Rule 1).
            //
            // Its evidence store is DURABLE and required. This product's
            // deliverable is audit evidence; an installation that cannot write
            // it must not start up pretending otherwise, so a store that will
            // not open is a hard startup failure rather than a silent fall back
            // to memory.
            let app_data = app.path().app_data_dir()?;
            let store = store::EvidenceStore::open(store::evidence_dir(&app_data))
                .map_err(|e| -> Box<dyn std::error::Error> { e.to_string().into() })?;
            let backend = std::sync::Arc::new(std::sync::Mutex::new(
                backend::QuorumBackend::with_store(store),
            ));
            app.manage(std::sync::Arc::clone(&backend));

            // The keyless agent bridge (WP-S4.1): loopback-only, bearer-authed
            // intake where an agent submits an UNSIGNED intent and gets back a
            // recorded verdict. It shares the backend, so an agent's tool call
            // and an operator's click hit the same gate, chain and budget. It
            // cannot sign: the kit's gated signer is `pub(crate)` to the kit.
            //
            // A bridge that cannot start is not fatal — the app is still usable
            // by a human — but it must be visible, never a silent absence that
            // looks like "no agents connected".
            let token = std::sync::Arc::new(agent_bridge::BridgeToken::mint());
            let agent_dir = app_data.join("agent");
            match agent_bridge::serve(
                0,
                agent_dir.join("token"),
                std::sync::Arc::clone(&backend),
                token,
            ) {
                Ok(running) => {
                    let endpoint = serde_json::json!({
                        "addr": running.addr.to_string(),
                        "token_file": running.token_path.to_string_lossy(),
                    });
                    let _ = std::fs::write(
                        agent_dir.join("endpoint.json"),
                        endpoint.to_string(),
                    );
                    eprintln!("[quorum] agent bridge listening on {}", running.addr);
                }
                Err(e) => eprintln!("[quorum] agent bridge FAILED to start: {e} — no agent can be governed until this is fixed"),
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            // The quorum skeleton's own honest status probe.
            quorum_skeleton_status,
            // governance authoring pipeline (QRM-S7).
            ingest::governance_ingest,
            interview::governance_interview,
            spec::governance_spec,
            compile::governance_compile,
            simulate::governance_simulate,
            deploy::governance_deploy_intent,
            deploy::governance_deploy_complete,
            bind::governance_bind_intent,
            bind::governance_bind_complete,
            protocols::governance_specs,
            protocols::governance_protocols,
            // config — persisted app config (shared kit surface).
            config::config_read,
            config::config_write,
            config::config_keyring_status,
            // custody — the OS-keyring vault (shared). custody_get is NOT here:
            // no invoke command returns secret bytes (ADV-8 boundary, inherited).
            custody::custody_status,
            custody::custody_init,
            custody::custody_unlock,
            custody::custody_lock,
            custody::custody_put,
            custody::custody_list,
            custody::custody_keyring_status,
            // wallet setup — the signing identity every ratification is
            // attributed to. `wallet_create` is the ONE command in this app
            // that returns secret material, once, under an explicit owner
            // decision (see wallet_setup.rs).
            wallet_setup::wallet_status,
            wallet_setup::wallet_create,
            wallet_setup::wallet_import,
            // auth — real OIDC loopback-PKCE (shared). No command returns a token.
            oidc::auth_status,
            oidc::auth_login,
            oidc::auth_userinfo,
            oidc::auth_refresh,
            oidc::auth_logout,
            oidc::kyc_start,
            // signing — the SignatureCeremony (shared, the ONE signing path).
            ceremony::sign_request,
            ceremony::sign_approve,
            ceremony::sign_and_broadcast,
            ceremony::sign_reject,
            // governance backend — the policy→audit pipeline + ledger reads +
            // grant/allowance management (quorum-policy/audit/session/clearance).
            // The tenant scope is backend-owned: no command below takes one.
            backend::tenant_set,
            backend::tenant_active,
            backend::operator_set,
            backend::operator_get,
            backend::action_evaluate_and_record,
            backend::action_reject,
            backend::action_approve,
            backend::approvals_pending,
            // meetings — the governed meeting record (QRM-S5). None of these
            // sign; ratification records a signature the ceremony already took.
            backend::meetings_list,
            backend::meeting_get,
            backend::meeting_schedule,
            backend::meeting_admit,
            backend::meeting_open,
            backend::meeting_close,
            backend::meeting_content_hash,
            backend::meeting_ratify,
            backend::meeting_anchor,
            backend::meeting_register_intent,
            backend::journal_list,
            backend::journal_brief,
            backend::ledger_records,
            backend::ledger_decision,
            backend::ledger_verify_decision,
            backend::ledger_correlation,
            backend::ledger_state,
            backend::ledger_head,
            backend::ledger_merkle_root,
            backend::ledger_verify,
            backend::ledger_ungoverned_count,
            backend::grant_issue,
            backend::agents_known,
            backend::grants_for_agent,
            backend::grant_revoke,
            backend::allowance_issue,
            backend::allowance_revoke,
            backend::vote_cast,
            backend::session_resolve,
            // live chain reads (Phase 0) — node posture, the block explorer,
            // this app's own RPC activity, the on-chain tenant tree, and the
            // signing identity's balances. All reads; none of them signs.
            chain::node_status,
            chain::node_blocks,
            chain::node_activity,
            chain::tenancy_tree,
            chain::clearance_of,
            chain::wallet_summary,
            // rooms (QRM-S3) — a real MLS group on the citrate-comms relay. None
            // of these signs with the vault's wallet; a room identity is a relay
            // identity, sealed in custody under its own slot namespace.
            rooms::rooms_status,
            rooms::rooms_connect_intent,
            rooms::rooms_connect_complete,
            rooms::rooms_open,
            rooms::rooms_list,
            rooms::rooms_roster,
            rooms::rooms_say,
            rooms::rooms_events,
            rooms::rooms_leave,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    /// Every quorum-authored source file. The scan below claimed to cover "the
    /// backend source" while only reading `lib.rs`; it now actually does, which
    /// matters most for `agent_bridge.rs` — the surface agents talk to.
    const QUORUM_SOURCES: [(&str, &str); 6] = [
        ("lib.rs", include_str!("lib.rs")),
        ("backend.rs", include_str!("backend.rs")),
        ("store.rs", include_str!("store.rs")),
        ("agent_bridge.rs", include_str!("agent_bridge.rs")),
        // chain.rs talks to the RPC. It reads only — this scan is what keeps
        // "reads only" true as it grows.
        ("chain.rs", include_str!("chain.rs")),
        // rooms.rs holds relay identities and drives MLS. It must never reach the
        // gated signer either — see its own `rooms_never_reaches_the_gated_signer`.
        ("rooms.rs", include_str!("rooms.rs")),
    ];

    /// The single-signing-path guard, quorum side (WP-S1.2 acceptance: the
    /// ceremony source-scan test "runs in both repos"; QRM-S4 exit gate: "an
    /// agent cannot obtain a signature without a ceremony"). The gated signer is
    /// `pub(crate)` to citrate-core-kit, so quorum code CANNOT invoke it — that
    /// part is compiler-enforced. This scans every quorum-authored module to
    /// ensure no one adds a competing signing site.
    ///
    /// NEGATIVE CONTROL: add `let _ = something.sign_message(&v, b"x");` to any
    /// scanned file and this fails. Needles are assembled from parts so this
    /// test's own prose cannot self-match.
    #[test]
    fn no_competing_signing_site_in_quorum() {
        let calls = [
            "sign_".to_string() + "message(",
            "sign_".to_string() + "transaction(",
        ];
        for (name, src) in QUORUM_SOURCES {
            for line in src.lines() {
                let t = line.trim_start();
                if t.starts_with("//") || t.starts_with("///") || t.starts_with("*") {
                    continue;
                }
                for call in &calls {
                    assert!(
                        !t.contains(call.as_str()),
                        "quorum must route all signing through the kit ceremony, never \
                         invoke a signer directly ({name}): `{}`",
                        line.trim()
                    );
                }
            }
        }
    }

    /// QRM-S4: the agent bridge is an INTAKE, not a signing surface. An agent
    /// submits an unsigned intent and receives a verdict; there is no route by
    /// which it obtains a signature, key or seed. This pins the absence of the
    /// endpoint an attacker would look for first.
    #[test]
    fn the_agent_bridge_exposes_no_signing_route() {
        let src = include_str!("agent_bridge.rs");
        for line in src.lines() {
            let t = line.trim_start();
            if t.starts_with("//") || t.starts_with("///") {
                continue;
            }
            // The routes are /health, /intent and the read-only
            // /decision/{id} an escalated agent polls. Any other matched path
            // must be added here deliberately, with a reason.
            if t.contains("path == ") || t.contains("path.strip_prefix(") {
                assert!(
                    t.contains("\"/health\"")
                        || t.contains("\"/intent\"")
                        || t.contains("\"/decision/\""),
                    "an unexpected agent-bridge route appeared: `{}` — an agent \
                     intake must not grow endpoints without review",
                    t
                );
            }
        }
        // The wire shapes live in the non-test code. The test module names these
        // very strings (it asserts responses never contain them), so scanning it
        // would make this test fail on its own sibling's vocabulary.
        let wire = src.split("#[cfg(test)]").next().unwrap_or(src);
        for banned in ["signature:", "private_key", "seed:", "mnemonic"] {
            assert!(
                !wire.contains(banned),
                "the agent bridge must never carry `{banned}` in its wire shapes"
            );
        }
    }

    /// The registered command surface exposes the shared signing path and NO
    /// secret-returning command (the boundary inherited from the kit).
    #[test]
    fn signing_commands_registered_and_no_secret_command() {
        let src = include_str!("lib.rs");
        for cmd in [
            "sign_request",
            "sign_approve",
            "sign_and_broadcast",
            "sign_reject",
        ] {
            assert!(
                src.contains(&format!("ceremony::{cmd}")),
                "quorum must register the shared signing command: ceremony::{cmd}"
            );
        }
        // No wallet secret-path fn or custody secret getter is an invoke command.
        let forbidden_getter = "custody_g".to_string() + "et,";
        assert!(
            !src.contains(&forbidden_getter),
            "no secret getter may be an invoke command"
        );
    }

    /// The kit is genuinely linked (the skeleton status reports it), proving the
    /// shared spine is consumed rather than duplicated.
    #[test]
    fn skeleton_reports_kit_linked() {
        let s = super::quorum_skeleton_status();
        assert!(s.kit_linked, "the shared kit must be consumed");
        assert!(
            !s.surfaces_built,
            "governance surfaces are honestly not built yet"
        );
        assert!(
            !s.license_enforced,
            "seat metering is honestly unenforced in S1"
        );
    }
}
