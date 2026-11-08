//! Issue #345: the activity stack — one place for every long-running task, with ✕ to cancel. It lives under the
//! bell in the top bar: a ring and a line beside the bell while something runs, the rows in a list a click opens.

use std::time::Duration;

use lightcraft_engine::activity::{Cancel, Ending, Failure, Outcome, TaskInfo, Unit};
use serde_json::json;

use crate::headless::Headless;
use crate::panels::activity::{WIDTH, count_text, detail_of, failures_text, overall, readout_text, summary_of, took};
use crate::{LightcraftApp, Services};

// generous: merges and renders run on worker threads that a busy machine (a full workspace test run) slows a lot
const T: Duration = Duration::from_secs(60);

fn demo() -> Headless {
    demo_sized([1200.0, 800.0])
}

fn demo_sized(size: [f32; 2]) -> Headless {
    let app = LightcraftApp::new(lightcraft_engine::Session::with_demo(), Services { png: None, ..Default::default() });
    let mut h = Headless::new(app, size, 1.0);
    // let the first renders finish: a render still running when the test binary exits can crash in the
    // GPU driver's teardown (the preview pool doesn't join its workers)
    h.settle(T);
    h
}

fn has(h: &Headless, id: &str) -> bool {
    h.app.widgets.iter().any(|(w, _)| w == id)
}

fn rows(h: &Headless) -> usize {
    h.app.widgets.iter().filter(|(w, _)| w.starts_with("activity:row:")).count()
}

fn rect(h: &Headless, id: &str) -> egui::Rect {
    h.app.widgets.iter().find(|(w, _)| w == id).map(|(_, r)| *r).unwrap_or_else(|| panic!("no widget {id}"))
}

/// The bell shows a task once it is half a second old.
fn wait_visible(h: &mut Headless) {
    std::thread::sleep(Duration::from_millis(600));
    h.step();
    h.step();
}

/// Click a widget and let the synthetic events (move, press, release) play out.
fn click(h: &mut Headless, id: &str) {
    h.step();
    h.step();
    assert_eq!(h.request("ui.clickWidget", json!({"id": id}), T)["ok"], true, "{id}");
    for _ in 0..4 {
        h.step();
    }
}

/// Open the list under the bell (`view.tasks`) and let it lay itself out.
fn open_list(h: &mut Headless) {
    let r = h.request("engine.execute", json!({"command": "view.tasks", "params": {"show": true}}), T);
    assert_eq!(r["result"]["tasks"], true, "{r}");
    for _ in 0..4 {
        h.step();
    }
    assert!(has(h, "activity:list"), "the list is open");
}

/// A task as the registry reports it, for the functions that only word or weigh one.
fn info(label: &str, done: u64, total: u64) -> TaskInfo {
    TaskInfo {
        id: 1,
        kind: "export",
        label: label.into(),
        done,
        total,
        unit: Unit::Count,
        detail: String::new(),
        cancellable: true,
        cancelling: false,
        age_ms: 600,
        waiting: false,
        place: 0,
        lane: None,
        facts: String::new(),
        stalled: None,
    }
}

#[test]
fn a_task_rings_the_bell_after_half_a_second_and_its_row_is_in_the_list_under_it() {
    // Given a task that has just started
    let mut h = demo();
    let g = h.app.session.activity.start("export", "Exporting", Cancel::Yes);
    g.progress(2, 5);
    h.step();
    // (a frame on a loaded machine can outlast the half second: only judge a task still young after it)
    if h.app.session.activity.list()[0].age_ms < crate::panels::activity::SHOW_AFTER_MS {
        assert!(!has(&h, "activity:ring"), "not before 0.5 s");
    }
    // When it is half a second old
    wait_visible(&mut h);
    // Then the bell wears a ring, and nothing is drawn over the panels
    assert!(has(&h, "activity:ring"));
    assert_eq!(rows(&h), 0, "the list is closed until the bell is clicked");
    assert_eq!(h.request("ui.inspect", json!({}), T)["result"]["activityOpen"], false);
    // When the bell is clicked
    click(&mut h, "icon:bell");
    // Then the list is under the top bar, inside the window, with the task's row
    let (list, bell) = (rect(&h, "activity:list"), rect(&h, "icon:bell"));
    assert!(list.top() >= bell.bottom() && list.right() <= 1200.0 && list.left() >= 0.0, "{list:?} under {bell:?}");
    assert!(list.contains_rect(rect(&h, &format!("activity:row:{}", g.id()))));
    assert_eq!(h.request("ui.inspect", json!({}), T)["result"]["activityOpen"], true);
    // And its ✕ asks the task to stop, with the list still open
    click(&mut h, &format!("activity:cancel:{}", g.id()));
    assert!(g.is_cancelled());
    let inspect = h.request("ui.inspect", json!({}), T);
    assert_eq!(inspect["result"]["activity"][0]["cancelling"], true, "{}", inspect["result"]["activity"]);
    assert!(has(&h, "activity:list"), "a click inside the list keeps it open");
}

#[test]
fn the_bell_opens_the_tasks_not_history_and_says_when_nothing_runs() {
    let mut h = demo();
    let right = h.app.ui.right;
    assert!(!has(&h, "activity:ring"), "no ring while nothing runs");
    click(&mut h, "icon:bell");
    assert!(has(&h, "activity:empty"), "the list says that nothing runs");
    assert_eq!(h.app.ui.right, right, "the History panel is not opened by the bell");
    // a second click closes it
    click(&mut h, "icon:bell");
    assert!(!has(&h, "activity:list"));
}

#[test]
fn many_tasks_scroll_inside_the_window_newest_first() {
    let mut h = demo();
    let guards: Vec<_> = (0..30).map(|i| h.app.session.activity.start("export", &format!("Task {i}"), Cancel::Yes)).collect();
    wait_visible(&mut h);
    open_list(&mut h);
    let list = h.view.ctx.memory(|m| m.area_rect(crate::panels::activity::popup_id())).expect("the list is drawn");
    assert!(list.bottom() <= 800.0 && list.top() >= 0.0, "the list keeps to the window and scrolls: {list:?}");
    // the task started last is at the top, the one after it below; the oldest is scrolled out of view, and a row
    // out of view offers nothing to click
    let id = |i: usize| guards[i].id();
    let (first, second) = (rect(&h, &format!("activity:row:{}", id(29))), rect(&h, &format!("activity:row:{}", id(28))));
    assert!(list.contains_rect(first) && first.bottom() <= second.top(), "{first:?} above {second:?} in {list:?}");
    assert!(rows(&h) > 3, "more rows than the three the old stack showed: {}", rows(&h));
    assert!(rows(&h) < 30 && !has(&h, &format!("activity:cancel:{}", id(0))), "rows out of view are not offered: {}", rows(&h));
    drop(guards);
    h.step();
    h.step();
    assert_eq!(rows(&h), 0);
    assert!(!has(&h, "activity:ring"), "the ring goes with the last task");
}

#[test]
fn not_cancellable_rows_have_no_cross() {
    let mut h = demo();
    let g = h.app.session.activity.start("applePhotos", "Adding to Apple Photos", Cancel::No);
    wait_visible(&mut h);
    open_list(&mut h);
    assert!(has(&h, &format!("activity:row:{}", g.id())));
    assert!(!has(&h, &format!("activity:cancel:{}", g.id())));
}

#[test]
fn count_text_by_unit() {
    let mut t = info("Exporting", 3, 25);
    assert_eq!(count_text(&t), "3 of 25");
    (t.done, t.total, t.unit) = (12 * 1_048_576, 340 * 1_048_576, Unit::Bytes);
    assert_eq!(count_text(&t), "12 of 340 MB");
    (t.done, t.total, t.unit) = (400, 1000, Unit::Percent);
    assert_eq!(count_text(&t), "40 %");
    t.total = 0;
    assert_eq!(count_text(&t), "");
}

#[test]
fn the_line_beside_the_bell_names_one_task_or_counts_several() {
    assert_eq!(readout_text(&[], 0), "");
    assert_eq!(readout_text(&[info("Exporting", 10, 25)], 0), "Exporting · 40 %");
    assert_eq!(readout_text(&[info("Finding faces", 0, 0)], 0), "Finding faces", "nothing to say about how far while that isn't known");
    let stopping = TaskInfo { cancelling: true, ..info("Exporting", 10, 25) };
    assert_eq!(readout_text(&[stopping], 0), "Exporting · Stopping…");
    assert_eq!(readout_text(&[info("Exporting", 1, 2), info("Importing", 0, 0), info("Merging", 3, 4)], 0), "3 tasks");
    // those waiting in line are counted after what runs, and alone in the moment before the next one starts
    assert_eq!(readout_text(&[info("Building previews", 1, 4)], 2), "Building previews · 25 % · 2 waiting");
    assert_eq!(readout_text(&[info("Finding faces", 0, 0)], 1), "Finding faces · 1 waiting");
    assert_eq!(readout_text(&[], 1), "1 waiting");
}

#[test]
fn a_long_name_is_cut_short_before_the_percentage_is() {
    // Given a wide window and a task whose name alone is longer than the line beside the bell may be
    let mut h = demo_sized([1600.0, 900.0]);
    let long = h.app.session.activity.start("lightroom", "Reading the Lightroom catalog of the whole archive", Cancel::Yes);
    long.progress(2, 5);
    wait_visible(&mut h);
    let wide = rect(&h, "activity:readout");
    drop(long);
    h.step();
    // When a task with a short name is as far along
    let short = h.app.session.activity.start("export", "Exporting", Cancel::Yes);
    short.progress(2, 5);
    wait_visible(&mut h);
    let narrow = rect(&h, "activity:readout");
    // Then the long one took more room but kept to the line's limit: its name gave way, the line is still there
    assert!(wide.width() > narrow.width() && wide.width() <= 221.0, "{wide:?} / {narrow:?}");
    // And as the window narrows, the line never grows; before it goes there is a width at which only the
    // percentage is left (narrower than any name is allowed to be), and once gone it stays gone
    let long = h.app.session.activity.start("lightroom", "Reading the Lightroom catalog of the whole archive", Cancel::Yes);
    drop(short);
    long.progress(2, 5);
    wait_visible(&mut h);
    let widths: Vec<Option<f32>> = (640..=1600).rev().step_by(20).map(|w| readout_width_at(&mut h, w as f32)).collect();
    for pair in widths.windows(2) {
        let (wider, narrower) = (pair[0].unwrap_or(0.0), pair[1].unwrap_or(0.0));
        assert!(narrower <= wider + 0.5, "the line grew as the window narrowed: {widths:?}");
    }
    assert!(widths.iter().flatten().any(|w| *w < 56.0), "at some width only the percentage is left: {widths:?}");
    assert_eq!(widths.last(), Some(&None), "a narrow window has the ring alone: {widths:?}");
}

/// The width of the line beside the bell in a window `width` wide (`None` when it is left out).
fn readout_width_at(h: &mut Headless, width: f32) -> Option<f32> {
    assert_eq!(h.request("ui.resize", json!({"width": width, "height": 800.0}), T)["ok"], true);
    for _ in 0..3 {
        h.step();
    }
    h.app.widgets.iter().find(|(w, _)| w == "activity:readout").map(|(_, r)| r.width())
}

#[test]
fn the_ring_shows_the_mean_of_the_tasks_whose_work_is_known() {
    assert_eq!(overall(&[]), None);
    assert_eq!(overall(&[info("Finding faces", 0, 0)]), None, "the ring sweeps while no amount is known");
    assert_eq!(overall(&[info("Exporting", 1, 4)]), Some(0.25));
    assert_eq!(overall(&[info("Exporting", 1, 4), info("Importing", 3, 4), info("Finding faces", 0, 0)]), Some(0.5));
    assert_eq!(overall(&[info("Exporting", 9, 4)]), Some(1.0), "never past the whole ring");
}

#[test]
fn the_bar_names_the_task_where_it_has_room_and_counts_on_the_bell_where_it_has_not() {
    // Given a wide window and one task
    let mut h = demo_sized([1600.0, 900.0]);
    let a = h.app.session.activity.start("export", "Exporting", Cancel::Yes);
    a.progress(2, 5);
    wait_visible(&mut h);
    // Then a line beside the bell names it, between the filter icon and the bell
    let (line, bell, filter) = (rect(&h, "activity:readout"), rect(&h, "icon:bell"), rect(&h, "icon:filter"));
    assert!(line.right() <= bell.left() && line.left() >= filter.right(), "{line:?} between {filter:?} and {bell:?}");
    assert!(!has(&h, "activity:count"), "one task needs no count");
    // And a click on the line opens the list, as one on the bell does
    click(&mut h, "activity:readout");
    assert!(has(&h, "activity:list"));
    // Given a window with no room beside the bell, and two tasks
    let mut h = demo();
    let _a = h.app.session.activity.start("export", "Exporting", Cancel::Yes);
    let _b = h.app.session.activity.start("import", "Importing", Cancel::Yes);
    wait_visible(&mut h);
    assert_eq!(readout_width_at(&mut h, 640.0), None);
    // Then the line is left out, the ring stays, and the bell says how many
    assert!(!has(&h, "activity:readout"));
    assert!(has(&h, "activity:ring") && has(&h, "activity:count"));
    // And with one task left, the ring alone: there is nothing to count
    drop(_b);
    h.step();
    h.step();
    assert!(has(&h, "activity:ring") && !has(&h, "activity:count") && !has(&h, "activity:readout"));
}

#[test]
fn escape_closes_the_list_and_nothing_else() {
    let mut h = demo();
    h.request("engine.execute", json!({"command": "view.detail", "params": {}}), T);
    h.step();
    assert_eq!(h.app.ui.view, crate::state::ViewMode::Detail);
    open_list(&mut h);
    // Esc closes the list; the command Esc has otherwise (Back to Grid) waits for the next press
    assert_eq!(h.request("ui.key", json!({"key": "Escape"}), T)["ok"], true);
    for _ in 0..4 {
        h.step();
    }
    assert!(!has(&h, "activity:list") && !h.app.activity_open, "Esc closes the list");
    assert_eq!(h.app.ui.view, crate::state::ViewMode::Detail, "and does nothing else");
}

#[test]
fn a_click_elsewhere_or_on_the_bell_again_closes_the_list() {
    let mut h = demo_sized([1600.0, 900.0]);
    let g = h.app.session.activity.start("export", "Exporting", Cancel::Yes);
    g.progress(2, 5);
    wait_visible(&mut h);
    // open by the line beside the bell, closed by a second click on it
    click(&mut h, "activity:readout");
    assert!(has(&h, "activity:list"));
    click(&mut h, "activity:readout");
    assert!(!has(&h, "activity:list"), "the line toggles the list, as the bell does");
    // open again, closed by a click on something else
    click(&mut h, "icon:bell");
    assert!(has(&h, "activity:list"));
    click(&mut h, "icon:filter");
    assert!(!has(&h, "activity:list") && !h.app.activity_open, "a click elsewhere closes it");
    assert!(!g.is_cancelled(), "closing the list stops nothing");
}

