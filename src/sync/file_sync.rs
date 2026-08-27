//! Per-device snapshot sync via shared folders (iCloud Drive, Dropbox, etc.)
//!
//! Sync folder layout: `{sync_folder}/devices/` (one snapshot per device, plus a
//! `.meta` sidecar carrying its checksum) and `papers/` (mirrored PDFs).
//!
//! Each device is the only writer of its own file, so there are no write
//! conflicts to resolve and no per-peer cursor to keep. A device reads every
//! other device's snapshot and merges it; which copy of a row wins is decided
//! by its clock, identically on every device.

use std::path::{Path, PathBuf};

use rotero_db::Database;

pub struct FileSyncEngine {
    sync_folder: PathBuf,
    site_id: Vec<u8>,
}

impl FileSyncEngine {
    pub fn new(sync_folder: PathBuf, site_id: Vec<u8>) -> Self {
        Self {
            sync_folder,
            site_id,
        }
    }

    /// This device's identity as lowercase hex, used to name its snapshot.
    fn site_id_hex(&self) -> String {
        self.site_id.iter().map(|b| format!("{b:02x}")).collect()
    }

    fn devices_dir(&self) -> PathBuf {
        self.sync_folder.join("devices")
    }

    /// This device's snapshot, and the sidecar holding its checksum.
    fn snapshot_paths(&self) -> (PathBuf, PathBuf) {
        let dir = self.devices_dir();
        let hex = self.site_id_hex();
        (
            dir.join(format!("{hex}.snapshot")),
            dir.join(format!("{hex}.meta")),
        )
    }

    /// Write this device's snapshot into the shared folder.
    ///
    /// Returns the number of rows written, or 0 if nothing changed since the
    /// last export. The whole table set is rewritten each time rather than a
    /// delta: it is self-healing (a corrupt file is replaced on the next tick),
    /// idempotent, and there is no cursor to get wrong — the bug that let one
    /// device park a shared cursor at its own version and silently stop every
    /// other device from exporting.
    pub async fn export_changes(&self, db: &Database) -> Result<usize, String> {
        let bytes = db
            .write_snapshot()
            .await
            .map_err(|e| format!("Failed to build snapshot: {e}"))?;

        let (snapshot_path, meta_path) = self.snapshot_paths();
        let checksum = rotero_db::snapshot::checksum(&bytes);

        // Skip the write when nothing changed. A full snapshot is cheap to
        // build but not free to upload, and a cloud folder does not need a
        // multi-megabyte rewrite every 30 seconds while the user reads.
        if std::fs::read_to_string(&meta_path).is_ok_and(|prev| prev.trim() == checksum) {
            return Ok(0);
        }

        std::fs::create_dir_all(self.devices_dir())
            .map_err(|e| format!("Failed to create devices dir: {e}"))?;

        // Snapshot first, then the checksum. A peer that sees no sidecar, or one
        // that disagrees, skips this device for the tick — so a partly-uploaded
        // snapshot is a skip rather than a bad merge.
        write_atomic(&snapshot_path, &bytes)
            .map_err(|e| format!("Failed to write snapshot: {e}"))?;
        write_atomic(&meta_path, checksum.as_bytes())
            .map_err(|e| format!("Failed to write snapshot checksum: {e}"))?;

        let (header, _) = rotero_db::snapshot::parse_snapshot(&bytes)
            .map_err(|e| format!("Failed to re-read own snapshot: {e}"))?;
        Ok(header.rows)
    }

    /// Merge every other device's snapshot.
    ///
    /// Returns the total number of rows applied. A peer whose file is missing,
    /// unreadable, mismatched against its checksum, or written by a newer build
    /// is skipped for this tick and logged — never fatal. Cloud providers hand
    /// out partly-downloaded and placeholder files routinely, and one bad file
    /// must not stop every other device's changes from arriving. The previous
    /// engine used `?` here, so a single unparseable file aborted the whole
    /// import pass.
    pub async fn import_changes(&self, db: &Database) -> Result<usize, String> {
        let dir = self.devices_dir();
        if !dir.exists() {
            return Ok(0);
        }

        let mut total_applied = 0;
        for path in self.peer_files() {
            match self.merge_peer(db, &path).await {
                Ok(applied) => total_applied += applied,
                Err(e) => tracing::warn!("Skipping {}: {e}", path.display()),
            }
        }

        Ok(total_applied)
    }

