//! The folders: where the copy may go, what of the library's folder is copied beside the catalog, and what is left out.

use std::fs::{self, FileType};
use std::io;
use std::path::{Path, PathBuf};

use super::{ConvertError, Options, Report, Target};
use crate::journal::{LOG, SNAPSHOT};
use crate::lock::{LOCK, OWNER};
use crate::safe_file::same_file_strict;
use crate::segments::SEGMENT_DIR;

/// How deep a folder of the library is followed when it is copied (the deepest a library has is a few levels, and a
/// loop would be one of links, which are not followed).
const MAX_DEPTH: usize = 64;

fn refused(what: String) -> ConvertError {
    ConvertError::Refused(what)
}

fn io_error(what: &str, path: &Path, e: &io::Error) -> ConvertError {
    ConvertError::Io(format!("{what} {}: {e}", path.display()))
}

// ---- where the copy goes

/// Where `out` is, if the copy may go there: a place that is not in the library, not even by another name for it, and has
/// nothing in it. Nothing is created. (The folder is compared with the library by identity: file system and file number,
/// as a case-insensitive volume or a link makes the same folder have more than one spelling.)
pub(super) fn check_out(library: &Path, out: &Path) -> Result<PathBuf, ConvertError> {
    let library = fs::canonicalize(library).map_err(|e| refused(format!("can't resolve the library {}: {e}", library.display())))?;
    let wanted = std::path::absolute(out).map_err(|e| refused(format!("can't make {} absolute: {e}", out.display())))?;
    let out = resolve(&wanted)?;
    refuse_inside(&library, &out, "the copy goes in a new folder elsewhere")?;
    match fs::symlink_metadata(&out) {
        Ok(m) if m.is_dir() => {
            let mut entries = fs::read_dir(&out).map_err(|e| refused(format!("can't list {}: {e}", out.display())))?;
            if entries.next().is_some() {
                return Err(refused(format!("{} is not empty: the copy goes in a new (or an empty) folder", out.display())));
            }
        }
        Ok(_) => return Err(refused(format!("{} exists and is not a folder", out.display()))),
        Err(e) if e.kind() == io::ErrorKind::NotFound => {}
        Err(e) => return Err(refused(format!("can't look at {}: {e}", out.display()))),
    }
    Ok(out)
}

/// Where `out` is, if a file may be written there: not in the library (by any name for it) and not existing yet. Nothing is
/// created. Used by the census, which never writes over anything.
pub(super) fn check_file(library: &Path, out: &Path) -> Result<PathBuf, ConvertError> {
    let library = fs::canonicalize(library).map_err(|e| refused(format!("can't resolve the library {}: {e}", library.display())))?;
    let wanted = std::path::absolute(out).map_err(|e| refused(format!("can't make {} absolute: {e}", out.display())))?;
    let out = resolve(&wanted)?;
    refuse_inside(&library, &out, "the census goes in a file elsewhere")?;
    match fs::symlink_metadata(&out) {
        Ok(_) => Err(refused(format!("{} exists: a census is never written over anything", out.display()))),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(out),
        Err(e) => Err(refused(format!("can't look at {}: {e}", out.display()))),
    }
}

/// Refuse an `out` that is the (canonical) `library` or lies in it, however it is spelled.
fn refuse_inside(library: &Path, out: &Path, goes: &str) -> Result<(), ConvertError> {
    for at in out.ancestors() {
        match same_file_strict(at, library) {
            Ok(false) if at != library => {}
            Ok(_) => {
                return Err(refused(format!("{} is in the library {}: {goes}", out.display(), library.display())));
            }
            Err(e) => return Err(refused(format!("can't tell whether {} is the library or in it ({e}): refused", at.display()))),
        }
    }
    Ok(())
}

/// `path` with every link on the part of it that exists resolved, and the part that doesn't yet exist plain names (a
/// `..` in it could lead anywhere once the folders are made).
fn resolve(path: &Path) -> Result<PathBuf, ConvertError> {
    let mut tail = Vec::new();
    let mut at = path;
    loop {
        match fs::symlink_metadata(at) {
            Ok(_) => break,
            Err(e) if e.kind() == io::ErrorKind::NotFound => match (at.parent(), at.file_name()) {
                (Some(parent), Some(name)) => {
                    tail.push(name);
                    at = parent;
                }
                _ => {
                    return Err(refused(format!(
                        "{} is not a place a copy can go (it has a `..` in a folder that doesn't exist yet)",
                        path.display()
                    )));
                }
            },
            Err(e) => return Err(refused(format!("can't look at {}: {e}", at.display()))),
        }
    }
    let mut resolved = fs::canonicalize(at).map_err(|e| refused(format!("can't resolve {}: {e}", at.display())))?;
    for name in tail.into_iter().rev() {
        resolved.push(name);
    }
    Ok(resolved)
}

