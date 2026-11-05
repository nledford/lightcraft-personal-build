//! What the sidebar counts of the whole library: photos per folder, per day and per keyword, the
//! All Photos / Picks / Recently Deleted counts. Kept up to date from the photos the latest ops changed.
//!
//! Counting every photo is a pass over the library: about 100 ms at 180,000 photos. An import or a
//! Synchronize Folder adds photos a dozen or so at a time, and counting again after each batch
//! kept the window from answering for as long as the import ran. A [`LibraryTally`] remembers
//! what each photo added to the counts, so after an edit to a few photos
//! ([`Catalog::photos_changed_since`]) it takes their part out and puts what they add now back
//! in ([`LibraryTally::follow`]). What it can't follow (too many photos, a library just opened)
//! the caller counts whole ([`LibraryTally::of`]), wherever it wants: off the UI thread in the app.
//!
//! The trees are built from the counts by the code [`Catalog::folder_tree`],
//! [`Catalog::date_groups`] and [`Catalog::keyword_tree`] build theirs with.

use std::borrow::Borrow;
use std::collections::{HashMap, HashSet};
use std::hash::Hash;
use std::num::NonZeroU32;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use crate::folders::FolderNode;
use crate::keywords::KeywordNode;
use crate::query::DateGroup;
use crate::{Aspects, Catalog, Flag, Photo, PhotoId};

/// The most photos an edit may change for the tally to follow it photo by photo (each one costs
/// a few microseconds; past this, counting whole on a worker thread is the better deal).
pub const MAX_FOLLOWED: usize = 2048;

/// What the tally reads of a photo ([`Part`]): whether it is in the library or Recently Deleted,
/// its flag, its file, its capture time and its keywords. An edit to anything else (a rating, a
/// slider, however many photos) leaves it as it is.
const READS: Aspects = Aspects::PRESENCE.with(Aspects::FLAG).with(Aspects::FILE).with(Aspects::CAPTURED).with(Aspects::META);

/// All Photos, Picks and Recently Deleted.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct LibraryCounts {
    /// Photos in the library (not in Recently Deleted, not browse records of Local).
    pub total: usize,
    /// Of those, the picks.
    pub picks: usize,
    /// Photos in Recently Deleted.
    pub deleted: usize,
}

/// What one photo adds to the tally (nothing is kept for a photo that adds nothing: a browse
/// record of Local). Its folder, day and keyword levels are numbers ([`Id`]) of what the tally
/// holds once for every photo that shares them, so a record is 16 bytes however long a path is.
#[derive(Clone, Copy, Debug)]
struct Part {
    library: bool,
    pick: bool,
    trashed: bool,
    /// Its folder ([`crate::folders::photo_folder`]), when in the library.
    dir: Option<Id>,
    /// Its capture day ([`crate::query::capture_day`]), when in the library.
    day: Option<Id>,
    /// The keyword levels it carries, when in the library and it has any ([`Levels`]).
    set: Option<Id>,
}

/// What a photo adds to the tally, as its fields say it now ([`Part`] is what is kept of it).
struct Fresh<'c> {
    library: bool,
    pick: bool,
    trashed: bool,
    dir: Option<&'c str>,
    day: Option<&'c str>,
    keywords: &'c [String],
}

/// What `p` adds, `None` when nothing.
fn fresh(p: &Photo) -> Option<Fresh<'_>> {
    let library = p.in_library();
    let trashed = p.deleted && !p.local;
    if !library {
        return trashed.then_some(Fresh { library, pick: false, trashed, dir: None, day: None, keywords: &[] });
    }
    Some(Fresh {
        library,
        pick: p.flag == Flag::Pick,
        trashed,
        dir: crate::folders::photo_folder(p),
        day: crate::query::capture_day(p),
        keywords: &p.meta.keywords,
    })
}

/// The number of a key an [`Interner`] holds (from 1; a photo with no folder has `None`).
type Id = NonZeroU32;

/// Where key `id` is in an [`Interner`]'s slots.
fn at(id: Id) -> usize {
    (id.get() as usize).saturating_sub(1)
}

/// The keyword levels a photo carries: each once, by lower-case path, with the path as its first
/// keyword through that level writes it.
type Levels = Arc<[(Arc<str>, Arc<str>)]>;

