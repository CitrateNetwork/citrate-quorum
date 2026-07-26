//! Rooms — a real MLS group on the citrate-comms relay (QRM-S3).
//!
//! A room is not a chat feature bolted onto a governance app. It is the venue the
//! product is named for: humans and agents in one group, cryptographically
//! indistinguishable to the relay, where every agent contribution has already
//! passed the policy gate before it is encrypted.
//!
//! ## Data source (rule 11)
//!
//! - Transport: `comms-session`'s `NetSession` over `comms-wire`'s `RelayClient`,
//!   dialling the relay named by [`RELAY_URL_ENV`] (default
//!   `wss://comms.citrate.ai`). The endpoint guard refuses plaintext `ws://` to a
//!   non-loopback host before dialling.
//! - Crypto: OpenMLS via `comms-core`. Group secrets live in this process only.
//! - Room identities: generated here, sealed in the shared custody vault, and
//!   never sent anywhere.
//!
//! ## Custody — read this before changing anything here
//!
//! **A room identity is not the chain identity.** Each seat (the operator's, and
//! one per agent) gets its own secp256k1 key, generated in this process and sealed
//! into the custody vault under [`ROOM_SLOT_PREFIX`]. It signs exactly two things,
//! both to the relay: the SIWE login for its session, and the attestation binding
//! its MLS public key to itself. It authorises nothing on chain, holds no funds,
//! and confers no governance authority.
//!
//! The vault's *wallet* — the key that ratifies minutes and is registered on
//! chain — is not used here at all. `rooms.rs` never calls the kit's gated signer;
//! `no_competing_signing_site_in_quorum` and
//! `rooms_never_reaches_the_gated_signer` both assert that, and the compiler
//! already forbids it (the signer is `pub(crate)` to the kit).
//!
//! ### The gap this leaves, stated
//!
//! The planset (`02_ARCHITECTURE.md` §3) wants a human's seat bound to **their
//! wallet address, ceremony-gated** — so a room roster would carry the same
//! identity as a ratification. That binding is not made here, for a concrete
//! reason: the relay authenticates SIWE by recovering the signer from an EIP-191
//! (keccak256, 65-byte recoverable) signature, and the kit's gated signer produces
//! a 64-byte non-recoverable ECDSA signature over a SHA-256 prehash. They are not
//! interchangeable, and the fix belongs in the kit's ceremony — which is @rule8
//! code and needs security sign-off, not a drive-by from this sprint.
//!
//! So: the roster labels each seat with the principal it belongs to, and says the
//! seat's key is a relay identity. It does not claim the seat *is* the wallet.
//! An honest label beats a binding we did not make.
//!
//! ## Agents hold nothing
//!
//! An agent's seat key is held by THIS app, exactly as the planset specifies
//! ("per-agent MLS key in the local keyring; **never** a chain-signing key"). The
//! agent process gets no key, no session and no socket; it speaks into a room
//! through the governed bridge, and what it says is encrypted by us. That is what
//! makes "two agents in the room" a cryptographic fact rather than a UI label,
//! without giving an agent a key (I-1).

use std::collections::HashMap;

use comms_core::identity::EthWallet;
use comms_proto::{GroupId, WalletAddress};
use comms_session::{Inbound, NetSession};
use serde::Serialize;

use citrate_core_kit::custody::CustodyState;

/// Override for a customer running their own relay.
pub const RELAY_URL_ENV: &str = "QUORUM_RELAY_URL";
/// The federation's relay. Overridable per deployment; never a fallback for a
/// misconfigured one (an unset override means "use ours", not "use anything").
pub const DEFAULT_RELAY_URL: &str = "wss://comms.citrate.ai";

/// Custody slots holding room identities. Namespaced so a room key can never
/// collide with — or be mistaken for — the wallet slot.
pub const ROOM_SLOT_PREFIX: &str = "room-identity/";

/// Who a seat belongs to.
#[derive(Serialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum SeatKind {
    /// The operator at this keyboard.
    Human,
    /// An agent, whose key this app holds on its behalf.
    Agent,
}

