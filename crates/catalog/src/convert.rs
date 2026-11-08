//! Copying a library to the catalog format of another build: going back, or across.
//!
//! A tool of the personal build (`examples/convert_library.rs` is its command line), not of
//! LightCraft. Its one job is not to lose what the user keeps in a library, above all the color
//! labels and keywords of photos and folders, then ratings, edits and crops, when the library is
//! needed in a build whose catalog this one's can't be opened by.
//!
//! Two targets ([`Target`]):
//!
//! - [`Target::Official`]: what `origin/main` reads: format 12, `catalog.snap` alone, every photo
//!   record whole. `FolderRecord.made` is the one thing this build keeps that main has no place
//!   for.
//! - [`Target::Build7248`]: what the build `7248a97a77` reads: format 11 (photos in segments,
//!   compact records), the format it wrote before this build's. That build has no place for a few
//!   things this one has (see [`strip`]).
//!
//! The rules it keeps:
//!
//! - **The library is only read.** It is read the way a build opens it ([`Journal::open`], on a
//!   copy of its files in memory: the upgrade and repair an open does are done in memory, and
//!   written nowhere), under a shared lock on `catalog.lock`, so a program that opens the library
//!   meanwhile is refused and one that has it open is refused here.
//! - **The copy is a new folder.** An `out` inside the library (compared by file identity, so a
//!   different spelling of the same folder on a case-insensitive volume is caught too), or one
//!   that holds anything, is refused before anything is read. The copy is written beside it, in a
//!   hidden folder, read back and compared with what was meant to be written, and only then moved
//!   to `out`: a failure leaves nothing.
//! - **Nothing is dropped unasked.** Whatever the target has no place for, or this build can't
//!   see, is listed ([`Loss`]) and the copy is not made, unless [`Options::allow_loss`] says to go
//!   on, and then the report lists it again. Photos, albums, keywords, labels, ratings, edits,
//!   history and everything else the target holds are carried whole.
//! - **Files that were left out are listed.** The library's other files are copied (settings and
//!   presets, thumbnails, smart previews, originals the library holds), except what the target
//!   can't use, which is named in the report (the lock files, backups, older formats' kept
//!   copies, and every file this tool doesn't know). `prefs.json` is copied as it is, except for
//!   official LightCraft: its saved filter presets lose the lists and rule fields it has no place
//!   for (a loss each, as in a smart album's rules).
//!
//! What this build's reader doesn't know of a newer or other build's catalog is the one thing it
//! can't carry: [`read`] looks for such fields in the library's records and reports each one it
//! finds as a loss (`input:…`).
//!
//! [`census`] is the proof step beside it: a read-only manifest of what a library holds (above all the color labels and
//! keywords of photos and folders) and the comparison of two manifests, for a library and its copy, or before and after a swap.

pub mod census;
mod files;
mod read;
pub(crate) mod strip;
mod write;

use std::fmt;
use std::io::Write;
use std::path::{Path, PathBuf};

use serde_json::json;

/// The catalog format official LightCraft (`origin/main`) writes: the newest of the formats it
/// has, here the row of the shared segmentation grids.
pub const OFFICIAL_FORMAT: u32 = 12;
/// The catalog format of the build `7248a97a77`: its photos in segments (it counts compact
/// records as 10 and segments as 11, where this build counts them 14 and 15).
pub const BUILD_7248_FORMAT: u32 = 11;

/// What the copy is written for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Target {
    /// Official LightCraft (`origin/main`): format [`OFFICIAL_FORMAT`], one file, whole records.
    Official,
    /// The build `7248a97a77`: format [`BUILD_7248_FORMAT`], segments, compact records.
    Build7248,
}

impl Target {
    /// The catalog format number the copy's `catalog.snap` carries.
    pub const fn format(self) -> u32 {
        match self {
            Target::Official => OFFICIAL_FORMAT,
            Target::Build7248 => BUILD_7248_FORMAT,
        }
    }

