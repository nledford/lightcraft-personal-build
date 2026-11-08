use std::collections::HashMap;

#[cfg(not(target_arch = "wasm32"))]
use super::commit::workers;
#[cfg(test)]
use super::job::{ImportJob, Prepared, PreparedItem, ReadyFile};
use super::scan::ScanProgress;
#[cfg(test)]
use super::types::{ImportMode, ImportOptions};
use super::{Path, PathBuf, ProbeInfo, ReadWatch, Session, reads};

/// File extensions offered by import discovery and file pickers (lower case).
/// This includes preview-only raws; the file probe decides whether sensor decoding is available.
pub const EXTENSIONS: &[&str] = &[
    "jpg", "jpeg", "png", "tif", "tiff", "webp", "dng", "cr2", "cr3", "nef", "nrw", "arw", "sr2", "srf", "raf", "orf", "rw2", "rwl", "raw", "pef",
    "srw", "psd", "psb", "jxl", "gif", "bmp",
    // Decoded when built with the codecs' `heif` feature; otherwise the import reports them as
    // failed with the reason ("HEIC/HEIF support isn't included in this build").
    "heic", "heif", "avif", // no AV1 decoder yet: reported as failed, like a HEIC without the feature
    "iiq", "crw", "mrw", "x3f", "kdc", "mos", "erf", "3fr", "fff", "mef",
];

pub fn is_supported(path: &Path) -> bool {
    path.extension().is_some_and(|e| EXTENSIONS.contains(&e.to_string_lossy().to_lowercase().as_str()))
}

/// An XMP sidecar (`.xmp`, any letter case): read with its photo on import, never a photo itself.
fn is_xmp_sidecar(path: &Path) -> bool {
    path.extension().is_some_and(|e| e.eq_ignore_ascii_case("xmp"))
}

/// The web file picker's accept list, derived from [`EXTENSIONS`]. Each extension
/// is listed in both cases: browsers' `accept` matching is case-insensitive per
/// spec, but some implementations (e.g. Windows/Edge) match case-sensitively and
/// rejected uppercase `.NEF` files, so a lowercase-only list silently hid raws
/// such as CR3, NRW, RAF, ORF, RW2, RWL, RAW, PEF and SRW from the picker.
///
/// The picker only offers files this build can actually import: `.avif` has no
/// AV1 decoder in any build, and `.heic`/`.heif` decode only when the codecs'
/// `heif` feature is enabled ([`lightcraft_codecs::Format::Heif::can_decode`]).
/// [`EXTENSIONS`] itself stays complete — files on disk are still recognised,
/// and import reports them as failed, saying why.
pub fn accept_list() -> String {
    let heif_decodable = lightcraft_codecs::Format::Heif.can_decode();
    let mut out = String::new();
    for &ext in EXTENSIONS {
        // Skip extensions a web user could pick but this build could not import.
        let pickable = match ext {
            "avif" => false,
            "heic" | "heif" => heif_decodable,
            _ => true,
        };
        if !pickable {
            continue;
        }
        if !out.is_empty() {
            out.push(',');
        }
        out.push('.');
        out.push_str(ext);
        out.push(',');
        out.push('.');
        out.push_str(&ext.to_uppercase());
    }
    out
}

/// Expand files and folders (recursively) into supported files. `skip` (e.g. the library folder)
/// is never descended into. Each folder's walk is bounded ([`crate::walk::Limits::default`]); a
/// walk that stops at a bound is logged.
pub fn expand(paths: &[String], skip: Option<&Path>) -> Vec<String> {
    expand_within(paths, skip, crate::walk::Limits::default()).0
}

/// [`expand`] with the walk bounded by `limits` → (files, whether a walk stopped at a bound).
pub fn expand_within(paths: &[String], skip: Option<&Path>, limits: crate::walk::Limits) -> (Vec<String>, bool) {
    expand_noting(paths, skip, limits, None)
}

/// [`expand_within`], handing the listing of every folder that is walked to `note`
/// ([`crate::walk::files_in_noting`]).
pub(crate) fn expand_noting(
    paths: &[String],
    skip: Option<&Path>,
    limits: crate::walk::Limits,
    note: Option<crate::walk::NoteFolder>,
) -> (Vec<String>, bool) {
    let e = expand_report_noting(paths, skip, limits, note);
    (e.files, e.truncated)
}

/// What expanding import sources found, and what the walks left out ([`expand_report`]), so a
/// review can say that it did not see everything.
#[derive(Clone, Debug, Default)]
pub struct Expansion {
    /// Supported files, folders walked recursively and deduplicated by path, in walk order.
    pub files: Vec<String>,
    /// A walk stopped at a depth or entry bound: there may be more files.
    pub truncated: bool,
    /// Folders whose contents couldn't be listed (permissions, a folder that vanished or went offline).
    pub unreadable: usize,
    /// Files found inside the folders walked that are not a supported type (see [`is_supported`]),
    /// not counting XMP sidecars, which the import reads with their photos. A file named directly
    /// is attempted whatever its extension and is never counted here. Both counts add up over the
    /// sources, so a folder named twice (or inside another source) counts twice.
    pub unsupported: usize,
}

/// [`expand_within`] keeping the walks' diagnostics. A walk that stops at a bound is logged here
/// (once per folder), as it always was.
pub fn expand_report(paths: &[String], skip: Option<&Path>, limits: crate::walk::Limits) -> Expansion {
    expand_report_noting(paths, skip, limits, None)
}