/// The keys photos share (folders, days, keyword lists), each held once and numbered, with how
/// many photos hold it. A key goes, and its number is given to the next new one, with its last
/// photo. The photos' records keep the number (4 bytes), not the key (an `Arc<str>` is 16 and
/// every clone of it a count to bump), which a library of 1.7 million photos feels.
#[derive(Clone, Debug)]
struct Interner<K, T> {
    ids: HashMap<K, Id>,
    slots: Vec<Option<Slot<K, T>>>,
    free: Vec<Id>,
}

#[derive(Clone, Debug)]
struct Slot<K, T> {
    key: K,
    /// The photos that hold it.
    photos: usize,
    extra: T,
}

impl<K, T> Default for Interner<K, T> {
    fn default() -> Self {
        Interner { ids: HashMap::new(), slots: Vec::new(), free: Vec::new() }
    }
}

impl<K: Clone + Eq + Hash, T> Interner<K, T> {
    fn slot(&self, id: Id) -> Option<&Slot<K, T>> {
        self.slots.get(at(id))?.as_ref()
    }

    /// One more photo holds `key`: its number, made with `make` (the key and what goes with it)
    /// when no photo held it yet. `None` when `make` has nothing to make.
    fn hold<Q>(&mut self, key: &Q, make: impl FnOnce() -> Option<(K, T)>) -> Option<Id>
    where
        K: Borrow<Q>,
        Q: Hash + Eq + ?Sized,
    {
        let id = match self.ids.get(key) {
            Some(id) => *id,
            None => {
                let (key, extra) = make()?;
                self.insert(key, extra)?
            }
        };
        let held = self.slots.get_mut(at(id))?.as_mut()?;
        held.photos = held.photos.saturating_add(1);
        Some(id)
    }

    fn insert(&mut self, key: K, extra: T) -> Option<Id> {
        let slot = Some(Slot { key: key.clone(), photos: 0, extra });
        let id = match self.free.pop() {
            Some(id) => {
                *self.slots.get_mut(at(id))? = slot;
                id
            }
            None => {
                // (a key is held by a photo, which is kilobytes: 4 billion of them do not fit in memory)
                let id = Id::new(u32::try_from(self.slots.len()).ok()?.checked_add(1)?)?;
                self.slots.push(slot);
                id
            }
        };
        self.ids.insert(key, id);
        Some(id)
    }

    /// One photo fewer holds key `id`; the key goes with its last photo.
    fn release(&mut self, id: Id) {
        let Some(slot) = self.slots.get_mut(at(id)) else { return };
        let Some(held) = slot.as_mut() else { return };
        held.photos = held.photos.saturating_sub(1);
        if held.photos == 0
            && let Some(gone) = slot.take()
        {
            self.ids.remove(&gone.key);
            self.free.push(id);
        }
    }

    /// The keys held, in no order.
    fn iter(&self) -> impl Iterator<Item = &Slot<K, T>> {
        self.slots.iter().flatten()
    }
}

/// The folders or days held and how many photos each has, by name (what a `BTreeMap` of them iterates in).
fn by_name(keys: &Interner<Arc<str>, ()>) -> Vec<(&str, usize)> {
    let mut all: Vec<(&str, usize)> = keys.iter().map(|s| (&*s.key, s.photos)).collect();
    all.sort_unstable_by(|a, b| a.0.cmp(b.0));
    all
}

/// What each photo adds, by photo id. Ids are handed out in order, so a library's lie close
/// together, and a slot for each id from the first to the last ([`Part`] is 16 bytes) takes less
/// than a table's entry for each photo (24 bytes, in a table that is an eighth to half empty): a
/// library of 1.7 million photos spends 27 MiB on slots where a table took 52. The slots are made
/// when the whole library is counted ([`Parts::spanning`]); an id outside them (a photo added since,
/// or a library whose ids are far apart) goes in a table, as every id does when there are no slots.
#[derive(Clone, Debug, Default)]
struct Parts {
    /// The id of `slots[0]`.
    first: u64,
    slots: Vec<Option<Part>>,
    /// (a plain table: ids that are far apart are no reason to trust their low bits)
    far: HashMap<PhotoId, Part>,
}

