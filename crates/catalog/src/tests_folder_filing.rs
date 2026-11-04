//! A question about one library folder looks at the photos filed at or below it, not at every photo
//! of the library (the folder index, see [`crate::index`]), and is answered as a scan of every photo
//! would answer it, through every op, inverse, failed batch, snapshot, replay and clone.
//!
//! Scenarios, in the words of someone who opens a folder of a big library:
//!
//! * Given a library of many folders, showing one looks at that folder's photos only, however
//!   often it is shown and whichever folder is chosen next.
//! * Given photos moved, renamed, relinked, removed (and brought back by undo), browsed, added to
//!   the library or deleted, a folder shows exactly the photos a scan of the library says it holds.
//! * However a folder is spelled, and in whatever letter case, it shows what a scan says.
//! * A view that holds photos of other folders still lists them.
//! * To tell which disks a folder reaches, one photo of each folder in it is enough.
//! * An index built on several threads is the index one walk builds.

use std::collections::{BTreeMap, BTreeSet, HashSet};

use proptest::prelude::*;

use crate::*;

/// Where photos are put: odd spellings of one folder, the same folders in other letter cases, folders
/// that share the start of a name, the root of a disk, a share, another drive, and no folder at all.
const PATHS: &[&str] = &[
    "/lib/a/1.jpg",
    "/lib/a/2.jpg",
    "/lib/a/sub/3.jpg",
    "/lib/b/4.jpg",
    "/lib/ab/5.jpg",
    "/lib//a/./6.jpg",
    "/lib/x/../a/7.jpg",
    "/lib\\a\\8.jpg",
    "/9.jpg",
    "/Volumes/nas/10.jpg",
    "/Volumes/nas/p/11.jpg",
    "//srv/share/12.jpg",
    "\\\\srv\\share\\a\\13.jpg",
    "C:\\lib\\a\\14.jpg",
    "15.jpg",
    "/Lib/a/16.jpg",
    "/lib/A/17.jpg",
    "/LIB/a/sub/18.jpg",
    "/Volumes/NAS/19.jpg",
    "c:\\LIB\\a\\20.jpg",
];

/// Folders asked for, spelled differently from the photos' and from each other.
const ROOTS: &[&str] = &[
    "/",
    "/lib",
    "/lib/a",
    "/lib//a/",
    "/lib/a/sub",
    "/lib/b",
    "/lib/ab",
    "/lib/x/../a",
    "/Volumes",
    "/Volumes/nas",
    "//srv/share",
    "//srv/share/",
    "C:\\lib",
    "C:\\",
    "c:/lib/a",
    "/nowhere",
    "/Lib",
    "/lib/A",
    "/LIB/a/",
    "/Volumes/NAS",
    "C:\\LIB\\a",
];

fn photo(id: PhotoId, source: Source, local: bool) -> Photo {
    let name = match &source {
        Source::File { path } => path.rsplit(['/', '\\']).next().unwrap_or("x.jpg").to_string(),
        Source::Demo { .. } => "demo.jpg".to_string(),
    };
    let mut p = Photo::new(id, source, &name, "JPEG", 60, 40, "2026-01-01T10:00:00");
    p.local = local;
    p
}

fn file(path: &str) -> Source {
    Source::File { path: path.into() }
}

fn add(c: &mut Catalog, path: &str) -> PhotoId {
    let id = c.alloc_photo_id();
    c.apply(Op::AddPhoto { photo: Box::new(photo(id, file(path), false)) }).unwrap();
    id
}

fn folder(root: &str) -> Filter {
    Filter { library_folder: Some(root.into()), ..Default::default() }
}

/// The photos `f` lists, by id (how they are ordered is for the sort to say).
fn shown(c: &Catalog, f: &Filter) -> Vec<PhotoId> {
    let mut ids = c.query(f, &Sort::default());
    ids.sort_unstable();
    ids
}

/// What a scan of every photo says for `f`, one photo at a time (no index): what a query was before
/// photos were looked up by folder.
fn scanned(c: &Catalog, f: &Filter, sort: &Sort) -> Vec<PhotoId> {
    let mut v: Vec<&Photo> = c.photos().map(|p| p.as_ref()).filter(|p| f.matches(p, c)).collect();
    v.sort_by(|a, b| sort.order(a, b));
    v.into_iter().map(|p| p.id).collect()
}

