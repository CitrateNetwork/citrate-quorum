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
//! ### The human seat IS the wallet (as of the kit's EIP-191 path)
//!
//! The planset (`02_ARCHITECTURE.md` §3) wants a human's seat bound to **their
//! wallet address, ceremony-gated**, so a room roster carries the same identity as
//! a ratification. That binding is now made: [`rooms_connect_intent`] opens the
//! socket, takes the relay's challenge nonce, builds the SIWE message and hands it
//! to the SignatureCeremony as a `personal_sign` intent. A human approves it —
//! seeing the actual EIP-4361 text — and [`rooms_connect_complete`] finishes the
//! handshake with that signature. The relay recovers the operator's wallet address
//! and the roster shows it.
//!
//! One approval per SESSION, not per message: MLS group operations sign with the
//! member's own credential key, generated in-process. "Connect this machine to the
//! relay as me" is the act that deserves a human; "send this line of chat" is not.
//!
//! **What is still not wallet-bound: publishing a KeyPackage.** Its binding
//! attestation signs a BLAKE3 domain-separated digest, not an EIP-191 message, and
//! the kit deliberately exposes no "sign this arbitrary 32-byte digest" primitive —
//! that primitive would sign a transaction hash just as happily. So the operator's
//! seat does not publish one, which means it can OWN rooms (it creates them) but
//! cannot be ADDED to someone else's. That limit is real, it is stated on the
//! surface, and closing it is a citrate-comms protocol change (an EIP-191 binding
//! attestation), not something to fake here.
//!
//! ## MR-4 — a room's classification bounds who may be in it
//!
//! A room carries a classification, and an agent seat may only be admitted when a
//! LIVE capability grant clears it to at least that level ([`mr4_admits`]). This
//! is not a preference the operator can wave through: the ceiling comes from the
//! grants this tenant actually issued, and a revoked or expired grant stops
//! clearing its agent immediately (CG-2). An agent with no grant at all is
//! refused with a different sentence than one that is merely under-cleared,
//! because those are different problems for the operator to fix.
//!
//! The monotonic half: a room's classification is fixed when it opens and there
//! is no path that lowers it. Admitting somebody must never reclassify what has
//! already been said in front of the people who were already there — the same
//! rule `quorum-meetings` enforces for attendance, applied to a live room.
//!
//! **What is NOT bounded, and must be said:** the operator's own clearance. It
//! would come from the least of their commercial tier, their on-chain clearance
//! (`ClassificationRegistry`, deployed but not read by this app) and their
//! tenant's `classification_max` (`TenantHierarchy`, deployed with no root). With
//! neither source live, a strict reading of MR-4 makes every operator Public and
//! every room Public with them. Rather than fake a clearance or quietly exempt
//! the human, the room's classification is the operator's own declaration,
//! recorded, and the surface says it is not verified. The agent half is real
//! today and is the half that governs what an autonomous thing may hear.
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

/// A connected-but-unauthenticated seat, waiting for a human to approve its SIWE
/// login. Holding it keeps the socket — and the relay's single-use nonce — alive
/// while someone reads what they are signing.
struct PendingSeat {
    pending: comms_session::PendingLogin,
    principal: String,
    address: WalletAddress,
    /// The ceremony this login's signature must come from.
    ceremony_id: String,
}

/// What the frontend needs to run the approval.
#[derive(Serialize, Clone, Debug)]
pub struct ConnectIntent {
    pub ceremony_id: String,
    /// The wallet address the seat will claim — the operator's own.
    pub address: String,
    pub relay_url: String,
    /// The exact EIP-4361 text being signed, for display.
    pub siwe: String,
}

/// The rooms subsystem's state. One per app.
#[derive(Default)]
pub struct Rooms {
    seats: HashMap<String, Seat>,
    pending_login: Option<PendingSeat>,
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

/// Load an AGENT seat's room identity from the vault, generating and sealing one
/// on first use.
///
/// Only agent seats. The operator's seat authenticates as the vault's own wallet
/// through the ceremony ([`rooms_connect_intent`]), so it has no generated key.
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

// ---- MR-4 ------------------------------------------------------------------

/// Why a seat may not enter a room. Each case is a different thing to fix, so
/// they are different values rather than one string.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AdmissionRefusal {
    /// The agent holds no live grant in this tenant at all.
    NoGrant { agent: String },
    /// It holds one, but not to this classification.
    Undercleared {
        agent: String,
        cleared_to: String,
        room: String,
    },
    /// The room's classification is not one this build knows.
    UnknownClassification { room: String },
}