    pub const fn name(self) -> &'static str {
        match self {
            Target::Official => "official LightCraft (origin/main)",
            Target::Build7248 => "build 7248a97a77",
        }
    }
}

/// What to do besides what is always done.
#[derive(Clone, Copy, Debug, Default)]
pub struct Options {
    /// Make the copy although some data can't be carried over (the report lists it). Without it a
    /// copy that would lose data is not made.
    pub allow_loss: bool,
    /// Copy only the catalog and the settings files: not the thumbnail cache, smart previews,
    /// originals the library holds or the interop folder (they are listed as left out).
    pub catalog_only: bool,
}

/// Data of the library that the copy would not hold.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Loss {
    /// A stable name: `folder.made`, `input:photo.left_in_file`…
    pub id: String,
    /// What is dropped.
    pub what: String,
    /// How many records hold it (photos, folder records, log records…).
    pub records: u64,
    /// What the target makes of it being gone.
    pub effect: String,
}

/// What a conversion did (or, when it was refused for [`ConvertError::Losses`], would have done).
#[derive(Clone, Debug, Default)]
pub struct Report {
    pub library: PathBuf,
    pub out: PathBuf,
    pub target: Option<Target>,
    /// How the library was stored: the format, where its photos were and how records were written.
    pub layout: String,
    /// The format number of `catalog.snap` (0: the library has none, only a log).
    pub stamp: u32,
    pub photos: usize,
    pub albums: usize,
    pub stacks: usize,
    pub labeled_photos: usize,
    pub keyworded_photos: usize,
    pub labeled_folders: usize,
    /// Keywords of the library's own list.
    pub keyword_list: usize,
    pub losses: Vec<Loss>,
    /// Things that don't lose data but change how it reads.
    pub warnings: Vec<String>,
    pub notes: Vec<String>,
    /// Files and folders copied unchanged, rewritten for the target, and left out: `name: why`.
    pub copied: Vec<String>,
    pub rewritten: Vec<String>,
    pub left_out: Vec<String>,
    /// The catalog files written, with their sizes (empty for a refused conversion).
    pub written: Vec<String>,
}

#[derive(Debug, thiserror::Error)]
pub enum ConvertError {
    /// Refused before anything was read or written beyond looking: the library is in use, `out`
    /// is not a place for a copy.
    #[error("{0}")]
    Refused(String),
    /// The library can't be read the way a build opens it.
    #[error("{0}")]
    Unreadable(String),
    #[error("{0}")]
    Io(String),
    /// The copy did not read back as what was meant to be written (nothing was kept).
    #[error("the copy was not kept: {0}")]
    Verify(String),
    /// Some data can't be carried over and [`Options::allow_loss`] was not given: nothing was
    /// written. The report says what and how much.
    #[error("{} kinds of data would be lost; nothing was written (run again with --allow-loss to go on)", .0.losses.len())]
    Losses(Box<Report>),
}

/// Copy `library` to the new folder `out` in the format of `target`. See the module docs.
pub fn convert(library: &Path, out: &Path, target: Target, options: Options) -> Result<Report, ConvertError> {
    let out = files::check_out(library, out)?;
    // (held until this returns: a program that opens the library meanwhile is refused)
    let held = read::lock(library)?;
    let source = read::open(library)?;
    let mut items = files::survey(library, target, options)?;
    let (prefs, prefs_losses) = files::prefs(library, target, &mut items)?;
    let stripped = strip::for_target(&source.catalog, target);
    let mut report = Report::of(library, &out, target, &source, &items);
    report.notes.extend(held.note.clone());
    report.losses = source.losses.iter().cloned().chain(stripped.losses).chain(prefs_losses).collect();
    report.losses.sort_by(|a, b| a.id.cmp(&b.id));
    report.warnings.extend(stripped.warnings);
    if !report.losses.is_empty() && !options.allow_loss {
        return Err(ConvertError::Losses(Box::new(report)));
    }
    let staging = files::Staging::create(&out)?;
    write::catalog(staging.path(), target, &stripped.catalog, source.seq, source.record_bytes)
        .map_err(|e| ConvertError::Io(format!("can't write the copy: {e}")))?;
    files::copy(library, staging.path(), &items, &mut report)?;
    if let Some(bytes) = prefs {
        files::write_prefs(staging.path(), &bytes)?;
    }
    write::verify(staging.path(), target, &stripped.catalog, source.seq)?;
    report.written = write::listing(staging.path())?;
    staging.commit(&out)?;
    Ok(report)
}

