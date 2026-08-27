//! Running a generated scenario against real databases.
//!
//! The devices here are real libraries on disk exchanging real snapshots
//! through `TestSyncEngine`, not a model of them. A simulation would only ever
//! prove the model self-consistent, and the bugs this suite is built to find
//! live in the SQL — a tombstone that updates zero rows, a key predicate with
//! the wrong arity — none of which a model would reproduce.

use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet};
use std::future::Future;

use rotero_db::Database;
use rotero_db::sync_schema::SYNCED_TABLES;
use tokio::runtime::Runtime;

use super::sync_model::{Event, Op, Scenario, Sym, name_of, title_of};

thread_local! {
    static RT: RefCell<Option<Runtime>> = const { RefCell::new(None) };
}

/// Run a future on this thread's reused runtime.
///
/// One runtime per test thread rather than per case: proptest re-enters its
/// closure once per generated scenario, and building a runtime each time costs
/// more than the scenario does. Current-thread rather than multi-thread because
/// the work is one connection awaited in sequence, so a work-stealing pool buys
/// nothing and pays for thread spawns.
pub fn block_on<F: Future>(fut: F) -> F::Output {
    RT.with(|cell| {
        let mut slot = cell.borrow_mut();
        let rt = slot.get_or_insert_with(|| {
            tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .expect("building the test runtime must succeed")
        });
        rt.block_on(fut)
    })
}

/// Ids one device knows about, in creation order.
///
/// Append-only, including across deletes: a deleted entity keeps its slot so a
/// later op can still name it. Naming a dead row is worth generating — re-adding
/// a removed membership is exactly what `upsert_junction` exists to handle, and
/// dropping the id would make that sequence ungenerable.
#[derive(Default)]
struct Registry {
    papers: Vec<String>,
    collections: Vec<String>,
    tags: Vec<String>,
    notes: Vec<String>,
    annotations: Vec<String>,
}

fn resolve(pool: &[String], s: Sym) -> Option<String> {
    if pool.is_empty() {
        None
    } else {
        Some(pool[s.0 % pool.len()].clone())
    }
}

/// One device: its library, its registry, and its place in the shared folder.
pub struct DeviceCtx {
    pub db: Database,
    /// Where every device publishes, and this device's own name within it.
    shared: std::path::PathBuf,
    site: String,
    registry: Registry,
    _dir: tempfile::TempDir,
}

impl DeviceCtx {
    fn devices_dir(&self) -> std::path::PathBuf {
        self.shared.join("devices")
    }

    /// Publish this device's snapshot.
    async fn export(&self) -> Result<(), String> {
        let dir = self.devices_dir();
        std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
        let bytes = self.db.write_snapshot().await.map_err(|e| e.to_string())?;
        std::fs::write(dir.join(format!("{}.snapshot", self.site)), &bytes)
            .map_err(|e| e.to_string())
    }

    /// Merge every peer's snapshot.
    ///
    /// Deliberately not `TestSyncEngine::import_changes`, which unwraps a failed
    /// merge. Unwrapping is right for a hand-written test, where a merge error
    /// should stop things loudly — but here it aborts the process mid-scenario
    /// and takes the reason with it, so a real failure reads as a passing run.
    /// A merge that returns an error is itself a finding: the peer's entire
    /// snapshot is abandoned, and sync between those two devices stops.
    async fn import(&self) -> Result<(), String> {
        let dir = self.devices_dir();
        let Ok(entries) = std::fs::read_dir(&dir) else {
            return Ok(());
        };
        let mine = format!("{}.snapshot", self.site);
        let mut paths: Vec<_> = entries
            .filter_map(Result::ok)
            .map(|e| e.path())
            .filter(|p| {
                p.extension().is_some_and(|e| e == "snapshot")
                    && p.file_name().is_none_or(|n| n != mine.as_str())
            })
            .collect();
        // Read peers in a stable order: directory order is not guaranteed, and
        // a replayed seed has to merge them the way it did when it failed.
        paths.sort();

        for path in paths {
            let bytes = std::fs::read(&path).map_err(|e| e.to_string())?;
            self.db
                .merge_snapshot(&bytes)
                .await
                .map_err(|e| format!("merging {}: {e}", path.display()))?;
        }
        Ok(())
    }
}

