//! Journals, retros, and the standup brief assembled from them (WP-S5.7).
//!
//! The planset's `02_ARCHITECTURE.md` §3.2 says an agent's brief is assembled
//! from its journals and retros, its open work packages, its blockers and its
//! pending permission requests — *"so meeting content is grounded in artifacts,
//! not model recall."* That sentence is the whole design constraint: a brief
//! this module cannot trace to a file on disk is a brief it must not produce.
//!
//! ## What it reads
//!
//! - `<root>/.agentile/docs/journals/*.md` — session reflections
//! - `<root>/.agentile/sprints/completed/*/RETRO.md` — sprint retrospectives
//!
//! Both carry Rule 12 frontmatter (`created`, `author`), a `# Title`, and a
//! `>` blockquote summary. That is the shape this parses, and a file that does
//! not have it is skipped and counted rather than guessed at.
//!
//! ## What it deliberately does not decide
//!
//! Whether an author is a human. The `author:` line reads
//! `Claude Opus 4.8 (1M context), directed by @SaulBuilds` — a model *and* the
//! person who directed it. Rather than pattern-match model names and get it
//! confidently wrong, `human` is left `None` when the line names both, and the
//! surface renders the author verbatim. An attribution that is quietly guessed
//! is worse than one that is plainly stated.

use std::path::Path;

/// One journal or retro entry, traced to the file it came from.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct JournalEntry {
    /// The source file's stem — stable, and the thing to open to check it.
    pub id: String,
    /// From frontmatter `created:`, date part only.
    pub date: String,
    /// From frontmatter `author:`, verbatim.
    pub who: String,
    /// `journal` or `retro`.
    pub kind: String,
    /// The title, and the blockquote summary if the file has one.
    pub text: String,
    /// `None` when the author line names both a model and a director, which is
    /// the normal case in this repo. Never guessed.
    pub human: Option<bool>,
}

/// What a read actually found, so the caller can state it (Rule 11).
#[derive(Clone, Debug, Default)]
pub struct JournalSource {
    pub journals: usize,
    pub retros: usize,
    /// Files that were present but did not parse as an entry.
    pub skipped: usize,
    pub missing_root: Option<String>,
}

impl JournalSource {
    pub fn describe(&self) -> String {
        if let Some(root) = &self.missing_root {
            return format!("no .agentile artifacts under {root} — nothing to report from");
        }
        if self.journals == 0 && self.retros == 0 {
            return "no journals or retros found — nothing to report from".to_string();
        }
        let mut s = format!(
            "{} journals + {} retros from .agentile/docs/journals and sprints/completed",
            self.journals, self.retros
        );
        if self.skipped > 0 {
            s.push_str(&format!(
                " ({} file(s) skipped — no frontmatter)",
                self.skipped
            ));
        }
        s
    }
}

/// Read every journal and retro under `root`.
///
/// Never errors: a missing tree yields an empty list that says so. Entries are
/// returned newest-first by date, which is the order a standup wants.
pub fn from_workspace(root: &Path) -> (Vec<JournalEntry>, JournalSource) {
    let mut out = Vec::new();
    let mut src = JournalSource::default();

    let agentile = root.join(".agentile");
    if !agentile.is_dir() {
        src.missing_root = Some(agentile.to_string_lossy().into_owned());
        return (out, src);
    }

    // journals
    let jdir = agentile.join("docs").join("journals");
    if let Ok(entries) = std::fs::read_dir(&jdir) {
        let mut files: Vec<_> = entries
            .filter_map(|e| e.ok())
            .map(|e| e.path())
            .filter(|p| p.extension().is_some_and(|x| x == "md"))
            .collect();
        files.sort();
        for f in files {
            match parse(&f, "journal") {
                Some(e) => {
                    out.push(e);
                    src.journals += 1;
                }
                None => src.skipped += 1,
            }
        }
    }

    // retros
    let sdir = agentile.join("sprints").join("completed");
    if let Ok(entries) = std::fs::read_dir(&sdir) {
        let mut dirs: Vec<_> = entries
            .filter_map(|e| e.ok())
            .map(|e| e.path())
            .filter(|p| p.is_dir())
            .collect();
        dirs.sort();
        for d in dirs {
            let f = d.join("RETRO.md");
            if !f.exists() {
                continue;
            }
            match parse(&f, "retro") {
                Some(e) => {
                    out.push(e);
                    src.retros += 1;
                }
                None => src.skipped += 1,
            }
        }
    }

    // Newest first — a standup reads backwards from now.
    out.sort_by(|a, b| b.date.cmp(&a.date));
    (out, src)
}