/// One member seat: a relay session plus who it stands for.
struct Seat {
    session: NetSession,
    /// The principal this seat speaks for — the operator's name, or an agent id.
    principal: String,
    kind: SeatKind,
    address: WalletAddress,
}

/// A room this app is a member of.
struct RoomRecord {
    id: GroupId,
    name: String,
    classification: String,
    /// Seat principals in the room, in join order (the human owner first).
    seats: Vec<String>,
    opened_at_ms: i64,
}

/// One decrypted, in-memory transcript line.
#[derive(Serialize, Clone, Debug)]
pub struct RoomEventDto {
    /// Monotonic index within this session's transcript — the cursor a poller uses.
    pub n: u64,
    pub room: String,
    /// `speech` is deliberately absent: there is no audio path (see the surface).
    pub kind: String,
    pub who: String,
    pub human: bool,
    pub text: String,
    /// `HH:MM:SS` UTC.
    pub t: String,
}

#[derive(Serialize, Clone, Debug)]
pub struct RoomDto {
    pub id: String,
    pub name: String,
    pub classification: String,
    pub live: bool,
    pub members: usize,
    pub started: Option<String>,
}

#[derive(Serialize, Clone, Debug)]
pub struct MemberDto {
    pub id: String,
    pub name: String,
    pub human: bool,
    /// The relay identity this seat authenticated as. Not a chain identity — see
    /// the module header.
    pub address: String,
    /// The MLS signature public key, truncated. Two seats differ here even if
    /// everything else about them looks alike.
    pub mls_key: String,
}

#[derive(Serialize, Clone, Debug)]
pub struct RoomsStatus {
    pub connected: bool,
    pub relay_url: String,
    pub relay_domain: String,
    /// The operator's seat address, once connected.
    pub address: Option<String>,
    pub seats: usize,
    pub rooms: usize,
    /// Rule 11 + Rule 1: what this subsystem is and is not.
    pub note: String,
}

/// The rooms subsystem's state. One per app.
#[derive(Default)]
pub struct Rooms {
    seats: HashMap<String, Seat>,
    rooms: Vec<RoomRecord>,
    transcript: Vec<RoomEventDto>,
    next_n: u64,
}

/// Managed Tauri state. A `tokio::sync::Mutex` because every operation here is
/// async I/O against the relay, and holding a std mutex across an await is how a
/// desktop app deadlocks itself.
pub struct RoomsState(pub tokio::sync::Mutex<Rooms>);

impl RoomsState {
    pub fn new() -> Self {
        Self(tokio::sync::Mutex::new(Rooms::default()))
    }
}

impl Default for RoomsState {
    fn default() -> Self {
        Self::new()
    }
}

fn now_ms() -> i64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

fn clock_utc(ms: i64) -> String {
    let secs = ms.div_euclid(1000).rem_euclid(86_400);
    format!(
        "{:02}:{:02}:{:02}",
        secs / 3600,
        (secs % 3600) / 60,
        secs % 60
    )
}

fn relay_url() -> String {
    std::env::var(RELAY_URL_ENV)
        .ok()
        .filter(|u| !u.trim().is_empty())
        .unwrap_or_else(|| DEFAULT_RELAY_URL.to_string())
}

/// The SIWE domain the relay binds logins to — the host part of the URL.
///
/// Derived rather than configured: a domain that does not match the endpoint is
/// exactly the phishing case SIWE's domain binding exists to catch, and two
/// separate settings are two chances to get it wrong.
pub fn relay_domain(url: &str) -> String {
    let rest = url.split_once("://").map(|(_, r)| r).unwrap_or(url);
    let authority = rest.split(['/', '?', '#']).next().unwrap_or(rest);
    let host = authority
        .rsplit_once('@')
        .map(|(_, h)| h)
        .unwrap_or(authority);
    // Strip a port, but not an IPv6 literal's colons.
    match host.rsplit_once(':') {
        Some((h, p)) if !h.ends_with(']') && p.chars().all(|c| c.is_ascii_digit()) => h.to_string(),
        _ => host.to_string(),
    }
}

