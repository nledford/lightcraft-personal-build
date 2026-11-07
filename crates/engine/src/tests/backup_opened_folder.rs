//! Issue #3240: a backup reads the library by path and files what it reads in a folder (by default
//! `Backups` in the library folder), and the removal of old backups and of what restores replaced
//! works in the library folder and in the folder backups go to. So each asks the folder the library
//! was opened from first. When the path leads to another folder than that (a share mounted over it,
//! or the folder moved away and another put in its place), nothing is written or removed there, the
//! time of the backup is not noted, and the reason is the refusal's. A folder's identity is its device
//! and inode number, so these run on Unix.

use std::path::{Path, PathBuf};
use std::sync::atomic::Ordering;
use std::sync::{Arc, Mutex};

use lightcraft_catalog::backup::{self, Own, Watch};
use lightcraft_catalog::restore;
use serde_json::json;

use crate::Session;
use crate::cmd::backup::{InOpenedFolder, note_finished, remove_outdated, remove_replaced};

/// What the reason for a refusal says.
const REFUSED: &str = "is no longer the folder this library was opened from";

/// The clock of a test's sessions, which it sets.
type Clock = Arc<Mutex<String>>;

/// A folder of its own for a test.
fn scratch(tag: &str) -> PathBuf {
    let base = std::env::temp_dir().join(format!("lc-engine-backup-opened-folder-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&base);
    std::fs::create_dir_all(&base).unwrap();
    base
}

/// A library on disk in `dir`, and the clock its session goes by.
fn open(dir: &Path) -> (Session, Clock) {
    let mut s = Session::new();
    let now: Clock = Arc::new(Mutex::new("2026-03-01T09:00:00".to_string()));
    let clock = now.clone();
    s.clock = Box::new(move || clock.lock().unwrap().clone());
    s.open_library(dir, true).unwrap();
    (s, now)
}

/// Everything under `dir`, by path below it: a file's bytes, a folder listed (empty).
fn tree(dir: &Path) -> Vec<(String, Vec<u8>)> {
    fn walk(root: &Path, dir: &Path, into: &mut Vec<(String, Vec<u8>)>) {
        for entry in std::fs::read_dir(dir).unwrap() {
            let path = entry.unwrap().path();
            let name = path.strip_prefix(root).unwrap().to_string_lossy().into_owned();
            if path.is_dir() {
                into.push((name, Vec::new()));
                walk(root, &path, into);
            } else {
                into.push((name, std::fs::read(&path).unwrap()));
            }
        }
    }
    let mut all = Vec::new();
    walk(dir, dir, &mut all);
    all.sort();
    all
}

/// A library in `dir` that was backed up and then restored from that backup at each of `times`:
/// its `Replaced` holds a folder for each. Closed again.
fn library_restored_at(dir: &Path, times: &[&str]) {
    let (mut s, now) = open(dir);
    // (before the time the tests' sessions go by, so a backup they make is told from this one)
    *now.lock().unwrap() = "2026-01-05T09:00:00".to_string();
    let made = s.execute("library.backup", &json!({})).unwrap();
    for time in times {
        *now.lock().unwrap() = (*time).to_string();
        let r = s.execute("library.restore", &json!({"dir": made["report"]["dir"]})).unwrap();
        assert!(r["error"].is_null(), "{r}");
    }
    s.close_library().unwrap();
}

/// A library of its own in `dir`, never backed up (so it has no `Backups` folder): what is at the
/// path when the folder that was opened is not. Closed again.
fn another_library(dir: &Path) {
    open(dir).0.close_library().unwrap();
}

/// The folder the open library was opened from moved away (the path returned) and `other` put in its
/// place, as a share mounted over it would look: the library's path leads to `other`'s folder now.
fn put_in_its_place(lib: &Path, other: &Path) -> PathBuf {
    let away = lib.with_extension("away");
    std::fs::rename(lib, &away).unwrap();
    std::fs::rename(other, lib).unwrap();
    away
}

/// The folder that was opened (`away`), back where it was: the folder at the path is moved aside.
fn put_back(lib: &Path, away: &Path) {
    std::fs::rename(lib, lib.with_extension("theirs")).unwrap();
    std::fs::rename(away, lib).unwrap();
}

/// Wait until the backup in the background is over.
fn finished(s: &Session) {
    let job = s.backup.clone().expect("a backup was started");
    let t0 = std::time::Instant::now();
    while !job.finished.load(Ordering::Relaxed) {
        assert!(t0.elapsed() < std::time::Duration::from_secs(20), "the backup never ends");
        std::thread::sleep(std::time::Duration::from_millis(2));
    }
}

/// Given a library whose path leads to another folder than the one it was opened from, when it is
/// backed up (here and now, or in the background as the app asks), then the backup is refused with the
/// reason before anything is made: nothing is written into either folder, no id is given to the
/// library, no backup is under way, and no time of a backup is noted; and with the folder that was
/// opened back, the backup is made.
fn is_refused_while_the_folder_is_replaced(background: bool) {
    let base = scratch(if background { "refused-in-the-background" } else { "refused" });
    let (lib, theirs) = (base.join("Lib"), base.join("Theirs"));
    library_restored_at(&lib, &[]);
    another_library(&theirs);
    let (mut s, _) = open(&lib);
    let last = s.last_backup.clone();
    assert!(last.is_some(), "backed up before");
    let away = put_in_its_place(&lib, &theirs);
    let (theirs, ours) = (tree(&lib), tree(&away));

    let e = s.execute("library.backup", &json!({"background": background})).expect_err("nothing is backed up by another folder's path").to_string();
    assert!(e.contains(REFUSED) && e.contains(lib.to_string_lossy().as_ref()), "{e}");
    assert_eq!(tree(&lib), theirs, "nothing was written into the folder at the path now");
    assert_eq!(tree(&away), ours, "or into the folder that was opened");
    assert!(s.backup.is_none() && s.activity.list().is_empty(), "no backup is under way");
    assert_eq!(s.last_backup, last, "and no time of a backup is noted");

    // the refusal is not for good
    put_back(&lib, &away);
    let r = s.execute("library.backup", &json!({"background": background})).unwrap();
    if background {
        finished(&s);
    }
    let r = if background { s.backup.as_ref().unwrap().json() } else { r };
    assert!(r["error"].is_null() && Path::new(r["report"]["dir"].as_str().unwrap()).starts_with(lib.join("Backups")), "{r}");
    drop(s);
    let _ = std::fs::remove_dir_all(&base);
}

#[test]
fn a_backup_is_refused_while_the_librarys_path_leads_to_another_folder() {
    is_refused_while_the_folder_is_replaced(false);
}

#[test]
fn a_backup_in_the_background_is_refused_while_the_librarys_path_leads_to_another_folder() {
    is_refused_while_the_folder_is_replaced(true);
}

/// Given a backup in the background that has not begun to read, when the library's folder is
/// replaced, then the worker writes nothing into the folder that is there now (not even the folder
/// the backup would be made in), and the backup ends with the reason, which is the backup's error.
/// The time of a backup is not noted for it.
#[test]
fn the_worker_of_a_backup_writes_nothing_into_another_folder() {
    let base = scratch("worker");
    let (lib, theirs) = (base.join("Lib"), base.join("Theirs"));
    library_restored_at(&lib, &[]);
    another_library(&theirs);
    let (mut s, _) = open(&lib);
    let last = s.last_backup.clone();
    s.backup_hold.store(true, Ordering::SeqCst);
    s.execute("library.backup", &json!({"background": true})).unwrap();

    let away = put_in_its_place(&lib, &theirs);
    let (theirs, ours) = (tree(&lib), tree(&away));
    s.backup_hold.store(false, Ordering::SeqCst);
    finished(&s);
    let job = s.backup.clone().unwrap();
    assert!(job.outcome().is_some_and(|o| o.is_err_and(|e| e.contains(REFUSED))), "{:?}", job.outcome());
    assert_eq!(tree(&lib), theirs, "the backup is not made in the folder at the path now");
    assert_eq!(tree(&away), ours);
    note_finished(&mut s);
    assert_eq!(s.last_backup, last, "a backup that was not made is not noted");
    drop(s);
    let _ = std::fs::remove_dir_all(&base);
}

/// Given a backup in the background that was made while the library's path was fine, when the folder
/// is replaced before the session takes note of it, then it is not noted: the time of a backup is
/// kept in the library's preferences, which are not those of the folder at the path now.
#[test]
fn a_backup_that_was_made_is_not_noted_for_another_folder() {
    let base = scratch("noted");
    let (lib, theirs) = (base.join("Lib"), base.join("Theirs"));
    library_restored_at(&lib, &[]);
    another_library(&theirs);
    let (mut s, _) = open(&lib);
    let last = s.last_backup.clone();
    s.execute("library.backup", &json!({"background": true})).unwrap();
    finished(&s);
    assert!(s.backup.as_ref().unwrap().outcome().is_some_and(|o| o.is_ok()));

    let away = put_in_its_place(&lib, &theirs);
    let (theirs, ours) = (tree(&lib), tree(&away));
    note_finished(&mut s);
    assert_eq!(s.last_backup, last, "the time of the backup that was made is not noted for the folder at the path now");
    assert_eq!(tree(&lib), theirs, "the folder at the path now was not written to");
    assert_eq!(tree(&away), ours);
    drop(s);
    let _ = std::fs::remove_dir_all(&base);
}

/// Given a library that keeps its backups for 30 days and whose `Replaced` and shared backup folder
/// hold what is old enough to go, and another folder at its path whose own `Replaced` has folders of
/// the same names, when old backups are removed (`library.pruneBackups`), then it is refused with the
/// reason, and nothing is removed from any of them; with the folder that was opened back, they go.
#[test]
fn pruning_is_refused_while_the_librarys_path_leads_to_another_folder() {
    let base = scratch("prune");
    let (lib, theirs, shared) = (base.join("Lib"), base.join("Theirs"), base.join("Shared"));
    let times = ["2026-01-10T09:00:00", "2026-01-11T09:00:00", "2026-01-12T09:00:00", "2026-01-13T09:00:00", "2026-01-14T09:00:00"];
    library_restored_at(&lib, &times);
    library_restored_at(&theirs, &times);
    let (mut s, now) = open(&lib);
    s.execute("library.backupPreferences", &json!({"dir": shared.to_string_lossy()})).unwrap();
    for time in times {
        *now.lock().unwrap() = time.to_string();
        s.execute("library.backup", &json!({})).unwrap();
    }
    s.persist_if_dirty();
    *now.lock().unwrap() = "2027-01-01T09:00:00".to_string();
    let away = put_in_its_place(&lib, &theirs);
    let (theirs, ours, kept) = (tree(&lib), tree(&away), tree(&shared));

    let asked = json!({"keepDays": 30, "remove": true});
    let e = s.execute("library.pruneBackups", &asked).expect_err("nothing is removed in another folder").to_string();
    assert!(e.contains(REFUSED) && e.contains(lib.to_string_lossy().as_ref()), "{e}");
    assert_eq!(tree(&lib), theirs, "what the other folder's restores replaced is untouched");
    assert_eq!(tree(&away), ours);
    assert_eq!(tree(&shared), kept, "and so are the library's backups");
    assert!(s.activity.list().is_empty());

    put_back(&lib, &away);
    let r = s.execute("library.pruneBackups", &asked).unwrap();
    assert_eq!(r["replaced"]["removed"].as_array().map(Vec::len), Some(2), "what was refused is what would have gone: {r}");
    assert_eq!(r["removed"].as_array().map(Vec::len), Some(2), "{r}");
    drop(s);
    let _ = std::fs::remove_dir_all(&base);
}

/// What removes what is old after a backup (and what `library.pruneBackups` goes through) asks before
/// each folder it removes, as the removal of a long list can outlast a folder's being replaced: with
/// the library's path leading to another folder, `Replaced` and the backup folder lose nothing.
#[test]
fn what_removes_old_backups_asks_before_each_folder() {
    let base = scratch("removing");
    let (lib, theirs, shared) = (base.join("Lib"), base.join("Theirs"), base.join("Shared"));
    let times = ["2026-01-10T09:00:00", "2026-01-11T09:00:00", "2026-01-12T09:00:00", "2026-01-13T09:00:00", "2026-01-14T09:00:00"];
    library_restored_at(&lib, &times);
    library_restored_at(&theirs, &times);
    let (mut s, now) = open(&lib);
    s.execute("library.backupPreferences", &json!({"dir": shared.to_string_lossy()})).unwrap();
    for time in times {
        *now.lock().unwrap() = time.to_string();
        s.execute("library.backup", &json!({})).unwrap();
    }
    let id = s.library_id.clone().unwrap();
    let own = Own { library: &lib, id: &id };
    let (before, now) = ("2026-02-01T09:00:00", "2027-01-01T09:00:00");
    assert_eq!(backup::outdated(&shared, own, before, now).len(), 2, "two of the five are old enough, and not the newest three");
    let old: Vec<restore::Replaced> = restore::replaced(&lib, &crate::library::SETTINGS_FILES).into_iter().take(2).collect();
    assert_eq!(old.len(), 2);

    let away = put_in_its_place(&lib, &theirs);
    let (theirs, ours, kept) = (tree(&lib), tree(&away), tree(&shared));
    let (removed, not_removed) = remove_replaced(&lib, &old, 30, s.opened_folder(), &|| false, &|_| {});
    assert!(removed.is_empty() && not_removed.len() == 1 && not_removed[0].contains(REFUSED), "{removed:?} {not_removed:?}");
    let (removed, not_removed) = remove_outdated(&shared, own, now, 30, None, s.opened_folder(), &|| false);
    assert!(removed.is_empty() && not_removed.len() == 1 && not_removed[0].contains(REFUSED), "{removed:?} {not_removed:?}");
    assert_eq!(tree(&lib), theirs);
    assert_eq!(tree(&away), ours);
    assert_eq!(tree(&shared), kept);

    // with the folder that was opened back, both remove what they are given
    put_back(&lib, &away);
    let (removed, not_removed) = remove_replaced(&lib, &old, 30, s.opened_folder(), &|| false, &|_| {});
    assert_eq!((removed.len(), not_removed.len()), (2, 0), "{removed:?} {not_removed:?}");
    let (removed, not_removed) = remove_outdated(&shared, own, now, 30, None, s.opened_folder(), &|| false);
    assert_eq!((removed.len(), not_removed.len()), (2, 0), "{removed:?} {not_removed:?}");
    drop(s);
    let _ = std::fs::remove_dir_all(&base);
}

/// What a backup reads its files through asks the folder before each one, and says why it stopped; a
/// stop that somebody asked for is still one.
#[test]
fn the_watch_of_a_backup_stops_it_once_the_path_leads_to_another_folder() {
    struct Asked(bool);
    impl Watch for Asked {
        fn cancelled(&self) -> bool {
            self.0
        }
    }
    let base = scratch("watch");
    let (lib, theirs) = (base.join("Lib"), base.join("Theirs"));
    library_restored_at(&lib, &[]);
    another_library(&theirs);
    let (s, _) = open(&lib);
    let (quiet, stopped) = (Asked(false), Asked(true));
    let gate = InOpenedFolder::new(&quiet, s.opened_folder());
    assert!(!gate.cancelled() && gate.refusal().is_none());
    assert!(InOpenedFolder::new(&stopped, s.opened_folder()).cancelled(), "a ✕ stops it");

    let away = put_in_its_place(&lib, &theirs);
    assert!(gate.cancelled());
    assert!(gate.refusal().is_some_and(|why| why.contains(REFUSED)));
    let asked_for = InOpenedFolder::new(&stopped, s.opened_folder());
    assert!(asked_for.cancelled() && asked_for.refusal().is_none(), "the stop that was asked for is not the folder's");
    put_back(&lib, &away);
    drop(s);
    let _ = std::fs::remove_dir_all(&base);
}
