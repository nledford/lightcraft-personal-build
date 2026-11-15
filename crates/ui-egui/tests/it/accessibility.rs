//! The accessibility tests that are short: a few screens each, opened through the shared harness (`a11y_support`). The two
//! whole-app audits, which walk every screen of the app, run as a binary of their own (`tests/a11y_audit`), so they
//! do not share a small runner's cores with these.
use egui::accesskit::Role;
use egui::vec2;
use egui_kittest::{Harness, kittest::NodeT};
use lightcraft_ui_egui::{
    LightcraftApp, Services,
    state::{RightPanel, ViewMode},
};

use crate::a11y_support::harness;

fn slider_value(h: &Harness<'_, LightcraftApp>, name: &str) -> Option<f64> {
    h.root()
        .children_recursive()
        .map(|n| n.accesskit_node())
        .find(|n| n.role() == Role::Slider && n.label().as_deref() == Some(name))
        .and_then(|n| n.numeric_value())
}

/// A screen reader's Increment on a develop slider moves it one step, as an arrow key does.
#[test]
fn screen_reader_increment_moves_a_slider() {
    let mut h = harness(ViewMode::Detail, RightPanel::Edit);
    let before = slider_value(&h, "Exposure").expect("Exposure is in the tree");
    let target = h
        .root()
        .children_recursive()
        .find(|n| n.accesskit_node().role() == Role::Slider && n.accesskit_node().label().as_deref() == Some("Exposure"))
        .unwrap()
        .accesskit_node()
        .locate();
    h.event(egui::Event::AccessKitActionRequest(egui::accesskit::ActionRequest {
        action: egui::accesskit::Action::Increment,
        target_node: target.0,
        target_tree: target.1,
        data: None,
    }));
    h.run_steps(4);
    let after = slider_value(&h, "Exposure").unwrap();
    assert!(after > before, "{before} -> {after}");
}

/// The canvas tools' settings (crop angle and offsets, spot size, mask amount) are reachable with
/// Tab alone, and the arrow keys then adjust them.
#[test]
fn canvas_tool_sliders_are_reachable_with_tab() {
    for right in [RightPanel::Crop, RightPanel::Remove] {
        let mut h = harness(ViewMode::Detail, right);
        let mut reached = None;
        for _ in 0..300 {
            h.key_press(egui::Key::Tab);
            h.run_steps(1);
            let focused = h.root().children_recursive().map(|n| n.accesskit_node()).find(|n| n.is_focused() && n.role() == Role::Slider);
            if let Some(n) = focused {
                reached = Some((n.label().unwrap_or_default(), n.numeric_value()));
                break;
            }
        }
        let (name, before) = reached.unwrap_or_else(|| panic!("{right:?}: no slider takes the focus with Tab"));
        h.key_press(egui::Key::ArrowUp);
        h.run_steps(4);
        let after = slider_value(&h, &name);
        assert_ne!(before, after, "{right:?}: ArrowUp on the Tab-focused {name:?}");
    }
}

/// The canvas handles are in the tree by name, with their position as the value.
#[test]
fn canvas_handles_are_named_with_their_position() {
    let h = harness(ViewMode::Detail, RightPanel::Crop);
    let found: Vec<(String, String)> = h
        .root()
        .children_recursive()
        .map(|n| n.accesskit_node())
        .filter(|n| n.role() == Role::Button)
        .map(|n| (n.label().unwrap_or_default(), n.value().unwrap_or_default()))
        .collect();
    for name in ["Crop top-left corner", "Crop right edge", "Crop box"] {
        assert!(found.iter().any(|(l, v)| l == name && v.contains('%')), "{name}: {found:?}");
    }
    let h = harness(ViewMode::Detail, RightPanel::Remove);
    assert!(h.root().children_recursive().any(|n| n.accesskit_node().label().is_some_and(|l| l.starts_with("New spot"))));
}

