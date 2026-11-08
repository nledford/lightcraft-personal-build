//! The first-run Welcome tour: Back / Next walk its pages, Skip and the last page's Get Started
//! close it. (The auto-open at launch is decided by `startup::open_at_startup`, unit-tested there:
//! a headless host never gains the modal, so the tour is placed by hand here.)
//!
//! The empty library's welcome owns the window: the Tools panel steps aside (and comes back as it
//! was once photos arrive), the bottom bar's rating and flags hide, and a row of sample photos
//! under the buttons opens the samples on the one clicked. The window around it steps back: no
//! photo count, no bottom bar, one line in the sidebar for the sections still to fill.

use crate::headless::Headless;
use crate::state::{Dialog, RightPanel, ViewMode};
use crate::{LightcraftApp, Services};
use serde_json::json;
use std::time::Duration;

const T: Duration = Duration::from_secs(20);

/// The app with the tour open on `page` (the last page of [`crate::panels::dialogs::WELCOME_PAGES`]
/// is `WELCOME_PAGES.len() - 1`).
fn tour(page: usize) -> Headless {
    let services = Services { png: None, ..Default::default() };
    let mut app = LightcraftApp::new(lightcraft_engine::Session::with_demo(), services);
    app.ui.dialog = Some(Dialog::Welcome { page });
    let mut h = Headless::new(app, [1200.0, 800.0], 1.0);
    // fonts land on the second frame, and the dialog window settles its place a frame or two later;
    // clicks must aim at the settled layout
    h.step();
    h.settle(Duration::from_secs(5));
    h
}

fn page_of(h: &Headless) -> Option<usize> {
    match &h.app.ui.dialog {
        Some(Dialog::Welcome { page }) => Some(*page),
        _ => None,
    }
}

/// Click an automation id and let the injected press/release play out (they can straddle a frame).
fn click(h: &mut Headless, id: &str) {
    let r = h.request("ui.clickWidget", json!({"id": id}), T);
    assert_eq!(r["ok"], true, "{r}");
    h.step();
    h.step();
}

#[test]
fn welcome_pages_walk_back_next_and_skip_closes() {
    let mut h = tour(0);
    // Back is disabled on the first page: the click lands on nothing and the tour stays.
    click(&mut h, "button:welcomeBack");
    assert_eq!(page_of(&h), Some(0), "{:?}", h.app.ui.dialog);
    click(&mut h, "button:welcomeNext");
    assert_eq!(page_of(&h), Some(1), "{:?}", h.app.ui.dialog);
    click(&mut h, "button:welcomeBack");
    assert_eq!(page_of(&h), Some(0), "{:?}", h.app.ui.dialog);
    click(&mut h, "button:welcomeSkip");
    assert!(h.app.ui.dialog.is_none(), "{:?}", h.app.ui.dialog);
}

#[test]
fn welcome_last_page_gets_started_closes() {
    let last = crate::panels::dialogs::WELCOME_PAGES.len() - 1;
    let mut h = tour(last);
    click(&mut h, "button:welcomeNext");
    assert!(h.app.ui.dialog.is_none(), "{:?}", h.app.ui.dialog);
}

/// The web build omits the tour page that names the desktop-only launch flow (`--control`,
/// `lightcraft-cli mcp --connect`): a browser has no control port for an agent to attach to.
/// The host is a compile-time fact (wasm32), so this harness — native, a desktop host — tests
/// the pure decision ([`crate::dialogs::welcome_pages`]) for both answers.
#[test]
fn the_web_tour_omits_the_desktop_only_mcp_page() {
    let all = crate::dialogs::WELCOME_PAGES.as_slice();
    assert_eq!(crate::dialogs::welcome_pages(false), all, "the desktop shows every page");
    let web = crate::dialogs::welcome_pages(true);
    assert_eq!(web, &all[..all.len() - 1], "the web tour drops the last page");
    // the dropped page is the desktop-only one: its opening names the launch flow
    let (_, _, opening) = all.last().unwrap();
    assert!(opening.contains("--control 7980") && opening.contains("lightcraft-cli"), "{opening}");
}

