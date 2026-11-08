use super::{Caches, album_picker, i18n, panels};

/// The albums' counts as the sidebar shows them ([`Caches::album_counts`]).
pub(crate) struct AlbumCounts {
    /// The library they are of ([`lightcraft_engine::Session::library_generation`]).
    library: u64,
    /// The minute they are of while a smart album asks about "the last N days" (else empty).
    minute: String,
    /// What follows edits to a photo or a few.
    tally: lightcraft_catalog::tally::AlbumTally,
    /// The counts as the panels take them.
    shown: std::sync::Arc<std::collections::HashMap<lightcraft_catalog::AlbumId, usize>>,
}

/// A count of every photo on a worker thread ([`Caches::album_counts`]).
#[cfg_attr(target_arch = "wasm32", allow(dead_code))]
pub(crate) struct AlbumRecount {
    /// The library, minute and catalog revision it counts.
    library: u64,
    minute: String,
    revision: u64,
    /// The count, sent before the repaint is asked for, so the frame it wakes finds it.
    done: std::sync::mpsc::Receiver<Result<lightcraft_catalog::tally::AlbumTally, String>>,
    /// The thread, until it has let go of its copy of the catalog.
    worker: std::thread::JoinHandle<()>,
}

/// How often every library photo's file is checked for the Missing Photos count (each one once,
/// after whatever the panels ask: a library on a share is not checked over and over).
pub(crate) const SWEEP_SECONDS: f64 = 300.0;

/// The values the filter bar's pickers offer.
#[derive(Clone, Debug, Default)]
pub struct FilterValues {
    pub cameras: Vec<String>,
    pub lenses: Vec<String>,
    pub keywords: Vec<String>,
}

/// What the smart-album rule dialog shows besides the rules themselves: their problems (each row
/// marks its own) and the albums an Album rule picks from. (How many photos the rules match is a [`Count`].)
pub struct RulesView {
    pub problems: Vec<lightcraft_catalog::rules::Problem>,
    pub albums: Vec<album_picker::AlbumEntry>,
}

/// How many photos a dialog's filter matches ([`LiveCount::of`]).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Count {
    /// The count of this filter, if one has been made (none while the first is still running, or after one failed).
    pub n: Option<usize>,
    /// The catalog has changed since `n` was counted, and a new count is waiting or running: the dialog marks it.
    pub updating: bool,
}

/// How many of a filter's counts may still be running after its filter changed (a count can't be stopped).
#[cfg_attr(target_arch = "wasm32", allow(dead_code))]
const MAX_LET_GO: usize = 2;

/// How long a frame waits for a count it has just started: a small library is counted in well under this, and shows
/// the count at once instead of a frame of "counting…" for each change.
#[cfg_attr(target_arch = "wasm32", allow(dead_code))]
const QUICK: std::time::Duration = std::time::Duration::from_millis(4);

/// The longest a count of the same rules rests after one that took long: counts of a catalog that changes under them
/// follow each other no closer than the last one took (at most this), so one core isn't counting for a whole import.
#[cfg_attr(target_arch = "wasm32", allow(dead_code))]
const MAX_REST: std::time::Duration = std::time::Duration::from_secs(2);

/// How long a count that waits for the worker gate (counts of rules since changed, still running) waits before it asks
/// again: the repaint such a worker asks for as it ends can come before its thread has, and nothing else need repaint.
const GATE_RETRY: std::time::Duration = std::time::Duration::from_millis(100);

/// What a count was made of: the catalog revision, and the minute "in the last…" rules counted back from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Stamp {
    revision: u64,
    minute: u64,
}

/// A count of the photos a filter matches, made on a worker thread: counting every photo takes seconds in a large
/// library, and an import changes the catalog on every frame, so a count per catalog revision would hold the UI
/// thread (or the worker) for the whole import. The count is of what is counted (`what`: the filter and the folder
/// it carries), not of the catalog revision or the minute: while either changes the last count stays shown, marked
/// as updating, and the next one starts when the worker is free (and has rested, for a library so large that
/// counting it takes long). Where no thread can be had (wasm32) the count is made on the spot.
#[derive(Default)]
pub struct LiveCount {
    /// The last count to finish, with what it counted and when.
    done: Option<(u64, Stamp, usize)>,
    /// The count running now.
    job: Option<CountJob>,
    /// Counts of a filter that has since changed, still running: they end on their own, and new counts wait for
    /// them only when several are.
    #[cfg_attr(target_arch = "wasm32", allow(dead_code))]
    let_go: Vec<std::thread::JoinHandle<()>>,
    /// The what and stamp of a count that failed (already logged): not tried again for the same catalog.
    failed: Option<(u64, Stamp)>,
    /// No count of the same rules starts before this: the last one's own duration after it ended.
    #[cfg_attr(target_arch = "wasm32", allow(dead_code))]
    rest_until: Option<std::time::Instant>,
    /// The pass (of the `egui::Context`) of the last call that answered "updating" with no count running: a count is
    /// still to start (resting, or waiting for the gate). Headless settling waits for it while a dialog asks.
    waiting: Option<u64>,
    /// How many counts ran on the worker, and how many on the UI thread (no thread to spare, or wasm).
    pub jobs: usize,
    pub ui_thread_scans: usize,
    /// Tests: how long each worker count waits before it counts (a large library's seconds, made small).
    #[cfg(test)]
    pub(crate) slow: Option<std::time::Duration>,
    /// Tests: every count ends in an error.
    #[cfg(test)]
    pub(crate) fails: bool,
    /// Tests: no worker thread can be had (the count is made on the spot, as on wasm32).
    #[cfg(test)]
    pub(crate) no_thread: bool,
}

/// What a count answers: how many, and how long it took.
type Counted = Result<(usize, std::time::Duration), String>;

/// A count of every photo on a worker thread ([`LiveCount`]).
#[cfg_attr(target_arch = "wasm32", allow(dead_code))]
struct CountJob {
    what: u64,
    stamp: Stamp,
    /// The count, sent before the repaint is asked for, so the frame it wakes finds it.
    done: std::sync::mpsc::Receiver<Counted>,
    worker: std::thread::JoinHandle<()>,
}

impl LiveCount {
    /// The last count to finish, and the catalog revision it counted (tests).
    #[cfg(test)]
    pub(crate) fn last(&self) -> Option<(usize, u64)> {
        self.done.map(|(_, stamp, n)| (n, stamp.revision))
    }

    /// Whether a count is running on the worker (it has not finished, taken or not), or one is still to start for a
    /// dialog that was drawn in the last pass: a recount that rests or waits for the worker gate. Not while no
    /// dialog asks for it any more.
    pub(crate) fn running(&self, ctx: &egui::Context) -> bool {
        self.job.as_ref().is_some_and(|job| !job.worker.is_finished())
            || self.waiting.is_some_and(|pass| pass.saturating_add(1) >= ctx.cumulative_pass_nr())
    }

    /// The count of `what` (the filter `filter` makes) in `cat`: the one made, or the one before while a new one
    /// runs. "In the last…" rules count back from `now`, of which `minute` is the part that tells a new count from
    /// the last. `ctx` repaints when the worker is done.
    pub(crate) fn of(
        &mut self,
        cat: &lightcraft_catalog::Catalog,
        what: u64,
        minute: &str,
        filter: impl FnOnce() -> lightcraft_catalog::Filter,
        now: &str,
        ctx: &egui::Context,
    ) -> Count {
        self.collect();
        self.waiting = None;
        let stamp = Stamp { revision: cat.revision, minute: key_of(minute) };
        // a count of other rules is no use now: let it go (it ends on its own)
        if let Some(job) = self.job.take_if(|j| j.what != what) {
            self.let_go.push(job.worker);
        }
        let shown = self.done.filter(|(of, _, _)| *of == what);
        if let Some((_, at, n)) = shown
            && at == stamp
        {
            return Count { n: Some(n), updating: false };
        }
        self.let_go.retain(|w| !w.is_finished());
        if self.failed == Some((what, stamp)) {
            return Count::default();
        }
        let before = shown.map(|(_, _, n)| n);
        if self.job.is_none() && self.let_go.len() < MAX_LET_GO {
            // (new rules count at once; the same rules again, once the catalog has changed, after a rest)
            if before.is_some()
                && let Some(rest) = self.rest()
            {
                ctx.request_repaint_after(rest);
            } else if let Some(n) = self.start(cat, what, stamp, filter(), now, ctx) {
                return Count { n: Some(n), updating: false };
            } else if self.failed == Some((what, stamp)) {
                // (it ended at once, in an error: not "counting…")
                return Count::default();
            }
        } else if self.job.is_none() {
            ctx.request_repaint_after(GATE_RETRY);
        }
        if self.job.is_none() {
            self.waiting = Some(ctx.cumulative_pass_nr());
        }
        Count { n: before, updating: true }
    }