/// Tab walks each right panel from top to bottom (rows left to right): the order a sighted user
/// reads it in. Each step down may not jump back up by more than a row.
#[test]
fn tab_walks_the_right_panels_top_to_bottom() {
    for right in [RightPanel::Edit, RightPanel::Crop, RightPanel::Remove, RightPanel::Masking] {
        let mut h = harness(ViewMode::Detail, right);
        let panel_left = 1500.0 - h.state().ui.right_width - 10.0;
        // the tool strip beside the panel is a column of its own, walked before it
        let strip = h
            .root()
            .children_recursive()
            .map(|n| n.accesskit_node())
            .find(|n| n.label().as_deref() == Some("Edit") && n.description().as_deref() == Some("E"))
            .and_then(|n| n.bounding_box())
            .map(|b| b.x0 as f32)
            .unwrap_or(f32::INFINITY);
        let mut seen: Vec<(String, f32)> = Vec::new();
        for _ in 0..120 {
            h.key_press(egui::Key::Tab);
            h.run_steps(1);
            let Some(n) = h.root().children_recursive().map(|n| n.accesskit_node()).find(|n| n.is_focused()) else { continue };
            let Some(b) = n.bounding_box() else { continue };
            if (b.x0 as f32) < panel_left || (b.x0 as f32) >= strip - 1.0 {
                continue;
            }
            let name = n.label().unwrap_or_default();
            if seen.iter().any(|(s, y)| *s == name && (*y - b.y0 as f32).abs() < 1.0) && !name.is_empty() {
                break;
            }
            seen.push((name, b.y0 as f32));
        }
        assert!(seen.len() > 3, "{right:?}: Tab reaches the panel: {seen:?}");
        for w in seen.windows(2) {
            assert!(w[1].1 >= w[0].1 - 30.0, "{right:?}: Tab jumps up from {:?} to {:?}\n{seen:?}", w[0], w[1]);
        }
    }
}

/// A slider's value field is "<slider> value" (the instruction to click, type or drag is its
/// description, not part of the name VoiceOver reads on every visit).
#[test]
fn a_slider_value_field_is_named_value_and_describes_how_to_use_it() {
    const INSTRUCTION: &str = "Click to type a value, drag to change it";
    let h = harness(ViewMode::Detail, RightPanel::Edit);
    let buttons: Vec<_> = h.root().children_recursive().filter(|n| n.accesskit_node().role() == Role::Button).collect();
    let names: Vec<String> = buttons.iter().filter_map(|n| n.accesskit_node().label()).collect();
    let field = buttons
        .iter()
        .find(|n| n.accesskit_node().label().as_deref() == Some("Exposure value"))
        .unwrap_or_else(|| panic!("a value field named \"Exposure value\": {names:?}"));
    assert_eq!(field.accesskit_node().description().as_deref(), Some(INSTRUCTION), "the instruction is its description");
    assert!(!names.iter().any(|n| n.contains(INSTRUCTION)), "no name carries the instruction: {names:?}");
}

/// Whether a button named `name` has `help` as its accessible description (the menu bar has an
/// "Edit" menu too, so the name alone doesn't pick the tool-strip button). The bottom bar's view
/// buttons are radio buttons (#3259: one of several choices), the rest plain buttons.
fn button_described(h: &Harness<'_, LightcraftApp>, name: &str, help: &str) -> bool {
    h.root().children_recursive().map(|n| n.accesskit_node()).any(|n| {
        matches!(n.role(), Role::Button | Role::RadioButton) && n.label().as_deref() == Some(name) && n.description().as_deref() == Some(help)
    })
}