    /// Every other device's snapshot in the shared folder, sorted.
    ///
    /// Excludes this device's own file: reading it back would decompress and
    /// walk a whole copy of the library on every tick to apply nothing.
    fn peer_files(&self) -> Vec<PathBuf> {
        let mine = format!("{}.snapshot", self.site_id_hex());
        let Ok(entries) = std::fs::read_dir(self.devices_dir()) else {
            return Vec::new();
        };
        let mut files: Vec<PathBuf> = entries
            .filter_map(|e| e.ok())
            .map(|e| e.path())
            .filter(|p| p.extension().is_some_and(|ext| ext == "snapshot"))
            .filter(|p| p.file_name().is_none_or(|n| n != mine.as_str()))
            .collect();
        files.sort();
        files
    }

    /// The oldest `generated_at` across every readable peer snapshot.
    ///
    /// This is what proves the other devices have seen everything older than it,
    /// and it is the bound the tombstone reaper needs. `None` when no peer
    /// snapshot could be read — either there are no peers yet, or none of their
    /// files are currently intact — in which case nothing may be reaped.
    ///
    /// Deliberately the minimum rather than the maximum: one device that has not
    /// published in months holds every tombstone back, which is the safe
    /// direction. Reaping past it would delete a row that device still needs to
    /// hear about.
    pub async fn peer_horizon(&self) -> Option<i64> {
        let dir = self.devices_dir();
        let mine = format!("{}.snapshot", self.site_id_hex());
        let entries = std::fs::read_dir(&dir).ok()?;

        let mut oldest: Option<i64> = None;
        for entry in entries.filter_map(Result::ok) {
            let path = entry.path();
            if path.extension().is_none_or(|e| e != "snapshot")
                || path.file_name().is_some_and(|n| n == mine.as_str())
            {
                continue;
            }
            let Ok(bytes) = std::fs::read(&path) else {
                continue;
            };
            let Ok((header, _)) = rotero_db::snapshot::parse_snapshot(&bytes) else {
                continue;
            };
            oldest = Some(oldest.map_or(header.generated_at, |o: i64| o.min(header.generated_at)));
        }
        oldest
    }

    /// Merge one peer's snapshot, verifying it first.
    async fn merge_peer(&self, db: &Database, path: &Path) -> Result<usize, String> {
        let bytes = tokio::fs::read(path)
            .await
            .map_err(|e| format!("unreadable: {e}"))?;

        // The sidecar is a few dozen bytes, so it lands atomically enough even
        // over a network sync; the snapshot beside it may still be arriving.
        let meta_path = path.with_extension("meta");
        let expected =
            std::fs::read_to_string(&meta_path).map_err(|e| format!("no checksum sidecar: {e}"))?;
        let actual = rotero_db::snapshot::checksum(&bytes);
        if expected.trim() != actual {
            return Err("checksum mismatch — file is still uploading or corrupt".into());
        }

        let stats = db
            .merge_snapshot(&bytes)
            .await
            .map_err(|e| format!("merge failed: {e}"))?;
        Ok(stats.applied)
    }

