//! Reading a library the way a build opens it, and writing nothing.
//!
//! The files are read into memory (`catalog.snap`, `catalog.log` and the segments the snapshot
//! lists) and opened there with [`Journal::open`]: the build's own reader, with every check it
//! makes (a segment's size and checksum, a damaged log, a catalog of a newer build), and the
//! upgrade and repair an open does, which happen to the copy in memory and are written nowhere.

use std::collections::{BTreeMap, BTreeSet};
use std::fs::{self, File, TryLockError};
use std::io;
use std::path::Path;
use std::sync::Arc;

use serde::Deserialize;
use serde_json::Value;

use super::{ConvertError, Loss};
use crate::journal::{self, Journal, LOG, SNAPSHOT};
use crate::lock::{LOCK, LockError, LockOwner, OWNER};
use crate::segments::{SEGMENT_DIR, SEGMENT_PREFIX, SEGMENT_SUFFIX, Segment};
use crate::store::MemStore;
use crate::stored::ReadingOver;
use crate::{Catalog, CatalogError, LoadReport, Photo, stored};

/// A shared lock on the library's `catalog.lock`, held until it is dropped: a program that opens
/// the library meanwhile (it takes the lock exclusively) is refused. Nothing is written for it.
pub(super) struct Held {
    _file: Option<File>,
    /// Said when the file system can't lock: the library may then be open somewhere unseen.
    pub note: Option<String>,
}

/// Take the shared lock, or refuse if the library is open in a program.
pub(super) fn lock(dir: &Path) -> Result<Held, ConvertError> {
    let path = dir.join(LOCK);
    let file = match File::open(&path) {
        Ok(f) => f,
        // a library no program has locked yet
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(Held { _file: None, note: None }),
        Err(e) => return Err(ConvertError::Refused(format!("can't open {} to see whether the library is open: {e}", path.display()))),
    };
    match file.try_lock_shared() {
        Ok(()) => Ok(Held { _file: Some(file), note: None }),
        Err(TryLockError::WouldBlock) => {
            let owner = fs::read(dir.join(OWNER)).ok().and_then(|b| serde_json::from_slice::<LockOwner>(&b).ok());
            Err(ConvertError::Refused(format!("{} Nothing was read or written.", LockError::InUse(owner))))
        }
        Err(TryLockError::Error(e)) => Ok(Held {
            _file: None,
            note: Some(format!("{LOCK} can't be locked here ({e}): make sure no LightCraft has the library open while this runs")),
        }),
    }
}

/// What a library says, as a build reads it.
pub(super) struct Source {
    pub catalog: Catalog,
    /// The op the catalog is at (`catalog.snap`'s `seq` and the log records after it).
    pub seq: u64,
    /// The snapshot's format number (0: none, a library of a log only).
    pub stamp: u32,
    pub layout: String,
    /// What an average photo record takes in the library's files, as far as known (0: not known).
    pub record_bytes: u64,
    pub notes: Vec<String>,
    /// Data this read dropped or could not apply.
    pub losses: Vec<Loss>,
}

/// `catalog.snap` without its catalog.
#[derive(Default, Deserialize)]
struct Head {
    #[serde(default)]
    version: u32,
    #[serde(default)]
    defaults: Option<Value>,
    #[serde(default)]
    segments: Vec<Segment>,
}