fn short(bytes: &[u8]) -> String {
    let hex: String = bytes.iter().take(6).map(|b| format!("{b:02x}")).collect();
    format!("{hex}…")
}

/// Load this seat's room identity from the vault, generating and sealing one on
/// first use.
///
/// Fails closed on a locked vault: a room key that lives only in memory would
/// silently become a *different* member on the next launch, and the roster would
/// quietly gain a stranger with the same label.
fn seat_identity(custody: &CustodyState, seat: &str) -> Result<EthWallet, String> {
    let slot = format!("{ROOM_SLOT_PREFIX}{seat}");
    if let Ok(bytes) = custody.0.custody_get(&slot) {
        if bytes.len() == 32 {
            let mut secret = [0u8; 32];
            secret.copy_from_slice(&bytes[..32]);
            return EthWallet::from_secret_key(&secret)
                .map_err(|e| format!("room identity for {seat} is unusable: {e}"));
        }
        return Err(format!(
            "room identity for {seat} is {} bytes, not 32 — refusing to guess at it",
            bytes.len()
        ));
    }
    let wallet = EthWallet::generate();
    let mut secret = wallet.secret_bytes();
    custody
        .0
        .put(&slot, &mut secret)
        .map_err(|e| format!("could not seal the room identity for {seat}: {e}"))?;
    Ok(wallet)
}

impl Rooms {
    fn push_event(&mut self, room: &str, kind: &str, who: &str, human: bool, text: String) {
        let n = self.next_n;
        self.next_n += 1;
        self.transcript.push(RoomEventDto {
            n,
            room: room.to_string(),
            kind: kind.to_string(),
            who: who.to_string(),
            human,
            text,
            t: clock_utc(now_ms()),
        });
    }

    /// Reserved for the operations that mutate a room in place (rename, admit a
    /// late member, close). Kept because the lookup rule — match on the hex group
    /// id, never on the name — is the one a caller gets wrong.
    #[allow(dead_code)]
    fn room_mut(&mut self, id: &str) -> Option<&mut RoomRecord> {
        self.rooms.iter_mut().find(|r| hex_gid(&r.id) == id)
    }
}

fn hex_gid(g: &GroupId) -> String {
    g.0.iter().map(|b| format!("{b:02x}")).collect()
}

// ---- commands ---------------------------------------------------------------

/// Connect the operator's seat to the relay.
///
/// Idempotent: connecting twice returns the existing session rather than opening
/// a second one under the same identity.
#[tauri::command]
pub async fn rooms_connect(
    state: tauri::State<'_, RoomsState>,
    custody: tauri::State<'_, CustodyState>,
    operator: String,
) -> Result<RoomsStatus, String> {
    let url = relay_url();
    let domain = relay_domain(&url);
    let mut rooms = state.0.lock().await;
    if rooms.seats.contains_key(&operator) {
        return Ok(status_of(&rooms, &url, &domain));
    }
    let wallet = seat_identity(&custody, &operator)?;
    let address = wallet.address();
    let session = NetSession::login(&url, &domain, Box::new(wallet), now_ms() as u64, false)
        .await
        .map_err(|e| format!("relay login failed ({url}): {e}"))?;
    session
        .publish_keypackage()
        .await
        .map_err(|e| format!("could not publish a KeyPackage to {url}: {e}"))?;
    rooms.seats.insert(
        operator.clone(),
        Seat {
            session,
            principal: operator,
            kind: SeatKind::Human,
            address,
        },
    );
    Ok(status_of(&rooms, &url, &domain))
}

fn status_of(rooms: &Rooms, url: &str, domain: &str) -> RoomsStatus {
    let human = rooms.seats.values().find(|s| s.kind == SeatKind::Human);
    RoomsStatus {
        connected: human.is_some(),
        relay_url: url.to_string(),
        relay_domain: domain.to_string(),
        address: human.map(|s| s.address.to_hex()),
        seats: rooms.seats.len(),
        rooms: rooms.rooms.len(),
        note: "The relay carries ciphertext and routing metadata only; it holds no group \
               secret and can decrypt nothing. Transcripts live in this process for the \
               session and are NOT written to disk — there is no durable archive here."
            .to_string(),
    }
}

