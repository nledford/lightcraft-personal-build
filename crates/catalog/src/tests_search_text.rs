//! The free-text search and smart albums' Text rules read a photo's lower-cased text from a cache
//! ([`crate::index`]) rather than lower-casing every field of every photo on every keystroke. The
//! cache must give the matches the fields give, and follow every edit of a field it holds. The search's text of a big library
//! is made on another thread ([`crate::search_text`]): the search must give the same matches before it is ready, and the edits
//! made meanwhile must not be lost.

use lightcraft_meta::{Rect, Region, RegionKind};
use serde_json::json;

use super::*;
use crate::index::PhotoText;
use crate::search_text::SearchText;

fn photo(id: u64, file: &str) -> Photo {
    Photo::new(PhotoId(id), Source::Demo { scene: 1 }, file, "JPEG", 60, 40, "2026-01-01T00:00:00")
}

fn face(name: &str) -> Region {
    Region { rect: Rect { x0: 0.4, y0: 0.4, x1: 0.6, y1: 0.6 }, kind: RegionKind::Face, name: Some(name.into()), description: None }
}

/// Photos whose text is hard on a lower-casing shortcut: Unicode that changes length or depends on
/// its neighbours, a word that is only there across two fields, a colon in a plain word.
fn library() -> Catalog {
    let mut photos = Vec::new();
    let mut p = photo(1, "IMG_0001.JPG");
    p.rating = 3;
    p.meta.camera = "Canon EOS R5".into();
    p.meta.lens = "RF24-70mm F2.8 L".into();
    p.meta.title = "Trip to Ōsaka".into();
    p.meta.caption = "Straße am Fluss".into();
    p.meta.location = "Kyōto".into();
    p.meta.keywords = vec!["Travel|Italy".into(), "Ὀδυσσεύς".into(), "Kyōto Japan".into()];
    photos.push(p);
    let mut p = photo(2, "İstanbul.jpg");
    p.meta.title = "ΣΊΣΥΦΟΣ".into();
    p.meta.caption = "Re: 10:30 meeting".into();
    p.meta.city = "Zürich".into();
    p.meta.regions = vec![face("Ana Lima")];
    photos.push(p);
    let mut p = photo(3, "plain.png");
    p.format = "PNG".into();
    photos.push(p);
    let mut p = photo(4, "abc");
    p.meta.camera = "def".into();
    p.meta.title = "ǅ ǆ".into();
    photos.push(p);
    let mut p = photo(5, "name:colon.jpg");
    p.meta.lens = "title:trip".into();
    p.copy_name = Some("Black and White".into());
    photos.push(p);
    let mut c = Catalog::new();
    for p in photos {
        c.apply(Op::AddPhoto { photo: Box::new(p) }).unwrap();
    }
    c
}

/// How the search read a word before the cache: each field lower-cased alone, for every photo.
fn before_text_matches(p: &Photo, t: &str) -> bool {
    let hay = [&p.file_name, &p.meta.title, &p.meta.caption, &p.meta.camera, &p.meta.lens, &p.meta.location, &p.format];
    hay.iter().any(|h| h.to_lowercase().contains(t))
        || p.meta.keywords.iter().any(|k| k.to_lowercase().contains(t))
        || p.people().iter().any(|n| n.to_lowercase().contains(t))
}

/// [`before_text_matches`] for the words the tests use: plain ones and the fields the cache serves.
fn before_matches(p: &Photo, text: &str) -> bool {
    text.split_whitespace().all(|tok| {
        let t = tok.to_lowercase();
        match t.split_once(':') {
            Some(("camera", val)) => p.meta.camera.to_lowercase().contains(val),
            Some(("lens", val)) => p.meta.lens.to_lowercase().contains(val),
            Some(("name" | "file", val)) => p.file_name.to_lowercase().contains(val),
            Some(("rating", val)) => p.rating.to_string() == val,
            Some(("kw" | "keyword", val)) => p.meta.keywords.iter().any(|k| crate::keywords::is_under(k, val)),
            _ => before_text_matches(p, &t),
        }
    })
}

