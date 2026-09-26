//! QRM-S7.6c — the constructor arguments an audited template is deployed with.
//!
//! `GovernanceProtocolFactory.deployProtocol` concatenates `creationCode ‖
//! params` and CREATE2s the result, so these bytes are not decoration: they are
//! half of the deployed contract's identity.
//!
//! # Why this module exists at all
//!
//! S7.6 phase one shipped with `params = &[]`. That is wrong in two ways that
//! compound, and the second is worse than the first:
//!
//! 1. Every template's constructor rejects it — `ThresholdApproval` reverts
//!    `ZeroTenant` before it reverts `EmptyApproverSet` — so the deploy fails.
//! 2. `predict()` hashes `creationCode ‖ params`. With the wrong params, the
//!    address shown to a human **before they sign** is the address of a contract
//!    that cannot exist. The ceremony's whole promise (GF-1: approve a known
//!    address, not "a protocol, somewhere") is void, and it fails silently —
//!    the prediction is a perfectly well-formed address.
//!
//! So the parameters have to be real before the prediction means anything.
//!
//! # What this module will not do
//!
//! It will not invent a parameter. Every template below shares a five-argument
//! prefix `(tenantId, templateId, version, specHash, specCID)` that the deploy
//! context supplies, and then takes its own arguments — a `foreignNationalFloor`,
//! a `requiredRole`, a `maxScan`. Where the compiler does not extract one, the
//! template is **refused by name with the missing input named**, exactly as
//! [`crate::deploy::creation_code`] refuses a template whose audited bytes this
//! app does not carry.
//!
//! That is R-A one layer down. A default `foreignNationalFloor` would deploy a
//! real, audited, correctly-predicted contract that enforces a rule nobody
//! wrote — the most expensive kind of wrong, because everything about it looks
//! right.
//!
//! # The approver identity space
//!
//! The chain pins it. `MultiSigEnvelope` binds every identity it records to
//! the address that sent the transaction: `sign` requires
//! `signer == QuorumIdentity.subjectKey(msg.sender)`, `draft` requires
//! `initiator == QuorumIdentity.subjectKey(msg.sender)`, and only a required
//! signer may `accept` or `reject`. The envelope-reading templates then count
//! only identities they were constructed with — `ThresholdApproval`'s
//! approvers, `SegregationOfDuties`' roster — and ignore an envelope drafted by
//! anyone else (`*_FOREIGN_PROPOSER`).
//!
//! So an approver is `QuorumIdentity.subjectKey(address)` =
//! `keccak256("0x" ‖ lowercase hex)`, the same derivation as
//! [`crate::chain::clearance_subject`], and a principal must be written as an
//! address ([`envelope_identity`]). A principal known only by name cannot draft
//! or sign anything on chain, so a protocol built from names is a control that
//! can never be satisfied. That is refused here, by name
//! ([`CtorError::UnbindablePrincipal`]), before a human is asked to approve it.

use std::collections::BTreeMap;

use crate::addresses::AddressBook;
use crate::anchor::keccak256;

/// Everything a constructor needs that does not come from the clause itself.
pub struct DeployContext<'a> {
    pub tenant_id: [u8; 32],
    pub template_id: [u8; 32],
    pub version: u32,
    pub spec_hash: [u8; 32],
    pub spec_cid: &'a str,
    /// Typed parameters the compiler extracted for the mapped clause.
    pub params: &'a BTreeMap<String, String>,
    /// The spec's structural principals, as written. Each envelope-reading
    /// template turns them into on-chain identities with [`envelope_identity`],
    /// and refuses the ones that are not addresses.
    pub principals: &'a [String],
    /// The governance/RBAC book. Every contract a supported template's
    /// constructor takes (`MultiSigEnvelope` today) lives in this one, and each
    /// lookup names which book answered. The main book is deliberately NOT
    /// carried: nothing here reads it, and a field that is threaded through but
    /// never used reads as a wired seam that is not.
    pub bfr: &'a AddressBook,
}

