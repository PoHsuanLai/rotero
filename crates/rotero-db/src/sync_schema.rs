//! Which tables and columns sync between a user's devices.
//!
//! Single-sources the set the snapshot writer serializes, the merge applies, and
//! the health check verifies, so those three cannot disagree. A column present
//! in the database but missing here is silently never synced, which is the
//! failure this module exists to make visible: [`SYNCED_TABLES`] is checked
//! against `PRAGMA table_info` by a test, so adding a column to the SQL and
//! forgetting it here is a test failure rather than data that quietly stops
//! reaching the other device.

/// How a synced table's primary key is shaped.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PkSpec {
    /// One column, always `id` today.
    Single(&'static str),
    /// Two columns, as the three junction tables use.
    Composite(&'static str, &'static str),
}

impl PkSpec {
    /// The key columns, in declaration order.
    pub fn columns(&self) -> Vec<&'static str> {
        match self {
            PkSpec::Single(a) => vec![*a],
            PkSpec::Composite(a, b) => vec![*a, *b],
        }
    }
}

/// One table that syncs between devices.
#[derive(Clone, Copy, Debug)]
pub struct SyncedTable {
    /// The table name.
    pub name: &'static str,
    /// How its primary key is shaped.
    pub pk: PkSpec,
    /// Columns carried in a snapshot, excluding the primary key and the sync
    /// bookkeeping columns ([`SYNC_COLUMNS`]).
    pub columns: &'static [&'static str],
    /// Whether each payload column carries its own clock.
    ///
    /// A row-level clock decides the whole row at once, so a peer row that loses
    /// discards every column — including ones it changed and this device did
    /// not. Per-column clocks let each column be judged on its own, at the cost
    /// of two extra columns apiece in the table and in every snapshot.
    ///
    /// Set per table rather than globally, and stated rather than inferred. See
    /// the note on [`SYNCED_TABLES`] for why only `papers` has it.
    pub per_column: bool,
}

impl SyncedTable {
    /// Every column a snapshot row carries: key, payload, then bookkeeping.
    ///
    /// For a per-column table this includes each payload column's clock pair, so
    /// the snapshot writer, the merge, and the schema guard all see them without
    /// any of them keeping its own list.
    pub fn all_columns(&self) -> Vec<&'static str> {
        let mut cols = self.pk.columns();
        cols.extend_from_slice(self.columns);
        cols.extend(self.clock_columns());
        cols.extend_from_slice(SYNC_COLUMNS);
        cols
    }

    /// The per-column clock columns, in payload order: `{col}_ua`, `{col}_ub`.
    ///
    /// Empty for a row-level table. Generated rather than listed so that adding
    /// a payload column cannot silently leave its clocks behind — the schema
    /// guard compares this against `PRAGMA table_info`, so a missing pair is a
    /// test failure rather than a column that quietly stops syncing.
    pub fn clock_columns(&self) -> Vec<&'static str> {
        if !self.per_column {
            return Vec::new();
        }
        self.columns
            .iter()
            .flat_map(|c| [clock_at(c), clock_by(c)])
            .collect()
    }
}

/// Every clock column name, interned once.
///
/// The names are derived from the manifest rather than listed, but callers need
/// them as `&'static str` and this runs on the snapshot path — so they are built
/// once and reused. Leaking per call would leak on every sync tick, forever.
static CLOCK_NAMES: std::sync::OnceLock<std::collections::BTreeMap<String, &'static str>> =
    std::sync::OnceLock::new();

fn clock_names() -> &'static std::collections::BTreeMap<String, &'static str> {
    CLOCK_NAMES.get_or_init(|| {
        let mut m = std::collections::BTreeMap::new();
        for table in SYNCED_TABLES {
            if !table.per_column {
                continue;
            }
            for c in table.columns {
                for suffix in ["ua", "ub"] {
                    let key = format!("{c}_{suffix}");
                    let leaked: &'static str = Box::leak(key.clone().into_boxed_str());
                    m.insert(key, leaked);
                }
            }
        }
        m
    })
}

/// The name of a column's clock timestamp: `title` -> `title_ua`.
///
/// Panics if `column` is not a payload column of a per-column table, which is a
/// programming error rather than a runtime condition: the manifest is fixed at
/// compile time.
pub fn clock_at(column: &str) -> &'static str {
    clock_names()[&format!("{column}_ua")]
}

/// The name of a column's clock device id: `title` -> `title_ub`.
pub fn clock_by(column: &str) -> &'static str {
    clock_names()[&format!("{column}_ub")]
}

/// The bookkeeping columns every synced table carries.
///
/// `updated_at` is unix milliseconds and `updated_by` is a device id, compared
/// as a tuple so a merge is deterministic regardless of the order peer files are
/// read in. `deleted` is a tombstone flag rather than a row removal: a hard
/// delete leaves nothing to publish, so a peer still holding the row would
/// resurrect it on the next merge.
pub const SYNC_COLUMNS: &[&str] = &["updated_at", "updated_by", "deleted"];

/// Tables that sync, in an order where parents precede their children.
///
/// The order is presentational only — `PRAGMA foreign_keys` is off, so a
/// junction row may be applied before the paper it references and nothing
/// rejects it. It resolves once the parent arrives.
pub const SYNCED_TABLES: &[SyncedTable] = &[
    SyncedTable {
        name: "papers",
        pk: PkSpec::Single("id"),
        per_column: true,
        // `fulltext` is deliberately absent: it is re-extractable from the PDF,
        // it dominates the table's size, and syncing it would let a background
        // extraction on one device overwrite a real metadata edit from another.
        columns: &[
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
            "pdf_path",
            "pdf_sha256",
            "date_added",
            "date_modified",
            "is_favorite",
            "is_read",
            "extra_meta",
            "citation_count",
            "item_type",
        ],
    },
    SyncedTable {
        name: "collections",
        pk: PkSpec::Single("id"),
        per_column: false,
        columns: &["name", "parent_id", "position"],
    },
    SyncedTable {
        name: "tags",
        pk: PkSpec::Single("id"),
        per_column: false,
        columns: &["name", "color"],
    },
    SyncedTable {
        name: "annotations",
        pk: PkSpec::Single("id"),
        per_column: false,
        columns: &[
            "paper_id",
            "page",
            "ann_type",
            "color",
            "content",
            "geometry",
            "created_at",
            "modified_at",
        ],
    },
    SyncedTable {
        name: "notes",
        pk: PkSpec::Single("id"),
        per_column: false,
        columns: &["paper_id", "title", "body", "created_at", "modified_at"],
    },
    SyncedTable {
        name: "saved_searches",
        pk: PkSpec::Single("id"),
        per_column: false,
        columns: &["name", "query", "created_at"],
    },
    SyncedTable {
        name: "paper_collections",
        pk: PkSpec::Composite("paper_id", "collection_id"),
        per_column: false,
        columns: &[],
    },
    SyncedTable {
        name: "paper_tags",
        pk: PkSpec::Composite("paper_id", "tag_id"),
        per_column: false,
        columns: &[],
    },
    SyncedTable {
        name: "paper_citations",
        pk: PkSpec::Composite("citing_paper_id", "cited_paper_id"),
        per_column: false,
        columns: &[],
    },
];

/// Look up a synced table by name.
pub fn synced_table(name: &str) -> Option<&'static SyncedTable> {
    SYNCED_TABLES.iter().find(|t| t.name == name)
}
