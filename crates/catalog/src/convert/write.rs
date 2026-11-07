//! Writing the copy's catalog files in the target's format, and reading them back.
//!
//! The two formats are written here, not by [`crate::journal`]: that writes this build's (15) and
//! the build's own defaults. What each one is:
//!
//! - **Official LightCraft (12)**: `catalog.snap` is `{"format","version":12,"seq","catalog"}` and a
//!   line feed; the catalog holds every photo whole (`Photo`'s own JSON), with each distinct
//!   segmentation grid once in `grids` (the shared grids are what format 12 adds). No `defaults`,
//!   `segments` or `photos` count, no segments folder. [`OfficialCatalog`] is main's `Catalog`
//!   field for field.
//! - **Build 7248 (11)**: the header also states the defaults of that build's develop settings
//!   (this build's without the one setting it doesn't have), the catalog holds no photos, and the
//!   photos are in the segment files the header lists, with compact records
//!   (`"version":11` in each segment file).
//!
//! Either way `catalog.log` is empty: the catalog is at `seq`, whole.

use std::collections::{BTreeMap, HashMap};
use std::fs;
use std::io::{self, BufWriter, Write};
use std::path::Path;
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::{ConvertError, Target};
use crate::journal::{Journal, LOG, SNAPSHOT, SnapshotPolicy};
use crate::keywords::ListedKeyword;
use crate::segments::{self, OnDisk, Photos, SEGMENT_DIR, SEGMENT_PREFIX, Segment, Size};
use crate::store::{FsStore, MemStore, Store};
use crate::{Album, AlbumId, Catalog, ColorLabel, FolderRecord, Photo, PhotoId, Stack, StackId, stored};

/// `catalog.snap`'s `format`.
const FORMAT: &str = "lightcraft-catalog";

/// Write the catalog files of `catalog` (at op `seq`) into the folder `dir`. `record_bytes`: what a photo record
/// took in the library it was read from (0: not known), to size the segments.
pub(super) fn catalog(dir: &Path, target: Target, catalog: &Catalog, seq: u64, record_bytes: u64) -> io::Result<()> {
    let mut store = FsStore::open(dir)?;
    let listed = match target {
        Target::Official => Vec::new(),
        Target::Build7248 => {
            let policy = SnapshotPolicy::default();
            let size = Size { per: policy.segment_photos, bytes: policy.segment_bytes, record_bytes };
            let baseline = Photos::new();
            let on_disk = OnDisk { version: target.format(), seq, segments: &[], baseline: &baseline, size, verify: true };
            segments::write(&mut store, catalog.photo_map(), &on_disk)?.segments
        }
    };
    // (the snapshot lists the segments and is the commit)
    store.write_atomic_with(SNAPSHOT, &mut |w| render(target, catalog, seq, &listed, w))?;
    store.write_atomic(LOG, b"")?;
    store.sync()
}

/// `catalog.snap` of `catalog` as `target` reads it.
fn render(target: Target, catalog: &Catalog, seq: u64, listed: &[Segment], w: &mut dyn Write) -> io::Result<()> {
    let mut b = BufWriter::with_capacity(1 << 20, w);
    match target {
        Target::Official => official(&mut b, catalog, seq)?,
        Target::Build7248 => build_7248(&mut b, catalog, seq, listed)?,
    }
    b.flush()
}

/// Main's `Catalog` (`origin/main`, `crates/catalog/src/lib.rs`), field for field in its order: what it serializes, by
/// the same names and the same rules for leaving a field out.
#[derive(Serialize)]
struct OfficialCatalog<'a> {
    photos: &'a BTreeMap<PhotoId, Arc<Photo>>,
    albums: &'a BTreeMap<AlbumId, Album>,
    next_photo: u64,
    next_album: u64,
    #[serde(skip_serializing_if = "no_entries")]
    stacks: &'a BTreeMap<StackId, Stack>,
    next_stack: u64,
    #[serde(skip_serializing_if = "no_entries")]
    label_names: &'a BTreeMap<ColorLabel, String>,
    #[serde(skip_serializing_if = "no_entries")]
    browsed: &'a BTreeMap<String, String>,
    #[serde(skip_serializing_if = "no_entries")]
    keyword_list: &'a BTreeMap<String, ListedKeyword>,
    #[serde(skip_serializing_if = "no_entries")]
    folder_records: &'a BTreeMap<String, FolderRecord>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    grids: Vec<String>,
}