/// A row's synced state, as another device would have to match it.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct RowState {
    pub deleted: bool,
    pub updated_at: i64,
    pub updated_by: String,
    /// Payload columns, or `None` for a tombstone.
    ///
    /// A tombstone carries no payload over the wire, and the receiver fills the
    /// NOT NULL columns with placeholders. Comparing dead rows' values would
    /// therefore flag a difference the protocol deliberately does not transmit.
    pub values: Option<BTreeMap<String, String>>,
}

/// Every synced row on one device, keyed by table and primary key.
pub type Canonical = BTreeMap<(&'static str, Vec<String>), RowState>;

/// A membership or row the scenario asked to be deleted and has not since
/// revived.
///
/// Recorded from the op log at the moment the op runs, never read back from the
/// database. That distinction is the whole point: when a delete fails to write
/// a tombstone at all — which is the bug this suite was built to find — a
/// database-derived expectation records nothing and the property has nothing to
/// check. The op log remembers the intent regardless of whether the engine
/// honoured it.
///
/// An edit or a re-add after a delete drops the entry again, because `touch`
/// and `upsert_junction` both clear `deleted`. A row that comes back after
/// being explicitly written to is a revival, not a resurrection, and the
/// difference is whether anyone asked for it.
pub type Intent = (&'static str, Vec<String>);

/// What running a scenario produced.
pub struct Outcome {
    pub devices: Vec<DeviceCtx>,
    /// Every deletion the scenario asked for, from the op log.
    pub tombstone_intents: BTreeSet<Intent>,
    /// Failures observed while running, reported rather than panicked so the
    /// property can attach the scenario to them.
    pub errors: Vec<String>,
}

/// Set up `n` devices sharing one folder.
///
/// Device ids are fixed and ordered rather than the random ones `Database::open`
/// mints, so a counterexample reads as "device-0 beat device-1" instead of two
/// opaque hex strings, and so a replayed seed resolves ties the same way it did
/// when it failed.
async fn setup(n: usize, shared: &std::path::Path) -> Vec<DeviceCtx> {
    let mut out = Vec::with_capacity(n);
    for i in 0..n {
        let dir = tempfile::tempdir().unwrap();
        let opened = super::open_test_db(dir.path()).await;
        let db = Database::from_parts(
            opened.conn().clone(),
            dir.path().to_path_buf(),
            std::sync::Arc::from(format!("device-{i}").as_str()),
        );
        out.push(DeviceCtx {
            db,
            shared: shared.to_path_buf(),
            site: format!("device-{i}"),
            registry: Registry::default(),
            _dir: dir,
        });
    }
    out
}

/// Give every device a small starting library.
///
/// Without this the generator wastes almost everything it draws. An op that
/// names a paper resolves against the acting device's own registry, and with a
/// handful of events spread across three devices, a removal almost always runs
/// before that device has created anything — measured at 124 scenarios in 200
/// containing a removal, but only 13 where the removal had a membership to
/// remove. Seeding is not part of the generated schedule and is identical on
/// every device, so it costs no schedule diversity; it just means the drawn ops
/// operate on something.
async fn seed(devices: &mut [DeviceCtx]) -> Vec<String> {
    let mut errors = Vec::new();
    for (i, ctx) in devices.iter_mut().enumerate() {
        for op in [
            Op::InsertPaper { title: 0 },
            Op::InsertPaper { title: 1 },
            Op::GetOrCreateTag { name: 0 },
            Op::InsertCollection { name: 0 },
            Op::AddTagToPaper {
                paper: Sym(0),
                tag: Sym(0),
            },
            Op::AddPaperToCollection {
                paper: Sym(0),
                coll: Sym(0),
            },
        ] {
            // Seeding uses the same code path as a generated op, so a failure
            // here is the same kind of finding — but it is not attributable to
            // any generated event, hence the separate label.
            let mut ignored = BTreeSet::new();
            if let Err(e) = apply(ctx, op, usize::MAX, &mut ignored).await {
                errors.push(format!("seeding device-{i}: {op:?}: {e}"));
            }
        }
    }
    errors
}

/// Run a scenario to completion, then settle the network.
pub async fn run(scenario: &Scenario, shared: &std::path::Path, max_papers: usize) -> Outcome {
    let mut devices = setup(scenario.devices, shared).await;
    let mut intents = BTreeSet::new();
    let mut errors = seed(&mut devices).await;

    for (step, event) in scenario.events.iter().enumerate() {
        match *event {
            Event::Local { device, op } => {
                let d = device as usize;
                if d >= devices.len() {
                    continue;
                }
                if let Err(e) = apply(&mut devices[d], op, max_papers, &mut intents).await {
                    errors.push(format!("step {step}: {op:?} on device-{d}: {e}"));
                }
            }
            Event::Export { device } => {
                let d = device as usize;
                if d < devices.len()
                    && let Err(e) = devices[d].export().await
                {
                    errors.push(format!("step {step}: export on device-{d}: {e}"));
                }
            }
            Event::Import { device } => {
                let d = device as usize;
                if d < devices.len()
                    && let Err(e) = devices[d].import().await
                {
                    errors.push(format!("step {step}: import on device-{d}: {e}"));
                }
            }
        }
    }

    errors.extend(quiesce(&devices).await);

    Outcome {
        devices,
        tombstone_intents: intents,
        errors,
    }
}

/// Exchange until every device has seen everything.
///
/// Not generated: convergence is only meaningful once the network has settled,
/// and a generator that had to discover a settling sequence on its own would
/// spend every case failing to find one.
///
/// Three rounds, not two. Two would be enough if merging only ever consumed
/// rows — one to publish, one to carry them on to a third device. But merging
/// can also *write*: resolving a tag-name collision repoints the loser's
/// memberships onto the survivor, and those rows are stamped by the device that
/// did the repointing, so they are news that still has to be published. A
/// membership created during the last round would otherwise be left sitting on
/// one device, looking exactly like a row that failed to propagate.
pub async fn quiesce(devices: &[DeviceCtx]) -> Vec<String> {
    let mut errors = Vec::new();
    for _ in 0..3 {
        // Export everything before importing anything. Interleaving the two
        // per device would let one device's rows reach a second within the
        // round but a third only in the next, making the number of rounds
        // needed depend on the order devices happen to be listed in.
        for d in devices {
            if let Err(e) = d.export().await {
                errors.push(format!("settling: export on {}: {e}", d.site));
            }
        }
        for d in devices {
            if let Err(e) = d.import().await {
                errors.push(format!("settling: import on {}: {e}", d.site));
            }
        }
    }
    errors
}

/// Apply one op, recording any deletion it asks for.
async fn apply(
    ctx: &mut DeviceCtx,
    op: Op,
    max_papers: usize,
    intents: &mut BTreeSet<Intent>,
) -> Result<(), String> {
    let db = &ctx.db;
    let err = |e: rotero_db::DbError| e.to_string();

    match op {
        Op::InsertPaper { title } => {
            // Past the cap this becomes a no-op rather than a rejected case:
            // every export re-serializes the whole table and every import
            // re-indexes FTS, so an unbounded tail of inserts costs more than
            // the rest of the suite. Filtering the scenario instead would fight
            // shrinking, which needs to be free to delete any event.
            if ctx.registry.papers.len() < max_papers {
                let id = db
                    .insert_paper(&rotero_models::Paper {
                        title: title_of(title),
                        ..Default::default()
                    })
                    .await
                    .map_err(err)?;
                ctx.registry.papers.push(id);
            }
        }
        Op::InsertCollection { name } => {
            let id = db
                .insert_collection(&rotero_models::Collection::new(name_of(name)))
                .await
                .map_err(err)?;
            ctx.registry.collections.push(id);
        }
        Op::GetOrCreateTag { name } => {
            let id = db
                .get_or_create_tag(&name_of(name), None)
                .await
                .map_err(err)?;
            // Creating a tag under a name a tombstone still holds revives that
            // row rather than making a second one, so this is a revival like
            // any edit: whoever asked for the name back asked for the tag back.
            intents.remove(&("tags", vec![id.clone()]));
            if !ctx.registry.tags.contains(&id) {
                ctx.registry.tags.push(id);
            }
        }
        Op::InsertNote { paper, body } => {
            if let Some(p) = resolve(&ctx.registry.papers, paper) {
                let mut note = rotero_models::Note::new(p, name_of(body));
                note.body = name_of(body);
                let id = db.insert_note(&note).await.map_err(err)?;
                ctx.registry.notes.push(id);
            }
        }
        Op::InsertAnnotation { paper, page } => {
            if let Some(p) = resolve(&ctx.registry.papers, paper) {
                let now = chrono::Utc::now();
                let ann = rotero_models::Annotation {
                    id: None,
                    paper_id: p,
                    page: page as i32,
                    ann_type: rotero_models::AnnotationType::Note,
                    color: "#ffff00".to_string(),
                    content: None,
                    geometry: serde_json::json!({}),
                    created_at: now,
                    modified_at: now,
                };
                let id = db.insert_annotation(&ann).await.map_err(err)?;
                ctx.registry.annotations.push(id);
            }
        }

        Op::RetitlePaper { paper, title } => {
            if let Some(p) = resolve(&ctx.registry.papers, paper) {
                db.update_paper_title(&p, &title_of(title))
                    .await
                    .map_err(err)?;
                intents.remove(&("papers", vec![p]));
            }
        }
        Op::SetFavorite { paper, on } => {
            if let Some(p) = resolve(&ctx.registry.papers, paper) {
                db.set_favorite(&p, on).await.map_err(err)?;
                intents.remove(&("papers", vec![p]));
            }
        }
        Op::RenameTag { tag, name } => {
            if let Some(t) = resolve(&ctx.registry.tags, tag) {
                // A rename onto a name another tag already holds violates the
                // UNIQUE constraint. That is a real hazard, but it is the
                // caller's to avoid, so it is not counted as an engine failure.
                if db.rename_tag(&t, &name_of(name)).await.is_ok() {
                    // Editing a row clears its tombstone: `touch` sets
                    // `deleted = 0`. An edit after a delete is therefore a
                    // deliberate revival, and the row is no longer expected to
                    // stay dead.
                    intents.remove(&("tags", vec![t]));
                }
            }
        }

        Op::AddTagToPaper { paper, tag } => {
            if let (Some(p), Some(t)) = (
                resolve(&ctx.registry.papers, paper),
                resolve(&ctx.registry.tags, tag),
            ) {
                db.add_tag_to_paper(&p, &t).await.map_err(err)?;
                intents.remove(&("paper_tags", vec![p, t]));
            }
        }
        Op::RemoveTagFromPaper { paper, tag } => {
            if let (Some(p), Some(t)) = (
                resolve(&ctx.registry.papers, paper),
                resolve(&ctx.registry.tags, tag),
            ) {
                db.remove_tag_from_paper(&p, &t).await.map_err(err)?;
                intents.insert(("paper_tags", vec![p, t]));
            }
        }
        Op::AddPaperToCollection { paper, coll } => {
            if let (Some(p), Some(c)) = (
                resolve(&ctx.registry.papers, paper),
                resolve(&ctx.registry.collections, coll),
            ) {
                db.add_paper_to_collection(&p, &c).await.map_err(err)?;
                intents.remove(&("paper_collections", vec![p, c]));
            }
        }
        Op::RemovePaperFromCollection { paper, coll } => {
            if let (Some(p), Some(c)) = (
                resolve(&ctx.registry.papers, paper),
                resolve(&ctx.registry.collections, coll),
            ) {
                db.remove_paper_from_collection(&p, &c).await.map_err(err)?;
                intents.insert(("paper_collections", vec![p, c]));
            }
        }
        Op::InsertCitation { citing, cited } => {
            if let (Some(a), Some(b)) = (
                resolve(&ctx.registry.papers, citing),
                resolve(&ctx.registry.papers, cited),
            ) {
                db.insert_citation(&a, &b).await.map_err(err)?;
            }
        }

        Op::DeletePaper { paper } => {
            if let Some(p) = resolve(&ctx.registry.papers, paper) {
                db.delete_paper(&p).await.map_err(err)?;
                intents.insert(("papers", vec![p]));
            }
        }
        Op::DeleteCollection { coll } => {
            if let Some(c) = resolve(&ctx.registry.collections, coll) {
                db.delete_collection(&c).await.map_err(err)?;
                intents.insert(("collections", vec![c]));
            }
        }
        Op::DeleteTag { tag } => {
            if let Some(t) = resolve(&ctx.registry.tags, tag) {
                db.delete_tag(&t).await.map_err(err)?;
                intents.insert(("tags", vec![t]));
            }
        }
    }
    Ok(())
}

/// Read one device's synced state.
///
/// Reads the base tables rather than the `_live` views. A device that merged a
/// deletion and a device that never heard of the row both show nothing through
/// `_live`, and calling those two equal is precisely how a lost deletion passes
/// for convergence.
pub async fn canonical(db: &Database) -> Canonical {
    let mut out = Canonical::new();

    for table in SYNCED_TABLES {
        let cols = table.all_columns();
        let sql = format!("SELECT {} FROM {}", cols.join(", "), table.name);
        let mut rows = db.conn().query(&sql, ()).await.unwrap();

        let key_len = table.pk.columns().len();
        let payload_len = table.columns.len();

        while let Some(row) = rows.next().await.unwrap() {
            let mut key = Vec::with_capacity(key_len);
            for i in 0..key_len {
                key.push(value_string(&row, i));
            }

            let mut values = BTreeMap::new();
            for (offset, name) in table.columns.iter().enumerate() {
                values.insert((*name).to_string(), value_string(&row, key_len + offset));
            }

            let base = key_len + payload_len;
            let updated_at = row
                .get_value(base)
                .ok()
                .and_then(|v| v.as_integer().copied())
                .unwrap_or(0);
            let updated_by = value_string(&row, base + 1);
            let deleted = row
                .get_value(base + 2)
                .ok()
                .and_then(|v| v.as_integer().copied())
                .unwrap_or(0)
                != 0;

            out.insert(
                (table.name, key),
                RowState {
                    deleted,
                    updated_at,
                    updated_by,
                    values: (!deleted).then_some(values),
                },
            );
        }
    }
    out
}

fn value_string(row: &turso::Row, idx: usize) -> String {
    match row.get_value(idx) {
        Ok(turso::Value::Null) => "\0null".to_string(),
        Ok(turso::Value::Integer(i)) => i.to_string(),
        Ok(turso::Value::Real(f)) => f.to_string(),
        Ok(turso::Value::Text(s)) => s.clone(),
        Ok(turso::Value::Blob(b)) => b.iter().map(|x| format!("{x:02x}")).collect(),
        Err(_) => "\0err".to_string(),
    }
}

/// Write rows in the snapshot format, without a database behind them.
///
/// `write_snapshot` serializes whatever is in the tables, which is the right
/// thing for the engine and the wrong thing for testing the format: it can only
/// ever produce rows the rest of the code already agreed to store. This builds
/// the same bytes from rows chosen freely, so the parser can be shown text the
/// writer would never have thought to emit.
pub fn encode_snapshot(site: &str, rows: &[rotero_db::snapshot::SnapshotRow]) -> Vec<u8> {
    use std::io::Write;

    let header = serde_json::json!({
        "format": rotero_db::snapshot::FORMAT_VERSION,
        "site_id": site,
        "generated_at": 0,
        "rows": rows.len(),
    });

    let mut plain = Vec::new();
    writeln!(plain, "{header}").unwrap();
    for row in rows {
        writeln!(plain, "{}", serde_json::to_string(row).unwrap()).unwrap();
    }

    let mut enc = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
    enc.write_all(&plain).unwrap();
    enc.finish().unwrap()
}

/// Whether two states of one row differ only in a tag's local retired name.
///
/// When two devices independently create a tag with the same name, every device
/// keeps the same survivor — `min(id)` — but renames the losers in its own
/// table only, because `tags.name` is UNIQUE and the rename is a local repair
/// rather than a fact to agree about. The devices therefore legitimately show
/// different names for a retired tag while agreeing on everything else,
/// including which tag won.
///
/// Narrow on purpose: it requires identical clocks and identical remaining
/// columns, and at least one side to actually carry a retired name. Any other
/// disagreement about a tag's name is still a failure.
fn differs_only_by_retirement(id: &str, x: &RowState, y: &RowState) -> bool {
    // The clock still has to match exactly. Retiring a duplicate is a local
    // repair that does not stamp anything, so it cannot move a row's clock —
    // and if the clocks differ, whatever caused that is not this.
    if x.updated_at != y.updated_at || x.updated_by != y.updated_by {
        return false;
    }

    // A retired name has to be the one derived from this row's own id. That is
    // what keeps the exemption honest: a device decides locally to retire a
    // tag, but it cannot invent a name, so the only retired name it can
    // legitimately show for this row is this one. A device showing a retired
    // name belonging to a different row is showing a decision that travelled,
    // which is what keeping the repair local exists to prevent.
    let expected = format!("__retired:{id}");
    let retired = |s: &RowState| match s.values.as_ref().and_then(|v| v.get("name")) {
        Some(n) => *n == expected,
        // A tombstone carries no payload, so a hidden retired row shows no
        // name at all. It counts as retired only if it is actually deleted.
        None => s.deleted,
    };
    if !retired(x) && !retired(y) {
        return false;
    }
    if let Some(n) = x.values.as_ref().and_then(|v| v.get("name"))
        && n.starts_with("__retired:")
        && *n != expected
    {
        return false;
    }
    if let Some(n) = y.values.as_ref().and_then(|v| v.get("name"))
        && n.starts_with("__retired:")
        && *n != expected
    {
        return false;
    }

    // Everything except the name may still have to agree — but only when both
    // sides carry a payload. One device hiding its retired duplicate while
    // another has not yet is the expected state, and a hidden row ships
    // nothing to compare.
    match (x.values.as_ref(), y.values.as_ref()) {
        (Some(xv), Some(yv)) => {
            let strip = |v: &BTreeMap<String, String>| {
                let mut c = v.clone();
                c.remove("name");
                c
            };
            strip(xv) == strip(yv)
        }
        _ => true,
    }
}

/// The first place two devices' states differ, rendered for a failure message.
///
/// A bare equality assertion on a map of a few hundred rows prints both in full
/// and leaves the reader to diff them by eye, which in practice means the
/// failure gets skimmed rather than read.
pub fn first_difference(a: &Canonical, b: &Canonical) -> Option<String> {
    let keys: BTreeSet<_> = a.keys().chain(b.keys()).collect();
    for k in keys {
        match (a.get(k), b.get(k)) {
            (x, y) if x == y => continue,
            (Some(x), Some(y))
                if k.0 == "tags"
                    && differs_only_by_retirement(
                        k.1.first().map(String::as_str).unwrap_or_default(),
                        x,
                        y,
                    ) =>
            {
                continue;
            }
            (Some(x), Some(y)) => {
                return Some(format!(
                    "{}{:?} differs:\n    left  = {x:?}\n    right = {y:?}",
                    k.0, k.1
                ));
            }
            (Some(x), None) => {
                return Some(format!("{}{:?} only on the left: {x:?}", k.0, k.1));
            }
            (None, Some(y)) => {
                return Some(format!("{}{:?} only on the right: {y:?}", k.0, k.1));
            }
            (None, None) => unreachable!("key came from one of the two maps"),
        }
    }
    None
}