/// Open `dir` as a build does, in memory.
pub(super) fn open(dir: &Path) -> Result<Source, ConvertError> {
    let snap = read_file(dir, SNAPSHOT)?;
    let log = read_file(dir, LOG)?;
    if snap.is_none() && log.is_none() {
        return Err(ConvertError::Unreadable(format!("{} holds neither {SNAPSHOT} nor {LOG}: it is not a library", dir.display())));
    }
    // (a snapshot that doesn't parse says so when it is opened, below)
    let head: Head = snap.as_deref().and_then(|b| serde_json::from_slice(b).ok()).unwrap_or_default();
    let mut notes = Vec::new();
    let mut scan = Scan::start(&head, &mut notes);
    if let Some(snap) = &snap {
        scan.snapshot(snap);
    }
    let mem = MemStore::new();
    let mut bytes = snap.as_ref().map_or(0, |b| b.len() as u64);
    bytes += load_segments(dir, &head.segments, &mem, &mut scan)?;
    if let Some(log) = &log {
        scan.log(log);
    }
    let unknown = scan.finish();
    // (moved into the store, which is what the open reads: not a second copy of a large snapshot)
    if let Some(snap) = snap {
        mem.set(SNAPSHOT, snap);
    }
    if let Some(log) = log {
        mem.set(LOG, log);
    }
    let (journal, catalog, load) = Journal::open(Box::new(mem)).map_err(|e| match e {
        CatalogError::Newer(m) => {
            ConvertError::Unreadable(format!("the library is of a newer build than this one ({m}): this tool reads what this build can"))
        }
        e => ConvertError::Unreadable(format!("this build can't open the library: {e}")),
    })?;
    let seq = journal.seq();
    drop(journal);
    let photos = catalog.len() as u64;
    Ok(Source {
        record_bytes: bytes.checked_div(photos).unwrap_or(0),
        layout: layout(&head, &load),
        stamp: head.version,
        seq,
        notes: notes_of(&load, notes),
        losses: losses_of(&load, unknown),
        catalog,
    })
}

fn read_file(dir: &Path, name: &str) -> Result<Option<Vec<u8>>, ConvertError> {
    match fs::read(dir.join(name)) {
        Ok(b) => Ok(Some(b)),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(ConvertError::Io(format!("can't read {}: {e}", dir.join(name).display()))),
    }
}

/// Only names a build gives its segments: nothing outside the segments folder.
fn segment_name(name: &str) -> bool {
    name.strip_prefix(SEGMENT_PREFIX)
        .and_then(|rest| rest.strip_suffix(SEGMENT_SUFFIX))
        .is_some_and(|stem| !stem.is_empty() && stem.len() <= 64 && stem.bytes().all(|b| b.is_ascii_digit() || b == b'-'))
}

/// Put the segments the snapshot lists into `mem`, and look at each one's records; the bytes read. A
/// segment that is missing is the open's to report (with the build's own words), and so is a name that
/// is not a segment's.
fn load_segments(dir: &Path, listed: &[Segment], mem: &MemStore, scan: &mut Scan) -> Result<u64, ConvertError> {
    let mut bytes = 0;
    if listed.is_empty() {
        return Ok(0);
    }
    // a link in the place of the folder could lead anywhere: a build refuses such a library too
    if fs::symlink_metadata(dir.join(SEGMENT_DIR)).is_ok_and(|m| m.file_type().is_symlink()) {
        return Err(ConvertError::Unreadable(format!("{SEGMENT_DIR} in {} is a link: refused, as a build refuses it", dir.display())));
    }
    for segment in listed.iter().filter(|s| segment_name(&s.name)) {
        if let Some(b) = read_file(dir, &segment.name)? {
            bytes += b.len() as u64;
            scan.segment(&b);
            mem.set(&segment.name, b);
        }
    }
    Ok(bytes)
}

fn layout(head: &Head, load: &LoadReport) -> String {
    if head.version == 0 {
        return "no catalog.snap: the library is a log only".to_string();
    }
    let photos = match head.segments.len() {
        0 => "photos in catalog.snap".to_string(),
        n => format!("photos in {n} segment file(s)"),
    };
    let records = if head.defaults.is_some() { "compact records" } else { "whole records" };
    let other = if load.other_defaults { ", settings read over the defaults the library states" } else { "" };
    format!("format {}: {photos}, {records}{other}", head.version)
}

fn notes_of(load: &LoadReport, mut notes: Vec<String>) -> Vec<String> {
    if load.replayed > 0 {
        notes.push(format!("{} change(s) of catalog.log were applied on top of catalog.snap: the copy holds them", load.replayed));
    }
    if load.torn_bytes > 0 {
        notes.push(format!("the log ended in a torn record of {} bytes, which is not applied (a build's open drops it too)", load.torn_bytes));
    }
    if load.torn_fragments > 0 {
        notes.push(format!("{} partial record(s) in the middle of the log were skipped (what a build's open does)", load.torn_fragments));
    }
    if let Some(from) = load.upgraded_from {
        notes.push(format!("the library's format {from} was upgraded in memory to this build's, as an open does: the library itself is untouched"));
    }
    notes.extend(load.repaired.iter().map(|r| format!("repaired as an open does (in memory): {r}")));
    if load.other_defaults {
        notes.push("the library states other develop defaults than this build: its records were read over their own".to_string());
    }
    notes
}

