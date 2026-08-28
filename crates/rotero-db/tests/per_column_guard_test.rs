//! The per-column clocks a write stamps must match the columns it writes.
//!
//! On a per-column table the merge judges each column by its own clock, so a
//! write has to declare which columns it changed. Both ways of getting that
//! wrong are silent:
//!
//! - a column left out of the declaration never propagates, while its siblings
//!   do — the paper syncs, the title syncs, the DOI quietly does not, forever;
//! - a column wrongly included stamps a value this device did not write, so a
//!   stale local copy outranks and destroys a peer's real edit.
//!
//! Neither produces an error, a log line, or a failing assertion anywhere else.
//! Nothing in the health check looks for it. This file is what makes the
//! declaration safe to get wrong, by comparing it against the `SET` list of the
//! statement it accompanies.

use std::collections::BTreeSet;

/// Columns named in a statement's `SET` clause.
///
/// Deliberately a small hand-rolled scan rather than a SQL parser: the input is
/// a fixed set of constants in this repository, and a dependency that could
/// disagree with turso about the dialect would be a worse foundation than a
/// parser that fails loudly on anything it does not recognise.
fn set_columns(sql: &str) -> BTreeSet<String> {
    let Some(start) = sql.find(" SET ") else {
        return BTreeSet::new();
    };
    let rest = &sql[start + 5..];
    let end = rest.find(" WHERE ").unwrap_or(rest.len());
    rest[..end]
        .split(',')
        .filter_map(|assignment| {
            let name = assignment.split('=').next()?.trim();
            (!name.is_empty() && name.chars().all(|c| c.is_alphanumeric() || c == '_'))
                .then(|| name.to_string())
        })
        .collect()
}

fn cols(names: &[&str]) -> BTreeSet<String> {
    names.iter().map(|s| (*s).to_string()).collect()
}

/// Every `papers` write states the columns it changes, and states them exactly.
///
/// The expectations here are transcribed from the `touch_columns` call at each
/// write site. The test's value is that it reads the *statement* instead, so a
/// statement that gains or loses a column without its call site following fails
/// here rather than silently desynchronising one field.
#[test]
fn every_paper_write_declares_the_columns_it_changes() {
    use rotero_models::queries as q;

    // (statement, what the call site declares, why any difference is deliberate)
    let cases: Vec<(&str, &str, BTreeSet<String>)> = vec![
        (
            "PAPER_SET_FAVORITE",
            q::PAPER_SET_FAVORITE,
            cols(&["is_favorite"]),
        ),
        ("PAPER_SET_READ", q::PAPER_SET_READ, cols(&["is_read"])),
        (
            "PAPER_UPDATE_TITLE",
            q::PAPER_UPDATE_TITLE,
            cols(&["title", "date_modified"]),
        ),
        (
            "PAPER_UPDATE_PDF_PATH",
            q::PAPER_UPDATE_PDF_PATH,
            cols(&["pdf_path", "pdf_sha256", "date_modified"]),
        ),
        ("PAPER_TOUCH", q::PAPER_TOUCH, cols(&["date_modified"])),
        (
            "PAPER_UPDATE_CITATION_COUNT",
            q::PAPER_UPDATE_CITATION_COUNT,
            cols(&["citation_count"]),
        ),
        (
            "PAPER_UPDATE_METADATA",
            q::PAPER_UPDATE_METADATA,
            cols(&[
                "title",
                "authors",
                "year",
                "doi",
                "abstract_text",
                "journal",
                "volume",
                "issue",
                "pages",
                "publisher",
                "url",
                "date_modified",
                "item_type",
            ]),
        ),
    ];

    for (name, sql, declared) in cases {
        let actual = set_columns(sql);
        assert!(
            !actual.is_empty(),
            "{name}: the SET list could not be read — the scan above needs to \
             learn this statement's shape rather than silently checking nothing"
        );
        assert_eq!(
            actual, declared,
            "{name} writes {actual:?} but its `touch_columns` call declares \
             {declared:?}. A column in the statement and not the declaration \
             never syncs; one in the declaration and not the statement destroys \
             a peer's edit. Neither fails anywhere but here."
        );
    }
}

/// A write to a column that does not sync stamps no column clock.
///
/// `citation_key` is local-only — it is derived on demand for citation export —
/// so `update_citation_key` deliberately declares no columns. Stamping one
/// would publish a local derivation and let it outrank a peer's real metadata
/// edit, which is the same shape as the tag-retirement bug the previous pass
/// found.
#[test]
fn a_write_to_a_local_only_column_stamps_no_column_clock() {
    use rotero_models::queries as q;

    let written = set_columns(q::PAPER_UPDATE_CITATION_KEY);
    assert_eq!(written, cols(&["citation_key"]));

    let synced: BTreeSet<String> = rotero_db::sync_schema::synced_table("papers")
        .expect("papers must be a synced table")
        .columns
        .iter()
        .map(|c| (*c).to_string())
        .collect();
    assert!(
        written.is_disjoint(&synced),
        "`citation_key` is expected to be local-only; if it starts syncing, \
         `update_citation_key` has to declare it"
    );
}

/// Every payload column of a per-column table has both of its clock columns.
///
/// The clock names are generated from the manifest rather than listed, so this
/// is really a check that the generation and the schema agree. A payload column
/// added without its pair would be judged against a clock that does not exist.
#[tokio::test]
async fn every_per_column_table_has_a_clock_pair_for_each_payload_column() {
    let dir = tempfile::tempdir().unwrap();
    let db = rotero_db::Database::open(dir.path().to_path_buf())
        .await
        .unwrap();

    for table in rotero_db::sync_schema::SYNCED_TABLES {
        if !table.per_column {
            continue;
        }

        let mut present = BTreeSet::new();
        let mut rows = db
            .conn()
            .query(&format!("PRAGMA table_info({})", table.name), ())
            .await
            .unwrap();
        while let Some(row) = rows.next().await.unwrap() {
            if let Some(name) = row.get_value(1).ok().and_then(|v| v.as_text().cloned()) {
                present.insert(name);
            }
        }

        for column in table.columns {
            for clock in [format!("{column}_ua"), format!("{column}_ub")] {
                assert!(
                    present.contains(&clock),
                    "`{}`.`{clock}` is missing: `{column}` would be merged \
                     against a clock the database does not have",
                    table.name
                );
            }
        }
    }
}
