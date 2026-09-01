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
    assert_eq!(
        got, 200,
        "composite-key upsert must resolve LWW the same way"
    );
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
    c.execute(
        "CREATE VIEW t_live AS SELECT * FROM t WHERE deleted = 0",
        (),
    )
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

/// Per-column LWW: a `CASE` expression inside `DO UPDATE SET`.
///
/// Row-level LWW gates the whole row on one `WHERE`, so a peer row that loses
/// discards every column — including ones it changed and this device did not.
/// Per-column clocks move the decision into each assignment, which needs
/// `excluded` to be readable inside a `CASE` on the right-hand side of a `SET`.
///
/// Pinned separately from the plain conditional upsert above because it is a
/// distinct capability: the merge for `papers` is generated around it, and if
/// turso ever evaluates `excluded` differently inside a `CASE` the result is
/// silently wrong column values rather than an error.
#[tokio::test]
async fn case_expressions_can_read_excluded_inside_do_update_set() {
    let c = memory_conn().await;
    c.execute(
        "CREATE TABLE t (id TEXT PRIMARY KEY, \
         a TEXT NOT NULL, a_ua INTEGER NOT NULL DEFAULT 0, \
         b TEXT NOT NULL, b_ua INTEGER NOT NULL DEFAULT 0)",
        (),
    )
    .await
    .unwrap();
    c.execute(
        "INSERT INTO t VALUES ('r', 'a-local', 100, 'b-local', 100)",
        (),
    )
    .await
    .unwrap();

    // Column `a` arrives newer, column `b` arrives older. Each must be judged
    // on its own clock, in one statement.
    let upsert = "INSERT INTO t (id, a, a_ua, b, b_ua) VALUES (?1, ?2, ?3, ?4, ?5) \
         ON CONFLICT(id) DO UPDATE SET \
            a = CASE WHEN excluded.a_ua > t.a_ua THEN excluded.a ELSE t.a END, \
            a_ua = MAX(t.a_ua, excluded.a_ua), \
            b = CASE WHEN excluded.b_ua > t.b_ua THEN excluded.b ELSE t.b END, \
            b_ua = MAX(t.b_ua, excluded.b_ua)";

    c.execute(
        upsert,
        turso::params::Params::Positional(vec![
            Value::Text("r".into()),
            Value::Text("a-peer".into()),
            Value::Integer(200),
            Value::Text("b-peer".into()),
            Value::Integer(50),
        ]),
    )
    .await
    .unwrap();

    let mut rows = c
        .query("SELECT a, a_ua, b, b_ua FROM t WHERE id = 'r'", ())
        .await
        .unwrap();
    let row = rows.next().await.unwrap().unwrap();
    let text = |i: usize| row.get_value(i).unwrap().as_text().cloned().unwrap();
    let int = |i: usize| *row.get_value(i).unwrap().as_integer().unwrap();

    assert_eq!(text(0), "a-peer", "the newer column must be taken");
    assert_eq!(int(1), 200, "and its clock with it");
    assert_eq!(text(2), "b-local", "the older column must be kept");
    assert_eq!(
        int(3),
        100,
        "and its clock must not move backwards to the peer's"
    );
}

/// Scalar `MAX` with a NULL argument returns NULL, not the non-NULL value.
///
/// The per-column merge advances a clock with `MAX(local, incoming)`. If either
/// side can be NULL — a column added by a later schema version, or a row the
/// backfill missed — that expression yields NULL and the clock is destroyed
/// rather than advanced, after which every future comparison against it fails.
///
/// This pins the hazard so the design's answer to it (`NOT NULL DEFAULT 0` on
/// every clock column) is a deliberate response to observed behaviour rather
/// than an assumption.
#[tokio::test]
async fn scalar_max_with_a_null_argument_is_null() {
    let c = memory_conn().await;
    let mut rows = c
        .query("SELECT MAX(NULL, 5), MAX(COALESCE(NULL, 0), 5)", ())
        .await
        .unwrap();
    let row = rows.next().await.unwrap().unwrap();

    assert!(
        row.get_value(0).unwrap().as_integer().is_none(),
        "MAX(NULL, 5) must be NULL — this is why clock columns are NOT NULL"
    );
    assert_eq!(
        row.get_value(1).unwrap().as_integer().copied(),
        Some(5),
        "COALESCE is the guard if a nullable clock ever appears"
    );
}