    /// How much longer a count of the same rules waits, if it does.
    fn rest(&self) -> Option<std::time::Duration> {
        // (none on wasm32, where `Instant::now` can't be asked)
        self.rest_until?.checked_duration_since(std::time::Instant::now())
    }

    /// Start a count of every photo on a worker thread, and say how many if it ends at once; or, where there is no
    /// thread to be had, count them here.
    fn start(
        &mut self,
        cat: &lightcraft_catalog::Catalog,
        what: u64,
        stamp: Stamp,
        filter: lightcraft_catalog::Filter,
        now: &str,
        ctx: &egui::Context,
    ) -> Option<usize> {
        #[cfg(not(target_arch = "wasm32"))]
        {
            // (the copy is of the photo pointers, not the photos: some milliseconds in a library of millions)
            let (copy, ctx, now) = (cat.clone(), ctx.clone(), now.to_string());
            #[cfg(test)]
            let (slow, fails) = (self.slow, self.fails);
            let (tx, done) = std::sync::mpsc::channel();
            #[cfg(test)]
            let no_thread = self.no_thread;
            #[cfg(not(test))]
            let no_thread = false;
            let spawned = if no_thread {
                Err(std::io::Error::other("no thread to spare"))
            } else {
                std::thread::Builder::new().name("lc-rules-count".into()).spawn({
                    let filter = filter.clone();
                    move || {
                        let began = std::time::Instant::now();
                        #[cfg(test)]
                        if let Some(wait) = slow {
                            std::thread::sleep(wait);
                        }
                        let counted = lightcraft_engine::guard::catch("Counting the photos a filter matches", || {
                            lightcraft_catalog::rules::set_now(Some(now));
                            copy.count(&filter)
                        });
                        #[cfg(test)]
                        let counted = counted.and_then(|n| if fails { Err("the count fails".to_string()) } else { Ok(n) });
                        let _ = tx.send(counted.map(|n| (n, began.elapsed())));
                        ctx.request_repaint();
                    }
                })
            };
            match spawned {
                Ok(worker) => {
                    self.jobs += 1;
                    return match done.recv_timeout(QUICK) {
                        Ok(counted) => self.finish(what, stamp, counted),
                        Err(_) => {
                            self.job = Some(CountJob { what, stamp, done, worker });
                            None
                        }
                    };
                }
                Err(e) => log::warn!("photo count: can't start a thread ({e}); counting here"),
            }
        }
        #[cfg(target_arch = "wasm32")]
        let _ = ctx;
        self.ui_thread_scans += 1;
        lightcraft_catalog::rules::set_now(Some(now.to_string()));
        let counted = lightcraft_engine::guard::catch("Counting the photos a filter matches", || cat.count(&filter));
        #[cfg(test)]
        let counted = counted.and_then(|n| if self.fails { Err("the count fails".to_string()) } else { Ok(n) });
        self.finish(what, stamp, counted.map(|n| (n, std::time::Duration::ZERO)))
    }

    /// Take the worker's count once it is done.
    fn collect(&mut self) {
        let Some(job) = &self.job else { return };
        let counted = match job.done.try_recv() {
            Ok(counted) => counted,
            Err(std::sync::mpsc::TryRecvError::Empty) => return,
            Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                log::error!("photo count: the count stopped without an answer");
                Err("stopped without an answer".to_string())
            }
        };
        let Some(job) = self.job.take() else { return };
        self.finish(job.what, job.stamp, counted);
    }

    /// A count of `what` ended: shown from now on, or (when it failed) not tried again for this catalog. Says how many.
    fn finish(&mut self, what: u64, stamp: Stamp, counted: Counted) -> Option<usize> {
        match counted {
            Ok((n, took)) => {
                self.failed = None;
                self.done = Some((what, stamp, n));
                // (only a count that ran on a thread took any time; on wasm32 there is none to ask the time of)
                if !took.is_zero() {
                    self.rest_until = Some(std::time::Instant::now() + took.min(MAX_REST));
                }
                Some(n)
            }
            Err(_) => {
                self.failed = Some((what, stamp));
                None
            }
        }
    }
}

pub(crate) fn key_of(parts: impl std::hash::Hash) -> u64 {
    use std::hash::{BuildHasher, BuildHasherDefault, DefaultHasher};
    BuildHasherDefault::<DefaultHasher>::default().hash_one(parts)
}

/// Whether a tally of the library made when the catalog was at `revision` is still right, given that it reads only
/// `reads` of a photo: nothing changed since, or only other things of some photos did (a rating changes no folder's
/// count, a slider changes none at all). Each of these tallies asks every photo, so one kept is a pass over the
/// library saved on every edit.
fn still_right(cat: &lightcraft_catalog::Catalog, revision: u64, reads: lightcraft_catalog::Aspects) -> bool {
    revision == cat.revision || cat.aspects_since(revision).is_some_and(|changed| !changed.any_of(reads))
}

/// Runs `list` on a thread of its own (named for the library's files) with `flag` set for as long as it runs. The flag
/// is cleared however the thread ends, a panic included, and when no thread could be started.
#[cfg(not(target_arch = "wasm32"))]
pub(crate) fn run_listing(flag: &std::sync::Arc<std::sync::atomic::AtomicBool>, list: impl FnOnce() + Send + 'static) -> std::io::Result<()> {
    struct Done(std::sync::Arc<std::sync::atomic::AtomicBool>);
    impl Drop for Done {
        fn drop(&mut self) {
            self.0.store(false, std::sync::atomic::Ordering::Release);
        }
    }
    flag.store(true, std::sync::atomic::Ordering::Release);
    let done = Done(flag.clone());
    // (a thread that did not start drops the closure, and with it the guard)
    std::thread::Builder::new()
        .name("lc-library-files".into())
        .spawn(move || {
            let _done = done;
            list();
        })
        .map(drop)
}

/// What the tallies below read of a photo, besides whether it is in the library.
const READS_FILE: lightcraft_catalog::Aspects = lightcraft_catalog::Aspects::PRESENCE.with(lightcraft_catalog::Aspects::FILE);

const READS_META: lightcraft_catalog::Aspects = lightcraft_catalog::Aspects::PRESENCE.with(lightcraft_catalog::Aspects::META);

const READS_PEOPLE: lightcraft_catalog::Aspects = READS_META.with(lightcraft_catalog::Aspects::ANALYSIS);

/// A tree built from the library's tally, kept while the counts it was built from are the
/// tally's (`stamp`) and nothing but photos changed since `revision` (a folder's label, which
/// the trees also show, changes no photo), and none of what the tree also shows besides the
/// photos' counts (`reads`: the keyword list for the keyword tree) did. A kept tree moves its
/// revision on.
fn kept_tree<T>(
    slot: &mut Option<(u64, u64, std::sync::Arc<T>)>,
    cat: &lightcraft_catalog::Catalog,
    stamp: u64,
    reads: lightcraft_catalog::Aspects,
) -> Option<std::sync::Arc<T>> {
    let (s, r, tree) = slot.take()?;
    (s == stamp && (r == cat.revision || cat.aspects_since(r).is_some_and(|changed| !changed.any_of(reads)))).then(|| {
        *slot = Some((s, cat.revision, tree.clone()));
        tree
    })
}