/// ([`expand_report`], handing the listing of every folder that is walked to `note`, as
/// [`expand_noting`] does.)
fn expand_report_noting(paths: &[String], skip: Option<&Path>, limits: crate::walk::Limits, note: Option<crate::walk::NoteFolder>) -> Expansion {
    expand_report_knowing(paths, skip, limits, note, false, &|_| false)
}

/// [`expand_noting`], taking the paths `known_file` says are files as files without asking the
/// disk (a review hands over thousands of them, and telling a file from a folder is a round trip
/// each on a network share). With `timed`, each folder's modification time goes to `note` too
/// ([`crate::walk::files_in_noting_timed`]).
pub(super) fn expand_knowing(
    paths: &[String],
    skip: Option<&Path>,
    limits: crate::walk::Limits,
    note: Option<crate::walk::NoteFolder>,
    timed: bool,
    known_file: &dyn Fn(&Path) -> bool,
) -> (Vec<String>, bool) {
    let e = expand_report_knowing(paths, skip, limits, note, timed, known_file);
    (e.files, e.truncated)
}

/// [`expand_knowing`] keeping the walks' diagnostics ([`Expansion`]).
pub(super) fn expand_report_knowing(
    paths: &[String],
    skip: Option<&Path>,
    limits: crate::walk::Limits,
    mut note: Option<crate::walk::NoteFolder>,
    timed: bool,
    known_file: &dyn Fn(&Path) -> bool,
) -> Expansion {
    let mut out = Expansion::default();
    for p in paths {
        let p = Path::new(p);
        if skip.is_some_and(|s| p == s) {
            continue;
        }
        if !known_file(p) && p.is_dir() {
            // XMP sidecars are read with their photos, not skipped: they are left out of the count
            let mut sidecars = 0usize;
            let keep = |f: &Path| {
                let keep = is_supported(f);
                if !keep && is_xmp_sidecar(f) {
                    sidecars = sidecars.saturating_add(1);
                }
                keep
            };
            let w = match note.as_mut() {
                Some(note) if timed => crate::walk::files_in_noting_timed(p, skip, limits, keep, &mut **note),
                Some(note) => crate::walk::files_in_noting(p, skip, limits, keep, &mut **note),
                None => crate::walk::files_in(p, skip, limits, keep),
            };
            if w.truncated {
                log::warn!(
                    "import: stopped looking in {} after {} entries (limits: {} entries, {} folder levels)",
                    p.display(),
                    w.entries,
                    limits.max_entries,
                    limits.max_depth
                );
            }
            out.truncated |= w.truncated;
            out.unreadable = out.unreadable.saturating_add(w.unreadable);
            out.unsupported = out.unsupported.saturating_add(w.skipped.saturating_sub(sidecars));
            out.files.extend(w.files.into_iter().map(|f| f.to_string_lossy().to_string()));
        } else {
            // explicitly named files are attempted even with an unknown extension (sniffed)
            out.files.push(p.to_string_lossy().to_string());
        }
    }
    let mut seen = std::collections::HashSet::new();
    out.files.retain(|p| seen.insert(p.clone()));
    out
}

/// What a path given on the command line at launch becomes ([`launch_path`]).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LaunchPath {
    /// Import this (absolute) file or folder, in the background.
    Import(String),
    /// Not imported: `path` (absolute) is a whole drive or the whole home folder, which is never
    /// walked without being chosen in the app (issue #374).
    Refused { path: String, why: String },
    /// Open this (absolute) library folder: the argument was a library's `catalog.snap` or its folder, as a
    /// double-click on a catalog in the file manager gives. Never imported.
    Library(String),
}

/// The library folder `path` names: a library's `catalog.snap` (its folder), or a folder holding one. `None` for
/// anything else (photos, folders of photos). The folder must pass the library opener's own test
/// ([`crate::library::is_library`]), so a `CATALOG.SNAP` the opener would not read (a case-sensitive file system)
/// never opens its folder as a new, empty library.
pub fn library_dir_of(path: &Path) -> Option<PathBuf> {
    let snapshot = lightcraft_catalog::journal::SNAPSHOT;
    let named = path.file_name().is_some_and(|n| n.to_string_lossy().eq_ignore_ascii_case(snapshot));
    if named && path.is_file() {
        return path.parent().filter(|dir| crate::library::is_library(dir)).map(Path::to_path_buf);
    }
    (path.is_dir() && crate::library::is_library(path)).then(|| path.to_path_buf())
}

/// How long the launch arguments' library checks may take, all of them together, before LightCraft gives up and
/// treats the arguments not yet checked as photos to import (in the background). A path on a dead network mount
/// can block a file-system query for minutes, and these checks run before the window opens.
pub const LAUNCH_PROBE_LIMIT: std::time::Duration = std::time::Duration::from_secs(2);

/// When the launch arguments' library checks, starting now, must be done: [`LAUNCH_PROBE_LIMIT`] from now.
pub fn launch_deadline() -> std::time::Instant {
    let now = std::time::Instant::now();
    now.checked_add(LAUNCH_PROBE_LIMIT).unwrap_or(now)
}

