//! QRM-S7.1 — INGEST: parse, classify, and refuse what cannot be classified.
//!
//! Stage 1 of the authoring pipeline. A document dump becomes text, chunks, and
//! a per-file classification with provenance.
//!
//! # Why refusal is the interesting part
//!
//! The detected classification **routes both the model and the storage**. Get it
//! wrong downward and controlled material reaches a model that is not cleared
//! for it — which is the one mistake in this pipeline that cannot be walked
//! back, because you cannot un-send a prompt.
//!
//! So an unmarked file is **refused**, never defaulted:
//!
//! - Defaulting to `Public` is fail-OPEN. It routes the document to anything.
//! - Defaulting to `ITAR` is fail-closed but useless: every unmarked file, which
//!   is most of them, becomes unusable and operators learn to mark everything
//!   ITAR to make the tool work — which destroys the ladder's meaning.
//!
//! The honest answer is that this program does not know, and a human must say.
//! [`Refusal::Unmarked`] carries that back with the filename so the operator can
//! mark it and retry.
//!
//! # Highest marking wins
//!
//! A document carrying more than one marking takes the **highest**. That is the
//! monotonicity rule (MR-4) applied at ingest, and it also happens to make
//! substring overlap safe: "CONTROLLED UNCLASSIFIED INFORMATION" contains
//! "UNCLASSIFIED", and taking the maximum means the CUI reading wins rather than
//! the Public one.
//!
//! # Markings are matched on word boundaries, and that is not pedantry
//!
//! `"CUI"` occurs inside `circuit`, `biscuit`, and `acuity`. A substring match
//! would silently classify an engineering document as CUI because it mentions a
//! circuit — or worse, mark something CUI that a reviewer then trusts as
//! deliberately marked. Every marking here must be delimited by a non-alphanumeric
//! character on both sides.

use quorum_tenancy::Classification;

/// The largest file this will read into memory to classify. A document that
/// cannot be classified without exhausting memory is refused, not streamed:
/// partial text produces a partial marking scan, and a marking scan that saw
/// only half the document is worse than no answer because it looks like one.
pub const MAX_BYTES: usize = 8 * 1024 * 1024;

/// Target chunk size in characters. Chunks split on paragraph boundaries, so
/// this is a target rather than a limit.
const CHUNK_TARGET: usize = 2_000;

/// Why a file was not ingested. Every variant names the file and what the
/// operator can do about it — a refusal an operator cannot act on is an outage.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Refusal {
    /// No classification marking was found. The operator must mark it.
    Unmarked,
    /// Text could not be extracted, so no marking scan was even possible.
    Unreadable { why: String },
    /// The file has no textual content to classify or chunk.
    Empty,
    /// Larger than [`MAX_BYTES`].
    TooLarge { bytes: usize },
}

impl Refusal {
    /// The sentence shown to the operator.
    pub fn why(&self) -> String {
        match self {
            Refusal::Unmarked => "no classification marking found — mark the document \
                 (Public / Proprietary / CUI / ITAR) and re-ingest. It is not \
                 assumed, because the marking decides which model may read it."
                .to_string(),
            Refusal::Unreadable { why } => format!("could not extract text: {why}"),
            Refusal::Empty => "no text content to classify".to_string(),
            Refusal::TooLarge { bytes } => format!(
                "{bytes} bytes exceeds the {MAX_BYTES}-byte ingest limit; a partial \
                 marking scan is not a marking"
            ),
        }
    }
}

/// One chunk of a parsed document, with enough provenance for a spec clause to
/// cite it and a reader to find it again.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Chunk {
    /// 0-based index within the file.
    pub ordinal: usize,
    /// Character offset of this chunk within the extracted text.
    pub offset: usize,
    pub text: String,
}

/// A successfully ingested file.
#[derive(Debug, Clone)]
pub struct Ingested {
    pub name: String,
    pub bytes: usize,
    pub classification: Classification,
    /// The literal marking text that produced the classification, verbatim from
    /// the document. A reader checking our work needs to see what we matched on,
    /// not just what we concluded.
    pub marking: String,
    pub chunks: Vec<Chunk>,
    /// BLAKE3 of the raw file bytes — the provenance anchor.
    pub digest: String,
}

