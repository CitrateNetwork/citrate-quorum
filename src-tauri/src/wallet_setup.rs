//! Wallet setup — the signing identity every ratification is attributed to.
//!
//! ## Why this exists
//!
//! `MeetingRegistry` records `msg.sender` as the ratifier, and
//! `ceremony::approve_and_broadcast` refuses to sign unless the transaction's
//! `from` equals this vault's own derived address. So the key that registers
//! minutes is the ratifying human's key, held here — there is no service key
//! and there must not be one (I-1). Until a wallet exists in the vault, nothing
//! in quorum can sign anything.
//!
//! Neither citrate-quorum nor citrate-core had a creation path: `wallet::create`
//! existed, was tested, and had zero production callers. This is that path.
//!
//! ## The I-2 exception, stated plainly
//!
//! The kit's rule is that **no invoke command returns secret material**, and a
//! BIP-39 recovery phrase is secret. [`wallet_create`] deliberately breaks that
//! rule, once, under an explicit owner decision (2026-07-26):
//!
//! - It is the **only** command in this app that returns secret material.
//! - It returns the phrase **exactly once**, at the moment of generation. There
//!   is no command that can read it back — [`wallet_status`] returns the address
//!   only, and the kit exposes no getter. Losing it means losing it.
//! - It **cannot overwrite** an existing wallet: the kit returns `AlreadyExists`,
//!   which is what stops a second call from silently minting a new identity and
//!   orphaning every ratification the old key signed.
//! - The caller's contract, from the kit's own header, is *display-and-drop,
//!   never persist*. The surface holds it in component state, renders it
//!   blurred until deliberately revealed, and clears it on continue.
//!
//! The alternative — never showing the phrase — makes the OS keyring the sole
//! copy of the identity that signed every ratification, with no recovery if the
//! keyring is lost. That was judged the worse failure.
//!
//! [`wallet_import`] takes a phrase *in* and returns only an address, so it does
//! not widen the boundary at all.

use citrate_core_kit::custody::CustodyState;
use citrate_core_kit::wallet;
use serde::Serialize;

/// The wallet's public identity. No secret material.
#[derive(Serialize, Clone, Debug)]
pub struct WalletStatus {
    pub exists: bool,
    /// `None` when no wallet is stored, or when the vault is locked — the two
    /// are not distinguished here because custody deliberately gives no
    /// locked-vs-absent oracle.
    pub address: Option<String>,
    /// Why there is no address, when there is none. For the surface to render
    /// instead of guessing.
    pub reason: Option<String>,
}

/// What creation returns — **once**.
#[derive(Serialize)]
pub struct WalletCreated {
    pub address: String,
    /// The 24-word BIP-39 recovery phrase. THE ONLY SECRET THIS APP'S COMMAND
    /// SURFACE EVER RETURNS. It is never obtainable again by any means.
    pub mnemonic: String,
}

/// Is there a signing identity, and what is its address?
#[tauri::command]
pub fn wallet_status(custody: tauri::State<'_, CustodyState>) -> WalletStatus {
    match wallet::address(&custody.0) {
        Ok(info) => WalletStatus {
            exists: true,
            address: Some(info.address),
            reason: None,
        },
        Err(e) => WalletStatus {
            exists: false,
            address: None,
            reason: Some(e.to_string()),
        },
    }
}

/// Create the signing identity and return its recovery phrase ONCE.
///
/// Requires an unlocked vault. Refuses if a wallet already exists — see the
/// module header for why that refusal is load-bearing rather than a nicety.
#[tauri::command]
pub fn wallet_create(custody: tauri::State<'_, CustodyState>) -> Result<WalletCreated, String> {
    let created = wallet::create(&custody.0).map_err(|e| e.to_string())?;
    Ok(WalletCreated {
        address: created.address,
        // `created.mnemonic` is `Zeroizing<String>`; this copy is the one that
        // crosses the boundary. It is not logged, not persisted, and not
        // recoverable after this response.
        mnemonic: created.mnemonic.to_string(),
    })
}

/// Adopt an existing identity from a recovery phrase. Returns the address only.
#[tauri::command]
pub fn wallet_import(
    custody: tauri::State<'_, CustodyState>,
    mnemonic: String,
) -> Result<WalletStatus, String> {
    let info = wallet::import(&custody.0, &mnemonic).map_err(|e| e.to_string())?;
    Ok(WalletStatus {
        exists: true,
        address: Some(info.address),
        reason: None,
    })
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    /// The I-2 exception must stay exactly one command wide.
    ///
    /// This is a source scan rather than a type check because the property is
    /// about the SHAPE of the command surface: if a second command ever starts
    /// returning a phrase, or if `wallet_create` stops being the only one, the
    /// owner decision that authorised this no longer covers what shipped.
    #[test]
    fn exactly_one_command_returns_secret_material() {
        // Scan the PRODUCTION half only: this test's own source contains the
        // marker as a literal, which would otherwise count itself.
        let src = include_str!("wallet_setup.rs");
        let prod = src.split("#[cfg(test)]").next().expect("production half");
        let returns_mnemonic = prod.matches("pub mnemonic: String").count();
        assert_eq!(
            returns_mnemonic, 1,
            "exactly one response type may carry a mnemonic; found {returns_mnemonic}"
        );

        // `wallet_import` takes a phrase IN and must return only an address.
        let import = prod
            .split("pub fn wallet_import")
            .nth(1)
            .expect("wallet_import must exist");
        let import_sig = import.split('{').next().unwrap_or("");
        assert!(
            import_sig.contains("Result<WalletStatus, String>"),
            "wallet_import must return WalletStatus (address only), got: {import_sig}"
        );
    }

    /// `WalletStatus` is the shape the app reads all the time; it must never
    /// grow a secret field.
    #[test]
    fn the_status_shape_carries_no_secret() {
        let src = include_str!("wallet_setup.rs");
        let src = src.split("#[cfg(test)]").next().expect("production half");
        let status = src
            .split("pub struct WalletStatus")
            .nth(1)
            .and_then(|s| s.split('}').next())
            .expect("WalletStatus must exist");
        for banned in ["mnemonic", "entropy", "seed", "private", "key"] {
            assert!(
                !status.to_lowercase().contains(banned),
                "WalletStatus must not carry `{banned}`"
            );
        }
    }
}