/// Every question about `roots` (and the same photos in Recently Deleted) is answered as a scan answers it.
fn agree_on(c: &Catalog, roots: &[&str], when: &str) {
    for sort in [Sort::default(), Sort { key: SortKey::FileName, ascending: true, ..Default::default() }] {
        for root in roots {
            for deleted in [false, true] {
                let f = Filter { deleted, ..folder(root) };
                let want = scanned(c, &f, &sort);
                assert_eq!(c.query(&f, &sort), want, "{when}: the photos of {root:?} (deleted: {deleted})");
                assert_eq!(c.count(&f), want.len(), "{when}: the count of {root:?} (deleted: {deleted})");
                assert_eq!(c.any(&f), !want.is_empty(), "{when}: any of {root:?} (deleted: {deleted})");
                samples_are_the_first_of_each_folder(c, &f, &want, when);
            }
        }
    }
}

fn agree(c: &Catalog, when: &str) {
    agree_on(c, ROOTS, when);
}

/// `want` are the photos matching `f`: the samples are the first (lowest id) of each folder they are in,
/// a folder being what the index files by, letter case set aside.
fn samples_are_the_first_of_each_folder(c: &Catalog, f: &Filter, want: &[PhotoId], when: &str) {
    let key_of = |id: PhotoId| match &c.photo(id)?.source {
        Source::File { path } => photo_folder_index_key(path),
        Source::Demo { .. } => None,
    };
    let mut first: BTreeMap<String, PhotoId> = BTreeMap::new();
    for id in want {
        if let Some(key) = key_of(*id) {
            first.entry(key).and_modify(|lowest| *lowest = (*lowest).min(*id)).or_insert(*id);
        }
    }
    let mut expected: Vec<PhotoId> = first.into_values().collect();
    expected.sort_unstable();
    let samples = c.folder_samples(f);
    assert_eq!(samples, expected, "{when}: the first photo of each folder with a match, in id order");
    // what the disk check reads from them: it says what it says of every match
    if let Some(root) = &f.library_folder {
        let covers = |ids: &[PhotoId]| {
            let volumes = ids.iter().filter_map(|id| c.photo(*id)).filter_map(|p| crate::folders::volume_of(p));
            volumes.into_iter().any(|v| v != "/" && !crate::query::folder_within(root, &v))
        };
        assert_eq!(covers(&samples), covers(want), "{when}: the disk check of {root:?}");
    }
}

/// How many photos `ask` made the catalog look at.
fn looked(c: &Catalog, ask: impl FnOnce(&Catalog)) -> u64 {
    let before = c.photos_looked_at();
    ask(c);
    c.photos_looked_at() - before
}

const FOLDERS: u64 = 40;
const PER_FOLDER: u64 = 25;
const TOTAL: u64 = FOLDERS * PER_FOLDER;

/// 40 folders of 25 photos.
fn library() -> Catalog {
    let mut c = Catalog::new();
    let ops = (0..FOLDERS)
        .flat_map(|d| (0..PER_FOLDER).map(move |i| (d, i)))
        .enumerate()
        .map(|(n, (d, i))| Op::AddPhoto { photo: Box::new(photo(PhotoId(n as u64 + 1), file(&format!("/lib/f{d}/p{i}.jpg")), false)) })
        .collect();
    c.apply(Op::Batch { ops }).unwrap();
    c
}

#[test]
fn showing_a_folder_looks_at_its_photos_not_the_librarys() {
    let c = library();
    let f = folder("/lib/f7");
    let first = looked(&c, |c| assert_eq!(c.query(&f, &Sort::default()).len(), PER_FOLDER as usize));
    assert!(first <= TOTAL + PER_FOLDER, "the first question builds the folder index (one pass over the library) and reads the folder: {first}");
    for again in 0..3 {
        let n = looked(&c, |c| assert_eq!(c.query(&f, &Sort::default()).len(), PER_FOLDER as usize));
        assert!(n <= PER_FOLDER, "asked again ({again}), only the folder's {PER_FOLDER} photos are looked at, not the library's {TOTAL}: {n}");
    }
}

#[test]
fn another_folder_is_chosen_without_another_pass_over_the_library() {
    let c = library();
    looked(&c, |c| drop(c.query(&folder("/lib/f0"), &Sort::default())));
    for d in 1..FOLDERS {
        let f = folder(&format!("/lib/f{d}"));
        let n = looked(&c, |c| {
            assert!(c.any(&f));
            assert_eq!(c.count(&f), PER_FOLDER as usize);
            assert_eq!(c.query(&f, &Sort::default()).len(), PER_FOLDER as usize);
            assert_eq!(c.folder_samples(&f).len(), 1);
        });
        assert!(n <= 4 * PER_FOLDER, "folder {d}: {n} photos looked at for four questions about {PER_FOLDER}");
    }
}

#[test]
fn asking_whether_a_folder_has_a_photo_stops_at_the_first() {
    let c = library();
    looked(&c, |c| {
        c.any(&folder("/lib"));
    });
    let n = looked(&c, |c| assert!(c.any(&folder("/lib"))));
    assert!(n <= 1, "every photo of the library is in /lib: {n}");
    let n = looked(&c, |c| assert!(!c.any(&folder("/elsewhere"))));
    assert_eq!(n, 0, "no folder of that name: no photo to look at");
}

