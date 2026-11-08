//! The library census ([`crate::convert::census`]): what it reads, what it compares, and that it never writes in the library.
//! The data a user keeps most, the color labels and keywords of photos and folders, is what the comparison reports first.
#![cfg(not(target_arch = "wasm32"))]

use std::collections::BTreeSet;
use std::fs;
use std::path::Path;

use crate::convert::census::{self, Manifest};
use crate::convert::{ConvertError, Target};
use crate::tests_convert::{LAYOUTS, Scratch, personal_library, put_store, run_lossy, saved, tree};
use crate::tests_stored::plain;
use crate::{Op, PhotoId, Source};

/// The key of the photo `plain(id)` makes.
fn key(id: u64) -> String {
    format!("/pictures/2019/trip/IMG_{id:04}.jpg")
}

fn read(library: &Path) -> Manifest {
    census::read(library).unwrap_or_else(|e| panic!("census of {}: {e}", library.display()))
}

fn kinds(d: &census::Diff) -> Vec<&'static str> {
    d.must_keep.iter().map(|c| c.kind).collect()
}

/// A library with the changes of `later_ops` in its log (a label, a rating, keywords, a folder's label and a label's name).
fn library_with_later_changes(dir: &Path) {
    personal_library(dir, false, 4, true);
}

// ---- what a census holds

#[test]
fn a_census_holds_labels_and_keywords_of_photos_and_folders() {
    let scratch = Scratch::new("holds");
    let library = scratch.join("library");
    personal_library(&library, false, 4, false);
    let m = read(&library);

    let eight = &m.photos[&key(8)];
    assert_eq!((eight.rating, eight.flag.as_str(), eight.label.as_deref()), (2, "reject", Some("red")));
    // (full hierarchy paths, sorted)
    assert_eq!(eight.keywords, ["Places|Lisbon", "boats", "people|Ana"]);
    assert_eq!(eight.albums, [1]);
    let stacked = m.photos[&key(9)].stack.as_ref().map(|s| (s.id, s.position));
    assert_eq!(stacked, Some((1, 1)));
    assert!(m.photos[&key(7)].edited, "the photo with settings");
    assert!(!m.photos[&key(8)].edited);
    assert_eq!(m.photos[&key(16)].keywords, ["a|b|c"]);

    assert_eq!(m.folders.len(), 2, "two folders have a label, the made-only record has none");
    assert_eq!(m.folders["/pictures/2019/trip"].label, "green");
    assert_eq!(m.folders["/pictures/2020"].label, "red");

    let boats = m.albums.values().find(|a| a.name == "Boats").expect("the smart album");
    assert_eq!(boats.smart.as_ref().and_then(|s| s["keyword"].as_str()), Some("boats"));
    assert_eq!(m.label_names["red"], "Approved");
    assert!(m.keyword_list.contains_key("Places|Lisbon"));

    let t = &m.totals;
    assert_eq!((t.photos, t.labeled_photos, t.labeled_folders, t.folder_records), (10, 4, 2, 3));
    assert_eq!((t.albums, t.smart_albums, t.stacks, t.listed_keywords), (3, 1, 1, 1));
    assert!(m.library.stamp > 0 && m.library.layout.contains("segment"), "{:?}", m.library);
}

/// The census is of what a build reads, so how the library was stored makes no difference to it.
#[test]
fn the_census_does_not_depend_on_how_the_library_was_stored() {
    let scratch = Scratch::new("layouts");
    let mut all = Vec::new();
    for (name, one_file, per, tail) in LAYOUTS {
        let library = scratch.join(name);
        personal_library(&library, one_file, per, tail);
        all.push((name, read(&library)));
    }
    // (the layouts without a log tail say the same, and so do the two with one)
    let said = |i: usize| all.get(i).map(|(_, m)| (m.photos.clone(), m.folders.clone(), m.albums.clone())).unwrap();
    assert_eq!(said(0), said(2), "segments or one file, the log empty");
    assert_eq!(said(1), said(3), "segments or one file, with a log tail");
    assert_ne!(said(0), said(1), "the log's changes are in the census");
    let tail = &all[1].1;
    assert_eq!(tail.photos[&key(8)].label.as_deref(), Some("yellow"));
    assert_eq!(tail.folders["/pictures/2019/trip"].label, "purple");
}