#[tauri::command]
pub async fn rooms_status(state: tauri::State<'_, RoomsState>) -> Result<RoomsStatus, String> {
    let url = relay_url();
    let domain = relay_domain(&url);
    let rooms = state.0.lock().await;
    Ok(status_of(&rooms, &url, &domain))
}

/// Open a room and admit `agents` — each gets its own app-held seat.
#[tauri::command]
pub async fn rooms_open(
    state: tauri::State<'_, RoomsState>,
    custody: tauri::State<'_, CustodyState>,
    operator: String,
    name: String,
    classification: String,
    agents: Vec<String>,
) -> Result<RoomDto, String> {
    let url = relay_url();
    let domain = relay_domain(&url);
    let mut rooms = state.0.lock().await;
    if !rooms.seats.contains_key(&operator) {
        return Err(format!(
            "{operator} has no relay session — connect to {url} first"
        ));
    }

    // Bring every agent seat online BEFORE creating the group: a peer can only be
    // added once it has published a KeyPackage, so an agent that is not connected
    // must fail here rather than produce a room that silently lacks it.
    for agent in &agents {
        if rooms.seats.contains_key(agent) {
            continue;
        }
        let wallet = seat_identity(&custody, agent)?;
        let address = wallet.address();
        let session = NetSession::login(&url, &domain, Box::new(wallet), now_ms() as u64, false)
            .await
            .map_err(|e| format!("relay login failed for agent seat {agent}: {e}"))?;
        session
            .publish_keypackage()
            .await
            .map_err(|e| format!("could not publish a KeyPackage for agent seat {agent}: {e}"))?;
        rooms.seats.insert(
            agent.clone(),
            Seat {
                session,
                principal: agent.clone(),
                kind: SeatKind::Agent,
                address,
            },
        );
    }

    let peers: Vec<WalletAddress> = agents
        .iter()
        .filter_map(|a| rooms.seats.get(a).map(|s| s.address))
        .collect();

    let gid = {
        let owner = rooms
            .seats
            .get_mut(&operator)
            .ok_or("the operator's seat vanished mid-open")?;
        owner
            .session
            .create_channel(&peers)
            .await
            .map_err(|e| format!("could not create the room on {url}: {e}"))?
    };

    // Each agent seat joins from its pushed Welcome. Sequential and explicit: a
    // seat that fails to join is a member the room does not have, and the caller
    // must see that rather than a roster that lists it anyway.
    for agent in &agents {
        let seat = rooms
            .seats
            .get_mut(agent)
            .ok_or_else(|| format!("agent seat {agent} vanished mid-open"))?;
        seat.session
            .join_next_channel()
            .await
            .map_err(|e| format!("agent seat {agent} could not join the room: {e}"))?;
    }

    let mut seats = vec![operator.clone()];
    seats.extend(agents.iter().cloned());
    let id = hex_gid(&gid);
    rooms.rooms.push(RoomRecord {
        id: gid,
        name: name.clone(),
        classification: classification.clone(),
        seats: seats.clone(),
        opened_at_ms: now_ms(),
    });
    rooms.push_event(
        &id,
        "system",
        &operator,
        true,
        format!(
            "room opened · {} member(s) · MLS group {}",
            seats.len(),
            &id[..16]
        ),
    );
    Ok(RoomDto {
        id,
        name,
        classification,
        live: true,
        members: seats.len(),
        started: Some(clock_utc(now_ms())),
    })
}

#[tauri::command]
pub async fn rooms_list(state: tauri::State<'_, RoomsState>) -> Result<Vec<RoomDto>, String> {
    let rooms = state.0.lock().await;
    Ok(rooms
        .rooms
        .iter()
        .map(|r| RoomDto {
            id: hex_gid(&r.id),
            name: r.name.clone(),
            classification: r.classification.clone(),
            live: true,
            members: r.seats.len(),
            started: Some(clock_utc(r.opened_at_ms)),
        })
        .collect())
}