/// The result of ingesting a batch: what was taken, and what was refused.
#[derive(Debug, Clone, Default)]
pub struct Batch {
    pub accepted: Vec<Ingested>,
    pub refused: Vec<(String, Refusal)>,
}

/// A marking the scanner recognises, and the level it implies.
///
/// **Ordered longest-phrase-first, NOT by level, deliberately.** Sorting by level
/// would make the array order silently guarantee "highest wins", so the
/// comparison in [`detect_marking`] would be dead code that looks alive — and a
/// negative control proved exactly that: with a level-sorted array, replacing
/// "highest wins" with "first wins" passed every test. Interleaving the levels
/// makes the comparison load-bearing and the test discriminating.
///
/// Longest-first also means the more specific phrase is what gets reported
/// (`CONTROLLED UNCLASSIFIED INFORMATION` rather than the bare `CUI` inside it).
const MARKINGS: &[(&str, Classification)] = &[
    ("CONTROLLED UNCLASSIFIED INFORMATION", Classification::Cui),
    ("APPROVED FOR PUBLIC RELEASE", Classification::Public),
    ("COMPANY CONFIDENTIAL", Classification::Proprietary),
    ("EXPORT-CONTROLLED", Classification::Itar),
    ("EXPORT CONTROLLED", Classification::Itar),
    ("PUBLIC RELEASE", Classification::Public),
    ("UNCLASSIFIED", Classification::Public),
    ("CONFIDENTIAL", Classification::Proprietary),
    ("PROPRIETARY", Classification::Proprietary),
    ("ITAR", Classification::Itar),
    ("CUI", Classification::Cui),
];

/// Extensions whose bytes are treated as UTF-8 text.
///
/// Deliberately a allowlist. An unknown extension is refused rather than
/// hopefully decoded: a binary that happens to contain the letters "CUI" would
/// otherwise be "classified".
const TEXT_EXTENSIONS: &[&str] = &[
    "txt", "md", "markdown", "csv", "tsv", "json", "yaml", "yml", "rst", "adoc", "log", "text",
];

/// Detect the classification marking in `text`.
///
/// Returns the **highest** marking found with the literal text that matched, or
/// `None` when the document carries no marking at all.
pub fn detect_marking(text: &str) -> Option<(Classification, String)> {
    let upper = text.to_ascii_uppercase();
    let mut best: Option<(Classification, String)> = None;

    for (needle, level) in MARKINGS {
        if !contains_word(&upper, needle) {
            continue;
        }
        let better = match &best {
            None => true,
            // Strictly greater: the FIRST marking at a given level wins, and
            // MARKINGS is ordered longest-first, so the more specific phrase is
            // what gets reported.
            Some((current, _)) => level > current,
        };
        if better {
            best = Some((*level, (*needle).to_string()));
        }
    }
    best
}

/// Does `haystack` contain `needle` delimited by non-alphanumeric characters?
///
/// This is what stops `circuit` from being read as a CUI marking. Both operands
/// are expected uppercase.
fn contains_word(haystack: &str, needle: &str) -> bool {
    let hb = haystack.as_bytes();
    let nb = needle.as_bytes();
    if nb.is_empty() || nb.len() > hb.len() {
        return false;
    }
    let boundary = |b: u8| !(b.is_ascii_alphanumeric() || b == b'_');
    let mut i = 0;
    while i + nb.len() <= hb.len() {
        if &hb[i..i + nb.len()] == nb {
            let before_ok = i == 0 || boundary(hb[i - 1]);
            let after_idx = i + nb.len();
            let after_ok = after_idx == hb.len() || boundary(hb[after_idx]);
            if before_ok && after_ok {
                return true;
            }
        }
        i += 1;
    }
    false
}

/// Split extracted text into chunks on paragraph boundaries.
pub fn chunk(text: &str) -> Vec<Chunk> {
    let mut out = Vec::new();
    let mut buf = String::new();
    let mut buf_offset = 0usize;
    let mut offset = 0usize;

    for para in text.split_inclusive("\n\n") {
        if buf.is_empty() {
            buf_offset = offset;
        }
        buf.push_str(para);
        offset += para.chars().count();
        if buf.chars().count() >= CHUNK_TARGET {
            out.push(Chunk {
                ordinal: out.len(),
                offset: buf_offset,
                text: buf.trim().to_string(),
            });
            buf.clear();
        }
    }
    if !buf.trim().is_empty() {
        out.push(Chunk {
            ordinal: out.len(),
            offset: buf_offset,
            text: buf.trim().to_string(),
        });
    }
    out
}

