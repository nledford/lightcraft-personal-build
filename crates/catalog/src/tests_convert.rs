//! The library converter ([`crate::convert`]): the copies it makes, what it refuses to do and what it reports. The data a
//! user keeps most, the color labels and keywords of photos and folders, is compared photo by photo and folder by folder.
#![cfg(not(target_arch = "wasm32"))]

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::SystemTime;

use lightcraft_develop::{DevelopSettings, EmbeddedLens, LensSource, Mask, Process};
use serde_json::{Value, json};

use crate::convert::strip::{clear, found_in};
use crate::convert::{self, ConvertError, Options, Report, Target};
use crate::journal::{LOG, SNAPSHOT};
use crate::keywords::KeywordInfo;
use crate::lock::LibraryLock;
use crate::tests_stored::{edited, fixture_library, full, plain};
use crate::{
    Album, AlbumId, Catalog, ColorLabel, Filter, Flag, FolderRecord, HistoryStep, Journal, LoadReport, MemStore, Meta, Op, Photo, PhotoId, Stack,
    StackId, Version, stored,
};

// ---- folders and files

/// A folder under the temp dir, removed when it goes out of scope.
struct Scratch(PathBuf);

impl Scratch {
    fn new(tag: &str) -> Scratch {
        static NEXT: AtomicU32 = AtomicU32::new(0);
        let dir = std::env::temp_dir().join(format!("lc-convert-{tag}-{}-{}", std::process::id(), NEXT.fetch_add(1, Ordering::Relaxed)));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        Scratch(dir)
    }

    fn join(&self, name: &str) -> PathBuf {
        self.0.join(name)
    }

    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

type Files = Vec<(&'static str, &'static [u8])>;

/// `files` as a library folder at `dir`.
fn put(dir: &Path, files: &[(&str, &[u8])]) {
    for (name, bytes) in files {
        let path = dir.join(name);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, bytes).unwrap();
    }
}

/// The files of a store (what a build saved) as a library folder at `dir`.
fn put_store(dir: &Path, m: &MemStore) {
    let files: Vec<(String, Vec<u8>)> = m.files.lock().unwrap().iter().map(|(k, v)| (k.clone(), v.clone())).collect();
    put(dir, &files.iter().map(|(k, v)| (k.as_str(), v.as_slice())).collect::<Vec<_>>());
}

/// The catalog files of the library folder `dir` as a store: `catalog.snap`, `catalog.log` and the segments.
fn store_of(dir: &Path) -> MemStore {
    let m = MemStore::new();
    for name in [SNAPSHOT, LOG] {
        if let Ok(bytes) = fs::read(dir.join(name)) {
            m.set(name, bytes);
        }
    }
    if let Ok(entries) = fs::read_dir(dir.join("segments")) {
        for entry in entries {
            let entry = entry.unwrap();
            m.set(&format!("segments/{}", entry.file_name().to_string_lossy()), fs::read(entry.path()).unwrap());
        }
    }
    m
}

/// What this build reads of the library in `dir`, in memory: nothing is written there.
fn read_lib(dir: &Path) -> (Catalog, LoadReport) {
    let (_, catalog, report) = Journal::open(Box::new(store_of(dir))).unwrap();
    (catalog, report)
}

/// Every file and folder under `dir` with its bytes and modification time: the library as seen from outside.
type Tree = BTreeMap<String, (Vec<u8>, Option<SystemTime>)>;

fn tree(dir: &Path) -> Tree {
    fn walk(dir: &Path, rel: &str, out: &mut Tree) {
        for entry in fs::read_dir(dir).unwrap() {
            let entry = entry.unwrap();
            let name = format!("{rel}{}", entry.file_name().to_string_lossy());
            let meta = fs::symlink_metadata(entry.path()).unwrap();
            if meta.is_dir() {
                out.insert(format!("{name}/"), (Vec::new(), meta.modified().ok()));
                walk(&entry.path(), &format!("{name}/"), out);
            } else if meta.file_type().is_symlink() {
                out.insert(format!("{name} ->"), (fs::read_link(entry.path()).unwrap().to_string_lossy().into_owned().into_bytes(), None));
            } else {
                out.insert(name, (fs::read(entry.path()).unwrap(), meta.modified().ok()));
            }
        }
    }
    let mut out = Tree::new();
    walk(dir, "", &mut out);
    out
}

/// The names under `dir`, folders with a closing `/`.
fn names(dir: &Path) -> BTreeSet<String> {
    tree(dir).into_keys().collect()
}

// ---- running the converter

fn run(library: &Path, out: &Path, target: Target) -> Result<Report, ConvertError> {
    convert::convert(library, out, target, Options::default())
}

/// A conversion that goes on whatever is lost.
fn run_lossy(library: &Path, out: &Path, target: Target) -> Report {
    convert::convert(library, out, target, Options { allow_loss: true, ..Options::default() }).unwrap_or_else(|e| panic!("{target:?}: {e}"))
}

fn loss_ids(report: &Report) -> Vec<&str> {
    report.losses.iter().map(|l| l.id.as_str()).collect()
}

fn refusal(result: Result<Report, ConvertError>) -> String {
    match result {
        Err(ConvertError::Refused(why)) => why,
        Err(other) => panic!("expected a refusal, got an error: {other}"),
        Ok(report) => panic!("expected a refusal, the copy was made:\n{report}"),
    }
}

/// What `convert::dump` says of the library, a parsed line each: the `library` line, the `catalog` line, a `photo` line each.
fn dump(library: &Path) -> Vec<Value> {
    let mut out = Vec::new();
    convert::dump(library, &mut out).unwrap();
    String::from_utf8(out).unwrap().lines().map(|l| serde_json::from_str(l).unwrap()).collect()
}

/// The paths (arrays as `[]`, the keys of the catalog's tables as `*`) where two JSON values differ or only one has a key.
fn differences(a: &Value, b: &Value, path: &str, out: &mut BTreeSet<String>) {
    const TABLES: [&str; 6] =
        ["catalog.albums", "catalog.stacks", "catalog.label_names", "catalog.browsed", "catalog.keyword_list", "catalog.folder_records"];
    match (a, b) {
        (Value::Object(x), Value::Object(y)) => {
            for key in x.keys().chain(y.keys()).collect::<BTreeSet<_>>() {
                let at = format!("{path}.{}", if TABLES.contains(&path) { "*" } else { key });
                match (x.get(key), y.get(key)) {
                    (Some(p), Some(q)) => differences(p, q, &at, out),
                    _ => {
                        out.insert(at);
                    }
                }
            }
        }
        (Value::Array(x), Value::Array(y)) => {
            let at = format!("{path}[]");
            if x.len() == y.len() {
                x.iter().zip(y).for_each(|(p, q)| differences(p, q, &at, out));
            } else {
                out.insert(at);
            }
        }
        _ if a != b => {
            out.insert(path.to_string());
        }
        _ => {}
    }
}

/// Where the library and its copy read differently, by dump.
fn diffs_between(library: &Path, copy: &Path) -> BTreeSet<String> {
    let (a, b) = (dump(library), dump(copy));
    assert_eq!(a.len(), b.len(), "the copy holds another number of photos");
    let mut diffs = BTreeSet::new();
    for (x, y) in a.iter().zip(&b).skip(1) {
        assert_eq!((&x["record"], &x["id"]), (&y["record"], &y["id"]));
        match x["record"].as_str() {
            Some("catalog") => differences(&x["catalog"], &y["catalog"], "catalog", &mut diffs),
            _ => differences(&x["photo"], &y["photo"], "photo", &mut diffs),
        }
    }
    diffs
}

/// Every difference is one of the declared losses (a path ending as one of `allowed`), and each declared loss shows.
fn assert_only_declared(diffs: &BTreeSet<String>, allowed: &[&str]) {
    for d in diffs {
        assert!(allowed.iter().any(|a| d.ends_with(a)), "`{d}` differs in the copy and is not a declared loss");
    }
    for a in allowed {
        assert!(diffs.iter().any(|d| d.ends_with(a)), "`{a}` is declared lost but nothing differs");
    }
}

/// What the user keeps most: per photo the color label, keywords and rating; per folder the color label.
fn kept(library: &Path) -> (BTreeMap<u64, (Value, Value, Value)>, BTreeMap<String, Value>) {
    let (mut photos, mut folders) = (BTreeMap::new(), BTreeMap::new());
    for line in dump(library) {
        match line["record"].as_str() {
            Some("photo") => {
                let p = &line["photo"];
                photos.insert(line["id"].as_u64().unwrap(), (p["label"].clone(), p["meta"]["keywords"].clone(), p["rating"].clone()));
            }
            Some("catalog") => {
                for (folder, record) in line["catalog"]["folder_records"].as_object().into_iter().flatten() {
                    if !record["label"].is_null() {
                        folders.insert(folder.clone(), record["label"].clone());
                    }
                }
            }
            _ => {}
        }
    }
    (photos, folders)
}