fn no_entries<K, V>(m: &&BTreeMap<K, V>) -> bool {
    m.is_empty()
}

impl<'a> OfficialCatalog<'a> {
    /// `catalog` as main holds it, with the grids of its photos' settings gathered in `grids`: the pool, and the table the
    /// masks refer to by position (what `journal::write_with_shared_grids` does for a one-file snapshot).
    fn of(catalog: &'a Catalog) -> (OfficialCatalog<'a>, HashMap<String, u32>) {
        let mut pool: Vec<String> = Vec::new();
        let mut index: HashMap<String, u32> = HashMap::new();
        for photo in catalog.photos.values() {
            for settings in photo.settings_holders() {
                settings.visit_seg_masks(&mut |m| {
                    if !m.data.is_empty() && !index.contains_key(m.data.as_str()) {
                        index.insert(m.data.clone(), u32::try_from(pool.len()).unwrap_or(u32::MAX));
                        pool.push(m.data.clone());
                    }
                });
            }
        }
        // every field by name: a new one doesn't compile until it is classified here
        let Catalog {
            photos,
            albums,
            next_photo,
            next_album,
            stacks,
            next_stack,
            label_names,
            browsed,
            keyword_list,
            folder_records,
            grids: _,
            revision: _,
            changes: _,
            index: _,
            plans: _,
        } = catalog;
        let body = OfficialCatalog {
            photos,
            albums,
            next_photo: *next_photo,
            next_album: *next_album,
            stacks,
            next_stack: *next_stack,
            label_names,
            browsed,
            keyword_list,
            folder_records,
            grids: pool,
        };
        (body, index)
    }
}

fn official(b: &mut impl Write, catalog: &Catalog, seq: u64) -> io::Result<()> {
    write!(b, "{{\"format\":\"{FORMAT}\",\"version\":{},\"seq\":{seq},\"catalog\":", Target::Official.format())?;
    let (body, index) = OfficialCatalog::of(catalog);
    match lightcraft_develop::with_grid_interner(index, || serde_json::to_writer(&mut *b, &body)) {
        Some(Ok(())) => {}
        Some(Err(e)) => return Err(io::Error::other(e)),
        // (writes don't nest: this is a bug, and no snapshot is better than one with grids missing)
        None => return Err(io::Error::other("nested grid interner while writing a snapshot")),
    }
    b.write_all(b"}\n")
}

/// The defaults build 7248's develop settings have: this build's, without the setting it added since.
fn defaults_7248() -> String {
    let mut defaults = stored::base().1.clone();
    if let Some(Value::Object(detail)) = defaults.get_mut("detail") {
        detail.remove("nr_outliers");
    }
    defaults.to_string()
}

fn build_7248(b: &mut impl Write, catalog: &Catalog, seq: u64, listed: &[Segment]) -> io::Result<()> {
    write!(b, "{{\"format\":\"{FORMAT}\",\"version\":{},\"seq\":{seq},\"defaults\":{},\"catalog\":", Target::Build7248.format(), defaults_7248())?;
    serde_json::to_writer(&mut *b, &catalog.without_photos()).map_err(io::Error::other)?;
    b.write_all(b",\"segments\":")?;
    serde_json::to_writer(&mut *b, listed).map_err(io::Error::other)?;
    writeln!(b, ",\"photos\":{}}}", catalog.len())
}

// ---- reading it back

/// A sink that counts and checksums what is written to it.
struct Sum {
    crc: crc32fast::Hasher,
    len: u64,
}