/// Extract text from raw bytes, given the file's extension.
///
/// PDF is **not** supported and says so. The planset's motivating example is a
/// board-resolution PDF, so this is a named gap rather than a silent one: a PDF
/// is refused with a reason that tells the operator to export it as text. A
/// half-working extractor would be worse than none, because a marking scan over
/// garbled text can miss a banner that is really there.
pub fn extract_text(name: &str, bytes: &[u8]) -> Result<String, Refusal> {
    if bytes.len() > MAX_BYTES {
        return Err(Refusal::TooLarge { bytes: bytes.len() });
    }
    let ext = name
        .rsplit_once('.')
        .map(|(_, e)| e.to_ascii_lowercase())
        .unwrap_or_default();

    if ext == "pdf" {
        return Err(Refusal::Unreadable {
            why: "PDF text extraction is not implemented (QRM-S7.1); export the \
                  document as .txt or .md and re-ingest"
                .to_string(),
        });
    }
    if !TEXT_EXTENSIONS.contains(&ext.as_str()) {
        return Err(Refusal::Unreadable {
            why: format!(
                "unsupported file type '{}'; ingest reads {} — an unknown format is \
                 refused rather than decoded hopefully, because a binary containing \
                 the letters of a marking is not a marked document",
                if ext.is_empty() { "(none)" } else { &ext },
                TEXT_EXTENSIONS.join(", ")
            ),
        });
    }
    match std::str::from_utf8(bytes) {
        Ok(s) => Ok(s.to_string()),
        Err(e) => Err(Refusal::Unreadable {
            why: format!("not valid UTF-8 at byte {}", e.valid_up_to()),
        }),
    }
}

/// Ingest one file's bytes.
pub fn ingest_bytes(name: &str, bytes: &[u8]) -> Result<Ingested, Refusal> {
    let text = extract_text(name, bytes)?;
    if text.trim().is_empty() {
        return Err(Refusal::Empty);
    }
    let (classification, marking) = detect_marking(&text).ok_or(Refusal::Unmarked)?;
    let chunks = chunk(&text);
    if chunks.is_empty() {
        return Err(Refusal::Empty);
    }
    Ok(Ingested {
        name: name.to_string(),
        bytes: bytes.len(),
        classification,
        marking,
        chunks,
        digest: blake3::hash(bytes).to_hex().to_string(),
    })
}