/// Write, one JSON object per line, what this build's reader makes of `library`: a `library` line
/// (format, layout, counts), a `catalog` line (everything but the photos) and a `photo` line per
/// photo, in the form the control channel shows (`photo.inspect`). The library is only read, as
/// [`convert`] reads it. What a copy made from it must still say, whatever the target.
pub fn dump(library: &Path, w: &mut dyn Write) -> Result<(), ConvertError> {
    let _held = read::lock(library)?;
    let source = read::open(library)?;
    let io = |e: std::io::Error| ConvertError::Io(format!("can't write the dump: {e}"));
    let head = json!({
        "record": "library",
        "stamp": source.stamp,
        "layout": source.layout,
        "seq": source.seq,
        "photos": source.catalog.len(),
    });
    writeln!(w, "{head}").map_err(io)?;
    let body = serde_json::to_value(source.catalog.without_photos()).map_err(|e| ConvertError::Io(e.to_string()))?;
    writeln!(w, "{}", json!({ "record": "catalog", "catalog": body })).map_err(io)?;
    for (id, photo) in source.catalog.photo_map() {
        let photo = serde_json::to_value(photo.as_ref()).map_err(|e| ConvertError::Io(e.to_string()))?;
        writeln!(w, "{}", json!({ "record": "photo", "id": id.0, "photo": photo })).map_err(io)?;
    }
    Ok(())
}

impl Report {
    fn of(library: &Path, out: &Path, target: Target, source: &read::Source, items: &[files::Item]) -> Report {
        let c = &source.catalog;
        let mut report = Report {
            library: library.to_path_buf(),
            out: out.to_path_buf(),
            target: Some(target),
            layout: source.layout.clone(),
            stamp: source.stamp,
            photos: c.len(),
            albums: c.albums.len(),
            stacks: c.stacks.len(),
            labeled_photos: c.photos.values().filter(|p| p.label.is_some()).count(),
            keyworded_photos: c.photos.values().filter(|p| !p.meta.keywords.is_empty()).count(),
            labeled_folders: c.folder_records.values().filter(|r| r.label.is_some()).count(),
            keyword_list: c.keyword_list.len(),
            notes: source.notes.clone(),
            ..Report::default()
        };
        files::describe(items, &mut report);
        report
    }
}

impl fmt::Display for Report {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let target = self.target.map_or("", Target::name);
        writeln!(f, "library:   {}", self.library.display())?;
        writeln!(f, "copy:      {} (for {target})", self.out.display())?;
        writeln!(f, "was:       {}", self.layout)?;
        writeln!(
            f,
            "holds:     {} photos ({} with a color label, {} with keywords), {} albums, {} stacks, {} keywords listed, {} folders with a color label",
            self.photos, self.labeled_photos, self.keyworded_photos, self.albums, self.stacks, self.keyword_list, self.labeled_folders
        )?;
        for (title, lines) in [("notes", &self.notes), ("warnings", &self.warnings)] {
            for line in lines {
                writeln!(f, "{title}: {line}")?;
            }
        }
        if self.losses.is_empty() {
            writeln!(f, "losses:    none")?;
        }
        for loss in &self.losses {
            writeln!(f, "LOSS {}: {} ({} record(s)); {}", loss.id, loss.what, loss.records, loss.effect)?;
        }
        for (title, lines) in [("copied", &self.copied), ("rewritten", &self.rewritten), ("left out", &self.left_out), ("written", &self.written)] {
            for line in lines {
                writeln!(f, "{title}: {line}")?;
            }
        }
        Ok(())
    }
}