// ---- the libraries

/// The libraries real builds wrote (`fixtures/`), and the catalog each is meant to hold where the tests of its format say
/// (`tests_stored::fixture_library`).
fn historic() -> Vec<(&'static str, Files, Option<u32>)> {
    let seg15 = ("segments/7-3.json", &include_bytes!("fixtures/v15-segments-7-3.json")[..]);
    let seg7248 = ("segments/1-0.json", &include_bytes!("fixtures/b7248-segments-1-0.json")[..]);
    let snap7248 = (SNAPSHOT, &include_bytes!("fixtures/b7248-catalog.snap.json")[..]);
    vec![
        (
            "format 6",
            vec![(SNAPSHOT, &include_bytes!("fixtures/v6-catalog.snap.json")[..]), (LOG, &include_bytes!("fixtures/v6-catalog.log.jsonl")[..])],
            Some(6),
        ),
        (
            "format 7",
            vec![(SNAPSHOT, &include_bytes!("fixtures/v7-catalog.snap.json")[..]), (LOG, &include_bytes!("fixtures/v7-catalog.log.jsonl")[..])],
            Some(7),
        ),
        (
            "format 8",
            vec![(SNAPSHOT, &include_bytes!("fixtures/v8-catalog.snap.json")[..]), (LOG, &include_bytes!("fixtures/v8-catalog.log.jsonl")[..])],
            Some(8),
        ),
        (
            "format 9",
            vec![(SNAPSHOT, &include_bytes!("fixtures/v9-catalog.snap.json")[..]), (LOG, &include_bytes!("fixtures/v9-catalog.log.jsonl")[..])],
            Some(9),
        ),
        (
            "format 14",
            vec![(SNAPSHOT, &include_bytes!("fixtures/v14-catalog.snap.json")[..]), (LOG, &include_bytes!("fixtures/v14-catalog.log.jsonl")[..])],
            Some(14),
        ),
        (
            "format 15",
            vec![
                (SNAPSHOT, &include_bytes!("fixtures/v15-catalog.snap.json")[..]),
                (LOG, &include_bytes!("fixtures/v15-catalog.log.jsonl")[..]),
                seg15,
            ],
            Some(15),
        ),
        ("build 7248, closed", vec![snap7248, seg7248], None),
        ("build 7248, not closed", vec![snap7248, seg7248, (LOG, &include_bytes!("fixtures/b7248-catalog.log.jsonl")[..])], None),
    ]
}

fn photo(id: u64, set: impl FnOnce(&mut Photo)) -> Photo {
    let mut p = plain(id);
    set(&mut p);
    p
}

fn outliers() -> Arc<DevelopSettings> {
    edited(|s| s.detail.nr_outliers = true)
}

fn masked() -> Arc<DevelopSettings> {
    edited(|s| s.masks = vec![Mask { on_photo: true, ..Mask::default() }])
}

fn a_version(settings: Arc<DevelopSettings>) -> Version {
    Version { name: "V".into(), created: "2026-10-10T12:30:00".into(), settings, auto: false }
}

fn a_step(settings: Arc<DevelopSettings>) -> HistoryStep {
    HistoryStep { label: "Mask".into(), settings }
}

/// A photo with every field set: the ones main and build 7248 have no place for too.
fn maximal_photo() -> Photo {
    let mut p = full();
    p.meta.make = "Maker".into();
    p.meta.lens_make = "Lens Maker".into();
    p.meta.flash = Some(true);
    p.meta.exposure_bias = Some(-0.5);
    p.meta.serial_number = "SN-1".into();
    p.meta.focus = Some(lightcraft_meta::FocusPoint {
        rect: lightcraft_meta::Rect { x0: 0.4, y0: 0.4, x1: 0.5, y1: 0.5 },
        mode: Some("AF-C".into()),
        area: Some("Wide".into()),
        tracking: None,
    });
    p.preview_checked = "0.8.0".into();
    p.history_cursor = Some(1);
    p.embedded_lens = Some(EmbeddedLens { vignette: Some(Default::default()), source: LensSource::Database, ..Default::default() });
    let mut now = (*p.develop).clone();
    now.detail.nr_outliers = true;
    now.process = Process::V5.version();
    p.develop = Arc::new(now);
    p
}

fn keyword_info() -> KeywordInfo {
    serde_json::from_value(json!({"synonyms": ["Lisboa"], "include_on_export": false, "keyword_shortcut": true, "new_keywords_inside": true}))
        .unwrap()
}

/// The changes that build a catalog with every kind of data this build keeps, and one photo for each thing a target can't
/// hold. Folder records: two with a label (one of them with a `made` too) and one with only a `made`.
fn personal_ops() -> Vec<Op> {
    let add = |p: Photo| Op::AddPhoto { photo: Box::new(p) };
    let words = |words: &[&str]| words.iter().map(|w| w.to_string()).collect::<Vec<_>>();
    let record = |folder: &str, label: Option<ColorLabel>, made: Option<&str>| Op::SetFolderRecord {
        folder: folder.into(),
        record: Some(FolderRecord { label, made: made.map(String::from) }),
    };
    vec![
        add(maximal_photo()),
        add(photo(8, |p| {
            p.rating = 2;
            p.flag = Flag::Reject;
            p.label = Some(ColorLabel::Red);
            p.meta.keywords = words(&["Places|Lisbon", "boats", "people|Ana"]);
        })),
        add(photo(9, |p| {
            p.label = Some(ColorLabel::Blue);
            p.versions = vec![a_version(outliers())];
        })),
        add(photo(10, |p| p.history = vec![a_step(masked())])),
        add(photo(11, |p| p.import_look = Some(outliers()))),
        add(photo(12, |p| p.embedded_lens = Some(EmbeddedLens { source: LensSource::Database, ..Default::default() }))),
        add(photo(13, |p| p.meta.flash = Some(false))),
        add(photo(14, |p| p.meta.serial_number = "SN-2".into())),
        add(photo(15, |p| p.develop = edited(|s| s.process = Process::V4.version()))),
        add(photo(16, |p| {
            p.label = Some(ColorLabel::Purple);
            p.meta.keywords = words(&["a|b|c"]);
        })),
        Op::AddAlbum {
            album: Album { photos: vec![PhotoId(7), PhotoId(8)], cover: Some(PhotoId(7)), order: Some(2), ..Album::new(AlbumId(1), "Keepers") },
        },
        Op::AddAlbum { album: Album { folder: true, ..Album::new(AlbumId(2), "Trips") } },
        Op::AddAlbum {
            album: Album {
                parent: Some(AlbumId(2)),
                smart: Some(Box::new(Filter { keyword: Some("boats".into()), ..Filter::default() })),
                ..Album::new(AlbumId(3), "Boats")
            },
        },
        Op::AddStack { stack: Stack { id: StackId(1), photos: vec![PhotoId(8), PhotoId(9)], collapsed: true } },
        Op::SetLabelName { label: ColorLabel::Red, name: Some("Approved".into()) },
        Op::SetBrowsed { folder: "/pictures/2019/trip".into(), at: Some("2026-10-10T12:00:00".into()) },
        Op::SetKeyword { path: "Places|Lisbon".into(), info: Some(keyword_info()) },
        record("/pictures/2019/trip", Some(ColorLabel::Green), None),
        record("/pictures/2020", Some(ColorLabel::Red), Some("2026-10-01T10:00:00")),
        record("/pictures/made-only", None, Some("2026-10-02T10:00:00")),
    ]
}

/// Changes made after the snapshot, as the log of a library that was not closed holds them: a label, a rating, keywords, a
/// folder's label and a label's name.
fn later_ops() -> Vec<Op> {
    vec![
        Op::SetLabel { id: PhotoId(8), label: Some(ColorLabel::Yellow) },
        Op::SetRating { id: PhotoId(8), rating: 5 },
        Op::SetMeta { id: PhotoId(16), meta: Box::new(Meta { keywords: vec!["late|keyword".into(), "a|b|c".into()], ..Meta::default() }) },
        Op::SetFolderRecord { folder: "/pictures/2019/trip".into(), record: Some(FolderRecord { label: Some(ColorLabel::Purple), made: None }) },
        Op::SetLabelName { label: ColorLabel::Blue, name: Some("Later".into()) },
    ]
}

/// The catalog `personal_ops` build.
fn personal() -> Catalog {
    let mut c = Catalog::new();
    for op in personal_ops() {
        c.apply(op).unwrap();
    }
    c
}