#[test]
fn the_census_is_the_same_bytes_every_time() {
    let scratch = Scratch::new("bytes");
    let library = scratch.join("library");
    personal_library(&library, true, 4096, false);
    let (a, b) = (scratch.join("a.json"), scratch.join("b.json"));
    census::write(&library, &a).unwrap();
    census::write(&library, &b).unwrap();
    let (a_bytes, b_bytes) = (fs::read(&a).unwrap(), fs::read(&b).unwrap());
    assert_eq!(a_bytes, b_bytes);
    let text = String::from_utf8(a_bytes).unwrap();
    let at = |name: &str| text.find(&format!("\n  \"{name}\"")).unwrap_or_else(|| panic!("no {name}"));
    assert!(at("census") < at("roots") && at("totals") < at("photos") && at("photos") < at("folders") && at("folders") < at("albums"));
    // (the photos' keys are in order)
    let keys: Vec<String> = read(&library).photos.keys().cloned().collect();
    assert!(keys.windows(2).all(|w| w[0] < w[1]));
    assert_eq!(census::load(&a).unwrap(), read(&library));
}

/// A photo of the library's own folder is named relative to it, and a copy that still points at the original's folder is
/// compared by the same names.
#[test]
fn files_in_the_library_are_named_relative_to_it_and_a_copy_compares_by_the_same_names() {
    let scratch = Scratch::new("relative");
    let library = scratch.join("library");
    let held = library.join("Originals").join("a.jpg");
    let mut copy_of = plain(2);
    copy_of.source = Source::File { path: held.to_string_lossy().into_owned() };
    copy_of.copy_of = Some(PhotoId(1));
    copy_of.copy_name = Some("Copy 1".into());
    let mut original = plain(1);
    original.source = copy_of.source.clone();
    original.label = Some(crate::ColorLabel::Blue);
    let outside = plain(3);
    let ops = [original, copy_of, outside].map(|p| Op::AddPhoto { photo: Box::new(p) });
    put_store(&library, &saved(&ops, &[], true, 4096));

    let m = read(&library);
    // (a virtual copy shares its original's file, so each has the id after the path)
    assert_eq!(m.photos.keys().cloned().collect::<Vec<_>>(), [key(3).as_str(), "Originals/a.jpg#1", "Originals/a.jpg#2"]);

    let copy = scratch.join("copy");
    run_lossy(&library, &copy, Target::Official);
    let there = read(&copy);
    assert!(
        there.photos.keys().any(|k| k.ends_with("Originals/a.jpg#1") && k.starts_with('/')),
        "the copy's records still name the original's folder"
    );
    let d = census::diff(&m, &there);
    assert!(!d.differs(), "{}", d.report(false));
    assert!(d.other.is_empty(), "{}", d.report(false));
}

// ---- comparing a library with its copy

#[test]
fn a_library_and_its_official_copy_have_no_must_keep_differences() {
    for (name, one_file, per, tail) in LAYOUTS {
        let scratch = Scratch::new("official");
        let library = scratch.join("library");
        personal_library(&library, one_file, per, tail);
        let copy = scratch.join("copy");
        run_lossy(&library, &copy, Target::Official);
        let (a, b) = (read(&library), read(&copy));
        let d = census::diff(&a, &b);
        assert!(!d.differs(), "{name}:\n{}", d.report(false));
        assert!(d.other.is_empty(), "{name}:\n{}", d.report(false));
        assert!(a.totals.labeled_photos >= 3 && a.totals.keyworded_photos >= 2 && a.totals.labeled_folders == 2, "{name}: not an empty proof");
        // (what a conversion changes on purpose is context, not a difference: the format, and the record that only said when a
        // folder was made, which official LightCraft has no place for)
        assert!(d.context.iter().any(|l| l.starts_with("format ")), "{name}: {:?}", d.context);
        assert!(d.context.contains(&"totals.folder_records 3 -> 2".to_string()), "{name}: {:?}", d.context);
        assert_eq!(census::diff(&a, &a), census::Diff::default(), "{name}: a census of itself");
    }
}

