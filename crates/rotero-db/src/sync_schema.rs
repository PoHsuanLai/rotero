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
}

impl SyncedTable {
    /// Every column a snapshot row carries: key, payload, then bookkeeping.
    pub fn all_columns(&self) -> Vec<&'static str> {
        let mut cols = self.pk.columns();
        cols.extend_from_slice(self.columns);
        cols.extend_from_slice(SYNC_COLUMNS);
        cols
    }
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
        columns: &["name", "parent_id", "position"],
    },
    SyncedTable {
        name: "tags",
        pk: PkSpec::Single("id"),
        columns: &["name", "color"],
    },
    SyncedTable {
        name: "annotations",
        pk: PkSpec::Single("id"),
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
        columns: &["paper_id", "title", "body", "created_at", "modified_at"],
    },
    SyncedTable {
        name: "saved_searches",
        pk: PkSpec::Single("id"),
        columns: &["name", "query", "created_at"],
    },
    SyncedTable {
        name: "paper_collections",
        pk: PkSpec::Composite("paper_id", "collection_id"),
        columns: &[],
    },
    SyncedTable {
        name: "paper_tags",
        pk: PkSpec::Composite("paper_id", "tag_id"),
        columns: &[],
    },
    SyncedTable {
        name: "paper_citations",
        pk: PkSpec::Composite("citing_paper_id", "cited_paper_id"),
        columns: &[],
    },
];

/// Look up a synced table by name.
pub fn synced_table(name: &str) -> Option<&'static SyncedTable> {
    SYNCED_TABLES.iter().find(|t| t.name == name)
}