#[derive(Debug, PartialEq, Eq)]
pub enum CtorError {
    /// This app cannot construct that template, because the pipeline does not
    /// produce one of its constructor arguments.
    Unsupported {
        template: String,
        missing: &'static str,
    },
    /// The template is supported but the compiler did not extract a parameter
    /// it needs — a compile that mapped the clause it should not have.
    MissingParam {
        template: String,
        param: &'static str,
    },
    /// A constructor takes the address of another contract and the book does
    /// not carry it.
    MissingContract {
        template: String,
        contract: &'static str,
        book: String,
    },
    /// The named parameter was not a number this argument can hold.
    BadParam {
        template: String,
        param: &'static str,
        value: String,
    },
    /// Fewer distinct approvers than the threshold requires.
    NotEnoughApprovers { have: usize, need: u32 },
    /// No approvers at all — the constructor reverts `EmptyApproverSet`.
    NoApprovers,
    /// A principal is not an address, so it has no identity the envelope can
    /// record: `draft` and `sign` bind every identity to the sending address.
    UnbindablePrincipal { principal: String },
    /// Two principals are the same on-chain identity (`DuplicateApprover` /
    /// `DuplicateRosterMember`).
    DuplicateApprover { principal: String },
    /// More identities than the templates bound (`ApproverSetTooLarge` /
    /// `RosterTooLarge`, both 64).
    TooManyApprovers { have: usize },
    /// `SegregationOfDuties` with `minApprovers == 0` reverts
    /// `MinApproversTooLow`: a protocol that requires nobody is a no-op.
    MinApproversTooLow,
    /// A roster that cannot seat one proposer plus `minApprovers` distinct
    /// approvers can never Allow (`RosterTooSmall`).
    RosterTooSmall { have: usize, min_approvers: u64 },
}

impl CtorError {
    pub fn why(&self) -> String {
        match self {
            CtorError::Unsupported { template, missing } => format!(
                "this app cannot deploy {template}: its constructor takes {missing}, \
                 and nothing in the pipeline produces one. Supplying a default would \
                 deploy a real, audited contract enforcing a rule nobody wrote — so \
                 it is refused instead. Extend the interview and the compiler to ask \
                 for it first. Deployable from here today: {}",
                SUPPORTED.join(", ")
            ),
            CtorError::MissingParam { template, param } => format!(
                "{template} needs a '{param}' parameter and the compiled clause has \
                 none. The clause should not have mapped — this is a compile bug, not \
                 an operator error"
            ),
            CtorError::MissingContract {
                template,
                contract,
                book,
            } => format!(
                "{template}'s constructor takes the address of {contract}, which is \
                 not in {book}. Run scripts/sync-addresses.sh against a chain where \
                 it is deployed"
            ),
            CtorError::BadParam {
                template,
                param,
                value,
            } => format!("{template}: '{param}' is not a number this argument can hold ({value})"),
            CtorError::NotEnoughApprovers { have, need } => format!(
                "the policy requires {need} approvals but the spec names {have} \
                 distinct principal(s). The contract would revert BadThreshold, and \
                 padding the set with a name nobody wrote is not a fix"
            ),
            CtorError::NoApprovers => "the spec names no principals, so there is nobody to \
                 approve. ThresholdApproval reverts EmptyApproverSet rather than deploy a \
                 protocol whose approver set is empty"
                .to_string(),
            CtorError::UnbindablePrincipal { principal } => format!(
                "'{principal}' is not an address. MultiSigEnvelope records the proposer \
                 and every signature as the identity of the address that sent it, so a \
                 principal known only by name can never propose or approve, and the \
                 protocol could never be satisfied. Name each approver by wallet address"
            ),
            CtorError::DuplicateApprover { principal } => format!(
                "'{principal}' appears twice as the same on-chain identity. The contract \
                 reverts on a duplicate, and one person cannot count as two approvers"
            ),
            CtorError::TooManyApprovers { have } => format!(
                "the spec names {have} principals; the governance templates accept at \
                 most {MAX_IDENTITIES}, so every gated action stays affordable to check"
            ),
            CtorError::MinApproversTooLow => "SegregationOfDuties needs at least one \
                 approver besides the proposer and the executor; zero would deploy a \
                 protocol that reads like a control and checks nothing"
                .to_string(),
            CtorError::RosterTooSmall { have, min_approvers } => format!(
                "SegregationOfDuties needs a proposer and {min_approvers} distinct \
                 approver(s) from its roster, so the roster needs at least {} members; \
                 the spec names {have}. The contract reverts RosterTooSmall, and adding \
                 a member nobody wrote is not a fix",
                min_approvers + 1
            ),
        }
    }
}

/// The most identities an envelope-reading template accepts
/// (`ApproverSetTooLarge`, `RosterTooLarge`).
pub const MAX_IDENTITIES: usize = 64;

