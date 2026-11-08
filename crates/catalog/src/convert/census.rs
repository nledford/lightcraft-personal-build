//! A census of a library, and the comparison of two: the proof that a swap, a conversion or a clone kept what the user keeps.
//!
//! The census is a JSON manifest of what this build's reader makes of a library: for every photo (keyed by the path of its
//! file, relative to the library's folder when it lies in it) its rating, flag, color label, keywords (full hierarchy
//! paths, sorted), albums, stack, whether it is edited and a hash of its develop settings; for every folder that has a color
//! label, the label; the albums with their smart rules, the custom label names, the keyword list and the totals; and the
//! format stamp and layout the library had. Keys are sorted, so two manifests of the same data are the same bytes.
//!
//! The data a user must keep above all is the color labels and keywords of photos and folders ([`Diff::must_keep`]).
//! [`diff`] lists every photo and folder whose label or keywords changed, appeared or disappeared first, then counts the
//! changes of every other field. Folders have no keywords in this build's catalog (a folder record holds a label and when
//! the folder was made), so a folder's census holds its label only.
//!
//! Read only: the census opens nothing for writing in the library and takes no lock on it (a library is read the way
//! [`super::dump`] reads it, in memory; one a program has open is read as the files stand, and may be refused by the open's
//! own checks), and the manifest goes to a new file outside the library, never over an existing one.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;
use std::fs::{self, OpenOptions};
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::{ConvertError, files, read};
use crate::{Album, Catalog, Photo, PhotoId, Source};

/// The census format; a manifest of another one is refused.
pub const CENSUS_FORMAT: u32 = 1;

/// How the library was stored; compared as context only (a conversion changes it on purpose).
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Stamp {
    /// The format number of `catalog.snap` (0: the library has only a log).
    pub stamp: u32,
    pub layout: String,
    /// The op the library is at.
    pub seq: u64,
}

/// Counts, worked out from the rest of the manifest.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Totals {
    pub photos: u64,
    pub deleted: u64,
    pub edited: u64,
    pub labeled_photos: u64,
    pub keyworded_photos: u64,
    /// Distinct keyword paths on photos.
    pub keywords_distinct: u64,
    pub labeled_folders: u64,
    /// Every folder record the library keeps (a made-only record has no label).
    pub folder_records: u64,
    pub albums: u64,
    pub smart_albums: u64,
    pub stacks: u64,
    pub listed_keywords: u64,
}