const QUERIES: &[&str] = &[
    "img",
    "IMG_0001",
    "IMG_0001.JPG",
    "canon r5",
    "CANON eos",
    "ōsaka",
    "ŌSAKA",
    "osaka",
    "straße",
    "STRASSE",
    "i̇stanbul",
    "İSTANBUL",
    "istanbul",
    "σίσυφος",
    "ΣΊΣΥΦΟΣ",
    "ς",
    "σ",
    "ὀδυσσεύς",
    "travel",
    "italy",
    "travel|italy",
    "kyōto japan",
    "kyōto",
    "10:30",
    "re:",
    "name:img",
    "NAME:IMG_0001",
    "file:plain",
    "name:",
    "camera:canon",
    "camera:r5",
    "lens:rf24",
    "lens:",
    "title:trip",
    "abcdef",
    "abc def",
    "ǆ",
    "rating:3 trip",
    "kw:travel ōsaka",
    "kw:travel osaka",
    "png jpeg",
    "",
    "   ",
    "zzz",
];

fn shown(c: &Catalog, text: &str) -> Vec<u64> {
    let f = Filter { text: text.into(), ..Default::default() };
    let mut ids: Vec<u64> = c.query(&f, &Sort::default()).into_iter().map(|id| id.0).collect();
    ids.sort_unstable();
    ids
}

fn expected(c: &Catalog, text: &str) -> Vec<u64> {
    c.photos().filter(|p| !p.deleted && before_matches(p, text)).map(|p| p.id.0).collect()
}

/// A search finds what the fields hold, whether it asks for one photo at a time, as a pass over the
/// library that makes the cache, or one photo after that.
#[test]
fn search_matches_as_the_fields_do() {
    let c = library();
    let mut one_by_one = Vec::new();
    for q in QUERIES.iter().copied() {
        let f = Filter { text: q.into(), ..Default::default() };
        one_by_one.push(c.photos().filter(|p| f.matches(p, &c)).map(|p| p.id.0).collect::<Vec<_>>());
    }
    assert!(c.photos().all(|p| c.search_text(p, false).is_none()), "testing one photo makes no cache for the library");
    for (q, got) in QUERIES.iter().zip(&one_by_one) {
        assert_eq!(*got, expected(&c, q), "{q:?}, one photo at a time");
    }
    for q in QUERIES.iter().copied() {
        assert_eq!(shown(&c, q), expected(&c, q), "{q:?}, a pass over the library");
    }
    assert!(c.photos().all(|p| c.search_text(p, false).is_some()), "a pass made the cache");
    for q in QUERIES.iter().copied() {
        let f = Filter { text: q.into(), ..Default::default() };
        let got: Vec<u64> = c.photos().filter(|p| f.matches(p, &c)).map(|p| p.id.0).collect();
        assert_eq!(got, expected(&c, q), "{q:?}, one photo at a time, with the cache");
    }
}

/// A photo that isn't the catalog's own (an edit not made yet, a candidate being tried) is read
/// from its own fields, not from what the catalog kept for the photo with its id.
#[test]
fn a_copy_of_a_photo_is_read_as_it_is() {
    let c = library();
    let f = Filter { text: "ōsaka".into(), ..Default::default() };
    assert_eq!(c.count(&f), 1);
    let mut p = (**c.photo(PhotoId(1)).unwrap()).clone();
    p.meta.title = "Trip to Nara".into();
    p.meta.keywords.clear();
    p.meta.location.clear();
    assert!(!f.matches(&p, &c), "the catalog's own text says Ōsaka, this copy doesn't");
    let nara = Filter { text: "nara".into(), ..Default::default() };
    assert!(nara.matches(&p, &c));
    assert!(!nara.matches(c.photo(PhotoId(1)).unwrap(), &c));
    let rule = Filter { rule_set: Some(rules(json!({"rules": [{"field": "text", "op": "contains", "value": "nara"}]}))), ..Default::default() };
    assert_eq!(c.count(&rule), 0);
    assert!(rule.matches(&p, &c));
}

fn rules(v: serde_json::Value) -> RuleSet {
    serde_json::from_value(v).unwrap()
}

fn text_rule(op: &str, value: &str) -> Filter {
    Filter { rule_set: Some(rules(json!({"rules": [{"field": "text", "op": op, "value": value}]}))), ..Default::default() }
}

