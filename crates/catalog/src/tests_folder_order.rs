//! A folder of the library, shown while the library's order for the sort is kept ([`crate::order`]), is put in
//! order by it instead of being sorted, and lists the photos in the order sorting them gives.
//!
//! Scenarios, in the words of someone who switches between the folders of a big library:
//!
//! * Given the order for the sort is kept, choosing a folder lists its photos from that order, in every sort
//!   both ways: no photo is sorted from scratch, and the list is the one a scan of the library, sorted, gives.
//! * Given no order is kept (a library just opened), a shuffle, or a folder of only a few photos, the folder's
//!   photos are sorted alone, and no order is made for them.
//! * Given photos edited or moved since the order was made, it is followed first: the folder lists what a scan
//!   says. An order whose changes are no longer remembered is dropped, and a big view makes it again.
//! * The photos a view holds from other folders are put in place with the folder's.

use std::collections::HashSet;

use crate::*;

const FOLDERS: u64 = 24;
const PER_FOLDER: u64 = 25;
const TOTAL: u64 = FOLDERS * PER_FOLDER;

const KEYS: [SortKey; 6] = [SortKey::CaptureDate, SortKey::ImportDate, SortKey::EditDate, SortKey::FileName, SortKey::Rating, SortKey::FileSize];

fn sorts() -> impl Iterator<Item = Sort> {
    KEYS.into_iter().flat_map(|key| [true, false].map(|ascending| Sort { key, ascending, ..Sort::default() }))
}

/// Photo `n` (0..[`TOTAL`]) of the library: [`FOLDERS`] folders of [`PER_FOLDER`], in folder order. Many ties on
/// every key, so the tie-breaks matter: a few names (some in another letter case), few dates.
fn photo(n: u64) -> Photo {
    let d = n / PER_FOLDER;
    let name = format!("{}{}.jpg", ["IMG_", "img_", "DSC_"][(n % 3) as usize], (n * 7) % 11);
    let mut p = Photo::new(
        PhotoId(n + 1),
        Source::File { path: format!("/lib/f{d}/{name}") },
        &name,
        "JPEG",
        60,
        40,
        &format!("2026-01-0{}T00:00:00", 1 + n % 3),
    );
    p.captured = (!n.is_multiple_of(5)).then(|| format!("2025-0{}-0{}T10:00:00", 1 + n % 4, 1 + n % 7));
    p.rating = (n % 6) as u8;
    p.file_size = n % 7;
    p.edited = n.is_multiple_of(4).then(|| "2026-02-01T00:00:00".to_string());
    p
}

fn library() -> Catalog {
    let mut c = Catalog::new();
    for n in 0..TOTAL {
        c.apply(Op::AddPhoto { photo: Box::new(photo(n)) }).unwrap();
    }
    c
}

/// The library's order for every sort key, made.
fn keep_orders(c: &Catalog) {
    for sort in sorts() {
        c.kept_ids(&sort);
    }
}

fn folder(d: u64) -> Filter {
    Filter { library_folder: Some(format!("/lib/f{d}")), ..Default::default() }
}

/// What a scan of every photo says (no index, no kept order): the photos matching `f`, and those of `keep` that
/// match `within`, sorted.
fn scanned(c: &Catalog, f: &Filter, sort: &Sort, keep: &HashSet<PhotoId>, within: &Filter) -> Vec<PhotoId> {
    let mut v: Vec<&Photo> = c.photos().map(|p| p.as_ref()).filter(|p| f.matches(p, c) || (keep.contains(&p.id) && within.matches(p, c))).collect();
    v.sort_by(|a, b| sort.order(a, b));
    v.into_iter().map(|p| p.id).collect()
}

/// Every sort, and some folders: the folder is listed as a scan lists it, from the kept order (`picked`) or not.
fn folders_agree(c: &Catalog, picked: bool, when: &str) {
    let none = HashSet::new();
    for sort in sorts() {
        for d in [0, 7, FOLDERS - 1] {
            let f = folder(d);
            let before = c.kept_picks();
            assert_eq!(c.query(&f, &sort), scanned(c, &f, &sort, &none, &f), "{when}: {sort:?}, folder {d}");
            assert_eq!(c.kept_picks() - before, usize::from(picked), "{when}: {sort:?}, folder {d}: put in order by the kept order or not");
        }
    }
}

#[test]
fn a_folder_is_put_in_order_by_the_kept_order_when_there_is_one() {
    let c = library();
    keep_orders(&c);
    let made = c.full_sorts();
    folders_agree(&c, true, "kept");
    assert_eq!(c.full_sorts(), made, "no photo was sorted from scratch for a folder");
}

#[test]
fn a_folder_is_sorted_alone_and_no_order_is_made_while_none_is_kept() {
    let c = library();
    folders_agree(&c, false, "none kept");
    assert_eq!((c.full_sorts(), c.index.orders.count()), (0, 0), "no order was made for a folder");
    // (a copy of a library with orders kept keeps none)
    keep_orders(&c);
    folders_agree(&c.clone(), false, "cloned");
}