/// `probe`'s answer, or `None` when it takes longer than `limit`: it runs on its own thread, which is left to
/// finish (or stay blocked) on its own. Also `None` when no thread can be started.
pub(crate) fn within<T: Send + 'static>(limit: std::time::Duration, probe: impl FnOnce() -> Option<T> + Send + 'static) -> Option<T> {
    let (tx, rx) = std::sync::mpsc::channel();
    let started = std::thread::Builder::new().name("launch-probe".into()).spawn(move || {
        let _ = tx.send(probe());
    });
    if let Err(e) = started {
        log::warn!("could not start the launch-path check: {e}");
        return None;
    }
    match rx.recv_timeout(limit) {
        Ok(found) => found,
        Err(_) => {
            log::warn!("the launch-path check took longer than {limit:?}; treating the argument as photos to import");
            None
        }
    }
}

/// Resolve a launch argument for importing (issue #374). An empty argument is ignored (`None`);
/// a relative one (`.`, `..`, `photos`; `/` or `\` on Windows, the current drive's root) is
/// resolved against `cwd` — the working folder a launcher picked, often a drive root or
/// `C:\Windows\System32` — and refused when that leaves no folder (`cwd` unknown). A drive
/// or file-system root is replaced by its camera folder (`DCIM`, from `dcim`: a memory card) or
/// refused, and so is the home folder `home`: either would walk most of the disk.
pub fn launch_path(arg: &str, cwd: Option<&Path>, home: Option<&Path>, dcim: impl Fn(&Path) -> Option<PathBuf>) -> Option<LaunchPath> {
    launch_path_until(arg, cwd, home, dcim, launch_deadline())
}

/// [`launch_path`], with its library check bounded by `deadline` (from [`launch_deadline`]), which every argument
/// of one launch shares: several paths on a dead mount wait [`LAUNCH_PROBE_LIMIT`] in all, not each. Once it has
/// passed, an argument is imported without the check.
pub fn launch_path_until(
    arg: &str,
    cwd: Option<&Path>,
    home: Option<&Path>,
    dcim: impl Fn(&Path) -> Option<PathBuf>,
    deadline: std::time::Instant,
) -> Option<LaunchPath> {
    if arg.trim().is_empty() {
        return None;
    }
    let given = Path::new(arg);
    let joined = if given.is_absolute() {
        given.to_path_buf()
    } else {
        match cwd {
            Some(c) => c.join(given),
            None => return Some(LaunchPath::Refused { path: arg.to_string(), why: "a relative path, and the working folder is unknown".into() }),
        }
    };
    // `.` and `..` resolved by name (no file-system access)
    let mut abs = PathBuf::new();
    for c in joined.components() {
        match c {
            std::path::Component::CurDir => {}
            std::path::Component::ParentDir => {
                abs.pop();
            }
            other => abs.push(other.as_os_str()),
        }
    }
    let shown = abs.to_string_lossy().to_string();
    if abs.parent().is_none() {
        return Some(match dcim(&abs) {
            Some(d) => LaunchPath::Import(d.to_string_lossy().to_string()),
            None => LaunchPath::Refused { path: shown, why: "a whole drive".into() },
        });
    }
    let key = lightcraft_catalog::query::folder_key;
    if home.is_some_and(|h| key(&h.to_string_lossy()) == key(&shown)) {
        return Some(LaunchPath::Refused { path: shown, why: "the whole home folder".into() });
    }
    let left = deadline.saturating_duration_since(std::time::Instant::now());
    let probed = abs.clone();
    if !left.is_zero()
        && let Some(dir) = within(left, move || library_dir_of(&probed))
    {
        return Some(LaunchPath::Library(dir.to_string_lossy().to_string()));
    }
    Some(LaunchPath::Import(shown))
}

/// Probe files (headers + content hash) as an import would.
pub(crate) fn probe_paths(s: &Session, paths: &[String]) -> Vec<Result<ProbeInfo, String>> {
    let progress = ScanProgress::new(s.import_stall_after, s.stuck_readers.clone());
    probe_watched(s.media.file_probe.as_ref(), paths, &progress.done, &progress.cancel, &progress.reads)
}

/// Probe files with `probe` (a clone of the session's file probe), off the session: on a worker thread.
pub(crate) fn probe_with(probe: &crate::media::FileProbe, paths: &[String]) -> Vec<Result<ProbeInfo, String>> {
    let progress = ScanProgress::default();
    probe_watched(Some(probe), paths, &progress.done, &progress.cancel, &progress.reads)
}

/// Probe `paths` (several at once), counting each in `done`; Reload from Disk's own run. Stops before the next file
/// once `cancel` is set: the files not reached come back as errors. It watches no stall of its own, so nothing
/// shows or skips one; the reads still run on the pool's threads, and ✕ ends the run at once.
pub(crate) fn probe_all(
    probe: Option<&crate::media::FileProbe>,
    paths: &[String],
    done: &std::sync::atomic::AtomicUsize,
    cancel: &std::sync::atomic::AtomicBool,
) -> Vec<Result<ProbeInfo, String>> {
    probe_watched(probe, paths, done, cancel, &ReadWatch::default())
}

/// [`read_files`] for plain probes: nothing cached, no sidecar.
fn probe_watched(
    probe: Option<&crate::media::FileProbe>,
    paths: &[String],
    done: &std::sync::atomic::AtomicUsize,
    cancel: &std::sync::atomic::AtomicBool,
    watch: &ReadWatch,
) -> Vec<Result<ProbeInfo, String>> {
    let jobs = paths.iter().map(|p| ReadJob { path: p.clone(), cached: None }).collect();
    read_files(probe, jobs, |_, _| false, None, done, cancel, watch).into_iter().map(|r| r.info).collect()
}