impl std::fmt::Display for AdmissionRefusal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NoGrant { agent } => write!(
                f,
                "{agent} holds no live capability grant in this tenant, so nothing \
                 clears it for a classified room. Issue one on the Agents surface \
                 (a revoked or expired grant stops clearing immediately)."
            ),
            Self::Undercleared {
                agent,
                cleared_to,
                room,
            } => write!(
                f,
                "{agent} is cleared to {cleared_to}; this room is {room}. Admitting it \
                 would put a {room} conversation in front of something granted only \
                 {cleared_to} — MR-4 refuses rather than silently downgrading the room."
            ),
            Self::UnknownClassification { room } => write!(
                f,
                "'{room}' is not a classification this build knows (Public, \
                 Proprietary, CUI, ITAR). Refusing rather than guessing which."
            ),
        }
    }
}

/// **MR-4.** May an agent cleared to `ceiling` enter a room classified `room`?
///
/// Pure, so the rule is testable without a relay, a vault or a chain. `None` for
/// the ceiling means the agent holds no live grant — deliberately distinct from
/// `Some(Public)`, which means it holds one and it clears Public only.
pub fn mr4_admits(
    agent: &str,
    ceiling: Option<quorum_tenancy::Classification>,
    room: &str,
) -> Result<(), AdmissionRefusal> {
    let Some(room_class) = crate::store::classification_from_str(room) else {
        return Err(AdmissionRefusal::UnknownClassification {
            room: room.to_string(),
        });
    };
    match ceiling {
        None => Err(AdmissionRefusal::NoGrant {
            agent: agent.to_string(),
        }),
        Some(c) if c < room_class => Err(AdmissionRefusal::Undercleared {
            agent: agent.to_string(),
            cleared_to: crate::store::classification_str(c).to_string(),
            room: room.to_string(),
        }),
        Some(_) => Ok(()),
    }
}

// ---- commands ---------------------------------------------------------------

/// **Phase 1 of connecting the operator's seat.** Opens the socket, takes the
/// relay's challenge nonce, builds the SIWE message, and hands it to the
/// SignatureCeremony. **Signs nothing.**
///
/// The returned ceremony id is what the human approves; `sign_approve` on that id
/// is the only thing that produces a signature, and it is single-use. The socket
/// stays open in the meantime because the nonce is bound to it — which is why
/// `comms-session` grew a two-phase login for exactly this shape.
#[tauri::command]
pub async fn rooms_connect_intent(
    state: tauri::State<'_, RoomsState>,
    custody: tauri::State<'_, citrate_core_kit::custody::CustodyState>,
    ceremony: tauri::State<'_, citrate_core_kit::ceremony::CeremonyState>,
    operator: String,
) -> Result<ConnectIntent, String> {
    use citrate_core_kit::ceremony::{IntentKind, SignatureIntent};

    let url = relay_url();
    let domain = relay_domain(&url);
    let mut rooms = state.0.lock().await;
    if rooms.seats.contains_key(&operator) {
        return Err(format!("{operator} already holds a seat on {url}"));
    }

    // The PUBLIC address only. This is the one thing rooms.rs reads from the
    // wallet, and it reads no key: the signature comes from the ceremony.
    let wallet = citrate_core_kit::wallet::address(&custody.0).map_err(|e| e.to_string())?;
    let address = parse_address(&wallet.address)?;

    let pending = NetSession::begin(&url, &domain, address, now_ms() as u64, false)
        .await
        .map_err(|e| format!("could not reach the relay at {url}: {e}"))?;

    // What the human will actually see and approve — the EIP-4361 text itself,
    // not a summary of it. `decode_personal_sign` in the kit surfaces UTF-8
    // verbatim, so the operator reads the domain, the address and the nonce they
    // are signing over.
    let siwe_text = pending.message().to_signing_string();
    let view = ceremony.0.request(SignatureIntent {
        origin: format!("rooms · {domain}"),
        kind: IntentKind::PersonalSign,
        chain_id: 40204,
        raw: format!("0x{}", hex::encode(siwe_text.as_bytes())),
    });

    rooms.pending_login = Some(PendingSeat {
        pending,
        principal: operator,
        address,
        ceremony_id: view.id.clone(),
    });
    Ok(ConnectIntent {
        ceremony_id: view.id,
        address: wallet.address,
        relay_url: url,
        siwe: siwe_text,
    })
}

