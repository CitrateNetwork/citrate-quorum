//! Agenda generation from real Agentile sprint files (WP-S5.2).
//!
//! The planset (`02_ARCHITECTURE.md` §7) says the room's agenda "can be
//! *generated from* the active sprint files". This module is that generator,
//! and it is the reason a Quorum standup is grounded in artifacts rather than
//! in what a model remembers about the project.
//!
//! ## What it reads
//!
//! `<root>/.agentile/sprints/active/*/SCOPE.md` — the same files a human reads
//! before a standup. From each it takes the open work packages, which are the
//! rows of the WP table, and the named risks.
//!
//! ## What it refuses to do
//!
//! It does **not** guess. A line that does not match the expected shape is
//! counted in `Agenda::skipped` and dropped. That counter exists because the
//! failure mode here is not a crash — it is a plausible-looking agenda derived
//! from a malformed file, which is exactly the class of invention Rule 1 bans
//! and which nobody would notice in a demo (sprint risk R-C).
//!
//! A missing directory is not an error either: it yields an empty agenda, and
//! the caller renders "no sprint source configured". An empty agenda that says
//! so is honest; an agenda with invented items is not.

use crate::Agenda;
use std::path::{Path, PathBuf};

/// What a generation run found, so the caller can state it rather than imply it.
#[derive(Clone, Debug, Default)]
pub struct AgendaSource {
    /// The sprint directories actually read.
    pub sprints: Vec<String>,
    /// Files that existed but yielded nothing parseable.
    pub empty_files: Vec<String>,
    /// Set when the sprints directory does not exist at all.
    pub missing_root: Option<String>,
}

impl AgendaSource {
    /// A one-line, honest description of where the agenda came from, for the
    /// Rule 11 data-source note the surface renders.
    pub fn describe(&self) -> String {
        if let Some(root) = &self.missing_root {
            return format!("no sprint source at {root} — agenda is empty, not generated");
        }
        if self.sprints.is_empty() {
            return "no active sprint found — agenda is empty, not generated".to_string();
        }
        format!(
            "generated from {} ({})",
            self.sprints.join(", "),
            "\u{2e}agentile/sprints/active/*/SCOPE.md"
        )
    }
}

/// Build an agenda from the active sprints under `root`.
///
/// Returns the agenda plus a description of what was read. Never errors: an
/// unreadable tree produces an empty agenda that says so, because a standup
/// that cannot start is worse than a standup with a stated-empty agenda.
pub fn from_workspace(root: &Path) -> (Agenda, AgendaSource) {
    let mut agenda = Agenda::new();
    let mut src = AgendaSource::default();

    let active = root.join(".agentile").join("sprints").join("active");
    let entries = match std::fs::read_dir(&active) {
        Ok(e) => e,
        Err(_) => {
            src.missing_root = Some(active.to_string_lossy().into_owned());
            return (agenda, src);
        }
    };

    // Sorted so the same workspace always produces the same agenda — and
    // therefore the same agendaHash. A hash that depends on readdir order
    // would be unverifiable on another machine.
    let mut dirs: Vec<PathBuf> = entries
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.is_dir())
        .collect();
    dirs.sort();

    for dir in dirs {
        let scope = dir.join("SCOPE.md");
        let Ok(text) = std::fs::read_to_string(&scope) else {
            continue;
        };
        let sprint = dir
            .file_name()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_else(|| "sprint".into());

        let before = agenda.len();
        let skipped = harvest(&text, &sprint, &mut agenda);
        agenda.skipped += skipped;

        if agenda.len() == before {
            src.empty_files.push(scope.to_string_lossy().into_owned());
        } else {
            src.sprints.push(sprint);
        }
    }

    (agenda, src)
}

