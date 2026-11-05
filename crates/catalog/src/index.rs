//! Lookups derived from the catalog's own data, so questions asked once per photo of a big library
//! don't scan a list each time: which stack a photo is in, whether a regular album holds it, which
//! photos have the same content, the lower-cased text the free-text search reads, and the text a smart album's Text
//! rule reads (each made for every photo the first time it is needed for every photo), which
//! photos have their file in a folder, which have a file of a given name. A view of one library
//! folder looks through the photos filed at or below it, not through every photo of the library.
//!
//! They are never saved (the on-disk format is unchanged). Each is built on first use from what the catalog holds (the free-text
//! search's text of a big library on a thread of its own, started when the library is opened: [`crate::search_text`]), so a catalog
//! read from a snapshot, replayed from the journal or cloned starts
//! without them and builds them when asked. Once built, [`crate::Catalog::apply`] keeps them up to
//! date: the ops that change stacks or album photos (`AddStack`, `RemoveStack`, `SetStack`,
//! `AddAlbum`, `RemoveAlbum`, `SetAlbumPhotos`) and those that add, remove or point a photo at
//! another file (`AddPhoto`, `RemovePhoto`, `Relink`, `SetFile`) update them as they apply,
//! inverses and the rollback of a failed batch included, as those are ops too. The contents index is kept up to
//! date, and the photos with a photo's content touched, by the ops that change a photo's content
//! as the library sees it, once a rule has built it or while a smart album asks about contents
//! ([`Catalog::follows_contents`], [`Catalog::content_moved`]). The two texts follow every op that changes
//! a photo's fields, and the photo moving to another place in memory.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, OnceLock};

use crate::query::key_within;
use crate::search_text::KeptText;
use crate::{AlbumId, Catalog, Photo, PhotoId, Source, StackId};

/// A folder's key, or a file name's, → the photos filed under it.
type FileIndex = HashMap<String, HashSet<PhotoId>>;

/// Below this many photos, building an index on several threads costs more than it saves.
const BUILD_ON_THREADS_FROM: usize = 1 << 16;

#[derive(Default)]
pub(crate) struct Indexes {
    /// Photo → the stack it is in; `None` when the catalog was read with a photo in two stacks
    /// (only a hand-edited file can hold that: ops refuse it), where a scan says which one counts.
    stacks: OnceLock<Option<HashMap<PhotoId, StackId>>>,
    /// Album → its photos (regular albums; a smart album's list is empty).
    albums: OnceLock<HashMap<AlbumId, HashSet<PhotoId>>>,
    /// Content ([`content_key`]) → the photos that have it.
    contents: OnceLock<HashMap<String, Vec<PhotoId>>>,
    /// Whether a smart album asks whether a photo has the same content as another ([`Catalog::follows_contents`]).
    contents_asked: OnceLock<bool>,
    /// The lower-cased text the free-text search looks through for every photo ([`PhotoText`]), and the making of it.
    search: KeptText,
    /// Photo → the lower-cased text a smart album's Text rule looks through ([`crate::rules::rule_text`]).
    rule_text: OnceLock<ByPhoto<RuleText>>,
    /// Every photo in each sort order, see [`crate::order`].
    pub(crate) orders: crate::order::Orders,
    /// Folder (its [`photo_folder_index_key`], lower-cased) → the photos whose file is directly in
    /// it, whatever their state (in the library, in Recently Deleted, browsed in Local). Demo
    /// photos have none.
    folders: OnceLock<HashMap<String, HashSet<PhotoId>>>,
    /// File name, lower-cased ([`file_name_index_key`]) → the photos whose file has it (same
    /// states as `folders`).
    names: OnceLock<HashMap<String, HashSet<PhotoId>>>,
    /// Photos queries have looked at ([`Catalog::photos_looked_at`]).
    looked_at: AtomicU64,
}

/// A copy builds its own when asked (copies are made to be saved, not queried).
impl Clone for Indexes {
    fn clone(&self) -> Self {
        Indexes::default()
    }
}

/// Derived from the rest of the catalog: two catalogs with the same data are equal whatever was
/// built.
impl PartialEq for Indexes {
    fn eq(&self, _: &Self) -> bool {
        true
    }
}