/// What became of one job of [`pooled`].
#[cfg(not(target_arch = "wasm32"))]
pub(crate) enum Pooled<R> {
    Done(R),
    /// It was skipped, or its worker panicked: why.
    Failed(String),
    /// Never started, or cut off: by ✕, or because the run ended.
    Unreached,
}

/// Run `work` on each of `jobs` on the pool's threads (see [`reads`]), `label(i)` naming job `i` when it stalls;
/// `on_done` counts each job settled.
#[cfg(not(target_arch = "wasm32"))]
pub(crate) fn pooled<J: Send + 'static, R: Send + 'static>(
    watch: &ReadWatch,
    cancel: &std::sync::atomic::AtomicBool,
    name: &'static str,
    jobs: Vec<J>,
    label: &dyn Fn(usize) -> String,
    work: impl Fn(J) -> R + Send + Sync + 'static,
    on_done: &dyn Fn(),
) -> Vec<Pooled<R>> {
    let pool = reads::Pool { watch, cancel, name, threads: workers(jobs.len()) };
    let outcomes = pool.run(jobs, label, std::sync::Arc::new(work), std::sync::Arc::new(|_| {}), std::sync::Arc::new(|| {}), &|_| on_done());
    outcomes
        .into_iter()
        .map(|o| match o {
            reads::Outcome::Done(r) => Pooled::Done(r),
            reads::Outcome::Failed(why) => Pooled::Failed(why),
            reads::Outcome::Unreached => Pooled::Unreached,
        })
        .collect()
}

/// A file to read: its path, and what an earlier scan probed of it (used while `still_valid` says it holds).
pub(super) struct ReadJob {
    pub(super) path: String,
    pub(super) cached: Option<ProbeInfo>,
}

/// What reading a file gave: its probe (or why not) and the text of the XMP sidecars it may read, when asked for and
/// there are any.
pub(super) struct Read {
    pub(super) info: Result<ProbeInfo, String>,
    pub(super) sidecars: Sidecars,
    /// The run had ended before this file was started: it was never tried, so it is neither readied nor failed.
    pub(super) unreached: bool,
    /// The probe was a valid one from an earlier scan: nothing was read from the disk for it (the import does not count
    /// it among the files it read).
    pub(super) reused: bool,
}

/// A sidecar a file of an import may read: where it was found, and its text (`None`: it could not be read).
pub(super) struct Found {
    pub(super) path: PathBuf,
    pub(super) text: Option<String>,
}

/// The sidecar the readers found for a file of an import: what the library's naming finds
/// ([`crate::sidecar::find_sidecar`]), read by a file that owns its stem's sidecar. A file that does not own it reads
/// its own `<file>.xmp` instead ([`Found::own`], asked at the planning: who owns the shared stem sidecar among the
/// files of the import and the library, [`crate::sidecar::ImportStems`] issue #1905, is only known once every file is
/// probed).
#[derive(Default)]
pub(super) struct Sidecars {
    pub(super) owner: Option<Found>,
}

impl Sidecars {
    fn read(path: &str, naming: crate::sidecar::SidecarNaming, listed: &super::ListedFolders) -> Sidecars {
        let owner = listed.find_for_import(path, naming).map(|path| {
            let text = std::fs::read_to_string(&path).ok();
            Found { path, text }
        });
        Sidecars { owner }
    }
}

impl Found {
    /// What `original` reads when it does not own its stem's sidecar, `self` being what the readers found for it: its own
    /// `<file>.xmp`, never the packet of the sibling that owns the shared one. A sidecar that is not under the shared
    /// name is the file's own already.
    pub(super) fn own(self, original: &str, naming: crate::sidecar::SidecarNaming, listed: &super::ListedFolders) -> Option<Found> {
        if !crate::sidecar::is_shared_stem_sidecar(original, &self.path) {
            return Some(self);
        }
        listed.asked.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let path = crate::sidecar::own_sidecar_for_import(original, naming, Some(&self.path))?;
        let text = std::fs::read_to_string(&path).ok();
        Some(Found { path, text })
    }
}

/// What the readers of a batch look a file's sidecar up with.
pub(super) struct SidecarSource {
    pub(super) naming: crate::sidecar::SidecarNaming,
    pub(super) listed: super::ListedFolders,
    /// The content the library has: a file with it is skipped as a duplicate, so no sidecar is looked up or read for
    /// it (`None`: every file's is).
    pub(super) known: Option<KnownContent>,
}

/// Content hash → the photo that has it (`None`: a file earlier in this import). Shared by the import and the readers
/// of a batch, which look content up on the pool's threads: the lock is held for a lookup only, never across a disk
/// access, and the planning adds to it once the batch is read.
#[derive(Clone, Default)]
pub(super) struct KnownContent(std::sync::Arc<std::sync::RwLock<HashMap<String, Option<lightcraft_catalog::PhotoId>>>>);

impl KnownContent {
    pub(super) fn contains(&self, hash: &str) -> bool {
        self.0.read().unwrap_or_else(std::sync::PoisonError::into_inner).contains_key(hash)
    }