#[test]
fn a_folder_of_a_few_photos_is_sorted_alone_even_when_an_order_is_kept() {
    let mut c = library();
    for n in TOTAL..TOTAL + 3 {
        let p = Photo::new(
            PhotoId(n + 1),
            Source::File { path: format!("/lib/tiny/{n}.jpg") },
            &format!("{n}.jpg"),
            "JPEG",
            60,
            40,
            "2026-01-01T00:00:00",
        );
        c.apply(Op::AddPhoto { photo: Box::new(p) }).unwrap();
    }
    keep_orders(&c);
    let tiny = Filter { library_folder: Some("/lib/tiny".into()), ..Default::default() };
    let none = HashSet::new();
    for sort in sorts() {
        assert_eq!(c.query(&tiny, &sort), scanned(&c, &tiny, &sort, &none, &tiny), "{sort:?}");
    }
    assert_eq!(c.kept_picks(), 0, "reading the order along would cost more than sorting three photos");
    // and a folder of a few more is not
    c.query(&folder(7), &Sort::default());
    assert_eq!(c.kept_picks(), 1);
}

#[test]
fn a_shuffle_is_never_put_in_order_by_a_kept_order() {
    let c = library();
    keep_orders(&c);
    let (f, none) = (folder(7), HashSet::new());
    for seed in [0, 7] {
        let sort = Sort { key: SortKey::Random, seed, ..Sort::default() };
        assert_eq!(c.query(&f, &sort), scanned(&c, &f, &sort, &none, &f), "seed {seed}");
    }
    assert_eq!(c.kept_picks(), 0);
}

/// Edits and moves of photos in every folder, between the order being made and the folder being chosen.
#[test]
fn a_folder_is_put_in_order_through_the_edits_made_since_the_order_was() {
    // (a few photos touched are moved in the kept list one by one, many are put back together)
    for edits in [1, 5, 16, 17, 60] {
        let mut c = library();
        keep_orders(&c);
        for i in 0..edits {
            let id = PhotoId((i * 37) % TOTAL + 1);
            c.apply(Op::SetRating { id, rating: ((i + 1) % 6) as u8 }).unwrap();
            let (file_name, path) = (format!("moved{i}.jpg"), format!("/lib/f{}/moved{i}.jpg", i % FOLDERS));
            c.apply(Op::SetFile { id, file_name, source: Source::File { path } }).unwrap();
        }
        c.apply(Op::RemovePhoto { id: PhotoId(8) }).unwrap();
        folders_agree(&c, true, &format!("{edits} photos edited"));
    }
}

#[test]
fn photos_added_since_the_order_was_made_are_put_in_order_too() {
    let mut c = library();
    keep_orders(&c);
    for n in TOTAL..TOTAL + 5 {
        // (into folders that exist, and one that is new)
        let name = format!("new{n}.jpg");
        let p = Photo::new(PhotoId(n + 1), Source::File { path: format!("/lib/f{}/{name}", n % 8) }, &name, "JPEG", 60, 40, "2026-03-01T00:00:00");
        c.apply(Op::AddPhoto { photo: Box::new(p) }).unwrap();
    }
    folders_agree(&c, true, "photos added");
}

#[test]
fn an_order_whose_changes_are_forgotten_is_dropped_and_a_big_view_makes_it_again() {
    let mut c = library();
    let sort = Sort { key: SortKey::Rating, ascending: false, ..Sort::default() };
    c.kept_ids(&sort);
    // more edits than are remembered
    for i in 0..9000u64 {
        c.apply(Op::SetRating { id: PhotoId(i % TOTAL + 1), rating: (i % 6) as u8 }).unwrap();
    }
    let (f, none) = (folder(7), HashSet::new());
    assert_eq!(c.query(&f, &sort), scanned(&c, &f, &sort, &none, &f));
    assert_eq!((c.kept_picks(), c.index.orders.count()), (0, 0), "the order is dropped, not made again for a folder");
    // all the photos, listed in that sort: the order is made
    assert_eq!(c.query(&Filter::default(), &sort).len() as u64, TOTAL);
    assert_eq!(c.index.orders.count(), 1);
    assert_eq!(c.query(&f, &sort), scanned(&c, &f, &sort, &none, &f));
    assert_eq!(c.kept_picks(), 1, "and the folder is put in order by it");
}

#[test]
fn photos_held_from_other_folders_are_put_in_place_with_the_folder() {
    let mut c = library();
    // one held photo is gone from where the view looks
    c.apply(Op::SetDeleted { id: PhotoId(301), deleted: true }).unwrap();
    keep_orders(&c);
    let (f, within) = (folder(7), Filter::default());
    // photos of other folders, one of the folder itself, and the one that was deleted
    let keep: HashSet<PhotoId> = [3, 77, 301, 599, 7 * PER_FOLDER + 2].into_iter().map(PhotoId).collect();
    for sort in sorts() {
        let before = c.kept_picks();
        assert_eq!(c.query_keeping(&f, &sort, &keep, &within), scanned(&c, &f, &sort, &keep, &within), "{sort:?}");
        assert_eq!(c.kept_picks() - before, 1, "{sort:?}");
    }
}