impl std::fmt::Debug for Indexes {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let stacks = self.stacks.get().map(|m| m.as_ref().map(HashMap::len));
        f.debug_struct("Indexes")
            .field("stacks", &stacks)
            .field("albums", &self.albums.get().map(HashMap::len))
            .field("folders", &self.folders.get().map(HashMap::len))
            .field("names", &self.names.get().map(HashMap::len))
            .finish()
    }
}

impl Indexes {
    /// Stack `id` held `old` and holds `new` now (empty: it was added, or is gone).
    pub(crate) fn stack_changed(&mut self, id: StackId, old: &[PhotoId], new: &[PhotoId]) {
        let Some(Some(m)) = self.stacks.get_mut() else { return };
        for p in old {
            if m.get(p) == Some(&id) {
                m.remove(p);
            }
        }
        for p in new {
            m.insert(*p, id);
        }
    }

    /// Album `id` holds `photos` now (`None`: it is gone).
    pub(crate) fn album_changed(&mut self, id: AlbumId, photos: Option<&[PhotoId]>) {
        let Some(m) = self.albums.get_mut() else { return };
        match photos {
            Some(photos) => {
                m.insert(id, photos.iter().copied().collect());
            }
            None => {
                m.remove(&id);
            }
        }
    }

    /// Photo `id` had content `old` and has `new` now (`None`: none the library counts).
    fn content_changed(&mut self, id: PhotoId, old: Option<&str>, new: Option<String>) {
        let Some(m) = self.contents.get_mut() else { return };
        if let Some(old) = old
            && let Some(ids) = m.get_mut(old)
        {
            ids.retain(|p| *p != id);
            if ids.is_empty() {
                m.remove(old);
            }
        }
        if let Some(new) = new {
            m.entry(new).or_default().push(id);
        }
    }

    /// An album came, went or got other rules: whether a smart album asks about contents is asked again.
    pub(crate) fn albums_changed(&mut self) {
        self.contents_asked = OnceLock::new();
    }

    /// Photo `id` is `p` now (`None`: it is gone): the text kept for it is made again.
    pub(crate) fn text_changed(&mut self, id: PhotoId, p: Option<&Photo>) {
        self.search.changed(id, p);
        if let Some(m) = self.rule_text.get_mut() {
            match p {
                Some(p) => m.insert(id, RuleText::of(p)),
                None => m.remove(&id),
            };
        }
    }

    /// Photo `id` is about to be changed in place, at `p`: an edit of a photo that others hold
    /// (`Arc::make_mut`) puts it at another address, which its text follows.
    pub(crate) fn text_moved(&mut self, id: PhotoId, p: &Photo) {
        self.search.moved(id, p);
        if let Some(t) = self.rule_text.get_mut().and_then(|m| m.get_mut(&id)) {
            t.at = addr(p);
        }
    }

    /// Photo `id`'s file was at `old` and is at `new` now (`None`: no file, or the photo was added
    /// or is gone).
    pub(crate) fn file_changed(&mut self, id: PhotoId, old: Option<&str>, new: Option<&str>) {
        if old == new {
            return;
        }
        if let Some(m) = self.folders.get_mut() {
            rekey(m, id, old.and_then(photo_folder_index_key), new.and_then(photo_folder_index_key));
        }
        if let Some(m) = self.names.get_mut() {
            rekey(m, id, old.and_then(file_name_index_key), new.and_then(file_name_index_key));
        }
    }
}

/// The content photo `p` stands for in the library: its file's hash, without the `:dup<id>` a Duplicate's carries.
/// `None` for a photo with no hash, a virtual copy (the same photo as its master, not another one), a photo in
/// Recently Deleted and one only browsed in Local.
pub(crate) fn content_key(p: &Photo) -> Option<&str> {
    if p.copy_of.is_some() || p.deleted || p.local {
        return None;
    }
    p.content_hash.as_deref().map(|h| h.split(':').next().unwrap_or(h)).filter(|h| !h.is_empty())
}

/// A map from photos, for a pass over the library that asks about each one in id order.
pub(crate) type ByPhoto<T> = HashMap<PhotoId, T, std::hash::BuildHasherDefault<IdHasher>>;

/// Hashes a photo id as itself in the table's low bits, which is where the table looks first, so
/// ids that follow each other sit next to each other and a pass in id order walks it in order
/// rather than jumping about (a pass over 300 000 photos spent most of its time on that). The top
/// bits, which tag an entry in the table, are mixed so entries still tell each other apart.
#[derive(Default)]
pub(crate) struct IdHasher(u64);