/// Pull work packages and risks out of one SCOPE.md.
///
/// Returns the number of candidate lines that looked like table rows but did
/// not parse — the caller adds these to `Agenda::skipped`.
fn harvest(text: &str, sprint: &str, agenda: &mut Agenda) -> usize {
    let mut skipped = 0usize;

    for line in text.lines() {
        let line = line.trim();
        if !line.starts_with('|') || !line.ends_with('|') {
            continue;
        }
        let cells: Vec<&str> = line
            .trim_matches('|')
            .split('|')
            .map(|c| c.trim())
            .collect();
        if cells.len() < 2 {
            skipped += 1;
            continue;
        }

        // The header row and the `|---|---|` separator are structure, not
        // content — skipping them is not a parse failure.
        let first = cells[0];
        if first.eq_ignore_ascii_case("wp")
            || first.eq_ignore_ascii_case("#")
            || first
                .chars()
                .all(|c| c == '-' || c == ':' || c.is_whitespace())
        {
            continue;
        }

        // A work-package row: `| **S5.1** | what | acceptance |`
        let id = first.trim_matches('*').trim();
        let what = strip_markdown(cells[1]);
        if id.is_empty() || what.is_empty() {
            skipped += 1;
            continue;
        }
        if is_wp_id(id) {
            agenda.push(format!("{id} — {what}"), format!("{sprint}/SCOPE.md"));
        } else if is_risk_id(id) {
            agenda.push(
                format!("Risk {id} — {what}"),
                format!("{sprint}/SCOPE.md risks"),
            );
        } else {
            // A table that is neither, e.g. the out-of-scope list. Not a
            // failure — just not agenda material.
            continue;
        }
    }

    skipped
}

/// `S5.1`, `S2D.4` — a sprint work-package id.
fn is_wp_id(s: &str) -> bool {
    let mut chars = s.chars();
    if chars.next() != Some('S') {
        return false;
    }
    s.contains('.')
        && s.len() <= 8
        && s.chars()
            .skip(1)
            .all(|c| c.is_ascii_alphanumeric() || c == '.')
}

/// `R-A`, `R1`, `R12` — a risk id.
///
/// Deliberately strict about what follows the `R`: either digits, or a hyphen
/// and a single capital. An earlier version accepted "any alphanumerics up to
/// five characters", which matched the literal word **"Risk"** — so a table
/// whose first column was headed `Risk` produced an agenda item reading
/// "Risk Risk — Mitigation". That is the R-C failure mode exactly: not a
/// crash, just a plausible line nobody would question in a demo.
fn is_risk_id(s: &str) -> bool {
    let Some(rest) = s.strip_prefix('R') else {
        return false;
    };
    if rest.is_empty() {
        return false;
    }
    if let Some(tag) = rest.strip_prefix('-') {
        return tag.len() <= 2 && !tag.is_empty() && tag.chars().all(|c| c.is_ascii_uppercase());
    }
    rest.len() <= 3 && rest.chars().all(|c| c.is_ascii_digit())
}