#[test]
fn a_folder_and_the_folders_inside_it_are_every_photo_filed_below() {
    let c = library();
    assert_eq!(c.query(&folder("/lib"), &Sort::default()).len() as u64, TOTAL);
    assert_eq!(c.count(&folder("/lib/f1")) as u64, PER_FOLDER);
    // names that only start alike are other folders
    assert_eq!(c.count(&folder("/lib/f")), 0);
    assert_eq!(c.count(&folder("/lib/f1/")) as u64, PER_FOLDER);
}

#[test]
fn a_moved_photo_leaves_its_folder_and_undo_brings_it_back() {
    let mut c = Catalog::new();
    let (a, b) = (add(&mut c, "/lib/a/1.jpg"), add(&mut c, "/lib/a/2.jpg"));
    agree(&c, "built");
    let (inv_relink, inv_file) = (
        c.apply(Op::Relink { id: a, file_name: "1.jpg".into(), source: file("/lib/b/1.jpg"), format: None }).unwrap(),
        c.apply(Op::SetFile { id: b, file_name: "z.jpg".into(), source: file("/lib/a/sub/z.jpg") }).unwrap(),
    );
    agree(&c, "moved");
    assert_eq!(shown(&c, &folder("/lib/b")), vec![a]);
    assert_eq!(shown(&c, &folder("/lib/a")), vec![b], "below /lib/a still, in a subfolder");
    assert!(!c.any(&folder("/lib/a/other")));
    c.apply(inv_file).unwrap();
    c.apply(inv_relink).unwrap();
    agree(&c, "undone");
    assert!(shown(&c, &folder("/lib/b")).is_empty(), "the folder is empty again");
    assert!(!c.any(&folder("/lib/b")));
}

#[test]
fn a_removed_photo_is_gone_from_its_folder_and_comes_back_with_undo() {
    let mut c = Catalog::new();
    let (a, b) = (add(&mut c, "/lib/a/1.jpg"), add(&mut c, "/lib/b/2.jpg"));
    agree(&c, "built");
    let undo = c.apply(Op::RemovePhoto { id: a }).unwrap();
    agree(&c, "removed");
    assert!(!c.any(&folder("/lib/a")), "its only photo is gone");
    assert_eq!(shown(&c, &folder("/lib")), vec![b]);
    c.apply(undo).unwrap();
    agree(&c, "back");
    assert_eq!(shown(&c, &folder("/lib/a")), vec![a]);
}

#[test]
fn a_batch_that_fails_leaves_the_folders_as_they_were() {
    let mut c = Catalog::new();
    let a = add(&mut c, "/lib/a/1.jpg");
    agree(&c, "built");
    let batch = Op::Batch {
        ops: vec![Op::Relink { id: a, file_name: "1.jpg".into(), source: file("/lib/b/1.jpg"), format: None }, Op::RemovePhoto { id: PhotoId(99) }],
    };
    assert!(c.apply(batch).is_err());
    agree(&c, "rolled back");
    assert_eq!(shown(&c, &folder("/lib/a")), vec![a]);
    assert!(!c.any(&folder("/lib/b")));
}

#[test]
fn browsed_deleted_and_demo_photos_are_filed_but_shown_only_where_a_filter_says() {
    let mut c = Catalog::new();
    agree(&c, "empty");
    let browsed = c.alloc_photo_id();
    c.apply(Op::AddPhoto { photo: Box::new(photo(browsed, file("/lib/a/b.jpg"), true)) }).unwrap();
    let kept = add(&mut c, "/lib/a/k.jpg");
    let demo = c.alloc_photo_id();
    c.apply(Op::AddPhoto { photo: Box::new(photo(demo, Source::Demo { scene: 1 }, false)) }).unwrap();
    agree(&c, "added");
    assert_eq!(shown(&c, &folder("/lib/a")), vec![kept], "a browsed photo is not the library's");
    c.apply(Op::SetLocal { id: browsed, local: false }).unwrap();
    agree(&c, "added to the library");
    assert_eq!(shown(&c, &folder("/lib/a")), vec![browsed, kept]);
    c.apply(Op::SetDeleted { id: kept, deleted: true }).unwrap();
    agree(&c, "deleted");
    assert_eq!(shown(&c, &folder("/lib/a")), vec![browsed]);
    let in_recently_deleted = Filter { deleted: true, ..folder("/lib/a") };
    assert_eq!(shown(&c, &in_recently_deleted), vec![kept]);
    c.apply(Op::SetLocal { id: browsed, local: true }).unwrap();
    agree(&c, "browsed again");
    assert!(!c.any(&folder("/lib/a")));
}