/// The identity a principal is known by inside a governance protocol, or
/// `None` when the principal is not an address.
///
/// `QuorumIdentity.subjectKey(who)` = `keccak256("0x" ‖ 40 lowercase hex)`.
/// This is the only identity `MultiSigEnvelope` will record for a transaction
/// sent by `who` — as `draft`'s `initiator` and as `sign`'s `signer` — so it is
/// the only one a template can ever count. Lowercasing means a checksummed
/// address and its lowercase form are one person, not two approvers.
pub fn envelope_identity(principal: &str) -> Option<[u8; 32]> {
    let p = principal.trim();
    let digits = p.strip_prefix("0x").or_else(|| p.strip_prefix("0X"))?;
    if digits.len() != 40 || !digits.chars().all(|c| c.is_ascii_hexdigit()) {
        return None;
    }
    Some(keccak256(format!("0x{}", digits.to_ascii_lowercase()).as_bytes()))
}

/// Every principal as an on-chain identity, refusing any that is not an
/// address, any duplicate, and a set larger than the templates accept.
fn envelope_identities(principals: &[String]) -> Result<Vec<[u8; 32]>, CtorError> {
    if principals.len() > MAX_IDENTITIES {
        return Err(CtorError::TooManyApprovers {
            have: principals.len(),
        });
    }
    let mut out: Vec<[u8; 32]> = Vec::with_capacity(principals.len());
    for p in principals {
        let id = envelope_identity(p).ok_or_else(|| CtorError::UnbindablePrincipal {
            principal: p.clone(),
        })?;
        if out.contains(&id) {
            return Err(CtorError::DuplicateApprover {
                principal: p.clone(),
            });
        }
        out.push(id);
    }
    Ok(out)
}

/// The distinct principals a structural clause names, in the order written.
///
/// Deliberately narrow, in the same spirit as `compile::parse_count`: it splits
/// on commas, semicolons, newlines and the word "and", trims, and drops what is
/// left of anything shorter than two characters. It does NOT try to recognise a
/// name — an operator who writes prose gets a short list and can see that it is
/// short, which is better than a parser that confidently finds five approvers in
/// a sentence about approving.
pub fn principals_of(text: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut seen: Vec<String> = Vec::new();
    for piece in text
        .split(['\n', ',', ';', '/'])
        .flat_map(|s| s.split(" and "))
        .flat_map(|s| s.split(" & "))
    {
        let name = piece.trim().trim_matches(|c: char| c == '.' || c == '"');
        let name = name.trim();
        if name.chars().filter(|c| c.is_alphanumeric()).count() < 2 {
            continue;
        }
        let key = name.to_ascii_lowercase();
        if seen.contains(&key) {
            continue;
        }
        seen.push(key);
        out.push(name.to_string());
    }
    out
}

// ── A small ABI encoder ─────────────────────────────────────────────────

/// One constructor argument, in the shape the ABI cares about.
#[derive(Debug, Clone)]
pub enum Arg {
    /// A `bytes32`, verbatim.
    Word([u8; 32]),
    /// Any unsigned integer that fits in 64 bits, right-aligned.
    Uint(u64),
    /// An `address`, right-aligned in a word.
    Address([u8; 20]),
    /// A `string` — dynamic.
    Str(String),
    /// A `bytes32[]` — dynamic.
    Words(Vec<[u8; 32]>),
}

impl Arg {
    fn is_dynamic(&self) -> bool {
        matches!(self, Arg::Str(_) | Arg::Words(_))
    }

    /// The tail bytes for a dynamic argument: length, then the payload padded
    /// to a word boundary.
    fn tail(&self) -> Vec<u8> {
        match self {
            Arg::Str(s) => {
                let b = s.as_bytes();
                let mut out = word_of_u64(b.len() as u64).to_vec();
                out.extend_from_slice(b);
                while out.len() % 32 != 0 {
                    out.push(0);
                }
                out
            }
            Arg::Words(ws) => {
                let mut out = word_of_u64(ws.len() as u64).to_vec();
                for w in ws {
                    out.extend_from_slice(w);
                }
                out
            }
            _ => Vec::new(),
        }
    }

    fn head_word(&self) -> [u8; 32] {
        match self {
            Arg::Word(w) => *w,
            Arg::Uint(n) => word_of_u64(*n),
            Arg::Address(a) => {
                let mut w = [0u8; 32];
                w[12..].copy_from_slice(a);
                w
            }
            // Replaced with the real offset by `abi_encode`.
            Arg::Str(_) | Arg::Words(_) => [0u8; 32],
        }
    }
}

fn word_of_u64(n: u64) -> [u8; 32] {
    let mut w = [0u8; 32];
    w[24..].copy_from_slice(&n.to_be_bytes());
    w
}