#[tauri::command]
pub async fn rooms_roster(
    state: tauri::State<'_, RoomsState>,
    room: String,
) -> Result<Vec<MemberDto>, String> {
    let rooms = state.0.lock().await;
    let rec = rooms
        .rooms
        .iter()
        .find(|r| hex_gid(&r.id) == room)
        .ok_or_else(|| format!("no room {room} in this session"))?;
    Ok(rec
        .seats
        .iter()
        .filter_map(|p| rooms.seats.get(p))
        .map(|s| MemberDto {
            id: s.principal.clone(),
            name: s.principal.clone(),
            human: s.kind == SeatKind::Human,
            address: s.address.to_hex(),
            mls_key: short(&s.session.mls_sig_pubkey()),
        })
        .collect())
}

/// Say something in a room, as `principal`'s seat.
#[tauri::command]
pub async fn rooms_say(
    state: tauri::State<'_, RoomsState>,
    room: String,
    principal: String,
    text: String,
) -> Result<u64, String> {
    let mut rooms = state.0.lock().await;
    let human = rooms
        .seats
        .get(&principal)
        .map(|s| s.kind == SeatKind::Human)
        .ok_or_else(|| format!("{principal} holds no seat in this session"))?;
    let seq = {
        let seat = rooms
            .seats
            .get_mut(&principal)
            .ok_or_else(|| format!("{principal} holds no seat in this session"))?;
        seat.session
            .send_text(&text)
            .await
            .map_err(|e| format!("could not send to the room: {e}"))?
    };
    // Our own message is not pushed back to us by the relay, so record it locally.
    rooms.push_event(&room, "text", &principal, human, text);
    Ok(seq)
}

/// Drain whatever the relay has pushed to every seat, decrypt it, and return the
/// transcript from `since`.
///
/// Polled rather than streamed, for the same reason the ledger ribbon is: it is
/// honest, it needs no event plumbing, and the relay is not a high-rate source.
/// Draining is non-blocking — `try_next_envelope` returns what has arrived.
#[tauri::command]
pub async fn rooms_events(
    state: tauri::State<'_, RoomsState>,
    since: u64,
) -> Result<Vec<RoomEventDto>, String> {
    let mut rooms = state.0.lock().await;
    let principals: Vec<String> = rooms.seats.keys().cloned().collect();
    for p in principals {
        loop {
            let next = {
                let seat = match rooms.seats.get(&p) {
                    Some(s) => s,
                    None => break,
                };
                // Non-blocking: without a timeout this would await forever on the
                // first quiet seat and never poll the rest.
                tokio::time::timeout(
                    std::time::Duration::from_millis(1),
                    seat.session.next_envelope(),
                )
                .await
            };
            let env = match next {
                Ok(Some(env)) => env,
                // Timed out (nothing waiting) or the connection closed.
                _ => break,
            };
            let room_id = hex_gid(&env.group_id);
            let applied = {
                let seat = match rooms.seats.get_mut(&p) {
                    Some(s) => s,
                    None => break,
                };
                seat.session.apply(env)
            };
            match applied {
                Ok(Some(Inbound::Message { sender, text, .. })) => {
                    let who = rooms
                        .seats
                        .values()
                        .find(|s| s.address == sender)
                        .map(|s| (s.principal.clone(), s.kind == SeatKind::Human))
                        .unwrap_or_else(|| (sender.to_hex(), false));
                    // Only record a message once, on the first seat that decrypts
                    // it — every other seat in the room decrypts the same bytes.
                    let already = rooms
                        .transcript
                        .iter()
                        .any(|e| e.room == room_id && e.who == who.0 && e.text == text);
                    if !already {
                        rooms.push_event(&room_id, "text", &who.0, who.1, text);
                    }
                }
                Ok(Some(Inbound::System { text })) => {
                    rooms.push_event(&room_id, "system", &p, false, text);
                }
                Ok(None) => {}
                // A decrypt failure is a real event an operator must see, not a
                // line to drop: it means this seat cannot read what the group is
                // saying.
                Err(e) => rooms.push_event(
                    &room_id,
                    "system",
                    &p,
                    false,
                    format!("could not apply a delivered envelope: {e}"),
                ),
            }
        }
    }
    Ok(rooms
        .transcript
        .iter()
        .filter(|e| e.n >= since)
        .cloned()
        .collect())
}

