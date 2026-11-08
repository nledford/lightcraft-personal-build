//! A folder of the library, shown while the library's order for the sort is kept ([`crate::order`]), is put in
//! order by it instead of being sorted, and lists the photos in the order sorting them gives.
//!
//! Scenarios, in the words of someone who switches between the folders of a big library:
//!
//! * Given the order for the sort is kept, choosing a folder (or folders shown together, one with another inside
//!   it) lists its photos from that order, in every sort both ways: no photo is sorted from scratch, and the list
//!   is the one a scan of the library, sorted, gives.
//! * Given no order is kept (a library just opened), a shuffle, or a folder of only a few photos, the folder's
//!   photos are sorted alone, and no order is made for them.
//! * Given photos edited, moved, added or removed since the order was made, it is followed first, whatever was
//!   asked before: the folder lists what a scan says. An order whose changes are no longer remembered is left as
//!   it is: the folder is sorted alone, and a big view works it out again without sorting every photo.
//! * Given the order was made on another thread ([`Catalog::prebuild_order`]) and has finished, a folder takes it.
//! * The photos a view holds from other folders are put in place with the folder's.

use std::collections::HashSet;

use proptest::prelude::*;

use crate::*;

const FOLDERS: u64 = 24;
const PER_FOLDER: u64 = 25;
const TOTAL: u64 = FOLDERS * PER_FOLDER;

const KEYS: [SortKey; 6] = [SortKey::CaptureDate, SortKey::ImportDate, SortKey::EditDate, SortKey::FileName, SortKey::Rating, SortKey::FileSize];

fn sorts() -> impl Iterator<Item = Sort> {
    KEYS.into_iter().flat_map(|key| [true, false].map(|ascending| Sort { key, ascending, ..Sort::default() }))
}