impl Caches {
    /// The folders the library's photos were imported from (none before the library's first
    /// count: see [`library_work::LibraryWork::current`]).
    pub fn folder_tree(&mut self, cat: &lightcraft_catalog::Catalog, library: u64) -> std::sync::Arc<Vec<lightcraft_catalog::FolderNode>> {
        let Some(t) = self.library.current(cat, library) else { return std::sync::Arc::default() };
        let stamp = t.folders_stamp();
        if let Some((s, r, tree)) = self.folder_tree.take()
            // (a label or anything else of a folder changed besides photos: build again)
            && (r == cat.revision || cat.aspects_since(r).is_some())
        {
            if s == stamp {
                self.folder_tree = Some((s, cat.revision, tree.clone()));
                return tree;
            }
            // photos came into or went from some folders (an import batch): only the rows on the way to
            // them change, so the rows of every other folder of the library needn't be made again
            let rows = std::sync::Arc::try_unwrap(tree).unwrap_or_else(|shared| (*shared).clone());
            if let Some(rows) = t.patch_folder_tree(cat, rows, &mut self.folder_upkeep) {
                let tree = std::sync::Arc::new(rows);
                self.folder_tree = Some((stamp, cat.revision, tree.clone()));
                return tree;
            }
        }
        let (rows, upkeep) = t.folder_tree_kept(cat);
        let tree = std::sync::Arc::new(rows);
        self.folder_upkeep = upkeep;
        self.folder_tree = Some((stamp, cat.revision, tree.clone()));
        tree
    }
    /// The folder tree as last built, when it still holds (a read-only check cannot rebuild it). Not
    /// while the library's tally lags the catalog ([`library_work::LibraryWork::is_current`]): the
    /// tree then shows the folders as they were before the change being counted. Nor a tree built from an older
    /// tally than the current one: one kept while the counts lagged moved its revision on with the catalog's, but
    /// its stamp is the old tally's.
    pub(crate) fn folder_tree_if_fresh(
        &self,
        cat: &lightcraft_catalog::Catalog,
        library: u64,
    ) -> Option<std::sync::Arc<Vec<lightcraft_catalog::FolderNode>>> {
        let stamp = self.library.current_tally(cat, library)?.folders_stamp();
        self.folder_tree.as_ref().filter(|(s, r, _)| *s == stamp && still_right(cat, *r, READS_FILE)).map(|(_, _, t)| t.clone())
    }
    /// The library's keyword tree.
    pub fn keyword_tree(&mut self, cat: &lightcraft_catalog::Catalog, library: u64) -> std::sync::Arc<Vec<lightcraft_catalog::KeywordNode>> {
        let Some(t) = self.library.current(cat, library) else { return std::sync::Arc::default() };
        let stamp = t.keywords_stamp();
        kept_tree(&mut self.keyword_tree, cat, stamp, lightcraft_catalog::Aspects::KEYWORD_LIST).unwrap_or_else(|| {
            let tree = std::sync::Arc::new(t.keyword_tree(cat));
            self.keyword_tree = Some((stamp, cat.revision, tree.clone()));
            tree
        })
    }
    /// The Keywording box's read-only names for the selection: what exported files carry
    /// (`export`), or the keywords with those containing them.
    pub(crate) fn keyword_names(&mut self, session: &lightcraft_engine::Session, export: bool) -> std::sync::Arc<Vec<panels::keywording::Chip>> {
        let (cat, selection) = (&session.catalog, session.selected_ids());
        let key = key_of((session.selection_key(), export));
        match &self.keyword_names {
            Some((r, k, c)) if *r == cat.revision && *k == key => c.clone(),
            _ => {
                let c = std::sync::Arc::new(if export {
                    panels::keywording::will_export(cat, selection)
                } else {
                    panels::keywording::with_containing(cat, selection)
                });
                self.keyword_names = Some((cat.revision, key, c.clone()));
                c
            }
        }
    }
    /// The selection's keywords (the Keywording box's chips).
    pub(crate) fn keyword_chips(&mut self, session: &lightcraft_engine::Session) -> std::sync::Arc<Vec<panels::keywording::Chip>> {
        let (cat, selection) = (&session.catalog, session.selected_ids());
        let key = key_of(session.selection_key());
        match &self.keyword_chips {
            Some((r, k, c)) if *r == cat.revision && *k == key => c.clone(),
            _ => {
                let c = std::sync::Arc::new(panels::keywording::chips(cat, selection));
                self.keyword_chips = Some((cat.revision, key, c.clone()));
                c
            }
        }
    }
    /// How many photos the selection is: each once, those the library has (what the Keywording
    /// box's "N of M" counts).
    pub(crate) fn selected_photos(&mut self, session: &lightcraft_engine::Session) -> usize {
        let (cat, selection) = (&session.catalog, session.selected_ids());
        let key = key_of(session.selection_key());
        match self.selected_photos {
            Some((r, k, n)) if r == cat.revision && k == key => n,
            _ => {
                let n = panels::keywording::distinct(cat, selection).len();
                self.selected_photos = Some((cat.revision, key, n));
                n
            }
        }
    }
    /// The selection's keywords grouped by the keyword they are inside (the grouped Keywording box).
    pub(crate) fn keyword_groups(&mut self, session: &lightcraft_engine::Session) -> std::sync::Arc<Vec<panels::keywording::Group>> {
        let cat = &session.catalog;
        let key = key_of(session.selection_key());
        match &self.keyword_groups {
            Some((r, k, g)) if *r == cat.revision && *k == key => g.clone(),
            _ => {
                let chips = self.keyword_chips(session);
                let g = std::sync::Arc::new(panels::keywording::grouped(&chips));
                self.keyword_groups = Some((cat.revision, key, g.clone()));
                g
            }
        }
    }
    /// How many selected photos have each keyword (the Keyword List's tick boxes).
    pub(crate) fn keyword_ticks(&mut self, session: &lightcraft_engine::Session) -> std::sync::Arc<panels::keyword_list::Ticks> {
        let (cat, selection) = (&session.catalog, session.selected_ids());
        let key = key_of(session.selection_key());
        match &self.keyword_ticks {
            Some((r, k, t)) if *r == cat.revision && *k == key => t.clone(),
            _ => {
                let t = std::sync::Arc::new(panels::keyword_list::Ticks::of(cat, selection));
                self.keyword_ticks = Some((cat.revision, key, t.clone()));
                t
            }
        }
    }
    /// The people named on faces among the photos the filter lets through (its own `person` aside),
    /// with photo counts.
    pub fn people(
        &mut self,
        cat: &lightcraft_catalog::Catalog,
        filter: &lightcraft_catalog::Filter,
    ) -> std::sync::Arc<Vec<lightcraft_catalog::Person>> {
        let key = lightcraft_catalog::Filter { person: None, ..filter.clone() };
        match &self.people {
            Some((r, f, t)) if *r == cat.revision && *f == key => t.clone(),
            _ => {
                let t = std::sync::Arc::new(cat.people_in(&key));
                self.people = Some((cat.revision, key, t.clone()));
                t
            }
        }
    }
    /// All Photos / Picks / Recently Deleted counts (none before the library's first count).
    pub fn counts(&mut self, cat: &lightcraft_catalog::Catalog, library: u64) -> lightcraft_catalog::library_tally::LibraryCounts {
        self.library.current(cat, library).map(|t| t.counts()).unwrap_or_default()
    }
    /// Missing Photos' count: the library photos whose file the background checks found gone
    /// (see [`missing_count::MissingCount`]). The files of photos that came in or moved are asked
    /// about at once; every file is handed to the checks when the library opens and every
    /// [`SWEEP_SECONDS`] (see [`Caches::check_files`]), and again when changes to the library
    /// couldn't be followed photo by photo; never once per edit.
    pub fn missing_count(&mut self, cat: &lightcraft_catalog::Catalog, library: u64, files: &lightcraft_engine::availability::Availability) -> usize {
        let (n, lost) = self.missing.update(cat, library, files);
        // photos changed while nobody followed them: a check of every file covers them, unless
        // the last one started after them
        self.files_unchecked |= lost && self.files_swept.is_none_or(|(of, revision, _)| of != library || revision < cat.revision);
        n
    }
    /// Whether the Missing Photos count is walking over the photos in steps (the panel asks for the next frame).
    pub fn missing_walking(&self) -> bool {
        self.missing.walking()
    }
    /// Hand every library photo's file to the background checks (one at a time, after what is
    /// asked) when the library is new here, files came in that no check was asked for, or
    /// [`SWEEP_SECONDS`] passed since the last check ended; `now` is egui's time. The list is made
    /// from a copy of the catalog on a worker thread. Returns whether a check is going on: the
    /// caller keeps asking for frames then, because the time the last one ended is the time of the
    /// last frame that saw it going on.
    pub fn check_files(
        &mut self,
        cat: &lightcraft_catalog::Catalog,
        library: u64,
        files: &lightcraft_engine::availability::Availability,
        now: f64,
    ) -> bool {
        use std::sync::atomic::Ordering;
        if self.files_swept.is_some_and(|(of, _, _)| of != library) {
            // another library: what is left of the one that was open is not what to check now, and a list
            // of its files being made must not start a sweep (the new library's list is due at once)
            files.restart_sweeps();
            self.files_listing = std::sync::Arc::default();
            self.files_swept = None;
        }
        let checking = files.sweeping() > 0 || self.files_listing.load(Ordering::Acquire);
        if checking {
            // (the next one is due a while after this one ends, however long it takes)
            if let Some((_, _, at)) = self.files_swept.as_mut() {
                *at = now;
            }
        }
        let due = self.files_unchecked || self.files_swept.is_none_or(|(_, _, at)| now - at >= SWEEP_SECONDS);
        if !due || !files.is_background() || checking {
            return checking;
        }
        self.files_swept = Some((library, cat.revision, now));
        self.files_unchecked = false;
        #[cfg(not(target_arch = "wasm32"))]
        {
            let epoch = files.restart_sweeps();
            let (cat, files) = (cat.clone(), files.clone());
            let started = run_listing(&self.files_listing, move || {
                files.sweep_in(epoch, lightcraft_engine::cmd::missing::candidates(&cat));
            });
            if let Err(e) = started {
                // tried again at the next check
                log::warn!("missing photos: no thread to list the library's files: {e}");
            }
        }
        true
    }
    /// Everyone named on a face in the library (for completing a name as it is typed).
    pub fn person_names(&mut self, cat: &lightcraft_catalog::Catalog) -> std::sync::Arc<Vec<String>> {
        let kept = self.person_names.take().filter(|(r, _)| still_right(cat, *r, READS_PEOPLE)).map(|(_, v)| v);
        let v = kept.unwrap_or_else(|| std::sync::Arc::new(cat.people().into_iter().map(|p| p.name).collect::<Vec<_>>()));
        self.person_names = Some((cat.revision, v.clone()));
        v
    }
    /// The By Date tree.
    pub fn date_groups(&mut self, cat: &lightcraft_catalog::Catalog, library: u64) -> std::sync::Arc<Vec<lightcraft_catalog::DateGroup>> {
        let Some(t) = self.library.current(cat, library) else { return std::sync::Arc::default() };
        let stamp = t.dates_stamp();
        match &self.date_groups {
            Some((s, g)) if *s == stamp => g.clone(),
            _ => {
                let g = std::sync::Arc::new(t.date_groups());
                self.date_groups = Some((stamp, g.clone()));
                g
            }
        }
    }
    /// Cameras, lenses and keywords in the library (filter bar pickers).
    pub fn filter_values(&mut self, cat: &lightcraft_catalog::Catalog) -> std::sync::Arc<FilterValues> {
        match &self.filter_values {
            Some((r, v)) if *r == cat.revision => v.clone(),
            _ => {
                let distinct = |mut v: Vec<String>| {
                    v.retain(|s| !s.trim().is_empty());
                    v.sort_by_key(|s| s.to_lowercase());
                    v.dedup();
                    v
                };
                let lib = || cat.photos().filter(|p| p.in_library());
                let v = std::sync::Arc::new(FilterValues {
                    cameras: distinct(lib().map(|p| p.meta.camera.clone()).collect()),
                    lenses: distinct(lib().map(|p| p.meta.lens.clone()).collect()),
                    keywords: cat.keywords().into_iter().map(|(k, _)| k).collect(),
                });
                self.filter_values = Some((cat.revision, v.clone()));
                v
            }
        }
    }
    /// What the Add keyword box offers for `typed` (see [`panels::keywording::offers`]). Kept apart
    /// from [`Self::suggestions`]: both are asked for on every frame of typing, and each looks
    /// through the library's keywords.
    pub(crate) fn completions(
        &mut self,
        cat: &lightcraft_catalog::Catalog,
        current: &[String],
        typed: &str,
    ) -> std::sync::Arc<panels::keywording::Offers> {
        let k = key_of((cat.revision, current, typed));
        match &self.completions {
            Some((h, v)) if *h == k => v.clone(),
            _ => {
                let v = std::sync::Arc::new(panels::keywording::offers(cat, current, typed));
                self.completions = Some((k, v.clone()));
                v
            }
        }
    }
    /// Keyword suggestions for a photo with `current` keywords and the typed `prefix`.
    pub fn suggestions(&mut self, cat: &lightcraft_catalog::Catalog, current: &[String], prefix: &str, n: usize) -> std::sync::Arc<Vec<String>> {
        let k = key_of((cat.revision, current, prefix, n));
        match &self.suggestions {
            Some((h, v)) if *h == k => v.clone(),
            _ => {
                let v = std::sync::Arc::new(cat.keyword_suggestions(current, prefix, n));
                self.suggestions = Some((k, v.clone()));
                v
            }
        }
    }
    /// Every album's photo count for the sidebar (a smart album evaluates its rules, which
    /// scans the catalog). Recomputed when the catalog changes (photos, metadata, album rules
    /// all bump its revision) and — only while some smart album has an "in the last…" rule —
    /// when `now` (the session clock, ISO) enters a new minute, so such counts follow the clock
    /// within a minute without rescanning every frame.
    /// [`RulesView`] for `rules` edited in smart album `editing` (`None`: a new album), within
    /// what it carries (`carried`: the albums and folders it was made from, a photo of any of them counts): worked out again only when the catalog, the
    /// rules, the language or (for "in the last…" rules) the minute changes, not every frame. How many photos the
    /// rules match is counted on a worker thread ([`LiveCount`]): the [`Count`] is the last one made while the
    /// catalog changes under it (an import commits a batch per frame), and none while a rule edit's count is
    /// still running.
    pub fn rules_view(
        &mut self,
        cat: &lightcraft_catalog::Catalog,
        rules: &lightcraft_catalog::RuleSet,
        editing: Option<u64>,
        carried: lightcraft_catalog::Filter,
        now: &str,
        ctx: &egui::Context,
    ) -> (std::sync::Arc<RulesView>, Count) {
        let relative = rules.depends_on_now();
        let minute = if relative { now.get(..16).unwrap_or(now) } else { "" };
        let written = serde_json::to_string(rules).unwrap_or_default();
        let carried_key = serde_json::to_string(&carried).unwrap_or_default();
        let k = key_of((cat.revision, &written, editing, &carried_key, minute, i18n::language().code()));
        let view = match &self.rules_view {
            Some((key, v)) if *key == k => v.clone(),
            _ => {
                // (problems and the albums to pick from look at the rules and the albums, not at every photo)
                let problems = rules.check_for(cat, editing.map(lightcraft_catalog::AlbumId));
                let v = std::sync::Arc::new(RulesView { problems, albums: panels::dialogs::album_entries(cat, editing) });
                self.rules_view = Some((k, v.clone()));
                v
            }
        };
        if !view.problems.is_empty() {
            return (view, Count::default());
        }
        let what = key_of((&written, &carried_key));
        let filter = || lightcraft_catalog::Filter {
            rule_set: Some(rules.clone()),
            library_folders: carried.library_folders.clone(),
            album: carried.album,
            albums: carried.albums.clone(),
            ..Default::default()
        };
        let count = self.rules_count.of(cat, what, minute, filter, now, ctx);
        (view, count)
    }