    /// The photo with this content (`Some(None)`: a file earlier in this import).
    pub(super) fn get(&self, hash: &str) -> Option<Option<lightcraft_catalog::PhotoId>> {
        self.0.read().unwrap_or_else(std::sync::PoisonError::into_inner).get(hash).copied()
    }

    pub(super) fn insert(&self, hash: String, photo: Option<lightcraft_catalog::PhotoId>) {
        self.0.write().unwrap_or_else(std::sync::PoisonError::into_inner).insert(hash, photo);
    }

    pub(super) fn extend(&self, content: impl IntoIterator<Item = (String, Option<lightcraft_catalog::PhotoId>)>) {
        self.0.write().unwrap_or_else(std::sync::PoisonError::into_inner).extend(content);
    }

    #[cfg(test)]
    pub(super) fn len(&self) -> usize {
        self.0.read().unwrap_or_else(std::sync::PoisonError::into_inner).len()
    }

    #[cfg(test)]
    pub(super) fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

/// A file's name, for the row that says what is waited for.
#[cfg(not(target_arch = "wasm32"))]
pub(super) fn shown(path: &str) -> String {
    Path::new(path).file_name().map_or_else(|| path.to_string(), |n| n.to_string_lossy().to_string())
}

/// Read `jobs` (several at once), counting each in `done`: a cached probe is checked with `still_valid` (it asks the
/// disk) and kept if it holds, else the file is probed; with `sidecars` (the library's naming, what the folders'
/// listings say and the content the library has), the file's XMP sidecar is looked up and read
/// too. Every disk access happens on the pool's threads (see [`reads`]), so that a drive that stops answering
/// never holds the caller: the file that waits is named by `watch`, and given up when it is asked to. Stops before
/// the next file once `cancel` is set: the files not reached come back as errors (the reason the watch ended, if
/// it did).
pub(super) fn read_files(
    probe: Option<&crate::media::FileProbe>,
    jobs: Vec<ReadJob>,
    still_valid: fn(&str, &ProbeInfo) -> bool,
    sidecars: Option<SidecarSource>,
    done: &std::sync::atomic::AtomicUsize,
    cancel: &std::sync::atomic::AtomicBool,
    watch: &ReadWatch,
) -> Vec<Read> {
    use std::sync::atomic::Ordering::Relaxed;
    let probe = probe.cloned();
    // (owned by what it moves into: the pool's threads outlive this call when a read hangs)
    let read_one = move |job: ReadJob| -> Read {
        let ReadJob { path, cached } = job;
        let kept = cached.filter(|info| still_valid(&path, info));
        let reused = kept.is_some();
        let info = match kept {
            Some(info) => Ok(info),
            None => match &probe {
                Some(probe) => probe(&path),
                None => Ok(ProbeInfo {
                    format: Path::new(&path).extension().map(|e| e.to_string_lossy().to_uppercase()).unwrap_or_default(),
                    ..Default::default()
                }),
            },
        };
        reads::progressed();
        // (a file that failed to read has none; one whose content the library has is skipped by the planning as a
        // duplicate, which asks the disk nothing: its sidecar is neither looked up nor read)
        let sidecars = sidecars
            .as_ref()
            .filter(|_| info.is_ok())
            .filter(|source| !source.known.as_ref().is_some_and(|known| info.as_ref().is_ok_and(|i| i.content_hash.as_ref().is_some_and(|h| known.contains(h)))))
            .map(|source| Sidecars::read(&path, source.naming, &source.listed))
            .unwrap_or_default();
        Read { info, sidecars, unreached: false, reused }
    };
    #[cfg(not(target_arch = "wasm32"))]
    {
        use std::sync::Arc;
        let paths: Vec<String> = jobs.iter().map(|j| j.path.clone()).collect();
        let pool = reads::Pool { watch, cancel, name: "lc-import-read", threads: workers(jobs.len()) };
        let work: Arc<dyn Fn(ReadJob) -> Read + Send + Sync> = Arc::new(read_one);
        let outcomes = pool.run(jobs, &|i| paths.get(i).map(|p| shown(p)).unwrap_or_default(), work, Arc::new(|_| {}), Arc::new(|| {}), &|_| {
            done.fetch_add(1, Relaxed);
        });
        let unreached = watch.ended().unwrap_or_else(|| "not probed".into());
        outcomes
            .into_iter()
            .map(|o| match o {
                reads::Outcome::Done(read) => read,
                reads::Outcome::Failed(why) => Read { info: Err(why), sidecars: Sidecars::default(), unreached: false, reused: false },
                reads::Outcome::Unreached => Read { info: Err(unreached.clone()), sidecars: Sidecars::default(), unreached: true, reused: false },
            })
            .collect()
    }
    #[cfg(target_arch = "wasm32")]
    {
        let _ = watch;
        jobs.into_iter()
            .map(|job| {
                if cancel.load(Relaxed) {
                    return Read { info: Err("cancelled".into()), sidecars: Sidecars::default(), unreached: true, reused: false };
                }
                let r = read_one(job);
                done.fetch_add(1, Relaxed);
                r
            })
            .collect()
    }
}

/// Copy `src` into `dir` as `name` (default: its own name), made unique with -1, -2…; returns the
/// new path. The copy is verified ([`crate::import_move::copy_new`]: a new file, synced and
/// checked against `probe_hash`, the content hash the probe computed — or compared byte for byte
/// with the source when there is none); a bad one is removed and the photo reported as failed.
pub(super) fn copy_into(dir: &Path, src: &str, name: Option<&str>, probe_hash: Option<&str>) -> Result<String, String> {
    let name = name
        .map(str::to_string)
        .unwrap_or_else(|| Path::new(src).file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_else(|| "photo".into()));
    let expect = probe_hash.and_then(lightcraft_preview::Hash128::parse);
    crate::import_move::copy_new(Path::new(src), dir, &name, expect).map(|p| p.to_string_lossy().to_string())
}

#[cfg(test)]
#[path = "tests_reads.rs"]
pub(crate) mod tests_reads;

#[cfg(test)]
mod prepared_tests {
    use super::*;
    use crate::import::commit_prepared;
    use lightcraft_catalog::{Op, Photo, PhotoId, Source};

