//! A library opens on a folder, and chooses another folder, by looking through its photos once at
//! most: the first question about a folder files the library's photos by folder, and from then on
//! a folder is read where it is filed (see `Catalog::photos_looked_at`). What the view then shows
//! is what it showed.
//!
//! Scenarios, in the words of someone with a big library:
//!
//! * Given a library last shown on a folder, opening it lists that folder's photos after one pass
//!   over the library (not one to find the folder is still there and another to list it).
//! * Given a library on All Photos, choosing a folder passes over the library once, and each
//!   folder chosen after it reads only its own photos.
//! * The folder shows its photos and the folders inside it, nothing else; the saved selection is
//!   kept, a first photo is picked when none was active, stacks are arranged as ever.
//! * A folder that is gone, or that holds nothing the library shows, opens on All Photos; one that
//!   reaches other disks, or none of the library's photos, is refused.

use std::path::{Path, PathBuf};

use lightcraft_catalog::{Op, Photo, PhotoId, Source, Stack, StackId};
use serde_json::json;

use crate::{LibrarySource, Selection, Session};

const FOLDERS: u64 = 40;
const PER_FOLDER: u64 = 50;
const TOTAL: u64 = FOLDERS * PER_FOLDER;

fn temp_dir(tag: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("lc-single-scan-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    d
}

/// The photos of folder `d`, by id.
fn folder_photos(d: u64) -> Vec<PhotoId> {
    (0..PER_FOLDER).map(|i| PhotoId(d * PER_FOLDER + i + 1)).collect()
}

/// Photo `n` (0..[`TOTAL`]) of the library: 40 folders of 50, in folder order.
fn photo(n: u64) -> Photo {
    let (d, i) = (n / PER_FOLDER, n % PER_FOLDER);
    Photo::new(PhotoId(n + 1), Source::File { path: format!("/lib/f{d}/p{i}.jpg") }, &format!("p{i}.jpg"), "JPEG", 60, 40, "2026-01-01T10:00:00")
}

fn source_folder(s: &mut Session, path: &str) -> serde_json::Value {
    s.execute("library.source", &json!({"kind": "libraryFolder", "path": path})).unwrap()
}

fn open(dir: &Path) -> Session {
    let mut s = Session::new();
    s.open_library(dir, false).unwrap();
    s
}

/// A saved library of [`FOLDERS`] folders of [`PER_FOLDER`] photos, last left showing folder
/// `shown` with its first photo selected (none: All Photos).
fn saved_library(tag: &str, shown: Option<u64>) -> PathBuf {
    let dir = temp_dir(tag);
    let mut s = open(&dir);
    s.commit("Add", Op::Batch { ops: (0..TOTAL).map(|n| Op::AddPhoto { photo: Box::new(photo(n)) }).collect() }).unwrap();
    s.persist().unwrap();
    if let Some(d) = shown {
        source_folder(&mut s, &format!("/lib/f{d}"));
        s.selection = Selection::single(PhotoId(d * PER_FOLDER + 1));
    }
    s.save_view();
    dir
}

/// What the view lists, by id.
fn listed(s: &mut Session) -> Vec<PhotoId> {
    let mut ids = s.visible().to_vec();
    ids.sort_unstable();
    ids
}

#[test]
fn opening_on_a_folder_looks_through_the_library_once() {
    let dir = saved_library("open", Some(7));
    let mut s = open(&dir);
    assert_eq!((s.source, s.library_folder.as_deref()), (LibrarySource::LibraryFolder, Some("/lib/f7")));
    assert_eq!(listed(&mut s), folder_photos(7));
    let n = s.catalog.photos_looked_at();
    assert!(
        n <= TOTAL + 3 * PER_FOLDER,
        "{n} photos looked at to open on a folder of {PER_FOLDER} in a library of {TOTAL}: one pass over the library, not two"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn choosing_another_folder_does_not_look_through_the_library_again() {
    let dir = saved_library("switch", None);
    let mut s = open(&dir);
    assert_eq!(s.visible().len() as u64, TOTAL);
    let mut looked = Vec::new();
    for d in 0..FOLDERS {
        let before = s.catalog.photos_looked_at();
        assert_eq!(source_folder(&mut s, &format!("/lib/f{d}"))["count"], PER_FOLDER, "folder {d}");
        looked.push(s.catalog.photos_looked_at() - before);
    }
    // the first choice files the library by folder (one pass over it); the others read their own
    assert!(looked[0] <= TOTAL + 6 * PER_FOLDER, "the first folder: {looked:?}");
    for (d, n) in looked.iter().enumerate().skip(1) {
        assert!(*n <= 6 * PER_FOLDER, "choosing folder {d} looked at {n} photos of a library of {TOTAL}");
    }
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_chosen_folder_lists_its_photos_and_the_folders_inside_it_and_nothing_else() {
    let dir = saved_library("lists", None);
    let mut s = open(&dir);
    for d in [0, 7, 39, 7, 12] {
        source_folder(&mut s, &format!("/lib/f{d}"));
        assert_eq!(listed(&mut s), folder_photos(d), "folder {d}");
        assert_eq!(s.source_total(), Some(PER_FOLDER as usize));
    }
    assert_eq!(source_folder(&mut s, "/lib")["count"], TOTAL, "the folder above shows the folders inside it");
    assert_eq!(source_folder(&mut s, "/lib/f1")["count"], PER_FOLDER, "/lib/f10 to /lib/f19 are other folders");
    // the filter bar narrows the folder, as it does an album
    s.execute("library.filter", &json!({"rating": 1})).unwrap();
    assert!(s.visible().is_empty());
    s.execute("library.clearFilter", &json!({})).unwrap();
    assert_eq!(s.visible().len() as u64, PER_FOLDER);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn opening_on_a_folder_keeps_the_saved_selection() {
    let dir = saved_library("selection", Some(7));
    let mut s = open(&dir);
    // two photos of the folder, one of another folder, one that is gone
    let (a, b, elsewhere, gone) = (PhotoId(7 * PER_FOLDER + 3), PhotoId(7 * PER_FOLDER + 4), PhotoId(12), PhotoId(999_999));
    s.selection = Selection { ids: vec![a, b, elsewhere, gone], active: Some(b) };
    s.save_view();
    drop(s);
    let s = open(&dir);
    assert_eq!(s.selection, Selection { ids: vec![a, b, elsewhere], active: Some(b) }, "what is in the library stays selected");
    assert_eq!(s.source, LibrarySource::LibraryFolder);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn opening_on_a_folder_picks_its_first_photo_when_none_was_active() {
    let dir = saved_library("pick", Some(7));
    let mut s = open(&dir);
    s.selection = Selection::default();
    s.save_view();
    drop(s);
    let mut s = open(&dir);
    let first = s.visible().first().copied();
    assert!(first.is_some_and(|id| folder_photos(7).contains(&id)), "{first:?}");
    assert_eq!(s.selection, Selection::single(first.unwrap()));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_saved_folder_with_nothing_left_to_show_opens_on_all_photos() {
    let dir = saved_library("emptied", None);
    let mut s = open(&dir);
    // every photo of the folder was deleted since the view was saved
    s.commit("Delete", Op::Batch { ops: folder_photos(7).into_iter().map(|id| Op::SetDeleted { id, deleted: true }).collect() }).unwrap();
    s.persist().unwrap();
    drop(s);
    std::fs::write(dir.join("view.json"), br#"{"source": {"kind": "libraryFolder"}, "library_folder": "/lib/f7"}"#).unwrap();
    let mut s = open(&dir);
    assert_eq!((s.source, s.library_folder.clone()), (LibrarySource::All, None), "a folder that shows nothing is not an empty grid");
    assert_eq!(s.visible().len() as u64, TOTAL - PER_FOLDER);
    // and the folder is as gone from what can be chosen
    assert!(s.execute("library.source", &json!({"kind": "libraryFolder", "path": "/lib/f7"})).is_err());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn deleting_the_last_photo_of_the_folder_shown_returns_to_all_photos() {
    let dir = saved_library("last", Some(7));
    let mut s = open(&dir);
    assert_eq!(s.visible().len() as u64, PER_FOLDER);
    let (all_but_one, last) = (folder_photos(7)[1..].to_vec(), folder_photos(7)[0]);
    s.commit("Delete", Op::Batch { ops: all_but_one.into_iter().map(|id| Op::SetDeleted { id, deleted: true }).collect() }).unwrap();
    assert_eq!(listed(&mut s), vec![last]);
    assert_eq!(s.source, LibrarySource::LibraryFolder, "one photo is left");
    s.commit("Delete", Op::SetDeleted { id: last, deleted: true }).unwrap();
    assert_eq!(s.visible().len() as u64, TOTAL - PER_FOLDER);
    assert_eq!((s.source, s.library_folder.clone()), (LibrarySource::All, None));
    // brought back, the folder can be chosen again
    s.execute("edit.undo", &json!({})).unwrap();
    s.execute("edit.undo", &json!({})).unwrap();
    assert_eq!(source_folder(&mut s, "/lib/f7")["count"], PER_FOLDER);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_photo_moved_to_another_folder_changes_what_both_show() {
    let dir = saved_library("moved", Some(7));
    let mut s = open(&dir);
    let moved = PhotoId(7 * PER_FOLDER + 1);
    assert_eq!(s.visible().len() as u64, PER_FOLDER);
    let op = Op::Relink { id: moved, file_name: "moved.jpg".into(), source: Source::File { path: "/lib/f8/moved.jpg".into() }, format: None };
    s.commit("Move", op).unwrap();
    assert_eq!(s.visible().len() as u64, PER_FOLDER - 1, "gone from the folder it left");
    assert_eq!(source_folder(&mut s, "/lib/f8")["count"], PER_FOLDER + 1, "and in the one it arrived in");
    s.execute("edit.undo", &json!({})).unwrap();
    assert_eq!(s.visible().len() as u64, PER_FOLDER, "undo puts it back");
    assert_eq!(source_folder(&mut s, "/lib/f7")["count"], PER_FOLDER);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_folder_view_arranges_stacks_as_ever() {
    let dir = saved_library("stacks", Some(7));
    let mut s = open(&dir);
    let stacked: Vec<PhotoId> = folder_photos(7).into_iter().take(3).collect();
    s.commit("Stack", Op::AddStack { stack: Stack { id: StackId(1), photos: stacked.clone(), collapsed: true } }).unwrap();
    assert_eq!(s.visible().len() as u64, PER_FOLDER - 2, "a collapsed stack of three is one cell");
    s.commit("Expand", Op::SetStack { id: StackId(1), photos: stacked, collapsed: false }).unwrap();
    assert_eq!(s.visible().len() as u64, PER_FOLDER);
    // another folder is not touched by it
    assert_eq!(source_folder(&mut s, "/lib/f8")["count"], PER_FOLDER);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_folder_that_reaches_other_disks_or_holds_nothing_is_refused() {
    let mut s = Session::new();
    let mut paths = vec!["/Volumes/nas/p/1.jpg", "/Volumes/nas/p/2.jpg", "/Volumes/nas/q/3.jpg", "/Volumes/usb/4.jpg", "/Users/me/5.jpg"];
    paths.extend(["/Volumes/gone/6.jpg", "/Volumes/browsed/7.jpg"]);
    let ops = paths
        .iter()
        .enumerate()
        .map(|(i, path)| {
            let mut p = Photo::new(PhotoId(i as u64 + 1), Source::File { path: (*path).into() }, "x.jpg", "JPEG", 60, 40, "2026-01-01T10:00:00");
            p.deleted = path.contains("gone");
            p.local = path.contains("browsed");
            Op::AddPhoto { photo: Box::new(p) }
        })
        .collect();
    s.commit("Add", Op::Batch { ops }).unwrap();
    for ok in ["/Volumes/nas", "/Volumes/nas/p", "/Volumes/usb", "/Users/me"] {
        assert!(s.execute("library.source", &json!({"kind": "libraryFolder", "path": ok})).is_ok(), "{ok}");
    }
    let refused = |s: &mut Session, path: &str| s.execute("library.source", &json!({"kind": "libraryFolder", "path": path})).unwrap_err().to_string();
    // above two disks: a way to reach them, not the folder of one
    let e = refused(&mut s, "/Volumes");
    assert!(e.contains("other disks"), "{e}");
    assert!(refused(&mut s, "/").contains("startup disk"));
    // no photo of the library was imported from them (the deleted and the browsed ones are not)
    for none in ["/elsewhere", "/Volumes/gone", "/Volumes/browsed", "/Volumes/nas/p/nothing"] {
        let e = refused(&mut s, none);
        assert!(e.contains("no photo"), "{none}: {e}");
    }
    assert_eq!(s.source, LibrarySource::LibraryFolder, "the last good choice stands");
    assert_eq!(s.library_folder.as_deref(), Some("/Users/me"));
}

#[test]
fn a_folder_is_not_refused_for_disks_only_photos_it_does_not_show_are_on() {
    let mut s = Session::new();
    // `/mnt` is on the startup disk; below it a deleted photo is on one disk and a browsed photo on another
    let photo_at = |id: u64, path: &str, deleted: bool, local: bool| {
        let mut p = Photo::new(PhotoId(id), Source::File { path: path.into() }, "x.jpg", "JPEG", 60, 40, "2026-01-01T10:00:00");
        (p.deleted, p.local) = (deleted, local);
        Op::AddPhoto { photo: Box::new(p) }
    };
    let ops = vec![photo_at(1, "/mnt/1.jpg", false, false), photo_at(2, "/mnt/gone/2.jpg", true, false), photo_at(3, "/mnt/usb/3.jpg", false, true)];
    s.commit("Add", Op::Batch { ops }).unwrap();
    assert_eq!(source_folder(&mut s, "/mnt")["count"], 1, "the folder shows one photo, on the startup disk");
    // brought back from Recently Deleted, the photo on the other disk is one the folder shows: it reaches that disk
    s.commit("Restore", Op::SetDeleted { id: PhotoId(2), deleted: false }).unwrap();
    let e = s.execute("library.source", &json!({"kind": "libraryFolder", "path": "/mnt"})).unwrap_err().to_string();
    assert!(e.contains("other disks"), "{e}");
}