/// What the cache holds is what a scan of the photos would give it, and every photo's is used.
fn assert_current(c: &Catalog) {
    for p in c.photos() {
        let kept = c.search_text(p, false).unwrap_or_else(|| panic!("photo {}: no text kept, or kept for another place", p.id.0));
        assert_eq!(kept, PhotoText::of(p).text(), "photo {}", p.id.0);
        assert_eq!(c.rule_text_of(p), Some(crate::rules::rule_text(p).as_str()), "photo {}: the Text rule's text", p.id.0);
    }
}

/// Make the caches, as the first search and the first smart album with a Text rule do.
fn warm(c: &Catalog) {
    c.count(&Filter { text: "x".into(), ..Default::default() });
    c.count(&text_rule("contains", "x"));
    assert_current(c);
}

fn found(c: &Catalog, text: &str) -> usize {
    c.count(&Filter { text: text.into(), ..Default::default() })
}

fn found_by_rule(c: &Catalog, text: &str) -> usize {
    c.count(&text_rule("contains", text))
}

/// An edit of each field that is searched puts the new text in the cache and takes the old one out,
/// through the op, its inverse and a batch that fails and is taken back.
#[test]
fn the_cache_follows_every_field_that_is_searched() {
    let mut c = library();
    warm(&c);
    let id = PhotoId(3);
    let edit = |c: &mut Catalog, set: &dyn Fn(&mut Meta)| {
        let mut meta = c.photo(id).unwrap().meta.clone();
        set(&mut meta);
        c.apply(Op::SetMeta { id, meta: Box::new(meta) }).unwrap()
    };
    type Set = fn(&mut Meta, &str);
    let fields: [(&str, Set); 6] = [
        ("title", |m, v| m.title = v.into()),
        ("caption", |m, v| m.caption = v.into()),
        ("camera", |m, v| m.camera = v.into()),
        ("lens", |m, v| m.lens = v.into()),
        ("location", |m, v| m.location = v.into()),
        ("keywords", |m, v| m.keywords = vec![v.into()]),
    ];
    for (n, (field, set)) in fields.into_iter().enumerate() {
        let word = format!("Zq{n}x");
        let before = found(&c, &word);
        let undo = edit(&mut c, &|m| set(m, &word));
        assert_eq!((before, found(&c, &word)), (0, 1), "{field}: found after the edit");
        assert_eq!(found_by_rule(&c, &word.to_lowercase()), 1, "{field}: the Text rule");
        assert_current(&c);
        c.apply(undo).unwrap();
        assert_eq!(found(&c, &word), 0, "{field}: gone after the undo");
        assert_current(&c);
    }
    // the fields only Text rules read
    let rule_fields: [(&str, Set); 3] =
        [("city", |m, v| m.city = v.into()), ("state", |m, v| m.state = v.into()), ("country", |m, v| m.country = v.into())];
    for (n, (field, set)) in rule_fields.into_iter().enumerate() {
        let word = format!("zr{n}y");
        let undo = edit(&mut c, &|m| set(m, &word));
        assert_eq!((found_by_rule(&c, &word), found(&c, &word)), (1, 0), "{field}");
        assert_current(&c);
        c.apply(undo).unwrap();
        assert_eq!(found_by_rule(&c, &word), 0, "{field}: gone after the undo");
    }
    let undo = edit(&mut c, &|m| m.alt_text = "zalt".into());
    assert_eq!(found_by_rule(&c, "zalt"), 1);
    c.apply(undo).unwrap();
    assert_eq!(found_by_rule(&c, "zalt"), 0);
    let undo = edit(&mut c, &|m| m.regions = vec![face("Zed Person")]);
    assert_eq!(found_by_rule(&c, "zed person"), 1, "a person named on a face");
    c.apply(undo).unwrap();
    assert_eq!(found_by_rule(&c, "zed person"), 0);
    assert_current(&c);
}