    /// How many photos `filter` matches, for Create Smart Album's live count: counted on a worker thread like
    /// [`Caches::rules_view`]'s.
    pub(crate) fn smart_album_count(
        &mut self,
        cat: &lightcraft_catalog::Catalog,
        filter: &lightcraft_catalog::Filter,
        now: &str,
        ctx: &egui::Context,
    ) -> Count {
        let minute = if filter.depends_on_now() { now.get(..16).unwrap_or(now) } else { "" };
        let what = key_of(format!("{filter:?}"));
        self.new_smart_album_count.of(cat, what, minute, || filter.clone(), now, ctx)
    }

    /// Whether a dialog's count is running on a worker (a frame may still have to take it), or a recount is still to
    /// start for a dialog drawn in the last pass ([`LiveCount::running`]).
    pub fn count_running(&self, ctx: &egui::Context) -> bool {
        self.rules_count.running(ctx) || self.new_smart_album_count.running(ctx)
    }

    /// A count an open dialog shows (photos a filter matches, photos with a keyword…): `count` runs
    /// again only when the catalog or `what` (what is counted) changes, not every frame.
    pub(crate) fn dialog_count(&mut self, cat: &lightcraft_catalog::Catalog, what: impl std::hash::Hash, count: impl FnOnce() -> usize) -> usize {
        let k = key_of((cat.revision, what));
        if let Some((key, n)) = self.dialog_count
            && key == k
        {
            return n;
        }
        let n = count();
        self.dialog_count_scans += 1;
        self.dialog_count = Some((k, n));
        n
    }

    /// The problems of every saved smart album that has some (`Catalog::smart_album_problems`),
    /// for the sidebar's ⚠ marks: worked out again only when the catalog changes.
    pub fn smart_album_problems(
        &mut self,
        cat: &lightcraft_catalog::Catalog,
    ) -> std::sync::Arc<std::collections::HashMap<lightcraft_catalog::AlbumId, Vec<lightcraft_catalog::rules::Problem>>> {
        if let Some((rev, v)) = &self.smart_problems
            && *rev == cat.revision
        {
            return v.clone();
        }
        let v: std::sync::Arc<std::collections::HashMap<_, _>> = std::sync::Arc::new(
            cat.albums().filter(|a| a.is_smart()).map(|a| (a.id, cat.smart_album_problems(a.id))).filter(|(_, p)| !p.is_empty()).collect(),
        );
        self.smart_problem_scans += 1;
        self.smart_problems = Some((cat.revision, v.clone()));
        v
    }

