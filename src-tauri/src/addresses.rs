//! The canonical chain address book (WP-Z), read at runtime.
//!
//! CLAUDE.md rule 8: *"No hardcoded chain addresses. Read the frozen address
//! book at runtime; a mismatch is a hard error, never a silent fallback."*
//!
//! ## Where the book comes from
//!
//! `scripts/sync-addresses.sh` vendors
//! `citrate-chain/contracts/addresses/40204.json` to
//! `src/generated/addresses.json`. That file is compiled in as the default so a
//! packaged installer carries a book without needing the chain repo beside it.
//!
//! A deployment pointed at its own chain overrides it with
//! `QUORUM_ADDRESS_BOOK=/path/to/book.json`. The override is read from disk at
//! call time, so a customer can repoint an installed app without a rebuild.
//!
//! ## Absent is not zero
//!
//! [`AddressBook::get`] returns `None` for a name the book does not carry, and
//! every caller must render that as unavailable. There is no default address,
//! no zero address, and no "try the other chain" path: a governance record
//! written against a guessed address is worse than one not written at all.

use std::collections::BTreeMap;

use serde::Deserialize;

/// The vendored canonical book. Regenerate with `scripts/sync-addresses.sh`.
const VENDORED: &str = include_str!("generated/addresses.json");

/// Environment override for a deployment running its own chain.
pub const OVERRIDE_ENV: &str = "QUORUM_ADDRESS_BOOK";

#[derive(Debug, Deserialize)]
struct RawBook {
    #[serde(rename = "chainId")]
    chain_id: u64,
    #[serde(rename = "rpcUrl")]
    rpc_url: Option<String>,
    contracts: BTreeMap<String, String>,
}

#[derive(Debug, Clone)]
pub struct AddressBook {
    pub chain_id: u64,
    pub rpc_url: Option<String>,
    contracts: BTreeMap<String, String>,
    /// Where this book was read from, for the Rule 11 provenance line.
    pub source: String,
}

#[derive(Debug)]
pub enum BookError {
    Parse { source: String, detail: String },
    Io { path: String, detail: String },
}

impl std::fmt::Display for BookError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Parse { source, detail } => {
                write!(f, "address book at {source} does not parse: {detail}")
            }
            Self::Io { path, detail } => {
                write!(f, "address book at {path} could not be read: {detail}")
            }
        }
    }
}
impl std::error::Error for BookError {}

impl AddressBook {
    /// Load the book: the `QUORUM_ADDRESS_BOOK` override if set, else the
    /// vendored copy.
    ///
    /// An override that is set but unreadable is an **error**, never a silent
    /// fall back to the vendored book — an operator who pointed the app at
    /// their chain and got ours would be reading someone else's addresses.
    pub fn load() -> Result<Self, BookError> {
        match std::env::var(OVERRIDE_ENV) {
            Ok(path) if !path.trim().is_empty() => {
                let raw = std::fs::read_to_string(&path).map_err(|e| BookError::Io {
                    path: path.clone(),
                    detail: e.to_string(),
                })?;
                Self::parse(&raw, &format!("{OVERRIDE_ENV}={path}"))
            }
            _ => Self::parse(VENDORED, "vendored src/generated/addresses.json"),
        }
    }

    pub fn parse(raw: &str, source: &str) -> Result<Self, BookError> {
        let book: RawBook = serde_json::from_str(raw).map_err(|e| BookError::Parse {
            source: source.to_string(),
            detail: e.to_string(),
        })?;
        Ok(Self {
            chain_id: book.chain_id,
            rpc_url: book.rpc_url,
            contracts: book.contracts,
            source: source.to_string(),
        })
    }

    /// The address booked under `name`, or `None` when the book does not carry
    /// it. `None` means unavailable — it never means "use a default".
    pub fn get(&self, name: &str) -> Option<&str> {
        self.contracts.get(name).map(String::as_str)
    }

    pub fn len(&self) -> usize {
        self.contracts.len()
    }

    /// A one-line description of which book answered, for Rule 11 provenance
    /// and for the "absent" diagnostics — an operator seeing "not in the book"
    /// needs to know WHICH book and WHICH chain was consulted.
    pub fn describe(&self) -> String {
        format!(
            "{}, chain {}, {} contracts",
            self.source,
            self.chain_id,
            self.len()
        )
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn the_vendored_book_parses_and_is_chain_40204() {
        let b = AddressBook::load().expect("vendored book must parse");
        assert_eq!(b.chain_id, 40204);
        assert!(b.len() > 40, "book looks truncated: {} entries", b.len());
    }

    #[test]
    fn the_vendored_book_carries_the_contracts_anchoring_needs() {
        // If this fails, `scripts/sync-addresses.sh` has not been run since
        // MeetingRegistry was deployed — and the anchor path will correctly,
        // but uselessly, report itself unavailable.
        let b = AddressBook::load().unwrap();
        assert!(b.get("MeetingRegistry").is_some());
        assert!(b.get("AnchorRegistry").is_some());
    }

    #[test]
    fn an_absent_name_is_none_never_a_default() {
        let b = AddressBook::load().unwrap();
        assert_eq!(b.get("NoSuchContract"), None);
    }

    #[test]
    fn addresses_are_hex_and_twenty_bytes() {
        let b = AddressBook::load().unwrap();
        for name in ["MeetingRegistry", "AnchorRegistry"] {
            let a = b.get(name).unwrap();
            assert!(a.starts_with("0x"), "{name}: {a}");
            assert_eq!(a.len(), 42, "{name}: {a}");
            assert!(a[2..].chars().all(|c| c.is_ascii_hexdigit()), "{name}: {a}");
        }
    }

    #[test]
    fn a_malformed_book_is_an_error_not_an_empty_one() {
        let e = AddressBook::parse("{ not json", "test").unwrap_err();
        assert!(format!("{e}").contains("does not parse"));
    }

    #[test]
    fn the_book_reports_where_it_came_from() {
        let b = AddressBook::load().unwrap();
        assert!(b.source.contains("addresses.json"), "{}", b.source);
    }
}