#[test]
fn the_cache_follows_a_file_name_and_a_format() {
    let mut c = library();
    warm(&c);
    let id = PhotoId(3);
    let source = Source::Demo { scene: 1 };
    let undo = c.apply(Op::SetFile { id, file_name: "Renamed_0042.PNG".into(), source: source.clone() }).unwrap();
    assert_eq!((found(&c, "renamed_0042"), found(&c, "plain"), found(&c, "name:renamed")), (1, 0, 1));
    assert_eq!((found_by_rule(&c, "renamed_0042"), found_by_rule(&c, "plain")), (1, 0));
    assert_current(&c);
    c.apply(undo).unwrap();
    assert_eq!((found(&c, "renamed_0042"), found(&c, "plain")), (0, 1));
    assert_current(&c);

    let undo = c.apply(Op::Relink { id, file_name: "Moved.TIF".into(), source: source.clone(), format: Some("TIFF".into()) }).unwrap();
    assert_eq!((found(&c, "moved"), found(&c, "tiff"), found(&c, "png")), (1, 1, 0));
    assert_eq!((found_by_rule(&c, "moved"), found_by_rule(&c, "tiff"), found_by_rule(&c, "png")), (1, 1, 0));
    assert_current(&c);
    c.apply(undo).unwrap();
    assert_eq!((found(&c, "moved"), found(&c, "tiff"), found(&c, "png")), (0, 0, 1));
    assert_current(&c);
}

/// A photo that comes or goes (an import, a virtual copy, a permanent delete and its undo) does
/// too, and a virtual copy's name is read by a Text rule.
#[test]
fn the_cache_follows_photos_that_come_and_go() {
    let mut c = library();
    warm(&c);
    let mut copy = (**c.photo(PhotoId(1)).unwrap()).clone();
    copy.id = PhotoId(9);
    copy.copy_of = Some(PhotoId(1));
    copy.meta = Default::default();
    copy.copy_name = Some("Zcopy Name".into());
    copy.file_name = "Zfresh.jpg".into();
    let undo = c.apply(Op::AddPhoto { photo: Box::new(copy) }).unwrap();
    assert_eq!((found(&c, "zfresh"), found_by_rule(&c, "zcopy"), found(&c, "zcopy")), (1, 1, 0));
    assert_current(&c);
    let redo = c.apply(undo).unwrap();
    assert_eq!((found(&c, "zfresh"), found_by_rule(&c, "zcopy")), (0, 0));
    assert_current(&c);
    c.apply(redo).unwrap();
    assert_eq!(found(&c, "zfresh"), 1);
    assert_current(&c);
    let removed = c.apply(Op::RemovePhoto { id: PhotoId(1) }).unwrap();
    assert_eq!(found(&c, "ōsaka"), 0);
    assert_current(&c);
    c.apply(removed).unwrap();
    assert_eq!(found(&c, "ōsaka"), 1);
    assert_current(&c);
}

/// A batch that fails half way is taken back, and the cache with it.
#[test]
fn the_cache_follows_a_batch_that_fails() {
    let mut c = library();
    warm(&c);
    let mut meta = c.photo(PhotoId(3)).unwrap().meta.clone();
    meta.title = "Zbatch title".into();
    let batch = Op::Batch {
        ops: vec![
            Op::SetMeta { id: PhotoId(3), meta: Box::new(meta) },
            Op::SetFile { id: PhotoId(3), file_name: "Zbatch.png".into(), source: Source::Demo { scene: 1 } },
            Op::SetFile { id: PhotoId(404), file_name: "gone.png".into(), source: Source::Demo { scene: 1 } },
        ],
    };
    assert!(c.apply(batch).is_err());
    assert_eq!((found(&c, "zbatch"), found_by_rule(&c, "zbatch")), (0, 0));
    assert_eq!(found(&c, "plain"), 1);
    assert_current(&c);
}

/// An edit of a photo that something else holds (a thumbnail list, an undo step) puts it at another
/// place in memory. The cache follows it, so a search isn't left reading every such photo from its fields.
#[test]
fn the_cache_follows_a_photo_that_others_hold() {
    let mut c = library();
    warm(&c);
    let held: Vec<_> = c.photos().cloned().collect();
    for p in &held {
        c.apply(Op::SetRating { id: p.id, rating: 5 }).unwrap();
    }
    assert!(held.iter().all(|old| !std::ptr::eq(&**old, &**c.photo(old.id).unwrap())), "the edits copied the photos");
    assert_current(&c);
    assert_eq!(found(&c, "ōsaka"), 1);
    c.apply(Op::SetFile { id: PhotoId(2), file_name: "after.jpg".into(), source: Source::Demo { scene: 1 } }).unwrap();
    assert_current(&c);
    assert_eq!((found(&c, "after"), found(&c, "istanbul")), (1, 0));
}