/// The first frame decides the launch dialog, but records nothing until the dialog is presented:
/// a window host with no stored version gets the tour; a headless host (the harness, snapshots,
/// scripted runs) gets nothing; a version already seen gets nothing; a version that changed gets
/// What's New.
#[test]
fn the_first_frame_opens_the_tour_for_a_window_host_only() {
    let ctx = egui::Context::default();
    let logic = |app: &mut LightcraftApp| {
        let mut out = ctx.run_ui(egui::RawInput::default(), |ui| app.logic(ui.ctx()));
        out.textures_delta.clear();
    };
    let fresh = || LightcraftApp::new(lightcraft_engine::Session::with_demo(), Services { png: None, ..Default::default() });

    let mut window = fresh();
    logic(&mut window);
    assert!(matches!(window.ui.dialog, Some(Dialog::Welcome { page: 0 })), "{:?}", window.ui.dialog);
    assert_eq!(window.ui.seen_version, None, "chosen, not yet presented");

    let mut headless = fresh();
    headless.headless_host = true;
    logic(&mut headless);
    assert!(headless.ui.dialog.is_none(), "{:?}", headless.ui.dialog);
    assert_eq!(headless.ui.seen_version, None);

    let mut seen = fresh();
    seen.ui.seen_version = Some(env!("CARGO_PKG_VERSION").to_string());
    logic(&mut seen);
    assert!(seen.ui.dialog.is_none(), "{:?}", seen.ui.dialog);

    let mut updated = fresh();
    updated.ui.seen_version = Some("0.0.1".to_string());
    logic(&mut updated);
    assert!(matches!(updated.ui.dialog, Some(Dialog::WhatsNew)), "{:?}", updated.ui.dialog);
}

/// The version is recorded when the dialog is presented, not when the logic tick picks it: a
/// hidden host that runs `logic` and quits before a visible frame must not burn the tour.
#[test]
fn the_startup_dialog_is_recorded_when_presented_not_when_chosen() {
    let ctx = egui::Context::default();
    let frame = |app: &mut LightcraftApp| {
        let mut out = ctx.run_ui(egui::RawInput::default(), |ui| {
            app.logic(ui.ctx());
            // the presentation pass (`ui` waits for the fonts, so frame one only runs `logic`)
            app.ui(ui);
        });
        out.textures_delta.clear();
    };
    let mut held = LightcraftApp::new(lightcraft_engine::Session::with_demo(), Services { png: None, ..Default::default() });

    frame(&mut held);
    assert!(matches!(held.ui.dialog, Some(Dialog::Welcome { page: 0 })), "{:?}", held.ui.dialog);
    assert_eq!(held.ui.seen_version, None, "not presented yet, not recorded");

    frame(&mut held);
    assert!(matches!(held.ui.dialog, Some(Dialog::Welcome { page: 0 })), "{:?}", held.ui.dialog);
    assert_eq!(held.ui.seen_version.as_deref(), Some(env!("CARGO_PKG_VERSION")));

    // shown once: closing it does not bring it back
    held.ui.dialog = None;
    frame(&mut held);
    assert!(held.ui.dialog.is_none(), "{:?}", held.ui.dialog);
}

/// Another dialog open on the first frame keeps its place: the tour is not chosen that frame and
/// nothing is recorded. Once that dialog closes, the tour opens and is recorded then — shown, not
/// merely due.
#[test]
fn the_tour_waits_for_a_dialog_that_is_already_open() {
    let ctx = egui::Context::default();
    let logic = |app: &mut LightcraftApp| {
        let mut out = ctx.run_ui(egui::RawInput::default(), |ui| app.logic(ui.ctx()));
        out.textures_delta.clear();
    };
    let present = |app: &mut LightcraftApp| {
        let mut out = ctx.run_ui(egui::RawInput::default(), |ui| {
            app.logic(ui.ctx());
            app.ui(ui);
        });
        out.textures_delta.clear();
    };
    let mut held = LightcraftApp::new(lightcraft_engine::Session::with_demo(), Services { png: None, ..Default::default() });
    held.ui.dialog = Some(Dialog::ViewOptions);

    logic(&mut held);
    assert!(matches!(held.ui.dialog, Some(Dialog::ViewOptions)), "{:?}", held.ui.dialog);
    assert_eq!(held.ui.seen_version, None, "not shown, not recorded");

    held.ui.dialog = None;
    logic(&mut held);
    assert!(matches!(held.ui.dialog, Some(Dialog::Welcome { page: 0 })), "{:?}", held.ui.dialog);
    assert_eq!(held.ui.seen_version, None, "chosen behind the other dialog's turn, not yet presented");
    present(&mut held);
    assert_eq!(held.ui.seen_version.as_deref(), Some(env!("CARGO_PKG_VERSION")));

    // shown once: closing it does not bring it back
    held.ui.dialog = None;
    logic(&mut held);
    assert!(held.ui.dialog.is_none(), "{:?}", held.ui.dialog);
}