    fn temp_dir(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("lc-launch-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    /// A launch argument names a library only when the library opener would read that folder: a `CATALOG.SNAP`
    /// on a case-sensitive file system (no `catalog.snap` or `catalog.log` beside it) is not one.
    #[test]
    fn a_launch_argument_names_a_library_only_when_the_opener_can_read_it() {
        let dir = temp_dir("upper");
        let upper = dir.join("CATALOG.SNAP");
        std::fs::write(&upper, b"x").unwrap();
        let found = library_dir_of(&upper);
        assert_eq!(found.is_some(), crate::library::is_library(&dir), "{found:?}");
        if let Some(d) = &found {
            assert!(crate::library::is_library(d), "{} opened as a library the opener can't read", d.display());
        }
        // a folder holding only the journal is a library to the opener, so it is one here too
        let journal = temp_dir("journal");
        std::fs::write(journal.join(lightcraft_catalog::journal::LOG), b"x").unwrap();
        assert_eq!(library_dir_of(&journal), Some(journal.clone()));
        // a folder of photos is not
        let photos = temp_dir("photos");
        std::fs::write(photos.join("a.jpg"), b"x").unwrap();
        assert_eq!(library_dir_of(&photos), None);
        for d in [dir, journal, photos] {
            let _ = std::fs::remove_dir_all(d);
        }
    }

    /// The library check runs before the window opens; one that blocks (a dead network mount) gives up after the
    /// limit and the argument is imported instead, so LightCraft still starts.
    #[test]
    fn a_blocked_launch_probe_gives_up_after_its_limit() {
        let start = std::time::Instant::now();
        let found = within(std::time::Duration::from_millis(50), || {
            std::thread::sleep(std::time::Duration::from_secs(5));
            Some(PathBuf::from("late"))
        });
        assert_eq!(found, None);
        assert!(start.elapsed() < std::time::Duration::from_secs(2), "waited {:?}", start.elapsed());
        assert_eq!(within(LAUNCH_PROBE_LIMIT, || Some(1)), Some(1), "a quick answer is kept");
    }

    /// The launch arguments share one deadline (CodeRabbit on #2979): once it has passed, an argument is imported
    /// without its library check, so several paths on a dead mount can't each wait the limit.
    #[test]
    fn launch_arguments_share_one_deadline() {
        let dir = temp_dir("deadline");
        std::fs::write(dir.join(lightcraft_catalog::journal::LOG), b"x").unwrap();
        let arg = dir.to_string_lossy().to_string();
        let none = |_: &Path| None;
        assert_eq!(launch_path_until(&arg, None, None, none, launch_deadline()), Some(LaunchPath::Library(arg.clone())));
        let start = std::time::Instant::now();
        assert_eq!(launch_path_until(&arg, None, None, none, start), Some(LaunchPath::Import(arg.clone())), "deadline passed");
        assert!(start.elapsed() < std::time::Duration::from_millis(500), "waited {:?}", start.elapsed());
        let _ = std::fs::remove_dir_all(dir);
    }

    /// Extensions the picker may offer: every one in [`EXTENSIONS`], minus the
    /// files this build cannot import (see [`accept_list`]).
    fn pickable_extensions() -> Vec<&'static str> {
        let heif_decodable = lightcraft_codecs::Format::Heif.can_decode();
        EXTENSIONS
            .iter()
            .copied()
            .filter(|ext| match *ext {
                "avif" => false,
                "heic" | "heif" => heif_decodable,
                _ => true,
            })
            .collect()
    }

    #[test]
    fn accept_list_covers_every_pickable_extension_in_both_cases() {
        let list = accept_list();
        for ext in pickable_extensions() {
            assert!(list.contains(&format!(".{ext}")), "lowercase .{ext} missing from accept list");
            assert!(list.contains(&format!(".{}", ext.to_uppercase())), "uppercase .{} missing from accept list", ext.to_uppercase());
        }
    }

    /// The web picker must not offer files this build cannot import: `.avif` has
    /// no AV1 decoder in any build, so it is never listed, in either case.
    #[test]
    fn accept_list_never_offers_avif() {
        let list = accept_list();
        assert!(!list.contains(".avif"), "{list}");
        assert!(!list.contains(".AVIF"), "{list}");
    }

    /// `.heic`/`.heif` follow the codecs' `heif` feature: listed (in both cases)
    /// only when this build can decode them; otherwise hidden, so a web user
    /// cannot pick a file that would fail to import.
    #[test]
    fn accept_list_offers_heic_heif_only_when_decodable() {
        let list = accept_list();
        let listed = lightcraft_codecs::Format::Heif.can_decode();
        for ext in ["heic", "heif"] {
            assert_eq!(list.contains(&format!(".{ext}")), listed, "lowercase .{ext}");
            assert_eq!(list.contains(&format!(".{}", ext.to_uppercase())), listed, "uppercase .{ext}");
        }
    }

    #[test]
    fn revalidate_add_only_converts_matching_ready_item() {
        let mut s = Session::new();
        let existing_path = std::env::temp_dir().join("lightcraft-revalidate-existing.jpg");
        let other_path = std::env::temp_dir().join("lightcraft-revalidate-other.jpg");
        let existing = Photo::new(PhotoId(1), Source::File { path: existing_path.to_string_lossy().into() }, "existing.jpg", "JPG", 1, 1, "now");
        s.commit("existing", Op::AddPhoto { photo: Box::new(existing) }).unwrap();
        let ready = |path: &Path| ReadyFile {
            path: path.to_string_lossy().into(),
            stored: path.to_string_lossy().into(),
            info: ProbeInfo { format: "JPG".into(), ..ProbeInfo::default() },
            sidecar: None,
            placed: None,
        };
        let mut prepared = Prepared {
            scanned: 2,
            items: vec![PreparedItem::Ready(Box::new(ready(&existing_path))), PreparedItem::Ready(Box::new(ready(&other_path)))],
            stopped: None,
        };
        prepared.revalidate_add(&s);
        assert!(matches!(prepared.items.first(), Some(PreparedItem::Duplicate { reason: "path", .. })));
        assert!(matches!(prepared.items.get(1), Some(PreparedItem::Ready(_))));
    }

    /// A photo whose key is `<hash>:dup<id>` still stands for its content: a file with that
    /// content that became Ready meanwhile is a duplicate, not a new photo.
    #[test]
    fn revalidate_add_matches_a_duplicate_keyed_photo_by_its_content() {
        let mut s = Session::new();
        let path = std::env::temp_dir().join("lightcraft-revalidate-dupkey.jpg");
        let mut twin = Photo::new(PhotoId(7), Source::File { path: "/elsewhere/twin.jpg".into() }, "twin.jpg", "JPG", 1, 1, "now");
        twin.content_hash = Some("abc123:dup5".into());
        s.commit("twin", Op::AddPhoto { photo: Box::new(twin) }).unwrap();
        let info = ProbeInfo { format: "JPG".into(), content_hash: Some("abc123".into()), ..ProbeInfo::default() };
        let ready = ReadyFile { path: path.to_string_lossy().into(), stored: path.to_string_lossy().into(), info, sidecar: None, placed: None };
        let mut prepared = Prepared { scanned: 1, items: vec![PreparedItem::Ready(Box::new(ready))], stopped: None };
        prepared.revalidate_add(&s);
        assert!(matches!(prepared.items.first(), Some(PreparedItem::Duplicate { reason: "content", existing: Some(PhotoId(7)), .. })));
    }

    /// A Duplicate without a named original is matched by content, and a photo keyed
    /// `<hash>:dup<id>` stands for its content too.
    #[test]
    fn commit_names_a_duplicate_keyed_photo_as_the_original_by_content() {
        let mut s = Session::new();
        let mut twin = Photo::new(PhotoId(7), Source::File { path: "/elsewhere/twin.jpg".into() }, "twin.jpg", "JPG", 1, 1, "now");
        twin.content_hash = Some("abc123:dup5".into());
        s.commit("twin", Op::AddPhoto { photo: Box::new(twin) }).unwrap();
        let dup = PreparedItem::Duplicate { path: "/new/c.jpg".into(), existing: None, reason: "content", hash: Some("abc123".into()), file: None };
        let prepared = Prepared { scanned: 1, items: vec![dup], stopped: None };
        let r = commit_prepared(&mut s, &ImportOptions::default(), "now", prepared).unwrap();
        assert_eq!(r.duplicates.first().map(|d| d.existing), Some(Some(7)), "{:?}", r.duplicates);
    }

    fn unread_paths(n: usize) -> Vec<String> {
        (0..n).map(|i| std::env::temp_dir().join(format!("lightcraft-probe-progress-{i}.jpg")).to_string_lossy().into()).collect()
    }

    /// Reading the files of a large import (a Lightroom catalog's originals) reported nothing:
    /// the progress stood still at "14 of 24554" for as long as it took.
    #[test]
    fn prepare_counts_the_files_it_reads() {
        use std::sync::atomic::Ordering::Relaxed;
        let mut s = Session::new();
        let seen = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let (seen_in_probe, done_in_probe) = (seen.clone(), std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0)));
        let mut job = ImportJob::new(&mut s, ImportOptions { mode: ImportMode::Add, ..ImportOptions::default() }).unwrap();
        let (done, last) = (job.read_done.clone(), done_in_probe.clone());
        job.probe = Some(std::sync::Arc::new(move |_: &str| {
            seen_in_probe.fetch_add(1, Relaxed);
            last.fetch_max(done.load(Relaxed), Relaxed);
            Ok(ProbeInfo { format: "JPEG".into(), ..ProbeInfo::default() })
        }));
        let prepared = job.prepare(&unread_paths(40), &std::sync::atomic::AtomicBool::new(false));
        assert_eq!(prepared.items.len(), 40);
        assert_eq!(seen.load(Relaxed), 40);
        assert!(done_in_probe.load(Relaxed) > 0, "progress moves while files are read");
        assert_eq!((job.read_done.load(Relaxed), job.read_total.load(Relaxed)), (40, 40), "every file read is counted");
        assert_eq!((job.done.load(Relaxed), job.total.load(Relaxed)), (40, 40), "handling them is counted on its own");
    }