/// A screen reader reads a control's name every time it lands on it: the shortcut belongs in the
/// description (macOS AXHelp, read after a pause), not in the name.
#[test]
fn tool_strip_and_toolbar_buttons_are_named_by_label_with_the_shortcut_as_description() {
    let h = harness(ViewMode::Detail, RightPanel::Edit);
    for (name, help) in [
        ("Presets", "Shift+P"),
        ("Edit", "E"),
        ("Crop & Rotate", "C"),
        ("Masking", "M"),
        ("Versions", "Shift+V"),
        // no "History & Activity" row: panel.activity has no key of its own (Y is Before/After's,
        // checked below); the strip button says no key, as it has since the menu row rework
        ("Keywords", "K"),
        ("Info", "I"),
        ("Photo Grid", "G"),
        ("Square Grid", "G"),
        ("Detail", "D"),
        ("Compare", "Shift+C"),
        ("Survey", "N"),
        ("Before/After", "Y"),
        ("Filmstrip", "/"),
    ] {
        assert!(button_described(&h, name, help), "a button named {name:?} describing {help:?}");
    }
    // the strip's History & Activity button no longer shows the Y Before/After owns
    let activity = h
        .root()
        .children_recursive()
        .map(|n| n.accesskit_node())
        .find(|n| n.role() == Role::Button && n.label().as_deref() == Some("History & Activity"))
        .expect("a button named \"History & Activity\"");
    assert_ne!(activity.description().as_deref(), Some("Y"), "Before/After's Y is not its key");
    let leaked: Vec<String> = h
        .root()
        .children_recursive()
        .map(|n| n.accesskit_node())
        .filter(|n| matches!(n.role(), Role::Button | Role::RadioButton))
        .filter_map(|n| n.label())
        .filter(|l| ["Edit (E)", "Presets (Shift+P)", "Photo Grid (G)", "Filmstrip (/)"].contains(&l.as_str()))
        .collect();
    assert!(leaked.is_empty(), "shortcuts left in names: {leaked:?}");
}

/// The top bar's Import button carries the key it is bound to now, written for this platform.
#[test]
fn import_button_describes_its_shortcut_as_this_platform_writes_it() {
    let h = harness(ViewMode::PhotoGrid, RightPanel::Edit);
    let mac = h.ctx.os() == egui::os::OperatingSystem::Mac;
    let sc = lightcraft_ui_egui::shortcuts::shortcut_of(&h.state().ui.settings.keymap, "file.addPhotos").expect("Import Photos… has a shortcut");
    let help = lightcraft_ui_egui::menubar::shortcut_text(sc, mac);
    assert!(button_described(&h, "Import Photos…", &help), "a button named \"Import Photos…\" describing {help:?}");
}

