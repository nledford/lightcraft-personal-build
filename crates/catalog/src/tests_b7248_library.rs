//! The library of the previous build opens in this one. That build (rebuild r4, "7248") stamped its catalog format 11
//! for the photos in segments, which is format 15 here, where 11 is another format of main's: a library is read by
//! what its files hold (`defaults`, `segments`), not by its number. `fixtures/b7248-*` is the demo library as that
//! build wrote it: `catalog.snap` listing one segment of 24 photos, and the log of four changes it had when it was
//! not closed.

use crate::journal::{LOG, SNAPSHOT, VERSION};
use crate::*;

const SEGMENT: &str = "segments/1-0.json";

/// The library as that build left it when it was closed: a snapshot and its segment, nothing in the log.
fn closed() -> MemStore {
    let m = MemStore::new();
    m.set(SNAPSHOT, include_bytes!("fixtures/b7248-catalog.snap.json").to_vec());
    m.set(SEGMENT, include_bytes!("fixtures/b7248-segments-1-0.json").to_vec());
    m
}

/// … and as it was left when it was not: the four changes made after the snapshot (on photo 21: a rating of 4, the
/// Pick flag, an exposure of 0.5 and a color temperature of 6100 K) are in the log.
fn not_closed() -> MemStore {
    let m = closed();
    m.set(LOG, include_bytes!("fixtures/b7248-catalog.log.jsonl").to_vec());
    m
}

/// Given the library of the previous build as it was closed, when this build opens it, then it holds the 24 photos of
/// its segment as they are written there, is left in this build's format, and opens again as it is.
#[test]
fn the_previous_builds_library_opens_and_is_upgraded() {
    let m = closed();
    let segment: serde_json::Value = serde_json::from_slice(&m.get(SEGMENT).unwrap()).unwrap();
    let (_, c, r) = Journal::open(Box::new(m.clone())).unwrap();
    assert_eq!((r.failed, r.replayed, r.upgraded_from), (0, 0, Some(11)));
    assert_eq!(c.len(), 24);
    for (id, written) in segment["photos"].as_object().unwrap() {
        let p = c.photo(PhotoId(id.parse().unwrap())).unwrap_or_else(|| panic!("photo {id} is in the library"));
        assert_eq!((p.id.0.to_string(), p.file_name.as_str()), (id.clone(), written["file_name"].as_str().unwrap()));
    }
    let head: serde_json::Value = serde_json::from_slice(&m.get(SNAPSHOT).unwrap()).unwrap();
    assert_eq!(head["version"], u64::from(VERSION));
    let (_, again, r) = Journal::open(Box::new(m.clone())).unwrap();
    assert_eq!((r.failed, r.upgraded_from), (0, None));
    assert_eq!(again.to_snapshot(), c.to_snapshot());
}

/// Given the same library as it was left when it was not closed, when this build opens it, then the four changes in
/// its log are replayed on top of the snapshot.
#[test]
fn the_previous_builds_log_is_replayed() {
    let (_, c, r) = Journal::open(Box::new(not_closed())).unwrap();
    assert_eq!((r.failed, r.replayed, r.upgraded_from), (0, 4, Some(11)));
    let p = c.photo(PhotoId(21)).unwrap();
    assert_eq!((p.rating, p.flag), (4, Flag::Pick));
    let develop = serde_json::to_value(&*p.develop).unwrap();
    assert_eq!(develop["light"]["exposure"], 0.5);
    assert_eq!((develop["wb"]["temp"].as_f64(), develop["wb"]["scale"].as_str()), (Some(6100.0), Some("kelvin")));
}