/// A catalog read from a snapshot or cloned starts with no cache and builds its own.
#[test]
fn a_clone_builds_its_own() {
    let mut c = library();
    warm(&c);
    let mut copy = c.clone();
    assert!(copy.photos().all(|p| copy.search_text(p, false).is_none()));
    assert_eq!(found(&copy, "ōsaka"), 1);
    warm(&copy);
    let mut meta = copy.photo(PhotoId(1)).unwrap().meta.clone();
    meta.title = "Elsewhere".into();
    copy.apply(Op::SetMeta { id: PhotoId(1), meta: Box::new(meta) }).unwrap();
    assert_eq!((found(&copy, "elsewhere"), found(&c, "elsewhere")), (1, 0));
    c.apply(Op::SetRating { id: PhotoId(1), rating: 1 }).unwrap();
    assert_current(&c);
}

/// The words of Text rules (not the search) read the same with the cache as without: what the fields give when each
/// photo is read alone (a copy of a photo is not the catalog's own, so the text kept for its id is not used), and what the
/// kept text gives.
#[test]
fn text_rules_match_as_the_fields_do() {
    let c = library();
    let every_op = ["contains", "notContains", "is", "isNot", "startsWith", "endsWith", "isEmpty", "isNotEmpty"];
    let values =
        ["", "ōsaka", "TRIP TO", "kyōto japan", "ana lima", "zürich", "img_0001.jpg", "black and white", "ς", "i̇stanbul", "png", "re: 10:30 meeting"];
    assert!(!c.rule_text_made(), "nothing made the text yet");
    let after: Vec<Vec<usize>> = every_op.iter().map(|op| values.iter().map(|v| c.count(&text_rule(op, v))).collect()).collect();
    assert!(c.rule_text_made(), "the first rule asked made the text");
    let from_the_fields: Vec<Vec<usize>> = every_op
        .iter()
        .map(|op| values.iter().map(|v| c.photos().filter(|p| text_rule(op, v).matches(&Photo::clone(p), &c)).count()).collect())
        .collect();
    assert_eq!(from_the_fields, after);
    assert!(after.iter().flatten().any(|&n| n > 0), "the rules found something");
    let again: Vec<Vec<usize>> =
        every_op.iter().map(|op| values.iter().map(|v| c.photos().filter(|p| text_rule(op, v).matches(p, &c)).count()).collect()).collect();
    assert_eq!(after, again);
}

/// A smart album's count through the tally path (`album_contains`, one photo at a time) reads the
/// kept text: it is made there, and an edit shows in the next count.
#[test]
fn a_smart_album_count_reads_the_kept_text() {
    let mut c = library();
    let mut album = Album::new(AlbumId(1), "Osaka");
    album.smart = Some(Box::new(text_rule("contains", "ōsaka")));
    c.apply(Op::AddAlbum { album }).unwrap();
    let one_by_one = |c: &Catalog| c.photos().filter(|p| c.album_contains(AlbumId(1), p)).count();
    assert!(!c.rule_text_made(), "nothing made the text yet");
    assert_eq!(one_by_one(&c), 1);
    assert!(c.rule_text_made(), "counting made the text");
    let mut meta = c.photo(PhotoId(3)).unwrap().meta.clone();
    meta.title = "Ōsaka again".into();
    c.apply(Op::SetMeta { id: PhotoId(3), meta: Box::new(meta) }).unwrap();
    assert_eq!(one_by_one(&c), 2);
}

/// The kept text holds the names of the people on a photo's faces, and a quoted phrase is read as
/// one word that never runs from one field into the next.
#[test]
fn the_kept_text_holds_the_people_and_a_phrase_stays_in_one_field() {
    let mut c = library();
    warm(&c);
    assert_eq!(found(&c, "ana"), 1, "a person's name is searched");
    assert_eq!(found(&c, "\"ana lima\""), 1, "a quoted name is one phrase");
    assert_eq!(found(&c, "\"lima ana\""), 0);
    assert_eq!(found(&c, "\"ōsaka straße\""), 0, "a phrase doesn't run from the title into the caption");
    assert_eq!(found(&c, "\"trip to ōsaka\""), 1, "…but fits inside one field");
    assert_eq!(found(&c, "\"Kyōto Japan\""), 1, "…or one keyword");
    // (the photo that has Ana Lima's face)
    let mut meta = c.photo(PhotoId(2)).unwrap().meta.clone();
    meta.regions = vec![face("Zed Person")];
    c.apply(Op::SetMeta { id: PhotoId(2), meta: Box::new(meta) }).unwrap();
    assert_eq!(found(&c, "\"zed person\""), 1, "a renamed person is found at once");
    assert_eq!(found(&c, "ana"), 0, "and the old name is gone");
    assert_current(&c);
}