    /// How many photos each album of the library `library` ([`lightcraft_engine::Session::library_generation`])
    /// shows, `now` being the session clock ("in the last…" rules count back from it).
    ///
    /// An edit to a photo or a few is followed at once: a smart album asks only those photos again (a keyword
    /// created and given to them too: the keyword list itself counts in no album). Anything more (an album's rules
    /// or photos changed, a stack, an edit to many photos such as a keyword renamed across them, a new minute while
    /// a smart album asks about "the last N days", another library) counts every photo again, seconds in a large
    /// library. That count runs on a worker thread (natively) while the counts shown before stay (none the first
    /// time, and none of another library); `ctx` repaints when it is done, and edits made meanwhile are followed
    /// from it.
    pub fn album_counts(
        &mut self,
        cat: &lightcraft_catalog::Catalog,
        library: u64,
        now: &str,
        ctx: &egui::Context,
    ) -> std::sync::Arc<std::collections::HashMap<lightcraft_catalog::AlbumId, usize>> {
        let (relative, minute) = Self::album_clock(cat, now);
        self.collect_album_recount(library);
        // (album ids start again in every library: another one's counts would be shown on this one's albums)
        if self.album_counts.as_ref().is_some_and(|c| c.library != library) {
            self.album_counts = None;
        }
        if let Some(counts) = self.album_counts.as_ref().filter(|c| c.minute == minute && c.tally.revision() == cat.revision) {
            return counts.shown.clone();
        }
        if relative {
            // "in the last N days" counts back from the session clock, as in the grid. (Within a minute an edit asks
            // only the photos it touched, at this second; the others were asked earlier in the minute. That is as
            // far out as the counts already were between edits, and the next minute counts them all again.)
            lightcraft_catalog::rules::set_now(Some(now.to_string()));
        }
        if let Some(counts) = self.album_counts.as_mut().filter(|c| c.minute == minute)
            && counts.tally.update(cat)
        {
            counts.shown = std::sync::Arc::new(counts.tally.counts().clone());
            return counts.shown.clone();
        }
        self.recount_albums(cat, library, minute, now, relative, ctx);
        self.album_counts.as_ref().map(|c| c.shown.clone()).unwrap_or_default()
    }

    /// Whether a smart album asks about "the last N days", and the minute the counts are of then (else empty).
    fn album_clock<'a>(cat: &lightcraft_catalog::Catalog, now: &'a str) -> (bool, &'a str) {
        let relative = cat.albums().any(|a| a.smart.as_ref().is_some_and(|f| f.depends_on_now()));
        (relative, if relative { now.get(..16).unwrap_or(now) } else { "" })
    }

    /// [`Caches::album_counts`] for the `albums.list` command, with how far they are from the catalog and the clock as
    /// they are now: the command answers at once from them (marking the albums that are behind), and never counts a
    /// photo on the UI thread.
    pub fn album_counts_for_list(
        &mut self,
        cat: &lightcraft_catalog::Catalog,
        library: u64,
        now: &str,
        ctx: &egui::Context,
    ) -> (std::sync::Arc<std::collections::HashMap<lightcraft_catalog::AlbumId, usize>>, lightcraft_engine::CountsAge) {
        use lightcraft_engine::CountsAge;
        let counts = self.album_counts(cat, library, now, ctx);
        let (_, minute) = Self::album_clock(cat, now);
        let age = match self.album_counts.as_ref() {
            Some(c) if c.library == library && c.tally.revision() == cat.revision => {
                if c.minute == minute {
                    CountsAge::Current
                } else {
                    CountsAge::ClockBehind
                }
            }
            _ => CountsAge::Behind,
        };
        (counts, age)
    }

    /// A count of every photo is running ([`Caches::album_counts`]).
    pub fn album_recount_running(&self) -> bool {
        self.album_recount.as_ref().is_some_and(|job| !job.worker.is_finished())
    }

    /// Count every photo for [`Caches::album_counts`]: on a worker thread when one can be started, else here.
    fn recount_albums(&mut self, cat: &lightcraft_catalog::Catalog, library: u64, minute: &str, now: &str, relative: bool, ctx: &egui::Context) {
        // one at a time: what it finds is followed up to the catalog as it is by then, or counted again
        let failed = self.album_recount_failed.as_ref().is_some_and(|(l, r, m)| *l == library && *r == cat.revision && m == minute);
        if self.album_recount.is_some() || failed {
            return;
        }
        self.album_count_scans += 1;
        #[cfg(not(target_arch = "wasm32"))]
        {
            // (cheap: the copy shares the photos)
            let (copy, ctx, now) = (cat.clone(), ctx.clone(), relative.then(|| now.to_string()));
            let (tx, done) = std::sync::mpsc::channel();
            let spawned = std::thread::Builder::new().name("lc-album-counts".into()).spawn(move || {
                let counted = lightcraft_engine::guard::catch("Counting the albums' photos", || {
                    lightcraft_catalog::rules::set_now(now);
                    lightcraft_catalog::tally::AlbumTally::of(&copy)
                });
                let _ = tx.send(counted);
                ctx.request_repaint();
            });
            match spawned {
                Ok(worker) => {
                    self.album_recount = Some(AlbumRecount { library, minute: minute.to_string(), revision: cat.revision, done, worker });
                    return;
                }
                Err(e) => log::warn!("album counts: can't start a thread ({e}); counting them here"),
            }
        }
        #[cfg(target_arch = "wasm32")]
        let _ = (now, relative, ctx);
        // (`album_counts` has set the clock on this thread)
        let counted = lightcraft_engine::guard::catch("Counting the albums' photos", || lightcraft_catalog::tally::AlbumTally::of(cat));
        self.finish_album_recount(library, cat.revision, minute.to_string(), counted);
    }

    /// Take the worker's count once it is done; a count of another library is let go (its thread ends on its own).
    fn collect_album_recount(&mut self, library: u64) {
        let Some(job) = &self.album_recount else { return };
        if job.library != library {
            self.album_recount = None;
            return;
        }
        let counted = match job.done.try_recv() {
            Ok(counted) => counted,
            Err(std::sync::mpsc::TryRecvError::Empty) => return,
            Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                log::error!("album counts: the count stopped without an answer");
                Err("stopped without an answer".to_string())
            }
        };
        let Some(job) = self.album_recount.take() else { return };
        self.finish_album_recount(job.library, job.revision, job.minute, counted);
    }

    /// The counts a count of every photo made are the ones shown. One that failed (already logged) leaves those
    /// shown before, and isn't tried again for the same library, catalog and minute.
    fn finish_album_recount(&mut self, library: u64, revision: u64, minute: String, counted: Result<lightcraft_catalog::tally::AlbumTally, String>) {
        match counted {
            Ok(tally) => {
                self.album_recount_failed = None;
                let shown = std::sync::Arc::new(tally.counts().clone());
                self.album_counts = Some(AlbumCounts { library, minute, tally, shown });
            }
            Err(_) => self.album_recount_failed = Some((library, revision, minute)),
        }
    }
}

#[cfg(test)]
mod cache_tests {
    use lightcraft_catalog::{Album, AlbumId, Catalog, Filter, Op, Photo, PhotoId, RuleSet, Source};

    fn photo(id: u64, captured: &str, rating: u8) -> Op {
        let mut p = Photo::new(PhotoId(id), Source::Demo { scene: 0 }, &format!("p{id}.jpg"), "JPEG", 60, 40, "2026-01-01T00:00:00");
        p.captured = Some(captured.into());
        p.rating = rating;
        Op::AddPhoto { photo: Box::new(p) }
    }

    fn smart(id: u64, rules: serde_json::Value) -> Op {
        let mut a = Album::new(AlbumId(id), format!("smart {id}"));
        let rules: RuleSet = serde_json::from_value(rules).unwrap();
        a.smart = Some(Box::new(Filter { rule_set: Some(rules), ..Default::default() }));
        Op::AddAlbum { album: a }
    }

    type Counts = std::sync::Arc<std::collections::HashMap<AlbumId, usize>>;