/// Parse one `.md` with Rule 12 frontmatter. `None` when it has none, which is
/// counted as skipped rather than filled in with defaults.
fn parse(path: &Path, kind: &str) -> Option<JournalEntry> {
    let text = std::fs::read_to_string(path).ok()?;
    let mut lines = text.lines();
    if lines.next()?.trim() != "---" {
        return None;
    }

    let (mut created, mut author) = (String::new(), String::new());
    for line in lines.by_ref() {
        let line = line.trim();
        if line == "---" {
            break;
        }
        if let Some(v) = line.strip_prefix("created:") {
            created = v.trim().to_string();
        } else if let Some(v) = line.strip_prefix("author:") {
            author = v.trim().to_string();
        }
    }
    if created.is_empty() || author.is_empty() {
        return None;
    }

    // Title, then the leading blockquote summary if there is one.
    let mut title = String::new();
    let mut summary = String::new();
    for line in lines {
        let t = line.trim();
        if title.is_empty() {
            if let Some(h) = t.strip_prefix("# ") {
                title = h.trim().to_string();
            }
            continue;
        }
        if let Some(q) = t.strip_prefix('>') {
            let q = q.trim();
            if !q.is_empty() {
                if !summary.is_empty() {
                    summary.push(' ');
                }
                summary.push_str(q);
            }
        } else if !summary.is_empty() {
            break; // the blockquote ended
        }
    }
    if title.is_empty() {
        return None;
    }

    let id = path.file_stem()?.to_string_lossy().into_owned();
    let date = created.split('T').next().unwrap_or(&created).to_string();
    let text = if summary.is_empty() {
        title.clone()
    } else {
        format!("{title} — {summary}")
    };

    Some(JournalEntry {
        id,
        date,
        who: author,
        kind: kind.to_string(),
        text,
        // Deliberately undecided: see the module docs.
        human: None,
    })
}

/// A section of a standup brief: a label and what it actually says.
pub type BriefSection = (String, String);

