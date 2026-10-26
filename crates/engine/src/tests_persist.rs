//! A failed save is an error, not a silent success: a command whose journal append fails returns
//! [`EngineError::NotSaved`], its change stays applied and queued, and the next save writes it.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use lightcraft_catalog::{Flag, MemStore, SnapshotPolicy, Store};
use serde_json::json;

use crate::library::LibraryStores;
use crate::{EngineError, Session};

/// A store whose writes fail while `fail` is set (a full or unplugged disk).
#[derive(Clone, Default)]
struct Flaky {
    files: MemStore,
    fail: Arc<AtomicBool>,
    /// Only snapshots fail (appends work).
    snapshots_only: bool,
    /// Only appends fail (a dead log handle; fresh files can still be written).
    appends_only: bool,
}

impl Flaky {
    fn failing(&self) -> bool {
        self.fail.load(Ordering::SeqCst)
    }
    fn set_failing(&self, on: bool) {
        self.fail.store(on, Ordering::SeqCst);
    }
    /// The files as a crash at this moment would leave them.
    fn crash_copy(&self) -> Flaky {
        let m = MemStore::new();
        *m.files.lock().unwrap() = self.files.files.lock().unwrap().clone();
        Flaky { files: m, ..Default::default() }
    }
}

impl Store for Flaky {
    fn read(&mut self, name: &str) -> std::io::Result<Option<Vec<u8>>> {
        self.files.read(name)
    }
    fn write_atomic(&mut self, name: &str, data: &[u8]) -> std::io::Result<()> {
        if self.failing() && !self.appends_only {
            return Err(std::io::Error::other("disk full"));
        }
        self.files.write_atomic(name, data)
    }
    fn append(&mut self, name: &str, data: &[u8]) -> std::io::Result<()> {
        if self.failing() && !self.snapshots_only {
            return Err(std::io::Error::other("disk full"));
        }
        self.files.append(name, data)
    }
    fn truncate(&mut self, name: &str, len: u64) -> std::io::Result<()> {
        self.files.truncate(name, len)
    }
    fn describe(&self) -> String {
        "flaky".into()
    }
}

fn open(store: &Flaky, seed: bool) -> Session {
    let mut s = Session::new();
    let stores = LibraryStores { dir: "flaky".into(), catalog: Box::new(store.clone()), files: Box::new(MemStore::new()), on_disk: false };
    s.open_library_in(stores, seed).unwrap();
    s
}

#[test]
fn failed_append_fails_the_command_and_is_retried() {
    let store = Flaky::default();
    let mut s = open(&store, true);
    let id = s.selection.active.unwrap();
    s.execute("photo.rate", &json!({"rating": 1})).unwrap();
    store.set_failing(true);

    let e = s.execute("photo.rate", &json!({"rating": 4})).unwrap_err();
    assert!(matches!(e, EngineError::NotSaved(_)), "{e:?}");
    let msg = e.to_string();
    assert!(
        msg.starts_with("saved in memory but not written to disk: ") && msg.contains("disk full") && msg.ends_with("LightCraft will retry"),
        "{msg}"
    );
    // applied in memory, undoable, queued
    assert_eq!(s.catalog.photo(id).unwrap().rating, 4);
    assert!(!s.undo.is_empty());
    assert_eq!(s.unsaved().map(|u| u.0), Some(1));
    // queries and commands that change nothing are unaffected (they retry the queue, but report
    // their own result)
    let info = s.execute("library.info", &json!({})).unwrap();
    assert_eq!(info["unsavedOps"], 1, "{info}");
    assert!(info["unsavedError"].as_str().unwrap().contains("disk full"), "{info}");
    s.execute("library.select", &json!({"ids": [id.0]})).unwrap();
    // a second change fails too; both stay queued, in order
    let e = s.execute("photo.flag", &json!({"flag": "pick"})).unwrap_err();
    assert!(matches!(e, EngineError::NotSaved(_)));
    assert_eq!(s.unsaved().map(|u| u.0), Some(2));
    // the frame loop backs off instead of retrying every frame
    s.persist_if_dirty();
    assert_eq!(s.unsaved().map(|u| u.0), Some(2));
    // a crash now loses only the unsaved changes; the earlier ones are on disk
    assert_eq!(open(&store.crash_copy(), true).catalog.photo(id).unwrap().rating, 1);

    // the disk comes back: the next save writes the whole queue
    store.set_failing(false);
    s.execute("library.info", &json!({})).unwrap();
    assert_eq!(s.unsaved(), None);
    assert!(s.library.as_ref().unwrap().last_error.is_none());
    let expect = s.catalog.to_snapshot();
    drop(s); // no close: like a crash
    let s2 = open(&store, true);
    assert_eq!(s2.catalog.to_snapshot(), expect);
    let p = s2.catalog.photo(id).unwrap();
    assert_eq!((p.rating, p.flag), (4, Flag::Pick));
}

