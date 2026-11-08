//! What an import (or the review before it) learned of the folders it listed: the files in them
//! and the `.xmp` names.
//!
//! Looking for a photo's sidecar asks the disk about up to nine names ([`sidecar_candidates`]).
//! On a network share a name that is not there costs a round trip as much as one that is, so a
//! folder with no `.xmp` at all cost nine for each of its photos. Telling a file from a folder
//! (`is_dir`) is a round trip too, for each of the thousands of files a review hands over. The
//! listing that found those files already has both answers.
use std::collections::{HashMap, HashSet};
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::SystemTime;

use crate::sidecar::{SidecarLookup, SidecarNaming, find_sidecar, sidecar_candidates};
use crate::walk::EntryKind;

/// One listed folder.
#[derive(Clone, Debug, Default)]
struct Folder {
    /// The names listed as files (links are not: the disk says where they lead).
    files: HashSet<String>,
    /// The `.xmp` names, lower-cased. Only for a folder that was listed completely, with plain
    /// ASCII names (the names a file system treats alike in other cases are the ones we can fold
    /// ourselves): for any other the disk decides, as before.
    xmp: Option<HashSet<String>>,
    /// The folder's modification time when it was listed: a file added, removed or replaced since
    /// changes it, and the `.xmp` and file names are then not trusted (a listing taken by a review
    /// can be hours old).
    modified: Option<SystemTime>,
}

/// The folders listed so far (see the module docs). The review's scan hands its listing to the
/// import that review starts ([`crate::import::ImportJob::with_listed`]), and nowhere else.
/// The `.xmp` names and the file names of a folder are trusted only while its modification time
/// is what it was when it was listed (looked at once per folder per batch: [`ListedFolders::new_batch`]); a
/// volume whose folders keep no usable time (it can't be read) is probed as before.
#[derive(Clone, Debug, Default)]
pub struct ListedFolders {
    // (shared, not copied, by the readers of a batch: they look sidecars up on threads of their own, see `import::reads`)
    folders: Arc<HashMap<PathBuf, Folder>>,
    /// folder → whether its time is still the listed one, for the batch being handled.
    checked: Arc<Mutex<HashMap<PathBuf, Arc<OnceLock<bool>>>>>,
    /// How often the disk was asked after all about a sidecar (for the tests).
    pub(crate) asked: Arc<AtomicUsize>,
    /// How often the disk was asked about a file's own sidecar (`<file>.xmp`), which a file that may not read the shared
    /// one reads, after the lookup that found the shared one (for the tests: `asked` counts the lookups of the
    /// library's naming only).
    pub(crate) asked_own: Arc<AtomicUsize>,
    /// How many folders' times were looked at (for the tests).
    pub(crate) looked: Arc<AtomicUsize>,
    /// The threads that looked a sidecar up for an import (for the tests: the lookups of a batch are
    /// not made one after another by the thread that plans it).
    #[cfg(test)]
    pub(crate) lookers: Arc<Mutex<HashSet<std::thread::ThreadId>>>,
}

impl ListedFolders {
    /// Take the listing of `folder` from the walk ([`crate::walk::files_in_noting`]), with the
    /// folder's time as the walk read it before listing (`None`: too recent to vouch for, or unreadable).
    pub(crate) fn note(&mut self, folder: &Path, entries: Vec<(OsString, EntryKind)>, complete: bool, modified: Option<SystemTime>) {
        let mut listed = Folder { modified, ..Default::default() };
        let mut xmp = (complete && listed.modified.is_some()).then(HashSet::new);
        for (name, kind) in entries {
            // (a name that is no text is not one a path can be compared with)
            let Some(name) = name.to_str() else {
                xmp = None;
                continue;
            };
            if let Some(set) = &mut xmp {
                if name.is_ascii() {
                    let lower = name.to_ascii_lowercase();
                    if lower.ends_with(".xmp") {
                        set.insert(lower);
                    }
                } else {
                    // (not ours to say what another name's other case is)
                    xmp = None;
                }
            }
            if kind == EntryKind::File && crate::import::is_supported(Path::new(name)) {
                listed.files.insert(name.to_string());
            }
        }
        listed.xmp = xmp;
        Arc::make_mut(&mut self.folders).insert(folder.to_path_buf(), listed);
    }

    /// A new batch of files is handled: look at the folders' times again. A folder's time is looked at
    /// once per batch, by the first lookup in it (a look for each photo would cost the round trip each
    /// that the listing saves), so a sidecar added there while the batch is handled is not seen until
    /// the next batch.
    pub(crate) fn new_batch(&self) {
        self.checked.lock().unwrap_or_else(|e| e.into_inner()).clear();
    }

    /// The names of files are needed to take a path for a file (once the files are expanded, no more).
    pub(crate) fn forget_files(&mut self) {
        for f in Arc::make_mut(&mut self.folders).values_mut() {
            f.files = HashSet::new();
        }
    }