    pub fn export_pdf(&self, library_papers_dir: &Path, rel_path: &str) -> Result<(), String> {
        let Some(safe) = safe_relative_path(rel_path) else {
            return Err(format!("Refusing to sync PDF at unsafe path {rel_path:?}"));
        };

        let src = library_papers_dir.join(&safe);
        if !src.exists() {
            return Ok(());
        }

        let dest = self.sync_folder.join("papers").join(&safe);
        if dest.exists() {
            return Ok(());
        }

        if let Some(parent) = dest.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| format!("Failed to create sync papers dir: {e}"))?;
        }
        std::fs::copy(&src, &dest).map_err(|e| format!("Failed to copy PDF to sync: {e}"))?;
        Ok(())
    }

    pub fn import_pdf(&self, library_papers_dir: &Path, rel_path: &str) -> Result<(), String> {
        let Some(safe) = safe_relative_path(rel_path) else {
            return Err(format!(
                "Refusing to import PDF at unsafe path {rel_path:?}"
            ));
        };

        let src = self.sync_folder.join("papers").join(&safe);
        if !src.exists() {
            return Ok(());
        }

        let dest = library_papers_dir.join(&safe);
        if dest.exists() {
            return Ok(());
        }

        if let Some(parent) = dest.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| format!("Failed to create local papers dir: {e}"))?;
        }
        std::fs::copy(&src, &dest).map_err(|e| format!("Failed to import PDF from sync: {e}"))?;
        Ok(())
    }
}

/// A peer-supplied path, accepted only if it stays inside the directory it is
/// joined onto.
///
/// `pdf_path` arrives over sync, so it is whatever another device wrote. It is
/// then joined onto the papers directory — and `Path::join` with an absolute
/// path *replaces* the base rather than extending it, so `/etc/foo` or
/// `../../../.zshrc` would read and write outside the library entirely. Only
/// ordinary path components are allowed: no root, no prefix, no `..`, and no
/// bare `.`.
fn safe_relative_path(rel_path: &str) -> Option<PathBuf> {
    if rel_path.is_empty() {
        return None;
    }

    let mut out = PathBuf::new();
    for component in Path::new(rel_path).components() {
        match component {
            std::path::Component::Normal(part) => out.push(part),
            // Anything else can leave the directory, so reject the whole path
            // rather than silently dropping the offending component.
            _ => return None,
        }
    }

    (!out.as_os_str().is_empty()).then_some(out)
}