#[test]
fn a_library_and_its_7248_copy_keep_every_label_and_keyword_and_the_edits_that_build_lacks_are_counted() {
    for (name, one_file, per, tail) in LAYOUTS {
        let scratch = Scratch::new("7248");
        let library = scratch.join("library");
        personal_library(&library, one_file, per, tail);
        let copy = scratch.join("copy");
        run_lossy(&library, &copy, Target::Build7248);
        let d = census::diff(&read(&library), &read(&copy));
        assert!(!d.differs(), "{name}:\n{}", d.report(false));
        // (the setting this build added and build 7248 has no place for)
        assert!(d.other.get("photo.develop").is_some_and(|n| *n >= 1), "{name}:\n{}", d.report(false));
    }
}

// ---- a deliberate change is reported

#[test]
fn a_changed_label_and_changed_keywords_on_photos_and_a_changed_folder_label_are_reported() {
    let scratch = Scratch::new("changed");
    let (before, after) = (scratch.join("before"), scratch.join("after"));
    personal_library(&before, false, 4, false);
    library_with_later_changes(&after);
    let (a, b) = (read(&before), read(&after));
    let d = census::diff(&a, &b);

    assert!(d.differs());
    let found: BTreeSet<(&str, &str, &str)> = d.must_keep.iter().map(|c| (c.kind, c.subject.as_str(), c.detail.as_str())).collect();
    let (eight, sixteen) = (key(8), key(16));
    assert_eq!(
        found,
        BTreeSet::from([
            ("photo.label", eight.as_str(), "red -> yellow"),
            ("photo.keywords", sixteen.as_str(), "+late|keyword"),
            ("folder.label", "/pictures/2019/trip", "green -> purple"),
        ])
    );
    // (the other fields: counted, not listed)
    assert_eq!(d.other.iter().map(|(k, n)| (*k, *n)).collect::<Vec<_>>(), [("catalog.label_names", 1), ("photo.rating", 1)]);

    // (and the other way round)
    let back = census::diff(&b, &a);
    assert!(back.must_keep.iter().any(|c| c.kind == "photo.label" && c.detail == "yellow -> red"));
    assert!(back.must_keep.iter().any(|c| c.kind == "photo.keywords" && c.detail == "-late|keyword"));
}

#[test]
fn a_label_or_keyword_taken_away_and_photos_and_folders_that_appear_or_disappear_are_reported() {
    let scratch = Scratch::new("gone");
    let library = scratch.join("library");
    personal_library(&library, false, 4, false);
    let a = read(&library);

    let mut b = a.clone();
    b.photos.get_mut(&key(8)).unwrap().label = None;
    b.photos.get_mut(&key(8)).unwrap().keywords.clear();
    b.photos.remove(&key(9));
    let extra = b.photos[&key(16)].clone();
    b.photos.insert(key(99), extra);
    b.folders.remove("/pictures/2020");
    b.folders.insert("/pictures/new".into(), census::FolderEntry { label: "blue".into() });
    b.folders.get_mut("/pictures/2019/trip").unwrap().label = "red".into();
    let d = census::diff(&a, &b);

    let mut seen = kinds(&d);
    seen.sort_unstable();
    assert_eq!(
        seen,
        ["folder.appeared", "folder.disappeared", "folder.label", "photo.appeared", "photo.disappeared", "photo.keywords", "photo.label"]
    );
    let detail = |kind: &str| d.must_keep.iter().find(|c| c.kind == kind).map(|c| (c.subject.clone(), c.detail.clone())).unwrap();
    assert_eq!(detail("photo.label"), (key(8), "red -> none".to_string()));
    assert_eq!(detail("photo.keywords"), (key(8), "-Places|Lisbon, -boats, -people|Ana".to_string()));
    assert_eq!(detail("photo.disappeared").0, key(9));
    assert_eq!(detail("photo.appeared").0, key(99));
    assert_eq!(detail("folder.label"), ("/pictures/2019/trip".to_string(), "green -> red".to_string()));
    assert_eq!(detail("folder.disappeared").0, "/pictures/2020");
    assert_eq!(detail("folder.appeared").0, "/pictures/new");
}