const _: () = assert!(std::mem::size_of::<Option<Part>>() == 16);

impl Parts {
    /// Room for the ids from `first` to `last`, when they are no more than twice the `photos` of
    /// the library (and a thousand): a catalog whose ids are far apart has no slots, never a
    /// slot per id it names.
    fn spanning(first: PhotoId, last: PhotoId, photos: usize) -> Parts {
        let span = usize::try_from(last.0.saturating_sub(first.0).saturating_add(1)).unwrap_or(usize::MAX);
        if span <= photos.saturating_mul(2).saturating_add(1024) {
            Parts { first: first.0, slots: vec![None; span], far: HashMap::new() }
        } else {
            Parts::default()
        }
    }

    /// Where `id` has its slot.
    fn slot_of(&self, id: PhotoId) -> Option<usize> {
        usize::try_from(id.0.checked_sub(self.first)?).ok().filter(|i| *i < self.slots.len())
    }

    /// What photo `id` added, taken out.
    fn take(&mut self, id: PhotoId) -> Option<Part> {
        match self.slot_of(id) {
            Some(i) => self.slots.get_mut(i)?.take(),
            None => self.far.remove(&id),
        }
    }

    fn put(&mut self, id: PhotoId, part: Part) {
        match self.slot_of(id).and_then(|i| self.slots.get_mut(i)) {
            Some(slot) => *slot = Some(part),
            None => {
                self.far.insert(id, part);
            }
        }
    }
}

/// One way a keyword level is written, by the photos that write it so.
#[derive(Clone, Debug)]
struct Spelling {
    path: Arc<str>,
    photos: usize,
    /// The lowest photo id that writes it so; `None` once that photo went (the next lowest is
    /// not known photo by photo).
    first: Option<PhotoId>,
}

/// A keyword level: the photos that carry it, and how they write it.
#[derive(Clone, Debug, Default)]
struct Level {
    photos: usize,
    spellings: Vec<Spelling>,
}

impl Level {
    /// The path as the photo with the lowest id writes it (what [`Catalog::keyword_tree`] shows).
    fn path(&self) -> Option<&Arc<str>> {
        self.spellings.iter().min_by_key(|s| s.first.map_or(u64::MAX, |id| id.0)).map(|s| &s.path)
    }

    /// Whether which spelling comes first is not known: two or more, and one of them lost its
    /// first photo.
    fn unsure(&self) -> bool {
        self.spellings.len() > 1 && self.spellings.iter().any(|s| s.first.is_none())
    }
}

/// Bumped by every change to a tally's folders, days or keywords, so a tree built from them can
/// be kept until one changes ([`LibraryTally::folders_stamp`]).
static STAMPS: AtomicU64 = AtomicU64::new(1);

fn stamp() -> u64 {
    STAMPS.fetch_add(1, Ordering::Relaxed)
}

/// The library's photos counted by folder, day and keyword, as of one revision of the catalog.
#[derive(Clone, Debug, Default)]
pub struct LibraryTally {
    revision: u64,
    parts: Parts,
    counts: LibraryCounts,
    dirs: Interner<Arc<str>, ()>,
    days: Interner<Arc<str>, ()>,
    /// The keyword lists photos carry (each as they write it), with the levels each names.
    sets: Interner<Arc<[String]>, Levels>,
    levels: HashMap<Arc<str>, Level>,
    /// A keyword level whose first spelling is not known for sure ([`LibraryTally::exact`]).
    unsure: bool,
    folders: u64,
    dates: u64,
    keywords: u64,
    /// Which counting of every photo these counts come from (a tree kept from another one can't
    /// be patched from the changes below).
    epoch: u64,
    /// The folders photos came into or went from since the counts were made, oldest first (the
    /// last [`MAX_FOLDER_LOG`] or so): `+1` a photo came, `-1` one went.
    folder_log: Vec<(Arc<str>, i8)>,
    /// How many changes were let go from the front of `folder_log`.
    folder_log_base: u64,
    /// Whether counting every photo is under way (nothing is logged then).
    counting: bool,
}

/// The most changes to the photos per folder a tally remembers for patching the folder tree
/// ([`LibraryTally::patch_folder_tree`]); the oldest half goes when there are more.
const MAX_FOLDER_LOG: usize = 8192;