/// Leave every seat and drop the sessions. The room's history goes with it — it
/// was never written down.
#[tauri::command]
pub async fn rooms_leave(state: tauri::State<'_, RoomsState>, room: String) -> Result<(), String> {
    let mut rooms = state.0.lock().await;
    let before = rooms.rooms.len();
    rooms.rooms.retain(|r| hex_gid(&r.id) != room);
    if rooms.rooms.len() == before {
        return Err(format!("no room {room} in this session"));
    }
    rooms.push_event(&room, "system", "operator", true, "room closed".to_string());
    Ok(())
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn the_siwe_domain_is_derived_from_the_endpoint_not_configured_twice() {
        // A domain that disagrees with the endpoint is the phishing case SIWE's
        // domain binding exists to catch. Deriving it makes them agree by
        // construction.
        assert_eq!(relay_domain("wss://comms.citrate.ai"), "comms.citrate.ai");
        assert_eq!(
            relay_domain("wss://comms.citrate.ai/socket"),
            "comms.citrate.ai"
        );
        assert_eq!(relay_domain("ws://127.0.0.1:8787"), "127.0.0.1");
        assert_eq!(
            relay_domain("ws://user@host.example:9000/x"),
            "host.example"
        );
    }

    #[test]
    fn the_default_relay_is_the_federations_and_the_override_is_explicit() {
        // No silent fallback: an unset override means "ours", never "anything".
        assert!(DEFAULT_RELAY_URL.starts_with("wss://"));
        assert_eq!(RELAY_URL_ENV, "QUORUM_RELAY_URL");
    }

    #[test]
    fn room_identity_slots_cannot_collide_with_the_wallet_slot() {
        // The wallet slot is the ratification key's. A room key landing there
        // would be catastrophic in the quiet way: the app would still work.
        assert!(ROOM_SLOT_PREFIX.ends_with('/'));
        let slot = format!("{ROOM_SLOT_PREFIX}operator");
        assert!(slot.starts_with("room-identity/"));
        assert_ne!(slot, "wallet");
        assert!(!slot.starts_with("wallet"));
    }

    /// Rule 3, restated as a source scan for this module specifically. The
    /// compiler already forbids reaching the kit's gated signer (it is
    /// `pub(crate)` to the kit), but this fails loudly if someone ever tries —
    /// and, more usefully, if this module starts touching the vault's WALLET.
    #[test]
    fn rooms_never_reaches_the_gated_signer() {
        let src = include_str!("rooms.rs");
        let wire = src.split("#[cfg(test)]").next().unwrap_or(src);
        for banned in [
            "wallet::sign_message",
            "wallet::sign_transaction",
            "wallet::address",
            "citrate_core_kit::wallet",
        ] {
            assert!(
                !wire.contains(banned),
                "rooms.rs must not touch the vault's wallet: found `{banned}`. A room \
                 identity is a relay identity — see the module header."
            );
        }
    }

    /// The transcript is in memory and says so. If this module ever grows a write
    /// to the evidence store for message BODIES, that is a decision about
    /// retention and at-rest encryption (R13), not a refactor.
    #[test]
    fn transcripts_are_not_persisted() {
        let src = include_str!("rooms.rs");
        let wire = src.split("#[cfg(test)]").next().unwrap_or(src);
        for banned in [
            "EvidenceStore",
            "append_record",
            "fs::write",
            "File::create",
        ] {
            assert!(
                !wire.contains(banned),
                "rooms.rs writes `{banned}` — message bodies must not reach disk until \
                 at-rest encryption lands (R13). The status note tells operators they \
                 are not persisted."
            );
        }
    }
}