/// Photo `n` (0..[`TOTAL`]) of the library: [`FOLDERS`] folders of [`PER_FOLDER`], in folder order, every seventh
/// photo in a folder of its own inside. Many ties on every key, so the tie-breaks matter: a few names (some in
/// another letter case), few dates.
fn photo(n: u64) -> Photo {
    let d = n / PER_FOLDER;
    let name = format!("{}{}.jpg", ["IMG_", "img_", "DSC_"][(n % 3) as usize], (n * 7) % 11);
    let dir = if n % 7 == 3 { format!("/lib/f{d}/sub") } else { format!("/lib/f{d}") };
    let mut p = Photo::new(
        PhotoId(n + 1),
        Source::File { path: format!("{dir}/{name}") },
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

/// A photo of the library's, in a folder of its own, named for it.
fn extra(n: u64, dir: &str) -> Photo {
    let name = format!("{n}.jpg");
    Photo::new(PhotoId(n + 1), Source::File { path: format!("/lib/{dir}/{name}") }, &name, "JPEG", 60, 40, "2026-01-01T00:00:00")
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

/// What is asked for: a folder (some of them hold another), and folders shown together (one inside another).
fn asked() -> Vec<(&'static str, Filter)> {
    let together = |folders: &[&str]| Filter {
        library_folder: folders.first().map(|f| f.to_string()),
        library_folders: folders.iter().skip(1).map(|f| f.to_string()).collect(),
        ..Default::default()
    };
    vec![
        ("first folder", folder(0)),
        ("a folder", folder(7)),
        ("last folder", folder(FOLDERS - 1)),
        ("three folders, one inside another", together(&["/lib/f3", "/lib/f9", "/lib/f17/sub"])),
        ("a folder and the one inside it", together(&["/lib/f5", "/lib/f5/sub"])),
    ]
}

/// What a scan of every photo says (no index, no kept order): the photos matching `f`, and those of `keep` that
/// match `within`, sorted.
fn scanned(c: &Catalog, f: &Filter, sort: &Sort, keep: &HashSet<PhotoId>, within: &Filter) -> Vec<PhotoId> {
    let mut v: Vec<&Photo> = c.photos().map(|p| p.as_ref()).filter(|p| f.matches(p, c) || (keep.contains(&p.id) && within.matches(p, c))).collect();
    v.sort_by(|a, b| sort.order(a, b));
    v.into_iter().map(|p| p.id).collect()
}

/// Every sort, and what is `asked`: each is listed as a scan lists it, from the kept order (`picked`) or not.
fn folders_agree(c: &Catalog, picked: bool, when: &str) {
    let none = HashSet::new();
    for sort in sorts() {
        for (what, f) in asked() {
            let before = c.kept_picks();
            assert_eq!(c.query(&f, &sort), scanned(c, &f, &sort, &none, &f), "{when}: {sort:?}, {what}");
            assert_eq!(c.kept_picks() - before, usize::from(picked), "{when}: {sort:?}, {what}: put in order by the kept order or not");
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
        c.apply(Op::AddPhoto { photo: Box::new(extra(n, "tiny")) }).unwrap();
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

/// The fewest photos a folder can hold to be put in order by the kept order: a 64th of the library.
#[test]
fn a_folder_of_a_64th_of_the_library_is_put_in_order_by_the_kept_order_and_one_photo_less_is_not() {
    let mut c = library();
    let mut next = TOTAL;
    for (dir, count) in [("ten", 10), ("nine", 9), ("filler", 21)] {
        for _ in 0..count {
            c.apply(Op::AddPhoto { photo: Box::new(extra(next, dir)) }).unwrap();
            next += 1;
        }
    }
    assert_eq!(c.len(), 64 * 10, "a 64th of the library is ten photos");
    keep_orders(&c);
    let none = HashSet::new();
    for (dir, picked) in [("ten", 1), ("nine", 0)] {
        let f = Filter { library_folder: Some(format!("/lib/{dir}")), ..Default::default() };
        for sort in sorts() {
            let before = c.kept_picks();
            assert_eq!(c.query(&f, &sort), scanned(&c, &f, &sort, &none, &f), "{dir}, {sort:?}");
            assert_eq!(c.kept_picks() - before, picked, "{dir}, {sort:?}");
        }
    }
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

/// Edits and moves of photos in every folder, between the order being made and the folder being chosen: up to a few
/// (`IN_PLACE`) the photos are moved in the kept list one by one, and more are put back together.
#[test]
fn a_folder_is_put_in_order_through_the_edits_made_since_the_order_was() {
    for edits in [1, 5, 16, 17, 60] {
        let mut c = library();
        keep_orders(&c);
        for i in 0..edits {
            let id = PhotoId((i * 37) % TOTAL + 1);
            c.apply(Op::SetRating { id, rating: ((i + 1) % 6) as u8 }).unwrap();
            let (file_name, path) = (format!("moved{i}.jpg"), format!("/lib/f{}/moved{i}.jpg", i % FOLDERS));
            c.apply(Op::SetFile { id, file_name, source: Source::File { path } }).unwrap();
        }
        folders_agree(&c, true, &format!("{edits} photos edited"));
    }
}

#[test]
fn photos_added_since_the_order_was_made_are_put_in_order_too() {
    let mut c = library();
    keep_orders(&c);
    for n in TOTAL..TOTAL + 5 {
        // (into folders that exist)
        let name = format!("new{n}.jpg");
        let p = Photo::new(PhotoId(n + 1), Source::File { path: format!("/lib/f{}/{name}", n % 8) }, &name, "JPEG", 60, 40, "2026-03-01T00:00:00");
        c.apply(Op::AddPhoto { photo: Box::new(p) }).unwrap();
    }
    folders_agree(&c, true, "photos added");
}

#[test]
fn photos_removed_or_deleted_since_the_order_was_made_are_left_out() {
    let mut c = library();
    keep_orders(&c);
    c.apply(Op::RemovePhoto { id: PhotoId(8) }).unwrap();
    c.apply(Op::SetDeleted { id: PhotoId(7 * PER_FOLDER + 3), deleted: true }).unwrap();
    c.apply(Op::SetDeleted { id: PhotoId(2), deleted: true }).unwrap();
    folders_agree(&c, true, "photos removed and deleted");
}

#[test]
fn an_order_whose_changes_are_forgotten_is_left_for_a_big_view_to_work_out_again() {
    let mut c = library();
    let sort = Sort { key: SortKey::Rating, ascending: false, ..Sort::default() };
    c.kept_ids(&sort);
    let made = c.full_sorts();
    // more edits than are remembered
    for i in 0..9000u64 {
        c.apply(Op::SetRating { id: PhotoId(i % TOTAL + 1), rating: (i % 6) as u8 }).unwrap();
    }
    let (f, none) = (folder(7), HashSet::new());
    assert_eq!(c.query(&f, &sort), scanned(&c, &f, &sort, &none, &f));
    assert_eq!(c.kept_picks(), 0, "it cannot be followed without a pass over every photo: the folder is sorted alone");
    assert_eq!((c.index.orders.count(), c.full_sorts()), (1, made), "and the order is neither dropped nor made again");
    // all the photos, listed in that sort: the order is worked out again from where it was, not sorted from scratch
    assert_eq!(c.query(&Filter::default(), &sort).len() as u64, TOTAL);
    assert_eq!((c.index.orders.count(), c.full_sorts()), (1, made));
    assert_eq!(c.query(&f, &sort), scanned(&c, &f, &sort, &none, &f));
    assert_eq!(c.kept_picks(), 1, "and a folder is put in order by it again");
}

/// A library big enough for its first sort to be made on another thread, in folders of a 40th of it.
fn big() -> Catalog {
    let mut c = Catalog::new();
    let ops = (0..50_000u64)
        .map(|n| {
            let name = format!("{}.jpg", (n * 7919) % 977);
            let p =
                Photo::new(PhotoId(n + 1), Source::File { path: format!("/lib/f{}/{name}", n % 40) }, &name, "JPEG", 60, 40, "2026-01-01T00:00:00");
            Op::AddPhoto { photo: Box::new(p) }
        })
        .collect();
    c.apply(Op::Batch { ops }).unwrap();
    c
}

#[test]
fn an_order_made_on_another_thread_is_taken_for_a_folder_too() {
    let c = big();
    let sort = Sort { key: SortKey::FileName, ascending: true, ..Sort::default() };
    c.prebuild_order(&sort);
    assert!(c.index.orders.pending(sort.key), "not started");
    c.index.orders.wait_built(sort.key);
    let (f, made) = (folder(7), c.full_sorts());
    assert_eq!(c.query(&f, &sort), scanned(&c, &f, &sort, &HashSet::new(), &f));
    assert_eq!(c.kept_picks(), 1, "put in order by the order that was made on the other thread");
    assert_eq!((c.full_sorts(), c.index.orders.count(), c.index.orders.pending(sort.key)), (made, 1, false));
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

/// An op on the library (photos `1..=`[`TOTAL`]; a few more are asked for that are not there) that moves a photo in
/// some sort, or from a folder to another, or is taken back.
fn mutation(kind: u8, n: u64, v: u8) -> Op {
    let id = PhotoId(n % (TOTAL + 20) + 1);
    let file = |name: &str| Source::File { path: format!("/lib/f{}/{name}", u64::from(v) % (FOLDERS + 1)) };
    match kind % 10 {
        0 => Op::AddPhoto { photo: Box::new(photo(TOTAL + n % 40)) },
        1 => Op::RemovePhoto { id },
        2 => Op::SetRating { id, rating: v % 6 },
        3 => Op::SetCaptured { id, captured: (!v.is_multiple_of(3)).then(|| format!("2025-0{}-0{}T10:00:00", 1 + v % 3, 1 + v % 2)) },
        4 => Op::SetFile { id, file_name: format!("m{}.jpg", v % 5), source: file(&format!("m{}.jpg", v % 5)) },
        5 => Op::Relink { id, file_name: format!("r{}.jpg", v % 3), source: file("r.jpg"), format: None },
        6 => Op::SetContent {
            id,
            width: 1,
            height: 1,
            file_size: u64::from(v % 3),
            content_hash: None,
            preview_only: None,
            preview_checked: String::new(),
            truncated: false,
        },
        7 => Op::SetDevelop {
            id,
            settings: Default::default(),
            label: String::new(),
            edited: v.is_multiple_of(2).then(|| format!("2026-02-0{}T00:00:00", 1 + v % 3)),
        },
        8 => Op::SetDeleted { id, deleted: v.is_multiple_of(2) },
        // what is applied is taken back: the last op fails (no such photo)
        _ => Op::Batch {
            ops: vec![
                Op::SetRating { id, rating: v % 6 },
                Op::SetFile { id: PhotoId(n % TOTAL + 1), file_name: "q.jpg".into(), source: file("q.jpg") },
                Op::RemovePhoto { id: PhotoId(TOTAL + 100) },
            ],
        },
    }
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(128))]

    /// Photos edited, moved, added and removed, and never anything but folders asked for: each folder's own query
    /// follows the order, the first one to see what changed, and lists what a scan lists. No photo is sorted from
    /// scratch for it.
    #[test]
    fn a_folder_follows_a_kept_order_through_any_ops(steps in proptest::collection::vec((0u8..10, 0u64..640, 0u8..255, 0u8..3), 1..30)) {
        let mut c = library();
        keep_orders(&c);
        let made = c.full_sorts();
        let none = HashSet::new();
        for (kind, n, v, asks) in steps {
            let _ = c.apply(mutation(kind, n, v));
            // (asked after some ops and not others, so several are followed at once)
            if asks != 0 {
                let sort = sorts().nth((n % 12) as usize).unwrap_or_default();
                for (what, f) in asked() {
                    prop_assert_eq!(c.query(&f, &sort), scanned(&c, &f, &sort, &none, &f), "{} in {:?}", what, sort);
                }
            }
        }
        prop_assert_eq!(c.full_sorts(), made, "a folder was listed by sorting every photo");
    }
}