/// The launch chooser or a library error is the blocking library window, and it is not in
/// `ui.dialog`: it must still hold the startup dialog back, or the chooser's New Library name
/// prompt (a `ui.dialog` of its own) would take the tour's place with the version already
/// recorded. Once the window closes or is dismissed, the tour opens and is recorded then.
#[test]
fn the_tour_waits_for_the_launch_window_too() {
    let ctx = egui::Context::default();
    let logic = |app: &mut LightcraftApp| {
        let mut out = ctx.run_ui(egui::RawInput::default(), |ui| app.logic(ui.ctx()));
        out.textures_delta.clear();
    };
    let present = |app: &mut LightcraftApp| {
        let mut out = ctx.run_ui(egui::RawInput::default(), |ui| {
            app.logic(ui.ctx());
            app.ui(ui);
        });
        out.textures_delta.clear();
    };
    let mut held = LightcraftApp::new(lightcraft_engine::Session::with_demo(), Services { png: None, ..Default::default() });
    held.library_problem = Some(crate::panels::library_problem::LibraryProblem::new("", "the last library did not open"));

    logic(&mut held);
    assert!(held.ui.dialog.is_none(), "{:?}", held.ui.dialog);
    assert_eq!(held.ui.seen_version, None, "not shown behind the library window, not recorded");

    held.library_problem = None;
    logic(&mut held);
    assert!(matches!(held.ui.dialog, Some(Dialog::Welcome { page: 0 })), "{:?}", held.ui.dialog);
    assert_eq!(held.ui.seen_version, None, "chosen, not yet presented");
    present(&mut held);
    assert_eq!(held.ui.seen_version.as_deref(), Some(env!("CARGO_PKG_VERSION")));

    // shown once: closing it does not bring it back
    held.ui.dialog = None;
    logic(&mut held);
    assert!(held.ui.dialog.is_none(), "{:?}", held.ui.dialog);
}

/// A dismissed library window (Continue Without Saving) shows the banner instead of the window:
/// it no longer blocks, so the startup dialog opens over it as usual.
#[test]
fn the_tour_opens_once_the_library_window_is_dismissed() {
    let ctx = egui::Context::default();
    let frame = |app: &mut LightcraftApp| {
        let mut out = ctx.run_ui(egui::RawInput::default(), |ui| {
            app.logic(ui.ctx());
            app.ui(ui);
        });
        out.textures_delta.clear();
    };
    let mut held = LightcraftApp::new(lightcraft_engine::Session::with_demo(), Services { png: None, ..Default::default() });
    let mut problem = crate::panels::library_problem::LibraryProblem::new("", "the last library did not open");
    problem.dismissed = true;
    held.library_problem = Some(problem);

    frame(&mut held);
    assert!(matches!(held.ui.dialog, Some(Dialog::Welcome { page: 0 })), "{:?}", held.ui.dialog);
    assert_eq!(held.ui.seen_version, None, "chosen this frame, presented once the fonts are in");
    frame(&mut held);
    assert_eq!(held.ui.seen_version.as_deref(), Some(env!("CARGO_PKG_VERSION")));
}

/// The browser build's File menu disables Print (`menus::ui_enabled` turns `dialog.print` off on
/// wasm), so the Share page's opening line names Export alone instead of a menu item the host
/// greys out. The desktop keeps both menu paths and the live Print binding.
#[test]
fn the_web_tour_names_export_alone_not_the_desktop_print_menu() {
    use crate::panels::dialogs::welcome_opening;
    assert_eq!(welcome_opening(4, "File › Export… · File › Print…", Some("Ctrl+P"), true), "File › Export…");
    assert_eq!(welcome_opening(4, "File › Export… · File › Print…", Some("Ctrl+P"), false), "File › Export… · File › Print… (Ctrl+P).");
    assert_eq!(welcome_opening(4, "File › Export… · File › Print…", None, false), "File › Export… · File › Print…");
}

fn has(h: &Headless, id: &str) -> bool {
    h.app.widgets.iter().any(|(w, _)| w == id)
}