/// A photo's place in its stack.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct StackPlace {
    pub id: u64,
    /// 0 is the top.
    pub position: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PhotoEntry {
    pub id: u64,
    pub rating: u8,
    pub flag: String,
    pub label: Option<String>,
    /// Full hierarchy paths (`a|b|c`), sorted.
    pub keywords: Vec<String>,
    /// Ids of the regular albums that hold the photo, sorted.
    pub albums: Vec<u64>,
    pub stack: Option<StackPlace>,
    pub edited: bool,
    /// FNV-1a 64 of the develop settings as JSON.
    pub develop: String,
    pub deleted: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct FolderEntry {
    pub label: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct AlbumEntry {
    pub name: String,
    pub parent: Option<u64>,
    pub folder: bool,
    pub quick: bool,
    pub order: Option<u32>,
    pub cover: Option<u64>,
    /// In the album's order.
    pub photos: Vec<u64>,
    /// A smart album's rules.
    pub smart: Option<Value>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Manifest {
    pub census: u32,
    /// The library's folder when the census was made, as it is and as the file system spells it (a census of a copy tells
    /// which of its keys were the original's).
    pub roots: Vec<String>,
    pub library: Stamp,
    pub totals: Totals,
    pub photos: BTreeMap<String, PhotoEntry>,
    /// Folders with a color label, by the folder's path (relative to the library when it lies in it).
    pub folders: BTreeMap<String, FolderEntry>,
    /// By album id.
    pub albums: BTreeMap<String, AlbumEntry>,
    pub label_names: BTreeMap<String, String>,
    pub keyword_list: BTreeMap<String, Value>,
}

/// What [`write`] says of the census it made: counts only, never a name.
#[derive(Clone, Debug)]
pub struct Summary {
    pub out: PathBuf,
    pub stamp: Stamp,
    pub totals: Totals,
}

impl std::fmt::Display for Summary {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let t = &self.totals;
        writeln!(f, "census:  {}", self.out.display())?;
        writeln!(f, "was:     {}", self.stamp.layout)?;
        writeln!(
            f,
            "holds:   {} photos ({} deleted, {} edited, {} with a color label, {} with keywords, {} distinct keywords), {} folders with a color label ({} folder records), {} albums ({} smart), {} stacks, {} keywords listed",
            t.photos,
            t.deleted,
            t.edited,
            t.labeled_photos,
            t.keyworded_photos,
            t.keywords_distinct,
            t.labeled_folders,
            t.folder_records,
            t.albums,
            t.smart_albums,
            t.stacks,
            t.listed_keywords
        )
    }
}

// ---- making a census

/// Read the census of the library `library`: nothing is written, no lock is taken.
pub fn read(library: &Path) -> Result<Manifest, ConvertError> {
    let source = read::open(library)?;
    Ok(manifest(&source, &Root::of(library)))
}

/// Write the census of `library` to the new file `out` (outside the library, not existing). A failure leaves no file.
pub fn write(library: &Path, out: &Path) -> Result<Summary, ConvertError> {
    let out = files::check_file(library, out)?;
    let manifest = read(library)?;
    let io = |what: &str, e: &dyn std::fmt::Display| ConvertError::Io(format!("{what} {}: {e}", out.display()));
    let file = OpenOptions::new().write(true).create_new(true).open(&out).map_err(|e| io("can't create", &e))?;
    let mut w = BufWriter::new(file);
    let done = serde_json::to_writer_pretty(&mut w, &manifest)
        .map_err(|e| e.to_string())
        .and_then(|()| w.write_all(b"\n").map_err(|e| e.to_string()))
        .and_then(|()| w.flush().map_err(|e| e.to_string()))
        .and_then(|()| w.get_ref().sync_all().map_err(|e| e.to_string()));
    if let Err(e) = done {
        drop(w);
        // (the file is ours: it did not exist a moment ago)
        let _ = fs::remove_file(&out);
        return Err(io("can't write", &e));
    }
    Ok(Summary { out, stamp: manifest.library, totals: manifest.totals })
}

/// Where the library is, for naming a photo's file relative to it.
struct Root(Vec<PathBuf>);

impl Root {
    fn of(library: &Path) -> Root {
        let mut roots = Vec::new();
        if let Ok(canonical) = fs::canonicalize(library) {
            roots.push(canonical);
        }
        if !roots.iter().any(|r| r == library) {
            roots.push(library.to_path_buf());
        }
        Root(roots)
    }

    /// `path` relative to the library, with `/` between its names, when it lies in it; else as it is stored.
    fn relative(&self, path: &str) -> String {
        self.0.iter().find_map(|root| relative_to(root, path)).unwrap_or_else(|| path.to_string())
    }
}

/// `path` below `root` as names joined by `/`; `None` when it is not below it (or is it).
fn relative_to(root: &Path, path: &str) -> Option<String> {
    let rest = Path::new(path).strip_prefix(root).ok()?;
    let names: Vec<String> = rest.components().map(|c| c.as_os_str().to_string_lossy().into_owned()).collect();
    (!names.is_empty()).then(|| names.join("/"))
}

fn json<T: Serialize>(v: &T) -> Value {
    serde_json::to_value(v).unwrap_or_else(|e| Value::String(format!("unserializable: {e}")))
}

/// How a value is written as a word (`red`, `pick`).
fn word<T: Serialize>(v: &T) -> String {
    match json(v) {
        Value::String(s) => s,
        other => other.to_string(),
    }
}

/// FNV-1a 64: a hash that stays the same across builds and Rust versions, which a diff of two manifests needs.
fn fnv64(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0xcbf2_9ce4_8422_2325, |h, b| (h ^ u64::from(*b)).wrapping_mul(0x0100_0000_01b3))
}

/// The key of every photo: its file's path (relative to the library when it lies in it); a path that more than one photo
/// has (a virtual copy shares its original's file) gets `#<id>` so each photo has a key of its own.
fn photo_keys(catalog: &Catalog, root: &Root) -> BTreeMap<PhotoId, String> {
    let bases: Vec<(PhotoId, String)> = catalog
        .photos()
        .map(|p| {
            let base = match &p.source {
                Source::File { path } => root.relative(path),
                Source::Demo { scene } => format!("demo:{scene}"),
            };
            (p.id, base)
        })
        .collect();
    let mut seen: BTreeMap<&str, usize> = BTreeMap::new();
    for (_, base) in &bases {
        *seen.entry(base.as_str()).or_default() += 1;
    }
    let shared: BTreeSet<String> = seen.into_iter().filter(|(_, n)| *n > 1).map(|(b, _)| b.to_string()).collect();
    bases
        .into_iter()
        .map(|(id, base)| {
            let key = if shared.contains(&base) { format!("{base}#{}", id.0) } else { base };
            (id, key)
        })
        .collect()
}

/// The regular albums each photo is in, by album id.
fn memberships(catalog: &Catalog) -> BTreeMap<PhotoId, BTreeSet<u64>> {
    let mut of: BTreeMap<PhotoId, BTreeSet<u64>> = BTreeMap::new();
    for album in catalog.albums() {
        for photo in &album.photos {
            of.entry(*photo).or_default().insert(album.id.0);
        }
    }
    of
}

fn photo_entry(catalog: &Catalog, photo: &Photo, albums: Option<&BTreeSet<u64>>) -> PhotoEntry {
    let mut keywords = photo.meta.keywords.clone();
    keywords.sort();
    let develop = serde_json::to_vec(&*photo.develop).map_or_else(|e| format!("unserializable: {e}"), |bytes| format!("{:016x}", fnv64(&bytes)));
    PhotoEntry {
        id: photo.id.0,
        rating: photo.rating,
        flag: word(&photo.flag),
        label: photo.label.as_ref().map(word),
        keywords,
        albums: albums.map(|a| a.iter().copied().collect()).unwrap_or_default(),
        stack: catalog.stack_of(photo.id).and_then(|s| {
            let position = s.position(photo.id)?;
            Some(StackPlace { id: s.id.0, position: position as u64 })
        }),
        edited: photo.is_edited(),
        develop,
        deleted: photo.deleted,
    }
}

fn album_entry(album: &Album) -> AlbumEntry {
    AlbumEntry {
        name: album.name.clone(),
        parent: album.parent.map(|p| p.0),
        folder: album.folder,
        quick: album.quick,
        order: album.order,
        cover: album.cover.map(|c| c.0),
        photos: album.photos.iter().map(|p| p.0).collect(),
        smart: album.smart.as_deref().map(json),
    }
}

fn manifest(source: &read::Source, root: &Root) -> Manifest {
    let catalog = &source.catalog;
    let keys = photo_keys(catalog, root);
    let in_albums = memberships(catalog);
    let mut photos = BTreeMap::new();
    for photo in catalog.photos() {
        let key = keys.get(&photo.id).cloned().unwrap_or_else(|| format!("#{}", photo.id.0));
        photos.insert(key, photo_entry(catalog, photo, in_albums.get(&photo.id)));
    }
    let folders: BTreeMap<String, FolderEntry> = catalog
        .folder_records
        .iter()
        .filter_map(|(key, record)| Some((root.relative(key), FolderEntry { label: word(record.label.as_ref()?) })))
        .collect();
    let albums: BTreeMap<String, AlbumEntry> = catalog.albums().map(|a| (a.id.0.to_string(), album_entry(a))).collect();
    let keywords: BTreeSet<&str> = photos.values().flat_map(|p| p.keywords.iter().map(String::as_str)).collect();
    let totals = Totals {
        photos: photos.len() as u64,
        deleted: photos.values().filter(|p| p.deleted).count() as u64,
        edited: photos.values().filter(|p| p.edited).count() as u64,
        labeled_photos: photos.values().filter(|p| p.label.is_some()).count() as u64,
        keyworded_photos: photos.values().filter(|p| !p.keywords.is_empty()).count() as u64,
        keywords_distinct: keywords.len() as u64,
        labeled_folders: folders.len() as u64,
        folder_records: catalog.folder_records.len() as u64,
        albums: albums.len() as u64,
        smart_albums: albums.values().filter(|a| a.smart.is_some()).count() as u64,
        stacks: catalog.stacks().count() as u64,
        listed_keywords: catalog.listed_keywords().count() as u64,
    };
    Manifest {
        census: CENSUS_FORMAT,
        roots: root.0.iter().map(|r| r.to_string_lossy().into_owned()).collect(),
        library: Stamp { stamp: source.stamp, layout: source.layout.clone(), seq: source.seq },
        totals,
        photos,
        folders,
        albums,
        label_names: catalog.label_names.iter().map(|(l, n)| (word(l), n.clone())).collect(),
        keyword_list: catalog.listed_keywords().map(|k| (k.path.clone(), json(k))).collect(),
    }
}

// ---- comparing two

/// One difference of the data a user must keep.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Change {
    /// `photo.label`, `photo.keywords`, `photo.appeared`, `photo.disappeared`, `folder.label`, `folder.appeared`,
    /// `folder.disappeared`.
    pub kind: &'static str,
    /// The photo's or folder's key.
    pub subject: String,
    /// What changed (`red -> blue`, `-a|b, +c`).
    pub detail: String,
}