/// Write a file by creating a sibling temporary and renaming it into place.
///
/// Everything here lives in a folder a cloud daemon watches and uploads. A plain
/// `write` is observable half-finished, and a truncated `.crr` is a parse error
/// that aborts a peer's entire import pass. The rename is atomic because the
/// temporary is in the same directory, hence the same filesystem.
fn write_atomic(path: &Path, data: &[u8]) -> std::io::Result<()> {
    let dir = path.parent().unwrap_or(Path::new("."));
    let tmp_name = format!(
        ".{}.tmp",
        path.file_name().and_then(|n| n.to_str()).unwrap_or("state")
    );
    let tmp = dir.join(tmp_name);

    std::fs::write(&tmp, data)?;
    match std::fs::rename(&tmp, path) {
        Ok(()) => Ok(()),
        Err(e) => {
            let _ = std::fs::remove_file(&tmp);
            Err(e)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn engine(folder: &Path, site: u8) -> FileSyncEngine {
        FileSyncEngine::new(folder.to_path_buf(), vec![site; 16])
    }

    fn hex(site: u8) -> String {
        vec![site; 16].iter().map(|b| format!("{b:02x}")).collect()
    }

    /// Each device writes its own file, so there is nothing to share.
    ///
    /// The changeset engine kept a per-device export cursor, and a shared state
    /// file let the busier device park it at its own version — after which the
    /// other found nothing newer, exported nothing, and reported success,
    /// permanently. Snapshots remove the cursor entirely; this pins down that
    /// two devices in one folder address separate files.
    #[test]
    fn each_device_owns_a_separate_snapshot_file() {
        let dir = tempfile::tempdir().unwrap();
        let (a_snap, a_meta) = engine(dir.path(), 1).snapshot_paths();
        let (b_snap, b_meta) = engine(dir.path(), 2).snapshot_paths();

        assert_ne!(
            a_snap, b_snap,
            "two devices must not write the same snapshot"
        );
        assert_ne!(a_meta, b_meta, "nor the same checksum sidecar");
        assert_eq!(
            a_snap.parent(),
            b_snap.parent(),
            "but they must still share one devices directory"
        );
    }

    /// A snapshot with no checksum sidecar, or the wrong one, must be skipped.
    ///
    /// Cloud providers routinely expose a file that is still uploading. Merging
    /// one would apply a truncated library; the sidecar is what makes that a
    /// skip instead.
    #[tokio::test]
    async fn a_peer_snapshot_without_a_matching_checksum_is_skipped() {
        let dir = tempfile::tempdir().unwrap();
        let lib = tempfile::tempdir().unwrap();
        let db = rotero_db::Database::open(lib.path().to_path_buf())
            .await
            .unwrap();

        let devices = dir.path().join("devices");
        std::fs::create_dir_all(&devices).unwrap();
        let peer = devices.join("aabbcc.snapshot");
        std::fs::write(&peer, b"not a snapshot at all").unwrap();

        let me = engine(dir.path(), 1);

        // No sidecar: skipped, and the pass still succeeds.
        assert_eq!(
            me.import_changes(&db).await.unwrap(),
            0,
            "a snapshot with no checksum must be skipped, not merged"
        );

        // Wrong sidecar: same.
        std::fs::write(devices.join("aabbcc.meta"), "0000").unwrap();
        assert_eq!(
            me.import_changes(&db).await.unwrap(),
            0,
            "a checksum mismatch must be skipped, not merged"
        );
    }

    /// A snapshot that parses cleanly but does not match its checksum is
    /// rejected.
    ///
    /// The other checksum test writes bytes that are not a snapshot at all, so
    /// it passes on the parse failure alone and proves nothing about the
    /// checksum. This one swaps in a *valid* snapshot from a different library —
    /// what a half-finished cloud upload of an older version looks like — so
    /// only the checksum can catch it.
    #[tokio::test]
    async fn a_valid_snapshot_with_the_wrong_checksum_is_rejected() {
        let shared = tempfile::tempdir().unwrap();
        let peer_lib = tempfile::tempdir().unwrap();
        let my_lib = tempfile::tempdir().unwrap();

        let peer_db = rotero_db::Database::open(peer_lib.path().to_path_buf())
            .await
            .unwrap();
        peer_db
            .insert_paper(&rotero_models::Paper {
                title: "Should not arrive".into(),
                ..Default::default()
            })
            .await
            .unwrap();
        engine(shared.path(), 7)
            .export_changes(&peer_db)
            .await
            .unwrap();

        // Corrupt only the sidecar, leaving a perfectly parseable snapshot.
        let devices = shared.path().join("devices");
        std::fs::write(devices.join(format!("{}.meta", hex(7))), "0".repeat(64)).unwrap();

        let my_db = rotero_db::Database::open(my_lib.path().to_path_buf())
            .await
            .unwrap();
        let applied = engine(shared.path(), 1)
            .import_changes(&my_db)
            .await
            .unwrap();

        assert_eq!(applied, 0, "a checksum mismatch must be skipped");
        assert!(
            my_db.list_papers().await.unwrap().is_empty(),
            "and nothing from that peer may be applied"
        );
    }

    /// A device's own snapshot is filtered out of the import.
    ///
    /// Skipping it is an optimisation rather than a correctness boundary: were
    /// it read, every row would lose the clock comparison against itself and a
    /// failed read is skipped anyway, so the library is safe either way. What
    /// this pins is that the filter selects the right file — an off-by-one in
    /// the name comparison would either read a whole extra copy of the library
    /// every tick, or silently skip a real peer.
    #[tokio::test]
    async fn a_device_filters_its_own_snapshot_out_of_the_import() {
        let shared = tempfile::tempdir().unwrap();
        let devices = shared.path().join("devices");
        std::fs::create_dir_all(&devices).unwrap();

        // Two files whose names differ only by device.
        for site in [3u8, 4u8] {
            std::fs::write(devices.join(format!("{}.snapshot", hex(site))), b"x").unwrap();
        }

        let mine = engine(shared.path(), 3);
        let peers = mine.peer_files();

        assert_eq!(
            peers.len(),
            1,
            "exactly one file is a peer's, got {peers:?}"
        );
        assert!(
            peers[0].ends_with(format!("{}.snapshot", hex(4))),
            "the peer's file must be the one kept, got {peers:?}"
        );
    }

    /// One unreadable peer must not stop the others from arriving.
    ///
    /// The changeset engine used `?` on the read and the parse, so a single bad
    /// file aborted the whole import pass and every other device's changes with
    /// it.
    #[tokio::test]
    async fn one_bad_peer_does_not_abort_the_pass() {
        let shared = tempfile::tempdir().unwrap();
        let good_lib = tempfile::tempdir().unwrap();
        let my_lib = tempfile::tempdir().unwrap();

        let good_db = rotero_db::Database::open(good_lib.path().to_path_buf())
            .await
            .unwrap();
        good_db
            .insert_paper(&rotero_models::Paper {
                title: "Should still arrive".into(),
                ..Default::default()
            })
            .await
            .unwrap();

        // A healthy peer writes a real snapshot...
        engine(shared.path(), 9)
            .export_changes(&good_db)
            .await
            .unwrap();

        // ...and a broken one sits alphabetically before it.
        let devices = shared.path().join("devices");
        std::fs::write(devices.join("00000000.snapshot"), b"garbage").unwrap();

        let my_db = rotero_db::Database::open(my_lib.path().to_path_buf())
            .await
            .unwrap();
        let applied = engine(shared.path(), 1)
            .import_changes(&my_db)
            .await
            .unwrap();

        assert!(
            applied > 0,
            "the healthy peer's rows must arrive despite the broken file"
        );
        assert_eq!(my_db.list_papers().await.unwrap().len(), 1);
    }

    /// The horizon is the oldest peer, not the newest.
    ///
    /// It bounds what the tombstone reaper may destroy, so taking the maximum
    /// would let one recently-synced device authorize deleting rows another
    /// device has never seen.
    #[tokio::test]
    async fn the_peer_horizon_is_the_oldest_peer() {
        let shared = tempfile::tempdir().unwrap();
        let devices = shared.path().join("devices");
        std::fs::create_dir_all(&devices).unwrap();

        // Two peers with different generation times, written by exporting real
        // libraries so the snapshots are well-formed.
        let mut stamps = Vec::new();
        for (i, site) in [7u8, 8u8].into_iter().enumerate() {
            let lib = tempfile::tempdir().unwrap();
            let db = rotero_db::Database::open(lib.path().to_path_buf())
                .await
                .unwrap();
            db.insert_paper(&rotero_models::Paper {
                title: format!("Peer {i}"),
                ..Default::default()
            })
            .await
            .unwrap();
            engine(shared.path(), site)
                .export_changes(&db)
                .await
                .unwrap();

            let bytes = std::fs::read(devices.join(format!("{}.snapshot", hex(site)))).unwrap();
            let (header, _) = rotero_db::snapshot::parse_snapshot(&bytes).unwrap();
            stamps.push(header.generated_at);

            // Make the two generation times distinguishable.
            tokio::time::sleep(std::time::Duration::from_millis(5)).await;
        }

        let horizon = engine(shared.path(), 1).peer_horizon().await;
        assert_eq!(
            horizon,
            Some(*stamps.iter().min().unwrap()),
            "the horizon must be the oldest peer, so a lagging device holds \
             tombstones back"
        );
    }

    /// With no peers there is no horizon, and so nothing may be reaped.
    #[tokio::test]
    async fn no_peers_means_no_horizon() {
        let shared = tempfile::tempdir().unwrap();
        assert_eq!(
            engine(shared.path(), 1).peer_horizon().await,
            None,
            "an empty folder must not authorize reaping"
        );
    }

    /// Two libraries, one shared folder, through this engine.
    ///
    /// The scenarios the rollout plan lists for manual end-to-end checking, run
    /// against the real transport rather than the in-test harness — so the
    /// checksum sidecars, the skip-on-bad-file handling, and the on-disk layout
    /// are exercised too, not just the merge underneath them.
    mod two_device {
        use super::*;
        use rotero_db::Database;

        struct Pair {
            a: Database,
            b: Database,
            ea: FileSyncEngine,
            eb: FileSyncEngine,
            shared: tempfile::TempDir,
            _da: tempfile::TempDir,
            _db: tempfile::TempDir,
        }

        impl Pair {
            async fn new() -> Self {
                let shared = tempfile::tempdir().unwrap();
                let da = tempfile::tempdir().unwrap();
                let db_dir = tempfile::tempdir().unwrap();
                let a = Database::open(da.path().to_path_buf()).await.unwrap();
                let b = Database::open(db_dir.path().to_path_buf()).await.unwrap();
                Self {
                    a,
                    b,
                    ea: engine(shared.path(), 0xa1),
                    eb: engine(shared.path(), 0xb2),
                    shared,
                    _da: da,
                    _db: db_dir,
                }
            }

            /// One full round trip, as two sync ticks would produce.
            async fn sync(&self) {
                for _ in 0..2 {
                    self.ea.export_changes(&self.a).await.unwrap();
                    self.eb.export_changes(&self.b).await.unwrap();
                    self.ea.import_changes(&self.a).await.unwrap();
                    self.eb.import_changes(&self.b).await.unwrap();
                }
            }
        }

        async fn add_paper(db: &Database, title: &str) -> String {
            db.insert_paper(&rotero_models::Paper {
                title: title.into(),
                ..Default::default()
            })
            .await
            .unwrap()
        }

        async fn titles(db: &Database) -> Vec<String> {
            let mut t: Vec<String> = db
                .list_papers()
                .await
                .unwrap()
                .into_iter()
                .map(|p| p.title)
                .collect();
            t.sort();
            t
        }

        #[tokio::test]
        async fn a_new_paper_reaches_the_other_device() {
            let p = Pair::new().await;
            add_paper(&p.a, "Attention Is All You Need").await;
            p.sync().await;
            assert_eq!(titles(&p.b).await, vec!["Attention Is All You Need"]);
        }

        /// A tag from one device and a favorite from the other must both survive.
        #[tokio::test]
        async fn edits_from_both_devices_survive() {
            let p = Pair::new().await;
            let paper = add_paper(&p.a, "Shared").await;
            p.sync().await;

            let tag = p.a.get_or_create_tag("method", None).await.unwrap();
            p.a.add_tag_to_paper(&paper, &tag).await.unwrap();
            p.b.set_favorite(&paper, true).await.unwrap();
            p.sync().await;

            for (name, db) in [("A", &p.a), ("B", &p.b)] {
                assert_eq!(
                    db.list_tags_for_paper(&paper).await.unwrap().len(),
                    1,
                    "device {name} lost the tag"
                );
                let row = db
                    .get_papers_by_ids(std::slice::from_ref(&paper))
                    .await
                    .unwrap()
                    .pop()
                    .unwrap();
                assert!(row.status.is_favorite, "device {name} lost the favorite");
            }
        }

        /// The pre-existing cascade bug, asserted through the real transport.
        #[tokio::test]
        async fn deleting_a_collection_clears_memberships_everywhere() {
            let p = Pair::new().await;
            let paper = add_paper(&p.a, "Shelved").await;
            let shelf =
                p.a.insert_collection(&rotero_models::Collection::new("Shelf".into()))
                    .await
                    .unwrap();
            p.a.add_paper_to_collection(&paper, &shelf).await.unwrap();
            p.sync().await;
            assert_eq!(
                p.b.list_collections_for_paper(&paper).await.unwrap().len(),
                1
            );

            p.a.delete_collection(&shelf).await.unwrap();
            p.sync().await;

            for (name, db) in [("A", &p.a), ("B", &p.b)] {
                assert!(
                    db.list_collections_for_paper(&paper)
                        .await
                        .unwrap()
                        .is_empty(),
                    "device {name} kept a membership pointing at a deleted collection"
                );
                assert_eq!(db.list_papers().await.unwrap().len(), 1, "device {name}");
            }
        }

        #[tokio::test]
        async fn deleting_a_paper_removes_its_children_everywhere() {
            let p = Pair::new().await;
            let paper = add_paper(&p.a, "Doomed").await;
            p.a.insert_note(&rotero_models::Note::new(paper.clone(), "Thought".into()))
                .await
                .unwrap();
            p.sync().await;
            assert_eq!(p.b.list_notes_for_paper(&paper).await.unwrap().len(), 1);

            p.a.delete_paper(&paper).await.unwrap();
            p.sync().await;

            assert!(titles(&p.b).await.is_empty(), "the paper must be gone on B");
            assert!(
                p.b.list_notes_for_paper(&paper).await.unwrap().is_empty(),
                "its note must be gone on B too"
            );
        }

        #[tokio::test]
        async fn offline_edits_on_both_devices_converge() {
            let p = Pair::new().await;
            add_paper(&p.a, "Before").await;
            p.sync().await;

            add_paper(&p.a, "From A").await;
            add_paper(&p.b, "From B").await;
            p.sync().await;

            let expected = vec![
                "Before".to_string(),
                "From A".to_string(),
                "From B".to_string(),
            ];
            assert_eq!(titles(&p.a).await, expected, "device A");
            assert_eq!(titles(&p.b).await, expected, "device B");
        }

        /// A half-uploaded peer file must be skipped, and recover on republish.
        #[tokio::test]
        async fn a_truncated_peer_file_does_not_stop_sync() {
            let p = Pair::new().await;
            add_paper(&p.a, "Before truncation").await;
            p.sync().await;
            assert_eq!(titles(&p.b).await.len(), 1);

            let a_snap = p
                .shared
                .path()
                .join("devices")
                .join(format!("{}.snapshot", hex(0xa1)));
            let bytes = std::fs::read(&a_snap).unwrap();
            std::fs::write(&a_snap, &bytes[..bytes.len() / 2]).unwrap();

            p.eb.import_changes(&p.b).await.unwrap();
            assert_eq!(
                titles(&p.b).await.len(),
                1,
                "B must keep what it already had"
            );

            add_paper(&p.a, "After truncation").await;
            p.sync().await;
            assert_eq!(
                titles(&p.b).await.len(),
                2,
                "a corrupt file must self-heal on the next export"
            );
        }
    }

    /// `pdf_path` arrives from a peer, and `Path::join` with an absolute path
    /// replaces the base instead of extending it.
    #[test]
    fn peer_supplied_paths_cannot_escape_the_papers_directory() {
        for escape in [
            "../../../.zshrc",
            "/etc/passwd",
            "a/../../b.pdf",
            "..",
            "",
            "./x.pdf",
        ] {
            assert!(
                safe_relative_path(escape).is_none(),
                "{escape:?} must be rejected"
            );
        }

        assert_eq!(
            safe_relative_path("2024/paper.pdf"),
            Some(PathBuf::from("2024/paper.pdf")),
            "an ordinary relative path must still work"
        );
    }

    /// A reader must never observe a partially written file: peers scan for
    /// `*.crr` and abort their whole import pass on one parse error.
    #[test]
    fn writes_are_atomic_and_leave_no_temporary_behind() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("state.json");

        write_atomic(&path, b"first").unwrap();
        assert_eq!(std::fs::read(&path).unwrap(), b"first");

        write_atomic(&path, b"second").unwrap();
        assert_eq!(std::fs::read(&path).unwrap(), b"second");

        let leftovers: Vec<_> = std::fs::read_dir(dir.path())
            .unwrap()
            .filter_map(|e| e.ok())
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .filter(|n| n != "state.json")
            .collect();
        assert!(leftovers.is_empty(), "stray temp files: {leftovers:?}");
    }
}