#[test]
fn however_a_folder_is_spelled_it_is_one_folder() {
    let mut c = Catalog::new();
    let ids: Vec<PhotoId> =
        ["/pics/trip/a.jpg", "/pics//trip/./b.jpg", "/pics\\trip\\c.jpg", "/pics/x/../trip/d.jpg"].iter().map(|p| add(&mut c, p)).collect();
    // company, so that the folder asked for holds less than half of the library and the index is what is read
    for i in 0..12 {
        add(&mut c, &format!("/other/{}/p{i}.jpg", i % 3));
    }
    // asked before the others are added: the index exists when they come
    assert_eq!(shown(&c, &folder("/pics/trip")), ids);
    let late = add(&mut c, "/pics/./trip//e.jpg");
    for root in ["/pics/trip", "/pics//trip/", "/pics\\trip", "/pics/y/../trip", "/pics"] {
        assert_eq!(shown(&c, &folder(root)), [ids.clone(), vec![late]].concat(), "{root:?}");
    }
    assert!(shown(&c, &folder("/pics/tri")).is_empty());
}

#[test]
fn a_folder_spelled_in_another_letter_case_is_what_a_scan_says_it_is() {
    let mut c = Catalog::new();
    let (upper, lower, below) = (add(&mut c, "/Photos/Trip/a.jpg"), add(&mut c, "/photos/trip/b.jpg"), add(&mut c, "/PHOTOS/Trip/day/c.jpg"));
    // company, so that the folder asked for holds less than half of the library and the index is what is read
    for i in 0..8 {
        add(&mut c, &format!("/other/{}/p{i}.jpg", i % 2));
    }
    // the index files all three under one folder; which of them a spelling shows is the filter's to say, as it always was:
    // another folder where file names are case-sensitive (Linux, macOS, Samba), the same one on Windows
    let all = [upper, lower, below];
    for (root, posix) in [("/Photos/Trip", vec![upper]), ("/photos/trip", vec![lower]), ("/PHOTOS/Trip", vec![below]), ("/PHOTOS", vec![below])] {
        let expected = if cfg!(windows) { all.to_vec() } else { posix };
        assert_eq!(shown(&c, &folder(root)), expected, "{root:?}");
        assert_eq!(c.count(&folder(root)), expected.len(), "{root:?}");
        assert_eq!(c.any(&folder(root)), !expected.is_empty(), "{root:?}");
    }
    agree(&c, "letter case");
}

#[test]
fn a_photo_is_found_from_its_folder_and_every_folder_above_however_its_path_is_spelled() {
    let deep = format!("/{}a.jpg", "d/".repeat(300));
    let paths = [
        "/a/b/c.jpg",
        "/a/b/d/e.jpg",
        "/a/bc/x.jpg",
        "/a/./b/x.jpg",
        "/a/../a/b/x.jpg",
        "/a//b/x.jpg",
        "/a\\b\\x.jpg",
        "/a/b ",
        "/a/b /x.jpg",
        "/a/b/x.jpg ",
        "/.hidden/x.jpg",
        "/a/.b/x.jpg",
        "/a/b/.x",
        "/x.jpg",
        "/",
        "//",
        "//srv/share/x.jpg",
        "//srv/share",
        "C:\\a\\b\\x.jpg",
        "C:/a/x.jpg",
        "relative/a/b/x.jpg",
        "x.jpg",
        "",
        "/Volumes/tokyo/a/x.jpg",
        "/日本/写真/x.jpg",
        "/a/b/",
        "/a/b//",
        "/a/b/c.jpg/",
        "/A/B/c.jpg",
        "/Ä/ö/ß.jpg",
        &deep,
    ];
    let mut c = Catalog::new();
    let ids: Vec<PhotoId> = paths.iter().map(|p| add(&mut c, p)).collect();
    for (id, path) in ids.iter().zip(paths) {
        let Some(dir) = c.photo(*id).and_then(|p| crate::local::folder_of(p)) else { continue };
        // the folder its photo is in, and each one above it
        let mut roots = vec![dir.clone()];
        let mut up = dir.as_str();
        while let Some((above, _)) = up.trim_end_matches('/').rsplit_once('/') {
            roots.push(if above.is_empty() { "/".to_string() } else { above.to_string() });
            up = above;
        }
        let roots: Vec<&str> = roots.iter().map(String::as_str).collect();
        assert!(shown(&c, &folder(&dir)).contains(id), "{path:?} is in {dir:?}");
        agree_on(&c, &roots, path);
    }
}

#[test]
fn a_photo_with_no_folder_is_in_none() {
    let mut c = Catalog::new();
    add(&mut c, "x.jpg");
    add(&mut c, "");
    agree(&c, "no folders");
    for root in ROOTS {
        assert!(!c.any(&folder(root)), "{root:?}");
    }
    assert!(c.folder_samples(&Filter::default()).is_empty());
}