/// The most folders a patch of the folder tree is made for; past this, building it again costs
/// no more.
const MAX_PATCHED_FOLDERS: usize = 512;

/// What a folder tree built from a tally ([`LibraryTally::folder_tree_kept`]) needs to be patched
/// from it later ([`LibraryTally::patch_folder_tree`]).
#[derive(Clone, Debug, Default)]
pub struct FolderUpkeep {
    epoch: u64,
    /// How many changes to the folders of the tally the tree has.
    seen: u64,
    /// The folders written in more than one way (see [`Catalog::patch_folder_tree_of`]).
    mixed: HashSet<String>,
}

impl LibraryTally {
    /// Counted from every photo.
    pub fn of(cat: &Catalog) -> LibraryTally {
        let mut t = LibraryTally {
            revision: cat.revision,
            folders: stamp(),
            dates: stamp(),
            keywords: stamp(),
            epoch: stamp(),
            counting: true,
            ..Default::default()
        };
        if let (Some((first, _)), Some((last, _))) = (cat.photos.first_key_value(), cat.photos.last_key_value()) {
            t.parts = Parts::spanning(*first, *last, cat.len());
        }
        let mut chains = Chains::new();
        for p in cat.photos() {
            t.replace(p.id, fresh(p), &mut chains);
        }
        t.counting = false;
        t
    }

    /// The catalog revision the counts are of.
    pub fn revision(&self) -> u64 {
        self.revision
    }

    /// Bring the counts up to `cat` as it is now from the photos changed since: the photos that
    /// were taken into account (none when no photo changed). `None` when that can't be done (more
    /// than [`MAX_FOLLOWED`] photos changed, or what changed is not known): count whole then; the
    /// counts are left as they were.
    ///
    /// `cat` has to be the catalog the counts were made of, further along.
    pub fn follow(&mut self, cat: &Catalog) -> Option<Vec<PhotoId>> {
        if self.revision == cat.revision {
            return Some(Vec::new());
        }
        let changed = cat.photos_changed_since(self.revision, READS)?;
        if changed.len() > MAX_FOLLOWED {
            return None;
        }
        let mut chains = Chains::new();
        for id in &changed {
            self.replace(*id, cat.photo(*id).and_then(|p| fresh(p)), &mut chains);
        }
        self.revision = cat.revision;
        Some(changed)
    }

    /// Whether the counts are what counting every photo gives. Not so in one case: a keyword
    /// written in two ways (`Body`, `body`) is shown as the photo with the lowest id writes it,
    /// and when that photo goes, which of the others comes first is not known photo by photo.
    /// Count whole then.
    pub fn exact(&self) -> bool {
        !self.unsure
    }

    pub fn counts(&self) -> LibraryCounts {
        self.counts
    }

    /// [`Catalog::folder_tree`]: the folder labels are `cat`'s.
    pub fn folder_tree(&self, cat: &Catalog) -> Vec<FolderNode> {
        cat.folder_tree_of(by_name(&self.dirs))
    }

    /// [`LibraryTally::folder_tree`], with what [`LibraryTally::patch_folder_tree`] needs to bring
    /// it up to date later.
    pub fn folder_tree_kept(&self, cat: &Catalog) -> (Vec<FolderNode>, FolderUpkeep) {
        let (tree, mixed) = cat.folder_tree_and_mixed_of(by_name(&self.dirs));
        (tree, FolderUpkeep { epoch: self.epoch, seen: self.folder_log_base + self.folder_log.len() as u64, mixed })
    }