/// A library as this build saves it: `first` in the snapshot (in segments of `per` photos, or all in the one file), `later`
/// in the log after it.
fn saved(first: &[Op], later: &[Op], one_file: bool, per: usize) -> MemStore {
    let m = MemStore { single_file: one_file, ..MemStore::new() };
    let (mut j, mut c, _) = Journal::open(Box::new(m.clone())).unwrap();
    j.policy.segment_photos = per;
    for op in first {
        c.apply(op.clone()).unwrap();
    }
    j.append(first).unwrap();
    j.snapshot(&c).unwrap();
    for op in later {
        c.apply(op.clone()).unwrap();
    }
    j.append(later).unwrap();
    drop(j);
    m
}

/// The layouts a personal build leaves a library in: photos in several segments or one file, with the log empty or not.
const LAYOUTS: [(&str, bool, usize, bool); 4] = [
    ("segments, closed", false, 4, false),
    ("segments, log tail", false, 4, true),
    ("one file, closed", true, 4096, false),
    ("one file, log tail", true, 4096, true),
];

fn personal_library(dir: &Path, one_file: bool, per: usize, tail: bool) {
    let later = if tail { later_ops() } else { Vec::new() };
    put_store(dir, &saved(&personal_ops(), &later, one_file, per));
}

/// Whether the photo's metadata (public form) has a non-empty text under `key`.
fn has_text(photo: &Value, key: &str) -> bool {
    photo["meta"][key].as_str().is_some_and(|s| !s.is_empty())
}

// ---- what a copy holds

/// Copies of the historic libraries, in both formats, read as the catalogs the fixtures hold and as the libraries they were made
/// from: nothing is lost by any of them, the library is not touched, and no field this build doesn't know shows in any.
#[test]
fn every_historic_library_converts_whole_to_both_targets() {
    for (name, files, version) in historic() {
        let scratch = Scratch::new("historic");
        let library = scratch.join("library");
        put(&library, &files);
        let before = tree(&library);
        for target in [Target::Official, Target::Build7248] {
            let out = scratch.join(&format!("copy-{target:?}"));
            let report = run(&library, &out, target).unwrap_or_else(|e| panic!("{name} to {target:?}: {e}"));
            assert!(report.losses.is_empty(), "{name} to {target:?}: {:?}", report.losses);
            assert_eq!(tree(&library), before, "{name} to {target:?}: the library was changed");
            assert_eq!(diffs_between(&library, &out), BTreeSet::new(), "{name} to {target:?}");
            let (mut copy, loaded) = read_lib(&out);
            assert_eq!((loaded.upgraded_from, loaded.failed, loaded.replayed), (Some(target.format()), 0, 0), "{name} to {target:?}");
            if let Some(version) = version {
                let want = fixture_library(version);
                copy.revision = want.revision;
                assert!(copy == want, "{name} to {target:?}: the copy is not the catalog the fixture holds");
            }
        }
    }
}

/// Converting the library of build 7248 as it was closed to that build's format gives the files that build wrote, byte for
/// byte: the header and its defaults, the catalog beside the photos, the segment and its listing.
#[test]
fn the_closed_7248_library_converts_to_itself_byte_for_byte() {
    let scratch = Scratch::new("itself");
    let library = scratch.join("library");
    let (snap, segment) = (&include_bytes!("fixtures/b7248-catalog.snap.json")[..], &include_bytes!("fixtures/b7248-segments-1-0.json")[..]);
    put(&library, &[(SNAPSHOT, snap), ("segments/1-0.json", segment)]);
    let out = scratch.join("copy");
    run(&library, &out, Target::Build7248).unwrap();
    assert_eq!(fs::read(out.join("segments/1-0.json")).unwrap(), segment, "the segment");
    assert_eq!(String::from_utf8(fs::read(out.join(SNAPSHOT)).unwrap()).unwrap(), String::from_utf8(snap.to_vec()).unwrap(), "catalog.snap");
    assert_eq!(fs::read(out.join(LOG)).unwrap(), b"");
}

/// Given a library with a log that was never folded into a snapshot, the copy holds the changes too and its own log is empty.
#[test]
fn a_log_tail_is_part_of_the_copy() {
    let scratch = Scratch::new("tail");
    let library = scratch.join("library");
    put(&library, &historic().into_iter().find(|(name, ..)| *name == "build 7248, not closed").unwrap().1);
    let report = run(&library, &scratch.join("copy"), Target::Official).unwrap();
    assert!(report.notes.iter().any(|n| n.contains("4 change(s) of catalog.log")), "{:?}", report.notes);
    let (copy, loaded) = read_lib(&scratch.join("copy"));
    assert_eq!(loaded.replayed, 0);
    let p = copy.photo(PhotoId(21)).unwrap();
    assert_eq!((p.rating, p.flag), (4, Flag::Pick), "the changes of the log are in the copy");
    assert_eq!(fs::read(scratch.join("copy").join(LOG)).unwrap(), b"");
    let head: Value = serde_json::from_slice(&fs::read(scratch.join("copy").join(SNAPSHOT)).unwrap()).unwrap();
    assert_eq!(head["seq"], 4, "the copy is at the op the log ended at");
}

/// A library of a log only (no `catalog.snap`) is a library.
#[test]
fn a_library_that_is_only_a_log_converts() {
    let scratch = Scratch::new("log-only");
    let library = scratch.join("library");
    let m = MemStore::new();
    let (mut j, _, _) = Journal::open(Box::new(m.clone())).unwrap();
    j.append(&personal_ops()[..10]).unwrap();
    drop(j);
    put(&library, &[(LOG, &m.get(LOG).unwrap())]);
    let report = run_lossy(&library, &scratch.join("copy"), Target::Official);
    assert_eq!((report.stamp, report.photos), (0, 10));
    assert_eq!(read_lib(&scratch.join("copy")).0.len(), 10);
}

/// Given a library in each layout a personal build leaves one in, with every kind of data, when it is converted for
/// official LightCraft, then only what main has no place for differs (a folder's `made`), and what is lost is reported.
#[test]
fn a_personal_library_converts_to_official_in_every_layout() {
    for (name, one_file, per, tail) in LAYOUTS {
        let scratch = Scratch::new("official");
        let library = scratch.join("library");
        personal_library(&library, one_file, per, tail);
        let report = run_lossy(&library, &scratch.join("copy"), Target::Official);
        assert_eq!(loss_ids(&report), ["folder.made"], "{name}");
        assert_eq!(report.losses[0].records, 2, "{name}: the records with a `made`");
        assert_only_declared(&diffs_between(&library, &scratch.join("copy")), &["catalog.folder_records.*", "catalog.folder_records.*.made"]);
    }
}

/// … and for build 7248, whose copy loses what that build has no place for: the metadata and settings this build added.
#[test]
fn a_personal_library_converts_to_7248_in_every_layout() {
    const LOST: [&str; 10] = [
        "catalog.folder_records.*",
        "catalog.folder_records.*.made",
        "photo.meta.make",
        "photo.meta.lens_make",
        "photo.meta.flash",
        "photo.meta.exposure_bias",
        "photo.meta.serial_number",
        "photo.embedded_lens.source",
        ".detail.nr_outliers",
        ".masks[].on_photo",
    ];
    for (name, one_file, per, tail) in LAYOUTS {
        let scratch = Scratch::new("7248");
        let library = scratch.join("library");
        personal_library(&library, one_file, per, tail);
        let report = run_lossy(&library, &scratch.join("copy"), Target::Build7248);
        let ids: BTreeMap<&str, u64> = report.losses.iter().map(|l| (l.id.as_str(), l.records)).collect();
        // (counted by looking at the dump, not by the code that counts them)
        let photos = dump(&library).into_iter().filter(|l| l["record"] == "photo").collect::<Vec<_>>();
        let having = |f: &dyn Fn(&Value) -> bool| photos.iter().filter(|l| f(&l["photo"])).count() as u64;
        let (outliers, on_photo) = (having(&|p| mentions(p, "nr_outliers", &json!(true))), having(&|p| mentions(p, "on_photo", &json!(true))));
        let want: BTreeMap<&str, u64> = [
            ("folder.made", 2),
            ("meta.make", having(&|p| has_text(p, "make"))),
            ("meta.lens_make", having(&|p| has_text(p, "lens_make"))),
            ("meta.flash", having(&|p| !p["meta"]["flash"].is_null())),
            ("meta.exposure_bias", having(&|p| !p["meta"]["exposure_bias"].is_null())),
            ("meta.serial_number", having(&|p| has_text(p, "serial_number"))),
            ("embedded_lens.source", having(&|p| p["embedded_lens"]["source"] == "database")),
            ("settings.detail.nr_outliers", outliers),
            ("settings.masks.on_photo", on_photo),
        ]
        .into();
        assert_eq!(ids, want, "{name}");
        assert!(want.values().all(|n| *n >= 1), "{name}: every kind of loss is in the library: {want:?}");
        assert_only_declared(&diffs_between(&library, &scratch.join("copy")), &LOST);
        assert!(report.warnings.iter().any(|w| w.contains("process")), "{name}: a rendering process newer than the build's: {:?}", report.warnings);
    }
}