/// A word that holds NUL (a filter's text can) is looked for inside one field, as before the cache: it never matches across
/// the NUL the cache puts between two fields, and still matches a field that holds NUL itself.
#[test]
fn a_word_holding_nul_never_runs_over_two_fields() {
    let mut c = Catalog::new();
    let mut p = photo(1, "a");
    p.meta.title = "b".into();
    c.apply(Op::AddPhoto { photo: Box::new(p) }).unwrap();
    let mut p = photo(2, "c\0d");
    p.meta.title = "e".into();
    c.apply(Op::AddPhoto { photo: Box::new(p) }).unwrap();
    for text in ["a\0b", "\"a\0b\"", "b\0", "\0e"] {
        assert_eq!(shown(&c, text), Vec::<u64>::new(), "{text:?}");
    }
    assert_eq!(shown(&c, "c\0d"), vec![2], "a field's own NUL is still found");
}

/// A thread held where it starts until the test lets it go, so a test decides when the text it makes may be ready.
struct Gate {
    open: std::sync::Mutex<bool>,
    changed: std::sync::Condvar,
}

impl Gate {
    fn new() -> Arc<Gate> {
        Arc::new(Gate { open: std::sync::Mutex::new(false), changed: std::sync::Condvar::new() })
    }

    fn wait(&self) {
        let mut open = self.open.lock().unwrap();
        while !*open {
            open = self.changed.wait(open).unwrap();
        }
    }

    fn open(&self) {
        *self.open.lock().unwrap() = true;
        self.changed.notify_all();
    }
}

/// The five photos, whose text is made on another thread however few they are, with the thread held at its start.
fn held() -> (Catalog, Arc<Gate>) {
    let mut c = library();
    c.set_search_background_from(0);
    let gate = Gate::new();
    let at_the_gate = gate.clone();
    c.hold_search_text(move || at_the_gate.wait());
    (c, gate)
}

/// Wait (a minute at most) until `done` says so.
fn wait_for(c: &Catalog, what: &str, done: impl Fn(&Catalog) -> bool) {
    let by = std::time::Instant::now() + std::time::Duration::from_secs(60);
    while !done(c) {
        assert!(std::time::Instant::now() < by, "{what}: not in a minute (the text is {:?})", c.search_text_state());
        std::thread::sleep(std::time::Duration::from_millis(1));
    }
}

/// Wait until the text made on the other thread is the one the searches read.
fn wait_until_ready(c: &Catalog) {
    wait_for(c, "the text was not made", Catalog::search_text_ready);
}

/// In a big library the first search starts the text on another thread and doesn't wait for it: until it is ready, each photo
/// is read from its fields, as the first search read it before the text was kept.
#[test]
fn a_search_before_the_text_is_ready_reads_the_fields() {
    let (c, gate) = held();
    for q in QUERIES.iter().copied() {
        assert_eq!(shown(&c, q), expected(&c, q), "{q:?}: before the text is ready");
    }
    assert_eq!(c.search_text_state(), "making", "the first search started the text on another thread and did not wait for it");
    assert_eq!(c.search_text_spawned(), 1, "…once");
    assert!(c.photos().all(|p| c.search_text(p, false).is_none()), "nothing is kept yet");
    // (one photo at a time, as a smart album's tally asks, reads the fields too)
    for q in QUERIES.iter().copied() {
        let f = Filter { text: q.into(), ..Default::default() };
        let got: Vec<u64> = c.photos().filter(|p| f.matches(p, &c)).map(|p| p.id.0).collect();
        assert_eq!(got, expected(&c, q), "{q:?}, one photo at a time");
    }
    gate.open();
    wait_until_ready(&c);
    assert_current(&c);
    for q in QUERIES.iter().copied() {
        assert_eq!(shown(&c, q), expected(&c, q), "{q:?}: from the text the other thread made");
    }
    assert_eq!((c.search_text_state(), c.search_text_spawned()), ("made", 1));
}

