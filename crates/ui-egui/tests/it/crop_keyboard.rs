//! The Crop panel's rows of radio buttons (the overlay, the upright mode) under the keyboard: each
//! segment is a Tab stop of its own, so Tab leaves the row from its last member, and the arrow keys
//! and Home, which only move the focus there, change neither the overlay, the photo nor the crop.
use egui::accesskit::Role;
use egui::vec2;
use egui_kittest::{Harness, kittest::NodeT};
use lightcraft_ui_egui::{
    LightcraftApp, Services,
    state::{RightPanel, ViewMode},
};

type App = Harness<'static, LightcraftApp>;

fn crop() -> App {
    let mut app = LightcraftApp::new(lightcraft_engine::Session::with_demo(), Services { png: None, ..Default::default() });
    // a headless host never gains the first-run tour: its modal would take the Tab focus under test
    app.headless_host = true;
    app.ui.left_panel = true;
    app.ui.view = ViewMode::Detail;
    app.ui.right = RightPanel::Crop;
    let mut builder = Harness::builder().with_size(vec2(1500.0, 950.0)).with_accessibility_check(false);
    if lightcraft_engine::CRAFT_FONTS.is_empty() {
        builder = builder.allow_missing_glyphs();
    }
    let mut h = builder.build_ui_state(
        |ui, app: &mut LightcraftApp| {
            app.logic(&ui.ctx().clone());
            app.ui(ui);
        },
        app,
    );
    h.run_steps(10);
    h
}

/// The role and name of the control with the focus.
fn focused(h: &App) -> Option<(Role, String)> {
    let id = h.ctx.memory(|m| m.focused())?.value();
    let node = h.root().children_recursive().map(|n| n.accesskit_node()).find(|n| n.locate().0.0 == id)?;
    Some((node.role(), node.label().unwrap_or_default().to_string()))
}

/// Gives the focus to the overlay radio button named `name`.
fn focus_overlay(h: &mut App, name: &str) {
    let node = h
        .root()
        .children_recursive()
        .find(|n| n.accesskit_node().role() == Role::RadioButton && n.accesskit_node().label().as_deref() == Some(name))
        .unwrap_or_else(|| panic!("no radio button named {name:?}"));
    node.focus();
    h.run_steps(3);
    assert_eq!(focused(h), Some((Role::RadioButton, name.to_string())), "the focus did not go to {name:?}");
}

#[test]
fn tab_leaves_the_overlay_row_from_its_last_member() {
    let mut h = crop();
    focus_overlay(&mut h, "None");
    h.key_press(egui::Key::Tab);
    h.run_steps(1);
    let (_, name) = focused(&h).expect("Tab keeps the focus somewhere");
    assert!(
        !["Thirds", "Grid", "Golden", "Diagonal", "Triangle", "Spiral", "None"].contains(&name.as_str()),
        "Tab stays in the overlay row: it went to {name:?}"
    );
}

#[test]
fn tab_visits_every_member_of_the_overlay_row() {
    let mut h = crop();
    focus_overlay(&mut h, "Thirds");
    let mut seen = vec!["Thirds".to_string()];
    for _ in 0..6 {
        h.key_press(egui::Key::Tab);
        h.run_steps(1);
        seen.push(focused(&h).expect("Tab keeps the focus somewhere").1);
    }
    assert_eq!(seen, ["Thirds", "Grid", "Golden", "Diagonal", "Triangle", "Spiral", "None"]);
}

#[test]
fn home_and_the_arrow_keys_in_the_overlay_row_change_nothing_but_the_focus() {
    let mut h = crop();
    let (photo, overlay) = (h.state().session.active(), h.state().ui.crop_overlay);
    focus_overlay(&mut h, "Grid");
    for key in [egui::Key::Home, egui::Key::ArrowRight, egui::Key::ArrowRight, egui::Key::ArrowLeft, egui::Key::ArrowDown, egui::Key::ArrowUp] {
        h.key_press(key);
        h.run_steps(2);
        assert_eq!((h.state().session.active(), h.state().ui.crop_overlay), (photo, overlay), "{key:?} changed the photo or the overlay");
    }
}