/// Whether any object in `v` has `key` set to `value`.
fn mentions(v: &Value, key: &str, value: &Value) -> bool {
    match v {
        Value::Object(m) => m.get(key) == Some(value) || m.values().any(|c| mentions(c, key, value)),
        Value::Array(a) => a.iter().any(|c| mentions(c, key, value)),
        _ => false,
    }
}

/// Labels, keywords and ratings of every photo and the color labels of every folder are the same in the copy, for each
/// target, in each layout (the log tail changes some of them: they are the ones after the changes).
#[test]
fn labels_and_keywords_survive_per_photo_and_per_folder() {
    for (name, one_file, per, tail) in LAYOUTS {
        let scratch = Scratch::new("kept");
        let library = scratch.join("library");
        personal_library(&library, one_file, per, tail);
        let (photos, folders) = kept(&library);
        assert!(
            photos.values().filter(|(label, ..)| !label.is_null()).count() >= 3
                && photos.values().any(|(_, k, _)| k.as_array().is_some_and(|k| k.len() == 3))
        );
        assert_eq!(folders.len(), 2, "{name}: two folders have a label");
        if tail {
            assert_eq!(photos[&8].0, "yellow", "the label the log changed");
            assert_eq!(folders["/pictures/2019/trip"], "purple");
        }
        for target in [Target::Official, Target::Build7248] {
            let out = scratch.join(&format!("copy-{target:?}"));
            run_lossy(&library, &out, target);
            assert_eq!(kept(&out), (photos.clone(), folders.clone()), "{name} to {target:?}");
        }
    }
}

/// The report counts what the library holds.
#[test]
fn the_report_says_what_the_library_holds() {
    let scratch = Scratch::new("report");
    let library = scratch.join("library");
    personal_library(&library, false, 4, true);
    let report = run_lossy(&library, &scratch.join("copy"), Target::Official);
    assert_eq!((report.photos, report.albums, report.stacks), (10, 3, 1));
    assert_eq!((report.labeled_photos, report.keyworded_photos, report.labeled_folders, report.keyword_list), (4, 3, 2, 1));
    assert_eq!(report.target, Some(Target::Official));
    assert!(report.layout.contains("segment"), "{}", report.layout);
    let shown = report.to_string();
    assert!(shown.contains("LOSS folder.made") && shown.contains("10 photos (4 with a color label, 3 with keywords)"), "{shown}");
}

// ---- the formats

/// The names of the keys of a JSON object.
fn keys(v: &Value) -> BTreeSet<&str> {
    v.as_object().unwrap().keys().map(String::as_str).collect()
}