/// **Phase 2.** Finish the handshake with the signature the ceremony produced.
///
/// The relay recovers the signer from the EIP-191 signature; if it recovers
/// anyone other than the address we claimed, the session is refused rather than
/// continued under a name we cannot substantiate.
#[tauri::command]
pub async fn rooms_connect_complete(
    state: tauri::State<'_, RoomsState>,
    ceremony_id: String,
    signature_hex: String,
) -> Result<RoomsStatus, String> {
    let url = relay_url();
    let domain = relay_domain(&url);
    let mut rooms = state.0.lock().await;
    let seat = rooms
        .pending_login
        .take()
        .ok_or("no connection is waiting for a signature — start with rooms_connect_intent")?;
    // Bind the signature to THIS intent. A signature approved for some other
    // ceremony must not complete this login (the ceremony is single-use on its
    // own side; this is the second half of that check, on ours).
    if seat.ceremony_id != ceremony_id {
        return Err(format!(
            "signature is for ceremony {ceremony_id}, but the pending login is {}",
            seat.ceremony_id
        ));
    }
    let raw = hex::decode(signature_hex.trim_start_matches("0x"))
        .map_err(|_| "signature is not hex".to_string())?;
    if raw.len() != 65 {
        return Err(format!(
            "expected a 65-byte recoverable signature (r||s||v), got {} bytes — the \
             ceremony must be signing personal_sign through the kit's EIP-191 path",
            raw.len()
        ));
    }
    let mut sig = [0u8; 65];
    sig.copy_from_slice(&raw);

    let session = seat
        .pending
        .complete(sig, None)
        .await
        .map_err(|e| format!("the relay refused the signed login: {e}"))?;
    rooms.seats.insert(
        seat.principal.clone(),
        Seat {
            session,
            principal: seat.principal,
            kind: SeatKind::Human,
            address: seat.address,
        },
    );
    Ok(status_of(&rooms, &url, &domain))
}

