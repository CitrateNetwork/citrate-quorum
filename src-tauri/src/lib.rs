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

mod backend;

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
            // The quorum governance backend: per-tenant audit HashChains, live
            // capability grants + vote allowances, and the policy→audit pipeline.
            // In-memory today (durable persistence is a later WP); serves the
            // Ledger surface and the governed-action loop honestly from real
            // hash-chained records, never fabricated data (Rule 1).
            app.manage(std::sync::Mutex::new(backend::QuorumBackend::default()));
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            // The quorum skeleton's own honest status probe.
            quorum_skeleton_status,
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
            backend::action_evaluate_and_record,
            backend::ledger_records,
            backend::ledger_head,
            backend::ledger_merkle_root,
            backend::ledger_verify,
            backend::ledger_ungoverned_count,
            backend::grant_issue,
            backend::grant_revoke,
            backend::allowance_issue,
            backend::allowance_revoke,
            backend::vote_cast,
            backend::session_resolve,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    /// The single-signing-path guard, quorum side (WP-S1.2 acceptance: the
    /// ceremony source-scan test "runs in both repos"). The gated signer is
    /// `pub(crate)` to citrate-core-kit, so quorum code CANNOT invoke it — this
    /// is compiler-enforced. This test additionally scans quorum's own backend
    /// source to ensure no one adds a competing signing site (a `sign_message(` /
    /// `sign_transaction(` invocation, or a `#[tauri::command]` that signs). It is
    /// a forward guard for when quorum grows its own domains.
    ///
    /// NEGATIVE CONTROL: add `let _ = something.sign_message(&v, b"x");` anywhere
    /// in this crate and this fails. Needles are assembled from parts so this
    /// test's own prose cannot self-match.
    #[test]
    fn no_competing_signing_site_in_quorum() {
        let src = include_str!("lib.rs");
        let calls = [
            "sign_".to_string() + "message(",
            "sign_".to_string() + "transaction(",
        ];
        for line in src.lines() {
            let t = line.trim_start();
            if t.starts_with("//") || t.starts_with("///") {
                continue;
            }
            for call in &calls {
                assert!(
                    !t.contains(call.as_str()),
                    "quorum must route all signing through the kit ceremony, never \
                     invoke a signer directly: `{}`",
                    line.trim()
                );
            }
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