/// Ingest a batch of paths. A refusal for one file never fails the batch — the
/// operator gets everything that could be read plus a reason for each that could
/// not, in one pass, rather than fixing them one error at a time.
pub fn ingest_paths(paths: &[String]) -> Batch {
    let mut batch = Batch::default();
    for p in paths {
        let name = std::path::Path::new(p)
            .file_name()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| p.clone());
        match std::fs::read(p) {
            Err(e) => batch.refused.push((
                name,
                Refusal::Unreadable {
                    why: format!("cannot read the file: {e}"),
                },
            )),
            Ok(bytes) => match ingest_bytes(&name, &bytes) {
                Ok(f) => batch.accepted.push(f),
                Err(r) => batch.refused.push((name, r)),
            },
        }
    }
    batch
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    // ── The refusal that this stage exists for ──────────────────────

    /// An unmarked document is REFUSED, not defaulted. Defaulting down is
    /// fail-open (it routes controlled material to any model); defaulting up
    /// makes every unmarked file unusable and teaches operators to mark
    /// everything ITAR, which destroys the ladder.
    #[test]
    fn an_unmarked_document_is_refused_not_defaulted() {
        let err = ingest_bytes("policy.md", b"Spend over 100 requires two approvers.")
            .expect_err("must refuse");
        assert_eq!(err, Refusal::Unmarked);
        assert!(err.why().contains("mark the document"), "{}", err.why());
    }

    /// The word-boundary rule, which is the difference between a classifier and
    /// a substring search. Every one of these contains the letters of a marking
    /// and none of them is a marked document.
    #[test]
    fn marking_letters_inside_ordinary_words_are_not_markings() {
        for text in [
            "The circuit board layout is attached.",
            "Biscuit procurement, Q3.",
            "Visual acuity requirements for operators.",
            "ITARGET is our internal tool.",
            "This is a confidentiality agreement.", // CONFIDENTIAL is a prefix here
        ] {
            assert_eq!(
                detect_marking(text),
                None,
                "false marking detected in: {text}"
            );
        }
    }

    /// …and a real marking, delimited, is found.
    #[test]
    fn a_delimited_marking_is_found() {
        let (c, m) = detect_marking("CUI // Procurement authority").expect("marked");
        assert_eq!(c, Classification::Cui);
        assert_eq!(m, "CUI");

        let (c, _) = detect_marking("Distribution: ITAR-controlled.").expect("marked");
        assert_eq!(c, Classification::Itar);
    }

    /// Highest wins. This is MR-4 at ingest, and it is also what makes the
    /// substring overlap between "CONTROLLED UNCLASSIFIED INFORMATION" and
    /// "UNCLASSIFIED" safe: the CUI reading beats the Public one.
    #[test]
    fn the_highest_marking_wins() {
        let (c, m) = detect_marking(
            "CONTROLLED UNCLASSIFIED INFORMATION\n\nPreviously released as UNCLASSIFIED.",
        )
        .expect("marked");
        assert_eq!(c, Classification::Cui, "a CUI doc must not read as Public");
        assert_eq!(m, "CONTROLLED UNCLASSIFIED INFORMATION", "report the specific phrase");

        // This case discriminates because MARKINGS is ordered by phrase length,
        // not by level: "APPROVED FOR PUBLIC RELEASE" is scanned BEFORE "ITAR".
        // A "first match wins" implementation returns Public here. Only the
        // comparison returns ITAR.
        let (c, _) = detect_marking("PROPRIETARY\n\nITAR\n\nAPPROVED FOR PUBLIC RELEASE")
            .expect("marked");
        assert_eq!(
            c,
            Classification::Itar,
            "highest must win over array order — otherwise an ITAR document \
             carrying a public-release note reads as Public"
        );
    }

    // ── Reading files at all ────────────────────────────────────────

    /// A PDF is refused with a reason the operator can act on. The planset's
    /// motivating example is a board-resolution PDF, so this is a NAMED gap: a
    /// half-working extractor would be worse than none, because a marking scan
    /// over garbled text can miss a banner that is really there.
    #[test]
    fn a_pdf_is_refused_with_a_named_reason() {
        let err = ingest_bytes("resolution.pdf", b"%PDF-1.7 ...").expect_err("refuse");
        match &err {
            Refusal::Unreadable { why } => {
                assert!(why.contains("PDF"), "{why}");
                assert!(why.contains("export"), "tell them what to do: {why}");
            }
            other => panic!("expected Unreadable, got {other:?}"),
        }
    }

    /// An unknown extension is refused rather than hopefully decoded — a binary
    /// containing the letters of a marking is not a marked document.
    #[test]
    fn an_unknown_extension_is_refused_not_guessed() {
        let err = ingest_bytes("blob.bin", b"CUI").expect_err("refuse");
        assert!(matches!(err, Refusal::Unreadable { .. }));
    }

    #[test]
    fn non_utf8_is_refused() {
        let err = ingest_bytes("notes.txt", &[0xff, 0xfe, 0x00]).expect_err("refuse");
        match err {
            Refusal::Unreadable { why } => assert!(why.contains("UTF-8"), "{why}"),
            other => panic!("expected Unreadable, got {other:?}"),
        }
    }

    #[test]
    fn an_empty_document_is_refused() {
        assert_eq!(
            ingest_bytes("empty.md", b"   \n\n  ").expect_err("refuse"),
            Refusal::Empty
        );
    }

    #[test]
    fn an_oversized_file_is_refused_rather_than_partially_scanned() {
        let big = vec![b'a'; MAX_BYTES + 1];
        match ingest_bytes("big.txt", &big).expect_err("refuse") {
            Refusal::TooLarge { bytes } => assert_eq!(bytes, MAX_BYTES + 1),
            other => panic!("expected TooLarge, got {other:?}"),
        }
    }

    // ── What a successful ingest carries ────────────────────────────

    /// The marking is reported VERBATIM alongside the conclusion, so a reader
    /// checking our work sees what we matched on and not merely what we decided.
    #[test]
    fn an_accepted_file_carries_its_evidence() {
        let body = "CUI\n\nClause 1. Spend over 100 SALT requires two approvers.\n\n";
        let f = ingest_bytes("policy.md", body.as_bytes()).expect("accepted");
        assert_eq!(f.classification, Classification::Cui);
        assert_eq!(f.marking, "CUI");
        assert_eq!(f.bytes, body.len());
        assert_eq!(f.digest, blake3::hash(body.as_bytes()).to_hex().to_string());
        assert!(!f.chunks.is_empty());
        assert_eq!(f.chunks[0].ordinal, 0);
    }

    /// Chunks carry offsets so a clause can cite a location a human can find.
    #[test]
    fn chunks_are_ordered_and_offset() {
        let para = "x".repeat(1_500);
        let text = format!("CUI\n\n{para}\n\n{para}\n\n{para}\n\n");
        let f = ingest_bytes("long.md", text.as_bytes()).expect("accepted");
        assert!(f.chunks.len() > 1, "a long document should chunk");
        for (i, c) in f.chunks.iter().enumerate() {
            assert_eq!(c.ordinal, i);
        }
        for w in f.chunks.windows(2) {
            assert!(w[1].offset > w[0].offset, "offsets must advance");
        }
    }

    /// One bad file does not fail the batch: the operator gets everything
    /// readable plus a reason per refusal, in one pass.
    #[test]
    fn a_refusal_does_not_fail_the_whole_batch() {
        let dir = std::env::temp_dir().join(format!("qrm-s7-ingest-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("mkdir");
        let good = dir.join("good.md");
        let bad = dir.join("bad.md");
        std::fs::write(&good, "ITAR\n\nClause 1.").expect("write");
        std::fs::write(&bad, "Clause 1, unmarked.").expect("write");

        let batch = ingest_paths(&[
            good.to_string_lossy().to_string(),
            bad.to_string_lossy().to_string(),
            dir.join("missing.md").to_string_lossy().to_string(),
        ]);
        assert_eq!(batch.accepted.len(), 1);
        assert_eq!(batch.accepted[0].classification, Classification::Itar);
        assert_eq!(batch.refused.len(), 2);
        assert!(batch.refused.iter().any(|(_, r)| *r == Refusal::Unmarked));
        std::fs::remove_dir_all(&dir).ok();
    }
}

// ── Tauri seam ──────────────────────────────────────────────────────────

use serde::Serialize;

/// One accepted file, as the Governance surface renders it.
#[derive(Serialize, Debug, Clone)]
pub struct IngestFileDto {
    pub name: String,
    pub size: String,
    pub status: String,
    pub class: String,
    /// The literal marking matched, so a reviewer sees the evidence and not
    /// only the conclusion.
    pub note: String,
    /// `blake3:<digest> · <n> chunks` — enough for a clause to cite it.
    pub prov: String,
}

/// A file that was not ingested, and what the operator can do about it.
#[derive(Serialize, Debug, Clone)]
pub struct RefusedDto {
    pub name: String,
    pub why: String,
}

#[derive(Serialize, Debug, Clone)]
pub struct IngestResultDto {
    pub spec_id: String,
    pub files: Vec<IngestFileDto>,
    pub refused: Vec<RefusedDto>,
}

// ── What ingest is the only place to learn ──────────────────────────────

/// The durable facts about a spec that only ingest can establish.
///
/// A protocol is deployed with a `classificationCeiling`, and a policy drafted
/// from a CUI document governs at CUI. Ingest is where that is known — after it,
/// the spec is clauses and answers, and nothing in them remembers what the
/// source documents were marked.
///
/// Before this existed, `deploy_intent` passed a hardcoded `2` (CUI) for every
/// deployment. That is a silent privilege inflation for a Public policy and a
/// silent under-statement for an ITAR one, and it would have been invisible
/// forever: the ceiling is a number in a constructor, not a surface anyone reads.
#[derive(Serialize, serde::Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct SpecMeta {
    pub spec_id: String,
    /// The HIGHEST classification among the documents this spec was built from.
    /// Highest, not lowest: a policy drawn from one CUI memo and four public
    /// ones is a CUI policy.
    pub classification: String,
    /// Which document and which literal marking produced it — a reader checking
    /// our work needs to see what we matched on, not just what we concluded.
    pub evidence: String,
    /// `name · blake3:digest` per accepted source.
    pub sources: Vec<String>,
}

