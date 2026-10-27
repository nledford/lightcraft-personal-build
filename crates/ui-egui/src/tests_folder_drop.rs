//! Photos dragged from the grid onto a Folders row move there on disk: the row lights up, the
//! dragged label says where they will go, a closed folder held under the pointer opens, and a
//! release asks first (unless the person said not to), then moves the files in the background
//! (one undo step; `move_task`). Photos that can't go there, or are there already, say so and
//! stay where they are.
//! A folder's menu does the same with Move Selected Photos Here, grayed out until photos are selected.

use std::path::{Path, PathBuf};

use egui::{Pos2, Rect};
use serde_json::json;

use crate::headless::Headless;
use crate::state::MoveProblemKind;
use crate::{LightcraftApp, Services};

/// Folders `trip` and `home` in a fresh temporary folder, with a photo file in each.
pub(crate) fn folders(tag: &str) -> (PathBuf, PathBuf, PathBuf) {
    let root = std::env::temp_dir().join(format!("lc-folder-drop-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let (trip, home) = (root.join("trip"), root.join("home"));
    for (dir, file) in [(&trip, "beach.png"), (&home, "garden.png")] {
        std::fs::create_dir_all(dir).unwrap();
        write_png(&dir.join(file), [90, 120, 160]);
    }
    (root, trip, home)
}

/// A small PNG at `path`, its picture told apart from the others' by `rgb`.
fn write_png(path: &Path, rgb: [u8; 3]) {
    let img = lightcraft_raster::Rgba8 { width: 8, height: 8, data: vec![[rgb[0], rgb[1], rgb[2], 255]; 64] };
    let png = lightcraft_codecs::encode_png(&lightcraft_codecs::EncodeImage::rgba8(&img), &lightcraft_codecs::EncodeMeta::default()).unwrap();
    std::fs::write(path, png).unwrap();
}

/// The photos at `paths` in the catalog (`local`: browsed, not in the library), in the grid with
/// the sidebar out, tall enough for the Folders section.
pub(crate) fn app(paths: &[(PathBuf, bool)]) -> Headless {
    app_with(paths, Services { png: None, ..Default::default() })
}

/// [`app`] on a host with `services` (a folder picker, say).
fn app_with(paths: &[(PathBuf, bool)], services: Services) -> Headless {
    use lightcraft_catalog::{Op, Photo, Source};
    let mut session = lightcraft_engine::Session::new().with_fs();
    for (path, local) in paths {
        let id = session.catalog.alloc_photo_id();
        let name = path.file_name().unwrap().to_string_lossy().to_string();
        let mut p = Photo::new(id, Source::File { path: path.to_string_lossy().into() }, &name, "PNG", 8, 8, "2026-01-01T10:00:00");
        p.local = *local;
        session.catalog.apply(Op::AddPhoto { photo: Box::new(p) }).unwrap();
    }
    let mut h = Headless::new(LightcraftApp::new(session, services), [1300.0, 1600.0], 1.0);
    let r = h.request("ui.set", json!({"view": "photoGrid", "leftPanel": true, "sidebar": ["folders"]}), std::time::Duration::from_secs(10));
    assert_eq!(r["ok"], true, "{r}");
    for _ in 0..6 {
        h.step();
    }
    h
}

pub(crate) fn widget(h: &Headless, id: &str) -> Option<Rect> {
    h.app.widgets.iter().find(|(w, _)| w == id).map(|(_, r)| *r)
}

/// How the Folders section names `dir` in widget ids: with `/` separators, on Windows too.
pub(crate) fn folder_key(dir: &Path) -> String {
    dir.to_string_lossy().replace('\\', "/")
}

pub(crate) fn folder_row(h: &Headless, dir: &Path) -> Option<Rect> {
    widget(h, &format!("source:libfolder:{}", folder_key(dir)))
}

pub(crate) fn id_of(h: &Headless, file: &str) -> u64 {
    h.app.session.catalog.photos().find(|p| p.file_name == file).map(|p| p.id.0).unwrap()
}

pub(crate) fn press(h: &mut Headless, at: Pos2) {
    h.app.synthetic.push(egui::Event::PointerMoved(at));
    h.app.synthetic.push(egui::Event::PointerButton {
        pos: at,
        button: egui::PointerButton::Primary,
        pressed: true,
        modifiers: egui::Modifiers::NONE,
    });
    h.step();
}

/// Move the pointer, pressed, from `from` to `to` over a few frames.
pub(crate) fn glide(h: &mut Headless, from: Pos2, to: Pos2) {
    for i in 1..=8 {
        h.app.synthetic.push(egui::Event::PointerMoved(from + (to - from) * (i as f32 / 8.0)));
        h.step();
    }
}

pub(crate) fn release(h: &mut Headless, at: Pos2) {
    h.app.synthetic.push(egui::Event::PointerButton {
        pos: at,
        button: egui::PointerButton::Primary,
        pressed: false,
        modifiers: egui::Modifiers::NONE,
    });
    h.step();
    h.step();
}

/// What the dragged label says this frame instead of the count, if a drop target set it.
pub(crate) fn hint(h: &Headless) -> Option<String> {
    h.view.ctx.data(|d| d.get_temp::<(u64, String)>(egui::Id::unique("drag-photos-hint"))).map(|(_, text)| text)
}

/// Click a widget the way a person does (press and release on it).
pub(crate) fn click(h: &mut Headless, id: &str) {
    let at = widget(h, id).unwrap_or_else(|| panic!("no {id}")).center();
    press(h, at);
    release(h, at);
}

pub(crate) fn toast(h: &Headless) -> String {
    h.app.ui.toast().map(|t| t.text.clone()).unwrap_or_default()
}

/// Let the background move end, as the frames that follow it do.
fn settle(h: &mut Headless) {
    assert!(crate::move_task::wait(&mut h.app, &h.view.ctx, std::time::Duration::from_secs(20)), "the move ended");
}

#[test]
fn photos_dropped_on_a_folder_move_there_on_disk() {
    let (root, trip, home) = folders("move");
    let mut h = app(&[(trip.join("beach.png"), false), (home.join("garden.png"), false)]);
    let beach = id_of(&h, "beach.png");
    // the two folders sit in one that starts closed: only it is there to drop on
    let parent = folder_row(&h, &root).unwrap_or_else(|| {
        panic!("the folder holding both: {:?}", h.app.widgets.iter().filter(|(w, _)| w.starts_with("source:")).collect::<Vec<_>>())
    });
    assert!(folder_row(&h, &home).is_none(), "closed");
    // Given the beach photo dragged from the grid and held over the closed folder
    let from = widget(&h, &format!("thumb:{beach}")).unwrap().center();
    press(&mut h, from);
    glide(&mut h, from, parent.center());
    assert_eq!(h.app.ui.dragging_photos, Some(vec![beach]));
    for _ in 0..60 {
        h.step();
    }
    // Then it opens, so a folder inside it can be reached
    let target = folder_row(&h, &home).expect("held over, the folder opened");
    // When it is brought over home: the label says where it will go
    glide(&mut h, parent.center(), target.center());
    assert_eq!(hint(&h).as_deref(), Some("Move 1 file to “home”"));
    // and released there: it asks first, nothing moved yet
    release(&mut h, target.center());
    assert!(matches!(h.app.ui.dialog, Some(crate::state::Dialog::MovePhotos { .. })), "{:?}", h.app.ui.dialog);
    assert!(trip.join("beach.png").is_file(), "not before Move");
    h.step();
    click(&mut h, "button:dialogOk");
    settle(&mut h);
    // Then the file moved, the photo follows it, and the toast says Undo moves it back
    assert!(home.join("beach.png").is_file() && !trip.join("beach.png").exists(), "moved on disk");
    let photo = h.app.session.catalog.photo(lightcraft_catalog::PhotoId(beach)).unwrap();
    assert!(matches!(&photo.source, lightcraft_catalog::Source::File { path } if Path::new(path) == home.join("beach.png")));
    assert!(toast(&h).starts_with("Moved 1 file to “home”. Edit → Undo moves them back."), "{}", toast(&h));
    assert!(h.app.ui.dragging_photos.is_none(), "the drag ended");
    // and one Undo moves it back
    h.app.run("edit.undo", json!({})).unwrap();
    assert!(trip.join("beach.png").is_file() && !home.join("beach.png").exists(), "back where it was");
    let _ = std::fs::remove_dir_all(&root);
}

/// A selection set with a repeated id holds each photo once, so the drag carries it once.
#[test]
fn a_drag_after_selecting_with_a_repeated_id_carries_each_photo_once() {
    let (root, trip, home) = folders("dedupe");
    let mut h = app(&[(trip.join("beach.png"), false), (home.join("garden.png"), false)]);
    let (beach, garden) = (id_of(&h, "beach.png"), id_of(&h, "garden.png"));
    h.app.run("library.select", json!({"ids": [beach, garden, beach], "active": beach})).unwrap();
    let from = widget(&h, &format!("thumb:{beach}")).unwrap().center();
    press(&mut h, from);
    glide(&mut h, from, from + egui::vec2(60.0, 60.0));
    assert_eq!(h.app.ui.dragging_photos, Some(vec![beach, garden]));
    release(&mut h, from);
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn local_photos_are_refused_with_the_reason() {
    let (root, trip, home) = folders("local");
    // the garden photo imported; the beach one only looked at under Local, where the grid shows it
    let mut h = app(&[(home.join("garden.png"), false)]);
    let r = h.request(
        "engine.execute",
        json!({"command": "library.browse", "params": {"path": trip.to_string_lossy()}}),
        std::time::Duration::from_secs(10),
    );
    assert_eq!(r["ok"], true, "{r}");
    // (the folder is read in the background)
    let shown = |h: &Headless| {
        h.app.session.catalog.photos().any(|p| p.file_name == "beach.png") && h.app.widgets.iter().any(|(w, _)| w.starts_with("thumb:"))
    };
    h.step_until(std::time::Duration::from_secs(20), |h| shown(h));
    let beach = id_of(&h, "beach.png");
    assert!(h.app.session.catalog.photo(lightcraft_catalog::PhotoId(beach)).unwrap().local);
    let target = folder_row(&h, &home).expect("the imported folder");
    let from = widget(&h, &format!("thumb:{beach}")).unwrap().center();
    press(&mut h, from);
    glide(&mut h, from, target.center());
    // Then the row says it can't take them, and a release says why
    assert_eq!(hint(&h).as_deref(), Some("Can't be moved here"));
    release(&mut h, target.center());
    assert!(trip.join("beach.png").is_file() && !home.join("beach.png").exists(), "stays");
    assert!(toast(&h).contains("Add to My Photos"), "{}", toast(&h));
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn photos_already_in_the_folder_stay_put() {
    let (root, trip, home) = folders("here");
    let mut h = app(&[(trip.join("beach.png"), false), (home.join("garden.png"), false)]);
    // open the folder holding both
    let toggle = widget(&h, &format!("libraryFolderToggle:{}", folder_key(&root))).unwrap();
    h.app.synthetic.push(egui::Event::PointerMoved(toggle.center()));
    h.app.synthetic.push(egui::Event::PointerButton {
        pos: toggle.center(),
        button: egui::PointerButton::Primary,
        pressed: true,
        modifiers: egui::Modifiers::NONE,
    });
    h.step();
    release(&mut h, toggle.center());
    let target = folder_row(&h, &home).expect("opened");
    let garden = id_of(&h, "garden.png");
    h.app.synthetic.push(egui::Event::PointerMoved(target.center()));
    h.app.synthetic.push(egui::Event::PointerButton {
        pos: target.center(),
        button: egui::PointerButton::Primary,
        pressed: true,
        modifiers: egui::Modifiers::NONE,
    });
    h.step();
    h.step();
    // a drag exists only with a button down: the photos are picked up once it is
    h.app.ui.dragging_photos = Some(vec![garden]);
    h.step();
    assert_eq!(hint(&h).as_deref(), Some("Already in “home”"));
    let undo = h.app.session.undo.len();
    release(&mut h, target.center());
    assert!(home.join("garden.png").is_file());
    assert_eq!(h.app.session.undo.len(), undo, "nothing done");
    assert!(widget(&h, "button:toastUndo").is_none(), "nothing to undo");
    let _ = std::fs::remove_dir_all(&root);
}

/// Open the folder holding `home`, add `extra_copies` photos at `trip/beach.png` (virtual copies of
/// the same file), hold the ids `pick` chooses over `home` and say what the label reads.
fn hint_over_home(tag: &str, extra_copies: usize, demo: bool, pick: impl Fn(&Headless) -> Vec<u64>) -> Option<String> {
    use lightcraft_catalog::{Op, Photo, Source};
    let (root, trip, home) = folders(tag);
    let mut h = app(&[(trip.join("beach.png"), false), (home.join("garden.png"), false)]);
    for _ in 0..extra_copies {
        let id = h.app.session.catalog.alloc_photo_id();
        let p =
            Photo::new(id, Source::File { path: trip.join("beach.png").to_string_lossy().into() }, "beach.png", "PNG", 8, 8, "2026-01-01T10:00:00");
        h.app.session.catalog.apply(Op::AddPhoto { photo: Box::new(p) }).unwrap();
    }
    if demo {
        let id = h.app.session.catalog.alloc_photo_id();
        let p = Photo::new(id, Source::Demo { scene: 1 }, "demo.jpg", "JPEG", 8, 8, "2026-01-01T10:00:00");
        h.app.session.catalog.apply(Op::AddPhoto { photo: Box::new(p) }).unwrap();
    }
    let toggle = widget(&h, &format!("libraryFolderToggle:{}", folder_key(&root))).unwrap();
    press(&mut h, toggle.center());
    release(&mut h, toggle.center());
    let target = folder_row(&h, &home).expect("opened");
    press(&mut h, target.center());
    h.step();
    // a drag exists only with a button down: the photos are picked up once it is
    h.app.ui.dragging_photos = Some(pick(&h));
    h.step();
    let text = hint(&h);
    let _ = std::fs::remove_dir_all(&root);
    text
}

#[test]
fn the_label_counts_only_the_photos_a_drop_moves() {
    // one already in home, one outside it
    let text = hint_over_home("count", 0, false, |h| vec![id_of(h, "garden.png"), id_of(h, "beach.png")]);
    assert_eq!(text.as_deref(), Some("Move 1 file to “home”"));
}

#[test]
fn a_photo_without_a_file_is_refused_not_moved_or_already_there() {
    let text = hint_over_home("demo", 0, true, |h| vec![id_of(h, "demo.jpg")]);
    assert_eq!(text.as_deref(), Some("Can't be moved here"));
}

#[test]
fn virtual_copies_of_one_file_count_once() {
    // the master and a virtual copy of beach.png: one file moves
    let text =
        hint_over_home("copies", 1, false, |h| h.app.session.catalog.photos().filter(|p| p.file_name == "beach.png").map(|p| p.id.0).collect());
    assert_eq!(text.as_deref(), Some("Move 1 file to “home”"));
}

/// What the question asked after the photos `pick` chooses are dropped on `home` says it moves.
fn question_over_home(tag: &str, extra_copies: usize, pick: impl Fn(&Headless) -> Vec<u64>) -> String {
    use lightcraft_catalog::{Op, Photo, Source};
    let (root, trip, home) = folders(tag);
    let mut h = app(&[(trip.join("beach.png"), false), (home.join("garden.png"), false)]);
    assert!(h.app.ui.settings.ask_before_move);
    for _ in 0..extra_copies {
        let id = h.app.session.catalog.alloc_photo_id();
        let p =
            Photo::new(id, Source::File { path: trip.join("beach.png").to_string_lossy().into() }, "beach.png", "PNG", 8, 8, "2026-01-01T10:00:00");
        h.app.session.catalog.apply(Op::AddPhoto { photo: Box::new(p) }).unwrap();
    }
    click(&mut h, &format!("libraryFolderToggle:{}", folder_key(&root)));
    let target = folder_row(&h, &home).expect("opened");
    press(&mut h, target.center());
    h.step();
    h.app.ui.dragging_photos = Some(pick(&h));
    h.step();
    release(&mut h, target.center());
    h.step();
    assert!(matches!(h.app.ui.dialog, Some(crate::state::Dialog::MovePhotos { .. })), "{:?}", h.app.ui.dialog);
    let text = h.painted_text().into_iter().find(|t| t.starts_with("Move ") && t.ends_with('?')).unwrap_or_default();
    let _ = std::fs::remove_dir_all(&root);
    text
}

#[test]
fn the_question_counts_virtual_copies_of_one_file_once_like_the_label() {
    let text = question_over_home("q-copies", 1, |h| h.app.session.catalog.photos().filter(|p| p.file_name == "beach.png").map(|p| p.id.0).collect());
    assert_eq!(text, "Move 1 file to “home”?");
}

#[test]
fn the_question_leaves_out_a_photo_already_in_the_folder_like_the_label() {
    let text = question_over_home("q-count", 0, |h| vec![id_of(h, "garden.png"), id_of(h, "beach.png")]);
    assert_eq!(text, "Move 1 file to “home”?");
}

#[test]
fn photos_that_leave_the_view_leave_the_selection() {
    let (root, trip, home) = folders("selection");
    std::fs::copy(trip.join("beach.png"), trip.join("dunes.png")).unwrap();
    let mut h = app(&[(trip.join("beach.png"), false), (trip.join("dunes.png"), false), (home.join("garden.png"), false)]);
    // shown by folder: trip's two photos, both selected
    let r = h.request(
        "engine.execute",
        json!({"command": "library.source", "params": {"kind": "libraryFolder", "path": trip.to_string_lossy()}}),
        std::time::Duration::from_secs(10),
    );
    assert_eq!(r["ok"], true, "{r}");
    let (beach, dunes) = (id_of(&h, "beach.png"), id_of(&h, "dunes.png"));
    h.app.session.selection =
        lightcraft_engine::Selection { ids: vec![lightcraft_catalog::PhotoId(beach)], active: Some(lightcraft_catalog::PhotoId(beach)) };
    for _ in 0..4 {
        h.step();
    }
    let target = folder_row(&h, &home).expect("the other folder is listed beside it");
    let from = widget(&h, &format!("thumb:{beach}")).unwrap().center();
    press(&mut h, from);
    glide(&mut h, from, target.center());
    release(&mut h, target.center());
    h.step();
    click(&mut h, "button:dialogOk");
    settle(&mut h);
    assert!(home.join("beach.png").is_file(), "moved");
    // it left trip's view, and the selection with it: a Delete next can't reach it unseen
    assert!(!h.app.session.selection.contains(lightcraft_catalog::PhotoId(beach)), "{:?}", h.app.session.selection);
    assert!(h.app.session.visible_cloned().contains(&lightcraft_catalog::PhotoId(dunes)));
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn a_folder_left_and_come_back_to_waits_again_before_opening() {
    let (root, trip, home) = folders("spring");
    let mut h = app(&[(trip.join("beach.png"), false), (home.join("garden.png"), false)]);
    let beach = id_of(&h, "beach.png");
    let parent = folder_row(&h, &root).unwrap();
    let from = widget(&h, &format!("thumb:{beach}")).unwrap().center();
    press(&mut h, from);
    // over the closed folder for a moment, then away, longer than it waits
    glide(&mut h, from, parent.center());
    glide(&mut h, parent.center(), from);
    for _ in 0..60 {
        h.step();
    }
    // back over it: it doesn't open at once
    glide(&mut h, from, parent.center());
    assert!(folder_row(&h, &home).is_none(), "came back to: it waits again");
    for _ in 0..60 {
        h.step();
    }
    assert!(folder_row(&h, &home).is_some(), "held: it opens");
    release(&mut h, parent.center());
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn a_folder_opened_by_holding_a_drag_over_it_is_not_kept_in_the_settings() {
    let (root, trip, home) = folders("spring-kept");
    let mut h = app(&[(trip.join("beach.png"), false), (home.join("garden.png"), false)]);
    let beach = id_of(&h, "beach.png");
    let parent = folder_row(&h, &root).unwrap();
    let from = widget(&h, &format!("thumb:{beach}")).unwrap().center();
    press(&mut h, from);
    glide(&mut h, from, parent.center());
    for _ in 0..60 {
        h.step();
    }
    assert!(folder_row(&h, &home).is_some(), "held: it opens");
    release(&mut h, parent.center());
    assert!(h.app.ui.sidebar_trees.is_empty(), "an automatic opening is for the session, not a choice kept");
    let _ = std::fs::remove_dir_all(&root);
}

/// Drag the beach photo from the grid onto `target` and let go.
fn drop_beach(h: &mut Headless, beach: u64, target: Rect) {
    let from = widget(h, &format!("thumb:{beach}")).unwrap().center();
    press(h, from);
    glide(h, from, target.center());
    release(h, target.center());
    h.step();
}

#[test]
fn cancelling_the_question_moves_nothing() {
    let (root, trip, home) = folders("cancel");
    let mut h = app(&[(trip.join("beach.png"), false), (home.join("garden.png"), false)]);
    let beach = id_of(&h, "beach.png");
    let target = folder_row(&h, &home).unwrap_or_else(|| folder_row(&h, &root).unwrap());
    let target = if folder_row(&h, &home).is_some() {
        target
    } else {
        click(&mut h, &format!("libraryFolderToggle:{}", folder_key(&root)));
        folder_row(&h, &home).unwrap()
    };
    let undo = h.app.session.undo.len();
    drop_beach(&mut h, beach, target);
    assert!(matches!(h.app.ui.dialog, Some(crate::state::Dialog::MovePhotos { .. })));
    click(&mut h, "button:dialogCancel");
    assert!(h.app.ui.dialog.is_none());
    assert!(trip.join("beach.png").is_file() && !home.join("beach.png").exists(), "nothing moved");
    assert_eq!(h.app.session.undo.len(), undo);
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn dont_ask_again_moves_the_next_drop_at_once() {
    let (root, trip, home) = folders("dont-ask");
    std::fs::copy(trip.join("beach.png"), trip.join("dunes.png")).unwrap();
    let mut h = app(&[(trip.join("beach.png"), false), (trip.join("dunes.png"), false), (home.join("garden.png"), false)]);
    click(&mut h, &format!("libraryFolderToggle:{}", folder_key(&root)));
    let target = folder_row(&h, &home).unwrap();
    let beach = id_of(&h, "beach.png");
    h.app.session.selection = lightcraft_engine::Selection::single(lightcraft_catalog::PhotoId(beach));
    drop_beach(&mut h, beach, target);
    // the question, with Don't ask again ticked
    click(&mut h, "check:moveDontAsk");
    click(&mut h, "button:dialogOk");
    settle(&mut h);
    assert!(home.join("beach.png").is_file());
    assert!(!h.app.ui.settings.ask_before_move, "kept in Settings");
    // the next drop goes at once
    let dunes = id_of(&h, "dunes.png");
    h.app.session.selection = lightcraft_engine::Selection::single(lightcraft_catalog::PhotoId(dunes));
    h.step();
    let target = folder_row(&h, &home).unwrap();
    drop_beach(&mut h, dunes, target);
    settle(&mut h);
    assert!(h.app.ui.dialog.is_none(), "not asked");
    assert!(home.join("dunes.png").is_file(), "moved at once");
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn dont_ask_again_is_kept_only_when_the_move_began() {
    let (root, trip, home) = folders("dont-ask-refused");
    let mut h = app(&[(trip.join("beach.png"), false), (home.join("garden.png"), false)]);
    click(&mut h, &format!("libraryFolderToggle:{}", folder_key(&root)));
    let target = folder_row(&h, &home).unwrap();
    let beach = id_of(&h, "beach.png");
    drop_beach(&mut h, beach, target);
    click(&mut h, "check:moveDontAsk");
    assert!(matches!(h.app.ui.dialog, Some(crate::state::Dialog::MovePhotos { dont_ask: true, .. })), "ticked");
    // a file of that name gets into the folder while the question is open (not in the library): the move is refused
    // (a name taken before the drop is found by the check and listed in the question, #2897)
    std::fs::copy(trip.join("beach.png"), home.join("beach.png")).unwrap();
    click(&mut h, "button:dialogOk");
    settle(&mut h);
    assert!(h.app.ui.dialog.is_none(), "the refusal closes the question");
    assert!(trip.join("beach.png").is_file(), "nothing moved");
    assert!(h.app.ui.settings.ask_before_move, "a refused move leaves the question on");
    let _ = std::fs::remove_dir_all(&root);
}

/// A confirmation that finds the photos in the folder already (they were moved there while the question was
/// open, or a folder synchronization relinked them) moves nothing, so it doesn't turn the question off either.
#[test]
fn dont_ask_again_is_kept_only_when_a_photo_moved() {
    let (root, trip, home) = folders("dont-ask-already");
    let mut h = app(&[(trip.join("beach.png"), false), (home.join("garden.png"), false)]);
    click(&mut h, &format!("libraryFolderToggle:{}", folder_key(&root)));
    let target = folder_row(&h, &home).unwrap();
    let beach = id_of(&h, "beach.png");
    drop_beach(&mut h, beach, target);
    click(&mut h, "check:moveDontAsk");
    assert!(matches!(h.app.ui.dialog, Some(crate::state::Dialog::MovePhotos { dont_ask: true, .. })), "ticked");
    // the photo gets there some other way while the question is open
    h.app.run("photo.moveToFolder", json!({"folder": home.to_string_lossy(), "ids": [beach]})).unwrap();
    assert!(home.join("beach.png").is_file());
    click(&mut h, "button:dialogOk");
    assert!(h.app.ui.dialog.is_none());
    assert!(h.app.ui.settings.ask_before_move, "nothing moved: the question stays on");
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn a_question_left_open_while_another_library_opens_moves_nothing() {
    let (root, trip, home) = folders("other-library");
    let mut h = app(&[(trip.join("beach.png"), false), (home.join("garden.png"), false)]);
    click(&mut h, &format!("libraryFolderToggle:{}", folder_key(&root)));
    let beach = id_of(&h, "beach.png");
    let target = folder_row(&h, &home).unwrap();
    drop_beach(&mut h, beach, target);
    assert!(matches!(h.app.ui.dialog, Some(crate::state::Dialog::MovePhotos { .. })));
    // another library opens meanwhile (the menu bar and agents reach past a dialog)
    let other = root.join("other-library");
    h.app.session.open_library(other.join("lib"), false).unwrap();
    let r = h.request("ui.dialog.confirm", json!({}), std::time::Duration::from_secs(10));
    assert_eq!(r["ok"], false, "{r}");
    assert!(r["error"].as_str().unwrap_or_default().contains("Another library was opened"), "{r}");
    assert!(trip.join("beach.png").is_file() && !home.join("beach.png").exists(), "nothing moved");
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn a_background_move_has_a_cancellable_row_and_holds_back_other_moves_until_it_ends() {
    let (root, trip, home) = folders("background");
    let mut h = app(&[(trip.join("beach.png"), false), (home.join("garden.png"), false)]);
    let (beach, garden) = (id_of(&h, "beach.png"), id_of(&h, "garden.png"));
    // When an agent asks for the move in the background, the call comes back at once
    let r = h.app.run("photo.moveToFolder", json!({"ids": [beach], "folder": home.to_string_lossy(), "background": true})).unwrap();
    assert_eq!(r, json!({"background": true, "files": 1}));
    // Then the activity stack shows it, with ✕
    let row = h.app.session.activity.list().into_iter().find(|t| t.kind == "move").expect("a row for the move");
    assert_eq!((row.label.as_str(), row.total, row.cancellable), ("Moving photos", 1, true));
    let inspect = crate::control::inspect(&h.app, &h.view.ctx);
    assert_eq!(inspect["move"]["running"]["files"], 1, "{}", inspect["move"]);
    // and, until it ends, another move waits for it, and Quit asks first
    let e = h.app.run("photo.moveToFolder", json!({"ids": [garden], "folder": trip.to_string_lossy()})).unwrap_err();
    assert!(e.contains("being moved"), "{e}");
    assert_eq!(toast(&h), e, "said where the person sees it: the status line isn't drawn");
    // so is a change that would take the moving photo out of the library, whichever menu or button asked
    h.app.ui.toasts.clear();
    h.app.run("library.select", json!({"ids": [beach]})).unwrap();
    let e = h.app.run("photo.deletePermanently", json!({"ids": [beach]})).unwrap_err();
    assert_eq!(e, "1 photo is being moved to another folder: wait for the move to finish");
    assert_eq!(toast(&h), e);
    assert!(h.app.session.catalog.photo(lightcraft_catalog::PhotoId(beach)).is_some());
    assert!(!crate::panels::notices::may_close(&mut h.app), "Quit asks while the move runs");
    assert!(format!("{:?}", h.app.quit_prompt).contains("Moving photos"), "{:?}", h.app.quit_prompt);
    h.app.quit_prompt = None;
    settle(&mut h);
    // Once it ends: the file moved, the photo follows, the row is gone and the end is reported
    assert!(home.join("beach.png").is_file() && !trip.join("beach.png").exists());
    let photo = h.app.session.catalog.photo(lightcraft_catalog::PhotoId(beach)).unwrap();
    assert!(matches!(&photo.source, lightcraft_catalog::Source::File { path } if Path::new(path) == home.join("beach.png")));
    assert!(h.app.session.activity.list().is_empty());
    assert!(toast(&h).starts_with("Moved 1 file to “home”."), "{}", toast(&h));
    let inspect = crate::control::inspect(&h.app, &h.view.ctx);
    assert_eq!(inspect["move"]["running"], serde_json::Value::Null);
    assert_eq!(inspect["move"]["last"], json!({"moved": 1, "of": 1, "folder": home.to_string_lossy(), "stopped": false, "skipped": []}));
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn a_background_move_refused_for_another_running_move_says_so_in_a_toast() {
    let (root, trip, home) = folders("background-refused");
    let mut h = app(&[(trip.join("beach.png"), false), (home.join("garden.png"), false)]);
    let (beach, garden) = (id_of(&h, "beach.png"), id_of(&h, "garden.png"));
    h.app.run("photo.moveToFolder", json!({"ids": [beach], "folder": home.to_string_lossy(), "background": true})).unwrap();
    // a menu, key or button that asks for the move in the background drops the returned error: the toast is what the person sees
    let e = h.app.run("photo.moveToFolder", json!({"ids": [garden], "folder": trip.to_string_lossy(), "background": true})).unwrap_err();
    assert!(e.contains("being moved"), "{e}");
    assert_eq!(toast(&h), e, "said where the person sees it");
    // and it stays up for 6 s of the app clock
    h.app.ui.toasts.clear();
    let _ = h.app.run("photo.moveToFolder", json!({"ids": [garden], "folder": trip.to_string_lossy(), "background": true}));
    let expires = h.app.ui.toast().map(|t| t.until).expect("a toast");
    assert!((expires - (h.app.tick_now() + 6.0)).abs() < 1.0, "{expires}");
    settle(&mut h);
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn a_move_started_from_the_grid_or_a_folder_row_that_is_refused_says_so_for_as_long_as_an_error() {
    // the Folders rows and the grid's Move to Folder… start a move through `move_task::start` directly
    let (root, trip, home) = folders("start-refused");
    let mut h = app(&[(trip.join("beach.png"), false), (home.join("garden.png"), false)]);
    let (beach, garden) = (id_of(&h, "beach.png"), id_of(&h, "garden.png"));
    h.app.run("photo.moveToFolder", json!({"ids": [beach], "folder": home.to_string_lossy(), "background": true})).unwrap();
    h.app.ui.toasts.clear();
    let ctx = h.view.ctx.clone();
    let e = crate::move_task::start(&mut h.app, &ctx, &json!({"ids": [garden], "folder": trip.to_string_lossy()})).unwrap_err();
    assert!(e.contains("being moved"), "{e}");
    let toast = h.app.ui.toast().cloned().expect("the refusal is told");
    let (text, until) = (toast.text, toast.until);
    assert_eq!(text, e);
    assert!(until - h.app.tick_now() > 5.0, "an error stays 6 s, not the 1.4 s of a plain toast: {}", until - h.app.tick_now());
    settle(&mut h);
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn a_move_refused_when_the_question_is_answered_stays_up_as_long_as_an_error() {
    let (root, trip, home) = folders("question-refused");
    std::fs::copy(trip.join("beach.png"), trip.join("dunes.png")).unwrap();
    let mut h = app(&[(trip.join("beach.png"), false), (trip.join("dunes.png"), false), (home.join("garden.png"), false)]);
    click(&mut h, &format!("libraryFolderToggle:{}", folder_key(&root)));
    let target = folder_row(&h, &home).unwrap();
    let (beach, dunes) = (id_of(&h, "beach.png"), id_of(&h, "dunes.png"));
    h.app.session.selection = lightcraft_engine::Selection::single(lightcraft_catalog::PhotoId(beach));
    drop_beach(&mut h, beach, target);
    assert!(matches!(h.app.ui.dialog, Some(crate::state::Dialog::MovePhotos { .. })), "{:?}", h.app.ui.dialog);
    // another move starts while the question is up: the answer is refused
    // (started on the engine and never run: it holds the library as a move on a worker would)
    let _held = lightcraft_engine::move_job::start(&mut h.app.session, &json!({"ids": [dunes], "folder": home.to_string_lossy()}), true).unwrap();
    h.app.ui.toasts.clear();
    click(&mut h, "button:dialogOk");
    let toast = h.app.ui.toast().cloned().expect("the refusal is told");
    let (text, until) = (toast.text, toast.until);
    assert!(text.contains("being moved"), "{text}");
    assert!(until - h.app.tick_now() > 5.0, "an error stays 6 s, not the 1.4 s of a plain toast: {}", until - h.app.tick_now());
    settle(&mut h);
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn a_stopped_move_says_how_far_it_got() {
    assert_eq!(crate::move_task::stopped_text(0, 3, "home"), "Stopped: no photo was moved");
    assert_eq!(crate::move_task::stopped_text(2, 6, "Porto"), "Stopped: moved 2 of 6 photos to “Porto”. Edit → Undo moves them back.");
}

/// ✕ stops a move after the file in hand, and the library follows the files that moved only when the worker
/// ends: Quit asks until then (Quit Anyway quits, Cancel keeps working and the next Quit asks again).
#[test]
fn quit_asks_while_a_stopped_move_finishes_its_file() {
    let (root, _trip, _home) = folders("quit-stopping");
    let mut h = app(&[]);
    let (_tx, guard) = crate::move_task::stalled(&mut h.app);
    h.app.session.activity.cancel(guard.id()).unwrap();
    assert!(h.app.session.activity.running_cancellable().is_empty(), "the row is stopping");
    assert!(!crate::panels::notices::may_close(&mut h.app), "Quit asks while the stopped move finishes its file");
    assert!(format!("{:?}", h.app.quit_prompt).contains("1 task is still running: Moving photos."), "listed once: {:?}", h.app.quit_prompt);
    let click = |h: &mut crate::headless::Headless, id: &str| {
        for _ in 0..3 {
            h.step();
        }
        assert!(h.app.widgets.iter().any(|(w, _)| w == id), "{id} is shown");
        assert_eq!(h.request("ui.clickWidget", json!({"id": id}), std::time::Duration::from_secs(10))["ok"], true);
    };
    // Cancel keeps working, and the next quit asks again
    click(&mut h, "button:quitCancel");
    assert!(!h.quit_requested());
    assert!(!h.app.quit_cuts_move);
    assert!(!crate::panels::notices::may_close(&mut h.app), "the next Quit asks again");
    click(&mut h, "button:quitAnyway");
    assert!(h.quit_requested(), "Quit Anyway quits instead of asking again");
    let _ = std::fs::remove_dir_all(&root);
}

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn a_move_that_finished_as_the_app_exits_is_followed_by_the_library() {
    let (root, trip, home) = folders("exit");
    let mut h = app(&[(trip.join("beach.png"), false), (home.join("garden.png"), false)]);
    let beach = id_of(&h, "beach.png");
    h.app.run("photo.moveToFolder", json!({"ids": [beach], "folder": home.to_string_lossy(), "background": true})).unwrap();
    // no frame runs `poll`: wait until the worker has finished (so its result is sent), then the
    // exit path gets exactly one look
    h.app.move_run.as_mut().unwrap().worker.take().unwrap().join().unwrap();
    assert!(home.join("beach.png").is_file());
    h.app.finish_ready_move();
    let photo = h.app.session.catalog.photo(lightcraft_catalog::PhotoId(beach)).unwrap();
    assert!(matches!(&photo.source, lightcraft_catalog::Source::File { path } if Path::new(path) == home.join("beach.png")), "{:?}", photo.source);
    assert!(h.app.move_run.is_none());
    let _ = std::fs::remove_dir_all(&root);
}

/// Folders `a/home` and `b/home` (two of one name) and `trip`, with a photo in each.
fn twin_folders(tag: &str) -> (PathBuf, PathBuf, PathBuf, PathBuf) {
    let root = std::env::temp_dir().join(format!("lc-folder-twin-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let (trip, a, b) = (root.join("trip"), root.join("a").join("home"), root.join("b").join("home"));
    for (dir, file) in [(&trip, "beach.png"), (&a, "one.png"), (&b, "two.png")] {
        std::fs::create_dir_all(dir).unwrap();
        let img = lightcraft_raster::Rgba8 { width: 8, height: 8, data: vec![[90, 120, 160, 255]; 64] };
        let png = lightcraft_codecs::encode_png(&lightcraft_codecs::EncodeImage::rgba8(&img), &lightcraft_codecs::EncodeMeta::default()).unwrap();
        std::fs::write(dir.join(file), png).unwrap();
    }
    (root, trip, a, b)
}

#[test]
fn two_folders_with_one_name_are_told_apart_in_the_label_the_question_and_the_toast() {
    let (root, trip, a, b) = twin_folders("label");
    let mut h = app(&[(trip.join("beach.png"), false), (a.join("one.png"), false), (b.join("two.png"), false)]);
    let toggle = widget(&h, &format!("libraryFolderToggle:{}", folder_key(&root))).unwrap();
    press(&mut h, toggle.center());
    release(&mut h, toggle.center());
    let beach = id_of(&h, "beach.png");
    let from = widget(&h, &format!("thumb:{beach}")).unwrap().center();
    press(&mut h, from);
    // When the photo is brought over each of the two: the label names the folder each one is in
    let (row_a, row_b) = (folder_row(&h, &a).expect("a/home"), folder_row(&h, &b).expect("b/home"));
    glide(&mut h, from, row_a.center());
    // (the label is read from the pointer as the frame before saw it)
    h.step();
    let label_a = hint(&h).expect("a label over a/home");
    glide(&mut h, row_a.center(), row_b.center());
    h.step();
    let label_b = hint(&h).expect("a label over b/home");
    assert!(label_a.starts_with("Move 1 file to “home (") && label_a.ends_with("/a)”"), "{label_a}");
    assert!(label_b.starts_with("Move 1 file to “home (") && label_b.ends_with("/b)”"), "{label_b}");
    // and the question asked on the release says the same
    release(&mut h, row_b.center());
    let Some(crate::state::Dialog::MovePhotos { name, .. }) = &h.app.ui.dialog else { panic!("{:?}", h.app.ui.dialog) };
    assert!(name.starts_with("home (") && name.ends_with("/b)"), "{name}");
    // and so does the toast
    h.step();
    click(&mut h, "button:dialogOk");
    settle(&mut h);
    assert!(toast(&h).starts_with("Moved 1 file to “home (") && toast(&h).contains("/b)”"), "{}", toast(&h));
    assert!(b.join("beach.png").is_file(), "moved into the one that was asked about");
    let _ = std::fs::remove_dir_all(&root);
}

/// Open the folder `root` (it holds `trip` and `home`) and drag the beach photo onto `home`'s row.
fn drag_beach_onto_home(h: &mut Headless, root: &Path, home: &Path) {
    let toggle = widget(h, &format!("libraryFolderToggle:{}", folder_key(root))).unwrap();
    press(h, toggle.center());
    release(h, toggle.center());
    let target = folder_row(h, home).expect("opened");
    let beach = id_of(h, "beach.png");
    let from = widget(h, &format!("thumb:{beach}")).unwrap().center();
    press(h, from);
    glide(h, from, target.center());
    release(h, target.center());
}

#[test]
fn a_move_by_drop_offers_undo_on_its_toast() {
    let (root, trip, home) = folders("undo");
    let mut h = app(&[(trip.join("beach.png"), false), (home.join("garden.png"), false)]);
    drag_beach_onto_home(&mut h, &root, &home);
    assert!(matches!(h.app.ui.dialog, Some(crate::state::Dialog::MovePhotos { .. })), "{:?}", h.app.ui.dialog);
    h.step();
    click(&mut h, "button:dialogOk");
    settle(&mut h);
    assert!(home.join("beach.png").is_file(), "moved");
    h.step();
    // Then the toast has an Undo button, and a click on it moves the photo back
    click(&mut h, "button:toastUndo");
    assert!(trip.join("beach.png").is_file() && !home.join("beach.png").exists(), "back where it was");
    assert_eq!(toast(&h), "Undo");
    // and the button goes with the toast it belonged to
    h.step();
    assert!(widget(&h, "button:toastUndo").is_none(), "no button on the toast that says Undo");
    let _ = std::fs::remove_dir_all(&root);
}

/// `trip` holds beach.png and dunes.png (both selected, shown by folder), `home` holds garden.png
/// and the Home row to drop on; the photos are dragged by the beach.
fn trip_with_two(tag: &str) -> (PathBuf, PathBuf, PathBuf, Headless, u64, u64) {
    let (root, trip, home) = folders(tag);
    std::fs::copy(trip.join("beach.png"), trip.join("dunes.png")).unwrap();
    let mut h = app(&[(trip.join("beach.png"), false), (trip.join("dunes.png"), false), (home.join("garden.png"), false)]);
    let r = h.request(
        "engine.execute",
        json!({"command": "library.source", "params": {"kind": "libraryFolder", "path": trip.to_string_lossy()}}),
        std::time::Duration::from_secs(10),
    );
    assert_eq!(r["ok"], true, "{r}");
    let (beach, dunes) = (id_of(&h, "beach.png"), id_of(&h, "dunes.png"));
    let both = [lightcraft_catalog::PhotoId(beach), lightcraft_catalog::PhotoId(dunes)];
    h.app.session.selection = lightcraft_engine::Selection { ids: both.to_vec(), active: Some(both[0]) };
    for _ in 0..4 {
        h.step();
    }
    (root, trip, home, h, beach, dunes)
}

/// Drag the selection by the beach onto the Home row and let go.
fn drop_both(h: &mut Headless, beach: u64, home: &Path) {
    let target = folder_row(h, home).expect("the Home row");
    drop_beach(h, beach, target);
}

#[test]
fn the_question_lists_a_name_clash_and_moves_the_rest_when_told_to_skip() {
    let (root, trip, home, mut h, beach, dunes) = trip_with_two("clash");
    std::fs::copy(trip.join("dunes.png"), home.join("dunes.png")).unwrap();
    drop_both(&mut h, beach, &home);
    // the problem is known before Move: the question names it and offers to skip it
    let Some(crate::state::Dialog::MovePhotos { problems, movable, .. }) = &h.app.ui.dialog else { panic!("{:?}", h.app.ui.dialog) };
    assert_eq!(
        (problems.clone(), *movable),
        (vec![crate::state::MoveProblem { id: dunes, name: "dunes.png".into(), kind: MoveProblemKind::Clash }], 1)
    );
    assert!(trip.join("beach.png").is_file(), "nothing moved before the click");
    click(&mut h, "button:dialogOk");
    settle(&mut h);
    assert!(home.join("beach.png").is_file() && !trip.join("beach.png").exists(), "the others moved");
    assert!(trip.join("dunes.png").is_file(), "the clashing one stayed");
    assert_eq!(toast(&h), "Moved 1 photo to “home”, skipped 1. Edit → Undo moves them back.");
    assert!(h.app.ui.dialog.is_none());
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn undo_is_not_offered_once_something_else_was_done() {
    let (root, trip, home) = folders("undo-stale");
    let mut h = app(&[(trip.join("beach.png"), false), (home.join("garden.png"), false)]);
    h.app.ui.settings.ask_before_move = false;
    drag_beach_onto_home(&mut h, &root, &home);
    settle(&mut h);
    h.step();
    assert!(toast(&h).starts_with("Moved 1 file"), "{}", toast(&h));
    assert!(widget(&h, "button:toastUndo").is_some(), "offered after the drop");
    // When another step is committed while the toast is still up, Undo would reach that step instead
    h.app.run("photo.rate", json!({"rating": 3})).unwrap();
    h.step();
    assert!(toast(&h).starts_with("Moved 1 file"), "still the same toast");
    assert!(widget(&h, "button:toastUndo").is_none(), "withdrawn: it would undo the rating");
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn with_nothing_that_can_move_the_question_offers_only_cancel() {
    let (root, trip, home, mut h, beach, _) = trip_with_two("all-problems");
    for f in ["beach.png", "dunes.png"] {
        std::fs::copy(trip.join(f), home.join(f)).unwrap();
    }
    drop_both(&mut h, beach, &home);
    let Some(crate::state::Dialog::MovePhotos { problems, movable, .. }) = &h.app.ui.dialog else { panic!("{:?}", h.app.ui.dialog) };
    assert_eq!((problems.len(), *movable), (2, 0));
    h.step();
    assert!(widget(&h, "button:dialogOk").is_none(), "no Move button");
    assert!(widget(&h, "button:dialogCancel").is_some());
    click(&mut h, "button:dialogCancel");
    assert!(h.app.ui.dialog.is_none() && trip.join("beach.png").is_file() && trip.join("dunes.png").is_file());
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn a_file_that_is_offline_is_found_before_the_move_and_asks_even_when_not_asking() {
    let (root, trip, home, mut h, beach, dunes) = trip_with_two("offline");
    h.app.ui.settings.ask_before_move = false;
    std::fs::remove_file(trip.join("dunes.png")).unwrap();
    drop_both(&mut h, beach, &home);
    // a move with a problem is a decision, so it asks although the person said not to
    let Some(crate::state::Dialog::MovePhotos { problems, .. }) = &h.app.ui.dialog else { panic!("{:?}", h.app.ui.dialog) };
    assert_eq!(problems.iter().map(|p| (p.id, p.kind)).collect::<Vec<_>>(), [(dunes, MoveProblemKind::Missing)]);
    assert!(trip.join("beach.png").is_file(), "nothing moved yet");
    click(&mut h, "button:dialogOk");
    settle(&mut h);
    assert!(home.join("beach.png").is_file());
    assert!(toast(&h).starts_with("Moved 1 photo to “home”, skipped 1."), "{}", toast(&h));
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn the_offline_question_names_the_menu_with_an_arrow_the_fonts_can_draw() {
    let (root, trip, home, mut h, beach, _) = trip_with_two("offline-arrow");
    std::fs::remove_file(trip.join("dunes.png")).unwrap();
    drop_both(&mut h, beach, &home);
    h.step();
    let shown = h.painted_text().join("\n");
    let line = crate::i18n::tr("Reconnect the disk or use File → Find Missing Photos…, then try again.");
    assert!(shown.contains(line), "the dialog shows the line through tr: {shown}");
    assert!(line.contains('→') && !line.contains('▸'), "no UI font has ▸, which draws as a box: {line}");
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn a_folder_that_is_gone_says_so_in_plain_words_at_the_drop() {
    let (root, _trip, home, mut h, beach, _) = trip_with_two("gone");
    let target = folder_row(&h, &home).expect("the Home row");
    std::fs::remove_dir_all(&home).unwrap();
    drop_beach(&mut h, beach, target);
    assert!(h.app.ui.dialog.is_none(), "{:?}", h.app.ui.dialog);
    let said = toast(&h);
    assert!(said.contains("is not a folder"), "{said}");
    assert!(!said.contains('`') && !said.contains("invalid parameters"), "no command names: {said}");
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn two_photos_of_one_name_in_the_move_are_listed_as_twins() {
    let (root, trip, home, mut h, beach, _) = trip_with_two("twins");
    // a second photo named beach.png, from another folder, joins the move
    let other = root.join("other");
    std::fs::create_dir_all(&other).unwrap();
    std::fs::copy(trip.join("beach.png"), other.join("beach.png")).unwrap();
    {
        use lightcraft_catalog::{Op, Photo, Source};
        let id = h.app.session.catalog.alloc_photo_id();
        let p =
            Photo::new(id, Source::File { path: other.join("beach.png").to_string_lossy().into() }, "beach.png", "PNG", 8, 8, "2026-01-01T10:00:00");
        h.app.session.catalog.apply(Op::AddPhoto { photo: Box::new(p) }).unwrap();
        h.app.session.selection.ids.push(id);
    }
    drop_both(&mut h, beach, &home);
    let Some(crate::state::Dialog::MovePhotos { problems, movable, .. }) = &h.app.ui.dialog else { panic!("{:?}", h.app.ui.dialog) };
    assert_eq!(problems.iter().map(|p| (p.name.as_str(), p.kind)).collect::<Vec<_>>(), [("beach.png", MoveProblemKind::Twin)]);
    assert_eq!(*movable, 2, "one of the pair and the other photo go");
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn each_kind_of_problem_has_its_own_line() {
    let line = |kind| crate::panels::dialogs::move_problem_line(&crate::state::MoveProblem { id: 1, name: "a.png".into(), kind }, "home");
    let kinds = [MoveProblemKind::Clash, MoveProblemKind::Twin, MoveProblemKind::Missing, MoveProblemKind::Unmovable];
    let lines: Vec<String> = kinds.into_iter().map(line).collect();
    assert!(lines.iter().all(|l| l.contains("a.png")), "each line names its photo: {lines:?}");
    assert!(lines[0].contains("home"), "a clash names the folder: {lines:?}");
    let mut distinct = lines.clone();
    distinct.sort();
    distinct.dedup();
    assert_eq!(distinct.len(), kinds.len(), "each kind says its own thing: {lines:?}");
}

#[test]
fn a_check_answer_maps_to_a_kind_and_an_unknown_kind_is_unmovable() {
    let kind = |v: serde_json::Value| MoveProblemKind::from_check(&v);
    assert_eq!(kind(json!({"kind": "clash"})), MoveProblemKind::Clash);
    assert_eq!(kind(json!({"kind": "clash", "twin": true})), MoveProblemKind::Twin);
    assert_eq!(kind(json!({"kind": "missing"})), MoveProblemKind::Missing);
    assert_eq!(kind(json!({"kind": "unmovable"})), MoveProblemKind::Unmovable);
    assert_eq!(kind(json!({"kind": "something new"})), MoveProblemKind::Unmovable);
}

/// Move Selected Photos Here stays in a folder's menu with nothing selected, grayed out (a click
/// moves nothing), so the menu keeps its shape; once a photo is selected it moves its file there.
#[test]
fn move_selected_photos_here_is_grayed_out_until_photos_are_selected() {
    let (root, trip, home) = folders("menu-move");
    let mut h = app(&[(trip.join("beach.png"), false), (home.join("garden.png"), false)]);
    click(&mut h, &format!("libraryFolderToggle:{}", folder_key(&root)));
    let garden = id_of(&h, "garden.png");
    let menu_item = |h: &mut Headless| {
        let at = folder_row(h, &trip).unwrap().center();
        let r = h.request("ui.click", json!({"x": at.x, "y": at.y, "button": "right"}), std::time::Duration::from_secs(10));
        assert_eq!(r["ok"], true, "{r}");
        h.step();
        h.step();
        let r = h.request("ui.clickWidget", json!({"id": "folderMovePhotosHere"}), std::time::Duration::from_secs(10));
        assert_eq!(r["ok"], true, "listed: {r}");
        h.step();
        h.step();
    };
    h.app.run("library.select", json!({"ids": []})).unwrap();
    h.app.ui.toasts.clear();
    menu_item(&mut h);
    // an enabled row would try and say "no photos to move"; a grayed-out one says nothing
    assert!(h.app.ui.toast().is_none(), "a grayed-out row does nothing: {}", toast(&h));
    assert!(home.join("garden.png").is_file() && !trip.join("garden.png").exists(), "grayed out: nothing moved");
    // close the menu, then select the photo and open it again
    let r = h.request("ui.key", json!({"key": "Escape"}), std::time::Duration::from_secs(10));
    assert_eq!(r["ok"], true, "{r}");
    h.step();
    h.step();
    h.app.run("library.select", json!({"ids": [garden], "active": garden})).unwrap();
    h.step();
    menu_item(&mut h);
    settle(&mut h);
    assert!(trip.join("garden.png").is_file() && !home.join("garden.png").exists(), "the selected photo's file moved");
    let path = h.app.session.catalog.photo(lightcraft_catalog::PhotoId(garden)).map(|p| p.source.clone());
    assert!(
        matches!(&path, Some(lightcraft_catalog::Source::File { path }) if Path::new(path) == trip.join("garden.png")),
        "the library follows the file: {path:?}"
    );
    let _ = std::fs::remove_dir_all(&root);
}

/// Right-click the row `id` and click the menu item `item`.
fn menu_click(h: &mut Headless, row: &str, item: &str) {
    let t = std::time::Duration::from_secs(10);
    let at = widget(h, row).unwrap_or_else(|| panic!("{row} is drawn")).center();
    assert_eq!(h.request("ui.click", json!({"x": at.x, "y": at.y, "button": "right"}), t)["ok"], true);
    h.step();
    h.step();
    let r = h.request("ui.clickWidget", json!({"id": item}), t);
    assert_eq!(r["ok"], true, "{item}: {r}");
}

/// The Import review once the scan has finished.
fn review(h: &mut Headless) -> crate::import::ImportDialog {
    assert!(
        h.step_until(std::time::Duration::from_secs(20), |h| matches!(h.app.ui.dialog, Some(crate::state::Dialog::Import { .. }))),
        "the review opens"
    );
    // (the wait above ended on this dialog)
    let Some(crate::state::Dialog::Import { opts }) = h.app.ui.dialog.clone() else { return Default::default() };
    *opts
}

/// A folder's menu starts bringing photos in: Import to This Folder… asks for a folder, then opens
/// the Import review for its files copying into this folder (all into it, not into dated
/// subfolders), and confirming puts the files there while the originals stay.
#[test]
fn import_to_this_folder_opens_the_review_copying_into_the_folder() {
    let (root, trip, home) = folders("menu-import-here");
    let card = root.join("card");
    std::fs::create_dir_all(&card).unwrap();
    write_png(&card.join("sunset.png"), [200, 90, 30]);
    let from = card.to_string_lossy().to_string();
    let services = Services { png: None, pick_folder: Some(Box::new(move || Some(from.clone()))), ..Default::default() };
    let mut h = app_with(&[(trip.join("beach.png"), false), (home.join("garden.png"), false)], services);
    click(&mut h, &format!("libraryFolderToggle:{}", folder_key(&root)));
    let row = format!("source:libfolder:{}", folder_key(&trip));
    menu_click(&mut h, &row, "folderImportHere");
    let d = review(&mut h);
    assert_eq!(d.sources, [card.to_string_lossy().to_string()]);
    assert_eq!(d.candidates.iter().map(|c| c.name.as_str()).collect::<Vec<_>>(), ["sunset.png"]);
    assert!(d.copy && !d.move_files, "copies by default: the originals stay where they are");
    assert_eq!(d.destination, folder_key(&trip), "the folder as the Folders tree names it");
    assert_eq!(d.organize, "flat", "the photos go into the folder itself");
    let r = h.request("ui.dialog.confirm", json!({}), std::time::Duration::from_secs(10));
    assert_eq!(r["ok"], true, "{r}");
    assert!(h.step_until(std::time::Duration::from_secs(30), |_| trip.join("sunset.png").is_file()), "the copy is in the folder");
    assert!(card.join("sunset.png").is_file(), "the original stays");
    let _ = std::fs::remove_dir_all(&root);
}

/// A folder picker that is cancelled opens nothing; a host with no picker has no such row.
#[test]
fn import_to_this_folder_does_nothing_when_the_picker_is_cancelled_and_needs_a_picker() {
    let (root, trip, home) = folders("menu-import-cancel");
    let services = Services { png: None, pick_folder: Some(Box::new(|| None)), ..Default::default() };
    let mut h = app_with(&[(trip.join("beach.png"), false), (home.join("garden.png"), false)], services);
    click(&mut h, &format!("libraryFolderToggle:{}", folder_key(&root)));
    let row = format!("source:libfolder:{}", folder_key(&trip));
    menu_click(&mut h, &row, "folderImportHere");
    for _ in 0..10 {
        h.step();
    }
    assert!(h.app.ui.dialog.is_none() && h.app.scan.is_none(), "cancelled: nothing opens: {:?}", h.app.ui.dialog);
    let mut bare = app(&[(trip.join("beach.png"), false), (home.join("garden.png"), false)]);
    click(&mut bare, &format!("libraryFolderToggle:{}", folder_key(&root)));
    let at = widget(&bare, &row).unwrap().center();
    bare.request("ui.click", json!({"x": at.x, "y": at.y, "button": "right"}), std::time::Duration::from_secs(10));
    bare.step();
    bare.step();
    assert!(widget(&bare, "folderRename").is_some() && widget(&bare, "folderImportHere").is_none(), "no picker, no row");
    let _ = std::fs::remove_dir_all(&root);
}

/// A Local folder's Add to My Photos… opens the review for the folder, adding in place: the
/// photos the folder was browsed with are offered (checked) instead of "In library", and
/// confirming brings them into the library without moving a file.
#[test]
fn add_to_my_photos_reviews_a_local_folders_browsed_photos() {
    let (root, trip, _home) = folders("menu-add-local");
    write_png(&trip.join("dunes.png"), [210, 180, 90]);
    let mut h = app(&[(trip.join("beach.png"), true), (trip.join("dunes.png"), true)]);
    let place = trip.to_string_lossy().to_string();
    h.app.run("local.addRoot", json!({"path": place})).unwrap();
    assert!(h.step_until(std::time::Duration::from_secs(10), |h| widget(h, &format!("source:local:{place}")).is_some()));
    menu_click(&mut h, &format!("source:local:{place}"), "folderAddToMyPhotos");
    let d = review(&mut h);
    assert_eq!(d.sources, std::slice::from_ref(&place));
    assert!(!d.copy, "added where they are");
    assert_eq!(d.candidates.len(), 2);
    assert_eq!(d.selected_paths().len(), 2, "both browsed photos are offered, checked");
    let r = h.request("ui.dialog.confirm", json!({}), std::time::Duration::from_secs(10));
    assert_eq!(r["ok"], true, "{r}");
    assert!(h.step_until(std::time::Duration::from_secs(30), |h| h.app.session.catalog.photos().all(|p| !p.local)), "the photos joined the library");
    let paths: std::collections::BTreeSet<String> = h
        .app
        .session
        .catalog
        .photos()
        .filter(|p| !p.deleted)
        .filter_map(|p| match &p.source {
            lightcraft_catalog::Source::File { path } => Some(path.clone()),
            _ => None,
        })
        .collect();
    let want: std::collections::BTreeSet<String> = ["beach.png", "dunes.png"].iter().map(|f| trip.join(f).to_string_lossy().to_string()).collect();
    assert_eq!(paths, want, "the same two photos, now in the library");
    assert!(trip.join("beach.png").is_file() && trip.join("dunes.png").is_file(), "no file moved");
    let _ = std::fs::remove_dir_all(&root);
}

/// While a scan or review is waiting, both rows are grayed out: another scan would be refused.
#[test]
fn the_import_rows_are_grayed_out_while_a_scan_runs() {
    let (root, trip, home) = folders("menu-import-busy");
    let services = Services { png: None, pick_folder: Some(Box::new(|| Some("/nowhere".into()))), ..Default::default() };
    let mut h = app_with(&[(trip.join("beach.png"), false), (home.join("garden.png"), false)], services);
    click(&mut h, &format!("libraryFolderToggle:{}", folder_key(&root)));
    let _scan = crate::import::stall_scan(&mut h.app);
    menu_click(&mut h, &format!("source:libfolder:{}", folder_key(&trip)), "folderImportHere");
    for _ in 0..5 {
        h.step();
    }
    assert!(h.app.ui.dialog.is_none(), "a grayed-out row opens nothing: {:?}", h.app.ui.dialog);
    assert!(h.app.ui.toast().is_none(), "and says nothing: {}", toast(&h));
    // a Local row's Add to My Photos… is grayed out the same way
    let place = trip.to_string_lossy().to_string();
    h.app.run("local.addRoot", json!({"path": place})).unwrap();
    assert!(h.step_until(std::time::Duration::from_secs(10), |h| widget(h, &format!("source:local:{place}")).is_some()));
    menu_click(&mut h, &format!("source:local:{place}"), "folderAddToMyPhotos");
    for _ in 0..5 {
        h.step();
    }
    assert!(h.app.ui.dialog.is_none() && h.app.ui.toast().is_none(), "{:?} {}", h.app.ui.dialog, toast(&h));
}

/// Export Folder with Preset ▸ a preset runs that preset's export, in the background, on exactly
/// the folder's photos and those of the folders inside it: the files written are theirs, and a
/// neighbour folder's photo stays out.
#[test]
fn export_folder_with_a_preset_writes_exactly_the_folders_photos() {
    let (root, trip, home) = folders("menu-export");
    std::fs::create_dir_all(trip.join("dunes")).unwrap();
    std::fs::copy(trip.join("beach.png"), trip.join("dunes").join("sand.png")).unwrap();
    let out = root.join("out");
    let mut h = app(&[(trip.join("beach.png"), false), (trip.join("dunes").join("sand.png"), false), (home.join("garden.png"), false)]);
    h.app.services.write_shared = Some(std::sync::Arc::new(|p: &str, bytes: &[u8]| lightcraft_engine::export::write_file(p, bytes)));
    h.app.run("export.savePreset", json!({"name": "Tiny PNG", "params": {"format": "png", "longEdge": 4}})).unwrap();
    // a preset names no folder: the last export's is used
    h.app.session.last_export = Some(json!({"dir": out}));
    click(&mut h, &format!("libraryFolderToggle:{}", folder_key(&root)));
    let at = folder_row(&h, &trip).unwrap().center();
    let r = h.request("ui.click", json!({"x": at.x, "y": at.y, "button": "right"}), std::time::Duration::from_secs(10));
    assert_eq!(r["ok"], true, "{r}");
    h.step();
    h.step();
    for id in ["folderExportPresetMenu", "folderExportPreset:Tiny PNG"] {
        let r = h.request("ui.clickWidget", json!({"id": id}), std::time::Duration::from_secs(10));
        assert_eq!(r["ok"], true, "{id}: {r}");
        h.step();
        h.step();
    }
    assert!(h.app.ui.dialog.is_none(), "a preset starts without asking: {}", toast(&h));
    let t0 = std::time::Instant::now();
    while (h.app.export.is_some() || h.app.last_export_result.is_none()) && t0.elapsed() < std::time::Duration::from_secs(60) {
        h.step();
    }
    let last = h.app.last_export_result.clone().expect("the export reports");
    assert_eq!(last["files"].as_array().map(Vec::len).or(last["files"].as_u64().map(|n| n as usize)), Some(2), "{last}");
    let mut names: Vec<String> = std::fs::read_dir(&out).unwrap().map(|e| e.unwrap().file_name().to_string_lossy().to_string()).collect();
    names.sort();
    assert_eq!(names.len(), 2, "{names:?}");
    assert!(names.iter().any(|n| n.contains("beach")) && names.iter().any(|n| n.contains("sand")), "{names:?}");
    assert!(!names.iter().any(|n| n.contains("garden")), "{names:?}");
    let _ = std::fs::remove_dir_all(&root);
}
