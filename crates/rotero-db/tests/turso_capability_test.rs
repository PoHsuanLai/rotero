//! Capabilities the per-device sync design depends on.
//!
//! The merge applies a peer row with a single conditional upsert, and the
//! soft-delete read path hides tombstoned rows behind a view. Both are SQLite
//! features that turso implements independently, so neither is guaranteed by the
//! dialect being "SQLite-compatible". These tests pin them: if a turso upgrade
//! drops or changes one, the failure names the capability instead of surfacing
//! as silently wrong sync results much later.

use turso::Value;

async fn memory_conn() -> turso::Connection {
    turso::Builder::new_local(":memory:")
        .build()
        .await
        .unwrap()
        .connect()
        .unwrap()
}

/// The LWW guard: `ON CONFLICT ... DO UPDATE ... WHERE`.
///
/// The `WHERE` on `DO UPDATE` is what makes the merge idempotent and keeps a
/// peer's older row from clobbering a newer local edit. Without it every apply
/// would overwrite unconditionally and last-writer-to-arrive would win, which is
/// not the same thing as last-writer-to-edit.
#[tokio::test]
async fn conditional_upsert_resolves_lww() {
    let c = memory_conn().await;
    c.execute(
        "CREATE TABLE t (id TEXT PRIMARY KEY, v TEXT NOT NULL, \
         updated_at INTEGER NOT NULL, updated_by TEXT NOT NULL)",
        (),
    )
    .await
    .unwrap();
    c.execute("INSERT INTO t VALUES ('a', 'local', 100, 'devA')", ())
        .await
        .unwrap();

    let upsert = "INSERT INTO t (id, v, updated_at, updated_by) VALUES (?1, ?2, ?3, ?4) \
         ON CONFLICT(id) DO UPDATE SET \
            v = excluded.v, updated_at = excluded.updated_at, updated_by = excluded.updated_by \
         WHERE excluded.updated_at > t.updated_at \
            OR (excluded.updated_at = t.updated_at AND excluded.updated_by > t.updated_by)";

    let apply = async |v: &str, at: i64, by: &str| {
        c.execute(
            upsert,
            turso::params::Params::Positional(vec![
                Value::Text("a".into()),
                Value::Text(v.into()),
                Value::Integer(at),
                Value::Text(by.into()),
            ]),
        )
        .await
        .unwrap();
        let mut rows = c.query("SELECT v FROM t WHERE id = 'a'", ()).await.unwrap();
        rows.next()
            .await
            .unwrap()
            .unwrap()
            .get_value(0)
            .unwrap()
            .as_text()
            .cloned()
            .unwrap()
    };

    assert_eq!(
        apply("older", 50, "devB").await,
        "local",
        "an older peer row must not clobber a newer local edit"
    );
    assert_eq!(
        apply("newer", 200, "devB").await,
        "newer",
        "a newer peer row must win"
    );
    assert_eq!(
        apply("tiebreak", 200, "devC").await,
        "tiebreak",
        "an equal timestamp must fall to the higher updated_by, deterministically"
    );
    assert_eq!(
        apply("stale-tie", 200, "devA").await,
        "tiebreak",
        "and the lower updated_by must lose that tiebreak"
    );
}

/// The same guard on a composite primary key, for the three junction tables.
#[tokio::test]
async fn conditional_upsert_works_on_composite_keys() {
    let c = memory_conn().await;
    c.execute(
        "CREATE TABLE j (a TEXT NOT NULL, b TEXT NOT NULL, updated_at INTEGER NOT NULL, \
         PRIMARY KEY (a, b))",
        (),
    )
    .await
    .unwrap();

    let upsert = "INSERT INTO j (a, b, updated_at) VALUES ('p', 't', ?1) \
         ON CONFLICT(a, b) DO UPDATE SET updated_at = excluded.updated_at \
         WHERE excluded.updated_at > j.updated_at";

    for at in [100, 50, 200] {
        c.execute(upsert, [Value::Integer(at)]).await.unwrap();
    }

    let mut rows = c.query("SELECT updated_at FROM j", ()).await.unwrap();
    let got = *rows
        .next()
        .await
        .unwrap()
        .unwrap()
        .get_value(0)
        .unwrap()
        .as_integer()
        .unwrap();
    assert_eq!(got, 200, "composite-key upsert must resolve LWW the same way");
}

/// The soft-delete read path: a view that hides tombstoned rows.
///
/// Reads go through `<table>_live` so forgetting the `deleted = 0` filter means
/// naming the wrong relation rather than silently resurrecting deleted rows.
#[tokio::test]
async fn a_view_hides_soft_deleted_rows() {
    let c = memory_conn().await;
    c.execute(
        "CREATE TABLE t (id TEXT PRIMARY KEY, deleted INTEGER NOT NULL DEFAULT 0)",
        (),
    )
    .await
    .unwrap();
    c.execute("CREATE VIEW t_live AS SELECT * FROM t WHERE deleted = 0", ())
        .await
        .unwrap();
    c.execute("INSERT INTO t VALUES ('a', 0), ('b', 0)", ())
        .await
        .unwrap();

    let live = async || {
        let mut rows = c.query("SELECT COUNT(*) FROM t_live", ()).await.unwrap();
        *rows
            .next()
            .await
            .unwrap()
            .unwrap()
            .get_value(0)
            .unwrap()
            .as_integer()
            .unwrap()
    };

    assert_eq!(live().await, 2, "a view must select live rows");
    c.execute("UPDATE t SET deleted = 1 WHERE id = 'a'", ())
        .await
        .unwrap();
    assert_eq!(live().await, 1, "and must hide a row once it is tombstoned");
}