/// The folder the copy is made in, beside `out`: the copy is moved to `out` once it is complete and has been read back,
/// and is removed if anything fails before that.
pub(super) struct Staging {
    path: PathBuf,
    kept: bool,
}

impl Staging {
    pub(super) fn create(out: &Path) -> Result<Staging, ConvertError> {
        let (Some(parent), Some(name)) = (out.parent(), out.file_name()) else {
            return Err(refused(format!("{} is not a place a copy can go", out.display())));
        };
        fs::create_dir_all(parent).map_err(|e| io_error("can't create", parent, &e))?;
        let path = parent.join(format!(".{}.converting-{}", name.to_string_lossy(), std::process::id()));
        // (not create_dir_all: a folder of that name is not ours to use, or to remove)
        fs::create_dir(&path).map_err(|e| io_error("can't create", &path, &e))?;
        Ok(Staging { path, kept: false })
    }

    pub(super) fn path(&self) -> &Path {
        &self.path
    }

    /// Move the copy to `out` (replacing the empty folder that may be there).
    pub(super) fn commit(mut self, out: &Path) -> Result<(), ConvertError> {
        if fs::symlink_metadata(out).is_ok() {
            fs::remove_dir(out).map_err(|e| io_error("can't replace the empty folder", out, &e))?;
        }
        if let Err(e) = fs::rename(&self.path, out) {
            // the finished copy stays where it is, and the error says so
            self.kept = true;
            return Err(ConvertError::Io(format!(
                "the copy is complete but can't be moved to {}: {e}; it is in {}",
                out.display(),
                self.path.display()
            )));
        }
        self.kept = true;
        Ok(())
    }
}

impl Drop for Staging {
    fn drop(&mut self) {
        if !self.kept {
            let _ = fs::remove_dir_all(&self.path);
        }
    }
}

// ---- what is in the library's folder

/// What becomes of a file or folder of the library's folder.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum Action {
    /// A catalog file: the copy has its own, written for the target (how).
    Catalog(&'static str),
    /// Copied as it is.
    Copy,
    /// Not copied (why).
    Leave(&'static str),
}

pub(super) struct Item {
    pub name: String,
    pub dir: bool,
    pub action: Action,
}

/// What the library's folder holds, and what is done with each thing. Reads the folder, nothing else.
pub(super) fn survey(library: &Path, target: Target, options: Options) -> Result<Vec<Item>, ConvertError> {
    let mut items = Vec::new();
    for entry in fs::read_dir(library).map_err(|e| io_error("can't list", library, &e))? {
        let entry = entry.map_err(|e| io_error("can't list", library, &e))?;
        let kind = entry.file_type().map_err(|e| io_error("can't look at", &entry.path(), &e))?;
        let name = entry.file_name().to_string_lossy().into_owned();
        items.push(Item { action: classify(&name, kind, target, options), dir: kind.is_dir(), name });
    }
    items.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(items)
}

/// Where each file the library holds goes. A name this tool doesn't know is left out (and listed), not guessed at.
fn classify(name: &str, kind: FileType, target: Target, options: Options) -> Action {
    if kind.is_symlink() {
        return Action::Leave("a link: not followed (the copy would share what it points to with the library)");
    }
    match (name, kind.is_dir()) {
        (SNAPSHOT, false) => Action::Catalog("written again in the target's format"),
        (LOG, false) => Action::Catalog("written empty: the copy's catalog.snap holds every change"),
        (SEGMENT_DIR, true) => match target {
            Target::Official => Action::Leave("official LightCraft has no segments: the photos are in the copy's catalog.snap"),
            Target::Build7248 => Action::Catalog("the photos written again as the copy's own segments"),
        },
        ("presets.json" | "view.json" | "prefs.json", false) => Action::Copy,
        ("thumbs" | "Smart Previews" | "Originals" | "Interop", true) if options.catalog_only => Action::Leave("--catalog-only"),
        ("thumbs" | "Smart Previews" | "Originals" | "Interop", true) => Action::Copy,
        (LOCK | OWNER, false) => Action::Leave("who has the library open: a copy has its own"),
        ("Backups" | "Replaced", true) | ("backup.json", false) => {
            Action::Leave("kept by a build for its own use; neither the catalog nor a setting")
        }
        _ => Action::Leave(unknown(name)),
    }
}

/// Why a file this tool has no place for is left out: the files a build sets aside are named, so that the list says what they are.
fn unknown(name: &str) -> &'static str {
    let set_aside = |prefix: &str| name.starts_with(prefix);
    if set_aside("catalog.snap.v") || set_aside("catalog.log.v") {
        "an older format's copy that an upgrade kept"
    } else if set_aside("catalog.log.damaged-") {
        "a damaged log that an open set aside"
    } else if name.ends_with(".tmp") || name.contains(".converting-") {
        "half written by a program that didn't finish"
    } else if name.contains(".corrupt-") {
        "a settings file that could not be read, set aside by a build"
    } else {
        "not a file this tool knows: not copied (it is still in the library)"
    }
}

