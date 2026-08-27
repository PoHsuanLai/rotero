//! Every read of a synced table must go through its `_live` view.
//!
//! This is the mechanical half of soft delete. The views make a forgotten
//! `deleted = 0` name a relation that does not exist, but only for queries that
//! actually use them — nothing stops a new query from selecting the base table
//! and quietly returning tombstoned rows. That failure is invisible until a user
//! notices a deleted paper reappearing, so it is checked here rather than left
//! to review.
//!
//! Crude by design: it reads the SQL as text. A query that legitimately needs
//! every row, tombstones included, is listed in `READS_ALL_ROWS` with a reason.

use std::collections::HashSet;

/// The tables whose reads must be filtered.
const SYNCED: &[&str] = &[
    "papers",
    "collections",
    "tags",
    "annotations",
    "notes",
    "saved_searches",
    "paper_collections",
    "paper_tags",
    "paper_citations",
];

/// Query constants that deliberately read tombstoned rows too.
const READS_ALL_ROWS: &[(&str, &str)] = &[
    // The delete cascades collect the rows they are about to tombstone. A
    // tombstoned membership still has to be found and re-stamped, or the
    // deletion stops propagating at whichever device already applied it.
    (
        "COLLECTION_MEMBER_PAPER_IDS",
        "cascade: collects rows to tombstone",
    ),
    (
        "TAG_MEMBER_PAPER_IDS",
        "cascade: collects rows to tombstone",
    ),
    (
        "PAPER_COLLECTION_IDS",
        "cascade: collects rows to tombstone",
    ),
    ("PAPER_TAG_IDS", "cascade: collects rows to tombstone"),
    (
        "TAG_FIND_BY_NAME_ANY",
        "`tags.name` is UNIQUE across dead rows, so creating a tag by name has \
         to see the tombstone that still holds it",
    ),
    // `tags.name` is UNIQUE across dead rows too, so a retired duplicate still
    // holds its name against the survivor. Looking only at live rows would miss
    // the clash and the insert would fail on every later merge.
    ("TAG_FIND_NAME_CLASH", "UNIQUE spans tombstones"),
];

#[test]
fn every_select_on_a_synced_table_uses_its_live_view() {
    let src = include_str!("../../rotero-models/src/queries.rs");
    let exempt: HashSet<&str> = READS_ALL_ROWS.iter().map(|(name, _)| *name).collect();

    let mut offenders = Vec::new();

    for block in src.split("pub const ").skip(1) {
        let Some((name, rest)) = block.split_once(':') else {
            continue;
        };
        let name = name.trim();
        // Take what is between the first `"` and the statement's `;` — the
        // literal itself, without the `&str =` that precedes it. Getting this
        // wrong is silent: the check simply inspects nothing and passes.
        let Some(semi) = rest.find(';') else {
            continue;
        };
        let Some(open_quote) = rest[..semi].find('"') else {
            continue;
        };
        let sql = &rest[open_quote..semi];

        // Collapse the literal, its escapes, and its line continuations into one
        // line of SQL.
        let flat: String = sql
            .replace(['\\', '"'], " ")
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ");

        if !flat.to_uppercase().starts_with("SELECT") || exempt.contains(name) {
            continue;
        }

        for table in SYNCED {
            for keyword in ["FROM", "JOIN"] {
                let needle = format!("{keyword} {table} ");
                let tail = format!("{keyword} {table}");
                let hits = flat.contains(&needle) || flat.trim_end().ends_with(&tail);
                if hits {
                    offenders.push(format!("{name} reads `{table}` instead of `{table}_live`"));
                }
            }
        }
    }

    assert!(
        offenders.is_empty(),
        "these queries would return soft-deleted rows:\n  {}\n\n\
         Point them at the `_live` view, or add them to READS_ALL_ROWS with a reason.",
        offenders.join("\n  ")
    );
}