#[test]
fn a_held_photo_from_another_folder_is_still_listed_once() {
    let mut c = Catalog::new();
    let (a, b, other) = (add(&mut c, "/lib/a/1.jpg"), add(&mut c, "/lib/a/2.jpg"), add(&mut c, "/lib/b/3.jpg"));
    // (a folder holding more than half of the library is walked with it: these are a minority, so the folders are what is looked at)
    for i in 0..8 {
        add(&mut c, &format!("/lib/z/{i}.jpg"));
    }
    let held = |asks: &Filter, keep: &HashSet<PhotoId>, within: &Filter| {
        let mut ids = c.query_keeping(asks, &Sort::default(), keep, within);
        ids.sort_unstable();
        ids
    };
    // the view asks for folder /lib/a with a rating nothing has; it holds what it showed
    let asks = Filter { rating: 3, ..folder("/lib/a") };
    let keep: HashSet<PhotoId> = [a, other].into_iter().collect();
    // held photos stay where `within` still looks: the library, not the folder
    let in_library = Filter::default();
    assert_eq!(held(&asks, &keep, &in_library), vec![a, other], "the held photo of /lib/b stays");
    // where it looks is the folder: the photo of /lib/b is out of it
    assert_eq!(held(&asks, &keep, &folder("/lib/a")), vec![a]);
    // a held photo that is also in the folder is listed once
    assert_eq!(held(&folder("/lib/a"), &keep, &in_library), vec![a, b, other]);
    assert!(held(&asks, &HashSet::new(), &in_library).is_empty());
}

#[test]
fn one_photo_of_each_folder_says_which_disks_a_folder_reaches() {
    let mut c = Catalog::new();
    let (nas1, nas2, nas_sub) = (add(&mut c, "/Volumes/nas/p/1.jpg"), add(&mut c, "/Volumes/nas/p/2.jpg"), add(&mut c, "/Volumes/nas/p/sub/3.jpg"));
    let usb = add(&mut c, "/Volumes/usb/4.jpg");
    let deleted = add(&mut c, "/Volumes/gone/5.jpg");
    c.apply(Op::SetDeleted { id: deleted, deleted: true }).unwrap();
    // a folder's samples are photos the filter matches, the lowest of each folder they are in
    let samples = c.folder_samples(&folder("/Volumes"));
    assert_eq!(samples, vec![nas1, nas_sub, usb], "{nas2:?} is in a folder already sampled; the deleted photo is not shown");
    assert_eq!(c.folder_samples(&folder("/Volumes/nas")), vec![nas1, nas_sub]);
    assert_eq!(c.folder_samples(&folder("/Volumes/nas/p/sub")), vec![nas_sub]);
    assert!(c.folder_samples(&folder("/Volumes/gone")).is_empty());
    assert_eq!(c.folder_samples(&Filter { deleted: true, ..folder("/Volumes") }), vec![deleted]);
    // the disks those say are the disks every photo says
    let disks = |ids: &[PhotoId]| -> BTreeSet<String> { ids.iter().filter_map(|id| crate::folders::volume_of(c.photo(*id)?)).collect() };
    let all = c.query(&folder("/Volumes"), &Sort::default());
    assert_eq!(disks(&samples), disks(&all));
}