/// Standard ABI head/tail encoding of a constructor's arguments.
///
/// Pinned against `cast abi-encode` in the tests below. A hand-rolled encoder
/// that is one word out still produces a well-formed blob — it just builds a
/// different contract, at a different (and correctly predicted!) address.
pub fn abi_encode(args: &[Arg]) -> Vec<u8> {
    let head_len = args.len() * 32;
    let mut head = Vec::with_capacity(head_len);
    let mut tail = Vec::new();

    for a in args {
        if a.is_dynamic() {
            head.extend_from_slice(&word_of_u64((head_len + tail.len()) as u64));
            tail.extend_from_slice(&a.tail());
        } else {
            head.extend_from_slice(&a.head_word());
        }
    }
    head.extend_from_slice(&tail);
    head
}

// ── Per-template constructors ───────────────────────────────────────────

/// The five arguments every audited template's constructor opens with.
fn prefix(ctx: &DeployContext) -> Vec<Arg> {
    vec![
        Arg::Word(ctx.tenant_id),
        Arg::Word(ctx.template_id),
        Arg::Uint(u64::from(ctx.version)),
        Arg::Word(ctx.spec_hash),
        Arg::Str(ctx.spec_cid.to_string()),
    ]
}

/// The templates this app can construct, and what each still needs.
///
/// Kept as one table so "which templates can actually be deployed from here"
/// is a question with a written answer rather than something a reader has to
/// infer from a match arm.
pub const SUPPORTED: &[&str] = &["ThresholdApproval", "SegregationOfDuties"];

/// ABI-encode the constructor arguments for a template.
pub fn encode(template: &str, ctx: &DeployContext) -> Result<Vec<u8>, CtorError> {
    match template {
        // constructor(bytes32,bytes32,uint32,bytes32,string,address,bytes32[],uint8)
        "ThresholdApproval" => {
            let threshold = uint_param(template, ctx, "threshold")?;
            if ctx.principals.is_empty() {
                return Err(CtorError::NoApprovers);
            }
            let approvers = envelope_identities(ctx.principals)?;
            if (approvers.len() as u64) < threshold {
                return Err(CtorError::NotEnoughApprovers {
                    have: approvers.len(),
                    need: threshold as u32,
                });
            }
            let envelopes = contract(template, ctx.bfr, "MultiSigEnvelope")?;
            let mut args = prefix(ctx);
            args.push(Arg::Address(envelopes));
            args.push(Arg::Words(approvers));
            args.push(Arg::Uint(threshold));
            Ok(abi_encode(&args))
        }

        // constructor(bytes32,bytes32,uint32,bytes32,string,address,uint8,bytes32[])
        //
        // The roster is the set whose members may propose and approve. The
        // chain counts a proposer or an approval only from a roster member, so
        // it is the spec's principals and nothing else.
        "SegregationOfDuties" => {
            let min_approvers = uint_param(template, ctx, "minApprovers")?;
            if min_approvers == 0 {
                return Err(CtorError::MinApproversTooLow);
            }
            if ctx.principals.is_empty() {
                return Err(CtorError::NoApprovers);
            }
            let roster = envelope_identities(ctx.principals)?;
            // One proposer plus `minApprovers` approvers, all distinct; the
            // executor may sit outside the roster.
            if (roster.len() as u64) < min_approvers.saturating_add(1) {
                return Err(CtorError::RosterTooSmall {
                    have: roster.len(),
                    min_approvers,
                });
            }
            let envelopes = contract(template, ctx.bfr, "MultiSigEnvelope")?;
            let mut args = prefix(ctx);
            args.push(Arg::Address(envelopes));
            args.push(Arg::Uint(min_approvers));
            args.push(Arg::Words(roster));
            Ok(abi_encode(&args))
        }

        // The rest are refused BY NAME, each naming the argument the pipeline
        // does not produce. These are not TODOs hidden behind a default.
        "ClassificationGate" => Err(CtorError::Unsupported {
            template: template.to_string(),
            missing: "a foreignNationalFloor — the classification above which a foreign \
                      national is refused. The interview asks for a ceiling and never \
                      asks for this",
        }),
        "TimeBoundedElevation" => Err(CtorError::Unsupported {
            template: template.to_string(),
            missing: "a requiredRole (bytes32) and an escalations contract address; the \
                      compiler extracts only a duration",
        }),
        "IncidentEscalation" => Err(CtorError::Unsupported {
            template: template.to_string(),
            missing: "a responder set, a contradiction-ledger address and a maxScan bound; \
                      the compiler extracts only an SLA duration",
        }),
        "BudgetedAutonomy" => Err(CtorError::Unsupported {
            template: template.to_string(),
            missing: "a per-action ceiling and two identity sets (alwaysSigned, approvers); \
                      no interview topic maps to this template at all",
        }),
        other => Err(CtorError::Unsupported {
            template: other.to_string(),
            missing: "a constructor this app has never been taught to encode",
        }),
    }
}