#[test]
fn the_other_fields_are_counted_and_do_not_make_the_comparison_fail() {
    let scratch = Scratch::new("other");
    let library = scratch.join("library");
    personal_library(&library, false, 4, false);
    let a = read(&library);
    let mut b = a.clone();
    {
        let p = b.photos.get_mut(&key(8)).unwrap();
        p.rating = 4;
        p.flag = "pick".into();
        p.albums.clear();
        p.edited = true;
        p.develop = "0000000000000000".into();
    }
    b.photos.get_mut(&key(9)).unwrap().stack = None;
    b.albums.remove("3");
    b.albums.get_mut("1").unwrap().photos.reverse();
    b.albums.get_mut("1").unwrap().name = "Renamed".into();
    let d = census::diff(&a, &b);
    assert!(!d.differs(), "{}", d.report(false));
    let counts: Vec<(&str, u64)> = d.other.iter().map(|(k, n)| (*k, *n)).collect();
    assert_eq!(
        counts,
        [
            ("album.disappeared", 1),
            ("album.photos", 1),
            ("album.properties", 1),
            ("photo.albums", 1),
            ("photo.develop", 1),
            ("photo.edited", 1),
            ("photo.flag", 1),
            ("photo.rating", 1),
            ("photo.stack", 1),
        ]
    );
}

// ---- the report

#[test]
fn counts_only_names_no_photo_folder_label_or_keyword() {
    let scratch = Scratch::new("counts");
    let (before, after) = (scratch.join("before"), scratch.join("after"));
    personal_library(&before, false, 4, false);
    library_with_later_changes(&after);
    let d = census::diff(&read(&before), &read(&after));

    let full = d.report(false);
    for named in ["IMG_0008.jpg", "IMG_0016.jpg", "/pictures/2019/trip", "red -> yellow", "late|keyword"] {
        assert!(full.contains(named), "{named} in:\n{full}");
    }
    assert!(full.starts_with("MUST KEEP"), "the must-keep section comes first:\n{full}");
    let counts = d.report(true);
    assert!(counts.contains("photo.label: 1") && counts.contains("photo.keywords: 1") && counts.contains("folder.label: 1"), "{counts}");
    assert!(counts.contains("photo.rating: 1") && counts.contains("result: DIFFERENT"), "{counts}");
    for secret in ["IMG_", "pictures", "yellow", "green", "purple", "late|", "Lisbon", "boats"] {
        assert!(!counts.contains(secret), "`{secret}` in the counts-only report:\n{counts}");
    }
}

#[test]
fn the_same_library_reports_no_difference() {
    let scratch = Scratch::new("same");
    let library = scratch.join("library");
    personal_library(&library, false, 4, true);
    let m = read(&library);
    let d = census::diff(&m, &m);
    assert!(!d.differs());
    let report = d.report(true);
    assert!(report.contains("no differences") && report.ends_with("result: same\n"), "{report}");
}

// ---- it only reads

#[test]
fn the_library_is_never_written_and_no_lock_is_taken() {
    for (name, one_file, per, tail) in LAYOUTS {
        let scratch = Scratch::new("readonly");
        let library = scratch.join("library");
        personal_library(&library, one_file, per, tail);
        let before = tree(&library);
        census::write(&library, &scratch.join("census.json")).unwrap_or_else(|e| panic!("{name}: {e}"));
        read(&library);
        assert_eq!(tree(&library), before, "{name}: files, bytes and dates");
        assert!(!before.contains_key("catalog.lock") && !library.join("catalog.lock").exists(), "{name}: no lock file made");
    }
}

#[test]
fn a_library_held_open_can_still_be_read_for_a_census() {
    let scratch = Scratch::new("held");
    let library = scratch.join("library");
    personal_library(&library, false, 4, false);
    let held = crate::lock::LibraryLock::acquire(&library, "a program that has the library open").expect("the program that has the library open");
    let before = tree(&library);
    let out = scratch.join("census.json");
    let made = census::write(&library, &out).expect("a census needs no lock");
    assert_eq!(made.totals.photos, 10);
    assert!(held.held());
    assert_eq!(tree(&library), before, "the lock files and the library were not touched");
    assert!(out.exists());
}