#[test]
fn folders_that_differ_only_in_letter_case_share_a_sample_that_is_one_the_filter_matches() {
    let mut c = Catalog::new();
    let (upper, lower) = (add(&mut c, "/Volumes/NAS/p/1.jpg"), add(&mut c, "/Volumes/nas/p/2.jpg"));
    // one folder to the index, so one sample: the lowest photo
    assert_eq!(c.folder_samples(&folder("/Volumes")), vec![upper]);
    // where the sample is asked for one spelling, it is a photo of that spelling (on Windows, both are)
    let want = if cfg!(windows) { upper } else { lower };
    assert_eq!(c.folder_samples(&folder("/Volumes/nas")), vec![want]);
    assert_eq!(c.folder_samples(&folder("/Volumes/NAS")), vec![upper]);
    agree(&c, "letter case");
}

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn an_index_built_on_several_threads_is_the_one_a_single_walk_builds() {
    // folders that alternate with the ids, so each runs across the borders between the threads' runs, and the odd spellings
    let mut c = Catalog::new();
    for i in 0..400 {
        add(&mut c, &format!("/lib/f{}/p{i}.jpg", i % 7));
    }
    for p in PATHS {
        add(&mut c, p);
    }
    add(&mut c, "no-folder.jpg");
    let keys: [(&str, fn(&str) -> Option<String>); 2] = [("folders", photo_folder_index_key), ("names", file_name_index_key)];
    for (what, key) in keys {
        let one = crate::index::build(&c.photos, key, usize::MAX, 1);
        assert!(one.len() > 7, "{what}: the index has what to compare");
        for threads in [2, 3, 8, 64] {
            // (`build` would walk by itself if the threads were not used: ask for them)
            assert_eq!(crate::index::build_on_threads(&c.photos, key, 1, threads).as_ref(), Some(&one), "{what}, {threads} threads");
            assert_eq!(crate::index::build(&c.photos, key, 1, threads), one, "{what}, {threads} threads");
        }
        // fewer photos than the bar are walked by one thread, however many may be used
        assert_eq!(crate::index::build(&c.photos, key, usize::MAX, 8), one, "{what}");
        assert_eq!(crate::index::build_on_threads(&c.photos, key, usize::MAX, 8), None, "{what}");
        assert_eq!(crate::index::build_on_threads(&c.photos, key, 1, 1), None, "{what}");
        // and what a walk files is what the keys say
        assert_eq!(one.values().map(HashSet::len).sum::<usize>(), c.photos().filter(|p| file_path_of(p).and_then(key).is_some()).count(), "{what}");
        for (k, ids) in &one {
            for id in ids {
                assert_eq!(c.photo(*id).and_then(|p| file_path_of(p)).and_then(key).as_deref(), Some(k.as_str()), "{what}: {id:?}");
            }
        }
    }
    // the index a catalog builds for itself is that index, and answers as the single walk does
    for d in 0..7 {
        let want: Vec<PhotoId> = (0..400).filter(|i| i % 7 == d).map(|i| PhotoId(i as u64 + 1)).collect();
        assert_eq!(c.photos_in_folder_any_case(&folder_index_key(&format!("/lib/f{d}"))), want);
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn file_path_of(p: &Photo) -> Option<&str> {
    match &p.source {
        Source::File { path } => Some(path),
        Source::Demo { .. } => None,
    }
}

#[test]
fn a_folder_holding_most_of_the_library_is_listed_as_any_other_is() {
    let mut c = Catalog::new();
    let big: Vec<PhotoId> = (0..90).map(|i| add(&mut c, &format!("/lib/big/{}/p{i}.jpg", i % 3))).collect();
    let small: Vec<PhotoId> = (0..10).map(|i| add(&mut c, &format!("/lib/small/p{i}.jpg"))).collect();
    let ids = |f: &str| c.query(&folder(f), &Sort::default()).into_iter().collect::<BTreeSet<_>>();
    assert_eq!(ids("/lib/big"), big.iter().copied().collect::<BTreeSet<_>>());
    assert_eq!(ids("/lib/small"), small.iter().copied().collect::<BTreeSet<_>>());
    // one that holds most of the library is walked with it, a small one reads only its own
    let counted = |f: &str| {
        let before = c.photos_looked_at();
        assert_eq!(c.count(&folder(f)), if f == "/lib/big" { big.len() } else { small.len() });
        c.photos_looked_at() - before
    };
    assert_eq!(counted("/lib/big"), c.len() as u64);
    assert_eq!(counted("/lib/small"), small.len() as u64);
}

#[test]
fn the_index_is_never_saved_and_does_not_take_part_in_equality() {
    let mut c = library();
    let cold = c.clone();
    assert_eq!(c.query(&folder("/lib/f3"), &Sort::default()).len() as u64, PER_FOLDER);
    assert!(c == cold, "a catalog that has been asked and one that has not are the same catalog");
    assert_eq!(c.to_snapshot(), cold.to_snapshot());
    let read = Catalog::from_snapshot(&c.to_snapshot()).unwrap();
    let n = looked(&read, |r| assert_eq!(r.count(&folder("/lib/f3")) as u64, PER_FOLDER));
    assert!((TOTAL..=TOTAL + PER_FOLDER).contains(&n), "a catalog read from a snapshot builds its index when first asked (one pass): {n}");
    c.apply(Op::RemovePhoto { id: PhotoId(1) }).unwrap();
    assert!(c != cold);
}

/// A photo of the universe (ids 1..=10), at one of [`PATHS`] or a demo scene.
fn source_of(m: usize) -> Source {
    match PATHS.get(m % (PATHS.len() + 1)) {
        Some(p) => file(p),
        None => Source::Demo { scene: 1 },
    }
}

fn step(kind: u8, n: u64, m: usize, flag: bool) -> Op {
    let id = PhotoId(n % 10 + 1);
    let relink = |id| Op::Relink { id, file_name: "r.jpg".into(), source: source_of(m), format: None };
    match kind % 9 {
        0 | 1 => Op::AddPhoto { photo: Box::new(photo(id, source_of(m), flag)) },
        2 => Op::RemovePhoto { id },
        3 => relink(id),
        4 => Op::SetFile { id, file_name: "s.jpg".into(), source: source_of(m) },
        5 => Op::SetLocal { id, local: flag },
        6 => Op::SetDeleted { id, deleted: flag },
        // a batch that goes through
        7 => Op::Batch { ops: vec![relink(id), Op::SetLocal { id, local: flag }, Op::SetDeleted { id: PhotoId(n % 7 + 1), deleted: flag }] },
        // a batch that fails on its last op: what it did is rolled back, the index with it
        _ => Op::Batch { ops: vec![relink(id), Op::SetDeleted { id, deleted: flag }, Op::RemovePhoto { id: PhotoId(99) }] },
    }
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]

    #[test]
    fn a_folder_asked_for_shows_what_a_scan_shows(steps in proptest::collection::vec((0u8..9, 0u64..20, 0..=PATHS.len(), any::<bool>()), 1..40), kept in any::<bool>()) {
        let mut c = Catalog::new();
        let start = c.to_snapshot();
        // built before the ops, so every op has to keep it up to date
        agree(&c, "empty");
        // with the orders for the sorts asked kept too, a folder is put in order by them (and they follow every op)
        if kept {
            for sort in [Sort::default(), Sort { key: SortKey::FileName, ascending: true, ..Default::default() }] {
                c.kept_ids(&sort);
            }
        }
        let mut log = String::new();
        let mut inverses = Vec::new();
        for (i, (kind, n, m, flag)) in steps.into_iter().enumerate() {
            let op = step(kind, n, m, flag);
            if let Ok(inv) = c.apply(op.clone()) {
                log.push_str(&Catalog::op_to_log_line(&op));
                inverses.push(inv);
            }
            agree(&c, &format!("step {i}: {op:?}"));
        }
        // a snapshot, a replayed journal and a clone build their own
        agree(&Catalog::from_snapshot(&c.to_snapshot()).unwrap(), "from a snapshot");
        let mut replayed = Catalog::from_snapshot(&start).unwrap();
        replayed.replay(&log).unwrap();
        agree(&replayed, "replayed");
        agree(&c.clone(), "cloned");
        prop_assert!(c.clone() == c, "the index doesn't take part in equality");
        held_views_agree(&c);
        // undone step by step
        for (i, inv) in inverses.into_iter().rev().enumerate() {
            c.apply(inv).unwrap();
            agree(&c, &format!("undo {i}"));
        }
        prop_assert!(c.photos().next().is_none(), "all undone");
    }
}

/// A view that holds photos (`keep`) lists them as a scan would, whichever folder they are in.
fn held_views_agree(c: &Catalog) {
    let ids: Vec<PhotoId> = (1..=11).map(PhotoId).collect();
    for sort in [Sort::default(), Sort { key: SortKey::FileName, ascending: true, ..Default::default() }] {
        for bits in [0u16, 0b1, 0b1010101010, 0b11111111111] {
            let keep: HashSet<PhotoId> = ids.iter().enumerate().filter(|(i, _)| bits & (1 << i) != 0).map(|(_, id)| *id).collect();
            for root in ROOTS {
                for rating in [0, 3] {
                    let asks = Filter { rating, ..folder(root) };
                    for within in [Filter::default(), folder("/lib/a"), folder(root), Filter { deleted: true, ..Default::default() }] {
                        let mut want: Vec<&Photo> =
                            c.photos().map(|p| p.as_ref()).filter(|p| asks.matches(p, c) || (keep.contains(&p.id) && within.matches(p, c))).collect();
                        want.sort_by(|a, b| sort.order(a, b));
                        let want: Vec<PhotoId> = want.into_iter().map(|p| p.id).collect();
                        assert_eq!(
                            c.query_keeping(&asks, &sort, &keep, &within),
                            want,
                            "held {keep:?}, asking {root:?} (rating {rating}), within {within:?}"
                        );
                    }
                }
            }
        }
    }
}

// ---- several folders at once (a multi-selection in Folders) and the folders scan: what this build adds to main ----

/// The photos of any of `roots`: the first is the folder shown, the others are shown beside it.
fn several(roots: &[&str]) -> Filter {
    Filter {
        library_folder: roots.first().map(|r| r.to_string()),
        library_folders: roots.iter().skip(1).map(|r| r.to_string()).collect(),
        ..Default::default()
    }
}

#[test]
fn several_folders_show_the_photos_of_any_of_them_each_once() {
    let mut c = Catalog::new();
    for p in PATHS {
        add(&mut c, p);
    }
    // company, so that the folders asked for hold less than half of the library and the index is what is read
    for i in 0..80 {
        add(&mut c, &format!("/elsewhere/{}/p{i}.jpg", i % 5));
    }
    let sets: &[&[&str]] = &[
        &["/lib/a", "/lib/b"],
        // one inside the other: its photos once
        &["/lib", "/lib/a"],
        &["/lib/a", "/lib//a/"],
        &["/lib/a/sub", "/Volumes/nas", "C:\\"],
        &["/Lib", "/lib/A", "/LIB/a/"],
        // a blank one is no folder; one that names none shows nothing
        &["/lib/a", "   ", "/nowhere"],
        &["/nowhere", "/also/nowhere"],
        &[".", "/lib/b"],
        &["/lib/ab", "/lib/a"],
        &["//srv/share", "\\\\srv\\share\\a"],
    ];
    for roots in sets {
        let f = several(roots);
        for sort in [Sort::default(), Sort { key: SortKey::FileName, ascending: true, ..Default::default() }] {
            assert_eq!(c.query(&f, &sort), scanned(&c, &f, &sort), "{roots:?}");
        }
        let want = scanned(&c, &f, &Sort::default());
        assert_eq!(c.count(&f), want.len(), "{roots:?}");
        assert_eq!(c.any(&f), !want.is_empty(), "{roots:?}");
        let samples = c.folder_samples(&f);
        assert!(samples.iter().all(|id| want.contains(id)), "{roots:?}: every sample is a photo the folders show");
        assert_eq!(samples.is_empty(), want.is_empty(), "{roots:?}");
    }
    // two folders of a few photos: those photos are what is looked at, not the library's
    let f = several(&["/lib/a", "/lib/b"]);
    let n = looked(&c, |c| assert_eq!(c.count(&f), scanned(c, &f, &Sort::default()).len()));
    assert!(n < c.len() as u64 / 2, "looked at {n} of {} photos", c.len());
}

/// What [`Catalog::folders_scan`] said before the folder index: one pass over every photo.
fn folders_scanned_by_a_pass(c: &Catalog, folders: &[String]) -> Vec<crate::query::FolderScan> {
    let roots: Vec<String> = folders.iter().map(|d| crate::query::folder_key(d)).collect();
    let mut out = vec![crate::query::FolderScan::default(); roots.len()];
    for p in c.photos().filter(|p| !p.deleted) {
        for (scan, root) in out.iter_mut().zip(&roots) {
            if crate::query::photo_in_root(p, root) {
                scan.holds = true;
                if crate::folders::volume_of(p).is_some_and(|v| v != "/" && !crate::query::key_within(root, &v)) {
                    scan.other_disks = true;
                }
            }
        }
    }
    out
}

#[test]
fn the_folders_scan_says_what_a_pass_over_every_photo_says() {
    let mut c = Catalog::new();
    for p in PATHS {
        add(&mut c, p);
    }
    // photos that are not the library's, on another disk
    let browsed = c.alloc_photo_id();
    c.apply(Op::AddPhoto { photo: Box::new(photo(browsed, file("/Volumes/usb/b.jpg"), true)) }).unwrap();
    let deleted = add(&mut c, "/Volumes/gone/d.jpg");
    c.apply(Op::SetDeleted { id: deleted, deleted: true }).unwrap();
    add(&mut c, "/mnt/disk/x/e.jpg");
    add(&mut c, "/run/media/me/stick/f.jpg");
    let mut lists: Vec<Vec<String>> = ROOTS.iter().map(|r| vec![r.to_string()]).collect();
    lists.push(ROOTS.iter().map(|r| r.to_string()).collect());
    lists.push(
        ["/Volumes", "/Volumes/usb", "/Volumes/gone", "/mnt", "/mnt/disk", "/run/media", "/run/media/me/stick", "", "  ", ".", "/"]
            .map(String::from)
            .to_vec(),
    );
    lists.push(Vec::new());
    for folders in lists {
        let by_a_pass = folders_scanned_by_a_pass(&c, &folders);
        assert_eq!(c.folders_scan(&folders), by_a_pass, "{folders:?}");
        assert_eq!(c.folders_holding(&folders), by_a_pass.iter().map(|s| s.holds).collect::<Vec<_>>(), "{folders:?}");
    }
}

#[test]
fn no_folder_asked_for_is_every_folder() {
    let mut c = Catalog::new();
    let (a1, _a2, b) = (add(&mut c, "/x/a/1.jpg"), add(&mut c, "/x/a/2.jpg"), add(&mut c, "/x/b/3.jpg"));
    add(&mut c, "/X/A/4.jpg");
    // no roots: the sets of every folder (the lower-cased ones are one), so one sample of each
    assert_eq!(c.folder_samples(&Filter::default()), vec![a1, b]);
    assert!(c.any(&Filter::default()));
    assert_eq!(c.count(&Filter::default()), 4);
}