/// `prebuild_search_text` starts the text for a library big enough for making it to be felt, once, and for no other.
#[test]
fn the_text_of_a_big_library_is_started_once_and_a_small_ones_is_not_started() {
    let small = library();
    small.prebuild_search_text();
    assert_eq!((small.search_text_state(), small.search_text_spawned()), ("idle", 0), "five photos are made where a search stands");
    let (c, gate) = held();
    c.prebuild_search_text();
    c.prebuild_search_text();
    assert_eq!(c.search_text_state(), "making", "started without a search");
    assert_eq!(shown(&c, "img"), expected(&c, "img"));
    c.prebuild_search_text();
    assert_eq!(c.search_text_spawned(), 1, "one thread for the library");
    gate.open();
    wait_until_ready(&c);
    c.prebuild_search_text();
    assert_eq!((c.search_text_state(), c.search_text_spawned()), ("made", 1), "nothing to start once it is made");
}

/// A photo edited, added or removed while the other thread makes the text is searched as it is now, not as the thread saw it:
/// the text is made again for those photos when it is taken, and kept up to date after that.
#[test]
fn edits_made_while_the_text_is_being_made_are_searched_as_they_are_now() {
    let (mut c, gate) = held();
    c.prebuild_search_text();
    assert_eq!(c.search_text_state(), "making");
    let edit = |c: &mut Catalog, id: u64, set: &dyn Fn(&mut Meta)| {
        let mut meta = c.photo(PhotoId(id)).unwrap().meta.clone();
        set(&mut meta);
        c.apply(Op::SetMeta { id: PhotoId(id), meta: Box::new(meta) }).unwrap();
    };
    // a word put in, words taken away, a photo gone, a photo come, a photo that only moves in memory
    edit(&mut c, 3, &|m| m.title = "Zq0x title".into());
    edit(&mut c, 1, &|m| {
        m.title = "Trip to Nara".into();
        m.keywords.clear();
        m.location.clear();
    });
    c.apply(Op::RemovePhoto { id: PhotoId(4) }).unwrap();
    let mut fresh = photo(9, "Zfresh.jpg");
    fresh.meta.keywords = vec!["Zq1x".into()];
    c.apply(Op::AddPhoto { photo: Box::new(fresh) }).unwrap();
    c.apply(Op::SetRating { id: PhotoId(2), rating: 5 }).unwrap();
    let early = ["zq0x", "zq1x", "ōsaka", "nara", "kyōto", "abc", "def", "zfresh", "plain", "istanbul", "img"];
    for w in early {
        assert_eq!(shown(&c, w), expected(&c, w), "{w:?}: while the thread makes the text");
    }
    assert_eq!(c.search_text_state(), "making");
    // the thread is done but its text is not taken yet, and more edits come
    gate.open();
    wait_for(&c, "the thread did not end", |c| c.search_text_state() != "making");
    assert_eq!(c.search_text_state(), "finished");
    c.apply(Op::SetFile { id: PhotoId(5), file_name: "After.jpg".into(), source: Source::Demo { scene: 1 } }).unwrap();
    edit(&mut c, 3, &|m| m.keywords = vec!["Zq2x".into()]);
    wait_until_ready(&c);
    assert_current(&c);
    for w in early.iter().copied().chain(["after", "zq2x"]).chain(QUERIES.iter().copied()) {
        assert_eq!(shown(&c, w), expected(&c, w), "{w:?}: from the text");
    }
    assert_eq!((found(&c, "after"), found(&c, "zq2x"), found(&c, "zq0x"), found(&c, "nara")), (1, 1, 1, 1));
    // …and the text, once taken, follows the edits as any other
    edit(&mut c, 3, &|m| m.keywords = vec!["Zq3x".into()]);
    c.apply(Op::RemovePhoto { id: PhotoId(9) }).unwrap();
    assert_current(&c);
    assert_eq!((found(&c, "zq3x"), found(&c, "zq2x"), found(&c, "zq1x"), found(&c, "zfresh")), (1, 0, 0, 0));
}

