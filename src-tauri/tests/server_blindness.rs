//! The workspace denies `expect`/`unwrap`/`panic` in production code. This is an
//! integration TEST: an assertion that cannot fail loudly is not an assertion, and
//! the whole file exists to fail loudly. Same allowance the unit tests take.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! **The QRM-S3 exit gate**, run as a test rather than asserted in a deck.
//!
//! Planset: *"Two humans + two agents in a room; relay reads nothing (proof
//! test)."* Both halves are here, and the second half is the one that matters:
//! anyone can claim end-to-end encryption, so this takes a known phrase, puts it
//! through a real four-member MLS group, and then goes through **everything the
//! relay saw and stored** looking for it.
//!
//! Why this lives in citrate-quorum rather than upstream: the claim being tested
//! is quorum's, and it is about quorum's arrangement of seats — two of the four
//! members are agents whose keys this app holds. That arrangement is what makes
//! "an agent in the room" cryptographically true while an agent still holds no
//! key, and it is the thing a customer's security review will poke at.
//!
//! The relay under test is a real `RelayServer` with its real store, in-process so
//! the test can open the drawer afterwards. The wire is the same wire; what makes
//! the deployed relay blind is the same code path, and `live_relay_*` in
//! `comms-session` covers the deployed one end to end.

use comms_core::identity::EthWallet;
use comms_proto::WalletAddress;
use comms_relay::ws::RelayServer;
use comms_relay::DeliveryService;
use comms_session::{Inbound, NetSession};

const DOMAIN: &str = "relay.citrate.ai";

/// The phrase the relay must not be able to find. Distinctive on purpose: a
/// substring search for it across everything the relay holds is only meaningful
/// if a false positive is implausible.
const SECRET: &str = "CUI-7731 line-4 tolerance stack-up is out of spec on the aft spar";

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// Everything the relay could possibly be storing, flattened to bytes.
///
/// Deliberately over-broad: this walks the relay's on-disk store directory and
/// reads every file, so a future field, index, log line or debug dump is covered
/// by construction rather than by remembering to add it here. A test that only
/// checked the fields we know about would pass the day someone adds one.
fn everything_the_relay_holds(dir: &std::path::Path) -> Vec<(String, Vec<u8>)> {
    let mut out = Vec::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(p) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&p) else {
            continue;
        };
        for e in entries.flatten() {
            let path = e.path();
            if path.is_dir() {
                stack.push(path);
            } else if let Ok(bytes) = std::fs::read(&path) {
                out.push((path.display().to_string(), bytes));
            }
        }
    }
    out
}

fn contains(haystack: &[u8], needle: &[u8]) -> bool {
    haystack.windows(needle.len()).any(|w| w == needle)
}