/// What two censuses differ in.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Diff {
    /// The labels and keywords of photos and folders, and photos and folders that appeared or disappeared.
    pub must_keep: Vec<Change>,
    /// Every other changed field: its kind and how many records changed.
    pub other: BTreeMap<&'static str, u64>,
    /// What the two were stored as, and the totals that differ (a conversion changes them on purpose): not counted.
    pub context: Vec<String>,
}

impl Diff {
    /// Whether anything of what a user must keep differs.
    pub fn differs(&self) -> bool {
        !self.must_keep.is_empty()
    }

    fn must(&mut self, kind: &'static str, subject: &str, detail: String) {
        self.must_keep.push(Change { kind, subject: subject.to_string(), detail });
    }

    /// Count `n` more records of `kind` (nothing when `n` is 0).
    fn also(&mut self, kind: &'static str, n: usize) {
        if n > 0 {
            *self.other.entry(kind).or_default() += n as u64;
        }
    }

    /// The kinds of must-keep differences with how many there are of each.
    pub fn must_keep_counts(&self) -> BTreeMap<&'static str, u64> {
        let mut counts = BTreeMap::new();
        for change in &self.must_keep {
            *counts.entry(change.kind).or_default() += 1;
        }
        counts
    }

    /// The report: the must-keep differences first, then the counts of the other fields, then the context. With
    /// `counts_only` no photo, folder, label or keyword is named: kinds and counts only.
    pub fn report(&self, counts_only: bool) -> String {
        let mut out = String::new();
        let _ = self.write_report(&mut out, counts_only);
        out
    }

    fn write_report(&self, out: &mut String, counts_only: bool) -> std::fmt::Result {
        if self.must_keep.is_empty() {
            writeln!(out, "must keep (labels and keywords of photos and folders): no differences")?;
        } else {
            writeln!(out, "MUST KEEP (labels and keywords of photos and folders): {} difference(s)", self.must_keep.len())?;
            for (kind, n) in self.must_keep_counts() {
                writeln!(out, "  {kind}: {n}")?;
            }
            if !counts_only {
                for c in &self.must_keep {
                    writeln!(out, "  {} {}: {}", c.kind, c.subject, c.detail)?;
                }
            }
        }
        if self.other.is_empty() {
            writeln!(out, "other fields: no differences")?;
        } else {
            writeln!(out, "other fields:")?;
            for (kind, n) in &self.other {
                writeln!(out, "  {kind}: {n}")?;
            }
        }
        for line in &self.context {
            writeln!(out, "context: {line}")?;
        }
        writeln!(out, "result: {}", if self.differs() { "DIFFERENT" } else { "same" })
    }
}