fn meta_path(root: &std::path::Path, spec_id: &str) -> std::path::PathBuf {
    root.join("specs").join(format!("{spec_id}.meta.json"))
}

pub fn load_meta(root: &std::path::Path, spec_id: &str) -> Option<SpecMeta> {
    let raw = std::fs::read_to_string(meta_path(root, spec_id)).ok()?;
    serde_json::from_str(&raw).ok()
}

pub fn save_meta(root: &std::path::Path, meta: &SpecMeta) -> Result<(), String> {
    let path = meta_path(root, &meta.spec_id);
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    let body = serde_json::to_vec_pretty(meta).map_err(|e| e.to_string())?;
    std::fs::write(&path, body).map_err(|e| e.to_string())
}

/// Build the spec's meta from an accepted batch, or `None` when nothing was
/// accepted — a spec with no sources has no classification, and inventing
/// `Public` for it is the one mistake this must never make.
pub fn meta_of(spec_id: &str, accepted: &[Ingested]) -> Option<SpecMeta> {
    let top = accepted.iter().max_by_key(|f| f.classification)?;
    Some(SpecMeta {
        spec_id: spec_id.to_string(),
        classification: crate::store::classification_str(top.classification).to_string(),
        evidence: format!("{} marked \"{}\"", top.name, top.marking),
        sources: accepted
            .iter()
            .map(|f| format!("{} · blake3:{}", f.name, &f.digest[..12]))
            .collect(),
    })
}