/// A thread that fails leaves the search answering from the fields: the catalog is not taken down with it, and no second
/// thread is started for the same failure.
#[test]
fn a_thread_that_fails_leaves_the_search_answering_from_the_fields() {
    let mut c = library();
    c.set_search_background_from(0);
    c.hold_search_text(|| panic!("the thread that makes the text fails (a test)"));
    assert_eq!(found(&c, "ōsaka"), 1);
    wait_for(&c, "the thread did not end", |c| c.search_text_state() != "making");
    assert_eq!(c.search_text_state(), "finished");
    for q in QUERIES.iter().copied() {
        assert_eq!(shown(&c, q), expected(&c, q), "{q:?}: after the thread failed");
    }
    assert_eq!((c.search_text_state(), c.search_text_ready()), ("failed", false));
    c.apply(Op::SetRating { id: PhotoId(3), rating: 2 }).unwrap();
    c.prebuild_search_text();
    assert_eq!(found(&c, "plain"), 1);
    assert_eq!((c.search_text_state(), c.search_text_spawned()), ("failed", 1), "no second thread for the same failure");
}

/// A copy starts without the text and makes its own, whatever the catalog it was copied from is doing.
#[test]
fn a_clone_of_a_catalog_whose_text_is_being_made_makes_its_own() {
    let (c, gate) = held();
    assert_eq!(found(&c, "ōsaka"), 1);
    let copy = c.clone();
    assert_eq!((copy.search_text_state(), copy.search_text_spawned()), ("idle", 0));
    gate.open();
    wait_until_ready(&c);
    assert_eq!(copy.search_text_state(), "idle", "the copy does not take what the other catalog's thread made");
    assert_eq!(found(&copy, "ōsaka"), 1);
}

/// Dropping a catalog doesn't wait for the thread that makes its text.
#[test]
fn a_catalog_dropped_while_its_text_is_being_made_does_not_wait_for_the_thread() {
    let (c, gate) = held();
    assert_eq!(found(&c, "ōsaka"), 1);
    let (done, dropped) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        drop(c);
        let _ = done.send(());
    });
    assert!(dropped.recv_timeout(std::time::Duration::from_secs(30)).is_ok(), "dropping the catalog waited for the thread");
    gate.open();
}

proptest::proptest! {
    #![proptest_config(proptest::prelude::ProptestConfig::with_cases(128))]

    /// The block of every photo's text reads, for each photo, as the text made for that photo alone, whatever the fields hold
    /// and in whatever order the photos are asked for; a copy of a photo is at another place and reads as none.
    #[test]
    fn the_block_reads_as_each_photos_own_text(
        fields in proptest::collection::vec(proptest::collection::vec("[aAzZ0 ßİΣσǅ|:\\x00]{0,5}", 9), 0..24),
        step in 1usize..7,
    ) {
        let photos: Vec<Photo> = fields
            .iter()
            .enumerate()
            .map(|(i, f)| {
                let mut p = photo(3 * i as u64 + 2, &f[0]);
                p.meta.title = f[1].clone();
                p.meta.caption = f[2].clone();
                p.meta.camera = f[3].clone();
                p.meta.lens = f[4].clone();
                p.meta.location = f[5].clone();
                p.format = f[6].clone();
                p.meta.keywords = f[7..].to_vec();
                p
            })
            .collect();
        let block = SearchText::of(photos.iter()).unwrap();
        let n = photos.len();
        let ascending: Vec<usize> = (0..n).collect();
        let descending: Vec<usize> = (0..n).rev().collect();
        let strided: Vec<usize> = (0..step).flat_map(|first| (first..n).step_by(step)).collect();
        for order in [ascending, descending, strided] {
            for i in order {
                let p = &photos[i];
                let own = PhotoText::of(p);
                proptest::prop_assert_eq!(block.get(p), Some(own.text()), "photo {}", p.id.0);
            }
        }
        for p in &photos {
            proptest::prop_assert_eq!(block.get(&p.clone()), None, "a copy of photo {}", p.id.0);
        }
        proptest::prop_assert_eq!(block.get(&photo(1, "not there")), None);
    }
}