/// A new library on disk (the welcome's thumbnails are offered over a library, as the samples
/// banner is).
fn library(tag: &str) -> lightcraft_engine::Session {
    let dir = std::env::temp_dir().join(format!("lc-ui-welcome-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let mut s = lightcraft_engine::Session::new().with_fs();
    s.open_library(dir.join("lib"), false).unwrap();
    s
}

fn headless(session: lightcraft_engine::Session, size: [f32; 2]) -> Headless {
    let mut a = LightcraftApp::new(session, Services::default());
    a.ui.view = ViewMode::PhotoGrid;
    a.ui.right = RightPanel::Edit;
    let mut h = Headless::new(a, size, 1.0);
    h.step();
    h.step();
    h
}

/// Steps until a sample thumbnail has arrived from the worker that draws them (the tiles are
/// there from the first frame, as placeholders).
fn thumbnails_shown(h: &mut Headless) -> bool {
    let arrived = |h: &Headless| h.app.welcome_samples.as_ref().is_some_and(|s| s.has_thumbnail());
    h.step_until(T, |h| arrived(h));
    arrived(h) && h.app.widgets.iter().any(|(w, _)| w.starts_with("button:sample:"))
}

#[test]
fn an_empty_library_puts_the_tools_panel_aside_until_photos_arrive() {
    // Given an empty library opened with the Edit panel out
    let mut h = headless(library("aside"), [1300.0, 900.0]);
    // Then the welcome has the window: the panel is put aside, remembered
    assert_eq!(h.app.ui.right, RightPanel::None);
    assert_eq!(h.app.ui.right_before_welcome, Some(RightPanel::Edit));
    // When photos arrive, it is back as it was
    h.app.run("library.addSamples", json!({})).unwrap();
    h.step();
    h.step();
    assert_eq!(h.app.ui.right, RightPanel::Edit);
    assert_eq!(h.app.ui.right_before_welcome, None);
}

#[test]
fn a_panel_opened_while_the_library_is_empty_stays_open() {
    let mut h = headless(library("open"), [1300.0, 900.0]);
    assert_eq!(h.app.ui.right, RightPanel::None);
    h.app.run("panel.info", json!({})).unwrap();
    h.step();
    h.step();
    assert_eq!(h.app.ui.right, RightPanel::Info, "only becoming empty puts it aside");
}

#[test]
fn the_panel_comes_back_after_a_restart_while_the_library_was_empty() {
    // quit while empty: ui.json held the panel put aside; the library has photos by the next launch
    let mut a = LightcraftApp::new(lightcraft_engine::Session::with_demo(), Services::default());
    a.ui.right = RightPanel::None;
    a.ui.right_before_welcome = Some(RightPanel::Info);
    let mut h = Headless::new(a, [1300.0, 900.0], 1.0);
    h.step();
    h.step();
    assert_eq!(h.app.ui.right, RightPanel::Info);
}

#[test]
fn an_editing_tool_put_aside_comes_back_as_the_edit_panel() {
    // the photo it was used on is gone: Masking on a newly arrived photo would be a surprise
    let mut a = LightcraftApp::new(lightcraft_engine::Session::with_demo(), Services::default());
    a.ui.right = RightPanel::None;
    a.ui.right_before_welcome = Some(RightPanel::Masking);
    let mut h = Headless::new(a, [1300.0, 900.0], 1.0);
    h.step();
    h.step();
    assert_eq!(h.app.ui.right, RightPanel::Edit);
}

#[test]
fn a_panel_opened_and_closed_during_the_welcome_stays_closed() {
    let mut h = headless(library("closed"), [1300.0, 900.0]);
    h.app.run("panel.info", json!({})).unwrap();
    h.step();
    h.step();
    h.app.ui.right = RightPanel::None;
    h.step();
    h.app.run("library.addSamples", json!({})).unwrap();
    h.step();
    h.step();
    assert_eq!(h.app.ui.right, RightPanel::None, "closing it was the person's choice");
}

#[test]
fn the_thumbnails_go_with_the_welcome_and_the_window_settles() {
    // photos arriving while the thumbnails are still drawing: nothing keeps waiting for them
    let mut h = headless(library("settle"), [1600.0, 1000.0]);
    assert!(h.app.welcome_samples.is_some());
    h.app.run("library.addSamples", json!({})).unwrap();
    h.step();
    h.step();
    assert!(h.app.welcome_samples.is_none());
    assert!(h.settle(T), "nothing left busy");
}

#[test]
fn the_thumbnails_go_when_another_view_replaces_the_welcome() {
    // the library is still empty, but People (or Compare, or a filter's message) is on screen
    let mut h = headless(library("people"), [1600.0, 1000.0]);
    assert!(h.app.welcome_samples.is_some());
    h.app.ui.view = ViewMode::People;
    h.step();
    h.step();
    assert!(h.app.welcome_samples.is_none());
    assert!(h.settle(T), "nothing left busy");
}

#[test]
fn a_row_out_of_room_for_a_moment_keeps_its_thumbnails() {
    // a resize past the threshold and back: no new worker, no placeholders flashing back
    let mut h = headless(library("resize"), [1600.0, 1000.0]);
    assert!(thumbnails_shown(&mut h));
    assert_eq!(h.request("ui.resize", json!({"width": 1600, "height": 440}), T)["ok"], true);
    h.step();
    h.step();
    assert!(h.app.welcome_samples.as_ref().is_some_and(|s| s.has_thumbnail()), "kept while the row is out of room");
    assert_eq!(h.request("ui.resize", json!({"width": 1600, "height": 1000}), T)["ok"], true);
    h.step();
    assert!(h.app.welcome_samples.as_ref().is_some_and(|s| s.has_thumbnail()));
}

#[test]
fn a_short_view_offers_the_button_instead_of_the_row() {
    // a wide but short window: the welcome's view is too low for the row under the message
    let mut h = headless(library("short"), [1300.0, 440.0]);
    h.step();
    assert!(has(&h, "button:emptySamples"));
    assert!(!h.app.widgets.iter().any(|(w, _)| w.starts_with("button:sample:")));
}

#[test]
fn the_bottom_bar_offers_no_rating_or_flags_with_nothing_to_rate() {
    let mut h = headless(library("bottom"), [1300.0, 900.0]);
    // (the Photo Grid hides the whole bar anyway: People shows it, so it is the view that reaches the check)
    h.app.ui.view = ViewMode::People;
    h.step();
    h.step();
    assert!(!crate::panels::empty_library::in_view(&h.app), "People is not the welcome");
    assert!(!has(&h, "star:bottom:1"));
    assert!(!has(&h, "flag:pick"));
    let h = headless(lightcraft_engine::Session::with_demo(), [1300.0, 900.0]);
    assert!(has(&h, "star:bottom:1"));
}

#[test]
fn the_welcome_shows_sample_photos_that_open_the_one_clicked() {
    // Given the welcome of a new library
    let mut h = headless(library("strip"), [1600.0, 1000.0]);
    assert!(thumbnails_shown(&mut h), "the sample thumbnails render and show");
    assert!(!has(&h, "button:emptySamples"), "the row replaces the button");
    let id = h.app.widgets.iter().find(|(w, _)| w.starts_with("button:sample:")).map(|(w, _)| w.clone()).unwrap();
    let scene: u32 = id.trim_start_matches("button:sample:").parse().unwrap();
    // When one is clicked
    assert_eq!(h.request("ui.clickWidget", json!({"id": id}), T)["ok"], true);
    h.step();
    h.step();
    // Then the samples are in, and that photo is open in Detail, ready to edit
    assert!(h.app.session.has_samples());
    let active = h.app.session.active().and_then(|id| h.app.session.catalog.photo(id).cloned()).unwrap();
    assert!(matches!(active.source, lightcraft_catalog::Source::Demo { scene: s } if s == scene));
    assert_eq!(h.app.ui.view, ViewMode::Detail);
    assert_eq!(h.app.ui.right, RightPanel::Edit, "the panel put aside is back to edit with");
}

#[test]
fn a_column_too_narrow_for_the_row_offers_the_button() {
    // In the docked layout a 540 or 560 point window still leaves the center column 360 wide,
    // enough for three thumbnails, so the rule is checked where it is made. Under three
    // thumbnails wide there is no row (`show` then offers the button, as `a_short_view_offers_the_button_
    // instead_of_the_row` shows for a short view), at three there is one.
    use crate::panels::empty_library::strip_layout;
    assert!(strip_layout(300.0).is_none());
    assert!(strip_layout(307.0).is_none());
    assert_eq!(strip_layout(312.0).map(|(n, _)| n), Some(3));
}

#[test]
fn a_temporary_session_offers_no_samples() {
    // nothing done there is saved: samples would only be lost
    let mut a = LightcraftApp::new(lightcraft_engine::Session::new(), Services::default());
    a.ui.view = ViewMode::PhotoGrid;
    a.library_problem = Some(crate::panels::library_problem::LibraryProblem {
        dismissed: true,
        ..crate::panels::library_problem::LibraryProblem::new("/nowhere", "test")
    });
    let mut h = Headless::new(a, [1300.0, 900.0], 1.0);
    h.step();
    h.step();
    h.step();
    assert!(has(&h, "button:emptyImportPhotos"));
    assert!(!has(&h, "button:emptySamples"));
    assert!(!h.app.widgets.iter().any(|(w, _)| w.starts_with("button:sample:")));
}

#[test]
fn the_welcome_keeps_clear_of_the_grid_header() {
    // a short window: the welcome sits below "All Photos", never over it
    for size in [[1300.0, 440.0], [1300.0, 900.0]] {
        let h = headless(library(&format!("header-{}", size[1])), size);
        let title = h.app.widgets.iter().find(|(w, _)| w == "text:emptyTitle").map(|(_, r)| *r).unwrap();
        // the header is 44 points under the 42-point top bar
        assert!(title.top() >= 42.0 + 44.0, "{size:?}: the title starts at {}", title.top());
    }
}

#[test]
fn the_welcome_keeps_clear_of_the_filter_bar() {
    // the Filter Bar open (`\`) under the header: the welcome sits below it, whatever the height
    for height in (400..=560).step_by(20) {
        let mut h = headless(library(&format!("bar-{height}")), [1300.0, height as f32]);
        h.app.ui.filter_bar = true;
        h.step();
        h.step();
        let title = widget(&h, "text:emptyTitle");
        let bar = widget(&h, "filterbar");
        assert!(title.top() >= bar.bottom(), "{height}: the title starts at {}, the bar ends at {}", title.top(), bar.bottom());
    }
}

fn widget(h: &Headless, id: &str) -> egui::Rect {
    h.app.widgets.iter().find(|(w, _)| w == id).map(|(_, r)| *r).unwrap_or_else(|| panic!("no {id}"))
}

#[test]
fn the_window_steps_back_while_the_welcome_shows() {
    // Given a new library's welcome, with the sidebar out as an empty library opens
    let mut a = LightcraftApp::new(library("quiet"), Services::default());
    a.ui.view = ViewMode::PhotoGrid;
    a.ui.left_panel = true;
    let mut h = Headless::new(a, [1600.0, 1000.0], 1.0);
    h.step();
    h.step();
    // Then nothing around it competes: no "0 photos", no bottom bar, and one line in the sidebar
    // in place of four empty sections
    assert!(has(&h, "grid:title") && !has(&h, "grid:count"));
    assert!(!has(&h, "slider:thumbSize"), "the bottom bar is not drawn");
    assert!(has(&h, "sidebarEmpty:welcome"));
    for section in ["albums", "byDate", "folders", "keywords"] {
        assert!(!has(&h, &format!("sidebarSection:{section}")), "{section} is folded into the line");
    }
    // When photos arrive, the window is back as it was
    h.app.run("library.addSamples", json!({})).unwrap();
    h.app.ui.view = ViewMode::PhotoGrid;
    h.step();
    h.step();
    assert!(has(&h, "grid:count") && has(&h, "slider:thumbSize"));
    assert!(!has(&h, "sidebarEmpty:welcome"));
    assert!(has(&h, "sidebarSection:albums") && has(&h, "sidebarSection:keywords"));
}

#[test]
fn another_view_keeps_the_window_as_it_is() {
    // the welcome isn't what the People view shows: its way out stays
    let mut h = headless(library("people-keeps"), [1600.0, 1000.0]);
    h.app.ui.view = ViewMode::People;
    h.step();
    h.step();
    assert!(!crate::panels::empty_library::in_view(&h.app), "People keeps its bottom bar");
    h.app.ui.view = ViewMode::Detail;
    h.step();
    assert!(crate::panels::empty_library::in_view(&h.app), "Detail with nothing open shows the welcome");
    h.step();
    assert!(has(&h, "text:emptyTitle"));
    assert!(!has(&h, "slider:thumbSize"), "no bottom bar");
    let canvas = h.app.canvas_rect.unwrap();
    assert!(canvas.bottom() >= 999.0, "no empty filmstrip under the welcome: {canvas:?}");
}

#[test]
fn the_welcome_reads_as_a_short_paragraph() {
    // a wide window: the message breaks into even lines rather than running across it
    let h = headless(library("measure"), [1600.0, 1000.0]);
    let title = widget(&h, "text:emptyTitle");
    let body = widget(&h, "text:emptyBody");
    assert!(body.width() <= 540.0, "the body runs {} wide", body.width());
    assert!(body.height() > title.height(), "two lines: {body:?}");
    assert!(body.width() > 360.0, "the lines are even, not one long over one short: {body:?}");
    // still centred under the title
    assert!((body.center().x - title.center().x).abs() < 2.0);
}