/// Put what the survey found in the report.
pub(super) fn describe(items: &[Item], report: &mut Report) {
    for item in items {
        let name = if item.dir { format!("{}/", item.name) } else { item.name.clone() };
        match &item.action {
            Action::Catalog(how) => report.rewritten.push(format!("{name}: {how}")),
            Action::Copy => report.copied.push(name),
            Action::Leave(why) => report.left_out.push(format!("{name}: {why}")),
        }
    }
}

// ---- copying

#[derive(Default)]
struct Tally {
    files: u64,
    bytes: u64,
    /// What a folder holds that is neither a file nor a folder (a link, a socket): not copied.
    skipped: Vec<String>,
}

/// Copy what the survey says to copy, from the library into `staging`, and say in `report` how much.
pub(super) fn copy(library: &Path, staging: &Path, items: &[Item], report: &mut Report) -> Result<(), ConvertError> {
    report.copied.clear();
    for item in items.iter().filter(|i| i.action == Action::Copy) {
        let (from, to) = (library.join(&item.name), staging.join(&item.name));
        let mut tally = Tally::default();
        if item.dir {
            copy_dir(&from, &to, &item.name, 0, &mut tally)?;
        } else {
            copy_file(&from, &to, &mut tally)?;
        }
        let name = if item.dir { format!("{}/", item.name) } else { item.name.clone() };
        report.copied.push(format!("{name}: {} file(s), {} bytes", tally.files, tally.bytes));
        report.left_out.extend(tally.skipped.into_iter().map(|s| format!("{s}: a link or another kind of file: not copied")));
    }
    Ok(())
}

fn copy_dir(from: &Path, to: &Path, shown: &str, depth: usize, tally: &mut Tally) -> Result<(), ConvertError> {
    if depth > MAX_DEPTH {
        return Err(ConvertError::Io(format!("{} is nested deeper than {MAX_DEPTH} folders: refused", from.display())));
    }
    fs::create_dir(to).map_err(|e| io_error("can't create", to, &e))?;
    let mut entries = Vec::new();
    for entry in fs::read_dir(from).map_err(|e| io_error("can't list", from, &e))? {
        entries.push(entry.map_err(|e| io_error("can't list", from, &e))?);
    }
    entries.sort_by_key(|e| e.file_name());
    for entry in entries {
        let kind = entry.file_type().map_err(|e| io_error("can't look at", &entry.path(), &e))?;
        let (src, dst) = (entry.path(), to.join(entry.file_name()));
        let here = format!("{shown}/{}", entry.file_name().to_string_lossy());
        if kind.is_dir() {
            copy_dir(&src, &dst, &here, depth + 1, tally)?;
        } else if kind.is_file() {
            copy_file(&src, &dst, tally)?;
        } else {
            tally.skipped.push(here);
        }
    }
    Ok(())
}

fn copy_file(from: &Path, to: &Path, tally: &mut Tally) -> Result<(), ConvertError> {
    let bytes = fs::copy(from, to).map_err(|e| io_error("can't copy", from, &e))?;
    // (the file's date is part of it; where it can't be set the copy still is the file)
    if let Ok(modified) = fs::metadata(from).and_then(|m| m.modified())
        && let Ok(file) = fs::OpenOptions::new().write(true).open(to)
    {
        let _ = file.set_modified(modified);
    }
    tally.files += 1;
    tally.bytes += bytes;
    Ok(())
}