impl std::hash::Hasher for IdHasher {
    fn finish(&self) -> u64 {
        self.0 ^ (self.0.wrapping_mul(0x9E37_79B9_7F4A_7C15) & 0xFE00_0000_0000_0000)
    }

    fn write(&mut self, bytes: &[u8]) {
        for b in bytes {
            self.0 = self.0.rotate_left(8) ^ u64::from(*b);
        }
    }

    fn write_u64(&mut self, n: u64) {
        self.0 = n;
    }
}

/// Where `p` is in memory. The contract of the kept texts: a text is used only for the photo at the
/// address it was made for. The catalog's own photos (`Arc<Photo>` contents) stay where they are
/// until `Arc::make_mut` copies one that others hold, which `photo_mut` reports ([`Indexes::text_moved`]);
/// every op that changes a searched field makes the text again ([`Indexes::text_changed`]). A copy
/// of a photo (a candidate tried with other values) is at another address, so it is read from its
/// own fields, never from a text kept for the photo with its id. Only ids and addresses are
/// compared; the address is never turned back into a reference. A catalog read from a snapshot
/// (and then repaired) or cloned starts with no texts at all: they are made when asked.
pub(crate) fn addr(p: &Photo) -> usize {
    std::ptr::from_ref(p).addr()
}

/// What the free-text search looks through for one photo, lower-cased as `str::to_lowercase` does
/// for each field alone: file name, title, caption, camera, lens, location, format, then each
/// keyword, then the name of each person (a named face), separated by NUL. A search word may hold
/// white space (a quoted phrase); one that holds NUL is read from the fields one by one
/// ([`PhotoText::field_contains`]), so no word runs over a field.
pub(crate) struct PhotoText {
    /// The address of the photo this was made from ([`addr`]).
    pub(crate) at: usize,
    text: Box<str>,
}

impl PhotoText {
    /// The fields of `p` that the text joins before its keywords and people.
    fn fields(p: &Photo) -> [&String; 7] {
        [&p.file_name, &p.meta.title, &p.meta.caption, &p.meta.camera, &p.meta.lens, &p.meta.location, &p.format]
    }

    pub(crate) fn of(p: &Photo) -> PhotoText {
        let people = p.people();
        let mut text = String::with_capacity(text_len(p, &people));
        push_text(&mut text, p, &people);
        PhotoText { at: addr(p), text: text.into_boxed_str() }
    }

    /// The lower-cased text: the file name, title, caption, camera, lens, location, format, keywords and people, joined by NUL.
    pub(crate) fn text(&self) -> &str {
        &self.text
    }

    /// Whether the lower-cased word `t` is in one field of `p`, read from the fields one by one, for a word that holds NUL:
    /// the joined text can't tell the NUL that separates two fields from one inside a field, and a word never runs over two
    /// fields.
    pub(crate) fn field_contains(p: &Photo, t: &str) -> bool {
        Self::fields(p)
            .into_iter()
            .map(String::as_str)
            .chain(p.meta.keywords.iter().map(String::as_str))
            .chain(p.people())
            .any(|f| f.to_lowercase().contains(t))
    }
}

/// The length of the text of `p`, whose people are `people` ([`Photo::people`]). Exact for ASCII, which most text is: the box
/// takes the buffer without copying it. [`push_text`] pushes this much.
pub(crate) fn text_len(p: &Photo, people: &[&str]) -> usize {
    PhotoText::fields(p)
        .iter()
        .map(|f| f.len() + 1)
        .chain(p.meta.keywords.iter().map(|k| k.len() + 1))
        .chain(people.iter().map(|n| n.len() + 1))
        .sum::<usize>()
        .saturating_sub(1)
}

/// Push the text of `p`, whose people are `people`, onto `out`: each field lower-cased as `str::to_lowercase` does it alone,
/// with a NUL between every two.
pub(crate) fn push_text(out: &mut String, p: &Photo, people: &[&str]) {
    for (i, field) in PhotoText::fields(p).into_iter().enumerate() {
        if i > 0 {
            out.push('\0');
        }
        push_lowercase(out, field);
    }
    for k in p.meta.keywords.iter().map(String::as_str).chain(people.iter().copied()) {
        out.push('\0');
        push_lowercase(out, k);
    }
}