    /// `tree` (made by [`LibraryTally::folder_tree_kept`] from this tally, or patched by this
    /// function, as of the `upkeep` it came with) brought up to date from the changes made to the
    /// tally since: exactly what building the tree again gives, at the cost of the folders that
    /// changed, not of every folder of the library. `None` when it can't be told apart cheaply
    /// (counted whole again since, too many changes, a folder's spelling…): build it again. The
    /// folder labels are as `cat`'s records were when `tree` was built.
    pub fn patch_folder_tree(&self, cat: &Catalog, tree: Vec<FolderNode>, upkeep: &mut FolderUpkeep) -> Option<Vec<FolderNode>> {
        let seen = usize::try_from(upkeep.seen.checked_sub(self.folder_log_base)?).ok()?;
        if upkeep.epoch != self.epoch || seen > self.folder_log.len() {
            return None;
        }
        let mut net: HashMap<&str, isize> = HashMap::new();
        for (dir, d) in self.folder_log.get(seen..)? {
            *net.entry(dir).or_default() += isize::from(*d);
        }
        net.retain(|_, d| *d != 0);
        if net.len() > MAX_PATCHED_FOLDERS {
            return None;
        }
        let changes: Vec<(&str, isize)> = net.into_iter().collect();
        let tree = cat.patch_folder_tree_of(tree, &mut upkeep.mixed, &changes)?;
        upkeep.seen = self.folder_log_base + self.folder_log.len() as u64;
        Some(tree)
    }

    /// [`Catalog::date_groups`].
    pub fn date_groups(&self) -> Vec<DateGroup> {
        crate::query::date_groups_of(by_name(&self.days))
    }

    /// [`Catalog::keyword_tree`]: the keyword list is `cat`'s.
    pub fn keyword_tree(&self, cat: &Catalog) -> Vec<KeywordNode> {
        let counted = self.levels.iter().filter_map(|(lower, l)| l.path().map(|p| (&**lower, l.photos, &**p)));
        crate::keywords::keyword_tree_of(cat.listed_keywords(), counted)
    }

    /// Changes whenever the photos counted per folder do (a tree built from them is right until then).
    pub fn folders_stamp(&self) -> u64 {
        self.folders
    }

    /// Changes whenever the photos counted per day do.
    pub fn dates_stamp(&self) -> u64 {
        self.dates
    }

    /// Changes whenever the photos counted per keyword level do.
    pub fn keywords_stamp(&self) -> u64 {
        self.keywords
    }

    /// Photo `id` adds `new` now (`None`: nothing, or it is gone): what it added before is taken
    /// out. Only what differs is touched, so the trees of what didn't change are kept.
    fn replace<'c>(&mut self, id: PhotoId, new: Option<Fresh<'c>>, chains: &mut Chains<'c>) {
        let old = self.parts.take(id);
        let n = new.as_ref();
        for (count, was, is) in [
            (&mut self.counts.total, old.is_some_and(|o| o.library), n.is_some_and(|n| n.library)),
            (&mut self.counts.picks, old.is_some_and(|o| o.pick), n.is_some_and(|n| n.pick)),
            (&mut self.counts.deleted, old.is_some_and(|o| o.trashed), n.is_some_and(|n| n.trashed)),
        ] {
            *count = count.saturating_sub(usize::from(was)).saturating_add(usize::from(is));
        }
        let od = old.and_then(|o| o.dir);
        let left = if self.counting { None } else { od.and_then(|d| self.dirs.slot(d)).map(|s| Arc::clone(&s.key)) };
        let (dir, moved) = rekey(&mut self.dirs, od, n.and_then(|n| n.dir));
        if moved {
            self.folders = stamp();
            if !self.counting {
                let came = dir.and_then(|d| self.dirs.slot(d)).map(|s| Arc::clone(&s.key));
                self.folder_log.extend(left.map(|d| (d, -1)));
                self.folder_log.extend(came.map(|d| (d, 1)));
                if self.folder_log.len() > MAX_FOLDER_LOG {
                    let gone = self.folder_log.len() / 2;
                    self.folder_log.drain(..gone);
                    self.folder_log_base += gone as u64;
                }
            }
        }
        let (day, moved) = rekey(&mut self.days, old.and_then(|o| o.day), n.and_then(|n| n.day));
        if moved {
            self.dates = stamp();
        }
        // (the new keyword list is held before the old one is let go, so one both have is never gone in between)
        let (old_set, levels) = (old.and_then(|o| o.set), &self.levels);
        let set = match n {
            Some(n) if !n.keywords.is_empty() => {
                self.sets.hold(n.keywords, || Some((Arc::from(n.keywords), levels_of_list(n.keywords, chains, levels)?)))
            }
            _ => None,
        };
        let levels_of = |set: Option<Id>| set.and_then(|s| self.sets.slot(s)).map(|s| Arc::clone(&s.extra));
        let (old_levels, new_levels) = (levels_of(old_set), levels_of(set));
        let none: &[(Arc<str>, Arc<str>)] = &[];
        let (ol, nl) = (old_levels.as_deref().unwrap_or(none), new_levels.as_deref().unwrap_or(none));
        if old_set != set && ol != nl {
            for (lower, path) in ol.iter().filter(|l| !nl.contains(l)) {
                self.level_lost(lower, path, id);
            }
            for (lower, path) in nl.iter().filter(|l| !ol.contains(l)) {
                self.level_gained(lower, path, id);
            }
            self.keywords = stamp();
        }
        if let Some(s) = old_set {
            self.sets.release(s);
        }
        if let Some(n) = n {
            self.parts.put(id, Part { library: n.library, pick: n.pick, trashed: n.trashed, dir, day, set });
        }
    }

    /// Photo `id` carries keyword level `lower`, written `path`.
    fn level_gained(&mut self, lower: &Arc<str>, path: &Arc<str>, id: PhotoId) {
        let level = self.levels.entry(Arc::clone(lower)).or_default();
        level.photos += 1;
        match level.spellings.iter_mut().find(|s| s.path == *path) {
            // (a first photo that went is not known any more: it stays so)
            Some(s) => {
                s.photos += 1;
                s.first = s.first.map(|f| f.min(id));
            }
            None => level.spellings.push(Spelling { path: Arc::clone(path), photos: 1, first: Some(id) }),
        }
        self.unsure |= level.unsure();
    }

    /// Photo `id` no longer carries keyword level `lower` written `path`.
    fn level_lost(&mut self, lower: &Arc<str>, path: &Arc<str>, id: PhotoId) {
        let Some(level) = self.levels.get_mut(lower) else { return };
        level.photos = level.photos.saturating_sub(1);
        if let Some(i) = level.spellings.iter().position(|s| s.path == *path)
            && let Some(s) = level.spellings.get_mut(i)
        {
            s.photos = s.photos.saturating_sub(1);
            if s.photos == 0 {
                level.spellings.swap_remove(i);
            } else if s.first == Some(id) {
                s.first = None;
            }
        }
        self.unsure |= level.unsure();
        if level.photos == 0 {
            self.levels.remove(lower);
        }
    }
}