#[test]
fn the_list_gives_way_to_a_dialog_and_view_tasks_says_why_it_cannot_open() {
    let mut h = demo();
    open_list(&mut h);
    h.request("engine.execute", json!({"command": "app.shortcuts", "params": {}}), T);
    for _ in 0..4 {
        h.step();
    }
    assert!(h.app.ui.dialog.is_some(), "a dialog is open");
    assert!(!has(&h, "activity:list") && !h.app.activity_open, "the list gave way to the dialog");
    // asked for over the dialog, it is refused with the reason, and nothing opens behind the dialog or after it
    let r = h.request("engine.execute", json!({"command": "view.tasks", "params": {"show": true}}), T);
    assert_eq!(r["ok"], false, "{r}");
    assert!(r["error"].as_str().is_some_and(|e| e.contains("a dialog is open")), "{r}");
    h.app.ui.dialog = None;
    for _ in 0..4 {
        h.step();
    }
    assert!(!has(&h, "activity:list"), "the refused request is not kept for later");
}

#[test]
fn view_tasks_is_refused_while_the_blocking_library_window_is_up() {
    let mut h = demo();
    open_list(&mut h);
    h.app.library_problem = Some(crate::panels::library_problem::LibraryProblem::new("/gone", "not found"));
    for _ in 0..4 {
        h.step();
    }
    assert!(crate::panels::library_problem::blocks_dialog_input(&h.app), "the library window is on screen");
    assert!(!has(&h, "activity:list") && !h.app.activity_open, "the list gave way to it");
    let r = h.request("engine.execute", json!({"command": "view.tasks", "params": {"show": true}}), T);
    assert_eq!(r["ok"], false, "{r}");
    assert!(r["error"].as_str().is_some_and(|e| e.contains("library window")), "{r}");
}

#[test]
fn the_list_gives_way_to_the_question_before_quitting() {
    let mut h = demo();
    let _g = h.app.session.activity.start("export", "Exporting", Cancel::Yes);
    wait_visible(&mut h);
    open_list(&mut h);
    assert!(!crate::panels::notices::may_close(&mut h.app));
    for _ in 0..4 {
        h.step();
    }
    assert!(has(&h, "button:quitCancel"), "the question is asked");
    assert!(!has(&h, "activity:list") && !h.app.activity_open, "and the list is not left open over it");
    let r = h.request("engine.execute", json!({"command": "view.tasks", "params": {"show": true}}), T);
    assert_eq!(r["ok"], false, "{r}");
}

#[test]
fn full_screen_preview_closes_the_list_and_keeps_no_request_for_later() {
    let mut h = demo();
    h.request("engine.execute", json!({"command": "view.detail", "params": {}}), T);
    open_list(&mut h);
    // Given the list open, When the photo goes full screen (no top bar)
    h.app.ui.fullscreen = true;
    for _ in 0..3 {
        h.step();
    }
    // Then the list is closed, and said to be
    assert!(!has(&h, "activity:list") && !h.app.activity_open);
    assert_eq!(h.request("ui.inspect", json!({}), T)["result"]["activityOpen"], false);
    // And asking for it there is refused with the reason, not kept until full screen ends
    let r = h.request("engine.execute", json!({"command": "view.tasks", "params": {"show": true}}), T);
    assert_eq!(r["ok"], false, "{r}");
    assert!(r["error"].as_str().is_some_and(|e| e.contains("Full Screen Preview")), "{r}");
    h.app.ui.fullscreen = false;
    for _ in 0..4 {
        h.step();
    }
    assert!(!has(&h, "activity:list"), "nothing pops open on the way back");
}

#[test]
fn a_task_just_started_has_its_row_in_an_open_list_at_once() {
    let mut h = demo();
    open_list(&mut h);
    assert!(has(&h, "activity:empty"));
    // the bell waits half a second so quick tasks don't flash; a list that is open never says "none" over a task
    let g = h.app.session.activity.start("export", "Exporting", Cancel::Yes);
    h.step();
    h.step();
    assert!(has(&h, &format!("activity:row:{}", g.id())) && !has(&h, "activity:empty"));
}

#[test]
fn view_tasks_opens_and_closes_the_list() {
    let mut h = demo();
    open_list(&mut h);
    // without `show` it toggles
    let r = h.request("engine.execute", json!({"command": "view.tasks", "params": {}}), T);
    assert_eq!(r["result"]["tasks"], false, "{r}");
    for _ in 0..4 {
        h.step();
    }
    assert!(!has(&h, "activity:list"));
    let r = h.request("engine.execute", json!({"command": "view.tasks", "params": {}}), T);
    assert_eq!(r["result"]["tasks"], true, "{r}");
    for _ in 0..4 {
        h.step();
    }
    assert!(has(&h, "activity:list"));
}

#[test]
fn two_view_tasks_toggles_in_one_frame_end_where_they_began() {
    let mut h = demo();
    open_list(&mut h);
    let first = crate::menus::run_ui_command(&mut h.app, "view.tasks", &json!({})).unwrap().unwrap();
    let second = crate::menus::run_ui_command(&mut h.app, "view.tasks", &json!({})).unwrap().unwrap();
    assert_eq!(first["tasks"], false);
    assert_eq!(second["tasks"], true);
    for _ in 0..4 {
        h.step();
    }
    assert!(has(&h, "activity:list"));
}

#[test]
fn stale_cancel_is_harmless() {
    let mut h = demo();
    let g = h.app.session.activity.start("export", "Exporting", Cancel::Yes);
    wait_visible(&mut h);
    let id = g.id();
    drop(g);
    let r = h.request("engine.execute", json!({"command": "activity.cancel", "params": {"id": id}}), T);
    assert_eq!(r["ok"], false, "an error result, not a panic: {r}");
    h.step();
    assert!(h.app.ui.toast().is_none(), "{:?}", h.app.ui.toast);
    // the same through the row's ✕ (clicked in the frame the task ended): no message in the status bar either
    h.app.ui.status = "Ready".into();
    crate::panels::activity::cancel(&mut h.app, id);
    h.step();
    assert_eq!(h.app.ui.status, "Ready");
    assert!(h.app.ui.toast().is_none(), "{:?}", h.app.ui.toast);
}

#[test]
fn long_detail_stays_inside_the_list() {
    let mut h = demo();
    let g = h.app.session.activity.start("export", "Exportieren", Cancel::Yes);
    g.detail(&"IMG_".repeat(75));
    wait_visible(&mut h);
    open_list(&mut h);
    let row = rect(&h, &format!("activity:row:{}", g.id()));
    assert!(row.width() <= WIDTH + 1.0, "{row:?}");
}

#[test]
fn cross_sits_at_the_right_edge_of_its_row() {
    let mut h = demo();
    let g = h.app.session.activity.start("export", "Exporting", Cancel::Yes);
    wait_visible(&mut h);
    open_list(&mut h);
    let row = rect(&h, &format!("activity:row:{}", g.id()));
    let cross = rect(&h, &format!("activity:cancel:{}", g.id()));
    assert!(cross.right() >= row.right() - 2.0, "the cross is at the right, not after the label: {cross:?} in {row:?}");
}

#[test]
fn quit_with_a_running_task_asks_and_quit_anyway_cancels() {
    let mut h = demo();
    let g = h.app.session.activity.start("export", "Exporting", Cancel::Yes);
    wait_visible(&mut h);
    assert!(!crate::panels::notices::may_close(&mut h.app));
    assert!(matches!(h.app.quit_prompt, Some(crate::QuitPrompt::Tasks(_))), "{:?}", h.app.quit_prompt);
    h.step();
    assert!(!has(&h, "button:quitRetry"));
    click(&mut h, "button:quitAnyway");
    assert!(g.is_cancelled() && h.quit_requested());
}