#[test]
fn a_census_is_never_written_inside_the_library_or_over_a_file() {
    let scratch = Scratch::new("refuse");
    let library = scratch.join("library");
    personal_library(&library, false, 4, false);
    let before = tree(&library);
    for inside in [library.join("census.json"), library.join("thumbs").join("c.json"), library.join("segments").join("c.json")] {
        match census::write(&library, &inside) {
            Err(ConvertError::Refused(why)) => assert!(why.contains("is in the library"), "{why}"),
            other => panic!("expected a refusal for {}, got {other:?}", inside.display()),
        }
    }
    assert_eq!(tree(&library), before, "nothing was made in the library, not even a folder");

    let taken = scratch.join("taken.json");
    fs::write(&taken, b"mine").unwrap();
    assert!(matches!(census::write(&library, &taken), Err(ConvertError::Refused(why)) if why.contains("exists")));
    assert_eq!(fs::read(&taken).unwrap(), b"mine", "a file that is there is not written over");

    // a folder of the same name as the census file is in the way too
    let folder = scratch.join("folder.json");
    fs::create_dir(&folder).unwrap();
    assert!(matches!(census::write(&library, &folder), Err(ConvertError::Refused(_))));
}

#[cfg(unix)]
#[test]
fn a_link_to_the_library_is_still_the_library() {
    let scratch = Scratch::new("link");
    let library = scratch.join("library");
    personal_library(&library, false, 4, false);
    std::os::unix::fs::symlink(&library, scratch.join("alias")).unwrap();
    let before = tree(&library);
    let result = census::write(&library, &scratch.join("alias").join("c.json"));
    assert!(matches!(result, Err(ConvertError::Refused(_))), "{result:?}");
    assert_eq!(tree(&library), before);
    assert!(!library.join("c.json").exists());
}

#[test]
fn nothing_is_left_when_the_census_fails() {
    let scratch = Scratch::new("fails");
    // a folder that is no library
    let empty = scratch.join("empty");
    fs::create_dir_all(&empty).unwrap();
    let out = scratch.join("census.json");
    assert!(matches!(census::write(&empty, &out), Err(ConvertError::Unreadable(_))));
    assert!(!out.exists(), "no file for a library that can't be read");

    // a folder for the file that does not exist
    let library = scratch.join("library");
    personal_library(&library, false, 4, false);
    let nowhere = scratch.join("no-such-folder").join("census.json");
    assert!(matches!(census::write(&library, &nowhere), Err(ConvertError::Io(_))));
    assert!(!scratch.join("no-such-folder").exists());
    // a library that isn't there
    assert!(census::write(&scratch.join("missing"), &out).is_err());
    assert!(!out.exists());
}

#[test]
fn a_census_file_that_is_not_one_is_refused() {
    let scratch = Scratch::new("load");
    let write = |name: &str, text: &str| {
        let path = scratch.join(name);
        fs::write(&path, text).unwrap();
        path
    };
    let why = |path: &Path| match census::load(path) {
        Err(ConvertError::Unreadable(why)) => why,
        other => panic!("expected an unreadable census, got {other:?}"),
    };
    assert!(why(&write("text.json", "not json")).contains("not a census"));
    assert!(why(&write("none.json", "{}")).contains("no `census` format number"));
    assert!(why(&write("newer.json", "{\"census\": 2}")).contains("census format 2"));
    assert!(why(&write("short.json", "{\"census\": 1}")).contains("not a census"));
    assert!(matches!(census::load(&scratch.join("missing.json")), Err(ConvertError::Io(_))));
    assert!(census::diff_files(&scratch.join("missing.json"), &scratch.join("missing.json")).is_err());
}

/// The comparison of two census files is what the comparison of the manifests says.
#[test]
fn two_census_files_are_compared_by_what_they_hold() {
    let scratch = Scratch::new("files");
    let (before, after) = (scratch.join("before"), scratch.join("after"));
    personal_library(&before, false, 4, false);
    library_with_later_changes(&after);
    let (a, b) = (scratch.join("a.json"), scratch.join("b.json"));
    census::write(&before, &a).unwrap();
    census::write(&after, &b).unwrap();
    let d = census::diff_files(&a, &b).unwrap();
    assert_eq!(d, census::diff(&read(&before), &read(&after)));
    assert_eq!(d.must_keep.len(), 3);
    // (the same library twice: the same)
    let again = scratch.join("again.json");
    census::write(&before, &again).unwrap();
    assert!(!census::diff_files(&a, &again).unwrap().differs());
}