/// Push `s` lower-cased as `str::to_lowercase` gives it, without a temporary string for ASCII.
fn push_lowercase(out: &mut String, s: &str) {
    if s.is_ascii() {
        let from = out.len();
        out.push_str(s);
        if let Some(added) = out.get_mut(from..) {
            added.make_ascii_lowercase();
        }
    } else {
        out.push_str(&s.to_lowercase());
    }
}

/// A photo's text for a smart album's Text rule: [`crate::rules::rule_text`].
struct RuleText {
    at: usize,
    text: Box<str>,
}

impl RuleText {
    fn of(p: &Photo) -> RuleText {
        RuleText { at: addr(p), text: crate::rules::rule_text(p).into_boxed_str() }
    }
}

fn rekey(m: &mut HashMap<String, HashSet<PhotoId>>, id: PhotoId, old: Option<String>, new: Option<String>) {
    if let Some(k) = old
        && let Some(set) = m.get_mut(&k)
    {
        set.remove(&id);
        if set.is_empty() {
            m.remove(&k);
        }
    }
    if let Some(k) = new {
        m.entry(k).or_default().insert(id);
    }
}

/// Whether `path` is a plain POSIX path, its own identity under [`crate::query::folder_key`]: no
/// `\`, no repeated separator, no `.` or `..` part, no trailing separator.
fn plain_posix(path: &str) -> bool {
    !cfg!(windows)
        && path.starts_with('/')
        && !path.contains('\\')
        && !path.contains("//")
        && !path.contains("/./")
        && !path.contains("/../")
        && !path.ends_with('/')
        && !path.ends_with("/.")
        && !path.ends_with("/..")
}

/// The folder-index key of a file path: its folder as [`crate::folder_of`] spells it, lower-cased
/// (`None` for a bare relative name). A plain POSIX path is cut at its last `/` without building a
/// [`crate::query::folder_key`].
pub fn photo_folder_index_key(path: &str) -> Option<String> {
    let cut = |key: &str| key.rsplit_once('/').map(|(dir, _)| if dir.is_empty() { "/".to_string() } else { dir.to_lowercase() });
    if plain_posix(path) { cut(path) } else { cut(&crate::query::folder_key(path)) }
}

/// The folder-index key of a folder path (as [`photo_folder_index_key`] gives for the files in it).
pub fn folder_index_key(folder: &str) -> String {
    let key = crate::query::folder_key(folder).to_lowercase();
    // a disk's or share's root key ends in `/` (`c:/`, `//srv/share/`); its files' folder key doesn't
    match key.strip_suffix('/') {
        Some(k) if !k.is_empty() => k.to_string(),
        _ => key,
    }
}

/// The name-index key of a file path: its file name, lower-cased (`None`: it has none).
pub fn file_name_index_key(path: &str) -> Option<String> {
    // `/` and `\` both separate on every platform (a catalog can move between them), as for the folder key
    path.trim_end_matches(['/', '\\']).rsplit(['/', '\\']).next().filter(|n| !n.is_empty() && *n != "." && *n != "..").map(str::to_lowercase)
}

/// The path of a photo's file (`None` for a demo scene).
pub(crate) fn file_path(s: &Source) -> Option<&str> {
    match s {
        Source::File { path } => Some(path),
        Source::Demo { .. } => None,
    }
}

fn sorted(set: Option<&HashSet<PhotoId>>) -> Vec<PhotoId> {
    let mut v: Vec<PhotoId> = set.map(|s| s.iter().copied().collect()).unwrap_or_default();
    v.sort_unstable();
    v
}

/// How many threads an index may be built on (one where there are none: wasm32).
fn build_threads() -> usize {
    #[cfg(not(target_arch = "wasm32"))]
    {
        std::thread::available_parallelism().map_or(1, |n| n.get()).min(8)
    }
    #[cfg(target_arch = "wasm32")]
    {
        1
    }
}

/// The photos of `photos` that have a file, by the `key` of its path: one walk.
fn file_by_key<'a>(photos: impl Iterator<Item = &'a Arc<Photo>>, key: fn(&str) -> Option<String>) -> FileIndex {
    let mut m = FileIndex::new();
    for p in photos {
        if let Source::File { path } = &p.source
            && let Some(k) = key(path)
        {
            m.entry(k).or_default().insert(p.id);
        }
    }
    m
}