    /// [`super::Caches::album_counts`] of library `library` once a count of every photo it started is done and
    /// taken.
    fn counted_in(c: &mut super::Caches, cat: &Catalog, library: u64, now: &str) -> Counts {
        let ctx = egui::Context::default();
        let t0 = std::time::Instant::now();
        loop {
            let shown = c.album_counts(cat, library, now, &ctx);
            if c.album_recount.is_none() {
                return shown;
            }
            assert!(t0.elapsed() < std::time::Duration::from_secs(30), "the count is done");
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
    }

    fn counted(c: &mut super::Caches, cat: &Catalog, now: &str) -> Counts {
        counted_in(c, cat, 0, now)
    }

    /// Smart-album counts: unchanged frames reuse them; metadata and rule changes and (for
    /// "in the last…" rules) the clock crossing into a new minute recompute them.
    #[test]
    fn smart_album_counts_are_cached_until_something_changes() {
        let mut cat = Catalog::default();
        let mut c = super::Caches::default();
        for (i, d) in ["2026-09-30T11:59:30", "2026-09-29T08:00:00", "2026-01-01T08:00:00"].iter().enumerate() {
            cat.apply(photo(i as u64 + 1, d, if i == 0 { 5 } else { 1 })).unwrap();
        }
        cat.apply(smart(10, serde_json::json!({"rules": [{"field": "rating", "op": "gte", "value": 3}]}))).unwrap();
        cat.apply(smart(11, serde_json::json!({"rules": [{"field": "captureDate", "op": "inLast", "value": {"n": 1, "unit": "hours"}}]}))).unwrap();
        let now = "2026-09-30T12:00:00";
        let n = counted(&mut c, &cat, now);
        assert_eq!((n[&AlbumId(10)], n[&AlbumId(11)]), (1, 1));
        assert_eq!(c.album_count_scans, 1);
        for _ in 0..10 {
            counted(&mut c, &cat, "2026-09-30T12:00:40");
        }
        assert_eq!(c.album_count_scans, 1, "unchanged frames in the same minute don't rescan");
        // a photo rated: followed from that photo, not counted from every one
        cat.apply(Op::SetRating { id: PhotoId(2), rating: 4 }).unwrap();
        assert_eq!(counted(&mut c, &cat, now)[&AlbumId(10)], 2);
        assert_eq!(c.album_count_scans, 1);
        // rule change
        let rs: RuleSet = serde_json::from_value(serde_json::json!({"rules": [{"field": "rating", "op": "gte", "value": 5}]})).unwrap();
        cat.apply(Op::SetAlbumRules { id: AlbumId(10), rules: Box::new(Filter { rule_set: Some(rs), ..Default::default() }) }).unwrap();
        assert_eq!(counted(&mut c, &cat, now)[&AlbumId(10)], 1);
        // an hour later the 11:59:30 photo has left "in the last hour", with no catalog change
        assert_eq!(counted(&mut c, &cat, "2026-09-30T13:00:10")[&AlbumId(11)], 0);
        assert_eq!(c.album_count_scans, 3, "new rules and a new minute are counted from every photo; the rating was not");
        lightcraft_catalog::rules::set_now(None);
    }

    fn rule_set(v: u64) -> RuleSet {
        serde_json::from_value(serde_json::json!({"rules": [{"field": "rating", "op": "gte", "value": v}]})).unwrap()
    }

    type Shown = (std::sync::Arc<super::RulesView>, super::Count);

    /// [`super::Caches::rules_view`] of `rules` once the count it started is done and taken.
    fn rules_counted(c: &mut super::Caches, cat: &Catalog, rules: &RuleSet, now: &str) -> Shown {
        rules_counted_within(c, cat, rules, Filter::default(), now)
    }

    /// [`rules_counted`] for a smart album that carries `carried`.
    fn rules_counted_within(c: &mut super::Caches, cat: &Catalog, rules: &RuleSet, carried: Filter, now: &str) -> Shown {
        let ctx = egui::Context::default();
        let t0 = std::time::Instant::now();
        loop {
            let shown = c.rules_view(cat, rules, None, carried.clone(), now, &ctx);
            if !shown.1.updating {
                return shown;
            }
            assert!(t0.elapsed() < std::time::Duration::from_secs(30), "the count is done");
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
    }

    /// A smart album made from a view of several albums stays limited to them (`album.setRules` with
    /// `replace` keeps its albums and folders): the editor's count is of the photos the saved album shows.
    #[test]
    fn the_rule_dialog_counts_within_the_albums_the_smart_album_was_made_from() {
        let mut cat = Catalog::default();
        let mut c = super::Caches::default();
        for i in 1..=3u64 {
            cat.apply(photo(i, "2026-09-30T11:00:00", 5)).unwrap();
        }
        let mut a = Album::new(AlbumId(20), "Trip");
        a.photos = vec![PhotoId(1), PhotoId(2)];
        cat.apply(Op::AddAlbum { album: a }).unwrap();
        let rules = rule_set(1);
        let now = "2026-09-30T12:00:00";
        let within = Filter { albums: vec![AlbumId(20)], ..Default::default() };
        assert_eq!(rules_counted_within(&mut c, &cat, &rules, within, now).1.n, Some(2), "only the photos of its albums");
        assert_eq!(
            rules_counted_within(&mut c, &cat, &rules, Filter { album: Some(AlbumId(20)), ..Default::default() }, now).1.n,
            Some(2),
            "also its single album"
        );
        assert_eq!(rules_counted(&mut c, &cat, &rules, now).1.n, Some(3), "none carried: every photo");
    }

    fn rated_library() -> Catalog {
        let mut cat = Catalog::default();
        for i in 0..3u64 {
            cat.apply(photo(i + 1, "2026-09-30T11:00:00", if i == 0 { 5 } else { 1 })).unwrap();
        }
        cat
    }

    /// The rule dialog's problems and album list are worked out once per change to the catalog or the rules, not
    /// every frame; so are the sidebar's smart-album problems. Its count is made once per rules (and catalog).
    #[test]
    fn rule_dialog_and_problems_are_cached_until_something_changes() {
        let mut cat = rated_library();
        let mut c = super::Caches::default();
        let now = "2026-09-30T12:00:00";
        let (view, count) = rules_counted(&mut c, &cat, &rule_set(3), now);
        assert_eq!((count.n, view.problems.len()), (Some(1), 0));
        let jobs = c.rules_count.jobs;
        for _ in 0..10 {
            rules_counted(&mut c, &cat, &rule_set(3), now);
        }
        assert_eq!(c.rules_count.jobs, jobs, "unchanged frames reuse it");
        assert_eq!(rules_counted(&mut c, &cat, &rule_set(1), now).1.n, Some(3), "a rule change");
        let (view, count) = rules_counted(&mut c, &cat, &rule_set(9), now);
        assert_eq!((view.problems.len(), count), (1, super::Count::default()), "rules with a problem count nothing");
        cat.apply(Op::SetRating { id: PhotoId(2), rating: 4 }).unwrap();
        assert_eq!(rules_counted(&mut c, &cat, &rule_set(3), now).1.n, Some(2), "a catalog change");
        assert_eq!(c.rules_count.ui_thread_scans, 0);
        // the sidebar's problems
        cat.apply(Op::AddAlbum { album: lightcraft_catalog::Album::new(AlbumId(20), "Trip") }).unwrap();
        cat.apply(smart(21, serde_json::json!({"rules": [{"field": "album", "op": "is", "value": 20}]}))).unwrap();
        assert!(c.smart_album_problems(&cat).get(&AlbumId(21)).is_none_or(Vec::is_empty));
        for _ in 0..10 {
            c.smart_album_problems(&cat);
        }
        assert_eq!(c.smart_problem_scans, 1);
        cat.apply(Op::RemoveAlbum { id: AlbumId(20) }).unwrap();
        assert!(c.smart_album_problems(&cat).get(&AlbumId(21)).is_some_and(|p| !p.is_empty()), "its album is gone");
        assert_eq!(c.smart_problem_scans, 2);
    }

    const SLOW: std::time::Duration = std::time::Duration::from_millis(120);

    /// Counting every photo takes seconds in a large library: it runs on a worker, never on the UI thread, and
    /// while the catalog changes (an import commits a batch per frame) the count made before stays, marked as
    /// updating, until the worker is free to make the next.
    #[test]
    fn the_rules_count_is_made_on_a_worker_and_follows_a_catalog_that_changes_every_frame() {
        let mut cat = rated_library();
        let mut c = super::Caches::default();
        c.rules_count.slow = Some(SLOW);
        let ctx = egui::Context::default();
        let now = "2026-09-30T12:00:00";
        let rules = rule_set(3);
        let first = c.rules_view(&cat, &rules, None, Filter::default(), now, &ctx).1;
        assert_eq!(first, super::Count { n: None, updating: true }, "a count that takes a while has none to show yet");
        assert_eq!((c.rules_count.jobs, c.rules_count.ui_thread_scans), (1, 0));
        assert_eq!(rules_counted(&mut c, &cat, &rules, now).1, super::Count { n: Some(1), updating: false });
        // a revision a frame: the last count stays, and nothing is counted on this thread
        let (jobs, began) = (c.rules_count.jobs, std::time::Instant::now());
        let mut marked = 0;
        for i in 0..40u64 {
            cat.apply(Op::SetRating { id: PhotoId(1 + i % 3), rating: if i % 2 == 0 { 5 } else { 4 } }).unwrap();
            let count = c.rules_view(&cat, &rules, None, Filter::default(), now, &ctx).1;
            assert!(count.n.is_some(), "the count made before is shown while the next is made");
            marked += usize::from(count.updating);
            std::thread::sleep(SLOW / 10);
        }
        assert_eq!(marked, 40, "every frame says the count is out of date");
        assert_eq!(c.rules_count.ui_thread_scans, 0, "no frame counted the library");
        let most = began.elapsed().as_millis() / SLOW.as_millis() + 1;
        assert!((c.rules_count.jobs - jobs) as u128 <= most, "one count at a time: {} in {most} counts' time", c.rules_count.jobs - jobs);
        // the changes stop: the count reaches the right value
        let right = cat.count(&Filter { rule_set: Some(rules.clone()), ..Default::default() });
        assert_eq!(rules_counted(&mut c, &cat, &rules, now).1, super::Count { n: Some(right), updating: false });
    }

    /// An edit to the rules shows the new rules' count once it is made, never the old rules' meanwhile; going back
    /// to the rules before keeps their count.
    #[test]
    fn a_rule_edit_shows_its_own_count_not_the_one_before() {
        let cat = rated_library();
        let mut c = super::Caches::default();
        c.rules_count.slow = Some(SLOW);
        let ctx = egui::Context::default();
        let now = "2026-09-30T12:00:00";
        assert_eq!(rules_counted(&mut c, &cat, &rule_set(3), now).1.n, Some(1));
        let edited = c.rules_view(&cat, &rule_set(1), None, Filter::default(), now, &ctx).1;
        assert_eq!(edited, super::Count { n: None, updating: true }, "not the 1 of the rules before");
        // back to the first rules while the edit is being counted: their count is as it was, and stays so when the
        // edit's count ends
        let back = c.rules_view(&cat, &rule_set(3), None, Filter::default(), now, &ctx).1;
        assert_eq!(back, super::Count { n: Some(1), updating: false });
        std::thread::sleep(SLOW * 3);
        assert_eq!(c.rules_view(&cat, &rule_set(3), None, Filter::default(), now, &ctx).1, super::Count { n: Some(1), updating: false });
        assert_eq!(c.rules_count.jobs, 2, "no count of them again");
        // edits typed faster than counts end: the last rules' count is the one that shows
        for v in [2, 3, 4, 5, 1] {
            c.rules_view(&cat, &rule_set(v), None, Filter::default(), now, &ctx);
        }
        assert_eq!(rules_counted(&mut c, &cat, &rule_set(1), now).1, super::Count { n: Some(3), updating: false });
        assert_eq!(c.rules_count.ui_thread_scans, 0);
    }

    /// "In the last…" rules count back from the session clock on the worker too; a new minute is a count out of date
    /// like a change to the catalog (the count before stays, marked), not new rules.
    #[test]
    fn a_new_minute_marks_the_count_and_the_worker_counts_back_from_the_session_clock() {
        let mut cat = Catalog::default();
        for i in 0..3u64 {
            cat.apply(photo(i + 1, "2026-09-30T11:30:00", 1)).unwrap();
        }
        let mut c = super::Caches::default();
        c.rules_count.slow = Some(SLOW);
        let ctx = egui::Context::default();
        let last_hour: RuleSet =
            serde_json::from_value(serde_json::json!({"rules": [{"field": "captureDate", "op": "inLast", "value": {"n": 1, "unit": "hours"}}]}))
                .unwrap();
        // (the session's clock, not the machine's, which is long past these photos)
        let t0 = std::time::Instant::now();
        let mut shown = c.rules_view(&cat, &last_hour, None, Filter::default(), "2026-09-30T12:00:00", &ctx).1;
        while shown.updating {
            assert!(t0.elapsed() < std::time::Duration::from_secs(30), "the count is done");
            std::thread::sleep(std::time::Duration::from_millis(1));
            shown = c.rules_view(&cat, &last_hour, None, Filter::default(), "2026-09-30T12:00:00", &ctx).1;
        }
        assert_eq!(shown.n, Some(3), "counted back from the session clock");
        let later = c.rules_view(&cat, &last_hour, None, Filter::default(), "2026-09-30T12:40:00", &ctx).1;
        assert_eq!(later, super::Count { n: Some(3), updating: true }, "the minute changed: the count before, marked");
        assert_eq!(rules_counted(&mut c, &cat, &last_hour, "2026-09-30T12:40:00").1.n, Some(0), "an hour and ten minutes on");
        lightcraft_catalog::rules::set_now(None);
    }

    /// A count of the same rules waits a while after one that took long, so a catalog that changes every frame
    /// doesn't keep a core counting.
    #[test]
    fn counts_of_the_same_rules_rest_between_them() {
        let mut cat = rated_library();
        let mut c = super::Caches::default();
        c.rules_count.slow = Some(SLOW);
        let ctx = egui::Context::default();
        let now = "2026-09-30T12:00:00";
        let rules = rule_set(3);
        assert_eq!(rules_counted(&mut c, &cat, &rules, now).1.n, Some(1));
        let jobs = c.rules_count.jobs;
        // (the count took SLOW: the next waits as long)
        cat.apply(Op::SetRating { id: PhotoId(2), rating: 4 }).unwrap();
        assert_eq!(c.rules_view(&cat, &rules, None, Filter::default(), now, &ctx).1, super::Count { n: Some(1), updating: true });
        assert_eq!(c.rules_count.jobs, jobs, "resting");
        let t0 = std::time::Instant::now();
        while c.rules_count.jobs == jobs {
            assert!(t0.elapsed() < std::time::Duration::from_secs(30), "the rest ends");
            std::thread::sleep(std::time::Duration::from_millis(5));
            assert_eq!(c.rules_view(&cat, &rules, None, Filter::default(), now, &ctx).1, super::Count { n: Some(1), updating: true });
        }
        assert_eq!(c.rules_count.jobs, jobs + 1, "rested, counting");
        assert_eq!(rules_counted(&mut c, &cat, &rules, now).1.n, Some(2));
    }

    /// A pass of `ctx` with nothing drawn: the dialog that asked for the count is no longer open.
    fn pass_without_the_dialog(ctx: &egui::Context) {
        ctx.run_ui(egui::RawInput::default(), |_| {}).textures_delta.clear();
    }

    /// Headless settling waits for a recount that is still to start (the catalog changed after the last count, which
    /// rests before the next), not only for one that is running: a screenshot then never shows the stale count.
    #[test]
    fn a_recount_that_waits_keeps_the_headless_view_busy_until_the_count_is_current() {
        let mut cat = rated_library();
        let mut c = super::Caches::default();
        c.rules_count.slow = Some(SLOW);
        let ctx = egui::Context::default();
        let now = "2026-09-30T12:00:00";
        let rules = rule_set(3);
        let mut shown = c.rules_view(&cat, &rules, None, Filter::default(), now, &ctx).1;
        let t0 = std::time::Instant::now();
        while shown.updating {
            assert!(t0.elapsed() < std::time::Duration::from_secs(30), "the count is done");
            std::thread::sleep(std::time::Duration::from_millis(1));
            shown = c.rules_view(&cat, &rules, None, Filter::default(), now, &ctx).1;
        }
        assert!(!c.count_running(&ctx), "a current count is nothing to wait for");
        let jobs = c.rules_count.jobs;
        cat.apply(Op::SetRating { id: PhotoId(2), rating: 4 }).unwrap();
        assert_eq!(c.rules_view(&cat, &rules, None, Filter::default(), now, &ctx).1, super::Count { n: Some(1), updating: true });
        assert_eq!(c.rules_count.jobs, jobs, "resting: no worker runs");
        assert!(c.count_running(&ctx), "the recount is still to come");
        // the dialog is closed (nothing asks for the count any more): nothing is waited for
        pass_without_the_dialog(&ctx);
        pass_without_the_dialog(&ctx);
        assert!(!c.count_running(&ctx), "a count nobody shows is not waited for");
        // open again: the recount is waited for, until it is made and shown
        c.rules_view(&cat, &rules, None, Filter::default(), now, &ctx);
        assert!(c.count_running(&ctx));
        assert_eq!(rules_counted_in(&mut c, &cat, &rules, now, &ctx).1.n, Some(2));
        assert!(!c.count_running(&ctx), "current again");
    }

    /// [`rules_counted`] with the caller's `ctx`.
    fn rules_counted_in(c: &mut super::Caches, cat: &Catalog, rules: &RuleSet, now: &str, ctx: &egui::Context) -> Shown {
        let t0 = std::time::Instant::now();
        loop {
            let shown = c.rules_view(cat, rules, None, Filter::default(), now, ctx);
            if !shown.1.updating {
                return shown;
            }
            assert!(t0.elapsed() < std::time::Duration::from_secs(30), "the count is done");
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
    }

    /// Two counts of rules since changed fill the worker gate; a count of the latest rules that has to wait for
    /// them asks to be tried again soon, because the repaint a finishing worker asks for can come before its thread
    /// has ended (and nothing else need repaint).
    #[test]
    fn a_count_held_back_by_the_workers_let_go_asks_to_be_tried_again() {
        let cat = rated_library();
        let mut c = super::Caches::default();
        c.rules_count.slow = Some(std::time::Duration::from_millis(1500));
        let ctx = egui::Context::default();
        let delays = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        ctx.set_repaint_observer({
            let delays = delays.clone();
            move |info| delays.lock().unwrap().push(info.delay)
        });
        let now = "2026-09-30T12:00:00";
        for v in [3, 1] {
            assert_eq!(c.rules_view(&cat, &rule_set(v), None, Filter::default(), now, &ctx).1, super::Count { n: None, updating: true });
        }
        assert_eq!(c.rules_count.jobs, 2, "both counts run (the second's rules have since changed)");
        delays.lock().unwrap().clear();
        // the third rules: the gate holds (both before are still counting), and it will not open by itself
        assert_eq!(c.rules_view(&cat, &rule_set(2), None, Filter::default(), now, &ctx).1, super::Count { n: None, updating: true });
        assert_eq!(c.rules_count.jobs, 2, "held back");
        assert!(delays.lock().unwrap().iter().any(|d| !d.is_zero()), "a later try is asked for: {:?}", delays.lock().unwrap());
        assert!(c.count_running(&ctx), "headless settling waits for it");
        // (once they end the held count is made)
        assert_eq!(rules_counted_in(&mut c, &cat, &rule_set(2), now, &ctx).1.n, Some(1));
    }

    /// A count that fails at once is the failed state on the first frame, not "counting…" until some other repaint:
    /// it ended on the worker before the frame stopped waiting for it, or where no thread can be had it ended
    /// on the spot.
    #[test]
    fn a_count_that_fails_at_once_shows_the_failed_state_on_the_first_frame() {
        let cat = rated_library();
        let ctx = egui::Context::default();
        let now = "2026-09-30T12:00:00";
        // no thread: counted here, and it fails
        let mut c = super::Caches::default();
        (c.rules_count.no_thread, c.rules_count.fails) = (true, true);
        assert_eq!(c.rules_view(&cat, &rule_set(3), None, Filter::default(), now, &ctx).1, super::Count::default());
        assert_eq!(c.rules_count.ui_thread_scans, 1);
        assert_eq!(c.rules_view(&cat, &rule_set(3), None, Filter::default(), now, &ctx).1, super::Count::default(), "and stays so");
        assert_eq!(c.rules_count.ui_thread_scans, 1, "not tried again for this catalog");
        // a worker that fails within the wait (a few photos, no delay: it ends in microseconds)
        let mut c = super::Caches::default();
        c.rules_count.fails = true;
        let first = c.rules_view(&cat, &rule_set(3), None, Filter::default(), now, &ctx).1;
        if c.rules_count.failed.is_some() {
            assert_eq!(first, super::Count::default());
        } else {
            // (a machine so busy that the worker missed the wait: the count was still pending)
            assert_eq!(first, super::Count { n: None, updating: true });
        }
        assert_eq!(c.rules_count.jobs, 1);
        let t0 = std::time::Instant::now();
        while c.rules_view(&cat, &rule_set(3), None, Filter::default(), now, &ctx).1.updating {
            assert!(t0.elapsed() < std::time::Duration::from_secs(30), "the failure is taken");
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
        assert_eq!(c.rules_count.jobs, 1, "not tried again for this catalog");
        // a count that is only slow is still pending on the first frame
        let mut c = super::Caches::default();
        c.rules_count.slow = Some(SLOW);
        assert_eq!(c.rules_view(&cat, &rule_set(3), None, Filter::default(), now, &ctx).1, super::Count { n: None, updating: true });
    }

    /// Create Smart Album's count is made the same way.
    #[test]
    fn the_new_smart_album_count_is_made_on_a_worker_too() {
        let mut cat = rated_library();
        let mut c = super::Caches::default();
        c.new_smart_album_count.slow = Some(SLOW);
        let ctx = egui::Context::default();
        let now = "2026-09-30T12:00:00";
        let filter = Filter { rule_set: Some(rule_set(3)), ..Default::default() };
        let t0 = std::time::Instant::now();
        let mut count = c.smart_album_count(&cat, &filter, now, &ctx);
        assert_eq!(count.n, None);
        while count.updating {
            assert!(t0.elapsed() < std::time::Duration::from_secs(30), "the count is done");
            std::thread::sleep(std::time::Duration::from_millis(1));
            count = c.smart_album_count(&cat, &filter, now, &ctx);
        }
        assert_eq!(count.n, Some(1));
        cat.apply(Op::SetRating { id: PhotoId(2), rating: 4 }).unwrap();
        std::thread::sleep(SLOW * 2);
        assert_eq!(c.smart_album_count(&cat, &filter, now, &ctx), super::Count { n: Some(1), updating: true }, "the count before, marked");
        assert_eq!(c.new_smart_album_count.ui_thread_scans, 0);
    }

    /// Without "in the last…" rules the clock never causes a rescan.
    #[test]
    fn absolute_rules_ignore_the_clock() {
        let mut cat = Catalog::default();
        let mut c = super::Caches::default();
        cat.apply(photo(1, "2026-09-30T11:59:30", 5)).unwrap();
        cat.apply(smart(10, serde_json::json!({"rules": [{"field": "rating", "op": "gte", "value": 3}]}))).unwrap();
        counted(&mut c, &cat, "2026-09-30T12:00:00");
        counted(&mut c, &cat, "2027-01-01T00:00:00");
        assert_eq!(c.album_count_scans, 1);
    }

    /// Counting every photo again (an album's rules changed, an edit to many photos) takes seconds in a large
    /// library: it runs off the UI thread, and the sidebar keeps the counts it showed until the new ones are ready.
    #[test]
    fn a_count_of_every_photo_runs_in_the_background_and_the_counts_shown_stay_meanwhile() {
        let mut cat = Catalog::default();
        let mut c = super::Caches::default();
        for i in 0..3u64 {
            cat.apply(photo(i + 1, "2026-09-30T11:00:00", if i == 0 { 5 } else { 1 })).unwrap();
        }
        cat.apply(smart(10, serde_json::json!({"rules": [{"field": "rating", "op": "gte", "value": 3}]}))).unwrap();
        let now = "2026-09-30T12:00:00";
        assert_eq!(counted(&mut c, &cat, now)[&AlbumId(10)], 1);
        // new rules: every photo is asked again
        let rs: RuleSet = serde_json::from_value(serde_json::json!({"rules": [{"field": "rating", "op": "gte", "value": 1}]})).unwrap();
        cat.apply(Op::SetAlbumRules { id: AlbumId(10), rules: Box::new(Filter { rule_set: Some(rs), ..Default::default() }) }).unwrap();
        let ctx = egui::Context::default();
        assert_eq!(c.album_counts(&cat, 0, now, &ctx)[&AlbumId(10)], 1, "the frame after the change shows the counts it had");
        assert!(c.album_recount.is_some(), "and every photo is counted elsewhere");
        // done counting is said whether or not a frame has taken the count yet (a folded Albums section takes none)
        let t0 = std::time::Instant::now();
        while c.album_recount_running() {
            assert!(t0.elapsed() < std::time::Duration::from_secs(30), "the count ends");
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
        // a photo rated meanwhile: followed from what the count finds
        cat.apply(Op::SetRating { id: PhotoId(3), rating: 0 }).unwrap();
        assert_eq!(counted(&mut c, &cat, now)[&AlbumId(10)], 2, "three photos rated at least 1, then one not");
        assert_eq!(c.album_count_scans, 2, "counted whole once for the new rules; the rating was followed");
        // the first time there is nothing to show yet: no count, rather than a wrong one
        let mut fresh = super::Caches::default();
        assert!(fresh.album_counts(&cat, 0, now, &ctx).get(&AlbumId(10)).is_none());
        assert_eq!(counted(&mut fresh, &cat, now)[&AlbumId(10)], 2);
    }

    /// Album ids start again in every library: once another library is open, the albums of the one before never
    /// lend it their counts, nor does a count of that library still running when it closed.
    #[test]
    fn another_library_never_shows_the_counts_of_the_one_before() {
        let library = |rated: &[u8]| {
            let mut cat = Catalog::default();
            for (i, r) in rated.iter().enumerate() {
                cat.apply(photo(i as u64 + 1, "2026-09-30T11:00:00", *r)).unwrap();
            }
            cat.apply(smart(10, serde_json::json!({"rules": [{"field": "rating", "op": "gte", "value": 3}]}))).unwrap();
            cat
        };
        let (mut first, second) = (library(&[5, 1, 1]), library(&[5, 5, 1]));
        let (mut c, ctx, now) = (super::Caches::default(), egui::Context::default(), "2026-09-30T12:00:00");
        assert_eq!(counted_in(&mut c, &first, 1, now)[&AlbumId(10)], 1);
        assert!(c.album_counts(&second, 2, now, &ctx).get(&AlbumId(10)).is_none(), "nothing until its own are counted");
        assert_eq!(counted_in(&mut c, &second, 2, now)[&AlbumId(10)], 2);
        // a count of the first library under way when the second opens is let go
        let mut c = super::Caches::default();
        assert_eq!(counted_in(&mut c, &first, 1, now)[&AlbumId(10)], 1);
        let rs: RuleSet = serde_json::from_value(serde_json::json!({"rules": [{"field": "rating", "op": "gte", "value": 1}]})).unwrap();
        first.apply(Op::SetAlbumRules { id: AlbumId(10), rules: Box::new(Filter { rule_set: Some(rs), ..Default::default() }) }).unwrap();
        assert_eq!(c.album_counts(&first, 1, now, &ctx)[&AlbumId(10)], 1);
        assert!(c.album_recount.is_some(), "the first library is being counted");
        assert_eq!(counted_in(&mut c, &second, 2, now)[&AlbumId(10)], 2, "the second library's own count");
    }
}