/// Undo is a command like any other: a failed write of its op is reported, and retried.
#[test]
fn failed_append_of_undo_is_reported_and_retried() {
    let store = Flaky::default();
    let mut s = open(&store, true);
    let id = s.selection.active.unwrap();
    let before = s.catalog.photo(id).unwrap().rating;
    s.execute("photo.rate", &json!({"rating": 3})).unwrap();
    store.set_failing(true);
    assert!(matches!(s.execute("edit.undo", &json!({})), Err(EngineError::NotSaved(_))));
    assert_eq!(s.catalog.photo(id).unwrap().rating, before);
    store.set_failing(false);
    s.persist().unwrap();
    drop(s);
    assert_eq!(open(&store, true).catalog.photo(id).unwrap().rating, before);
}

/// A failed compaction is not a failed command: the command's own records were appended.
#[test]
fn failed_snapshot_does_not_fail_the_command() {
    let store = Flaky { snapshots_only: true, ..Default::default() };
    let mut s = open(&store, false);
    s.library.as_mut().unwrap().journal_mut().policy =
        SnapshotPolicy { max_records: 1, max_bytes: u64::MAX, snapshot_share: 0, ..SnapshotPolicy::default() };
    store.set_failing(true);
    s.execute("album.create", &json!({"name": "A"})).unwrap();
    s.execute("album.create", &json!({"name": "B"})).unwrap();
    assert_eq!(s.unsaved(), None);
    assert!(s.library.as_ref().unwrap().last_error.is_some(), "the compaction failure is reported");
    drop(s);
    store.set_failing(false);
    assert_eq!(open(&store, false).catalog.albums().count(), 2);
}

/// Issue #101: quitting while the log can't be appended to still saves the queued changes in the
/// closing snapshot (a fresh file), and reports success; they are not appended again later.
#[test]
fn close_saves_queued_changes_in_the_snapshot_when_appends_fail() {
    let store = Flaky { appends_only: true, ..Default::default() };
    let mut s = open(&store, true);
    let id = s.selection.active.unwrap();
    store.set_failing(true);
    assert!(matches!(s.execute("photo.rate", &json!({"rating": 4})), Err(EngineError::NotSaved(_))));
    assert!(matches!(s.execute("photo.flag", &json!({"flag": "pick"})), Err(EngineError::NotSaved(_))));
    assert_eq!(s.unsaved().map(|u| u.0), Some(2));
    s.close_library().unwrap();
    assert_eq!(s.unsaved(), None);
    assert!(s.pending_log.is_empty());
    let expect = s.catalog.to_snapshot();
    drop(s);
    store.set_failing(false);
    let mut s2 = open(&store, true);
    assert_eq!(s2.catalog.to_snapshot(), expect);
    let p = s2.catalog.photo(id).unwrap();
    assert_eq!((p.rating, p.flag), (4, Flag::Pick));
    // later edits continue after the absorbed ops
    s2.execute("photo.rate", &json!({"rating": 2})).unwrap();
    let expect = s2.catalog.to_snapshot();
    drop(s2);
    assert_eq!(open(&store, true).catalog.to_snapshot(), expect);
}