/// Assemble the artifact-derived half of a brief for `agent`.
///
/// The caller adds the live sections it alone knows — pending permission
/// requests, budget — because those come from the policy engine, not from
/// files. What this returns is grounded entirely in `.agentile/` artifacts.
///
/// An agent with no journals gets a brief that SAYS it has none. §3.2's point
/// is that a brief is grounded in artifacts; a generated one for an agent that
/// wrote nothing is precisely the failure the sentence guards against.
pub fn brief_from_artifacts(root: &Path, agent: &str) -> (Vec<BriefSection>, JournalSource) {
    let (entries, jsrc) = from_workspace(root);
    let (agenda, asrc) = crate::agenda_source::from_workspace(root);
    let mut sections: Vec<BriefSection> = Vec::new();

    // Entries this agent is named in. Substring rather than equality: the
    // author line is "Claude Opus 4.8 (1M context), directed by @SaulBuilds",
    // so an exact match would find nothing and silently report "no journals".
    let mine: Vec<&JournalEntry> = entries
        .iter()
        .filter(|e| e.who.to_lowercase().contains(&agent.to_lowercase()))
        .collect();

    sections.push((
        "Since last time".to_string(),
        if mine.is_empty() {
            format!("no journals or retros name \"{agent}\" — nothing to report from artifacts")
        } else {
            let heads: Vec<String> = mine
                .iter()
                .take(3)
                .map(|e| {
                    format!(
                        "{} ({})",
                        e.text.split(" — ").next().unwrap_or(&e.text),
                        e.date
                    )
                })
                .collect();
            format!("{} entries · most recent: {}", mine.len(), heads.join("; "))
        },
    ));

    sections.push((
        "Open work packages".to_string(),
        if agenda.is_empty() {
            "no active sprint found — nothing open to report".to_string()
        } else {
            format!("{} in {}", agenda.len(), asrc.sprints.join(", "))
        },
    ));

    // R5 again, stated per brief rather than only in the docs: the parts of
    // §3.2 that need repos and the memory graph are not here, and a brief that
    // omitted the omission would read as complete.
    sections.push((
        "Not in this brief".to_string(),
        "live branches and working trees (RepoDomain, QRM-S8) and the memory graph — \
         this brief is assembled only from .agentile files on disk"
            .to_string(),
    ));

    (sections, jsrc)
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    struct TempDir(PathBuf);
    impl TempDir {
        fn new(tag: &str) -> Self {
            let mut p = std::env::temp_dir();
            p.push(format!("quorum-journal-{tag}-{}", std::process::id()));
            let _ = std::fs::remove_dir_all(&p);
            std::fs::create_dir_all(&p).unwrap();
            Self(p)
        }
        fn journal(&self, name: &str, body: &str) {
            let d = self.0.join(".agentile/docs/journals");
            std::fs::create_dir_all(&d).unwrap();
            std::fs::write(d.join(name), body).unwrap();
        }
        fn retro(&self, sprint: &str, body: &str) {
            let d = self.0.join(".agentile/sprints/completed").join(sprint);
            std::fs::create_dir_all(&d).unwrap();
            std::fs::write(d.join("RETRO.md"), body).unwrap();
        }
    }
    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    const J: &str = "---\ncreated: 2026-07-25T05:30:00Z\nbranch: b\nauthor: Claude Opus 4.8, directed by @SaulBuilds\nstatus: final\n---\n\n# Running it is the test\n\n> Nine bugs a green gate could not see.\n> The packaged app is the test.\n\nBody text here.\n";

    #[test]
    fn a_journal_becomes_an_entry_carrying_its_author_verbatim() {
        let t = TempDir::new("one");
        t.journal("2026-07-25T0530_running.md", J);
        let (entries, src) = from_workspace(&t.0);

        assert_eq!(entries.len(), 1);
        let e = &entries[0];
        assert_eq!(e.date, "2026-07-25");
        assert_eq!(e.kind, "journal");
        assert_eq!(e.who, "Claude Opus 4.8, directed by @SaulBuilds");
        assert!(e.text.starts_with("Running it is the test — Nine bugs"));
        assert_eq!(e.human, None, "authorship is stated, never guessed");
        assert_eq!(src.journals, 1);
    }

    #[test]
    fn retros_are_read_too_and_labelled_as_retros() {
        let t = TempDir::new("retro");
        t.retro("sprint-qrm-s4", J);
        let (entries, src) = from_workspace(&t.0);
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].kind, "retro");
        assert_eq!(src.retros, 1);
    }

    #[test]
    fn a_file_without_frontmatter_is_counted_not_invented() {
        let t = TempDir::new("nofm");
        t.journal("bad.md", "# Just a heading\n\nno frontmatter at all\n");
        let (entries, src) = from_workspace(&t.0);
        assert!(entries.is_empty());
        assert_eq!(src.skipped, 1, "the skip must be visible, not silent");
    }

    #[test]
    fn a_missing_tree_says_so_rather_than_returning_a_confident_empty() {
        let t = TempDir::new("missing");
        let (entries, src) = from_workspace(&t.0);
        assert!(entries.is_empty());
        assert!(src.missing_root.is_some());
        assert!(src.describe().contains("nothing to report"));
    }

    #[test]
    fn entries_come_back_newest_first() {
        let t = TempDir::new("order");
        t.journal(
            "a.md",
            &J.replace("2026-07-25T05:30:00Z", "2026-07-20T05:30:00Z"),
        );
        t.journal("b.md", J);
        let (entries, _) = from_workspace(&t.0);
        assert_eq!(entries[0].date, "2026-07-25");
    }

    #[test]
    fn an_agent_with_no_journals_gets_a_brief_that_says_so() {
        let t = TempDir::new("nobody");
        t.journal("a.md", J);
        let (sections, _) = brief_from_artifacts(&t.0, "devin");

        let since = &sections[0].1;
        assert!(
            since.contains("nothing to report from artifacts"),
            "a brief for an agent that wrote nothing must say so, never be generated: {since}"
        );
    }

    #[test]
    fn a_brief_matches_an_agent_named_inside_a_longer_author_line() {
        // "Claude Opus 4.8, directed by @SaulBuilds" — an exact-match filter
        // would find nothing here and report "no journals" for the agent that
        // wrote every one of them.
        let t = TempDir::new("substr");
        t.journal("a.md", J);
        let (sections, _) = brief_from_artifacts(&t.0, "Claude");
        assert!(
            sections[0].1.starts_with("1 entries"),
            "got: {}",
            sections[0].1
        );
    }

    #[test]
    fn every_brief_names_what_it_leaves_out() {
        let t = TempDir::new("omit");
        t.journal("a.md", J);
        let (sections, _) = brief_from_artifacts(&t.0, "Claude");
        let omitted = sections.iter().find(|(k, _)| k == "Not in this brief");
        assert!(
            omitted.is_some(),
            "a brief that hides its own gaps reads as complete"
        );
        assert!(omitted.unwrap().1.contains("QRM-S8"));
    }
}