/// How a label shows (`none` for no label).
fn shown(label: Option<&str>) -> &str {
    label.unwrap_or("none")
}

fn keyword_detail(a: &[String], b: &[String]) -> String {
    let (a, b): (BTreeSet<&str>, BTreeSet<&str>) = (a.iter().map(String::as_str).collect(), b.iter().map(String::as_str).collect());
    let mut parts: Vec<String> = a.difference(&b).map(|k| format!("-{k}")).collect();
    parts.extend(b.difference(&a).map(|k| format!("+{k}")));
    if parts.is_empty() {
        // (the same set: a keyword twice, or the order)
        parts.push("same keywords, listed differently".to_string());
    }
    parts.join(", ")
}

fn photo_changes(key: &str, a: &PhotoEntry, b: &PhotoEntry, d: &mut Diff) {
    if a.label != b.label {
        d.must("photo.label", key, format!("{} -> {}", shown(a.label.as_deref()), shown(b.label.as_deref())));
    }
    if a.keywords != b.keywords {
        d.must("photo.keywords", key, keyword_detail(&a.keywords, &b.keywords));
    }
    let others = [
        ("photo.id", a.id != b.id),
        ("photo.rating", a.rating != b.rating),
        ("photo.flag", a.flag != b.flag),
        ("photo.albums", a.albums != b.albums),
        ("photo.stack", a.stack != b.stack),
        ("photo.edited", a.edited != b.edited),
        ("photo.develop", a.develop != b.develop),
        ("photo.deleted", a.deleted != b.deleted),
    ];
    for (kind, changed) in others {
        d.also(kind, usize::from(changed));
    }
}