/// A 20-byte address from `0x…` hex.
fn parse_address(hex_addr: &str) -> Result<WalletAddress, String> {
    let body = hex_addr.strip_prefix("0x").unwrap_or(hex_addr);
    let bytes = hex::decode(body).map_err(|_| format!("not a hex address: {hex_addr}"))?;
    if bytes.len() != 20 {
        return Err(format!("not a 20-byte address: {hex_addr}"));
    }
    let mut a = [0u8; 20];
    a.copy_from_slice(&bytes);
    Ok(WalletAddress(a))
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
    backend: tauri::State<'_, std::sync::Arc<std::sync::Mutex<crate::backend::QuorumBackend>>>,
    operator: String,
    name: String,
    classification: String,
    agents: Vec<String>,
) -> Result<RoomDto, String> {
    let url = relay_url();
    let domain = relay_domain(&url);

    // MR-4 FIRST, before a single seat logs in. Checking after the seats exist
    // would leave an agent authenticated to the relay for a room it was then
    // refused from — a live session nobody accounted for. The lock is taken and
    // released here so no relay I/O happens while the evidence mutex is held.
    {
        let b = backend
            .lock()
            .map_err(|_| "backend lock poisoned".to_string())?;
        let tenant = b.require_tenant()?;
        let now = now_ms();
        for agent in &agents {
            let ceiling = b.agent_classification_ceiling(tenant.as_str(), agent, now);
            mr4_admits(agent, ceiling, &classification).map_err(|e| e.to_string())?;
        }
    }

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

    use quorum_tenancy::Classification;

    #[test]
    fn mr4_admits_an_agent_cleared_to_the_room_or_above() {
        // At the line and above it. A CUI grant is not spent by entering a
        // Proprietary room — a ceiling is a maximum, not an exact match.
        assert!(mr4_admits("codex", Some(Classification::Cui), "CUI").is_ok());
        assert!(mr4_admits("codex", Some(Classification::Itar), "CUI").is_ok());
        assert!(mr4_admits("codex", Some(Classification::Cui), "Proprietary").is_ok());
        assert!(mr4_admits("codex", Some(Classification::Public), "Public").is_ok());
    }

    #[test]
    fn mr4_refuses_an_undercleared_agent_and_says_both_levels() {
        let e = mr4_admits("claude-code", Some(Classification::Proprietary), "CUI")
            .expect_err("Proprietary must not enter a CUI room");
        match &e {
            AdmissionRefusal::Undercleared {
                agent,
                cleared_to,
                room,
            } => {
                assert_eq!(agent, "claude-code");
                assert_eq!(cleared_to, "Proprietary");
                assert_eq!(room, "CUI");
            }
            other => panic!("expected Undercleared, got {other:?}"),
        }
        // The operator has to know which side to fix, so both levels are in the
        // sentence, not just "denied".
        let msg = e.to_string();
        assert!(msg.contains("Proprietary") && msg.contains("CUI"), "{msg}");
        assert!(msg.contains("MR-4"), "{msg}");
    }

    /// "No grant" and "cleared to Public" are DIFFERENT problems: one is an
    /// operator who has not issued a capability, the other is a capability that
    /// does not reach. Collapsing them sends someone to fix the wrong thing.
    #[test]
    fn mr4_distinguishes_no_grant_from_cleared_to_public() {
        let none = mr4_admits("devin", None, "Proprietary").expect_err("no grant");
        assert!(matches!(none, AdmissionRefusal::NoGrant { .. }));
        assert!(none.to_string().contains("no live capability grant"));

        let public = mr4_admits("devin", Some(Classification::Public), "Proprietary")
            .expect_err("public grant");
        assert!(matches!(public, AdmissionRefusal::Undercleared { .. }));
        assert_ne!(none.to_string(), public.to_string());

        // …and an agent with no grant is still refused from a PUBLIC room. A
        // seat in a room is a capability, so "ungranted" never means "harmless".
        assert!(mr4_admits("devin", None, "Public").is_err());
    }

    #[test]
    fn mr4_refuses_a_classification_it_does_not_recognise() {
        // Never guess which level an unknown NAME means. Guessing down admits an
        // agent that should have been refused; guessing up locks out one that
        // should not have been. Either way the operator is not told.
        for bogus in ["Secret", "top-secret", "", "confidential", "CUI-2"] {
            assert!(
                matches!(
                    mr4_admits("codex", Some(Classification::Itar), bogus),
                    Err(AdmissionRefusal::UnknownClassification { .. })
                ),
                "{bogus} must not be interpreted"
            );
        }
        // Case and surrounding whitespace ARE accepted, and that is not a guess:
        // "cui" names exactly one level, unambiguously. The at-rest codec is
        // deliberately lenient here so a stored record round-trips, and MR-4
        // reuses it rather than keeping a second, stricter parser that could
        // disagree with what was persisted. (This assertion was the other way
        // round first — it was the test that was wrong.)
        assert!(mr4_admits("codex", Some(Classification::Itar), "cui").is_ok());
        assert!(mr4_admits("codex", Some(Classification::Itar), " Public ").is_ok());
        // …and leniency must not become permissiveness: a lower-cased name still
        // has to CLEAR.
        assert!(mr4_admits("codex", Some(Classification::Public), "cui").is_err());
    }

    /// Rule 3, restated as a source scan for this module specifically. The
    /// compiler already forbids reaching the kit's gated signer (it is
    /// `pub(crate)` to the kit), but this fails loudly if someone ever tries —
    /// and, more usefully, if this module starts touching the vault's WALLET.
    #[test]
    fn rooms_never_reaches_the_gated_signer() {
        let src = include_str!("rooms.rs");
        let wire = src.split("#[cfg(test)]").next().unwrap_or(src);
        // The SIGNERS, not the public address. `wallet::address` returns only the
        // 0x… address and no key material, and the human seat needs it to say who
        // it is claiming to be; the signature itself comes from the ceremony.
        for banned in [
            "wallet::sign_message",
            "wallet::sign_transaction",
            "wallet::sign_personal",
        ] {
            assert!(
                !wire.contains(banned),
                "rooms.rs must not invoke a gated signer: found `{banned}`. Every \
                 signature here comes from the SignatureCeremony (the human seat) or \
                 from a seat's own relay identity (agent seats) — see the module header."
            );
        }
        // The one wallet call that IS allowed, pinned so a future edit cannot
        // quietly widen it into something that signs.
        assert!(
            wire.matches("citrate_core_kit::wallet::").count() == 1
                && wire.contains("citrate_core_kit::wallet::address("),
            "the only permitted wallet call in rooms.rs is `address()` (public, no key)"
        );
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