/// Strip the markdown emphasis and inline code a SCOPE table uses, so an agenda
/// line reads as prose rather than as source.
fn strip_markdown(s: &str) -> String {
    let cleaned: String = s.replace("**", "").replace('`', "");
    cleaned.trim().to_string()
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    struct TempDir(PathBuf);
    impl TempDir {
        fn new(tag: &str) -> Self {
            let mut p = std::env::temp_dir();
            p.push(format!("quorum-agenda-{tag}-{}", std::process::id()));
            let _ = std::fs::remove_dir_all(&p);
            std::fs::create_dir_all(&p).unwrap();
            Self(p)
        }
        fn sprint(&self, name: &str, scope: &str) {
            let d = self.0.join(".agentile/sprints/active").join(name);
            std::fs::create_dir_all(&d).unwrap();
            std::fs::write(d.join("SCOPE.md"), scope).unwrap();
        }
    }
    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    const SCOPE: &str = "\
---
sprint: QRM-S5
---
# Sprint

| WP | What | Acceptance |
|----|------|-----------|
| **S5.1** | `quorum-meetings` crate — the lifecycle | it holds |
| **S5.2** | Agenda from real sprint files | items carry their source |

## Risks

| # | Risk | Sev |
|---|------|-----|
| **R-A** | The gate reads as met when it is half met | High |
";

    #[test]
    fn work_packages_and_risks_become_agenda_items_naming_their_file() {
        let t = TempDir::new("wp");
        t.sprint("sprint-qrm-s5", SCOPE);

        let (agenda, src) = from_workspace(&t.0);
        let items = agenda.items();
        assert_eq!(items.len(), 3, "two WPs and one risk");

        assert_eq!(items[0].n, 1);
        assert!(items[0].text.starts_with("S5.1 — quorum-meetings crate"));
        assert_eq!(items[0].src, "sprint-qrm-s5/SCOPE.md");

        assert!(items[2].text.starts_with("Risk R-A —"));
        assert_eq!(items[2].src, "sprint-qrm-s5/SCOPE.md risks");
        assert!(src.describe().contains("sprint-qrm-s5"));
    }

    #[test]
    fn a_missing_sprint_tree_yields_an_empty_agenda_that_says_so() {
        let t = TempDir::new("missing");
        let (agenda, src) = from_workspace(&t.0);
        assert!(agenda.is_empty());
        assert!(src.missing_root.is_some());
        assert!(
            src.describe().contains("not generated"),
            "an empty agenda must state it was not generated, never imply it was"
        );
    }

    #[test]
    fn a_scope_with_no_work_packages_invents_nothing() {
        let t = TempDir::new("prose");
        t.sprint(
            "sprint-x",
            "---\nsprint: X\n---\n# just prose\n\nNo tables here.\n",
        );
        let (agenda, src) = from_workspace(&t.0);
        assert!(agenda.is_empty(), "no table means no agenda, not a guess");
        assert_eq!(src.sprints.len(), 0);
        assert_eq!(src.empty_files.len(), 1);
    }

    #[test]
    fn the_same_workspace_always_produces_the_same_hash() {
        // Two sprints, so directory ordering could matter. It must not: a hash
        // that depends on readdir order cannot be verified on another machine.
        let t = TempDir::new("stable");
        t.sprint("sprint-b", SCOPE);
        t.sprint("sprint-a", SCOPE);

        let (first, _) = from_workspace(&t.0);
        let (second, _) = from_workspace(&t.0);
        assert_eq!(first.hash(), second.hash());
        assert_eq!(
            first.items()[0].src,
            "sprint-a/SCOPE.md",
            "sorted, not readdir order"
        );
    }

    #[test]
    fn a_malformed_row_is_counted_not_silently_dropped() {
        let t = TempDir::new("malformed");
        t.sprint(
            "sprint-y",
            "| WP | What |\n|---|---|\n| **S9.1** | real one |\n|  |\n",
        );
        let (agenda, _) = from_workspace(&t.0);
        assert_eq!(agenda.len(), 1);
        assert_eq!(agenda.skipped, 1, "the unparseable row must be visible");
    }

    #[test]
    fn out_of_scope_tables_are_not_agenda_material() {
        let t = TempDir::new("oos");
        t.sprint(
            "sprint-z",
            "| Needed for | Contract | Status |\n|---|---|---|\n| minutes hash | AnchorRegistry | not deployed |\n",
        );
        let (agenda, _) = from_workspace(&t.0);
        assert!(agenda.is_empty());
    }

    #[test]
    fn a_risk_table_header_does_not_become_an_agenda_item() {
        // Regression: a risk table headed `| Risk | Mitigation |` used to
        // produce the agenda line "Risk Risk — Mitigation", because the id
        // rule matched the word "Risk" itself.
        let t = TempDir::new("riskhdr");
        t.sprint(
            "sprint-r",
            "| Risk | Mitigation |\n|---|---|\n| **R-A** | a real one |\n",
        );
        let (agenda, _) = from_workspace(&t.0);
        assert_eq!(agenda.len(), 1, "the header must not become an item");
        assert!(agenda.items()[0].text.starts_with("Risk R-A —"));
    }

    #[test]
    fn id_shapes() {
        assert!(is_wp_id("S5.1"));
        assert!(is_wp_id("S2D.4"));
        assert!(!is_wp_id("Sprint"));
        assert!(!is_wp_id("Needed for"));
        assert!(is_risk_id("R-A"));
        assert!(is_risk_id("R12"));
        // The header word "Risk" is not a risk id. An earlier, looser rule
        // matched it and turned a table header into an agenda line.
        assert!(!is_risk_id("Risk"));
        assert!(!is_risk_id("Repos"));
        assert!(!is_risk_id("R"));
        assert!(!is_risk_id("R-abc"));
    }
}