fn uint_param(template: &str, ctx: &DeployContext, name: &'static str) -> Result<u64, CtorError> {
    let raw = ctx
        .params
        .get(name)
        .ok_or_else(|| CtorError::MissingParam {
            template: template.to_string(),
            param: name,
        })?;
    raw.parse::<u64>().map_err(|_| CtorError::BadParam {
        template: template.to_string(),
        param: name,
        value: raw.clone(),
    })
}

fn contract(
    template: &str,
    book: &AddressBook,
    name: &'static str,
) -> Result<[u8; 20], CtorError> {
    let addr = book.get(name).ok_or_else(|| CtorError::MissingContract {
        template: template.to_string(),
        contract: name,
        book: book.describe(),
    })?;
    let bytes = hex::decode(addr.strip_prefix("0x").unwrap_or(addr)).map_err(|_| {
        CtorError::MissingContract {
            template: template.to_string(),
            contract: name,
            book: book.describe(),
        }
    })?;
    let mut out = [0u8; 20];
    if bytes.len() != 20 {
        return Err(CtorError::MissingContract {
            template: template.to_string(),
            contract: name,
            book: book.describe(),
        });
    }
    out.copy_from_slice(&bytes);
    Ok(out)
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    fn w(b: u8) -> [u8; 32] {
        [b; 32]
    }

    fn bfr() -> AddressBook {
        AddressBook::load_bfr().expect("vendored BFR book")
    }

    fn ctx<'a>(
        params: &'a BTreeMap<String, String>,
        principals: &'a [String],
        bfr: &'a AddressBook,
    ) -> DeployContext<'a> {
        DeployContext {
            tenant_id: w(0x11),
            template_id: w(0x22),
            version: 1,
            spec_hash: w(0x33),
            spec_cid: "bafySpec",
            params,
            principals,
            bfr,
        }
    }

    /// Three distinct wallet addresses, written the way an operator pastes them.
    const A1: &str = "0x1111111111111111111111111111111111111111";
    const A2: &str = "0x2222222222222222222222222222222222222222";
    const A3: &str = "0x3333333333333333333333333333333333333333";

    fn ps(xs: &[&str]) -> Vec<String> {
        xs.iter().map(|x| x.to_string()).collect()
    }

    fn id(a: &str) -> [u8; 32] {
        envelope_identity(a).expect("an address")
    }

    // ── The encoder, pinned against cast ────────────────────────────

    /// Pinned against `cast abi-encode`. This is the encoder whose output is
    /// hashed into the predicted address, so a drift here does not throw — it
    /// shows a human the correct address of the wrong contract.
    #[test]
    fn the_abi_encoder_matches_cast() {
        let got = abi_encode(&[
            Arg::Word(w(0x11)),
            Arg::Word(w(0x22)),
            Arg::Uint(1),
            Arg::Word(w(0x33)),
            Arg::Str("bafySpec".into()),
            Arg::Address([0xab; 20]),
            Arg::Words(vec![w(0xaa), w(0xbb)]),
            Arg::Uint(2),
        ]);
        // cast abi-encode \
        //   "f(bytes32,bytes32,uint32,bytes32,string,address,bytes32[],uint8)" \
        //   0x1111… 0x2222… 1 0x3333… bafySpec 0xabab…ab "[0xaaaa…,0xbbbb…]" 2
        let want = concat!(
            "1111111111111111111111111111111111111111111111111111111111111111",
            "2222222222222222222222222222222222222222222222222222222222222222",
            "0000000000000000000000000000000000000000000000000000000000000001",
            "3333333333333333333333333333333333333333333333333333333333333333",
            "0000000000000000000000000000000000000000000000000000000000000100",
            "000000000000000000000000abababababababababababababababababababab",
            "0000000000000000000000000000000000000000000000000000000000000140",
            "0000000000000000000000000000000000000000000000000000000000000002",
            // tail: the string, then the array
            "0000000000000000000000000000000000000000000000000000000000000008",
            "6261667953706563000000000000000000000000000000000000000000000000",
            "0000000000000000000000000000000000000000000000000000000000000002",
            "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
            "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
        );
        assert_eq!(hex::encode(got), want, "constructor encoder drifted from cast");
    }

    /// A dynamic argument's offset is measured from the START of the blob, and
    /// counts the whole head — including the words that come after it. Getting
    /// this wrong is the classic ABI bug and it decodes into other fields
    /// rather than failing.
    #[test]
    fn dynamic_offsets_count_the_whole_head() {
        let got = abi_encode(&[Arg::Str("a".into()), Arg::Uint(7), Arg::Str("b".into())]);
        // head is 3 words = 0x60; the first string's tail is 2 words = 0x40.
        assert_eq!(hex::encode(&got[..32]), format!("{}60", "0".repeat(62)));
        assert_eq!(hex::encode(&got[64..96]), format!("{}a0", "0".repeat(62)));
    }

    // ── ThresholdApproval ───────────────────────────────────────────

    #[test]
    fn threshold_approval_encodes_with_the_live_envelope_address() {
        let f = bfr();
        let params = BTreeMap::from([("threshold".to_string(), "2".to_string())]);
        let principals = ps(&[A1, A2]);
        let out = encode("ThresholdApproval", &ctx(&params, &principals, &f)).expect("encodes");

        // 8 head words + string tail (2) + array tail (1 + 2).
        assert_eq!(out.len(), 32 * (8 + 2 + 3));
        // The envelopes argument is the address the BFR book actually carries,
        // not a constant compiled in here (rule 8).
        let want = f.get("MultiSigEnvelope").expect("book carries it").to_ascii_lowercase();
        assert!(
            hex::encode(&out[32 * 5..32 * 6]).ends_with(want.trim_start_matches("0x")),
            "the envelopes argument is not the booked MultiSigEnvelope"
        );
        // The approvers are the identities the envelope records for those
        // addresses, in the order written.
        assert_eq!(&out[32 * 11..32 * 12], &id(A1));
        assert_eq!(&out[32 * 12..32 * 13], &id(A2));
    }

    /// **The refusal that matters most here.** A 3-of-N policy naming two people
    /// is not a policy that can be deployed with a third approver invented to
    /// make the numbers work — the contract reverts `BadThreshold`, and padding
    /// the set would deploy a control that answers to a name nobody wrote.
    #[test]
    fn a_threshold_above_the_approver_count_is_refused() {
        let f = bfr();
        let params = BTreeMap::from([("threshold".to_string(), "3".to_string())]);
        let principals = ps(&[A1, A2]);
        let e = encode("ThresholdApproval", &ctx(&params, &principals, &f)).expect_err("refuse");
        assert_eq!(e, CtorError::NotEnoughApprovers { have: 2, need: 3 });
        assert!(e.why().contains("padding the set"), "{}", e.why());
    }

    #[test]
    fn an_empty_approver_set_is_refused_before_the_chain_sees_it() {
        let f = bfr();
        let params = BTreeMap::from([("threshold".to_string(), "1".to_string())]);
        let e = encode("ThresholdApproval", &ctx(&params, &[], &f)).expect_err("refuse");
        assert_eq!(e, CtorError::NoApprovers);
    }

    // ── SegregationOfDuties ─────────────────────────────────────────

    /// Pinned against `cast abi-encode` for the chain's 8-argument
    /// constructor, `(…, address envelopes, uint8 minApprovers, bytes32[] roster)`.
    /// The roster is the LAST argument; encoding it before `minApprovers`
    /// would still be a well-formed blob, for a different contract.
    #[test]
    fn segregation_of_duties_encodes_the_roster_as_the_eighth_argument() {
        let got = abi_encode(&[
            Arg::Word(w(0x11)),
            Arg::Word(w(0x22)),
            Arg::Uint(1),
            Arg::Word(w(0x33)),
            Arg::Str("bafySpec".into()),
            Arg::Address([0xab; 20]),
            Arg::Uint(2),
            Arg::Words(vec![w(0xaa), w(0xbb), w(0xcc)]),
        ]);
        // cast abi-encode \
        //   "f(bytes32,bytes32,uint32,bytes32,string,address,uint8,bytes32[])" \
        //   0x1111… 0x2222… 1 0x3333… bafySpec 0xabab…ab 2 "[0xaaaa…,0xbbbb…,0xcccc…]"
        let want = concat!(
            "1111111111111111111111111111111111111111111111111111111111111111",
            "2222222222222222222222222222222222222222222222222222222222222222",
            "0000000000000000000000000000000000000000000000000000000000000001",
            "3333333333333333333333333333333333333333333333333333333333333333",
            "0000000000000000000000000000000000000000000000000000000000000100",
            "000000000000000000000000abababababababababababababababababababab",
            "0000000000000000000000000000000000000000000000000000000000000002",
            "0000000000000000000000000000000000000000000000000000000000000140",
            "0000000000000000000000000000000000000000000000000000000000000008",
            "6261667953706563000000000000000000000000000000000000000000000000",
            "0000000000000000000000000000000000000000000000000000000000000003",
            "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
            "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
            "cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc",
        );
        assert_eq!(hex::encode(got), want, "SoD constructor layout drifted from cast");
    }

    #[test]
    fn segregation_of_duties_passes_the_principals_as_its_roster() {
        let f = bfr();
        let params = BTreeMap::from([("minApprovers".to_string(), "2".to_string())]);
        let principals = ps(&[A1, A2, A3]);
        let out =
            encode("SegregationOfDuties", &ctx(&params, &principals, &f)).expect("encodes");

        // 8 head words + string tail (2) + roster tail (1 + 3).
        assert_eq!(out.len(), 32 * (8 + 2 + 4));
        // Word 6 is minApprovers, word 7 the roster's offset.
        assert_eq!(out[32 * 7 - 1], 2, "minApprovers");
        assert_eq!(&out[32 * 8 - 2..32 * 8], &[0x01, 0x40], "roster offset");
        assert_eq!(out[32 * 11 - 1], 3, "roster length");
        assert_eq!(&out[32 * 11..32 * 12], &id(A1));
        assert_eq!(&out[32 * 12..32 * 13], &id(A2));
        assert_eq!(&out[32 * 13..32 * 14], &id(A3));
    }

    /// `RosterTooSmall`: a proposer plus `minApprovers` distinct approvers must
    /// fit in the roster, or the protocol can never Allow.
    #[test]
    fn a_roster_that_cannot_seat_proposer_and_approvers_is_refused() {
        let f = bfr();
        let params = BTreeMap::from([("minApprovers".to_string(), "2".to_string())]);
        let principals = ps(&[A1, A2]);
        let e = encode("SegregationOfDuties", &ctx(&params, &principals, &f)).expect_err("refuse");
        assert_eq!(
            e,
            CtorError::RosterTooSmall {
                have: 2,
                min_approvers: 2
            }
        );
        assert!(e.why().contains("at least 3 members"), "{}", e.why());
        // Exactly minApprovers + 1 is enough.
        let principals = ps(&[A1, A2, A3]);
        encode("SegregationOfDuties", &ctx(&params, &principals, &f)).expect("fits");
    }

    #[test]
    fn segregation_of_duties_refuses_zero_approvers_and_an_empty_roster() {
        let f = bfr();
        let zero = BTreeMap::from([("minApprovers".to_string(), "0".to_string())]);
        let principals = ps(&[A1, A2]);
        assert_eq!(
            encode("SegregationOfDuties", &ctx(&zero, &principals, &f)).expect_err("refuse"),
            CtorError::MinApproversTooLow
        );
        let one = BTreeMap::from([("minApprovers".to_string(), "1".to_string())]);
        assert_eq!(
            encode("SegregationOfDuties", &ctx(&one, &[], &f)).expect_err("refuse"),
            CtorError::NoApprovers
        );
    }

    // ── Identities bound to the drafting address ────────────────────

    /// A principal known only by name has no identity `MultiSigEnvelope` will
    /// ever record, because `draft` and `sign` bind it to the sending address.
    /// Both envelope-reading templates refuse it by name.
    #[test]
    fn a_principal_that_is_not_an_address_is_refused_by_name() {
        let f = bfr();
        let params = BTreeMap::from([
            ("threshold".to_string(), "1".to_string()),
            ("minApprovers".to_string(), "1".to_string()),
        ]);
        let principals = ps(&[A1, "R. Ortiz"]);
        for t in SUPPORTED {
            let e = encode(t, &ctx(&params, &principals, &f)).expect_err("refuse");
            assert_eq!(
                e,
                CtorError::UnbindablePrincipal {
                    principal: "R. Ortiz".into()
                },
                "{t}"
            );
            assert!(e.why().contains("wallet address"), "{}", e.why());
        }
    }

    /// One wallet written twice (checksummed and lowercase) is one approver.
    #[test]
    fn the_same_address_twice_is_a_duplicate() {
        let f = bfr();
        let params = BTreeMap::from([("threshold".to_string(), "1".to_string())]);
        let principals = ps(&[
            "0xabcdefabcdefabcdefabcdefabcdefabcdefabcd",
            "0xABCDEFabcdefABCDEFabcdefABCDEFabcdefABCD",
        ]);
        let e = encode("ThresholdApproval", &ctx(&params, &principals, &f)).expect_err("refuse");
        assert!(matches!(e, CtorError::DuplicateApprover { .. }), "{e:?}");
    }

    #[test]
    fn more_than_sixty_four_principals_is_refused() {
        let f = bfr();
        let params = BTreeMap::from([("threshold".to_string(), "1".to_string())]);
        let principals: Vec<String> = (0..=MAX_IDENTITIES).map(|i| format!("0x{i:040x}")).collect();
        let e = encode("ThresholdApproval", &ctx(&params, &principals, &f)).expect_err("refuse");
        assert_eq!(e, CtorError::TooManyApprovers { have: 65 });
        let principals: Vec<String> = (1..=MAX_IDENTITIES).map(|i| format!("0x{i:040x}")).collect();
        encode("ThresholdApproval", &ctx(&params, &principals, &f)).expect("64 is the bound");
    }

    /// Pinned against the chain: `QuorumIdentity.subjectKey(0x…ab)`, the same
    /// literal `ClassificationGate.t.sol::test_subjectKeyMatchesQuorumsDerivation`
    /// and `chain::clearance_subject` assert. If this moves, no envelope this app
    /// deploys a protocol for can ever be satisfied.
    #[test]
    fn the_envelope_identity_is_quorum_subject_key() {
        let want = "549328a5435660214b49079937321d53a0b2204070da2b803319218631a426db";
        let a = "0x00000000000000000000000000000000000000ab";
        assert_eq!(hex::encode(id(a)), want);
        assert_eq!(hex::encode(id("  0x00000000000000000000000000000000000000AB ")), want);
        assert_eq!(id(a), crate::chain::clearance_subject(a));
    }

    #[test]
    fn only_a_forty_digit_hex_address_is_an_identity() {
        assert!(envelope_identity("R. Ortiz").is_none());
        assert!(envelope_identity("0x").is_none());
        assert!(envelope_identity("0x00000000000000000000000000000000000000a").is_none());
        assert!(envelope_identity("0x00000000000000000000000000000000000000abc").is_none());
        assert!(envelope_identity("0x00000000000000000000000000000000000000ag").is_none());
        assert!(envelope_identity("00000000000000000000000000000000000000ab00").is_none());
    }

    // ── The refusals ────────────────────────────────────────────────

    /// Every template the pipeline cannot fully supply is refused BY NAME, and
    /// the message names the missing argument. A default here would deploy a
    /// real, audited, correctly-predicted contract enforcing a rule nobody
    /// wrote.
    #[test]
    fn unsupported_templates_name_the_missing_argument() {
        let f = bfr();
        let params = BTreeMap::new();
        for t in [
            "ClassificationGate",
            "TimeBoundedElevation",
            "IncidentEscalation",
            "BudgetedAutonomy",
        ] {
            let e = encode(t, &ctx(&params, &[], &f)).expect_err("must refuse");
            match &e {
                CtorError::Unsupported { template, missing } => {
                    assert_eq!(template, t);
                    assert!(!missing.is_empty(), "{t}: refused without saying what is missing");
                }
                other => panic!("{t}: expected Unsupported, got {other:?}"),
            }
        }
    }

    /// The supported list and the encoder must not disagree: a name on the list
    /// that does not encode would read as deployable everywhere it is shown.
    #[test]
    fn every_supported_template_actually_encodes() {
        let f = bfr();
        let params = BTreeMap::from([
            ("threshold".to_string(), "1".to_string()),
            ("minApprovers".to_string(), "2".to_string()),
        ]);
        let principals = ps(&[A1, A2, A3]);
        for t in SUPPORTED {
            encode(t, &ctx(&params, &principals, &f))
                .unwrap_or_else(|e| panic!("{t} is listed as supported but refused: {}", e.why()));
        }
    }

    #[test]
    fn principals_are_split_and_deduplicated() {
        let got = principals_of("R. Ortiz, J. Mbeki and A. Lindqvist; r. ortiz");
        assert_eq!(got, vec!["R. Ortiz", "J. Mbeki", "A. Lindqvist"]);
        let got = principals_of(&format!("{A1}, {A2} and {A1}"));
        assert_eq!(got, vec![A1, A2]);
    }

    /// Prose does not become a long approver list. A parser that found five
    /// names in a sentence about approving would build a control answering to
    /// people who were never named.
    #[test]
    fn prose_yields_what_it_actually_names() {
        assert!(principals_of("").is_empty());
        assert!(principals_of("  ,  ; . ").is_empty());
        assert_eq!(principals_of("the CFO").len(), 1);
    }
}