fn losses_of(load: &LoadReport, unknown: BTreeMap<String, u64>) -> Vec<Loss> {
    let mut losses = Vec::new();
    if let Some(kept) = &load.damaged {
        losses.push(Loss {
            id: "log.damaged".into(),
            what: format!(
                "catalog.log has a damaged record in the middle (a build's open keeps the log as {kept}); the records after it are not applied"
            ),
            records: 1,
            effect: "the changes made after the damaged record are not in the copy".into(),
        });
    }
    if load.failed > 0 {
        losses.push(Loss {
            id: "log.failed".into(),
            what: "catalog.log records whose change could not be applied".into(),
            records: load.failed as u64,
            effect: "those changes are not in the copy".into(),
        });
    }
    for (path, records) in unknown {
        losses.push(Loss {
            id: format!("input:{path}"),
            what: format!("`{path}`, which the library's records state and this build's catalog has no field for"),
            records,
            effect: "this build's reader drops it when it reads the library, so it is not in the copy (the build that wrote the library keeps it)"
                .into(),
        });
    }
    losses
}

// ---- what this build's reader would not keep

/// The roots of develop settings in a record: what they state is what the build's defaults are
/// the base of.
const SETTINGS: [&str; 3] = ["develop", "settings", "import_look"];

/// The deepest a record's JSON is followed (what a parsed `Value` can hold is bounded anyway).
const MAX_DEPTH: usize = 48;

/// The catalog's tables of records by key (an id, a keyword, a folder): each entry is one record, whatever its key.
const TABLES: [&str; 6] = ["albums", "stacks", "label_names", "browsed", "keyword_list", "folder_records"];

/// What the library's records state that this build's model has no field for: the path of each such key
/// (array positions and ids left out) and in how many records. A record is read as this build's model reads
/// it, written out whole, and each key of the file that the written form lacks counts unless it says nothing
/// (`null`, `false`, `0`, an empty string, list or object) or states a develop setting's default.
///
/// This is the one thing a copy can lose that the build's own open does not report: a field of a newer or
/// other build's catalog is ignored when it is read, and is not in what is written. Where the settings are
/// read over other defaults than this build's, a setting is not compared (they are written over those).
struct Scan {
    skip_settings: bool,
    found: BTreeMap<String, u64>,
    // (while it lives, settings are read over the library's defaults)
    _over: Option<ReadingOver>,
}

impl Scan {
    fn start(head: &Head, notes: &mut Vec<String>) -> Scan {
        let over = head.defaults.as_ref().filter(|theirs| !stored::reads_the_same(theirs));
        if over.is_some() {
            notes.push(
                "the library states other develop defaults than this build: settings were not checked for fields this build doesn't know".to_string(),
            );
        }
        Scan { skip_settings: over.is_some(), found: BTreeMap::new(), _over: over.map(|theirs| stored::reading_over(Some(Arc::new(theirs.clone())))) }
    }

    /// What was found, and the settings read and written as they were before.
    fn finish(self) -> BTreeMap<String, u64> {
        self.found
    }