/// Every photo with a file by the `key` of its path: one walk of the library, or one walk of a run of it
/// on each of `threads` threads when there are `from` photos or more.
pub(crate) fn build(photos: &BTreeMap<PhotoId, Arc<Photo>>, key: fn(&str) -> Option<String>, from: usize, threads: usize) -> FileIndex {
    #[cfg(not(target_arch = "wasm32"))]
    {
        if let Some(m) = build_on_threads(photos, key, from, threads) {
            return m;
        }
    }
    #[cfg(target_arch = "wasm32")]
    let _ = (from, threads);
    file_by_key(photos.values(), key)
}

/// [`build`] on `threads` threads, each walking a run of the photos in id order. `None` when there are fewer
/// than `from` photos or only one thread, or when a thread could not be started or failed: one walk does it then.
#[cfg(not(target_arch = "wasm32"))]
pub(crate) fn build_on_threads(
    photos: &BTreeMap<PhotoId, Arc<Photo>>,
    key: fn(&str) -> Option<String>,
    from: usize,
    threads: usize,
) -> Option<FileIndex> {
    use std::collections::hash_map::Entry;
    use std::ops::Bound;

    if threads < 2 || photos.len() < from {
        return None;
    }
    // where each run starts: the photos are walked where they are, not copied into a list to cut
    let starts: Vec<PhotoId> = photos.keys().step_by(photos.len().div_ceil(threads).max(1)).copied().collect();
    let parts: Option<Vec<FileIndex>> = std::thread::scope(|sc| {
        let jobs: Vec<_> = starts
            .iter()
            .enumerate()
            .map(|(i, start)| {
                let to = starts.get(i + 1).map_or(Bound::Unbounded, |to| Bound::Excluded(*to));
                let run = photos.range((Bound::Included(*start), to)).map(|(_, p)| p);
                std::thread::Builder::new().spawn_scoped(sc, move || file_by_key(run, key)).ok()
            })
            .collect();
        // (every job is joined before any answer is looked at; one that did not start or panicked, which it can't, makes
        // the whole build `None`, and the caller then builds the index in one walk)
        let done: Vec<Option<FileIndex>> = jobs.into_iter().map(|job| job?.join().ok()).collect();
        done.into_iter().collect()
    });
    // a key's photos are in the run or runs that hold them: the runs are of different photos, so their sets add up
    let mut merged = FileIndex::new();
    for (k, ids) in parts?.into_iter().flatten() {
        match merged.entry(k) {
            Entry::Vacant(e) => {
                e.insert(ids);
            }
            Entry::Occupied(mut e) => e.get_mut().extend(ids),
        }
    }
    Some(merged)
}

impl Catalog {
    /// The stack photo `photo` is in, by the index (built on first use).
    pub(crate) fn stack_id_of(&self, photo: PhotoId) -> Option<StackId> {
        let m = self.index.stacks.get_or_init(|| {
            let mut m = HashMap::new();
            for s in self.stacks.values() {
                for p in &s.photos {
                    if m.insert(*p, s.id).is_some_and(|other| other != s.id) {
                        return None;
                    }
                }
            }
            Some(m)
        });
        match m {
            Some(m) => m.get(&photo).copied(),
            // in id order, the first stack listing the photo has it
            None => self.stacks.values().find(|s| s.photos.contains(&photo)).map(|s| s.id),
        }
    }

    /// Whether album `album`'s own list holds `photo`, by the index (built on first use).
    pub(crate) fn album_lists(&self, album: AlbumId, photo: PhotoId) -> bool {
        let m = self.index.albums.get_or_init(|| self.albums.values().map(|a| (a.id, a.photos.iter().copied().collect())).collect());
        m.get(&album).is_some_and(|s| s.contains(&photo))
    }

    /// Content → its photos, by the index (built on first use).
    fn contents(&self) -> &HashMap<String, Vec<PhotoId>> {
        self.index.contents.get_or_init(|| {
            let mut m: HashMap<String, Vec<PhotoId>> = HashMap::with_capacity(self.photos.len());
            for p in self.photos.values() {
                if let Some(k) = content_key(p) {
                    m.entry(k.to_string()).or_default().push(p.id);
                }
            }
            m
        })
    }