fn album_changes(a: &AlbumEntry, b: &AlbumEntry, d: &mut Diff) {
    let others = [
        ("album.properties", (&a.name, a.parent, a.folder, a.quick, a.order, a.cover) != (&b.name, b.parent, b.folder, b.quick, b.order, b.cover)),
        ("album.photos", a.photos != b.photos),
        ("album.rules", a.smart != b.smart),
    ];
    for (kind, changed) in others {
        d.also(kind, usize::from(changed));
    }
}

/// The keys two censuses share (with both values) and those only one has.
struct Delta<'a, V> {
    both: Vec<(&'a str, &'a V, &'a V)>,
    only_a: Vec<&'a str>,
    only_b: Vec<&'a str>,
}

fn delta<'a, V>(a: &'a BTreeMap<String, V>, b: &'a BTreeMap<String, V>) -> Delta<'a, V> {
    let mut d = Delta { both: Vec::new(), only_a: Vec::new(), only_b: Vec::new() };
    for (key, x) in a {
        match b.get(key) {
            Some(y) => d.both.push((key.as_str(), x, y)),
            None => d.only_a.push(key.as_str()),
        }
    }
    d.only_b.extend(b.keys().filter(|k| !a.contains_key(*k)).map(String::as_str));
    d
}

/// What the totals say differs, as `name a -> b`.
fn totals_context(a: &Totals, b: &Totals) -> Vec<String> {
    let rows = [
        ("photos", a.photos, b.photos),
        ("deleted", a.deleted, b.deleted),
        ("edited", a.edited, b.edited),
        ("labeled_photos", a.labeled_photos, b.labeled_photos),
        ("keyworded_photos", a.keyworded_photos, b.keyworded_photos),
        ("keywords_distinct", a.keywords_distinct, b.keywords_distinct),
        ("labeled_folders", a.labeled_folders, b.labeled_folders),
        ("folder_records", a.folder_records, b.folder_records),
        ("albums", a.albums, b.albums),
        ("smart_albums", a.smart_albums, b.smart_albums),
        ("stacks", a.stacks, b.stacks),
        ("listed_keywords", a.listed_keywords, b.listed_keywords),
    ];
    rows.into_iter().filter(|(_, x, y)| x != y).map(|(name, x, y)| format!("totals.{name} {x} -> {y}")).collect()
}