/// Files waiting for a running import or Synchronize Folder (issue #2779) make quitting ask, and
/// Quit Anyway gives them up instead of asking about them again and again.
#[test]
fn quit_anyway_gives_up_files_waiting_to_be_imported() {
    let dir = std::env::temp_dir().join(format!("lightcraft-quit-waiting-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    for i in 0..1000u32 {
        let img = lightcraft_raster::Rgba8 { width: 8, height: 8, data: vec![[(i % 251) as u8, (i / 251) as u8, 9, 255]; 64] };
        let png = lightcraft_codecs::encode_png(&lightcraft_codecs::EncodeImage::rgba8(&img), &lightcraft_codecs::EncodeMeta::default()).unwrap();
        std::fs::write(dir.join(format!("{i}.png")), png).unwrap();
    }
    let mut h = demo();
    // the file waits behind an import that is still running
    crate::import::start_paths(&mut h.app, vec![dir.to_string_lossy().to_string()]).unwrap();
    crate::import::start_paths(&mut h.app, vec![dir.join("0.png").to_string_lossy().to_string()]).unwrap();
    wait_visible(&mut h);
    assert!(h.app.import.is_some() && !h.app.session.activity.waiting().is_empty(), "still waiting");
    assert!(!crate::panels::notices::may_close(&mut h.app));
    assert!(matches!(h.app.quit_prompt, Some(crate::QuitPrompt::Tasks(_))), "{:?}", h.app.quit_prompt);
    h.step();
    assert!(h.app.import.is_some() && !h.app.session.activity.waiting().is_empty(), "still waiting at the click");
    click(&mut h, "button:quitAnyway");
    assert!(h.app.quit_prompt.is_none(), "not asked again: {:?}", h.app.quit_prompt);
    assert!(h.quit_requested());
    assert!(h.app.session.activity.waiting().is_empty());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn quit_cancel_keeps_the_task_running() {
    let mut h = demo();
    let g = h.app.session.activity.start("import", "Importing", Cancel::Yes);
    assert!(!crate::panels::notices::may_close(&mut h.app));
    click(&mut h, "button:quitCancel");
    assert!(!g.is_cancelled() && h.app.quit_prompt.is_none() && !h.app.quit_confirmed);
}

#[test]
fn cancelling_task_does_not_block_quit() {
    let mut h = demo();
    let g = h.app.session.activity.start("export", "Exporting", Cancel::Yes);
    h.app.session.activity.cancel(g.id()).unwrap();
    assert!(crate::panels::notices::may_close(&mut h.app));
}

#[test]
fn quit_names_a_running_job_that_cannot_be_stopped() {
    let mut h = demo();
    let _f = h.app.session.activity.start("applePhotos", "Adding to Apple Photos", Cancel::No);
    assert!(!crate::panels::notices::may_close(&mut h.app), "quitting would leave the job running unannounced");
    let Some(crate::QuitPrompt::Tasks(text)) = h.app.quit_prompt.clone() else { panic!("{:?}", h.app.quit_prompt) };
    let unstoppable = crate::i18n::tr_format!(
        "Still running and can't be stopped: {names}. Quitting now doesn't wait for them.",
        names = crate::i18n::tr("Adding to Apple Photos")
    );
    assert_eq!(text, unstoppable);
}

#[test]
fn a_quick_read_only_check_does_not_make_quit_ask() {
    let mut h = demo();
    let _check = h.app.session.activity.start("findMissing", "Find Missing Photos", Cancel::Quick);
    assert!(crate::panels::notices::may_close(&mut h.app), "nothing the user made is lost by quitting during a check");
    assert!(h.app.quit_prompt.is_none());
}

#[test]
fn quit_with_both_kinds_says_which_stop_and_which_are_not_waited_for_and_quit_anyway_does_not_ask_twice() {
    let png = |img: &lightcraft_raster::Rgba8| {
        lightcraft_codecs::encode_png(&lightcraft_codecs::EncodeImage::rgba8(img), &lightcraft_codecs::EncodeMeta::default()).unwrap_or_default()
    };
    let write = |p: &str, b: &[u8]| std::fs::write(p, b).map_err(|e| e.to_string());
    let app = LightcraftApp::new(
        lightcraft_engine::Session::with_demo(),
        Services { png: Some(Box::new(png)), write_new: Some(Box::new(write)), ..Default::default() },
    );
    let mut h = Headless::new(app, [1200.0, 800.0], 1.0);
    h.settle(T);
    let stoppable = h.app.session.activity.start("faces", "Finding faces", Cancel::Yes);
    let _unstoppable = h.app.session.activity.start("applePhotos", "Adding to Apple Photos", Cancel::No);
    wait_visible(&mut h);
    assert!(!crate::panels::notices::may_close(&mut h.app));
    let Some(crate::QuitPrompt::Tasks(text)) = h.app.quit_prompt.clone() else { panic!("{:?}", h.app.quit_prompt) };
    let stops = crate::i18n::tr_format!("1 task is still running: {names}. Quitting stops it.", names = crate::i18n::tr("Finding faces"));
    let unstoppable = crate::i18n::tr_format!(
        "Still running and can't be stopped: {names}. Quitting now doesn't wait for them.",
        names = crate::i18n::tr("Adding to Apple Photos")
    );
    assert_eq!(text, format!("{stops}\n{unstoppable}"));
    h.step();
    if let Some(p) = std::env::var_os("LIGHTCRAFT_TEST_SHOTS") {
        let r = h.request("ui.screenshot", json!({"path": std::path::Path::new(&p).join("quit-prompt.png").to_string_lossy(), "headless": true}), T);
        assert_eq!(r["ok"], true, "{r}");
    }
    click(&mut h, "button:quitAnyway");
    assert!(stoppable.is_cancelled(), "the one that can stop is asked to");
    assert!(h.quit_requested(), "Quit Anyway ends the app");
    // the window's close request is checked again (frame.rs); the job that can't be stopped must not bring the prompt back
    assert!(crate::panels::notices::may_close(&mut h.app), "the job that can't be stopped brings the prompt back, so the app never quits");
}

/// A slow disk: every file takes `ms` to write (the UI must not wait for it).
fn slow_writer(ms: u64) -> crate::SharedWrite {
    std::sync::Arc::new(move |_: &str, _: &[u8]| {
        std::thread::sleep(Duration::from_millis(ms));
        Ok(())
    })
}

/// Export the first `n` photos in view through the Export dialog (small JPEGs, a made-up folder).
fn start_export(h: &mut Headless, n: usize) {
    let ids: Vec<u64> = h.app.session.visible_cloned().iter().take(n).map(|p| p.0).collect();
    assert_eq!(ids.len(), n, "the demo library has {n} photos");
    h.request("engine.execute", json!({"command": "library.select", "params": {"ids": ids}}), T);
    h.request("engine.execute", json!({"command": "dialog.export", "params": {}}), T);
    h.step();
    if let Some(crate::state::Dialog::Export { full_size, resize, dir, .. }) = &mut h.app.ui.dialog {
        *full_size = false;
        *resize = lightcraft_engine::export::Resize::long_edge(64);
        *dir = "/lc-test-out".into();
    }
    let r = h.request("ui.dialog.confirm", json!({}), T);
    assert_eq!(r["ok"], true, "{r}");
}

#[test]
fn export_shows_a_row_and_stops_on_activity_cancel() {
    let mut h = demo();
    h.app.services.write_shared = Some(slow_writer(200));
    start_export(&mut h, 5);
    assert!(h.app.export.is_some(), "running in the background");
    let tasks = h.request("ui.inspect", json!({}), T)["result"]["activity"].clone();
    assert_eq!(tasks[0]["kind"], "export", "{tasks}");
    assert_eq!(tasks[0]["total"], 5, "{tasks}");
    let id = tasks[0]["id"].as_u64().unwrap();
    assert_eq!(h.request("engine.execute", json!({"command": "activity.cancel", "params": {"id": id}}), T)["ok"], true);
    assert!(h.step_until(Duration::from_secs(60), |h| h.app.export.is_none()));
    assert!(h.app.session.activity.list().is_empty());
    assert!(h.app.last_export_result.as_ref().is_some_and(|r| r["cancelled"] == true), "{:?}", h.app.last_export_result);
    assert!(!has(&h, "button:exportCancel"), "the old window is gone");
}

#[test]
fn dead_export_worker_leaves_no_running_row() {
    let mut h = demo();
    h.app.services.write_shared = Some(std::sync::Arc::new(|_: &str, _: &[u8]| -> Result<(), String> { panic!("synthetic writer panic") }));
    start_export(&mut h, 2);
    assert!(h.step_until(Duration::from_secs(60), |h| h.app.export.is_none()));
    assert!(h.app.session.activity.list().is_empty());
}

/// A writer that records every path it is given (and takes `ms` per file).
fn recording_writer(ms: u64) -> (crate::SharedWrite, std::sync::Arc<std::sync::Mutex<Vec<String>>>) {
    let written = std::sync::Arc::new(std::sync::Mutex::new(Vec::<String>::new()));
    let w = written.clone();
    let write: crate::SharedWrite = std::sync::Arc::new(move |p: &str, _: &[u8]| {
        std::thread::sleep(Duration::from_millis(ms));
        w.lock().unwrap().push(p.to_string());
        Ok(())
    });
    (write, written)
}

/// Starting an export answers at once, with its row in the stack and ✕: the photos are prepared over the
/// frames that follow (a few milliseconds each, on the UI thread), not before the answer. Then the batch
/// runs as ever.
#[test]
fn export_row_is_there_before_its_photos_are_prepared() {
    let mut h = demo();
    crate::export_task::set_prep_budget(Duration::ZERO);
    let (write, written) = recording_writer(1);
    h.app.services.write_shared = Some(write);
    start_export(&mut h, 5);
    let seen = h.request("ui.inspect", json!({}), T)["result"].clone();
    assert_eq!(seen["export"]["running"]["preparing"], true, "{}", seen["export"]);
    assert_eq!(seen["export"]["running"]["total"], 5, "{}", seen["export"]);
    assert_eq!(seen["activity"][0]["kind"], "export", "{}", seen["activity"]);
    assert_eq!(seen["activity"][0]["cancellable"], true, "{}", seen["activity"]);
    assert!(written.lock().unwrap().is_empty(), "nothing is written before the photos are prepared");
    assert!(h.step_until(Duration::from_secs(60), |h| h.app.export.is_none()));
    assert_eq!(written.lock().unwrap().len(), 5, "{:?}", written.lock().unwrap());
    let last = h.app.last_export_result.clone().expect("the export reports");
    assert_eq!(last["files"].as_array().map(Vec::len), Some(5), "{last}");
    assert!(h.app.session.activity.list().is_empty(), "the row is gone");
}

/// The ✕ works while the photos are being prepared: nothing is prepared further or written, and the export
/// ends as a cancelled one does.
#[test]
fn export_can_be_stopped_while_its_photos_are_prepared() {
    let mut h = demo();
    crate::export_task::set_prep_budget(Duration::ZERO);
    let (write, written) = recording_writer(1);
    h.app.services.write_shared = Some(write);
    start_export(&mut h, 5);
    let seen = h.request("ui.inspect", json!({}), T)["result"].clone();
    assert_eq!(seen["export"]["running"]["preparing"], true, "{}", seen["export"]);
    let id = seen["activity"][0]["id"].as_u64().unwrap();
    assert_eq!(h.request("engine.execute", json!({"command": "activity.cancel", "params": {"id": id}}), T)["ok"], true);
    assert!(h.step_until(Duration::from_secs(60), |h| h.app.export.is_none()));
    assert!(written.lock().unwrap().is_empty(), "{:?}", written.lock().unwrap());
    assert!(h.app.last_export_result.as_ref().is_some_and(|r| r["cancelled"] == true), "{:?}", h.app.last_export_result);
    assert!(h.app.session.activity.list().is_empty());
}

/// A photo that can't be exported (here: a demo photo has no original file to copy) used to be refused by
/// `app.export` before anything started; now the export starts and ends with that error in a toast, with
/// nothing written, no running row, and a failed row in the finished ones that says why.
#[test]
fn a_photo_that_cannot_be_prepared_ends_the_export_with_its_error() {
    let mut h = demo();
    crate::export_task::set_prep_budget(Duration::ZERO);
    let (write, written) = recording_writer(1);
    h.app.services.write_shared = Some(write);
    let ids: Vec<u64> = h.app.session.visible_cloned().iter().take(3).map(|p| p.0).collect();
    let params = json!({"ids": ids, "dir": "/lc-test-out", "format": "original", "background": true});
    let r = h.request("engine.execute", json!({"command": "app.export", "params": params}), T);
    assert_eq!(r["ok"], true, "{r}");
    assert!(h.step_until(Duration::from_secs(60), |h| h.app.export.is_none()));
    let toast = h.app.ui.toast.clone().map(|t| t.text).unwrap_or_default();
    assert!(toast.contains("generated demo photo"), "{toast}");
    assert!(written.lock().unwrap().is_empty(), "nothing is written");
    assert!(h.app.session.activity.list().is_empty(), "no row is left running");
    let finished = h.app.session.activity.finished();
    let row = finished.iter().find(|r| r.kind == "export").expect("the export leaves a row");
    assert_eq!(row.ending, lightcraft_engine::activity::Ending::Failed, "{row:?}");
    assert!(row.detail.contains("generated demo photo"), "{row:?}");
}

fn preparing(h: &Headless) -> bool {
    h.app.export.as_ref().is_some_and(|e| e.status()["preparing"] == true)
}

/// The photos are set up whatever the window shows: a full-screen preview (which skips the panels' pass)
/// doesn't hold the export at "Preparing photos…".
#[test]
fn an_exports_photos_are_prepared_in_a_full_screen_preview() {
    let mut h = demo();
    crate::export_task::set_prep_budget(Duration::ZERO);
    let (write, written) = recording_writer(1);
    h.app.services.write_shared = Some(write);
    start_export(&mut h, 5);
    assert!(preparing(&h));
    h.app.ui.fullscreen = true;
    assert!(h.step_until(Duration::from_secs(60), |h| !preparing(h)), "still preparing in full screen");
    h.app.ui.fullscreen = false;
    assert!(h.step_until(Duration::from_secs(60), |h| h.app.export.is_none()));
    assert_eq!(written.lock().unwrap().len(), 5, "{:?}", written.lock().unwrap());
}

/// Photo ids belong to one library: when another is opened while the photos are being prepared, the export
/// ends as a cancelled one does, and nothing is exported from the new library.
#[test]
fn switching_library_while_an_export_prepares_stops_it() {
    let mut h = demo();
    crate::export_task::set_prep_budget(Duration::ZERO);
    let (write, written) = recording_writer(1);
    h.app.services.write_shared = Some(write);
    start_export(&mut h, 5);
    assert!(preparing(&h));
    let dir = std::env::temp_dir().join(format!("lc-export-switch-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    h.app.session.open_library(&dir, true).unwrap();
    assert!(h.step_until(Duration::from_secs(60), |h| h.app.export.is_none()));
    assert!(written.lock().unwrap().is_empty(), "{:?}", written.lock().unwrap());
    assert!(h.app.last_export_result.as_ref().is_some_and(|r| r["cancelled"] == true), "{:?}", h.app.last_export_result);
    let _ = std::fs::remove_dir_all(&dir);
}

/// A one-shot headless run (`lightcraft-cli snapshot`) that ends, or waits for Apple Photos, while an export
/// is still setting its photos up would leave Photos' slot held by an export nothing is working on: it
/// first lets the photos get prepared and the worker start.
#[test]
fn a_headless_run_lets_an_exports_photos_get_prepared_before_it_ends() {
    let mut h = demo();
    crate::export_task::set_prep_budget(Duration::ZERO);
    let (write, _written) = recording_writer(1);
    h.app.services.write_shared = Some(write);
    start_export(&mut h, 5);
    assert!(preparing(&h));
    h.finish_preparing();
    assert!(!preparing(&h), "the worker has not started");
    assert!(h.step_until(Duration::from_secs(60), |h| h.app.export.is_none()));
}

/// The settings of the export before the one a test starts (what Export with Previous would use).
fn previous_export() -> serde_json::Value {
    json!({"format": "jpeg", "quality": 77, "dir": "/lc-previous-out"})
}

/// What Export with Previous and the Export dialog remember is the last export that got going: a background
/// export's settings are kept once its photos are prepared and the worker starts, not when it is asked for.
#[test]
fn an_export_is_remembered_once_its_photos_are_prepared() {
    let mut h = demo();
    crate::export_task::set_prep_budget(Duration::ZERO);
    let (write, _written) = recording_writer(1);
    h.app.services.write_shared = Some(write);
    h.app.session.last_export = Some(previous_export());
    start_export(&mut h, 5);
    assert!(preparing(&h));
    assert_eq!(h.app.session.last_export, Some(previous_export()), "not yet: the photos are not prepared");
    assert!(h.step_until(Duration::from_secs(60), |h| !preparing(h)), "the photos get prepared");
    let last = h.app.session.last_export.clone().expect("remembered");
    assert_eq!(last["dir"], "/lc-test-out", "{last}");
    assert!(h.step_until(Duration::from_secs(60), |h| h.app.export.is_none()));
    assert_eq!(h.app.session.last_export.as_ref().map(|l| &l["dir"]), Some(&json!("/lc-test-out")));
}

/// An export that ends before it gets going (a photo that can't be prepared, the ✕ while preparing) is not
/// "the last export": Export with Previous keeps the one before.
#[test]
fn an_export_that_never_got_going_leaves_the_previous_one_alone() {
    let kinds: [(&str, fn(&mut Headless)); 2] = [
        ("a photo that can't be prepared", |h| {
            let ids: Vec<u64> = h.app.session.visible_cloned().iter().take(3).map(|p| p.0).collect();
            let params = json!({"ids": ids, "dir": "/lc-test-out", "format": "original", "background": true});
            let r = h.request("engine.execute", json!({"command": "app.export", "params": params}), T);
            assert_eq!(r["ok"], true, "{r}");
        }),
        ("the ✕ while preparing", |h| {
            start_export(h, 5);
            let seen = h.request("ui.inspect", json!({}), T)["result"].clone();
            let id = seen["activity"][0]["id"].as_u64().unwrap();
            assert_eq!(h.request("engine.execute", json!({"command": "activity.cancel", "params": {"id": id}}), T)["ok"], true);
        }),
        // (a library opened meanwhile loads its own last export; the guard is the same `stopped` as the ✕'s:
        // `switching_library_while_an_export_prepares_stops_it` shows it stops, and this test is not run for it)
    ];
    for (what, go) in kinds {
        let mut h = demo();
        crate::export_task::set_prep_budget(Duration::ZERO);
        let (write, written) = recording_writer(1);
        h.app.services.write_shared = Some(write);
        h.app.session.last_export = Some(previous_export());
        go(&mut h);
        assert!(h.step_until(Duration::from_secs(60), |h| h.app.export.is_none()), "{what}: the export ended");
        assert!(written.lock().unwrap().is_empty(), "{what}: nothing written");
        assert_eq!(h.app.session.last_export, Some(previous_export()), "{what}: the previous export is still the last");
    }
}

/// A folder of `n` stand-in JPEGs and an app whose file probe takes 100 ms each (a slow drive).
fn slow_import(tag: &str, n: usize) -> (Headless, std::path::PathBuf) {
    gated_import(tag, n, None)
}

/// Like `slow_import`, and while `gate` is false the probe holds every file after the first batch
/// (`batch_size()` files, which join the catalog together), so a test can act mid-import without
/// racing the 100 ms-per-file clock. The hold gives up after 60 s, so a failed test never leaves
/// the import thread spinning.
fn gated_import(tag: &str, n: usize, gate: Option<std::sync::Arc<std::sync::atomic::AtomicBool>>) -> (Headless, std::path::PathBuf) {
    let dir = std::env::temp_dir().join(format!("lc-activity-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    for i in 0..n {
        std::fs::write(dir.join(format!("IMG_{i:03}.jpg")), format!("not really a jpeg {i}")).unwrap();
    }
    let mut s = lightcraft_engine::Session::new();
    s.media.file_probe = Some(std::sync::Arc::new(move |p: &str| {
        std::thread::sleep(Duration::from_millis(100));
        let index = p.rsplit_once("IMG_").and_then(|(_, n)| n.strip_suffix(".jpg")).and_then(|n| n.parse::<usize>().ok());
        if let (Some(gate), Some(index)) = (&gate, index)
            && index >= lightcraft_engine::import::batch_size()
        {
            let t0 = std::time::Instant::now();
            while !gate.load(std::sync::atomic::Ordering::Acquire) && t0.elapsed() < Duration::from_secs(60) {
                std::thread::sleep(Duration::from_millis(1));
            }
        }
        Ok(lightcraft_engine::media::ProbeInfo {
            width: 60,
            height: 40,
            format: "JPEG".into(),
            content_hash: Some(p.to_string()),
            ..Default::default()
        })
    }));
    let mut h = Headless::new(LightcraftApp::new(s, Services { png: None, ..Default::default() }), [1200.0, 800.0], 1.0);
    h.settle(T);
    (h, dir)
}

#[test]
fn import_cancelled_by_command_reports_cancelled() {
    let n = 40;
    let (mut h, dir) = slow_import("cancel", n);
    let undo0 = h.app.session.undo.len();
    crate::import::start_paths(&mut h.app, vec![dir.to_string_lossy().to_string()]).unwrap();
    assert!(h.step_until(Duration::from_secs(60), |h| h.app.import.as_ref().is_some_and(|t| t.imported >= 2)));
    let tasks = h.app.session.activity.list();
    assert_eq!(tasks.first().map(|t| t.kind), Some("import"), "{tasks:?}");
    h.app.session.activity.cancel(tasks[0].id).unwrap();
    assert!(h.step_until(Duration::from_secs(60), |h| h.app.import.is_none()));
    assert!(h.app.session.activity.list().is_empty());
    assert!(h.app.ui.toast.as_ref().is_some_and(|t| t.text.starts_with("Import cancelled")), "{:?}", h.app.ui.toast);
    // and, once the toast has passed, it is still in the list under the bell, saying how far it got
    let rows = h.app.session.activity.finished();
    assert_eq!(rows.len(), 1, "{rows:?}");
    assert_eq!((rows[0].kind, rows[0].ending), ("import", Ending::Stopped));
    assert!(rows[0].summary.starts_with("Import cancelled"), "{}", rows[0].summary);
    assert!(h.app.session.catalog.len() < n, "stopped part-way");
    assert_eq!(h.app.session.undo.len(), undo0 + 1, "one undo step");
    let _ = std::fs::remove_dir_all(&dir);
}

/// A photo chosen while an import runs stays the active one when the import ends: the import
/// selects its first photo only if nobody picked something else meanwhile (issue #1446).
#[test]
fn a_photo_chosen_during_an_import_stays_selected() {
    let gate = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let (mut h, dir) = gated_import("chosen", 20, Some(gate.clone()));
    crate::import::start_paths(&mut h.app, vec![dir.to_string_lossy().to_string()]).unwrap();
    assert!(h.step_until(Duration::from_secs(60), |h| h.app.import.as_ref().is_some_and(|t| t.imported >= 2)));
    // the newest photo so far, never the import's first
    let chosen = h.app.session.catalog.photos().map(|p| p.id).max().expect("photos arrived");
    let r = h.request("engine.execute", json!({"command": "library.select", "params": {"ids": [chosen.0]}}), T);
    assert_eq!(r["ok"], true, "{r}");
    // held at the gate until now, so the import cannot have ended however slowly the frames ran
    assert!(h.app.import.is_some(), "still importing");
    gate.store(true, std::sync::atomic::Ordering::Release);
    assert!(h.step_until(Duration::from_secs(60), |h| h.app.import.is_none()));
    assert_eq!(h.app.session.catalog.len(), 20, "all imported");
    assert_eq!(h.app.session.selection.active, Some(chosen));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn scan_shows_a_row_and_cancel_closes_it() {
    let (mut h, dir) = slow_import("scan", 120);
    crate::import::open(&mut h.app, vec![dir.to_string_lossy().to_string()]).unwrap();
    h.step();
    let tasks = h.app.session.activity.list();
    assert_eq!(tasks.first().map(|t| t.kind), Some("scan"), "{tasks:?}");
    assert!(tasks[0].cancellable);
    h.app.session.activity.cancel(tasks[0].id).unwrap();
    // like the old Cancel: the scan is dropped at once, without waiting for the file being read
    assert!(h.step_until(Duration::from_secs(10), |h| h.app.scan.is_none()));
    assert!(h.app.session.activity.list().is_empty());
    for _ in 0..3 {
        h.step();
    }
    assert!(h.app.ui.dialog.is_none(), "no review opened");
    let row = h.app.session.activity.last_finished().expect("a stopped scan leaves a row");
    assert_eq!((row.label.as_str(), row.ending), ("Scanning folder", Ending::Stopped));
    let _ = std::fs::remove_dir_all(&dir);
}

/// A library of three bracketed DNGs (−2, 0, +2 EV) on disk, all selected.
fn bracket(tag: &str) -> (Headless, std::path::PathBuf) {
    let dir = std::env::temp_dir().join(format!("lc-activity-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let mut paths = Vec::new();
    for (i, bytes) in lightcraft_merge::synth::bracket_dngs(1600, 1200, &[-2.0, 0.0, 2.0]).unwrap().into_iter().enumerate() {
        let p = dir.join(format!("IMG_{i}.dng"));
        std::fs::write(&p, bytes).unwrap();
        paths.push(p.to_string_lossy().to_string());
    }
    let mut session = lightcraft_engine::Session::new().with_fs();
    let r = session.execute("library.import", &json!({"paths": paths})).unwrap();
    let ids: Vec<u64> = r["imported"].as_array().unwrap().iter().map(|v| v.as_u64().unwrap()).collect();
    session.execute("library.select", &json!({"ids": ids})).unwrap();
    let app = LightcraftApp::new(session, Services { png: None, ..Default::default() });
    let mut h = Headless::new(app, [1200.0, 800.0], 1.0);
    h.settle(T);
    (h, dir)
}

#[test]
fn final_merge_shows_a_row_and_cancels() {
    let (mut h, dir) = bracket("merge");
    let before = h.app.session.catalog.len();
    crate::merge::start_last(&mut h.app, "merge.hdr").unwrap();
    h.step();
    let rows = h.app.session.activity.list();
    assert_eq!(rows.len(), 1);
    assert_eq!((rows[0].kind, rows[0].label.as_str(), rows[0].unit), ("merge", "Merging", Unit::Percent));
    assert!(rows[0].cancellable);
    assert_eq!(rows[0].total, 1000);
    h.app.session.activity.cancel(rows[0].id).unwrap();
    assert!(h.step_until(T, |h| h.app.merge.final_task.is_none()));
    assert!(h.app.merge.last_result.is_none(), "nothing merged");
    assert_eq!(h.app.session.catalog.len(), before, "no photo added");
    assert!(h.app.session.activity.list().is_empty());
    let toast = h.app.ui.toast().map(|t| t.text.clone()).unwrap_or_default();
    assert_eq!(toast, "HDR merge cancelled");
    let row = h.app.session.activity.last_finished().expect("a stopped merge leaves a row");
    assert_eq!((row.ending, row.summary.as_str()), (Ending::Stopped, "HDR merge cancelled"));
    let _ = std::fs::remove_dir_all(dir);
}

/// The merge dialog opens with the options of the last merge of its kind (per kind), and "with Last
/// Settings" uses them; a kind never merged opens with the defaults.
#[test]
fn merge_dialog_remembers_the_last_settings_per_kind() {
    let (mut h, dir) = bracket("merge-last");
    let opts = crate::merge::MergeDialog {
        align: false,
        deghost: "high".into(),
        auto_settings: false,
        ..crate::merge::MergeDialog::for_command("merge.hdr")
    };
    crate::merge::open(&mut h.app, "merge.hdr").unwrap();
    h.app.ui.dialog = None;
    crate::merge::start_final(&mut h.app, &opts).unwrap();
    let row = h.app.session.activity.list()[0].id;
    h.app.session.activity.cancel(row).unwrap();
    assert!(h.step_until(T, |h| h.app.merge.final_task.is_none()));
    crate::merge::open(&mut h.app, "merge.hdr").unwrap();
    let Some(crate::state::Dialog::Merge { opts: shown }) = &h.app.ui.dialog else { panic!("no merge dialog") };
    assert_eq!(shown, &opts);
    crate::merge::open(&mut h.app, "merge.panorama").unwrap();
    let Some(crate::state::Dialog::Merge { opts: shown }) = &h.app.ui.dialog else { panic!("no merge dialog") };
    assert_eq!(shown, &crate::merge::MergeDialog::for_command("merge.panorama"));
    h.app.ui.dialog = None;
    // older or hand-edited preferences: missing fields take their defaults, junk is ignored
    h.app.session.last_merge.insert("merge.panorama".into(), json!({"projection": "cylindrical", "command": "merge.hdr"}));
    h.app.session.last_merge.insert("merge.hdrPanorama".into(), json!("junk"));
    let pano = crate::merge::last_options(&h.app, "merge.panorama");
    assert_eq!((pano.command.as_str(), pano.projection.as_str(), pano.auto_crop), ("merge.panorama", "cylindrical", true));
    assert_eq!(crate::merge::last_options(&h.app, "merge.hdrPanorama"), crate::merge::MergeDialog::for_command("merge.hdrPanorama"));
    let _ = std::fs::remove_dir_all(dir);
}

/// Issue #2708: a preview that failed (a photo's file missing) is tried again when the dialog opens again; it used to
/// show neither a preview nor an error until an option changed.
#[test]
fn merge_dialog_retries_a_failed_preview_when_it_opens_again() {
    let (mut h, dir) = bracket("merge-retry");
    let (file, away) = (dir.join("IMG_1.dng"), dir.join("IMG_1.away"));
    std::fs::rename(&file, &away).unwrap();
    crate::merge::open(&mut h.app, "merge.panorama").unwrap();
    h.step();
    assert!(h.step_until(T, |h| h.app.merge.preview_task.is_none()));
    assert!(h.app.merge.error.is_some(), "the preview failed");
    h.app.ui.dialog = None;
    h.step();
    // the file is back (a drive reconnected); the same photos and options again
    std::fs::rename(&away, &file).unwrap();
    crate::merge::open(&mut h.app, "merge.panorama").unwrap();
    h.step();
    assert!(
        h.app.merge.preview.is_some() || h.app.merge.preview_task.is_some() || h.app.merge.error.is_some(),
        "the dialog shows a preview, one being made, or why not"
    );
    assert!(h.step_until(T, |h| h.app.merge.preview_task.is_none()));
    assert!(h.app.merge.preview.is_some(), "{:?}", h.app.merge.error);
    h.app.ui.dialog = None;
    h.step();
    let _ = std::fs::remove_dir_all(dir);
}

/// HDR for Each Bracket Set runs the merges one after the other, each with its activity row; cancelling one stops the
/// rest.
#[test]
fn batch_merge_runs_each_set_in_turn() {
    let dir = std::env::temp_dir().join(format!("lc-activity-batch-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let mut paths = Vec::new();
    // one `bracket_dngs` call renders the procedural scene once for all nine inputs (three calls
    // would render it three times); the exposures stay different, or the import would take one
    // set for duplicates of another
    let evs = [-2.0, 0.0, 2.0, -1.5, 0.5, 2.5, -1.0, 1.0, 3.0];
    let sets = ["A", "A", "A", "B", "B", "B", "C", "C", "C"];
    for (i, (set, bytes)) in sets.into_iter().zip(lightcraft_merge::synth::bracket_dngs(1600, 1200, &evs).unwrap()).enumerate() {
        let p = dir.join(format!("{set}_{}.dng", i % 3));
        std::fs::write(&p, bytes).unwrap();
        paths.push(p.to_string_lossy().to_string());
    }
    let mut session = lightcraft_engine::Session::new().with_fs();
    let r = session.execute("library.import", &json!({"paths": paths})).unwrap();
    let ids: Vec<u64> = r["imported"].as_array().unwrap().iter().map(|v| v.as_u64().unwrap()).collect();
    assert_eq!(ids.len(), 9);
    for set in ids.chunks(3) {
        session.execute("stack.group", &json!({"ids": set})).unwrap();
    }
    session.execute("library.select", &json!({"ids": ids})).unwrap();
    let app = LightcraftApp::new(session, Services { png: None, ..Default::default() });
    let mut h = Headless::new(app, [1200.0, 800.0], 1.0);
    // the merge ordering does not need the grid's nine thumbnails; one Detail render is enough
    let r = h.request("ui.set", json!({"view": "detail", "filmstrip": false}), T);
    assert_eq!(r["ok"], true, "{r}");
    assert!(h.settle(T), "the imported photos and the active preview settle");
    let r = crate::menus::run_ui_command(&mut h.app, "merge.hdrBatch", &json!({})).unwrap().unwrap();
    assert_eq!(r["sets"], 3);
    assert_eq!(h.app.merge.batch.len(), 2, "one running, two queued");
    let rows = h.app.session.activity.list();
    assert_eq!((rows.len(), rows.first().map(|r| r.kind)), (1, Some("merge")));
    // the first merge finishes and the second starts by itself
    assert!(h.step_until(T, |h| h.app.merge.batch.len() == 1 && h.app.merge.final_task.is_some()));
    assert_eq!(h.app.session.catalog.len(), 10);
    // cancelling the second drops the third
    let rows = h.app.session.activity.list();
    h.app.session.activity.cancel(rows[0].id).unwrap();
    assert!(h.step_until(T, |h| h.app.merge.final_task.is_none()));
    assert!(h.app.merge.batch.is_empty());
    assert_eq!(h.app.session.catalog.len(), 10, "nothing more merged");
    assert!(h.app.session.activity.list().is_empty());
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn find_missing_shows_an_indeterminate_row() {
    let mut h = demo();
    h.app.session.activity.keep_after(0);
    let dir = std::env::temp_dir().join(format!("lc-activity-findmissing-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    // run directly, not through a request (which runs frames): the search of an empty folder is over at once, but its
    // result is applied between frames, and the row is there until then
    let r = crate::menus::run_ui_command(&mut h.app, "file.findMissing", &json!({"folder": dir.to_string_lossy()})).unwrap().unwrap();
    assert_eq!(r["background"], true, "{r}");
    let rows = h.app.session.activity.list();
    assert_eq!(rows.len(), 1, "{rows:?}");
    assert_eq!((rows[0].kind, rows[0].label.as_str()), ("findMissing", "Find Missing Photos"));
    assert_eq!((rows[0].total, rows[0].cancellable), (0, false), "indeterminate, no ✕");
    assert!(h.step_until(T, |h| h.app.tasks.is_empty()));
    assert!(h.app.session.activity.list().is_empty(), "the row goes with the task");
    let row = h.app.session.activity.last_finished().expect("and a row says what it found");
    assert_eq!(row.ending, Ending::Done);
    assert!(row.summary.starts_with("Found 0 missing photos"), "{:?}", row.summary);
    let _ = std::fs::remove_dir_all(dir);
}

/// A Lightroom catalog that can't be read leaves a row saying the task failed, and why.
#[test]
fn a_lightroom_catalog_that_cannot_be_read_leaves_a_failed_row() {
    let mut h = demo();
    let dir = std::env::temp_dir().join(format!("lc-activity-lrcat-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("Not a catalog.lrcat");
    std::fs::write(&path, b"this is not a Lightroom catalog").unwrap();
    let r = crate::menus::run_ui_command(&mut h.app, "library.inspectLightroom", &json!({"path": path.to_string_lossy(), "wait": true}));
    assert!(r.is_some(), "the command is the interface's");
    let row = h.app.session.activity.last_finished().expect("a failed task always leaves a row");
    assert_eq!((row.kind, row.ending), ("lightroom", Ending::Failed));
    assert!(row.summary.starts_with("Lightroom inspection failed"), "{:?}", row.summary);
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn auto_import_listing_has_no_row() {
    let mut h = demo();
    let dir = std::env::temp_dir().join(format!("lc-activity-autoimport-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    h.app.session.import_defaults.auto_folder = Some(dir.to_string_lossy().to_string());
    // the watched folder is listed every few seconds, quietly
    assert!(h.step_until(Duration::from_secs(10), |h| h.app.tasks.is_running("Auto Import")));
    assert!(h.app.session.activity.list().is_empty(), "{:?}", h.app.session.activity.list());
    h.app.session.import_defaults.auto_folder = None;
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn rows_added_later_grow_the_open_list() {
    let mut h = demo();
    let a = h.app.session.activity.start("export", "Exporting", Cancel::Yes);
    a.progress(1, 24);
    a.detail("LC01347.jpg");
    wait_visible(&mut h);
    open_list(&mut h);
    // two more tasks while the list already shows one row: it grows to show them, nothing is cut off
    let b = h.app.session.activity.start("previews", "Building previews", Cancel::Yes);
    let c = h.app.session.activity.start("lightroom", "Reading Lightroom catalog", Cancel::Yes);
    wait_visible(&mut h);
    for _ in 0..5 {
        h.step();
    }
    let list = h.view.ctx.memory(|m| m.area_rect(crate::panels::activity::popup_id())).expect("the list is drawn");
    for g in [&a, &b, &c] {
        let r = rect(&h, &format!("activity:row:{}", g.id()));
        assert!(list.contains_rect(r), "row {} at {r:?} is cut off by the list at {list:?}", g.id());
    }
}

#[test]
fn immediate_library_commands_and_auto_sync_show_completion() {
    let mut h = demo();
    assert!(h.app.run("library.clearPreviews", json!({})).is_ok());
    assert_eq!(h.app.ui.toast.as_ref().map(|t| t.text.as_str()), Some("✓ Discard Preview Cache"));

    assert_eq!(h.app.run("develop.autoSync", json!({"on": true})).unwrap()["on"], true);
    assert_eq!(h.app.ui.toast.as_ref().map(|t| t.text.as_str()), Some("Auto Sync: On"));

    assert_eq!(h.app.run("develop.autoSync", json!({"on": false})).unwrap()["on"], false);
    assert_eq!(h.app.ui.toast.as_ref().map(|t| t.text.as_str()), Some("Auto Sync: Off"));

    // This demo has no on-disk library to optimize. A failed command must not
    // report completion (or accidentally keep its preceding success toast).
    h.app.ui.clear_toasts();
    assert!(h.app.run("library.compact", json!({})).is_err());
    assert!(h.app.ui.toast().is_none());
}

/// A task that ended `ending`, kept in the list whatever time it took. Its id.
fn ended(h: &mut Headless, label: &str, ending: Ending, summary: &str) -> u64 {
    h.app.session.activity.keep_after(0);
    let g = h.app.session.activity.start("export", label, Cancel::Yes);
    let id = g.id();
    assert!(g.finish(Outcome { ending, summary: summary.into(), ..Default::default() }));
    id
}

#[test]
fn a_task_that_ended_stays_in_the_list_until_dismissed() {
    let mut h = demo();
    // Given an export that ran long and ended well, and nothing running
    let id = ended(&mut h, "Exporting", Ending::Done, "Exported 5,000 photos");
    h.step();
    h.step();
    // Then the bell asks for nothing: no ring, no mark
    assert!(!has(&h, "activity:ring") && !has(&h, "activity:attention"));
    // When the list is opened, Then the task is there, and "No tasks are running" is not said over it
    open_list(&mut h);
    assert!(has(&h, &format!("activity:done:{id}")) && !has(&h, "activity:empty"));
    let inspect = h.request("ui.inspect", json!({}), T);
    assert_eq!(inspect["result"]["activityFinished"][0]["summary"], "Exported 5,000 photos", "{inspect}");
    // And it stays, frame after frame
    for _ in 0..20 {
        h.step();
    }
    assert!(has(&h, &format!("activity:done:{id}")));
    // When its ✕ is clicked, Then it is gone, and the list is still open
    click(&mut h, &format!("activity:dismiss:{id}"));
    assert!(h.app.session.activity.finished().is_empty());
    assert!(has(&h, "activity:empty"), "the list stays open on what is left");
}

#[test]
fn a_task_that_did_not_end_well_marks_the_bell_until_its_row_is_dismissed() {
    let mut h = demo();
    let id = ended(&mut h, "Exporting", Ending::Failed, "The disk is full");
    let other = ended(&mut h, "Importing", Ending::Stopped, "Import cancelled");
    h.step();
    h.step();
    // the mark is on the bell while the list is closed, however many frames pass
    assert!(has(&h, "activity:attention"));
    for _ in 0..10 {
        h.step();
    }
    assert!(has(&h, "activity:attention"));
    // looking at the list does not answer it: the mark stays while the list is open, and after it is closed
    open_list(&mut h);
    assert!(has(&h, "activity:attention") && has(&h, &format!("activity:done:{id}")));
    click(&mut h, "icon:bell");
    assert!(!has(&h, "activity:list") && has(&h, "activity:attention"), "seen is not dealt with");
    // dismissing one of two leaves the mark; dismissing the last takes it away
    open_list(&mut h);
    click(&mut h, &format!("activity:dismiss:{id}"));
    assert!(has(&h, "activity:attention") && h.app.session.activity.problems() == 1);
    click(&mut h, &format!("activity:dismiss:{other}"));
    assert!(!has(&h, "activity:attention") && h.app.session.activity.problems() == 0);
}

#[test]
fn rows_are_grouped_by_what_they_need_and_clear_finished_lets_go_of_the_ended_ones() {
    let mut h = demo();
    let bad = ended(&mut h, "Importing", Ending::Problems, "Imported 90 photos");
    let good = ended(&mut h, "Exporting", Ending::Done, "Exported 24 photos");
    let running = h.app.session.activity.start("previews", "Building previews", Cancel::Yes);
    wait_visible(&mut h);
    open_list(&mut h);
    // what needs looking at first, then what runs, then what ended well
    let y = |id: &str| rect(&h, id).top();
    let (bad_y, run_y, good_y) =
        (y(&format!("activity:done:{bad}")), y(&format!("activity:row:{}", running.id())), y(&format!("activity:done:{good}")));
    assert!(bad_y < run_y && run_y < good_y, "{bad_y} < {run_y} < {good_y}");
    // Clear finished lets go of both ended tasks and leaves the running one
    click(&mut h, "activity:clear");
    assert!(h.app.session.activity.finished().is_empty());
    assert!(has(&h, &format!("activity:row:{}", running.id())) && !has(&h, "activity:clear"));
    assert!(!has(&h, "activity:attention"), "and with them the mark on the bell");
    assert!(!running.is_cancelled(), "clearing stops nothing");
}

#[test]
fn a_task_that_just_ended_is_named_beside_the_bell_for_a_moment() {
    let mut h = demo_sized([1600.0, 900.0]);
    ended(&mut h, "Exporting", Ending::Done, "Exported 24 photos");
    h.step();
    h.step();
    assert!(has(&h, "activity:readout"), "the line says how it ended");
    // after a few seconds the line is gone; the row is not
    std::thread::sleep(Duration::from_millis(6_100));
    h.step();
    h.step();
    assert!(!has(&h, "activity:readout") && h.app.session.activity.finished().len() == 1);
}

#[test]
fn an_ended_task_is_worded_from_its_name_when_it_said_nothing_and_says_how_long_it_took() {
    let mut h = demo();
    let id = ended(&mut h, "Building previews", Ending::Died, "");
    let rows = h.app.session.activity.finished();
    let row = rows.iter().find(|f| f.id == id).unwrap();
    assert_eq!(summary_of(row), "Building previews: stopped unexpectedly");
    assert_eq!(
        (took(42_900).as_str(), took(12 * 60_000 + 5_000).as_str(), took(6_720_000).as_str()),
        ("took 42 s", "took 12 min", "took 1 h 52 min")
    );
}

#[test]
fn an_export_that_failed_for_a_file_leaves_a_row_with_the_file_and_the_reason() {
    let mut h = demo();
    // Given a disk that refuses the first file of five
    let refuse = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(true));
    h.app.services.write_shared =
        Some(std::sync::Arc::new(
            move |_: &str, _: &[u8]| {
                if refuse.swap(false, std::sync::atomic::Ordering::Relaxed) { Err("The disk is full".into()) } else { Ok(()) }
            },
        ));
    // When five photos are exported
    start_export(&mut h, 5);
    assert!(h.step_until(Duration::from_secs(60), |h| h.app.export.is_none()));
    // Then the export is in the list, however short it was: done, with problems, in its folder
    let rows = h.app.session.activity.finished();
    assert_eq!(rows.len(), 1, "{rows:?}");
    let row = &rows[0];
    assert_eq!((row.kind, row.ending, row.failed), ("export", Ending::Problems, 1), "{row:?}");
    assert_eq!(row.summary, "Exported 4 of 5 photos");
    assert!(row.path.as_deref().is_some_and(|p| p.starts_with("/lc-test-out/")), "one of the files written: {:?}", row.path);
    assert!(!row.failures[0].name.is_empty() && row.failures[0].reason.contains("The disk is full"), "{:?}", row.failures);
    // And it is said at once too, as ever; the bell is marked for when the toast has passed
    assert!(h.app.ui.toast.as_ref().is_some_and(|t| t.text.starts_with("Exported 4 of 5")), "{:?}", h.app.ui.toast);
    h.step();
    assert!(has(&h, "activity:attention"));
    // When the row's failed files are asked for, Then the file and the reason can be copied
    open_list(&mut h);
    click(&mut h, &format!("activity:failures:{}", row.id));
    click(&mut h, &format!("activity:copy:{}", row.id));
    let want = format!("{}\t{}", row.failures[0].name, row.failures[0].reason);
    assert!(h.step_until(T, |h| h.app.copied.as_deref() == Some(want.as_str())), "{:?}", h.app.copied);
}

#[test]
fn a_short_clean_export_is_a_toast_and_leaves_no_row() {
    let mut h = demo();
    // (short by any machine's clock: the ten seconds a row is kept after are far away here)
    h.app.session.activity.keep_after(600_000);
    h.app.services.write_shared = Some(slow_writer(1));
    start_export(&mut h, 2);
    assert!(h.step_until(Duration::from_secs(60), |h| h.app.export.is_none()));
    assert!(h.app.session.activity.finished().is_empty(), "watched to its end: nothing to come back to");
    assert!(h.app.ui.toast.as_ref().is_some_and(|t| t.text.starts_with("Exported 2 of 2")), "{:?}", h.app.ui.toast);
}

#[test]
fn a_stopped_export_and_a_dead_export_worker_each_leave_a_row() {
    let mut h = demo();
    h.app.services.write_shared = Some(slow_writer(200));
    start_export(&mut h, 5);
    assert_eq!(h.app.session.activity.cancel_all(), 1);
    assert!(h.step_until(Duration::from_secs(60), |h| h.app.export.is_none()));
    let rows = h.app.session.activity.finished();
    assert_eq!(rows.iter().map(|f| f.ending).collect::<Vec<_>>(), vec![Ending::Stopped], "{rows:?}");
    // a writer that panics takes the worker with it: the export says so instead of vanishing
    h.app.services.write_shared = Some(std::sync::Arc::new(|_: &str, _: &[u8]| -> Result<(), String> { panic!("synthetic writer panic") }));
    start_export(&mut h, 2);
    assert!(h.step_until(Duration::from_secs(60), |h| h.app.export.is_none()));
    let rows = h.app.session.activity.finished();
    assert_eq!(rows.len(), 2, "{rows:?}");
    assert_eq!((rows[0].ending, rows[0].summary.as_str()), (Ending::Died, "Export stopped unexpectedly"), "{:?}", rows[0]);
}

#[test]
fn the_failed_files_copied_say_how_many_were_not_kept_and_stay_a_line_each() {
    let h = demo();
    h.app.session.activity.keep_after(0);
    let g = h.app.session.activity.start("export", "Exporting", Cancel::Yes);
    let failures = vec![
        Failure { name: "IMG_1.jpg".into(), reason: "The disk\nis full".into() },
        Failure { name: "IMG\t2.jpg".into(), reason: "No permission".into() },
    ];
    // a job that counted 700 failures and could name two of them
    g.finish(Outcome { ending: Ending::Problems, failures, failed: 700, ..Default::default() });
    let row = &h.app.session.activity.finished()[0];
    assert_eq!(failures_text(row), "IMG_1.jpg\tThe disk is full\nIMG 2.jpg\tNo permission\nand 698 more");
}

/// The demo app, reading the files as they are on disk (the demo library reads none), keeping every ended task's row.
fn demo_on_disk() -> Headless {
    let mut h = demo();
    h.app.session.media.file_probe = lightcraft_engine::Session::new().with_fs().media.file_probe;
    h.app.session.activity.keep_after(0);
    h
}

/// A folder with a valid PNG and, in a subfolder, a JPEG cut short before its image data (the real reason).
fn folder_with_a_truncated_jpeg(tag: &str) -> std::path::PathBuf {
    use lightcraft_codecs::{ChromaSubsampling, EncodeImage, EncodeMeta};
    let dir = std::env::temp_dir().join(format!("lc-activity-unreadable-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("trip")).unwrap();
    let img = lightcraft_raster::Rgba8 { width: 64, height: 48, data: vec![[90, 120, 160, 255]; 64 * 48] };
    let png = lightcraft_codecs::encode_png(&EncodeImage::rgba8(&img), &EncodeMeta::default()).unwrap();
    std::fs::write(dir.join("good.png"), png).unwrap();
    let mut jpeg = lightcraft_codecs::encode_jpeg(&EncodeImage::rgba8(&img), 90, ChromaSubsampling::S444, &EncodeMeta::default()).unwrap();
    // cut before the image data starts (a file cut inside its data imports as a damaged photo since the
    // cut-short JPEG guard): nothing to show, so it is still unreadable, for the real reason
    let sos = jpeg.windows(2).position(|w| w == [0xFF, 0xDA]).unwrap();
    jpeg.truncate(sos);
    std::fs::write(dir.join("trip").join("bad.jpg"), jpeg).unwrap();
    dir
}

/// What an import's row says it could not read, and the text Copy all would put on the clipboard.
fn the_row_names_the_truncated_jpeg(h: &Headless) {
    let rows = h.app.session.activity.finished();
    assert_eq!(rows.len(), 1, "{rows:?}");
    let row = &rows[0];
    assert_eq!((row.kind, row.ending, row.failed), ("import", Ending::Problems, 1), "{row:?}");
    let names: Vec<&str> = row.failures.iter().map(|f| f.name.as_str()).collect();
    assert_eq!(names, vec![std::path::Path::new("trip").join("bad.jpg").to_string_lossy()], "{row:?}");
    assert!(row.failures[0].reason.contains("truncated"), "{}", row.failures[0].reason);
    assert!(failures_text(row).contains(&format!("bad.jpg\t{}", row.failures[0].reason)), "{}", failures_text(row));
}

#[test]
fn an_import_row_names_each_file_it_could_not_read_and_why() {
    let dir = folder_with_a_truncated_jpeg("paths");
    let mut h = demo_on_disk();
    crate::import::start_paths(&mut h.app, vec![dir.to_string_lossy().to_string()]).unwrap();
    assert!(h.step_until(Duration::from_secs(60), |h| h.app.import.is_none()));
    the_row_names_the_truncated_jpeg(&h);
    let _ = std::fs::remove_dir_all(&dir);
}

/// The review hands the engine the checked files, not the folder: the names of those it cannot read still start at
/// the source folder. (A file the scan could not read is not offered, so this one is made to look readable.)
#[test]
fn a_reviewed_imports_row_names_unreadable_files_below_its_source() {
    let dir = folder_with_a_truncated_jpeg("review");
    let mut h = demo_on_disk();
    crate::import::open(&mut h.app, vec![dir.to_string_lossy().to_string()]).unwrap();
    assert!(h.step_until(T, |h| matches!(h.app.ui.dialog, Some(crate::state::Dialog::Import { .. }))));
    h.app.session.activity.dismiss_all();
    let Some(crate::state::Dialog::Import { opts }) = &mut h.app.ui.dialog else { panic!("no review") };
    for c in &mut opts.candidates {
        c.error = None;
    }
    opts.checked.iter_mut().for_each(|on| *on = true);
    assert_eq!(opts.selected_paths().len(), 2);
    assert_eq!(h.request("ui.dialog.confirm", json!({}), T)["ok"], true);
    assert!(h.step_until(Duration::from_secs(60), |h| h.app.import.is_none()));
    the_row_names_the_truncated_jpeg(&h);
    let _ = std::fs::remove_dir_all(&dir);
}

/// A task that only counted what it could not do (a preview build) has the count said under its name; one that said
/// more keeps what it said.
#[test]
fn an_ended_task_that_only_counted_its_failures_says_how_many() {
    let h = demo();
    h.app.session.activity.keep_after(0);
    let g = h.app.session.activity.start("previews", "Building previews", Cancel::Yes);
    g.finish(Outcome { ending: Ending::Problems, failed: 3, ..Default::default() });
    let f = h.app.session.activity.last_finished().unwrap();
    assert_eq!((summary_of(&f), detail_of(&f)), ("Building previews: done, with problems".to_string(), "3 failed".to_string()));
    let g = h.app.session.activity.start("import", "Importing", Cancel::Yes);
    g.finish(Outcome { ending: Ending::Problems, failed: 2, detail: "2 not readable".into(), ..Default::default() });
    assert_eq!(detail_of(&h.app.session.activity.last_finished().unwrap()), "2 not readable");
}

/// A scan that ran to its end says so, and says when it found nothing to import.
#[test]
fn a_scan_that_found_nothing_leaves_a_row_saying_so() {
    let mut h = demo();
    h.app.session.activity.keep_after(0);
    let dir = std::env::temp_dir().join(format!("lc-activity-emptyscan-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    crate::import::open(&mut h.app, vec![dir.to_string_lossy().to_string()]).unwrap();
    assert!(h.step_until(T, |h| h.app.scan.is_none()));
    let row = h.app.session.activity.last_finished().expect("a row for the scan");
    assert_eq!((row.label.as_str(), row.ending, row.detail.as_str()), ("Scanning folder", Ending::Done, "No photos found"));
    let _ = std::fs::remove_dir_all(dir);
}

/// A scan that met files it could not read says so in its row, by name, whether it was for the import review or the
/// Local view (which leaves those files out, so nothing later would say it). The Local view's row says which folder,
/// and opening that folder again replaces it instead of adding another.
#[test]
#[allow(clippy::single_element_loop)]
fn a_scan_that_met_unreadable_files_leaves_a_row_with_problems() {
    // (integration: main's Local browse scan only lists the folder on its worker and reads nothing, so it counts no
    // unreadable files; the PR's `true` case, which expected 2, can't hold until the browse reads them again)
    for browse in [false] {
        let mut h = demo();
        h.app.session.activity.keep_after(0);
        // the files are read as on disk (the demo library reads none)
        h.app.session.media.file_probe = lightcraft_engine::Session::new().with_fs().media.file_probe;
        let dir = std::env::temp_dir().join(format!("lc-activity-badscan-{browse}-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("sub")).unwrap();
        std::fs::write(dir.join("broken.jpg"), b"not a jpeg at all").unwrap();
        std::fs::write(dir.join("sub").join("also.jpg"), b"not a jpeg either").unwrap();
        let path = dir.to_string_lossy().to_string();
        let scan = |h: &mut Headless| {
            if browse {
                crate::import::browse(&mut h.app, &path, Some(true)).unwrap();
            } else {
                crate::import::open(&mut h.app, vec![path.clone()]).unwrap();
            }
            assert!(h.step_until(T, |h| h.app.scan.is_none()));
        };
        scan(&mut h);
        let names = |h: &Headless| {
            let mut v: Vec<String> = h.app.session.activity.finished()[0].failures.iter().map(|f| f.name.clone()).collect();
            v.sort();
            v
        };
        let row = h.app.session.activity.finished()[0].clone();
        assert_eq!((row.ending, row.failed, row.detail.as_str()), (Ending::Problems, 2, "2 photos not readable"), "browse {browse}: {row:?}");
        // the Local view names a file by where it is in the folder (two subfolders can hold the same name)
        let deeper = if browse { std::path::Path::new("sub").join("also.jpg").to_string_lossy().into_owned() } else { "also.jpg".into() };
        let mut want = vec!["broken.jpg".to_string(), deeper];
        want.sort();
        assert_eq!(names(&h), want, "{row:?}");
        if browse {
            let folder = std::path::absolute(&dir).unwrap().to_string_lossy().trim_end_matches(std::path::MAIN_SEPARATOR).to_string();
            assert_eq!(row.path.as_deref(), Some(folder.as_str()), "{row:?}");
            scan(&mut h);
            let rows = h.app.session.activity.finished();
            assert_eq!((rows.len(), h.app.session.activity.problems()), (1, 1), "one row for the folder: {rows:?}");
            assert!(rows[0].id != row.id && rows[0].failed == 2, "the new row is the one kept: {rows:?}");
        }
        let _ = std::fs::remove_dir_all(dir);
    }
}

/// The menu item of `command` whose params have `size`, anywhere in the menu tree (`ui.menu.tree`).
fn menu_item(tree: &serde_json::Value, command: &str, size: &str) -> Option<serde_json::Value> {
    match tree {
        serde_json::Value::Array(nodes) => nodes.iter().find_map(|n| menu_item(n, command, size)),
        serde_json::Value::Object(node) => {
            let found = node.get("id").and_then(|v| v.as_str()) == Some(command) && node.get("params").is_some_and(|p| p["size"] == size);
            if found { Some(tree.clone()) } else { node.values().find_map(|v| menu_item(v, command, size)) }
        }
        _ => None,
    }
}

/// Given a preview build running, then Build Standard-Sized Previews is still offered. When it is chosen, then it
/// waits in line: a toast says so, and the list shows it under the running build, starting next. When its ✕ is
/// clicked, then it leaves the line, and the running build goes on.
#[test]
fn a_second_preview_build_waits_in_line_and_can_be_taken_out_of_it() {
    let mut h = demo();
    let build = std::sync::Arc::new(lightcraft_engine::cmd::previews::PreviewBuild::default());
    h.app.session.preview_build = Some(build.clone());
    let running = h.app.session.activity.start("previews", "Building previews", Cancel::Yes);
    let tree = h.request("ui.menu.tree", json!({}), T)["result"].clone();
    let item = menu_item(&tree, "library.buildPreviews", "standard").expect("Build Standard-Sized Previews in the menus");
    assert_eq!(item["enabled"], true, "not greyed out while a build runs: {item}");
    let r = h.request("ui.menu.invoke", json!({"id": "library.buildPreviews", "params": item["params"]}), T);
    let id = r["result"]["queued"].as_u64().unwrap_or_else(|| panic!("it waits in line: {r}"));
    assert!(h.app.ui.toast.as_ref().is_some_and(|t| t.text.starts_with("Queued")), "{:?}", h.app.ui.toast);
    open_list(&mut h);
    let (waiting, ahead) = (rect(&h, &format!("activity:row:{id}")), rect(&h, &format!("activity:row:{}", running.id())));
    assert!(waiting.top() > ahead.bottom(), "the waiting build is listed under the running one");
    click(&mut h, &format!("activity:cancel:{id}"));
    assert!(h.app.session.activity.waiting().is_empty(), "✕ took it out of the line");
    assert!(!running.is_cancelled(), "and left the running build alone");
    build.finished.store(true, std::sync::atomic::Ordering::Release);
    drop(running);
}

/// Given a preview build that did not go well, then its row offers Retry. When Retry is clicked, then the row goes and
/// the build runs again, for the same photos.
#[test]
fn a_build_that_did_not_go_well_offers_retry() {
    let mut h = demo();
    let photo = h.app.session.catalog.photos().next().map(|p| p.id.0).unwrap();
    let g = h.app.session.activity.start("previews", "Building previews", Cancel::Yes);
    let request = lightcraft_engine::activity::Request {
        command: "library.buildPreviews".into(),
        params: json!({"ids": [photo], "edge": 256}),
        ..Default::default()
    };
    g.set_request(request);
    let failed = g.id();
    g.finish(Outcome::failed("The disk is full"));
    open_list(&mut h);
    click(&mut h, &format!("activity:retry:{failed}"));
    assert!(h.app.session.activity.finished().iter().all(|f| f.id != failed), "the row was taken up by the retry");
    let progress = h.app.session.execute("library.previewProgress", &json!({})).unwrap();
    assert_eq!(progress["total"], 1, "a build of that one photo started: {progress}");
    assert!(h.step_until(T, |h| h.app.session.activity.list().is_empty()));
}

/// Given a preview build that did not go well and a free lane, when an agent asks `activity.retry`, then the build has
/// started by the time it answers, and no toast says it was queued.
#[test]
fn a_retry_asked_by_an_agent_that_starts_at_once_does_not_say_it_was_queued() {
    let mut h = demo();
    let photo = h.app.session.catalog.photos().next().map(|p| p.id.0).unwrap();
    let g = h.app.session.activity.start("previews", "Building previews", Cancel::Yes);
    g.set_request(lightcraft_engine::activity::Request {
        command: "library.buildPreviews".into(),
        params: json!({"ids": [photo], "edge": 256}),
        ..Default::default()
    });
    let failed = g.id();
    g.finish(Outcome::failed("The disk is full"));
    h.app.ui.toast = None;
    let r = h.app.run("activity.retry", json!({"id": failed}));
    assert!(r.as_ref().is_ok_and(|v| v["queued"].as_u64().is_some()), "{r:?}");
    assert!(!h.app.ui.toast.as_ref().is_some_and(|t| t.text.starts_with("Queued")), "{:?}", h.app.ui.toast);
    assert!(h.step_until(T, |h| h.app.session.activity.list().is_empty()));
}

/// Given a preview build running and another waiting for it, when Quit is chosen, then the question says that one
/// runs and one won't run; Quit Anyway stops the one and empties the line.
#[test]
fn quitting_says_how_many_tasks_waiting_in_line_would_not_run() {
    let mut h = demo();
    let build = std::sync::Arc::new(lightcraft_engine::cmd::previews::PreviewBuild::default());
    h.app.session.preview_build = Some(build.clone());
    let running = h.app.session.activity.start("previews", "Building previews", Cancel::Yes);
    let r = h.app.run("library.buildPreviews", json!({"edge": 256, "queue": true}));
    assert!(r.as_ref().is_ok_and(|v| v["place"] == 1), "{r:?}");
    assert!(!crate::panels::notices::may_close(&mut h.app));
    let Some(crate::QuitPrompt::Tasks(text)) = h.app.quit_prompt.clone() else { panic!("{:?}", h.app.quit_prompt) };
    assert_eq!(text, "1 task is still running: Building previews. Quitting stops it. 1 task waiting in line won't run.");
    h.step();
    click(&mut h, "button:quitAnyway");
    assert!(running.is_cancelled() && h.app.session.activity.waiting().is_empty());
    assert!(h.app.quit_prompt.is_none(), "not asked again: {:?}", h.app.quit_prompt);
    assert!(h.quit_requested(), "the window closes");
    build.finished.store(true, std::sync::atomic::Ordering::Release);
    drop(running);
}

/// Given a preview build running and another waiting in line behind it, then the list shows both, and the waiting
/// one starts by itself, in a frame, when the build ahead of it ends.
#[test]
fn a_task_waiting_in_line_has_a_row_and_starts_when_the_lane_frees() {
    use lightcraft_engine::cmd::previews::PreviewBuild;
    use std::sync::atomic::Ordering;

    let mut h = demo();
    let build = std::sync::Arc::new(PreviewBuild::default());
    h.app.session.preview_build = Some(build.clone());
    let running = h.app.session.activity.start("previews", "Building previews", Cancel::Yes);
    let ids: Vec<u64> = h.app.session.catalog.photos().take(1).map(|p| p.id.0).collect();
    let queued = h.app.session.execute("library.buildPreviews", &json!({"ids": ids, "edge": 256, "queue": true})).unwrap();
    let waiting = queued["queued"].as_u64().expect("it waits");
    assert_eq!(h.app.session.activity.list().len(), 2);
    wait_visible(&mut h);
    open_list(&mut h);
    assert!(has(&h, &format!("activity:row:{}", running.id())));
    assert!(has(&h, &format!("activity:row:{waiting}")) && rows(&h) == 2, "a waiting task draws a row, under the running one");
    // the lane frees with no command asked: the frame loop starts it
    build.finished.store(true, Ordering::Release);
    drop(running);
    let t0 = std::time::Instant::now();
    while h.app.session.activity.list().iter().any(|t| t.id == waiting && t.label == "Building previews") && t0.elapsed() < T {
        h.step();
        let started = h.app.session.preview_build.as_ref().is_some_and(|b| !std::sync::Arc::ptr_eq(b, &build));
        if started {
            break;
        }
    }
    assert!(h.app.session.preview_build.as_ref().is_some_and(|b| !std::sync::Arc::ptr_eq(b, &build)), "the waiting build started");
    assert!(h.step_until(T, |h| h.app.session.activity.list().is_empty()), "the started build ends");
}

/// Given a build that ended and a build waiting behind it, when a command starts the waiting one before the frame that
/// announces the first, then the first one's end is still announced, and once.
#[test]
fn a_build_replaced_by_the_one_waiting_behind_it_is_still_announced() {
    use lightcraft_engine::cmd::previews::PreviewBuild;
    use std::sync::atomic::Ordering;

    let mut h = demo();
    let first = std::sync::Arc::new(PreviewBuild::default());
    first.done.store(2, Ordering::Relaxed);
    h.app.session.preview_build = Some(first.clone());
    let row = h.app.session.activity.start("previews", "Building previews", Cancel::Yes);
    let ids: Vec<u64> = h.app.session.catalog.photos().take(1).map(|p| p.id.0).collect();
    h.app.session.execute("library.buildPreviews", &json!({"ids": ids, "edge": 256, "queue": true})).unwrap();
    first.finished.store(true, Ordering::Release);
    drop(row);
    // a command in the same frame, before the app looks at the finished build
    h.app.session.execute("activity.list", &json!({})).unwrap();
    assert!(h.app.session.preview_build.as_ref().is_some_and(|b| !std::sync::Arc::ptr_eq(b, &first)), "the waiting build replaced it");
    h.step();
    assert!(first.reported.load(Ordering::Relaxed), "its end was announced");
    let toast = h.app.ui.toast.as_ref().map(|t| t.text.clone()).unwrap_or_default();
    assert!(toast.contains("Previews ready for 2 photos"), "{toast}");
    assert!(h.app.session.take_unreported_builds().is_empty());
    assert!(h.step_until(T, |h| h.app.session.activity.list().is_empty()), "the started build ends");
}

/// Given a build that ended and a build waiting behind it, when the frame starts the waiting one, then the same frame
/// asks to be drawn again to see how it goes: nothing else repaints an idle window (the list is closed, or Full Screen
/// Preview hides it).
#[test]
fn the_frame_that_starts_a_waiting_build_asks_for_the_next_one() {
    use lightcraft_engine::cmd::previews::PreviewBuild;
    use std::sync::atomic::Ordering;

    let mut h = demo();
    let first = std::sync::Arc::new(PreviewBuild::default());
    h.app.session.preview_build = Some(first.clone());
    let row = h.app.session.activity.start("previews", "Building previews", Cancel::Yes);
    let ids: Vec<u64> = h.app.session.catalog.photos().take(1).map(|p| p.id.0).collect();
    h.app.session.execute("library.buildPreviews", &json!({"ids": ids, "edge": 256, "queue": true})).unwrap();
    first.finished.store(true, Ordering::Release);
    drop(row);
    let ctx = egui::Context::default();
    // (a few empty passes let the context's own first-frame requests go by)
    for _ in 0..4 {
        let mut out = ctx.run_ui(egui::RawInput::default(), |_| {});
        out.textures_delta.clear();
    }
    assert!(!ctx.has_requested_repaint(), "an idle context asks for nothing");
    h.app.tick_preview_builds(&ctx);
    assert!(h.app.session.preview_build.as_ref().is_some_and(|b| !std::sync::Arc::ptr_eq(b, &first)), "the waiting build started");
    assert!(ctx.has_requested_repaint(), "and the frame asked to look at it again");
    assert!(h.step_until(T, |h| h.app.session.activity.list().is_empty()), "the started build ends");
}

/// An import that brought in JPEGs cut short says how many in its message, and the photos carry
/// the mark.
#[test]
fn import_message_counts_the_jpegs_cut_short() {
    let (mut h, dir) = slow_import("damaged", 3);
    h.app.session.media.file_probe = Some(std::sync::Arc::new(|p: &str| {
        Ok(lightcraft_engine::media::ProbeInfo {
            width: 60,
            height: 40,
            format: "JPEG".into(),
            content_hash: Some(p.to_string()),
            truncated: p.ends_with("IMG_001.jpg"),
            ..Default::default()
        })
    }));
    crate::import::start_paths(&mut h.app, vec![dir.to_string_lossy().to_string()]).unwrap();
    assert!(h.step_until(Duration::from_secs(60), |h| h.app.import.is_none()));
    assert_eq!(h.app.session.catalog.len(), 3);
    assert_eq!(h.app.session.catalog.photos().filter(|p| p.truncated).count(), 1);
    assert_eq!(h.app.ui.toast.as_ref().map(|t| t.text.as_str()), Some("Imported 3 photos · 1 damaged"));
    let _ = std::fs::remove_dir_all(&dir);
}

/// A row that only waits on something outside the app (XMP writes waiting for their folder) doesn't keep the stack looking
/// again every tenth of a second once it shows: it can stay for hours. Any other row, or a young one, does.
#[test]
fn a_row_that_only_waits_does_not_keep_the_stack_polling() {
    use crate::panels::activity::{SHOW_AFTER_MS, needs_poll};
    let s = lightcraft_engine::Session::new();
    let waiting = s.activity.start("xmpWaiting", "Waiting to write XMP sidecars", Cancel::No);
    let running = s.activity.start("export", "Exporting", Cancel::Yes);
    let rows = s.activity.list();
    let of = |kind: &str| rows.iter().find(|t| t.kind == kind).cloned().unwrap();
    let (mut wait, mut run) = (of("xmpWaiting"), of("export"));
    wait.age_ms = SHOW_AFTER_MS + 1;
    run.age_ms = SHOW_AFTER_MS + 1;
    assert!(!needs_poll(&[wait.clone()]), "an old waiting row is still");
    assert!(needs_poll(&[wait.clone(), run]), "a running task moves");
    wait.age_ms = 0;
    assert!(needs_poll(&[wait]), "a young one is about to be shown");
    assert!(!needs_poll(&[]), "nothing to look at");
    drop((waiting, running));
}


/// A read that does not answer until `open`.
#[derive(Clone, Default)]
pub(crate) struct Gate(std::sync::Arc<(std::sync::Mutex<bool>, std::sync::Condvar)>);

impl Gate {
    pub(crate) fn wait(&self) {
        let (open, bell) = &*self.0;
        let mut open = open.lock().unwrap();
        while !*open {
            open = bell.wait(open).unwrap();
        }
    }

    pub(crate) fn open(&self) {
        *self.0.0.lock().unwrap() = true;
        self.0.1.notify_all();
    }
}

/// A folder of four stand-in JPEGs and one (`IMG_hang.jpg`) whose read never answers, and an app that calls a read
/// stalled after 500 ms (a network drive that dropped its connection).
fn hanging_import(tag: &str) -> (Headless, std::path::PathBuf, Gate) {
    hanging(tag, &["IMG_hang.jpg"])
}

/// [`hanging_import`] with the files that never answer named.
fn hanging(tag: &str, hangs: &[&str]) -> (Headless, std::path::PathBuf, Gate) {
    let dir = std::env::temp_dir().join(format!("lc-activity-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    for name in ["IMG_000.jpg", "IMG_001.jpg", "IMG_002.jpg", "IMG_003.jpg"].iter().chain(hangs) {
        std::fs::write(dir.join(name), format!("not really a jpeg {name}")).unwrap();
    }
    let gate = Gate::default();
    let mut s = lightcraft_engine::Session::new();
    s.import_stall_after = Duration::from_millis(500);
    // (its own count of blocked readers: tests share the process)
    s.stuck_readers = std::sync::Arc::new(lightcraft_engine::import::StuckReaders::new(lightcraft_engine::import::MAX_STUCK_READERS));
    let in_probe = gate.clone();
    s.media.file_probe = Some(std::sync::Arc::new(move |p: &str| {
        if p.contains("IMG_hang") {
            in_probe.wait();
        }
        Ok(lightcraft_engine::media::ProbeInfo {
            width: 60,
            height: 40,
            format: "JPEG".into(),
            content_hash: Some(p.to_string()),
            ..Default::default()
        })
    }));
    let mut h = Headless::new(LightcraftApp::new(s, Services { png: None, ..Default::default() }), [1200.0, 800.0], 1.0);
    h.settle(T);
    (h, dir, gate)
}

/// The import's row once it says it waits for a file.
fn stalled_row(h: &mut Headless) -> TaskInfo {
    assert!(h.step_until(Duration::from_secs(30), |h| h.app.session.activity.list().first().is_some_and(|t| t.stalled.is_some())), "no stall");
    h.app.session.activity.list().remove(0)
}

#[test]
fn the_row_words_what_a_task_waits_for() {
    let mut t = TaskInfo {
        id: 1,
        kind: "import",
        label: "Importing".into(),
        done: 3,
        total: 25,
        unit: Unit::Count,
        detail: "IMG_0001.jpg".into(),
        cancellable: true,
        cancelling: false,
        age_ms: 600,
        waiting: false,
        place: 0,
        lane: None,
        facts: String::new(),
        stalled: Some(lightcraft_engine::activity::Stalled { what: "IMG_0457.jpg".into(), secs: 14 }),
    };
    assert_eq!(crate::panels::activity::detail_line(&t), "Waiting for IMG_0457.jpg · 14 s");
    assert!(crate::panels::activity::offers_skip(&t), "a task that waits for a file offers Skip");
    t.cancelling = true;
    assert_eq!(crate::panels::activity::detail_line(&t), "Stopping…", "✕ comes first");
    assert!(!crate::panels::activity::offers_skip(&t), "no Skip once ✕ was clicked");
    (t.cancelling, t.stalled) = (false, None);
    assert_eq!(crate::panels::activity::detail_line(&t), "3 of 25 · IMG_0001.jpg");
    assert!(!crate::panels::activity::offers_skip(&t), "no Skip while nothing is waited for");
}

/// Given an import with a file whose read does not answer,
/// When the row shows that file and Skip is clicked,
/// Then that file counts as not readable and the other four are added.
#[test]
fn skip_on_a_stalled_import_row_fails_that_file_and_goes_on() {
    let (mut h, dir, gate) = hanging_import("skip");
    crate::import::start_paths(&mut h.app, vec![dir.to_string_lossy().to_string()]).unwrap();
    let row = stalled_row(&mut h);
    assert_eq!(row.kind, "import");
    assert_eq!(row.stalled.as_ref().map(|s| s.what.as_str()), Some("IMG_hang.jpg"));
    open_list(&mut h);
    assert!(h.step_until(T, |h| has(h, &format!("activity:skip:{}", row.id))), "the row offers Skip");
    assert!(has(&h, &format!("activity:cancel:{}", row.id)), "next to ✕");
    click(&mut h, &format!("activity:skip:{}", row.id));
    assert!(h.step_until(Duration::from_secs(30), |h| h.app.import.is_none()), "the import goes on");
    gate.open();
    assert_eq!(h.app.session.catalog.len(), 4, "the other files are added");
    assert!(
        h.app.ui.toast.as_ref().is_some_and(|t| t.text.contains("Imported 4 photos") && t.text.contains("1 not readable")),
        "{:?}",
        h.app.ui.toast
    );
    assert!(h.app.session.activity.list().is_empty());
    let _ = std::fs::remove_dir_all(&dir);
}

/// Given an import whose read does not answer,
/// When ✕ is pressed,
/// Then the import ends within a second or two, "Stopping…" does not last.
#[test]
fn cancel_ends_an_import_whose_read_hangs() {
    let (mut h, dir, gate) = hanging_import("cancel-hang");
    crate::import::start_paths(&mut h.app, vec![dir.to_string_lossy().to_string()]).unwrap();
    let row = stalled_row(&mut h);
    h.app.session.activity.cancel(row.id).unwrap();
    let pressed = std::time::Instant::now();
    assert!(h.step_until(Duration::from_secs(10), |h| h.app.import.is_none()), "still stopping");
    assert!(pressed.elapsed() < Duration::from_millis(2500), "took {:?}", pressed.elapsed());
    gate.open();
    assert!(h.app.ui.toast.as_ref().is_some_and(|t| t.text.starts_with("Import cancelled")), "{:?}", h.app.ui.toast);
    assert!(h.app.session.activity.list().is_empty());
    let _ = std::fs::remove_dir_all(&dir);
}

/// `activity.skip` through the control channel: an unknown id is an error, and for the stalled import it skips the
/// file.
#[test]
fn activity_skip_command_skips_the_stalled_file() {
    let (mut h, dir, gate) = hanging_import("skip-command");
    assert_eq!(h.request("engine.execute", json!({"command": "activity.skip", "params": {"id": 999}}), T)["ok"], false);
    crate::import::start_paths(&mut h.app, vec![dir.to_string_lossy().to_string()]).unwrap();
    let row = stalled_row(&mut h);
    let listed = h.request("engine.execute", json!({"command": "activity.list", "params": {}}), T);
    assert_eq!(listed["result"]["tasks"][0]["stalled"]["what"], "IMG_hang.jpg", "{listed}");
    let r = h.request("engine.execute", json!({"command": "activity.skip", "params": {"id": row.id}}), T);
    assert_eq!(r["ok"], true, "{r}");
    assert!(h.step_until(Duration::from_secs(30), |h| h.app.import.is_none()));
    gate.open();
    assert_eq!(h.app.session.catalog.len(), 4);
    assert!(h.app.ui.toast.as_ref().is_some_and(|t| t.text.contains("1 not readable")), "{:?}", h.app.ui.toast);
    let _ = std::fs::remove_dir_all(&dir);
}

/// The text of the toast on screen, if any.
fn toast_text(h: &Headless) -> String {
    h.app.ui.toast.as_ref().map(|t| t.text.clone()).unwrap_or_default()
}

/// Given four files of an import that never answer and four that do,
/// When each hung file is skipped as the row names it,
/// Then at the fourth the import ends by itself with an error that the drive is not answering, the files readied
/// before it stay, and its row goes.
#[test]
fn an_import_ends_with_an_error_at_the_fourth_skipped_read() {
    let (mut h, dir, gate) = hanging("cap", &["IMG_hang0.jpg", "IMG_hang1.jpg", "IMG_hang2.jpg", "IMG_hang3.jpg"]);
    crate::import::start_paths(&mut h.app, vec![dir.to_string_lossy().to_string()]).unwrap();
    let ended = h.step_until(Duration::from_secs(60), |h| {
        if let Some(row) = h.app.session.activity.list().first()
            && row.stalled.is_some()
        {
            let _ = h.app.session.activity.skip(row.id);
        }
        h.app.import.is_none()
    });
    gate.open();
    assert!(ended, "the import never ended");
    let toast = toast_text(&h);
    assert!(toast.contains("Import failed:") && toast.contains("not answering") && toast.contains("IMG_hang"), "{toast}");
    assert!(h.app.session.activity.list().is_empty(), "the row goes");
    assert!(h.app.session.catalog.len() <= 4, "no more than the files that answered");
    let _ = std::fs::remove_dir_all(&dir);
}

/// Given the import review's scan of a folder with a file that never answers,
/// When its row names the file and Skip is clicked,
/// Then the review opens and lists that file as unreadable, with the skip as its reason.
#[test]
fn skip_on_the_review_scans_row_lists_the_file_as_unreadable() {
    let (mut h, dir, gate) = hanging_import("scan-skip");
    crate::import::open(&mut h.app, vec![dir.to_string_lossy().to_string()]).unwrap();
    let row = stalled_row(&mut h);
    assert_eq!(row.kind, "scan", "{row:?}");
    assert_eq!(row.stalled.as_ref().map(|s| s.what.as_str()), Some("IMG_hang.jpg"));
    open_list(&mut h);
    assert!(h.step_until(T, |h| has(h, &format!("activity:skip:{}", row.id))), "the row offers Skip");
    click(&mut h, &format!("activity:skip:{}", row.id));
    assert!(h.step_until(Duration::from_secs(30), |h| h.app.scan.is_none()), "the scan goes on");
    gate.open();
    let Some(crate::state::Dialog::Import { opts }) = &h.app.ui.dialog else { panic!("no review: {:?}", h.app.ui.dialog.is_some()) };
    let hung = opts.candidates.iter().find(|c| c.name == "IMG_hang.jpg").expect("the file is listed");
    assert!(hung.error.as_deref().is_some_and(|e| e.starts_with("skipped: no answer after ")), "{hung:?}");
    assert_eq!(opts.candidates.iter().filter(|c| c.error.is_none()).count(), 4, "the others were read");
    let _ = std::fs::remove_dir_all(&dir);
}

/// The same for the Local view's reading of a folder (the browse): its scan reads no files (PERF-CULL-1), the import
/// that follows reads them, so that import's row names the file and offers Skip, and the import goes on with the others.
#[test]
fn skip_on_the_browse_scans_row_goes_on_with_the_other_files() {
    let (mut h, dir, gate) = hanging_import("browse-skip");
    crate::import::browse(&mut h.app, &dir.to_string_lossy(), None).unwrap();
    let row = stalled_row(&mut h);
    assert_eq!(row.kind, "import", "{row:?}");
    assert_eq!(row.stalled.as_ref().map(|s| s.what.as_str()), Some("IMG_hang.jpg"));
    let r = h.request("engine.execute", json!({"command": "activity.skip", "params": {"id": row.id}}), T);
    assert_eq!(r["ok"], true, "{r}");
    assert!(h.step_until(Duration::from_secs(30), |h| h.app.scan.is_none() && h.app.import.is_none()));
    gate.open();
    assert!(h.app.session.catalog.len() >= 4, "the files that answered are in Local: {}", h.app.session.catalog.len());
    let _ = std::fs::remove_dir_all(&dir);
}

/// Given the Local view's reading of a folder (the import after the browse's scan) whose files stop answering one
/// after another,
/// When the fourth is skipped,
/// Then an error toast says the drive is not answering.
#[test]
fn a_browse_that_stops_at_the_cap_says_so() {
    let (mut h, dir, gate) = hanging("browse-cap", &["IMG_hang0.jpg", "IMG_hang1.jpg", "IMG_hang2.jpg", "IMG_hang3.jpg"]);
    crate::import::browse(&mut h.app, &dir.to_string_lossy(), None).unwrap();
    let ended = h.step_until(Duration::from_secs(60), |h| {
        if let Some(row) = h.app.session.activity.list().first()
            && row.stalled.is_some()
        {
            let _ = h.app.session.activity.skip(row.id);
        }
        h.app.scan.is_none() && h.app.import.is_none() && h.app.ui.toast.is_some()
    });
    gate.open();
    assert!(ended, "no toast: {:?}", toast_text(&h));
    let toast = toast_text(&h);
    assert!(toast.starts_with("Import failed:") && toast.contains("not answering"), "{toast}");
    let _ = std::fs::remove_dir_all(&dir);
}

/// Given the import review's scan of a folder whose files stop answering one after another, and another dialog open
/// when the scan ends (so the review waits, and a toast made then would be gone by the time it opens),
/// When that dialog is closed and the review opens,
/// Then an error toast says the folder was read only as far as the drive answered.
#[test]
fn a_review_that_waits_for_a_dialog_says_the_scan_stopped_when_it_opens() {
    let (mut h, dir, gate) = hanging("review-cap", &["IMG_hang0.jpg", "IMG_hang1.jpg", "IMG_hang2.jpg", "IMG_hang3.jpg"]);
    crate::import::open(&mut h.app, vec![dir.to_string_lossy().to_string()]).unwrap();
    h.app.ui.dialog = Some(crate::state::Dialog::WhatsNew);
    let ended = h.step_until(Duration::from_secs(60), |h| {
        if let Some(row) = h.app.session.activity.list().first()
            && row.stalled.is_some()
        {
            let _ = h.app.session.activity.skip(row.id);
        }
        h.app.scan.is_none()
    });
    gate.open();
    assert!(ended, "the scan never ended");
    // (a toast made at the scan's end, if there was one, is over by now)
    h.app.ui.toast = None;
    h.step();
    assert!(h.app.review_pending.is_some(), "the review waits for the open dialog");
    h.app.ui.dialog = None;
    h.step();
    assert!(matches!(h.app.ui.dialog, Some(crate::state::Dialog::Import { .. })), "the review opened");
    let toast = toast_text(&h);
    assert!(toast.starts_with("Stopped:") && toast.contains("not answering"), "{toast}");
    let _ = std::fs::remove_dir_all(&dir);
}

/// What `services.keep_awake` was told, in order.
type Told = std::rc::Rc<std::cell::RefCell<Vec<Option<String>>>>;

fn demo_keeping_awake() -> (Headless, Told) {
    let told = Told::default();
    let sink = told.clone();
    let keep_awake: crate::KeepAwakeFn = Box::new(move |reason| sink.borrow_mut().push(reason.map(str::to_owned)));
    let app = LightcraftApp::new(lightcraft_engine::Session::with_demo(), Services { png: None, keep_awake: Some(keep_awake), ..Default::default() });
    let mut h = Headless::new(app, [1200.0, 800.0], 1.0);
    h.settle(T);
    told.borrow_mut().clear();
    (h, told)
}

#[test]
fn a_running_job_is_told_to_the_host_once_and_its_end_too() {
    let (mut h, told) = demo_keeping_awake();
    for _ in 0..3 {
        h.step();
    }
    assert!(told.borrow().is_empty(), "nothing runs: the host hears nothing");

    let g = h.app.session.activity.start("export", "Exporting", Cancel::Yes);
    for _ in 0..4 {
        h.step();
    }
    assert_eq!(*told.borrow(), [Some("Exporting".to_string())], "told once, not on every frame");

    drop(g);
    for _ in 0..4 {
        h.step();
    }
    assert_eq!(*told.borrow(), [Some("Exporting".to_string()), None]);
}

#[test]
fn a_cancelled_job_is_released_before_it_stops() {
    let (mut h, told) = demo_keeping_awake();
    let g = h.app.session.activity.start("export", "Exporting", Cancel::Yes);
    h.step();
    h.app.session.activity.cancel(g.id()).unwrap();
    h.step();
    assert_eq!(*told.borrow(), [Some("Exporting".to_string()), None]);
}

#[test]
fn a_window_with_no_host_service_ignores_jobs() {
    let mut h = demo();
    let _g = h.app.session.activity.start("export", "Exporting", Cancel::Yes);
    h.step();
    assert_eq!(h.app.awake_told, None);
}

/// What `services.dock_badge` was told, in order.
type Badges = std::rc::Rc<std::cell::RefCell<Vec<usize>>>;

fn demo_with_dock_badge() -> (Headless, Badges) {
    let badges = Badges::default();
    let sink = badges.clone();
    let dock_badge: crate::DockBadgeFn = Box::new(move |count| sink.borrow_mut().push(count));
    let app = LightcraftApp::new(lightcraft_engine::Session::with_demo(), Services { png: None, dock_badge: Some(dock_badge), ..Default::default() });
    let mut h = Headless::new(app, [1200.0, 800.0], 1.0);
    h.settle(T);
    badges.borrow_mut().clear();
    (h, badges)
}

#[test]
fn the_dock_badge_follows_the_number_of_running_jobs_and_clears_when_idle() {
    let (mut h, badges) = demo_with_dock_badge();
    for _ in 0..3 {
        h.step();
    }
    assert!(badges.borrow().is_empty(), "nothing runs: the host hears nothing");

    let export = h.app.session.activity.start("export", "Exporting", Cancel::Yes);
    h.step();
    let import = h.app.session.activity.start("import", "Importing", Cancel::Yes);
    h.step();
    h.step();
    assert_eq!(*badges.borrow(), [1, 2], "told on each change, not on every frame");

    drop(export);
    h.step();
    drop(import);
    h.step();
    assert_eq!(*badges.borrow(), [1, 2, 1, 0]);
}

#[test]
fn a_job_asked_to_stop_stays_in_the_dock_count_until_it_has_stopped() {
    let (mut h, badges) = demo_with_dock_badge();
    let g = h.app.session.activity.start("export", "Exporting", Cancel::Yes);
    h.step();
    h.app.session.activity.cancel(g.id()).unwrap();
    h.step();
    assert_eq!(*badges.borrow(), [1]);
    drop(g);
    h.step();
    assert_eq!(*badges.borrow(), [1, 0]);
}

/// What `services.notify` was shown, in order.
type Notices = std::rc::Rc<std::cell::RefCell<Vec<crate::Notice>>>;

fn demo_with_notifications() -> (Headless, Notices) {
    let shown = Notices::default();
    let sink = shown.clone();
    let notify: crate::NotifyFn = Box::new(move |n| sink.borrow_mut().push(n.clone()));
    let app = LightcraftApp::new(lightcraft_engine::Session::with_demo(), Services { png: None, notify: Some(notify), ..Default::default() });
    let mut h = Headless::new(app, [1200.0, 800.0], 1.0);
    h.settle(T);
    shown.borrow_mut().clear();
    (h, shown)
}

/// A job runs in front, then another app takes the keyboard (the window reports it unfocused) and the job ends.
fn end_in_the_background(h: &mut Headless, kind: &'static str, label: &str) {
    let g = h.app.session.activity.start(kind, label, Cancel::Yes);
    h.step();
    h.step();
    h.window_focused = false;
    drop(g);
    h.step();
}

#[test]
fn a_job_that_ends_in_the_background_is_announced() {
    let (mut h, shown) = demo_with_notifications();
    end_in_the_background(&mut h, "export", "Exporting");
    assert!(h.step_until(T, |_| !shown.borrow().is_empty()), "the announcement is made once the toast had its moment");
    let shown = shown.borrow();
    assert_eq!(shown.len(), 1, "{shown:?}");
    assert_eq!((shown[0].title.as_str(), shown[0].body.as_str()), ("LightCraft", "Exporting finished"));
}

#[test]
fn the_toast_that_reports_the_end_is_what_the_notification_says() {
    let (mut h, shown) = demo_with_notifications();
    let g = h.app.session.activity.start("export", "Exporting", Cancel::Yes);
    h.step();
    h.step();
    h.window_focused = false;
    drop(g);
    h.app.toast_for(&h.view.ctx.clone(), "Exported 24 photos", 15.0);
    assert!(h.step_until(T, |_| !shown.borrow().is_empty()));
    assert_eq!(shown.borrow()[0].body, "Exported 24 photos");
}

#[test]
fn a_job_that_ends_while_the_window_is_in_front_is_not_announced() {
    let (mut h, shown) = demo_with_notifications();
    let g = h.app.session.activity.start("export", "Exporting", Cancel::Yes);
    h.step();
    h.step();
    drop(g);
    // (the simulated clock runs well past the grace the announcement would wait for)
    for _ in 0..120 {
        h.step();
    }
    assert!(shown.borrow().is_empty(), "{:?}", shown.borrow());
}

/// Frames enough for the simulated clock to run well past the grace an announcement would wait for.
fn past_the_grace(h: &mut Headless) {
    for _ in 0..120 {
        h.step();
    }
}

#[test]
fn a_job_that_was_asked_to_stop_makes_no_notification() {
    let (mut h, shown) = demo_with_notifications();
    let g = h.app.session.activity.start("export", "Exporting", Cancel::Yes);
    h.step();
    h.app.session.activity.cancel(g.id()).unwrap();
    h.step();
    h.window_focused = false;
    drop(g);
    h.step();
    past_the_grace(&mut h);
    assert!(shown.borrow().is_empty(), "{:?}", shown.borrow());
}

#[test]
fn work_that_starts_by_itself_makes_no_notification() {
    let (mut h, shown) = demo_with_notifications();
    end_in_the_background(&mut h, "cache", "Tidying the thumbnail cache");
    past_the_grace(&mut h);
    assert!(shown.borrow().is_empty(), "{:?}", shown.borrow());
}

#[test]
fn with_the_setting_off_a_job_that_ends_in_the_background_makes_no_notification() {
    let (mut h, shown) = demo_with_notifications();
    h.app.ui.settings.notify_when_done = false;
    end_in_the_background(&mut h, "import", "Importing");
    past_the_grace(&mut h);
    assert!(shown.borrow().is_empty(), "{:?}", shown.borrow());
}

#[test]
fn notifications_are_on_for_settings_saved_before_they_existed() {
    let old: crate::state::AppSettings = serde_json::from_value(json!({"libraryPath": "/somewhere"})).unwrap();
    assert!(old.notify_when_done);
    assert_eq!(serde_json::to_value(&old).unwrap()["notifyWhenDone"], true);
}