    /// Cancel had no effect until every file had been read.
    #[test]
    fn cancel_stops_reading_files() {
        use std::sync::atomic::Ordering::Relaxed;
        let mut s = Session::new();
        let cancel = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let seen = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let mut job = ImportJob::new(&mut s, ImportOptions { mode: ImportMode::Add, ..ImportOptions::default() }).unwrap();
        let (seen_in_probe, cancel_in_probe) = (seen.clone(), cancel.clone());
        job.probe = Some(std::sync::Arc::new(move |_: &str| {
            seen_in_probe.fetch_add(1, Relaxed);
            cancel_in_probe.store(true, Relaxed);
            Ok(ProbeInfo { format: "JPEG".into(), ..ProbeInfo::default() })
        }));
        let prepared = job.prepare(&unread_paths(400), &cancel);
        // at most the files the (up to 8) readers had already started
        assert!(seen.load(Relaxed) <= 8, "read {} files after cancel", seen.load(Relaxed));
        assert!(prepared.items.is_empty());
    }

    /// A probe a scan left is used again for a file that is as it was, and only for it: a file that
    /// changed size, one that is gone and one with no probe are read.
    #[test]
    fn a_scans_probe_is_reused_only_for_a_file_as_it_was() {
        use std::sync::atomic::Ordering::Relaxed;
        let dir = std::env::temp_dir().join(format!("lightcraft-reuse-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let mut files = Vec::new();
        for (name, bytes) in [("same.jpg", 10usize), ("grown.jpg", 10), ("fresh.jpg", 10)] {
            let p = dir.join(name);
            std::fs::write(&p, vec![0u8; bytes]).unwrap();
            files.push(p.to_string_lossy().to_string());
        }
        let gone = dir.join("gone.jpg").to_string_lossy().to_string();
        files.push(gone.clone());
        let mut s = Session::new();
        let mut job = ImportJob::new(&mut s, ImportOptions { mode: ImportMode::Add, ..ImportOptions::default() }).unwrap();
        let probed = |size: u64| ProbeInfo { format: "JPEG".into(), file_size: size, ..ProbeInfo::default() };
        job.cache.insert(files[0].clone(), probed(10));
        job.cache.insert(files[1].clone(), probed(5));
        job.cache.insert(gone.clone(), probed(10));
        let read = std::sync::Arc::new(std::sync::Mutex::new(Vec::<String>::new()));
        let read_in_probe = read.clone();
        job.probe = Some(std::sync::Arc::new(move |p: &str| {
            read_in_probe.lock().unwrap().push(p.to_string());
            Ok(probed(10))
        }));
        let prepared = job.prepare_files(files.clone(), &std::sync::atomic::AtomicBool::new(false));
        let mut read = read.lock().unwrap().clone();
        read.sort();
        let mut expect = vec![files[1].clone(), files[2].clone(), gone];
        expect.sort();
        assert_eq!(read, expect, "only what was not as the scan saw it is read");
        assert_eq!(job.read_total.load(Relaxed), 3);
        assert_eq!(prepared.items.len(), 4);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// The sidecars found for a batch go to the photos they belong to, whichever have one, and a
    /// photo that failed to probe takes none of the others'.
    #[test]
    fn a_batchs_sidecars_go_to_their_own_photos() {
        let dir = std::env::temp_dir().join(format!("lightcraft-sidecars-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let xmp = r#"<x:xmpmeta xmlns:x="adobe:ns:meta/"><rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#"><rdf:Description xmlns:xmp="http://ns.adobe.com/xap/1.0/" xmp:Rating="4"/></rdf:RDF></x:xmpmeta>"#;
        let files: Vec<String> = ["a.jpg", "broken.jpg", "c.jpg", "d.jpg"].iter().map(|n| dir.join(n).to_string_lossy().to_string()).collect();
        for f in &files {
            std::fs::write(f, b"x").unwrap();
        }
        std::fs::write(dir.join("c.xmp"), xmp).unwrap();
        let mut s = Session::new();
        let mut job = ImportJob::new(&mut s, ImportOptions { mode: ImportMode::Add, ..ImportOptions::default() }).unwrap();
        job.probe = Some(std::sync::Arc::new(|p: &str| {
            if p.ends_with("broken.jpg") {
                Err("unreadable".into())
            } else {
                Ok(ProbeInfo { format: "JPEG".into(), file_size: 1, ..ProbeInfo::default() })
            }
        }));
        let prepared = job.prepare_files(files, &std::sync::atomic::AtomicBool::new(false));
        let rating = |name: &str| {
            prepared.items.iter().find_map(|i| match i {
                PreparedItem::Ready(r) if r.path.ends_with(name) => Some(r.sidecar.as_ref().and_then(|sc| sc.rating)),
                _ => None,
            })
        };
        assert_eq!((rating("a.jpg"), rating("c.jpg"), rating("d.jpg")), (Some(None), Some(Some(4)), Some(None)));
        assert!(prepared.items.iter().any(|i| matches!(i, PreparedItem::Failed(p, _) if p.ends_with("broken.jpg"))));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
