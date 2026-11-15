//! The harness of the accessibility tests, shared by the two test binaries that have them: `it`, the short tests
//! (`tests/it/accessibility.rs`), and `a11y_audit`, the two whole-app audits (`tests/a11y_audit`). It is a directory
//! with a `mod.rs` so Cargo does not take it for a test binary of its own.
use std::path::Path;

use egui::vec2;
use egui_kittest::Harness;
use lightcraft_ui_egui::{
    LightcraftApp, Services,
    state::{RightPanel, ViewMode},
};

pub type App = Harness<'static, LightcraftApp>;

/// The sidebar lists Pictures, Desktop, Downloads and Home when this computer has them, found by a
/// worker, so the controls the audit counts would follow the machine that runs it. They are hidden
/// from Local here (as a person can), and what is audited is the same everywhere.
pub fn hide_the_computers_places(app: &mut LightcraftApp) {
    let home = std::env::var("HOME").or_else(|_| std::env::var("USERPROFILE")).unwrap_or_default();
    if home.is_empty() {
        return;
    }
    for sub in ["Pictures", "Desktop", "Downloads", ""] {
        let path = if sub.is_empty() { home.clone() } else { Path::new(&home).join(sub).to_string_lossy().to_string() };
        app.ui.hidden_locations.push(path);
    }
}

/// Render workers of each harness: the library's tests use two (`TEST_WORKERS`, #3067); with the machine's pool every
/// harness of the audits' many would start up to six, and the audits starve the other tests' renders on a small runner.
pub const RENDER_WORKERS: usize = 2;

/// Run frames until the renders the screen asked for have come back (a photo the view shows is a
/// control only once its picture is there), so what the audit counts does not depend on how fast
/// the workers were.
pub fn settle_renders(h: &mut App) {
    let t0 = std::time::Instant::now();
    let mut quiet = 0;
    while quiet < 5 && t0.elapsed() < std::time::Duration::from_secs(20) {
        h.run_steps(1);
        if h.state().renderer.in_flight() == 0 {
            quiet += 1;
        } else {
            quiet = 0;
            std::thread::sleep(std::time::Duration::from_millis(2));
        }
    }
    assert!(quiet >= 5, "the renders did not finish in 20 s");
}

pub fn harness_with(setup: impl FnOnce(&mut LightcraftApp)) -> App {
    let mut app = LightcraftApp::new(lightcraft_engine::Session::with_demo(), Services { png: None, ..Default::default() });
    app.renderer = lightcraft_ui_egui::render::Renderer::with_worker_threads(RENDER_WORKERS);
    // a headless host never gains the first-run tour: its modal would take the Tab focus under test
    app.headless_host = true;
    app.ui.left_panel = true;
    hide_the_computers_places(&mut app);
    setup(&mut app);
    let mut builder = Harness::builder().with_size(vec2(1500.0, 950.0)).with_accessibility_check(false);
    // text such as "▸" in the Reference view's hint has its glyph in no face of the test fonts, craft-fonts or not (the
    // Linux run lists them and has none): it is drawn as a box, which this audit of names doesn't need
    builder = builder.allow_missing_glyphs();
    let mut h = builder.build_ui_state(
        |ui, app: &mut LightcraftApp| {
            app.logic(&ui.ctx().clone());
            app.ui(ui);
        },
        app,
    );
    h.run_steps(10);
    settle_renders(&mut h);
    h
}

pub fn harness(view: ViewMode, right: RightPanel) -> App {
    harness_with(|app| {
        app.ui.view = view;
        app.ui.right = right;
    })
}