/// Each keyword as written → its levels ([`crate::keywords::levels_of`]), worked out once: a
/// large library repeats a few thousand keywords tens of thousands of times.
type Chains<'c> = HashMap<&'c str, Vec<(String, String)>>;

/// Photo moves from folder (or day) `old` to the one named `new`: the number of the one it is in now,
/// and whether that is another one (the old one goes with its last photo).
fn rekey(keys: &mut Interner<Arc<str>, ()>, old: Option<Id>, new: Option<&str>) -> (Option<Id>, bool) {
    let same = match (old, new) {
        (None, None) => true,
        (Some(o), Some(n)) => keys.slot(o).is_some_and(|s| *s.key == *n),
        _ => false,
    };
    if same {
        return (old, false);
    }
    if let Some(o) = old {
        keys.release(o);
    }
    (new.and_then(|n| keys.hold(n, || Some((Arc::from(n), ())))), true)
}

/// The keyword levels the list `keywords` carries: each once, by lower-case path, with the path as
/// its first keyword through that level writes it, shared with what `known` holds. `None` when it
/// names none (a list of keywords that are only separators).
fn levels_of_list<'c>(keywords: &'c [String], chains: &mut Chains<'c>, known: &HashMap<Arc<str>, Level>) -> Option<Levels> {
    let mut levels: Vec<(Arc<str>, Arc<str>)> = Vec::new();
    for k in keywords {
        let chain = chains.entry(k.as_str()).or_insert_with(|| crate::keywords::levels_of(k));
        for (lower, path) in chain.iter() {
            if !levels.iter().any(|(l, _)| **l == **lower) {
                levels.push(match known.get_key_value(lower.as_str()) {
                    Some((k, l)) => {
                        (k.clone(), l.spellings.iter().find(|s| *s.path == **path).map_or_else(|| Arc::from(path.as_str()), |s| s.path.clone()))
                    }
                    None => (Arc::from(lower.as_str()), Arc::from(path.as_str())),
                });
            }
        }
    }
    (!levels.is_empty()).then(|| Levels::from(levels))
}