/// The entries of a census by key, a key that is a path inside the other library's folder (`other_roots`) made relative to it:
/// a copy of a library keeps pointing at the originals the library held, which the library names relative to itself.
fn rebased<'a, V>(map: &'a BTreeMap<String, V>, other_roots: &[String]) -> BTreeMap<String, &'a V> {
    map.iter().map(|(key, v)| (other_roots.iter().find_map(|root| relative_to(Path::new(root), key)).unwrap_or_else(|| key.clone()), v)).collect()
}

/// What `b` differs from `a` in.
pub fn diff(a: &Manifest, b: &Manifest) -> Diff {
    let mut d = Diff::default();

    let (photos_a, photos_b) = (rebased(&a.photos, &b.roots), rebased(&b.photos, &a.roots));
    let photos = delta(&photos_a, &photos_b);
    for key in photos.only_a {
        d.must("photo.disappeared", key, "in the first census only".to_string());
    }
    for key in photos.only_b {
        d.must("photo.appeared", key, "in the second census only".to_string());
    }
    for (key, x, y) in photos.both {
        photo_changes(key, x, y, &mut d);
    }

    let (folders_a, folders_b) = (rebased(&a.folders, &b.roots), rebased(&b.folders, &a.roots));
    let folders = delta(&folders_a, &folders_b);
    for key in folders.only_a {
        d.must("folder.disappeared", key, "in the first census only".to_string());
    }
    for key in folders.only_b {
        d.must("folder.appeared", key, "in the second census only".to_string());
    }
    for (key, x, y) in folders.both.into_iter().filter(|(_, x, y)| x.label != y.label) {
        d.must("folder.label", key, format!("{} -> {}", x.label, y.label));
    }

    let albums = delta(&a.albums, &b.albums);
    d.also("album.disappeared", albums.only_a.len());
    d.also("album.appeared", albums.only_b.len());
    for (_, x, y) in albums.both {
        album_changes(x, y, &mut d);
    }
    d.also("catalog.label_names", usize::from(a.label_names != b.label_names));
    let listed = delta(&a.keyword_list, &b.keyword_list);
    d.also("catalog.keyword_list", listed.only_a.len() + listed.only_b.len() + listed.both.iter().filter(|(_, x, y)| x != y).count());

    if a.library.stamp != b.library.stamp {
        d.context.push(format!("format {} -> {}", a.library.stamp, b.library.stamp));
    }
    if a.library.layout != b.library.layout {
        d.context.push(format!("layout `{}` -> `{}`", a.library.layout, b.library.layout));
    }
    if a.library.seq != b.library.seq {
        d.context.push(format!("seq {} -> {}", a.library.seq, b.library.seq));
    }
    d.context.extend(totals_context(&a.totals, &b.totals));
    d
}

/// A census file, as [`write`] made it.
pub fn load(path: &Path) -> Result<Manifest, ConvertError> {
    let bytes = fs::read(path).map_err(|e| ConvertError::Io(format!("can't read {}: {e}", path.display())))?;
    let bad = |why: String| ConvertError::Unreadable(format!("{} is not a census this build reads: {why}", path.display()));
    let value: Value = serde_json::from_slice(&bytes).map_err(|e| bad(e.to_string()))?;
    match value.get("census").and_then(Value::as_u64) {
        Some(n) if n == u64::from(CENSUS_FORMAT) => {}
        Some(n) => return Err(bad(format!("it is census format {n}, this build reads {CENSUS_FORMAT}"))),
        None => return Err(bad("it has no `census` format number".to_string())),
    }
    serde_json::from_value(value).map_err(|e| bad(e.to_string()))
}

/// Compare two census files.
pub fn diff_files(a: &Path, b: &Path) -> Result<Diff, ConvertError> {
    Ok(diff(&load(a)?, &load(b)?))
}