/// What a screen reader is told about the Settings tab strip: a tab list holding one tab per
/// section, the open one selected, and the open tab controlling the page under it. VoiceOver
/// read the tabs as check boxes (a button with a toggled state) before.
#[test]
fn the_settings_tab_strip_is_a_tab_list_of_tabs_that_control_their_page() {
    let tabs = lightcraft_ui_egui::panels::settings::TABS;
    let mut h = harness(ViewMode::PhotoGrid, RightPanel::None);
    h.state_mut().run("app.settings", serde_json::json!({"tab": "slideshow"})).expect("Settings opens");
    h.run_steps(8);
    let nodes: Vec<_> = h.root().children_recursive().map(|n| n.accesskit_node()).collect();
    // each node with its ancestors (id, role), nearest first
    let with_ancestors: Vec<_> = nodes
        .iter()
        .map(|n| {
            let mut up = n.parent();
            let mut chain = Vec::new();
            while let Some(p) = up {
                chain.push((p.id(), p.role()));
                up = p.parent();
            }
            (n, chain)
        })
        .collect();
    let lists: Vec<_> = nodes.iter().filter(|n| n.role() == Role::TabList).collect();
    assert_eq!(lists.len(), 1, "one tab list: {:?}", lists.iter().map(|n| n.label()).collect::<Vec<_>>());
    let list = lists[0];
    assert!(list.label().is_some_and(|l| !l.trim().is_empty()), "the tab list has a name");
    // one tab per section, in order, each under the list through nothing but plain containers
    // (macOS lists the tabs from the list's children, looking through those alone)
    let found: Vec<_> = with_ancestors
        .iter()
        .filter(|(n, chain)| {
            n.role() == Role::Tab
                && chain.iter().position(|(id, _)| *id == list.id()).is_some_and(|at| chain[..at].iter().all(|(_, r)| *r == Role::GenericContainer))
        })
        .map(|(n, _)| n)
        .collect();
    let names: Vec<String> = found.iter().map(|n| n.label().unwrap_or_default()).collect();
    let want: Vec<String> =
        tabs.iter().map(|(id, l)| lightcraft_ui_egui::i18n::tr(lightcraft_ui_egui::panels::settings::tab_label(id, l)).to_string()).collect();
    assert_eq!(names, want, "one tab per section");
    // the open one, and only it, is selected, and it is no toggle (a check box to VoiceOver)
    for (tab, (id, _)) in found.iter().zip(tabs) {
        assert_eq!(tab.is_selected(), Some(*id == "slideshow"), "{:?} selected", tab.label());
        assert!(tab.toggled().is_none(), "{:?} is no toggle", tab.label());
    }
    // the open tab, and no other, controls a tab panel named for it
    let open = found[tabs.iter().position(|(id, _)| *id == "slideshow").expect("a Slideshow tab")];
    let panel: Vec<_> = open.controls().collect();
    assert_eq!(panel.len(), 1, "the open tab controls its page");
    assert_eq!(panel[0].role(), Role::TabPanel);
    assert_eq!(panel[0].label(), open.label());
    assert!(found.iter().filter(|t| t.controls().next().is_some()).count() == 1, "only the open tab controls a page");
    // the page is in the panel: its controls, and Slideshow's slide preview pinned above them
    let in_panel = |name: &str| {
        with_ancestors
            .iter()
            .any(|(n, chain)| n.label().as_deref() == Some(name) && n.role() != Role::TabPanel && chain.iter().any(|(id, _)| *id == panel[0].id()))
    };
    assert!(in_panel("Intro"), "the preview's Photo / Intro / Ending choice is in the page's panel");
    assert!(
        with_ancestors.iter().filter(|(n, chain)| n.role() == Role::CheckBox && chain.iter().any(|(id, _)| *id == panel[0].id())).count() > 0,
        "the page's own controls are in the panel"
    );
}
/// Given a filter that hides every photo of All Photos, when the grid is read by a screen reader,
/// then the painted title is a heading and the count and the empty-state message are text nodes.
#[test]
fn the_grid_title_count_and_empty_message_are_read_when_a_filter_hides_every_photo() {
    let mut app = LightcraftApp::new(lightcraft_engine::Session::with_demo(), Services { png: None, ..Default::default() });
    app.ui.view = ViewMode::PhotoGrid;
    app.run("library.filter", serde_json::json!({"text": "no photo is called this"})).unwrap();
    let mut h = Harness::builder().with_size(vec2(1500.0, 950.0)).build_ui_state(
        |ui, app: &mut LightcraftApp| {
            app.logic(&ui.ctx().clone());
            app.ui(ui);
        },
        app,
    );
    h.run_steps(10);
    let nodes: Vec<(Role, String)> = h
        .root()
        .children_recursive()
        .map(|n| n.accesskit_node())
        .map(|n| (n.role(), n.label().or_else(|| n.value()).unwrap_or_default()))
        .filter(|(_, text)| !text.is_empty())
        .collect();
    let has = |role: Role, text: &str| nodes.iter().any(|(r, t)| *r == role && t == text);
    assert!(has(Role::Heading, "All Photos"), "the title is a heading: {nodes:?}");
    assert!(has(Role::Label, "0 of 24 photos"), "the count is text: {nodes:?}");
    assert!(has(Role::Label, "No photos match the active filters"), "the message is text: {nodes:?}");
    assert!(has(Role::Label, "24 photos in All Photos are hidden by the filters above"), "the reason is text: {nodes:?}");
}