/// Four members — two human seats, two agent seats — in one room, and the relay
/// holding nothing readable from it.
#[tokio::test]
async fn two_humans_and_two_agents_share_a_room_the_relay_cannot_read() {
    let store_dir = tempfile::tempdir().expect("a store dir");

    // The owner seat is the workspace trust anchor (it registers the group).
    let owner_w = EthWallet::generate();
    let owner_addr = owner_w.address();
    // A REAL on-disk store (RocksDB + AES-256-GCM), so the proof below can open
    // the drawer afterwards rather than trusting an in-memory claim. The master
    // key is a test constant; in production it comes from the OS keyring.
    let service = DeliveryService::open(store_dir.path(), DOMAIN, owner_addr, [7u8; 32], 0)
        .expect("delivery service on a real store");
    let server = RelayServer::new(service);
    let (addr, _accept) = server.bind("127.0.0.1:0").await.expect("relay bound");
    let url = format!("ws://{addr}");

    // Four seats. In the app, the two human seats belong to two operators at two
    // machines, and the two agent seats are keys THIS app holds on the agents'
    // behalf (see rooms.rs). To the relay they are four wallets; that
    // indistinguishability is the property under test.
    let seats: Vec<(&str, bool, EthWallet)> = vec![
        ("R. Ortiz", true, owner_w),
        ("M. Okonkwo", true, EthWallet::generate()),
        ("claude-code", false, EthWallet::generate()),
        ("codex", false, EthWallet::generate()),
    ];
    let addresses: Vec<WalletAddress> = seats.iter().map(|(_, _, w)| w.address()).collect();

    let mut sessions = Vec::new();
    for (name, _, w) in seats {
        let s = NetSession::login(&url, DOMAIN, Box::new(w), now_ms(), false)
            .await
            .unwrap_or_else(|e| panic!("{name} could not log in: {e}"));
        sessions.push(s);
    }
    // Everyone but the owner publishes a KeyPackage so the owner can add them.
    for s in sessions.iter().skip(1) {
        s.publish_keypackage().await.expect("publish keypackage");
    }

    let (owner, rest) = sessions.split_at_mut(1);
    let owner = &mut owner[0];
    let gid = owner
        .create_channel(&addresses[1..])
        .await
        .expect("owner creates the room");
    for (i, s) in rest.iter_mut().enumerate() {
        let joined = s.join_next_channel().await.expect("member joins");
        assert_eq!(joined, gid, "seat {} joined a different group", i + 1);
    }

    // A human says something classified; an agent answers. Both go through the
    // group, so both are only readable by the four members.
    owner.send_text(SECRET).await.expect("owner sends");
    let mut heard_by = 0;
    for s in rest.iter_mut() {
        // Each member drains until it sees the application message (Commits for
        // the later joins arrive first).
        loop {
            match s.recv().await.expect("recv") {
                Inbound::Message { text, .. } if text == SECRET => {
                    heard_by += 1;
                    break;
                }
                Inbound::Message { .. } | Inbound::System { .. } => continue,
            }
        }
    }
    assert_eq!(heard_by, 3, "every other member decrypted the message");

    // ── The proof. ──────────────────────────────────────────────────────────
    let plaintext = SECRET.as_bytes();
    let held = everything_the_relay_holds(store_dir.path());
    for (path, bytes) in &held {
        assert!(
            !contains(bytes, plaintext),
            "the relay is holding the plaintext at {path} — server-blindness is broken"
        );
    }
    // A whole-word from the phrase, in case an encoding split it: "tolerance"
    // appears nowhere in ciphertext by chance.
    for (path, bytes) in &held {
        assert!(
            !contains(bytes, b"tolerance"),
            "a plaintext token from the transcript survived in the relay store at {path}"
        );
    }

    // Negative control: the search WOULD find the phrase if it were there. Without
    // this, a test that searched the wrong directory (or an empty one) would pass
    // for the wrong reason — which is exactly how a proof becomes theatre.
    let decoy = store_dir.path().join("decoy-control");
    std::fs::write(&decoy, SECRET.as_bytes()).expect("write decoy");
    let with_decoy = everything_the_relay_holds(store_dir.path());
    assert!(
        with_decoy.iter().any(|(_, b)| contains(b, plaintext)),
        "the search cannot find plaintext it is standing on — this proof proves nothing"
    );
    std::fs::remove_file(&decoy).ok();
}

/// The other half of the claim: what the relay *does* see is routing metadata,
/// and it sees it in the clear. Stating this precisely matters — "the relay reads
/// nothing" is false as a blanket sentence, and a customer's security reviewer
/// will find that out faster than we can walk it back.
#[tokio::test]
async fn the_relay_does_see_who_is_talking_to_whom() {
    let owner_w = EthWallet::generate();
    let owner_addr = owner_w.address();
    let service = DeliveryService::new(DOMAIN, owner_addr, 0).expect("service");
    let server = RelayServer::new(service);
    let (addr, _accept) = server.bind("127.0.0.1:0").await.expect("bound");
    let url = format!("ws://{addr}");

    let bob_w = EthWallet::generate();
    let bob_addr = bob_w.address();
    let alice_addr = owner_addr;
    let mut alice = NetSession::login(&url, DOMAIN, Box::new(owner_w), now_ms(), false)
        .await
        .expect("alice");
    let mut bob = NetSession::login(&url, DOMAIN, Box::new(bob_w), now_ms(), false)
        .await
        .expect("bob");
    bob.publish_keypackage().await.expect("kp");
    alice.create_channel(&[bob_addr]).await.expect("channel");
    bob.join_next_channel().await.expect("join");
    alice.send_text(SECRET).await.expect("send");

    match bob.recv().await.expect("recv") {
        Inbound::Message { sender, text, .. } => {
            assert_eq!(text, SECRET, "bob decrypts what alice sent");
            // The relay routed this by sender and recipient — both are metadata it
            // necessarily holds. The product's claim is confidentiality, not
            // anonymity, and the two must not be conflated in anything we say.
            assert_eq!(sender, alice_addr);
        }
        other => panic!("expected the message, got {other:?}"),
    }
}