    /// Whether another photo of the library has the same content as `p` ([`content_key`]).
    pub fn has_same_content(&self, p: &Photo) -> bool {
        content_key(p).is_some_and(|k| self.contents().get(k).is_some_and(|ids| ids.iter().any(|id| *id != p.id)))
    }

    /// Whether ops have to follow photos' contents ([`Catalog::content_moved`]): the index was built (a rule
    /// asked), or a smart album asks (whose count may be followed from a copy of the catalog that built its own).
    /// Else nothing has asked, and a library that never does carries no index.
    pub(crate) fn follows_contents(&self) -> bool {
        self.index.contents.get().is_some()
            || *self.index.contents_asked.get_or_init(|| {
                self.albums.values().any(|a| a.smart.as_ref().and_then(|f| f.rule_set.as_ref()).is_some_and(|r| r.reads("sameContent")))
            })
    }

    /// The content photo `id` stands for now ([`content_key`]).
    pub(crate) fn content_of(&self, id: PhotoId) -> Option<String> {
        self.photos.get(&id).and_then(|p| content_key(p)).map(str::to_string)
    }

    /// The op just applied took photo `id` from content `old` to `new` ([`Catalog::follows_contents`]): the index
    /// follows, and the photos with either content are touched too ([`Catalog::touched_since`]), as whether they
    /// have the same content as another photo may have changed with it.
    pub(crate) fn content_moved(&mut self, id: PhotoId, old: Option<String>, new: Option<String>) {
        if old == new {
            return;
        }
        let built = self.index.contents.get().is_some();
        // (built now if it wasn't: from the catalog as it is, this op included, which holds the same other photos)
        let twins: Vec<PhotoId> =
            [&old, &new].into_iter().flatten().filter_map(|k| self.contents().get(k.as_str())).flatten().copied().filter(|p| *p != id).collect();
        if built {
            self.index.content_changed(id, old.as_deref(), new);
        }
        for p in twins {
            self.touch(p);
        }
    }

    /// `p`'s lower-cased text for the free-text search, when `p` is this catalog's own photo and the text is made. `pass`: a
    /// pass over the whole library asks (testing one photo shouldn't make it for 300 000): it waits for the thread that is making
    /// the text, if one is ([`crate::search_text`]), and makes it here if not.
    pub(crate) fn search_text(&self, p: &Photo, pass: bool) -> Option<&str> {
        self.index.search.get(&self.photos, pass)?.get(p)
    }

    /// Starts making the free-text search's text on another thread, when the library is big enough for making it to be felt and
    /// it isn't made or being made: the first search of a session then finds it made rather than waiting for all of it. A search
    /// that comes before it is done waits for the rest.
    pub fn prebuild_search_text(&self) {
        self.index.search.start(&self.photos);
    }

    /// Whether the search's text is made. A diagnostic: asking takes the text from its thread when that is done.
    #[doc(hidden)]
    pub fn search_text_ready(&self) -> bool {
        self.index.search.get(&self.photos, false).is_some()
    }

    #[cfg(test)]
    pub(crate) fn set_search_background_from(&mut self, photos: usize) {
        self.index.search.set_from(photos);
    }

    #[cfg(test)]
    pub(crate) fn hold_search_text(&self, f: impl Fn() + Send + Sync + 'static) {
        self.index.search.hold(std::sync::Arc::new(f));
    }