impl Write for Sum {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.crc.update(buf);
        self.len += buf.len() as u64;
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

#[derive(Deserialize)]
struct Listed {
    #[serde(default)]
    segments: Vec<Segment>,
}

/// Read the files in `dir` back into memory: the catalog files, and the segments.
fn read_back(dir: &Path) -> Result<MemStore, ConvertError> {
    let fail = |what: String| ConvertError::Verify(what);
    let mem = MemStore::new();
    for name in [SNAPSHOT, LOG] {
        let bytes = fs::read(dir.join(name)).map_err(|e| fail(format!("can't read {name} back: {e}")))?;
        mem.set(name, bytes);
    }
    let folder = dir.join(SEGMENT_DIR);
    if folder.exists() {
        for entry in fs::read_dir(&folder).map_err(|e| fail(format!("can't list {SEGMENT_DIR}: {e}")))? {
            let entry = entry.map_err(|e| fail(format!("can't list {SEGMENT_DIR}: {e}")))?;
            let name = format!("{SEGMENT_PREFIX}{}", entry.file_name().to_string_lossy());
            let bytes = fs::read(entry.path()).map_err(|e| fail(format!("can't read {name} back: {e}")))?;
            mem.set(&name, bytes);
        }
    }
    Ok(mem)
}

/// Read the copy in `dir` back as a build opens it, and hold it to what was meant to be written: the format it is
/// stamped with, the bytes of the snapshot, and the catalog it reads as (every photo's JSON as well).
pub(super) fn verify(dir: &Path, target: Target, expected: &Catalog, seq: u64) -> Result<(), ConvertError> {
    let fail = |what: String| ConvertError::Verify(what);
    let mem = read_back(dir)?;
    let snap = mem.get(SNAPSHOT).ok_or_else(|| fail(format!("{SNAPSHOT} is missing")))?;
    if target == Target::Official && dir.join(SEGMENT_DIR).exists() {
        return Err(fail(format!("{SEGMENT_DIR} was written for a target that has none")));
    }
    let listed: Listed = serde_json::from_slice(&snap).map_err(|e| fail(format!("{SNAPSHOT} doesn't parse: {e}")))?;
    // what is on disk is what the writer renders: not cut short, not changed
    let mut sum = Sum { crc: crc32fast::Hasher::new(), len: 0 };
    render(target, expected, seq, &listed.segments, &mut sum).map_err(|e| fail(format!("can't render {SNAPSHOT} again: {e}")))?;
    if sum.len != snap.len() as u64 || sum.crc.finalize() != crc32fast::hash(&snap) {
        return Err(fail(format!("{SNAPSHOT} is not what was meant to be written")));
    }
    if mem.get(LOG).is_none_or(|log| !log.is_empty()) {
        return Err(fail(format!("{LOG} is not empty")));
    }
    let (_, mut loaded, report) = Journal::open(Box::new(mem)).map_err(|e| fail(format!("this build can't open the copy: {e}")))?;
    let stamped = (report.upgraded_from, report.failed, report.replayed, report.damaged.is_some(), report.repaired.is_empty());
    if stamped != (Some(target.format()), 0, 0, false, true) {
        return Err(fail(format!(
            "the copy opens as {stamped:?} (stamp, failed, replayed, damaged, repaired-nothing): not a clean format {}",
            target.format()
        )));
    }
    loaded.revision = expected.revision;
    if loaded != *expected {
        return Err(fail("the copy reads as another catalog than the one written".to_string()));
    }
    let (read, meant) = (loaded.photo_map(), expected.photo_map());
    if read.len() != meant.len() {
        return Err(fail(format!("the copy holds {} photos where {} were written", read.len(), meant.len())));
    }
    for (id, photo) in meant {
        let same = read.get(id).is_some_and(|q| serde_json::to_string(photo.as_ref()).ok() == serde_json::to_string(q.as_ref()).ok());
        if !same {
            return Err(fail(format!("photo {} doesn't read back as written", id.0)));
        }
    }
    Ok(())
}

/// The catalog files in `dir`, one line each: `name (size)`.
pub(super) fn listing(dir: &Path) -> Result<Vec<String>, ConvertError> {
    let io = |e: io::Error| ConvertError::Io(format!("can't list the copy: {e}"));
    let mut lines = Vec::new();
    for name in [SNAPSHOT, LOG] {
        lines.push(format!("{name} ({} bytes)", fs::metadata(dir.join(name)).map_err(io)?.len()));
    }
    let folder = dir.join(SEGMENT_DIR);
    if folder.exists() {
        let mut names = Vec::new();
        for entry in fs::read_dir(&folder).map_err(io)? {
            let entry = entry.map_err(io)?;
            names.push((entry.file_name().to_string_lossy().into_owned(), entry.metadata().map_err(io)?.len()));
        }
        names.sort();
        lines.extend(names.into_iter().map(|(name, bytes)| format!("{SEGMENT_PREFIX}{name} ({bytes} bytes)")));
    }
    Ok(lines)
}