/// When neither the log nor the snapshot can be written, closing fails with `NotSaved` and the
/// changes stay queued (a later retry can still save them).
#[test]
fn close_fails_when_nothing_can_be_saved() {
    let store = Flaky::default();
    let mut s = open(&store, true);
    store.set_failing(true);
    assert!(s.execute("photo.rate", &json!({"rating": 4})).is_err());
    let e = s.close_library().unwrap_err();
    assert!(matches!(e, EngineError::NotSaved(_)), "{e:?}");
    assert_eq!(s.unsaved().map(|u| u.0), Some(1));
    store.set_failing(false);
    s.close_library().unwrap();
    assert_eq!(s.unsaved(), None);
}

/// Issue #2266: a presets.json that can't be written isn't only logged: the preset counts as
/// unsaved (the top bar warns, the web backup refuses) until a later save writes it.
#[test]
fn a_preset_that_cannot_be_written_counts_as_unsaved_until_it_is() {
    let files = Flaky::default();
    let mut s = Session::new();
    let stores = LibraryStores { dir: "flaky".into(), catalog: Box::new(MemStore::new()), files: Box::new(files.clone()), on_disk: false };
    s.open_library_in(stores, true).unwrap();
    assert!(s.unsaved().is_none());
    files.set_failing(true);
    s.execute("preset.create", &json!({"name": "Mine"})).unwrap();
    let (n, why) = s.unsaved().expect("the preset is unsaved");
    assert_eq!(n, 1);
    assert!(why.contains("presets") && why.contains("disk full"), "{why}");
    let stored = |f: &Flaky| f.files.files.lock().unwrap().get("presets.json").cloned().unwrap_or_default();
    assert!(!String::from_utf8_lossy(&stored(&files)).contains("Mine"));
    // still failing: still unsaved
    s.persist().unwrap();
    assert!(s.unsaved().is_some());
    files.set_failing(false);
    s.persist().unwrap();
    assert!(s.unsaved().is_none(), "written once the disk works again");
    assert!(String::from_utf8_lossy(&stored(&files)).contains("Mine"));
}

/// Given a library with a large catalog, when it is closed after one change, then the snapshot
/// is not rewritten for it: the change is in the log, and opening the library again shows it.
/// A small library (the default) is compacted on close as before, and closing with nothing
/// changed writes nothing.
#[test]
fn close_rewrites_a_large_snapshot_only_when_the_log_is_long() {
    let store = Flaky::default();
    let mut s = open(&store, true);
    let id = s.selection.active.unwrap();
    let snap = || store.files.get("catalog.snap").unwrap();
    let log = || store.files.get("catalog.log").unwrap_or_default();

    // small: compacted on close
    s.execute("photo.rate", &json!({"rating": 2})).unwrap();
    assert!(!log().is_empty());
    s.close_library().unwrap();
    assert!(log().is_empty());
    let compacted = snap();

    // nothing changed: nothing written
    let written = s.library.as_ref().unwrap().journal().stats().snapshots;
    s.close_library().unwrap();
    assert_eq!(s.library.as_ref().unwrap().journal().stats().snapshots, written);

    // "large": the snapshot is above what a close rewrites in passing
    s.library.as_mut().unwrap().journal_mut().policy.close_bytes = 0;
    s.execute("photo.rate", &json!({"rating": 5})).unwrap();
    s.close_library().unwrap();
    assert_eq!(snap(), compacted, "the snapshot was rewritten");
    assert!(!log().is_empty());
    let expect = s.catalog.to_snapshot();
    drop(s);
    let s2 = open(&store, true);
    assert_eq!(s2.catalog.to_snapshot(), expect);
    assert_eq!(s2.catalog.photo(id).unwrap().rating, 5);
}