    /// Whether `dir` is as it was listed.
    fn unchanged(&self, dir: &Path, listed: Option<SystemTime>) -> bool {
        // (no time to compare with: not trusted, and the disk is not asked)
        let Some(listed) = listed else { return false };
        // (the map's lock is held only to find the folder's cell: the time is read inside the cell, so a folder on a
        // slow share does not hold up the lookups in the others, and each folder is still looked at once)
        let cell = Arc::clone(self.checked.lock().unwrap_or_else(|e| e.into_inner()).entry(dir.to_path_buf()).or_default());
        *cell.get_or_init(|| {
            self.looked.fetch_add(1, Ordering::Relaxed);
            let now = std::fs::metadata(dir).and_then(|m| m.modified()).ok();
            now == Some(listed)
        })
    }

    /// Take over `walked`'s folders, which are newer than the ones held for them.
    pub(crate) fn absorb(&mut self, walked: ListedFolders) {
        let walked = Arc::try_unwrap(walked.folders).unwrap_or_else(|shared| (*shared).clone());
        Arc::make_mut(&mut self.folders).extend(walked);
    }

    /// Whether a listing says `path` is a file, as it was found: the disk need not be asked.
    /// Only while its folder is as it was listed (a file may have been replaced by a folder since;
    /// a folder with no usable time is never trusted).
    pub(crate) fn is_known_file(&self, path: &Path) -> bool {
        let (Some(dir), Some(name)) = (path.parent(), path.file_name().and_then(|n| n.to_str())) else { return false };
        // (the path ends in that name: no `/.` or separator after it, which the disk would resolve)
        path.to_str().is_some_and(|p| p.ends_with(name))
            && self.folders.get(dir).is_some_and(|f| f.files.contains(name) && self.unchanged(dir, f.modified))
    }

    /// The `.xmp` names of `original`'s folder as the listing has them, where it can vouch for them: the folder was
    /// listed completely, is as it was, and `original`'s name is one the listing and the disk read alike. `None`
    /// where it can't say.
    fn trusted_xmp(&self, original: &str) -> Option<&HashSet<String>> {
        let path = Path::new(original);
        let dir = path.parent()?;
        let folder = self.folders.get(dir)?;
        let xmp = folder.xmp.as_ref()?;
        let name = path.file_name().and_then(|n| n.to_str())?;
        if !original.ends_with(name) || crate::sync::asks_the_disk(name) || !crate::sync::plain_name(name) || !self.unchanged(dir, folder.modified) {
            return None;
        }
        Some(xmp)
    }

    /// Whether the listing shows that sidecar name `candidate` is not there (`false` where it can't say: a name the
    /// disk may resolve another way).
    fn is_absent(xmp: &HashSet<String>, candidate: &Path) -> bool {
        candidate.file_name().and_then(|n| n.to_str()).is_some_and(|n| crate::sync::plain_name(n) && !xmp.contains(&n.to_ascii_lowercase()))
    }

    /// Whether the listing of `original`'s folder shows that none of its sidecar names is there.
    /// `false` where it can't say: the folder was not listed (or not completely), or a name is
    /// one the disk may resolve another way.
    fn rules_out(&self, original: &str, naming: SidecarNaming) -> bool {
        let Some(xmp) = self.trusted_xmp(original) else { return false };
        sidecar_candidates(original, naming).all(|c| Self::is_absent(xmp, &c))
    }

    /// [`ListedFolders::find`] for a file an import is adding, made by the readers: what the library's naming finds,
    /// which a file that owns its stem sidecar ([`crate::sidecar::ImportStems`]) reads (one that doesn't reads its own
    /// `<file>.xmp`, [`ListedFolders::find_own_for_import`]).
    pub(crate) fn find_for_import(&self, original: &str, naming: SidecarNaming) -> Option<PathBuf> {
        #[cfg(test)]
        self.lookers.lock().unwrap_or_else(|e| e.into_inner()).insert(std::thread::current().id());
        self.find(original, naming)
    }

    /// The sidecar a file that does not own its stem's sidecar reads (issue #1905): its own `<file>.xmp`, the name it
    /// writes, never the packet of the sibling that owns the shared one ([`SidecarLookup::find`] without the shared
    /// names). Made by the readers, once the one found for the file proved to be under the shared name: the planning
    /// only chooses between what they found. Like [`ListedFolders::find`], it asks the disk only for the names the
    /// listing does not show absent.
    pub(crate) fn find_own_for_import(&self, original: &str, naming: SidecarNaming) -> Option<PathBuf> {
        #[cfg(test)]
        self.lookers.lock().unwrap_or_else(|e| e.into_inner()).insert(std::thread::current().id());
        let listed = self.trusted_xmp(original);
        let mut asked = false;
        let found = SidecarLookup { naming, owns_stem: false }
            .candidates(original)
            .filter(|c| !listed.is_some_and(|xmp| Self::is_absent(xmp, c)))
            .find(|c| {
                asked = true;
                c.is_file()
            });
        if asked {
            self.asked_own.fetch_add(1, Ordering::Relaxed);
        }
        found
    }

    /// [`find_sidecar`], without asking the disk where the listing shows there is none.
    pub(crate) fn find(&self, original: &str, naming: SidecarNaming) -> Option<PathBuf> {
        if self.rules_out(original, naming) {
            return None;
        }
        self.asked.fetch_add(1, Ordering::Relaxed);
        find_sidecar(original, naming)
    }
}