    /// The catalog of `catalog.snap`, and the photos it holds itself.
    fn snapshot(&mut self, bytes: &[u8]) {
        let Ok(Value::Object(mut doc)) = serde_json::from_slice::<Value>(bytes) else { return };
        let Some(Value::Object(mut body)) = doc.remove("catalog") else { return };
        // (the catalog is read without its photos: the table is put back empty, which a catalog has to have)
        if let Some(Value::Object(photos)) = body.insert("photos".to_string(), Value::Object(serde_json::Map::new())) {
            for record in photos.values() {
                self.photo(record);
            }
        }
        let Ok(typed) = Catalog::deserialize(&Value::Object(body.clone())) else { return };
        let Ok(Value::Object(mut written)) = serde_json::to_value(&typed) else { return };
        for table in TABLES {
            let (Some(Value::Object(file)), Some(Value::Object(held))) = (body.remove(table), written.remove(table)) else { continue };
            let root = format!("catalog.{table}.*");
            for (key, entry) in &file {
                if let Some(held) = held.get(key) {
                    self.record(&root, entry, held);
                }
            }
        }
        self.record("catalog", &Value::Object(body), &Value::Object(written));
    }

    fn segment(&mut self, bytes: &[u8]) {
        let Ok(Value::Object(mut doc)) = serde_json::from_slice::<Value>(bytes) else { return };
        if let Some(Value::Object(photos)) = doc.remove("photos") {
            for record in photos.values() {
                self.photo(record);
            }
        }
    }

    fn photo(&mut self, record: &Value) {
        let Ok(typed) = Photo::deserialize(record) else { return };
        if let Ok(written) = serde_json::to_value(&typed) {
            self.record("photo", record, &written);
        }
    }

    fn log(&mut self, bytes: &[u8]) {
        for line in bytes.split(|b| *b == b'\n') {
            let Ok(line) = std::str::from_utf8(line) else { continue };
            let line = line.trim_end_matches('\r');
            let Some((_, op)) = journal::decode_record(line) else { continue };
            let (Ok(Value::Object(mut file)), Ok(written)) = (serde_json::from_str::<Value>(line), serde_json::to_value(&op)) else { continue };
            if let Some(file) = file.remove("op") {
                self.record("op", &file, &written);
            }
        }
    }

    /// Count each key of `file` that `written` lacks, once per record.
    fn record(&mut self, root: &str, file: &Value, written: &Value) {
        let mut keys = BTreeSet::new();
        let mut path = vec![root];
        walk(self.skip_settings, file, written, None, &mut path, &mut keys);
        for key in keys {
            *self.found.entry(key).or_default() += 1;
        }
    }
}

/// `defaults`: what this build's develop defaults hold at `file`'s place, when it is inside settings.
fn walk<'a>(skip_settings: bool, file: &'a Value, written: &Value, defaults: Option<&Value>, path: &mut Vec<&'a str>, found: &mut BTreeSet<String>) {
    if path.len() > MAX_DEPTH {
        return;
    }
    match (file, written) {
        (Value::Object(f), Value::Object(h)) => {
            for (key, value) in f {
                // a mask that refers to the snapshot's shared grids: this build resolves the index when it reads, and a mask is never written back with one
                if key == "grid" && f.contains_key("side") {
                    continue;
                }
                let settings = SETTINGS.contains(&key.as_str());
                if settings && skip_settings {
                    continue;
                }
                let inside = if settings { Some(&stored::base().1) } else { defaults.and_then(|d| d.get(key.as_str())) };
                path.push(key);
                match h.get(key) {
                    Some(held) => walk(skip_settings, value, held, inside, path, found),
                    None if says_nothing(value) || inside == Some(value) => {}
                    // a group the written form leaves out at its defaults (a log writes settings as the difference from them): look at each setting in it
                    None if value.is_object() && inside.is_some_and(Value::is_object) => {
                        walk(skip_settings, value, &Value::Object(serde_json::Map::new()), inside, path, found);
                    }
                    None => {
                        found.insert(path.join("."));
                    }
                }
                path.pop();
            }
        }
        (Value::Array(f), Value::Array(h)) => {
            path.push("[]");
            for (value, held) in f.iter().zip(h) {
                walk(skip_settings, value, held, None, path, found);
            }
            path.pop();
        }
        _ => {}
    }
}

fn says_nothing(v: &Value) -> bool {
    match v {
        Value::Null | Value::Bool(false) => true,
        Value::Number(n) => n.as_f64() == Some(0.0),
        Value::String(s) => s.is_empty(),
        Value::Array(a) => a.is_empty(),
        Value::Object(o) => o.is_empty(),
        Value::Bool(true) => false,
    }
}