/// Given a library saved by an earlier version, when it is opened, then the person is told once
/// that it was upgraded and where the library as it was is kept (the files are theirs to
/// delete), and `library.info` lists it; a library that was not upgraded says nothing.
#[test]
fn an_upgraded_library_says_where_what_it_was_is_kept() {
    let dir = std::env::temp_dir().join(format!("lightcraft-kept-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("catalog.snap"), include_str!("../../catalog/src/fixtures/v6-catalog.snap.json")).unwrap();
    std::fs::write(dir.join("catalog.log"), include_str!("../../catalog/src/fixtures/v6-catalog.log.jsonl")).unwrap();
    let mut s = Session::new();
    s.open_library(&dir, false).unwrap();
    let told = s.take_library_warnings();
    assert!(told.iter().any(|w| w.contains("catalog.snap.v6-8") && w.contains("catalog.log.v6-8") && w.contains("delete")), "{told:?}");
    assert!(dir.join("catalog.snap.v6-8").exists() && dir.join("catalog.log.v6-8").exists());
    let info = s.execute("library.info", &serde_json::json!({})).unwrap();
    assert!(info["settingsWarnings"].to_string().contains("catalog.snap.v6-8"), "{info}");
    s.close_library().unwrap();
    s.open_library(&dir, false).unwrap();
    assert!(!s.take_library_warnings().iter().any(|w| w.contains("upgraded")));
    s.close_library().unwrap();
    let _ = std::fs::remove_dir_all(&dir);
}

/// A library in the one-file format (10) that could not keep its files at open: a directory sits where the
/// first kept file goes.
fn library_that_could_not_keep_its_files(name: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("lightcraft-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("catalog.snap.v10-10")).unwrap();
    std::fs::write(dir.join("catalog.snap.v10-10").join("in the way"), b"x").unwrap();
    std::fs::write(dir.join("catalog.snap"), include_str!("../../catalog/src/fixtures/v10-catalog.snap.json")).unwrap();
    std::fs::write(dir.join("catalog.log"), include_str!("../../catalog/src/fixtures/v10-catalog.log.jsonl")).unwrap();
    dir
}

/// The names of the files kept for format 10 in `dir`.
fn kept_for_v10(dir: &std::path::Path, prefix: &str) -> Vec<String> {
    let mut names: Vec<String> = std::fs::read_dir(dir)
        .unwrap()
        .map(|e| e.unwrap())
        .filter(|e| e.path().is_file())
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|n| n.starts_with(prefix) && !n.ends_with(".tmp"))
        .collect();
    names.sort();
    names
}

/// Given a library in the one-file format that could not keep its files at open, when a later compaction upgrades
/// it (the files can be kept now), then the person is told once where the library as it was is
/// kept, as when the open upgrades it, and a later compaction tells nothing more.
#[test]
fn a_later_upgrade_says_where_what_it_was_is_kept() {
    let dir = library_that_could_not_keep_its_files("kept-later");
    let mut s = Session::new();
    s.open_library(&dir, false).unwrap();
    assert!(s.take_library_warnings().iter().all(|w| !w.contains("upgraded")), "not upgraded at open");
    assert!(s.compact_library().is_err(), "its files can't be kept yet");
    std::fs::remove_dir_all(dir.join("catalog.snap.v10-10")).unwrap();
    s.compact_library().unwrap();
    let (snap, log) = (kept_for_v10(&dir, "catalog.snap.v10-"), kept_for_v10(&dir, "catalog.log.v10-"));
    assert_eq!((snap.len(), log.len()), (1, 1), "the files as they were are kept");
    let told = s.take_library_warnings();
    assert!(told.iter().any(|w| w.contains(&snap[0]) && w.contains(&log[0]) && w.contains("delete")), "{told:?}");
    let info = s.execute("library.info", &serde_json::json!({})).unwrap();
    assert!(info["settingsWarnings"].to_string().contains(&snap[0]), "{info}");
    s.compact_library().unwrap();
    assert!(s.take_library_warnings().is_empty(), "told once");
    s.close_library().unwrap();
    let _ = std::fs::remove_dir_all(&dir);
}

/// The same, when the compaction is written by the background worker: the person is told once its
/// result is taken (the frame loop's `persist`), not before.
#[test]
fn a_background_upgrade_says_where_what_it_was_is_kept() {
    let dir = library_that_could_not_keep_its_files("kept-background");
    let mut s = Session::new();
    s.open_library(&dir, false).unwrap();
    let _ = s.take_library_warnings();
    std::fs::remove_dir_all(dir.join("catalog.snap.v10-10")).unwrap();
    s.compact_soon();
    let started = std::time::Instant::now();
    let mut told = Vec::new();
    while told.is_empty() && started.elapsed() < std::time::Duration::from_secs(30) {
        std::thread::sleep(std::time::Duration::from_millis(10));
        s.persist().unwrap();
        told = s.take_library_warnings();
    }
    let (snap, log) = (kept_for_v10(&dir, "catalog.snap.v10-"), kept_for_v10(&dir, "catalog.log.v10-"));
    assert_eq!((snap.len(), log.len()), (1, 1));
    assert!(told.iter().any(|w| w.contains(&snap[0]) && w.contains(&log[0])), "{told:?}");
    s.persist().unwrap();
    assert!(s.take_library_warnings().is_empty(), "told once");
    s.close_library().unwrap();
    let _ = std::fs::remove_dir_all(&dir);
}

/// A batch that fails and whose rollback then fails halfway leaves the catalog changed. The change must reach the
/// journal (a restart finds what the session had) and the person must be able to take it back.
#[test]
fn a_batch_that_cannot_be_rolled_back_whole_is_journaled_and_undoable() {
    use lightcraft_catalog::{CatalogError, Op, PhotoId, Stack};

    let store = Flaky::default();
    let mut s = open(&store, true);
    let ids: Vec<PhotoId> = s.catalog.photos().map(|p| p.id).take(2).collect();
    let stack = s.catalog.alloc_stack_id();
    s.commit("Stack", Op::AddStack { stack: Stack { id: stack, photos: ids.clone(), collapsed: false } }).unwrap();
    s.persist().unwrap();
    let before = s.catalog.to_snapshot();

    // the photo goes, then its stack, then an op fails: putting the stack back needs the photo, which comes later
    let batch =
        Op::Batch { ops: vec![Op::RemovePhoto { id: ids[0] }, Op::RemoveStack { id: stack }, Op::SetRating { id: PhotoId(9_999), rating: 1 }] };
    let undo_steps = s.undo.len();
    let e = s.execute_fn("test.batch", |s| s.commit("Delete", batch).map(|()| json!({}))).unwrap_err();
    assert!(matches!(e, EngineError::Catalog(CatalogError::PartlyApplied { .. })), "{e:?}");
    assert!(e.to_string().contains("partly"), "the error says the change stayed in part: {e}");
    let torn = s.catalog.to_snapshot();
    assert_ne!(torn, before, "the rollback went only part of the way");
    assert!(s.catalog.photo(ids[0]).is_some() && s.catalog.stack(stack).is_none());

    // a restart in the middle finds what the session had
    assert_eq!(open(&store.crash_copy(), true).catalog.to_snapshot(), torn, "the journal and the memory agree");
    assert_eq!(s.undo.len(), undo_steps + 1, "what is left of the batch is one undo step");

    // undo finishes the rollback, and that too is journaled
    s.execute("edit.undo", &json!({})).unwrap();
    assert_eq!(s.catalog.to_snapshot(), before);
    assert_eq!(open(&store.crash_copy(), true).catalog.to_snapshot(), before);
}

/// An undo whose catalog step fails and cannot be rolled back whole is journaled as well, and what is left of it is a
/// step on the other stack that takes it back.
#[test]
fn an_undo_step_that_cannot_be_rolled_back_whole_is_journaled_and_can_be_taken_back() {
    use lightcraft_catalog::{CatalogError, Op, PhotoId, Stack};

    let store = Flaky::default();
    let mut s = open(&store, true);
    let ids: Vec<PhotoId> = s.catalog.photos().map(|p| p.id).take(2).collect();
    let stack = s.catalog.alloc_stack_id();
    s.commit("Stack", Op::AddStack { stack: Stack { id: stack, photos: ids.clone(), collapsed: false } }).unwrap();
    s.persist().unwrap();
    let before = s.catalog.to_snapshot();
    let torn =
        Op::Batch { ops: vec![Op::RemovePhoto { id: ids[0] }, Op::RemoveStack { id: stack }, Op::SetRating { id: PhotoId(9_999), rating: 1 }] };
    s.undo.push(crate::UndoEntry { label: "Broken".into(), op: torn, folders: Vec::new(), retry_inverse: Vec::new(), retry_sidecars: Vec::new(), trash: None, folders_done: Vec::new() });
    let (undo_len, redo_len) = (s.undo.len(), s.redo.len());

    let e = s.execute("edit.undo", &json!({})).unwrap_err();
    assert!(matches!(e, EngineError::Catalog(CatalogError::PartlyApplied { .. })), "{e:?}");
    let torn_state = s.catalog.to_snapshot();
    assert_ne!(torn_state, before);
    assert_eq!((s.undo.len(), s.redo.len()), (undo_len - 1, redo_len + 1), "the step is not left to fail again");
    assert_eq!(open(&store.crash_copy(), true).catalog.to_snapshot(), torn_state, "the journal and the memory agree");

    s.execute("edit.redo", &json!({})).unwrap();
    assert_eq!(s.catalog.to_snapshot(), before);
    assert_eq!(open(&store.crash_copy(), true).catalog.to_snapshot(), before);
}

/// A folder step whose catalog part is only partly taken back keeps the folder move in its undo step.
#[test]
fn a_partly_applied_folder_step_keeps_its_folder_move() {
    use lightcraft_catalog::{CatalogError, Op, PhotoId, Stack};

    let store = Flaky::default();
    let mut s = open(&store, true);
    let ids: Vec<PhotoId> = s.catalog.photos().map(|p| p.id).take(2).collect();
    let stack = s.catalog.alloc_stack_id();
    s.commit("Stack", Op::AddStack { stack: Stack { id: stack, photos: ids.clone(), collapsed: false } }).unwrap();
    let torn =
        || Op::Batch { ops: vec![Op::RemovePhoto { id: ids[0] }, Op::RemoveStack { id: stack }, Op::SetRating { id: PhotoId(9_999), rating: 1 }] };

    // the folder was renamed before the commit: the step that takes the change back renames it back
    let folder = crate::FolderMove { from: "/new".into(), to: "/old".into(), on_disk: true };
    let e = s.commit_with_folder("Rename Folder", torn(), folder.clone()).unwrap_err();
    assert!(matches!(e, EngineError::Catalog(CatalogError::PartlyApplied { .. })), "{e:?}");
    assert_eq!(s.undo.last().map(|u| u.folders.clone()), Some(vec![folder]));
    // (a failure that changed nothing leaves no step, so no folder to put on one)
    let steps = s.undo.len();
    assert!(
        s.commit_with_folder(
            "Rename Folder",
            Op::SetRating { id: PhotoId(9_999), rating: 1 },
            crate::FolderMove { from: "/a".into(), to: "/b".into(), on_disk: true }
        )
        .is_err()
    );
    assert_eq!(s.undo.len(), steps);
}

/// An undo step that fails part-way keeps the inverses of the path corrections it carried: they follow what is left,
/// as when a step succeeds.
#[test]
fn a_partly_applied_undo_step_keeps_its_retry_inverses() {
    use lightcraft_catalog::{Op, PhotoId, Stack};

    let mut s = open(&Flaky::default(), true);
    let ids: Vec<PhotoId> = s.catalog.photos().map(|p| p.id).take(2).collect();
    let fix = Op::SetRating { id: ids[1], rating: 4 };
    let stack = s.catalog.alloc_stack_id();
    s.commit("Stack", Op::AddStack { stack: Stack { id: stack, photos: ids.clone(), collapsed: false } }).unwrap();
    let step =
        Op::Batch { ops: vec![Op::RemovePhoto { id: ids[0] }, Op::RemoveStack { id: stack }, Op::SetRating { id: PhotoId(9_999), rating: 1 }] };
    s.undo.push(crate::UndoEntry { label: "Broken".into(), op: step, folders: Vec::new(), retry_inverse: vec![fix.clone()], retry_sidecars: Vec::new(), trash: None, folders_done: Vec::new() });
    assert!(s.execute("edit.undo", &json!({})).is_err());
    let Some(Op::Batch { ops }) = s.redo.last().map(|r| r.op.clone()) else { panic!("the step that takes it back is a batch") };
    assert_eq!(ops.last(), Some(&fix));
}