/// A copy for official LightCraft is one `catalog.snap` of format 12 with whole records and nothing else of a newer
/// build's: no `defaults`, no segments, no photo count, an empty log; and main's reading of it (the `catalog` object
/// parsed and repaired) is the catalog the library held.
#[test]
fn the_official_copy_is_what_main_reads() {
    const MAIN_CATALOG: [&str; 11] =
        ["photos", "albums", "next_photo", "next_album", "stacks", "next_stack", "label_names", "browsed", "keyword_list", "folder_records", "grids"];
    for (name, one_file, per, tail) in LAYOUTS {
        let scratch = Scratch::new("main");
        let library = scratch.join("library");
        personal_library(&library, one_file, per, tail);
        let out = scratch.join("copy");
        run_lossy(&library, &out, Target::Official);
        let text = fs::read_to_string(out.join(SNAPSHOT)).unwrap();
        assert!(text.starts_with(r#"{"format":"lightcraft-catalog","version":12,"seq":"#), "{name}: {}", &text[..60]);
        assert!(text.ends_with("}\n") && text.matches('\n').count() == 1, "{name}: one line");
        let doc: Value = serde_json::from_str(&text).unwrap();
        assert_eq!(keys(&doc), BTreeSet::from(["format", "version", "seq", "catalog"]), "{name}");
        assert!(keys(&doc["catalog"]).iter().all(|k| MAIN_CATALOG.contains(k)), "{name}: {:?}", keys(&doc["catalog"]));
        assert!(!out.join("segments").exists(), "{name}: main has no segments");
        assert_eq!(fs::read(out.join(LOG)).unwrap(), b"", "{name}");
        // whole records: the fields the compact form leaves out when they are the defaults are written
        for record in doc["catalog"]["photos"].as_object().unwrap().values() {
            for key in [
                "kind",
                "file_size",
                "captured",
                "meta",
                "rating",
                "flag",
                "label",
                "edited",
                "versions",
                "history",
                "deleted",
                "duration",
                "as_shot_wb",
            ] {
                assert!(record.get(key).is_some(), "{name}: a record without `{key}`");
            }
            assert!(record["develop"].get("light").is_some() && record["meta"].get("camera").is_some(), "{name}: a compact record");
        }
        // main reads the `catalog` object as it is and repairs it: the photos are in it
        let mut main_reads = Catalog::from_snapshot(&doc["catalog"].to_string()).unwrap();
        let (want, _) = read_lib(&out);
        main_reads.revision = want.revision;
        assert!(main_reads == want, "{name}");
        assert_eq!(main_reads.len(), 10);
    }
}

/// The segmentation grids the settings of several photos share are written once, in `grids`, as main's one-file snapshot does.
#[test]
fn the_official_copy_shares_the_grids_as_main_does() {
    let scratch = Scratch::new("grids");
    let library = scratch.join("library");
    personal_library(&library, false, 4, false);
    run_lossy(&library, &scratch.join("copy"), Target::Official);
    let doc: Value = serde_json::from_slice(&fs::read(scratch.join("copy").join(SNAPSHOT)).unwrap()).unwrap();
    let grids = doc["catalog"]["grids"].as_array().unwrap();
    assert_eq!(grids.len(), 1, "one grid, shared by every mask that has it: {grids:?}");
    let text = serde_json::to_string(&doc["catalog"]["photos"]).unwrap();
    assert!(!text.contains("eJxjYAAAAAEAAQ=="), "the grid is in the pool and not in the records");
    assert!(text.contains(r#""grid":0"#));
}

/// A copy for build 7248 is that build's layout: format 11, the header stating the defaults of its develop settings (the real
/// build's, without the one setting this build added), the catalog without photos, the segments listed with their size and
/// checksum, the photos in them in compact records, no photo in the snapshot.
#[test]
fn the_7248_copy_is_that_builds_layout() {
    let theirs = include_str!("fixtures/b7248-catalog.snap.json");
    let between = |text: &str| text[text.find(r#""defaults":"#).unwrap()..text.find(r#","catalog":"#).unwrap()].to_string();
    for (name, one_file, per, tail) in LAYOUTS {
        let scratch = Scratch::new("layout");
        let library = scratch.join("library");
        personal_library(&library, one_file, per, tail);
        let out = scratch.join("copy");
        run_lossy(&library, &out, Target::Build7248);
        let text = fs::read_to_string(out.join(SNAPSHOT)).unwrap();
        assert!(text.starts_with(r#"{"format":"lightcraft-catalog","version":11,"seq":"#), "{name}");
        let at = |pattern: &str| text.find(pattern).unwrap_or_else(|| panic!("{name}: no {pattern}"));
        let photos_at = text.rfind(",\"photos\":").unwrap();
        assert!(
            at(",\"defaults\":") < at(",\"catalog\":") && at(",\"catalog\":") < at("},\"segments\":[") && at("},\"segments\":[") < photos_at,
            "{name}: the order of the header"
        );
        assert_eq!(between(&text), between(theirs), "{name}: the defaults are the ones build 7248 states");
        let doc: Value = serde_json::from_str(&text).unwrap();
        assert_eq!(keys(&doc), BTreeSet::from(["format", "version", "seq", "defaults", "catalog", "segments", "photos"]), "{name}");
        assert_eq!(doc["catalog"]["photos"], json!({}), "{name}: no photo in the snapshot");
        assert_eq!(doc["photos"], 10, "{name}");
        let listed = doc["segments"].as_array().unwrap();
        assert!(!listed.is_empty(), "{name}: no segment");
        let mut in_segments = 0;
        for s in listed {
            let bytes = fs::read(out.join(s["name"].as_str().unwrap())).unwrap();
            assert_eq!((s["bytes"].as_u64(), s["crc"].as_u64()), (Some(bytes.len() as u64), Some(u64::from(crc32fast::hash(&bytes)))), "{name}: {s}");
            let segment: Value = serde_json::from_slice(&bytes).unwrap();
            assert_eq!((segment["format"].as_str(), segment["version"].as_u64()), (Some("lightcraft-catalog-segment"), Some(11)));
            in_segments += segment["photos"].as_object().unwrap().len();
        }
        assert_eq!(in_segments, 10, "{name}");
        assert_eq!(names(&out.join("segments")).len(), listed.len(), "{name}: no segment that isn't listed");
        assert_eq!(fs::read(out.join(LOG)).unwrap(), b"", "{name}");
    }
}

/// What a copy for build 7248 holds of each thing it can't hold: none of it, in any record, in any segment.
#[test]
fn the_7248_copy_holds_none_of_what_that_build_lacks() {
    let scratch = Scratch::new("none");
    let library = scratch.join("library");
    personal_library(&library, false, 4, false);
    run_lossy(&library, &scratch.join("copy"), Target::Build7248);
    let mut text = String::new();
    for entry in fs::read_dir(scratch.join("copy").join("segments")).unwrap() {
        text += &fs::read_to_string(entry.unwrap().path()).unwrap();
    }
    for what in [
        "\"make\":",
        "\"lens_make\":",
        "\"flash\":",
        "\"exposure_bias\":",
        "\"serial_number\":",
        "\"nr_outliers\":",
        "\"on_photo\":",
        "\"source\":\"database\"",
        "\"made\":",
    ] {
        assert!(!text.contains(what), "a segment of the copy holds {what}");
    }
    assert!(!fs::read_to_string(scratch.join("copy").join(SNAPSHOT)).unwrap().contains("\"made\":"));
}

// ---- what is lost

/// A copy that would lose something is not made unless asked for: the report says what, nothing is written, and the library
/// is as it was.
#[test]
fn a_loss_is_refused_without_allow_loss_and_nothing_is_written() {
    let scratch = Scratch::new("refused");
    let library = scratch.join("library");
    personal_library(&library, false, 4, true);
    let before = tree(&library);
    for (target, ids) in [(Target::Official, 1), (Target::Build7248, 9)] {
        let out = scratch.join("copy");
        match run(&library, &out, target) {
            Err(ConvertError::Losses(report)) => {
                assert_eq!(report.losses.len(), ids, "{:?}", loss_ids(&report));
                assert!(report.losses.iter().all(|l| l.records > 0 && !l.what.is_empty() && !l.effect.is_empty()));
                assert!(report.written.is_empty());
            }
            other => panic!("{target:?}: {other:?}"),
        }
        assert!(!out.exists(), "{target:?}: the copy was started");
        assert!(
            names(scratch.path()).iter().all(|n| !n.contains("converting") && !n.starts_with("copy")),
            "{target:?}: a staging folder was left: {:?}",
            names(scratch.path())
        );
        assert_eq!(tree(&library), before);
    }
}

/// A library that holds nothing the target lacks is copied without `--allow-loss`: the losses are what is there, not a
/// precaution.
#[test]
fn a_library_without_a_loss_needs_no_allow_loss() {
    let scratch = Scratch::new("none-lost");
    let library = scratch.join("library");
    let ops: Vec<Op> =
        personal_ops().into_iter().filter(|op| !matches!(op, Op::SetFolderRecord { record: Some(r), .. } if r.made.is_some())).collect();
    put_store(&library, &saved(&ops, &[], false, 4));
    // (official LightCraft holds all of what is left; build 7248 lacks some of it)
    assert!(run(&library, &scratch.join("official"), Target::Official).unwrap().losses.is_empty());
    assert!(!loss_ids(&run_lossy(&library, &scratch.join("old"), Target::Build7248)).is_empty());
}

/// A folder record that says only `made` is not kept (it says nothing else), and one with a label keeps the label.
#[test]
fn a_folder_s_label_is_kept_when_its_made_is_dropped() {
    let scratch = Scratch::new("made");
    let library = scratch.join("library");
    personal_library(&library, false, 4, false);
    for target in [Target::Official, Target::Build7248] {
        let out = scratch.join(&format!("copy-{target:?}"));
        run_lossy(&library, &out, target);
        let (c, _) = read_lib(&out);
        assert_eq!(c.folder_records.keys().cloned().collect::<Vec<_>>(), ["/pictures/2019/trip", "/pictures/2020"], "{target:?}");
        assert_eq!(c.folder_records["/pictures/2020"], FolderRecord { label: Some(ColorLabel::Red), made: None });
    }
}

/// A rendering process newer than build 7248's newest is a warning (the build keeps the number and renders it as its
/// newest) and not a loss; one that build knows says nothing.
#[test]
fn a_newer_rendering_process_is_a_warning_not_a_loss() {
    let scratch = Scratch::new("process");
    let library = scratch.join("library");
    let op = |id: u64, process: Process| Op::AddPhoto { photo: Box::new(photo(id, |p| p.develop = edited(|s| s.process = process.version()))) };
    put_store(&library, &saved(&[op(1, Process::V2)], &[], false, 4));
    let report = run(&library, &scratch.join("a"), Target::Build7248).unwrap();
    assert!(report.warnings.is_empty() && report.losses.is_empty());
    put_store(&scratch.join("newer"), &saved(&[op(1, Process::V2), op(2, Process::V6)], &[], false, 4));
    let report = run(&scratch.join("newer"), &scratch.join("b"), Target::Build7248).unwrap();
    assert!(report.losses.is_empty());
    assert!(report.warnings.len() == 1 && report.warnings[0].contains("1 photo(s)"), "{:?}", report.warnings);
    assert!(run(&scratch.join("newer"), &scratch.join("c"), Target::Official).unwrap().warnings.is_empty());
}

/// Every kind of thing build 7248 can't hold is found in each place a photo has settings (the photo's, a version's, a step of
/// its history, its import look) and in the metadata, cleared from a copy of the photo only: the photo it came from, and the
/// settings it shares with others, keep them.
#[test]
fn what_7248_lacks_is_found_where_a_photo_holds_it_and_cleared_in_the_copy_only() {
    let cases: Vec<(&str, &str, Box<dyn Fn(&mut Photo)>)> = vec![
        ("settings.detail.nr_outliers", "develop", Box::new(|p| p.develop = outliers())),
        ("settings.detail.nr_outliers", "a version", Box::new(|p| p.versions = vec![a_version(outliers())])),
        ("settings.detail.nr_outliers", "a history step", Box::new(|p| p.history = vec![a_step(outliers())])),
        ("settings.detail.nr_outliers", "the import look", Box::new(|p| p.import_look = Some(outliers()))),
        ("settings.masks.on_photo", "develop", Box::new(|p| p.develop = masked())),
        ("settings.masks.on_photo", "a version", Box::new(|p| p.versions = vec![a_version(masked())])),
        ("settings.masks.on_photo", "a history step", Box::new(|p| p.history = vec![a_step(masked())])),
        ("settings.masks.on_photo", "the import look", Box::new(|p| p.import_look = Some(masked()))),
        ("meta.make", "a maker", Box::new(|p| p.meta.make = "M".into())),
        ("meta.lens_make", "a lens maker", Box::new(|p| p.meta.lens_make = "M".into())),
        ("meta.flash", "no flash", Box::new(|p| p.meta.flash = Some(false))),
        ("meta.exposure_bias", "no bias", Box::new(|p| p.meta.exposure_bias = Some(0.0))),
        ("meta.serial_number", "a serial number", Box::new(|p| p.meta.serial_number = "1".into())),
        (
            "embedded_lens.source",
            "a database lens",
            Box::new(|p| p.embedded_lens = Some(EmbeddedLens { source: LensSource::Database, ..Default::default() })),
        ),
    ];
    for (id, holder, set) in &cases {
        let mut p = plain(1);
        set(&mut p);
        assert_eq!(found_in(&p), [*id], "{id} in {holder}");
        let mut cleared = p.clone();
        clear(&mut cleared);
        assert!(found_in(&cleared).is_empty(), "{id} in {holder} is still there after it is cleared");
        assert_eq!(found_in(&p), [*id], "{id} in {holder}: clearing the copy changed the photo it was made from");
    }
    // not found: a file lens, nothing set, the settings the build has
    let mut p = plain(1);
    p.embedded_lens = Some(EmbeddedLens::default());
    p.develop = edited(|s| s.masks = vec![Mask { on_photo: false, ..Mask::default() }]);
    assert!(found_in(&p).is_empty());
}

// ---- what this build's reader can't see

/// The real 7248 segment with `edit` applied to it, as a library folder's files (the listing of the segment, its size and
/// checksum, is made to match, as that build would write it).
fn b7248_edited(edit: impl FnOnce(&mut String), albums: &str) -> Vec<(String, Vec<u8>)> {
    let mut segment = include_str!("fixtures/b7248-segments-1-0.json").to_string();
    edit(&mut segment);
    let snap = include_str!("fixtures/b7248-catalog.snap.json")
        .replace(r#""bytes":11387,"crc":1975744581"#, &format!(r#""bytes":{},"crc":{}"#, segment.len(), crc32fast::hash(segment.as_bytes())))
        .replacen(r#""albums":{"#, &format!(r#""albums":{{{albums}"#), 1);
    vec![(SNAPSHOT.to_string(), snap.into_bytes()), ("segments/1-0.json".to_string(), segment.into_bytes())]
}

fn put_owned(dir: &Path, files: &[(String, Vec<u8>)]) {
    put(dir, &files.iter().map(|(k, v)| (k.as_str(), v.as_slice())).collect::<Vec<_>>());
}

/// Build 7248 has fields this build's catalog has no place for: what a sync chose to leave in the file, a truncated video, the
/// keywords, albums, dates and folders of a smart album's rules. This build's reader drops them, so each one found in the
/// library is a loss the copy would hide: it is reported, and the copy is not made without `--allow-loss`.
#[test]
fn fields_this_build_has_no_place_for_are_reported_as_losses() {
    let scratch = Scratch::new("unknown");
    let library = scratch.join("library");
    let smart = r#""9":{"id":9,"name":"Boats and more","parent":null,"folder":false,"photos":[],"cover":null,"smart":{"keywords":["boats","sea"]}},"#;
    put_owned(&library, &b7248_edited(|s| *s = s.replacen(r#""id":1,"#, r#""id":1,"left_in_file":{"keywords":true},"truncated":true,"#, 1), smart));
    let before = tree(&library);
    let Err(ConvertError::Losses(report)) = run(&library, &scratch.join("copy"), Target::Official) else { panic!("the copy was made") };
    let found: BTreeMap<&str, u64> = report.losses.iter().map(|l| (l.id.as_str(), l.records)).collect();
    assert_eq!(
        found,
        BTreeMap::from([("input:photo.left_in_file", 1), ("input:photo.truncated", 1), ("input:catalog.albums.*.smart.keywords", 1)]),
        "{report}"
    );
    assert!(!scratch.join("copy").exists());
    assert_eq!(tree(&library), before);
    // with `--allow-loss` the rest is copied, and the report names what was not
    let report = run_lossy(&library, &scratch.join("copy"), Target::Official);
    assert_eq!(report.losses.len(), 3);
    let (copy, _) = read_lib(&scratch.join("copy"));
    assert_eq!(copy.len(), 24);
}

/// The values that say nothing (null, false, zero, empty), and the settings a record leaves at the library's defaults, are not
/// reported: they are what the reader makes of a field it never heard of anyway.
#[test]
fn fields_that_say_nothing_are_not_losses() {
    let scratch = Scratch::new("nothing");
    let library = scratch.join("library");
    let said_nothing = r#""id":1,"truncated":false,"unheard_of":null,"other":[],"more":{},"#;
    put_owned(&library, &b7248_edited(|s| *s = s.replacen(r#""id":1,"#, said_nothing, 1), ""));
    assert!(run(&library, &scratch.join("copy"), Target::Official).unwrap().losses.is_empty());
}

/// A damaged record in the middle of the log is a loss (what follows it is not applied, as a build's open does); a torn
/// last record is only a note.
#[test]
fn a_damaged_log_is_a_loss_and_a_torn_one_a_note() {
    let scratch = Scratch::new("damaged");
    let library = scratch.join("library");
    let (_, files, _) = historic().into_iter().find(|(n, ..)| *n == "format 6").unwrap();
    put(&library, &files);
    let log = String::from_utf8(fs::read(library.join(LOG)).unwrap()).unwrap();
    let mut lines: Vec<String> = log.lines().map(String::from).collect();
    lines[2] = lines[2].replacen("\"op\":{\"op\":\"", "\"op\":{\"op\":\"x", 1);
    fs::write(library.join(LOG), format!("{}\n", lines.join("\n"))).unwrap();
    let before = tree(&library);
    let Err(ConvertError::Losses(report)) = run(&library, &scratch.join("copy"), Target::Official) else { panic!("the copy was made") };
    assert_eq!(loss_ids(&report), ["log.damaged"]);
    assert_eq!(tree(&library), before, "a build's open keeps the damaged log aside: this tool leaves the library as it is");
    // torn: the last record is cut short
    let torn = &log[..log.len() - 15];
    fs::write(library.join(LOG), torn).unwrap();
    let report = run(&library, &scratch.join("copy"), Target::Official).unwrap();
    assert!(report.losses.is_empty() && report.notes.iter().any(|n| n.contains("torn")), "{report}");
}

// ---- hostile and damaged libraries

fn unreadable(library: &Path, scratch: &Scratch) -> String {
    let mut reasons = Vec::new();
    for target in [Target::Official, Target::Build7248] {
        match run(library, &scratch.join("copy"), target) {
            Err(ConvertError::Unreadable(why)) => reasons.push(why),
            other => panic!("{target:?}: expected an unreadable library, got {other:?}"),
        }
        assert!(!scratch.join("copy").exists(), "{target:?}: a copy was started");
    }
    reasons.remove(0)
}

/// A library that can't be read the way a build reads it is an error, never a panic and never a copy: a snapshot that isn't
/// JSON, one of a newer build, a segment with the wrong checksum, a missing one, a segment named to lead out of the folder,
/// nesting a parser would overflow on, a folder that is no library.
#[test]
fn a_library_that_can_t_be_read_is_an_error_and_nothing_is_written() {
    let scratch = Scratch::new("hostile");
    let library = scratch.join("library");
    let snap = include_str!("fixtures/b7248-catalog.snap.json");
    let segment = &include_bytes!("fixtures/b7248-segments-1-0.json")[..];
    let cases: Vec<(&str, Vec<(&str, Vec<u8>)>)> = vec![
        ("not json", vec![(SNAPSHOT, b"this is not a catalog".to_vec())]),
        ("cut short", vec![(SNAPSHOT, snap.as_bytes()[..400].to_vec()), ("segments/1-0.json", segment.to_vec())]),
        (
            "newer build",
            vec![(SNAPSHOT, snap.replacen(r#""version":11"#, r#""version":99"#, 1).into_bytes()), ("segments/1-0.json", segment.to_vec())],
        ),
        (
            "wrong checksum",
            vec![(SNAPSHOT, snap.as_bytes().to_vec()), ("segments/1-0.json", segment.iter().map(|b| if *b == b'4' { b'5' } else { *b }).collect())],
        ),
        ("segment missing", vec![(SNAPSHOT, snap.as_bytes().to_vec())]),
        ("a way out of the folder", vec![(SNAPSHOT, snap.replace("segments/1-0.json", "segments/../../victim.json").into_bytes())]),
        (
            "deep nesting",
            vec![(SNAPSHOT, format!(r#"{{"format":"lightcraft-catalog","version":12,"seq":0,"catalog":{}}}"#, "[".repeat(20_000)).into_bytes())],
        ),
        (
            "a seq no log reaches",
            vec![(SNAPSHOT, br#"{"format":"lightcraft-catalog","version":12,"seq":18446744073709551615,"catalog":{"photos":{}}}"#.to_vec())],
        ),
    ];
    for (what, files) in cases {
        let _ = fs::remove_dir_all(&library);
        // (where the segment the snapshot names leads to: it is valid there, and still not read)
        fs::write(scratch.join("victim.json"), segment).unwrap();
        put(&library, &files.iter().map(|(k, v)| (*k, v.as_slice())).collect::<Vec<_>>());
        let why = unreadable(&library, &scratch);
        assert!(!why.is_empty(), "{what}");
        assert!(dump_fails(&library), "{what}");
    }
    // not a library at all
    let _ = fs::remove_dir_all(&library);
    fs::create_dir_all(&library).unwrap();
    assert!(unreadable(&library, &scratch).contains("not a library"));
}

fn dump_fails(library: &Path) -> bool {
    convert::dump(library, &mut Vec::new()).is_err()
}

/// A snapshot that is a link, or a segments folder that is, is not followed out of the library.
#[cfg(unix)]
#[test]
fn a_link_in_the_place_of_the_segments_is_refused() {
    let scratch = Scratch::new("link-segments");
    let library = scratch.join("library");
    let (_, files, _) = historic().into_iter().find(|(n, ..)| *n == "build 7248, closed").unwrap();
    put(&library, &files[..1]);
    let elsewhere = scratch.join("elsewhere");
    put(&elsewhere, &[("1-0.json", &include_bytes!("fixtures/b7248-segments-1-0.json")[..])]);
    std::os::unix::fs::symlink(&elsewhere, library.join("segments")).unwrap();
    assert!(unreadable(&library, &scratch).contains("link"));
}

// ---- the library is only read

/// `out` is a new folder outside the library: the library itself, a new folder in it, a folder it holds, the same folder by
/// another name (`..`, a link, on a case-insensitive volume another spelling) are all refused before anything is read, and
/// nothing is created.
#[test]
fn out_inside_the_library_is_refused() {
    let scratch = Scratch::new("inside");
    let library = scratch.join("library");
    personal_library(&library, false, 4, false);
    fs::create_dir_all(library.join("thumbs/ab")).unwrap();
    let before = tree(&library);
    let mut outs: Vec<PathBuf> = vec![
        library.clone(),
        library.join("copy"),
        library.join("a/b/c"),
        library.join("thumbs"),
        library.join("thumbs/ab"),
        library.join("segments"),
        scratch.join("library/../library/copy"),
        scratch.join("library/thumbs/../../library"),
        scratch.join("elsewhere/../library/new"),
    ];
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(&library, scratch.join("alias")).unwrap();
        outs.extend([scratch.join("alias"), scratch.join("alias/copy"), scratch.join("alias/thumbs/ab")]);
    }
    if case_insensitive(scratch.path()) {
        outs.extend([scratch.join("LIBRARY"), scratch.join("Library/Copy"), scratch.join("LIBRARY/thumbs")]);
    }
    for out in &outs {
        for target in [Target::Official, Target::Build7248] {
            let why = refusal(run(&library, out, target));
            assert!(why.contains("library"), "{}: {why}", out.display());
        }
    }
    assert_eq!(tree(&library), before, "something was written in the library");
}

fn case_insensitive(dir: &Path) -> bool {
    let probe = dir.join("CaseProbe");
    fs::write(&probe, b"").unwrap();
    let same = dir.join("caseprobe").exists();
    fs::remove_file(&probe).unwrap();
    same
}

/// `out` may be a folder that doesn't exist yet (and its parents, which are made) or one that is empty, and nothing that
/// holds anything, nor a file.
#[test]
fn out_must_be_new_or_empty() {
    let scratch = Scratch::new("out");
    let library = scratch.join("library");
    personal_library(&library, false, 4, false);
    let before = tree(&library);
    fs::create_dir_all(scratch.join("full")).unwrap();
    fs::write(scratch.join("full/file"), b"mine").unwrap();
    fs::write(scratch.join("a-file"), b"mine").unwrap();
    for out in ["full", "a-file", "library/../full"] {
        let why = refusal(run_lossy_result(&library, &scratch.join(out)));
        assert!(why.contains("not empty") || why.contains("not a folder") || why.contains("exists"), "{out}: {why}");
    }
    assert_eq!(fs::read(scratch.join("full/file")).unwrap(), b"mine");
    assert_eq!(fs::read(scratch.join("a-file")).unwrap(), b"mine");
    assert_eq!(names(&scratch.join("full")), BTreeSet::from(["file".to_string()]));
    // an empty folder, and a folder in folders that don't exist
    fs::create_dir_all(scratch.join("empty")).unwrap();
    run_lossy(&library, &scratch.join("empty"), Target::Official);
    assert!(scratch.join("empty").join(SNAPSHOT).is_file());
    run_lossy(&library, &scratch.join("x/y/z"), Target::Official);
    assert!(scratch.join("x/y/z").join(SNAPSHOT).is_file());
    assert_eq!(tree(&library), before);
    // no staging folder is left beside any of them
    for dir in [scratch.path().to_path_buf(), scratch.join("x/y")] {
        assert!(names(&dir).iter().all(|n| !n.contains("converting")), "{:?}", names(&dir));
    }
}

fn run_lossy_result(library: &Path, out: &Path) -> Result<Report, ConvertError> {
    convert::convert(library, out, Target::Official, Options { allow_loss: true, ..Options::default() })
}

/// A library that a program has open (it holds the library's lock) is refused, for both the copy and the dump, and nothing is
/// written or created; closed, it converts.
#[test]
fn a_library_that_is_open_is_refused() {
    let scratch = Scratch::new("locked");
    let library = scratch.join("library");
    personal_library(&library, false, 4, false);
    let lock = LibraryLock::acquire(&library, "a program that has the library open").unwrap();
    let before = tree(&library);
    for target in [Target::Official, Target::Build7248] {
        let why = refusal(run_lossy_result_for(&library, &scratch.join("copy"), target));
        assert!(why.contains("open") && why.contains("Nothing was read or written"), "{why}");
        assert!(!scratch.join("copy").exists());
    }
    assert!(matches!(convert::dump(&library, &mut Vec::new()), Err(ConvertError::Refused(_))));
    assert_eq!(tree(&library), before, "the lock files and the library were not touched");
    drop(lock);
    run_lossy(&library, &scratch.join("copy"), Target::Official);
}

fn run_lossy_result_for(library: &Path, out: &Path, target: Target) -> Result<Report, ConvertError> {
    convert::convert(library, out, target, Options { allow_loss: true, ..Options::default() })
}

/// Whatever happens, the library's folder is byte for byte, entry for entry and date for date what it was: after a copy to
/// either target, after a refused one, after a failed one and after a dump, in each layout.
#[test]
fn the_library_is_never_written() {
    for (name, one_file, per, tail) in LAYOUTS {
        let scratch = Scratch::new("read-only");
        let library = scratch.join("library");
        personal_library(&library, one_file, per, tail);
        put(&library, &[("presets.json", b"[]"), ("thumbs/ab/1.jpg", b"t"), ("prefs.json", b"{}")]);
        let before = tree(&library);
        let _ = run(&library, &scratch.join("refused"), Target::Official);
        let _ = run(&library, &library.join("inside"), Target::Build7248);
        run_lossy(&library, &scratch.join("official"), Target::Official);
        run_lossy(&library, &scratch.join("old"), Target::Build7248);
        convert::dump(&library, &mut Vec::new()).unwrap();
        assert_eq!(tree(&library), before, "{name}");
        assert!(
            !names(&library).iter().any(|n| n.starts_with("catalog.lock") || n.contains("damaged") || n.contains(".tmp")),
            "{name}: {:?}",
            names(&library)
        );
    }
}

// ---- the other files

/// What is copied beside the catalog, what is rewritten and what is left out (with the reason), by name.
fn library_with_everything(dir: &Path) {
    personal_library(dir, false, 4, false);
    put(
        dir,
        &[
            ("presets.json", b"[]"),
            ("view.json", b"{}"),
            ("prefs.json", b"{}"),
            ("thumbs/ab/1.jpg", b"thumb"),
            ("Smart Previews/1.dng", b"preview"),
            ("Originals/a.jpg", b"original"),
            ("Interop/x.xmp", b"xmp"),
            ("catalog.lock", b""),
            ("catalog.lock.owner", b"{}"),
            ("Backups/b1/catalog.snap", b"old"),
            ("Replaced/x", b"old"),
            ("backup.json", b"{}"),
            ("catalog.snap.v12-3", b"older format"),
            ("catalog.log.damaged-1", b"damaged"),
            ("prefs.json.corrupt-1", b"corrupt"),
            ("notes.txt", b"mine"),
            (".DS_Store", b"x"),
            ("half.tmp", b"x"),
        ],
    );
}

const COPIED: [&str; 7] = ["presets.json", "view.json", "prefs.json", "thumbs/", "Smart Previews/", "Originals/", "Interop/"];
const LEFT_OUT: [&str; 11] = [
    "catalog.lock",
    "catalog.lock.owner",
    "Backups/",
    "Replaced/",
    "backup.json",
    "catalog.snap.v12-3",
    "catalog.log.damaged-1",
    "prefs.json.corrupt-1",
    "notes.txt",
    ".DS_Store",
    "half.tmp",
];

/// The files the target uses are copied (with their dates), the catalog files are written for it, and everything else is left
/// out and listed, whichever name it has: nothing unknown is copied, nothing is dropped without a line saying so.
#[test]
fn files_are_copied_rewritten_or_left_out_by_name() {
    let scratch = Scratch::new("files");
    let library = scratch.join("library");
    library_with_everything(&library);
    let old = SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(1_500_000_000);
    fs::File::options().write(true).open(library.join("thumbs/ab/1.jpg")).unwrap().set_modified(old).unwrap();
    let before = tree(&library);
    for target in [Target::Official, Target::Build7248] {
        let out = scratch.join(&format!("copy-{target:?}"));
        let report = run_lossy(&library, &out, target);
        let mut want: BTreeSet<String> =
            ["catalog.snap", "catalog.log", "thumbs/ab/", "thumbs/ab/1.jpg", "Smart Previews/1.dng", "Originals/a.jpg", "Interop/x.xmp"]
                .map(String::from)
                .into();
        want.extend(COPIED.map(String::from));
        if target == Target::Build7248 {
            want.insert("segments/".into());
            want.extend(names(&out).into_iter().filter(|n| n.starts_with("segments/")));
        }
        assert_eq!(names(&out), want, "{target:?}");
        assert_eq!(fs::metadata(out.join("thumbs/ab/1.jpg")).unwrap().modified().unwrap(), old, "{target:?}: the date of a copied file");
        for name in LEFT_OUT {
            assert!(
                report.left_out.iter().any(|l| l.starts_with(&format!("{name}:"))),
                "{target:?}: `{name}` is not listed as left out: {:?}",
                report.left_out
            );
        }
        assert!(report.copied.iter().any(|c| c.starts_with("thumbs/: 1 file(s), 5 bytes")), "{:?}", report.copied);
        assert_eq!(report.left_out.iter().any(|l| l.starts_with("segments/:")), target == Target::Official, "{target:?}");
        assert!(report.rewritten.iter().any(|r| r.starts_with("catalog.snap:")) && report.rewritten.iter().any(|r| r.starts_with("catalog.log:")));
    }
    assert_eq!(tree(&library), before);
}

/// `--catalog-only` leaves out the caches and the originals and lists them.
#[test]
fn catalog_only_leaves_the_caches_and_originals_out() {
    let scratch = Scratch::new("catalog-only");
    let library = scratch.join("library");
    library_with_everything(&library);
    let report = convert::convert(&library, &scratch.join("copy"), Target::Official, Options { allow_loss: true, catalog_only: true }).unwrap();
    assert_eq!(
        names(&scratch.join("copy")),
        BTreeSet::from(["catalog.snap", "catalog.log", "presets.json", "view.json", "prefs.json"].map(String::from))
    );
    for name in ["thumbs/", "Smart Previews/", "Originals/", "Interop/"] {
        assert!(report.left_out.iter().any(|l| l.starts_with(&format!("{name}:")) && l.contains("--catalog-only")), "{name}");
    }
}

/// A link is never followed: a link as a file of the library, or inside a folder that is copied, is left out and listed.
#[cfg(unix)]
#[test]
fn links_are_not_followed() {
    let scratch = Scratch::new("links");
    let library = scratch.join("library");
    personal_library(&library, false, 4, false);
    put(&library, &[("thumbs/ab/1.jpg", b"t")]);
    let secret = scratch.join("secret");
    fs::create_dir_all(&secret).unwrap();
    fs::write(secret.join("private.txt"), b"private").unwrap();
    std::os::unix::fs::symlink(&secret, library.join("Originals")).unwrap();
    std::os::unix::fs::symlink(secret.join("private.txt"), library.join("thumbs/ab/link")).unwrap();
    let report = run_lossy(&library, &scratch.join("copy"), Target::Official);
    assert!(
        !names(&scratch.join("copy")).iter().any(|n| n.contains("private") || n.contains("Originals") || n.ends_with("link")),
        "{:?}",
        names(&scratch.join("copy"))
    );
    assert!(report.left_out.iter().any(|l| l.starts_with("Originals:") && l.contains("link")), "{:?}", report.left_out);
    assert!(report.left_out.iter().any(|l| l.starts_with("thumbs/ab/link:")), "{:?}", report.left_out);
}

/// A copy that fails half way leaves nothing: no `out`, no staging folder beside it, and the library as it was.
#[cfg(unix)]
#[test]
fn a_copy_that_fails_leaves_nothing() {
    use std::os::unix::fs::PermissionsExt;
    let scratch = Scratch::new("fails");
    let library = scratch.join("library");
    personal_library(&library, false, 4, false);
    put(&library, &[("thumbs/ab/1.jpg", b"t"), ("thumbs/ab/2.jpg", b"t")]);
    let hidden = library.join("thumbs/ab/2.jpg");
    let before = tree(&library);
    fs::set_permissions(&hidden, fs::Permissions::from_mode(0o000)).unwrap();
    if fs::read(&hidden).is_ok() {
        // (running as a user that can read anything: nothing to fail on)
        fs::set_permissions(&hidden, fs::Permissions::from_mode(0o644)).unwrap();
        return;
    }
    let result = run_lossy_result(&library, &scratch.join("copy"));
    fs::set_permissions(&hidden, fs::Permissions::from_mode(0o644)).unwrap();
    assert!(matches!(result, Err(ConvertError::Io(_))), "{result:?}");
    assert!(!scratch.join("copy").exists());
    assert!(names(scratch.path()).iter().all(|n| !n.contains("converting")), "{:?}", names(scratch.path()));
    assert_eq!(tree(&library), before);
}

// ---- no field is dropped without a name

/// The leaf key paths of `v`: object keys by name, array elements as `[]`; the keys in `skip` are not followed.
fn leaves(v: &Value, path: &str, skip: &[&str], out: &mut BTreeSet<String>) {
    match v {
        Value::Object(m) if !m.is_empty() => {
            for (key, child) in m.iter().filter(|(k, _)| !skip.contains(&k.as_str())) {
                leaves(child, &format!("{path}.{key}"), skip, out);
            }
        }
        Value::Array(a) if !a.is_empty() => a.iter().for_each(|child| leaves(child, &format!("{path}[]"), skip, out)),
        _ => {
            out.insert(path.to_string());
        }
    }
}

/// The key paths of a develop settings tree: groups and settings by name down to each value, lists and the like whole.
fn setting_paths(v: &Value, path: &str, out: &mut BTreeSet<String>) {
    match v {
        Value::Object(m) if !m.is_empty() => {
            m.iter().for_each(|(k, c)| setting_paths(c, &if path.is_empty() { k.clone() } else { format!("{path}.{k}") }, out))
        }
        _ => {
            out.insert(path.to_string());
        }
    }
}

/// The key paths of everything a fully set personal catalog writes, except the develop settings (their own test is below).
fn field_paths() -> BTreeSet<String> {
    const TABLES: [&str; 6] = ["albums", "stacks", "label_names", "browsed", "keyword_list", "folder_records"];
    let c = personal();
    let mut paths = BTreeSet::new();
    let body = serde_json::to_value(c.without_photos()).unwrap();
    for (table, value) in body.as_object().unwrap() {
        match value {
            Value::Object(entries) if TABLES.contains(&table.as_str()) => {
                entries.values().for_each(|e| leaves(e, &format!("catalog.{table}.*"), &[], &mut paths))
            }
            _ => leaves(value, &format!("catalog.{table}"), &[], &mut paths),
        }
    }
    for p in c.photos() {
        let v = serde_json::to_value(p.as_ref()).unwrap();
        leaves(&v, "photo", &["develop", "settings", "import_look"], &mut paths);
        // (the settings: only the lists of records inside them, which are not in the defaults)
        leaves(&v["develop"]["masks"], "photo.develop.masks", &[], &mut paths);
    }
    paths
}

/// Writes `fixtures/convert-fields.txt`: the key paths of a fully set catalog, which `every_field_is_classified` keeps.
/// Run `cargo test -p lightcraft-catalog write_the_field_list -- --ignored` after classifying a new field.
#[test]
#[ignore = "writes fixtures/convert-fields.txt"]
fn write_the_field_list() {
    let text: String = field_paths().into_iter().map(|p| format!("{p}\n")).collect();
    fs::write(Path::new(env!("CARGO_MANIFEST_DIR")).join("src/fixtures/convert-fields.txt"), text).unwrap();
}

/// A field this build gets that main or build 7248 lack is silently dropped by their readers: so each field of a catalog is
/// written down, and one that is new fails here until it is classified in `convert/strip.rs` (`DROPS`, for both targets) and
/// added to `fixtures/convert-fields.txt` (`write_the_field_list`). Two lists are kept: the develop settings, by comparing this
/// build's with the defaults build 7248 states (the only difference is the setting `DROPS` names), and the rest of a fully set
/// catalog's fields.
#[test]
fn every_field_is_classified() {
    let theirs: Value = serde_json::from_str::<Value>(include_str!("fixtures/b7248-catalog.snap.json")).unwrap()["defaults"].clone();
    let (mut ours, mut b7248) = (BTreeSet::new(), BTreeSet::new());
    setting_paths(&stored::base().1, "", &mut ours);
    setting_paths(&theirs, "", &mut b7248);
    assert_eq!(
        ours.difference(&b7248).cloned().collect::<Vec<_>>(),
        ["detail.nr_outliers"],
        "a develop setting this build has and build 7248 lacks: classify it in DROPS"
    );
    assert_eq!(
        b7248.difference(&ours).cloned().collect::<Vec<_>>(),
        Vec::<String>::new(),
        "a setting build 7248 has and this build lacks is dropped when this build reads the library"
    );
    let pinned: BTreeSet<String> = include_str!("fixtures/convert-fields.txt").lines().map(String::from).collect();
    let now = field_paths();
    assert_eq!(
        (now.difference(&pinned).cloned().collect::<Vec<_>>(), pinned.difference(&now).cloned().collect::<Vec<_>>()),
        (Vec::new(), Vec::new()),
        "the fields of a catalog changed (new, gone): classify them in convert/strip.rs and write the list again"
    );
    // every field the 7248 target drops is in the list, and the one the official target drops
    for dropped in [
        "photo.meta.make",
        "photo.meta.lens_make",
        "photo.meta.flash",
        "photo.meta.exposure_bias",
        "photo.meta.serial_number",
        "photo.embedded_lens.source",
        "photo.develop.masks[].on_photo",
        "catalog.folder_records.*.made",
    ] {
        assert!(now.contains(dropped), "{dropped}");
    }
}