fn human_size(bytes: usize) -> String {
    if bytes >= 1_048_576 {
        format!("{:.1} MB", bytes as f64 / 1_048_576.0)
    } else if bytes >= 1024 {
        format!("{:.1} KB", bytes as f64 / 1024.0)
    } else {
        format!("{bytes} B")
    }
}

/// `governance.ingest` — stage 1 of the authoring pipeline.
///
/// Reads the operator's chosen paths from the local filesystem. Content is never
/// uploaded by this command; classification is what decides where it may go
/// next, which is why a file that cannot be classified is returned in `refused`
/// rather than accepted with a guess.
#[tauri::command]
pub fn governance_ingest(
    app: tauri::AppHandle,
    paths: Vec<String>,
    spec_id: Option<String>,
) -> Result<IngestResultDto, String> {
    use tauri::Manager;
    let root = app
        .path()
        .app_data_dir()
        .map_err(|e| format!("no app data dir: {e}"))?;
    let batch = ingest_paths(&paths);
    let resolved_id = spec_id.clone().unwrap_or_else(|| {
        let mut h = blake3::Hasher::new();
        for f in &batch.accepted {
            h.update(f.digest.as_bytes());
        }
        format!("spec-{}", &h.finalize().to_hex()[..12])
    });

    // Attach candidate values from the documents to this spec's interview, as
    // PROPOSALS (S7.2/S7.3). Ingest is the moment new information arrives, so it
    // is the moment to surface what the documents appear to say — and a proposal
    // never answers a topic, so doing it here cannot skip a question.
    let mut interview = crate::interview::load(&root, &resolved_id);
    crate::spec::propose_from_documents(&mut interview, &batch.accepted);
    crate::interview::save(&root, &interview)?;

    // The classification of the documents is durable state: the deploy ceiling
    // is read from it, and after this moment nothing else in the pipeline
    // remembers what the sources were marked.
    if let Some(meta) = meta_of(&resolved_id, &batch.accepted) {
        save_meta(&root, &meta)?;
    }

    Ok(IngestResultDto {
        // A spec id is minted per ingest when the caller did not name one. It is
        // derived from the accepted digests so re-ingesting the same documents
        // lands on the same draft instead of silently forking one.
        spec_id: resolved_id,
        files: batch
            .accepted
            .iter()
            .map(|f| IngestFileDto {
                name: f.name.clone(),
                size: human_size(f.bytes),
                status: "ingested".to_string(),
                class: crate::store::classification_str(f.classification).to_string(),
                note: format!("marked \"{}\"", f.marking),
                prov: format!("blake3:{} · {} chunks", &f.digest[..12], f.chunks.len()),
            })
            .collect(),
        refused: batch
            .refused
            .iter()
            .map(|(name, r)| RefusedDto {
                name: name.clone(),
                why: r.why(),
            })
            .collect(),
    })
}