    #[cfg(test)]
    pub(crate) fn search_text_state(&self) -> &'static str {
        self.index.search.describe()
    }

    #[cfg(test)]
    pub(crate) fn search_text_spawned(&self) -> usize {
        self.index.search.spawned()
    }

    #[cfg(test)]
    pub(crate) fn search_text_again(&self) -> usize {
        self.index.search.again_len()
    }

    #[cfg(test)]
    pub(crate) fn refuse_search_threads(&mut self) {
        self.index.search.refuse_threads();
    }

    /// [`Catalog::search_text`] for a smart album's Text rule: the lower-cased, trimmed text of
    /// [`crate::rules::rule_text`], made for every photo on first use (a smart album's count or
    /// view asks for it photo by photo).
    pub(crate) fn rule_text_of(&self, p: &Photo) -> Option<&str> {
        let m = self.index.rule_text.get_or_init(|| self.photos.values().map(|p| (p.id, RuleText::of(p))).collect());
        let t = m.get(&p.id)?;
        (t.at == addr(p)).then_some(&*t.text)
    }

    /// Whether the Text rules' text has been made (asking for it makes it, so a test cannot look with
    /// [`Catalog::rule_text_of`]).
    #[cfg(test)]
    pub(crate) fn rule_text_made(&self) -> bool {
        self.index.rule_text.get().is_some()
    }

    /// The photos with a file by the `key` of its path, built on `threads` threads for a big library.
    fn file_index(&self, which: &str, key: fn(&str) -> Option<String>, threads: usize) -> FileIndex {
        let t = web_time::Instant::now();
        self.looked_at(self.photos.len());
        let m = build(&self.photos, key, BUILD_ON_THREADS_FROM, threads);
        log::info!("catalog: {which} index built for {} photos in {:.1} ms", self.photos.len(), t.elapsed().as_secs_f64() * 1e3);
        m
    }

    /// The photos by the folder their file is in, by the index (built on first use).
    fn folder_index(&self) -> &FileIndex {
        self.index.folders.get_or_init(|| self.file_index("folder", photo_folder_index_key, build_threads()))
    }

    /// The photos whose file is directly in the folder with key `folder_key` (from
    /// [`photo_folder_index_key`] or [`folder_index_key`]), whatever their state, sorted by id.
    /// The grouping ignores letter case: callers compare exact paths themselves. The first query
    /// builds the index in one pass over the library (on several threads for a big one); it costs about one set entry per photo.
    pub fn photos_in_folder_any_case(&self, folder_key: &str) -> Vec<PhotoId> {
        sorted(self.folder_index().get(folder_key))
    }

    /// The photos whose file is named `name_lower` (lower-cased, as [`file_name_index_key`] gives),
    /// whatever their state, sorted by id. The grouping ignores letter case: callers compare exact
    /// names themselves. The first query builds the index in one pass over the library; it costs
    /// about one set entry per photo.
    pub fn photos_named_any_case(&self, name_lower: &str) -> Vec<PhotoId> {
        // one walk: almost every photo has a name of its own, so on several threads the parts and the joined index are
        // held together (about 260 MiB more for 1.6 million photos, measured)
        sorted(self.index.names.get_or_init(|| self.file_index("name", file_name_index_key, 1)).get(name_lower))
    }

    /// The photo sets of the folders at or below any of the folders whose [`crate::query::folder_key`] are `roots`
    /// (every folder's when there are none), one set for each folder, whatever its photos' state: a folder below two
    /// of the roots is read once. The index files by lower-cased folder, so these are every photo the roots can show
    /// and, where two folders differ only in letter case, those of the other too: what a query makes of each photo is
    /// for its filter to say. The first call builds the index.
    pub(crate) fn folders_under<'a>(&'a self, roots: &[String]) -> impl Iterator<Item = &'a HashSet<PhotoId>> + 'a {
        let roots: Vec<String> = roots.iter().map(|r| r.to_lowercase()).collect();
        self.folder_index().iter().filter(move |(dir, _)| roots.is_empty() || roots.iter().any(|r| key_within(dir, r))).map(|(_, ids)| ids)
    }

    /// Whether the folders whose keys are `roots`, with what is below them, hold more than half of the library's
    /// photos: looking each of them up costs more than walking the library then.
    pub(crate) fn folder_holds_most(&self, roots: &[String]) -> bool {
        self.folders_under(roots).map(HashSet::len).sum::<usize>() > self.photos.len() / 2
    }

    /// How many photos the questions asked of the catalog have looked at so far, as a running total, for tests and
    /// benchmarks to read before and after an action: [`Catalog::query`], [`Catalog::query_keeping`],
    /// [`Catalog::count`], [`Catalog::any`] and [`Catalog::folder_samples`] add what they go through (the whole library;
    /// for a library folder only the photos filed at or below it, the library's size when they are more than half of
    /// it), and building an index (the first question it answers) adds the size of the library. Other walks over the
    /// library (tallies, date groups, the folder tree...) are not counted. A copy of the catalog starts again from 0.
    pub fn photos_looked_at(&self) -> u64 {
        self.index.looked_at.load(Ordering::Relaxed)
    }

    pub(crate) fn looked_at(&self, photos: usize) {
        self.index.looked_at.fetch_add(photos as u64, Ordering::Relaxed);
    }
}
