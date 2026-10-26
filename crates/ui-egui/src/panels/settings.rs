//! The Settings dialog (⌘,): General, Import, Performance, Display, Interface, Slideshow, Faces,
//! AI Denoise.
//!
//! Changes apply immediately (no OK/Cancel). Where they are stored:
//! - **app settings** ([`crate::state::AppSettings`]: startup view, delete confirmation, GPU,
//!   preview size, filmstrip/grid badges, last library, monitor profile, whether Import Lightroom
//!   Catalog… asks where a catalog goes) and Auto Advance live in
//!   the UI state,
//!   saved by the host in its config folder (`ui.json`);
//! - **library settings** (import defaults, XMP sidecars, thumbnail cache size, slideshow
//!   playback) go through the `library.preferences` / `library.xmpPreferences` /
//!   `slideshow.playback` commands into the library's `prefs.json`,
//!   so they travel with the library.

use egui::RichText;
use serde_json::{Value, json};

use crate::LightcraftApp;
use crate::state::{GridBadges, PREVIEW_LIMITS, StartupView};
use crate::theme::Tokens;
use crate::widgets::{register, register_geometry};

/// (id, label) of the tabs, in order.
pub const TABS: &[(&str, &str)] = &[
    ("general", "General"),
    ("import", "Import"),
    ("performance", "Performance"),
    ("display", "Display"),
    ("interface", "Interface"),
    ("slideshow", "Slideshow"),
    ("faces", "Faces"),
    ("denoise", "AI Denoise"),
];

/// Thumbnail cache sizes offered (MB).
const CACHE_SIZES: [u32; 5] = [512, 1024, 2048, 4096, 8192];

const LABEL_W: f32 = 150.0;

/// The page of `tab` (the tab bar in [`window`] switches `tab`).
pub fn body(app: &mut LightcraftApp, ui: &mut egui::Ui, tab: &str) {
    let t = Tokens::get(ui.ctx());
    match tab {
        "import" => import_tab(app, ui, &t),
        "performance" => performance_tab(app, ui, &t),
        "display" => display_tab(app, ui, &t),
        "interface" => interface_tab(app, ui, &t),
        "slideshow" => slideshow_tab(app, ui, &t, true),
        "faces" => super::faces::settings_tab(app, ui, &t),
        "denoise" => super::denoise::settings_tab(app, ui, &t),
        _ => general_tab(app, ui, &t),
    }
}

/// The widest the Settings window's content gets, and the tallest its pages get: the window is
/// one size on every tab, so it stays where it is as the tabs are gone through.
const PAGE_SIZE: egui::Vec2 = egui::vec2(560.0, 640.0);

/// The least room in which a page scrolls by itself. In a window with less (far below the
/// desktop minimum), the tabs and the page scroll together in the dialog's own scroll area.
const PAGE_MIN_HEIGHT: f32 = 120.0;

/// The share of the page's room the pinned slide preview of the Slideshow tab takes, when the page
/// has more room than it needs to keep [`PINNED_PAGE_MIN_HEIGHT`] for itself.
const PINNED_PREVIEW_SHARE: f32 = 0.4;

/// What the preview's heading, the one-line hint under the slide, its Photo / Intro / Ending
/// choice and the rule under it take around the slide (at the default spacing, about 82 pt).
const PREVIEW_CHROME: f32 = 82.0;

/// What the same takes once the page has been drawn a second time (measured, at the default
/// spacing: 104 pt). The room a pinned preview leaves for the page is checked against this, not
/// against [`PREVIEW_CHROME`], so a window just tall enough never ends up with a page below
/// its least and a dialog that grows a point on this tab.
const PREVIEW_CHROME_DRAWN: f32 = 104.0;

/// The least room the page keeps under the pinned preview: a few rows, with the filter row above
/// them in the smallest window (900 x 560) that the preview is still pinned in.
const PINNED_PAGE_MIN_HEIGHT: f32 = 100.0;

/// The Settings window's content: the tab bar, which stays put, above the page of `tab`
/// ([`body`]), which scrolls when it is taller than the room for it, with a scroll handle that
/// shows without the pointer on it. `max_height` is the room the app window leaves for both: the
/// two together never take more, so the scroll area every dialog is drawn in has nothing to
/// scroll (but for a window with less than [`PAGE_MIN_HEIGHT`] for the page, where that area
/// scrolls the two together). Every page gets the same room whatever it holds (but for the pinned
/// preview of Slideshow), and each keeps a scroll position of its own. Nothing here is wider than the window from 640 pt of app window
/// up: a row's controls wrap ([`row`]) and paths are shortened ([`path_label`]).
/// On Slideshow the slide preview is pinned between the tab bar and the page (where the room
/// allows), and comes off that page's room.
pub fn window(app: &mut LightcraftApp, ui: &mut egui::Ui, tab: &mut String, max_height: f32) {
    ui.set_width((ui.ctx().content_rect().width() - 64.0).clamp(240.0, PAGE_SIZE.x));
    let top = ui.cursor().top();
    // The filter, above the tabs it looks through. What is typed is kept from one frame of the
    // window to the next only: Settings opened again starts with it empty.
    let filter_id = egui::Id::unique("settings-filter");
    let pass = ui.ctx().cumulative_pass_nr();
    let kept: Option<Filter> = ui.data(|d| d.get_temp(filter_id));
    // (Settings shown anew: the filter starts empty, and nothing is marked as landed on)
    let opened = !kept.as_ref().is_some_and(|k| k.pass.checked_add(1) == Some(pass));
    if opened {
        ui.data_mut(|d| d.remove::<(String, f64)>(egui::Id::unique(LANDED)));
    }
    let mut filter = kept.filter(|_| !opened).unwrap_or_default();
    filter.pass = pass;
    // (the mark is for arriving at a section from the filter: a tab left, however, and come back
    // to shows none. Arriving sets it again when the heading is drawn, further down this frame.)
    if filter.tab != *tab {
        ui.data_mut(|d| d.remove::<(String, f64)>(egui::Id::unique(LANDED)));
    }
    filter.tab.clone_from(tab);
    let had_text = !filter.text.is_empty();
    let typed = crate::text_field::TextField::singleline("field:settingsFilter", &mut filter.text)
        .hint(crate::i18n::tr("Filter settings"))
        .width(ui.available_width())
        .show(ui);
    let filter_row = ui.cursor().top() - top;
    // The box is ready to type in when Settings opens: the filter reaches every section from
    // wherever one is. (Not where typing brings up a keyboard over the window, and not under a
    // prompt: the prompt's Cancel owns the focus until it is answered.)
    if opened
        && !crate::panels::notices::blocks_input(app)
        && !matches!(ui.ctx().os(), egui::os::OperatingSystem::Android | egui::os::OperatingSystem::IOS)
    {
        typed.response.request_focus();
    }
    // Esc with something typed empties the filter and is used up by that, wherever the focus is
    // (in the box, or on a section that was found); the focus goes back to the box.
    let escape = had_text && !crate::widgets::escape_taken(ui.ctx()) && ui.input(|i| i.key_pressed(egui::Key::Escape));
    if typed.cancelled() || escape {
        filter.text.clear();
        if had_text {
            crate::widgets::take_escape(ui.ctx());
            typed.response.request_focus();
        }
    }
    // (an emptied filter forgets what it read: the pages are read afresh the next time it is
    // used, so what is found is what they show then)
    if filter.text.trim().is_empty() {
        filter.sections = None;
    }
    // ↓ in the box goes to the first section found
    let to_first = typed.response.has_focus() && ui.input(|i| i.key_pressed(egui::Key::ArrowDown));
    let open_first = typed.ending == Some(crate::text_field::Ending::Return);
    // (with something typed the page is what was found, of any tab: no tab is the open one)
    let filtering = !filter.text.trim().is_empty();
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing = egui::vec2(4.0, 4.0);
        for (id, label) in TABS {
            if crate::widgets::text_button(ui, &format!("settingsTab-{id}"), label, tab == id && !filtering).clicked() {
                *tab = id.to_string();
                // (a tab opened is its page asked for: the filter gives way)
                filter.text.clear();
            }
        }
    });
    ui.separator();
    // With something typed, the page gives way to the sections found, of whatever tab.
    let words = filter.text.trim().to_lowercase();
    let now = ui.input(|i| i.time);
    let found = (!words.is_empty()).then(|| filter.found(app, &words, now));
    let mut goto = None;
    // (a point less than the room left, so rounding never makes the dialog's own scroll area scroll)
    let room = max_height - (ui.cursor().top() - top) - 1.0;
    let mut height = room.min(PAGE_SIZE.y);
    // The slide preview stays above the page on Slideshow: hovering a template in the Template
    // Browser previews it there, so it has to stay in view however far down the templates are
    // scrolled to. With its heading and choice it takes [`PINNED_PREVIEW_SHARE`] of the room
    // (at least the least tall slide); where that leaves less than [`PINNED_PAGE_MIN_HEIGHT`] for the
    // page, it scrolls with the page instead (as in the short window above). What was found
    // is drawn in its place while the filter has something typed.
    let mut pinned = false;
    if tab == "slideshow" && found.is_none() && room >= PAGE_MIN_HEIGHT {
        // (the filter row is taken off the page, not off the preview: the slide is as large as it was)
        let slide_h = ((room + filter_row).min(PAGE_SIZE.y) * PINNED_PREVIEW_SHARE - PREVIEW_CHROME).max(super::slide_layout::PREVIEW_MIN_H);
        if height - (slide_h + PREVIEW_CHROME_DRAWN) >= PINNED_PAGE_MIN_HEIGHT {
            let before = ui.cursor().top();
            super::slide_layout::preview(app, ui, &Tokens::get(ui.ctx()), slide_h);
            ui.add_space(2.0);
            ui.separator();
            height = (height - (ui.cursor().top() - before)).max(PINNED_PAGE_MIN_HEIGHT);
            pinned = true;
        }
    }
    let mut page = |ui: &mut egui::Ui| match &found {
        Some(found) => goto = results(ui, found, &words, open_first, to_first),
        None => {
            // (a section chosen that starts its tab, with no heading of its own: the page goes to its top)
            if ui
                .data(|d| d.get_temp::<(&'static str, String)>(egui::Id::unique(GOTO)))
                .is_some_and(|(to, section)| to == tab.as_str() && section.is_empty())
            {
                ui.scroll_to_cursor(Some(egui::Align::TOP));
            }
            if pinned {
                slideshow_tab(app, ui, &Tokens::get(ui.ctx()), false);
            } else {
                body(app, ui, tab.as_str());
            }
            // (what the page took: wider than the room for it would mean a control is cut off)
            register(ui, "settings:page", ui.min_rect());
        }
    };
    if room < PAGE_MIN_HEIGHT {
        page(ui);
    } else {
        // A page that goes on below shows its scroll handle all the time: without it, a page a
        // line or two too long looks cut off, not scrollable.
        ui.spacing_mut().scroll.dormant_handle_opacity = 0.6;
        let salt = if found.is_some() { "found" } else { tab.as_str() };
        let area = egui::ScrollArea::vertical()
            .id_salt(("settings-page", salt))
            .max_height(height)
            .min_scrolled_height(height)
            .auto_shrink([false, false])
            .show(ui, |ui| {
                ui.add_space(2.0);
                page(ui);
            });
        register(ui, "settings:pageRoom", area.inner_rect);
    }
    // A section chosen from what was found: its tab opens (on the next frame, as a clicked tab
    // does), the filter is emptied, and the section's heading is scrolled to when it is drawn.
    let goto_id = egui::Id::unique(GOTO);
    if let Some((to, section)) = goto {
        *tab = to.to_string();
        filter.text.clear();
        ui.data_mut(|d| d.insert_temp(goto_id, (to, section)));
        ui.ctx().request_repaint();
    } else if ui.data(|d| d.get_temp::<(&'static str, String)>(goto_id)).is_some_and(|(to, _)| to == tab.as_str()) {
        // (the page it was for has been drawn: its heading has had its chance)
        ui.data_mut(|d| d.remove::<(&'static str, String)>(goto_id));
    }
    ui.data_mut(|d| d.insert_temp(filter_id, filter));
}

/// Where the section to scroll to is kept between the click on a result and its page being drawn.
const GOTO: &str = "settings-goto";

/// Where the section landed on from the filter is kept while its heading is marked: its name
/// and when.
const LANDED: &str = "settings-landed";

/// How long the heading of the section landed on stays marked, in seconds.
const LANDED_FOR: f64 = 1.6;

/// Words people type for a section that the section does not say itself: the tab, the heading's
/// English name (empty for what a tab draws above its first heading) and the words, in lower
/// case and in the spelling [`fold`] gives. They are English whatever language is shown (in the
/// others a section is found by what it says), and count as much as a word in an explanation.
pub const ALSO: &[(&str, &str, &str)] = &[
    ("general", "", "locale translation translate"),
    ("general", "Startup", "launch start"),
    ("general", "Culling", "trash bin delete deleted"),
    ("general", "Library", "catalog database"),
    ("performance", "Smart previews", "proxy proxies offline"),
    ("import", "Raw defaults", "preset develop"),
    ("import", "Metadata", "iptc author credit"),
    ("import", "Cameras and Cards", "memory card sd camera connected device"),
    ("import", "Auto Import", "watch tether"),
    ("import", "XMP sidecars", "metadata"),
    ("general", "External Editing", "editor photoshop color space bit depth"),
    ("performance", "Rendering", "graphics card speed resolution"),
    ("performance", "Thumbnail cache", "disk space storage previews"),
    ("display", "Monitor profile", "screen color management calibration calibrate"),
    ("interface", "Appearance", "theme color scheme skin"),
    ("slideshow", "Playback", "presentation autoplay crossfade"),
    ("faces", "Face recognition", "people person"),
    ("denoise", "AI Denoise", "noise reduction"),
];

/// Pairs of spellings of one word, the one [`fold`] gives first.
const SPELLINGS: &[(&str, &str)] =
    &[("color", "colour"), ("gray", "grey"), ("catalog", "catalogue"), ("favorite", "favourite"), ("center", "centre"), ("license", "licence")];

/// `text` as the filter compares it: in lower case, and in one spelling of the words that have
/// two (colour as color), so that either finds the other.
fn fold(text: &str) -> String {
    let mut folded = text.to_lowercase();
    for (one, other) in SPELLINGS {
        if folded.contains(other) {
            folded = folded.replace(other, one);
        }
    }
    folded
}

/// A word typed (in lower case) in the one spelling ([`fold`]), which a word still being typed
/// in the other is brought to as well: "colou" is on the way to "colour" and is compared as
/// "color", "catalogu" as "catalog". Only once it has gone past where the two spellings part,
/// so that "colo" stays what it is.
fn fold_typed(word: &str) -> String {
    let folded = fold(word);
    for (one, other) in SPELLINGS {
        let shared = one.chars().zip(other.chars()).take_while(|(a, b)| a == b).count();
        // (the longest start of the other spelling the word ends in, past what the two share)
        let ends_in = other.char_indices().skip(shared + 1).filter_map(|(at, _)| other.get(..at)).filter(|start| folded.ends_with(start)).last();
        if let Some(stem) = ends_in.and_then(|start| folded.strip_suffix(start)) {
            return format!("{stem}{one}");
        }
    }
    folded
}

/// A text as the filter compares it: in lower case, and in one spelling ([`fold`]).
#[derive(Clone)]
struct Compared {
    lower: String,
    folded: String,
}

impl Compared {
    fn of(text: &str) -> Self {
        let lower = text.to_lowercase();
        Self { folded: fold(&lower), lower }
    }

    /// Whether the text has `word` (in lower case): as typed, or in the other spelling of it. As
    /// typed counts too, so that a word is found all the way while it is typed ("colou" is on
    /// the way to "colour", and is in no text once that is spelt "color").
    fn has(&self, word: &str) -> bool {
        self.lower.contains(word) || self.folded.contains(&fold_typed(word))
    }
}

/// A text a section draws: as drawn, as the filter compares it, and whether it is in the small
/// type of an explanation.
#[derive(Clone)]
struct Drawn {
    text: String,
    compared: Compared,
    small: bool,
}

/// A section of Settings as the filter knows it: the tab it is on, its heading's English name
/// (empty for what a tab draws above its first heading) and every piece of text it draws.
#[derive(Clone)]
struct Section {
    tab: &'static str,
    name: String,
    texts: Vec<Drawn>,
}

/// The filter's state between frames: what is typed, and the sections of every tab with their
/// text, read once for the language and library they were read in.
#[derive(Clone, Default)]
struct Filter {
    pass: u64,
    text: String,
    /// The tab that was open on the frame before.
    tab: String,
    sections: Option<(String, std::sync::Arc<Vec<Section>>)>,
}

impl Filter {
    /// The sections that have every word of `words` (in lower case), the best match first
    /// ([`Section::rank`]) and those that match alike in the order of the tabs.
    fn found(&mut self, app: &mut LightcraftApp, words: &str, now: f64) -> Vec<Section> {
        let of = format!("{}|{}", app.ui.language.code(), app.session.library.as_ref().map(|l| l.dir.display().to_string()).unwrap_or_default());
        let sections = match &self.sections {
            Some((read_for, sections)) if *read_for == of => sections.clone(),
            _ => {
                let sections = std::sync::Arc::new(sections(app, now));
                self.sections = Some((of, sections.clone()));
                sections
            }
        };
        let mut found: Vec<(u32, &Section)> = sections.iter().filter_map(|section| Some((section.rank(words)?, section))).collect();
        // (a stable sort: the order of the tabs is kept among equals)
        found.sort_by_key(|(rank, _)| *rank);
        found.into_iter().map(|(_, section)| section.clone()).collect()
    }
}

impl Section {
    /// The name of the section's tab, as shown.
    fn tab_name(&self) -> &'static str {
        TABS.iter().find(|(id, _)| *id == self.tab).map_or("", |(_, label)| crate::i18n::tr(label))
    }

    /// How well the section matches `words` (in lower case), less being better, or `None` when one
    /// of them is nowhere in it. Each word counts by the best place it is found in: 0 in the
    /// name of the section or its tab, 1 in a label, choice or button, 2 in an explanation or
    /// among the words people type for the section ([`ALSO`], from the start of one).
    fn rank(&self, words: &str) -> Option<u32> {
        let names = Compared::of(&format!("{} {}", self.tab_name(), crate::i18n::tr(&self.name)));
        let also = || ALSO.iter().filter(|(tab, name, _)| *tab == self.tab && *name == self.name).flat_map(|(_, _, also)| also.split_whitespace());
        words
            .split_whitespace()
            .map(|word| {
                if names.has(word) {
                    Some(0)
                } else if self.texts.iter().any(|drawn| !drawn.small && drawn.compared.has(word)) {
                    Some(1)
                } else if self.texts.iter().any(|drawn| drawn.compared.has(word)) || also().any(|also| also.starts_with(&fold_typed(word))) {
                    Some(2)
                } else {
                    None
                }
            })
            .sum()
    }

    /// What the section is called in the list of what was found: its tab, then its heading.
    fn title(&self) -> String {
        let tab = self.tab_name();
        if self.name.is_empty() { tab.to_string() } else { format!("{tab} › {}", crate::i18n::tr(&self.name)) }
    }
}

/// Where in `text` a word of `words` (in lower case) stands, as ranges of bytes of `text`, in
/// the order they stand in and none inside another: in either case and either spelling.
fn places(text: &str, words: &str) -> Vec<std::ops::Range<usize>> {
    // each char of the text in lower case, with where it starts (a char that is several in lower
    // case counts by its first: the ranges are of `text`, which `to_lowercase` would not keep)
    let chars: Vec<(usize, char)> = text.char_indices().map(|(at, c)| (at, c.to_lowercase().next().unwrap_or(c))).collect();
    let mut found: Vec<std::ops::Range<usize>> = Vec::new();
    for word in words.split_whitespace() {
        // (as typed, in the one spelling, and in the other)
        let folded = fold_typed(word);
        let other = SPELLINGS.iter().find(|(one, _)| folded.contains(one)).map(|(one, other)| folded.replace(one, other));
        let mut spellings = vec![word.to_string()];
        spellings.extend(std::iter::once(folded).chain(other).filter(|spelt| spelt != word));
        for spelt in spellings {
            let spelt: Vec<char> = spelt.chars().collect();
            if spelt.is_empty() {
                continue;
            }
            for start in 0..chars.len() {
                let Some(run) = chars.get(start..start.saturating_add(spelt.len())) else { break };
                if run.iter().map(|(_, c)| c).eq(spelt.iter()) {
                    let from = run.first().map_or(0, |(at, _)| *at);
                    let to = chars.get(start + spelt.len()).map_or(text.len(), |(at, _)| *at);
                    found.push(from..to);
                }
            }
        }
    }
    found.sort_by_key(|range| (range.start, std::cmp::Reverse(range.end)));
    let mut apart: Vec<std::ops::Range<usize>> = Vec::new();
    for range in found {
        if apart.last().is_none_or(|last| range.start >= last.end) {
            apart.push(range);
        }
    }
    apart
}

/// How far into a line under a result the first word found may stand, in chars, before the
/// line is started later so that the word is in sight.
const SNIPPET_LEAD: usize = 40;

/// The line shown under a result for `text`: `text` itself, or from a word's start shortly
/// before the first word found with `…` before it when that word stands far in (the line is cut
/// at its end by the room there is), and where the words found stand in what is given.
fn snippet(text: &str, words: &str) -> (String, Vec<std::ops::Range<usize>>) {
    let found = places(text, words);
    let Some(first) = found.first().map(|range| range.start) else { return (text.to_string(), found) };
    let before = text.get(..first).unwrap_or("");
    if before.chars().count() <= SNIPPET_LEAD {
        return (text.to_string(), found);
    }
    // (from the start of a word, a few words before the one found)
    let lead = before.char_indices().rev().nth(SNIPPET_LEAD / 2).map_or(0, |(at, _)| at);
    let from = before.get(lead..).and_then(|rest| rest.find(' ')).map_or(lead, |space| lead + space + 1).min(first);
    let Some(shown) = text.get(from..) else { return (text.to_string(), found) };
    let shift = '…'.len_utf8();
    let found = found.into_iter().map(|range| range.start - from + shift..range.end - from + shift).collect();
    (format!("…{shown}"), found)
}

/// Every section of every tab with the text it draws, read by drawing each tab's page once in a
/// context of its own: nothing of it reaches the screen, the list of widgets or the window's own
/// memory, and whatever a tab draws is found without a list kept by hand. Drawing a page asks
/// what opening its tab asks (the smart previews folder, the installed models): the answers the
/// app keeps (the installed denoise models) are filled as opening the tab fills them, those a
/// page keeps in its own context (the smart previews folder, the face models) are asked again,
/// and the slide preview of Slideshow may be asked to render, as that page does. No setting is
/// changed by it, as nothing is clicked. What a page does not draw is not found: the choices of
/// a closed list, tooltips, and rows scrolled out of a list inside a page.
fn sections(app: &mut LightcraftApp, now: f64) -> Vec<Section> {
    fn texts(shape: &egui::Shape, out: &mut Vec<(egui::Pos2, String, bool)>) {
        match shape {
            egui::Shape::Text(text) => {
                // (an explanation is in 11 pt; a label, choice or button in 12 pt or more)
                let small = text.galley.job.sections.first().is_some_and(|section| section.format.font_id.size < 12.0);
                out.push((text.pos, text.galley.job.text.clone(), small));
            }
            egui::Shape::Vec(shapes) => shapes.iter().for_each(|shape| texts(shape, out)),
            _ => {}
        }
    }
    let scratch = egui::Context::default();
    crate::theme::install_fonts(&scratch);
    // (tall enough that no page is cut off: what is outside the screen is not drawn)
    let screen = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(PAGE_SIZE.x + 32.0, 20_000.0));
    let mut all = Vec::new();
    for (tab, _) in TABS {
        // (on the window's clock: what a page remembers with the time it asked is not left stamped
        // with another)
        let input = egui::RawInput { screen_rect: Some(screen), time: Some(now), ..Default::default() };
        let mut output = scratch.run_ui(input, |ui| {
            ui.set_width(PAGE_SIZE.x);
            body(app, ui, tab);
        });
        // (the font atlas this context made is for no screen: said so, or dropping it is an error)
        output.textures_delta.clear();
        // the page's headings, top to bottom, and each text under the last heading above it
        let registry = crate::widgets::take_registry(&scratch);
        let mut headings: Vec<(f32, String)> =
            registry.iter().filter_map(|(id, r)| Some((r.top(), id.strip_prefix("heading:")?.to_string()))).collect();
        // (where a folder or file is shown, [`path_label`], and what is in a text box: where a
        // library, a cache or an editor happens to be, or the name someone typed, is not what a
        // setting is called, and is not looked through)
        let paths: Vec<egui::Rect> =
            registry.iter().filter(|(id, _)| id.starts_with("path:") || id.starts_with("field:")).map(|(_, r)| r.expand(1.0)).collect();
        headings.sort_by(|a, b| a.0.total_cmp(&b.0));
        let mut drawn = Vec::new();
        output.shapes.iter().for_each(|clipped| texts(&clipped.shape, &mut drawn));
        drawn.sort_by(|a, b| a.0.y.total_cmp(&b.0.y));
        let mut of_tab: Vec<Section> = std::iter::once(String::new())
            .chain(headings.iter().map(|(_, name)| name.clone()))
            .map(|name| Section { tab, name, texts: Vec::new() })
            .collect();
        for (at, text, small) in drawn {
            // (a text counts as under a heading from a little above the heading's own top)
            let under = headings.iter().take_while(|(top, _)| *top <= at.y + 1.0).count();
            if let Some(section) = of_tab.get_mut(under)
                && !text.trim().is_empty()
                && !paths.iter().any(|path| path.contains(at))
            {
                section.texts.push(Drawn { compared: Compared::of(&text), text, small });
            }
        }
        all.extend(of_tab.into_iter().filter(|section| !section.texts.is_empty()));
    }
    all
}

/// The sections found, each a row to choose (`settingsFound:{tab}:{section}`): its tab and
/// heading, and under them the first texts of it that have a word of `words` ([`snippet`]). Gives the section
/// chosen, by a click, by Return or Space with the focus on it, or (`open_first`) by Return in
/// the filter, which chooses the first. `focus_first` gives the first row the keyboard focus.
fn results(ui: &mut egui::Ui, found: &[Section], words: &str, open_first: bool, focus_first: bool) -> Option<(&'static str, String)> {
    let t = Tokens::get(ui.ctx());
    if found.is_empty() {
        ui.add_space(8.0);
        ui.label(RichText::new(crate::i18n::tr("No setting matches what is typed.")).color(t.text_label));
        hint(ui, &t, "Try one word, or part of one: the names of tabs and sections, labels and explanations are all looked through.");
        return None;
    }
    let mut chosen = open_first.then(|| found.first().map(|first| (first.tab, first.name.clone()))).flatten();
    for (place, section) in found.iter().enumerate() {
        let fill = ui.painter().add(egui::Shape::Noop);
        let row = ui
            .scope_builder(egui::UiBuilder::new().sense(egui::Sense::click()), |ui| {
                ui.set_width(ui.available_width());
                ui.spacing_mut().item_spacing.y = 2.0;
                ui.add_space(3.0);
                ui.add(egui::Label::new(RichText::new(section.title()).font(t.semibold(13.0)).color(t.text)).selectable(false));
                // (what of it has a word that was typed; its heading is in the line above already)
                let heading = crate::i18n::tr(&section.name);
                let with = section
                    .texts
                    .iter()
                    .filter(|drawn| drawn.text != heading && words.split_whitespace().any(|word| drawn.compared.has(word)))
                    .take(2);
                for drawn in with {
                    let dim = t.text_dim;
                    // The word found is in sight and told apart: the line starts shortly before
                    // it, and it is in the text colour and a heavier weight among dimmer words.
                    let (line, found) = snippet(&drawn.text, words);
                    let plain = egui::TextFormat { font_id: egui::FontId::proportional(11.0), color: dim, ..Default::default() };
                    let strong = egui::TextFormat { font_id: t.semibold(11.0), color: t.text, ..Default::default() };
                    let mut job = egui::text::LayoutJob::default();
                    let mut at = 0;
                    for range in found {
                        job.append(line.get(at..range.start).unwrap_or(""), 0.0, plain.clone());
                        job.append(line.get(range.clone()).unwrap_or(""), 0.0, strong.clone());
                        at = range.end;
                    }
                    job.append(line.get(at..).unwrap_or(""), 0.0, plain);
                    ui.add(egui::Label::new(job).selectable(false).truncate());
                }
                ui.add_space(3.0);
            })
            .response;
        if focus_first && place == 0 {
            row.request_focus();
        }
        let around = row.rect.expand2(egui::vec2(6.0, 0.0));
        if row.hovered() || row.has_focus() {
            ui.painter().set(fill, egui::Shape::rect_filled(around, egui::CornerRadius::same(4), t.hover));
        }
        if row.has_focus() {
            ui.painter().rect_stroke(around, egui::CornerRadius::same(4), egui::Stroke::new(2.0, t.text), egui::StrokeKind::Inside);
        }
        row.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, section.title()));
        register(ui, format!("settingsFound:{}:{}", section.tab, section.name), row.rect);
        if row.clicked() {
            chosen = Some((section.tab, section.name.clone()));
        }
    }
    chosen
}

/// `path` shortened to fit `max` (as `width` measures it) by dropping leading folders, after
/// either kind of separator, so that its end stays (`…/Pictures/Library`,
/// `…\Pictures\Library`): the end says which folder it is. A last folder too long by itself
/// loses its end ([`crate::widgets::elide_head`]).
fn elide_path(path: &str, max: f32, width: impl Fn(&str) -> f32) -> String {
    if width(path) <= max {
        return path.to_string();
    }
    // (a separator at the very end is not where a name starts: `…/` alone would say nothing)
    let end = path.trim_end_matches(['/', '\\']).len();
    let mut rest = path;
    while let Some(i) = rest.find(['/', '\\']).filter(|i| path.len() - rest.len() + i + 1 < end) {
        let separator = if rest.get(i..).is_some_and(|r| r.starts_with('\\')) { '\\' } else { '/' };
        rest = rest.get(i + 1..).unwrap_or("");
        let shorter = format!("…{separator}{rest}");
        if width(&shorter) <= max {
            return shorter;
        }
    }
    let name = rest.trim_end_matches(['/', '\\']);
    // (nothing but separators: never an empty label)
    if name.is_empty() { "…".to_string() } else { crate::widgets::elide_head(name, max, width) }
}

/// A folder or file path on one line in the room left in its row (`text:{id}`): shortened from
/// the front when it is too long ([`elide_path`]), with all of it in a tooltip then. `after` is
/// the width of what follows it on the line (a button, a note) and its spacing: the path leaves
/// that room, so those never wrap onto a second line.
fn path_label(ui: &mut egui::Ui, id: &str, path: &str, font: egui::FontId, color: egui::Color32, after: f32) {
    let shown =
        elide_path(path, (ui.available_width() - after).max(0.0), |s| ui.painter().layout_no_wrap(s.to_string(), font.clone(), color).size().x);
    // (`truncate` is the last resort, for a room narrower than "…")
    let r = ui.add(egui::Label::new(RichText::new(&shown).font(font).color(color)).truncate().show_tooltip_when_elided(false));
    register(ui, format!("text:{id}"), r.rect);
    // (what the filter leaves out of what it looks through; not what stands here in words when
    // there is no folder to show)
    if path.contains(['/', '\\']) {
        register(ui, format!("path:{id}"), r.rect);
    }
    if shown != path {
        r.on_hover_text(path);
    }
}

/// The width of `text` in the style `style`, and the gap before it: what a label takes in a row.
fn label_width(ui: &egui::Ui, text: &str, style: egui::TextStyle) -> f32 {
    ui.painter().layout_no_wrap(text.to_string(), style.resolve(ui.style()), egui::Color32::WHITE).size().x + ui.spacing().item_spacing.x
}

/// The same for a button, which has padding either side of its text.
fn button_width(ui: &egui::Ui, text: &str) -> f32 {
    label_width(ui, text, egui::TextStyle::Button) + 2.0 * ui.spacing().button_padding.x
}

pub(super) fn heading(ui: &mut egui::Ui, t: &Tokens, text: &str) {
    ui.add_space(4.0);
    // `text` is the heading's English name (its message key). It is shown translated and
    // registered as `heading:{text}` whatever the language, where a section starts: for the
    // filter, scripts and tests that look for one.
    let behind = ui.painter().add(egui::Shape::Noop);
    let r = ui.label(RichText::new(crate::i18n::tr(text)).font(t.semibold(12.5)).color(t.text));
    let name = text;
    register(ui, format!("heading:{name}"), r.rect);
    // (chosen from what the filter found: the section is brought to the top of the page, and its
    // heading is marked for a moment, so that the one chosen is told from its neighbours)
    let now = ui.input(|i| i.time);
    let landed_id = egui::Id::unique(LANDED);
    if ui.data(|d| d.get_temp::<(&'static str, String)>(egui::Id::unique(GOTO))).is_some_and(|(_, section)| section == name) {
        r.scroll_to_me(Some(egui::Align::TOP));
        ui.data_mut(|d| d.insert_temp(landed_id, (name.to_string(), now)));
    }
    if let Some((_, since)) = ui.data(|d| d.get_temp::<(String, f64)>(landed_id)).filter(|(landed, _)| *landed == name) {
        // (full for the first half of its time, then fading out)
        let left = (1.0 - (now - since) / LANDED_FOR).clamp(0.0, 1.0) as f32;
        if left > 0.0 {
            let strength = (left * 2.0).min(1.0);
            let around = r.rect.expand2(egui::vec2(6.0, 2.0));
            let outline = egui::Stroke::new(1.0, t.accent.gamma_multiply(strength));
            ui.painter().set(behind, egui::Shape::rect_filled(around, egui::CornerRadius::same(4), t.hover.gamma_multiply(strength)));
            ui.painter().rect_stroke(around, egui::CornerRadius::same(4), outline, egui::StrokeKind::Inside);
            register_geometry(ui.ctx(), format!("settingsLanded:{name}"), around);
            ui.ctx().request_repaint();
        } else {
            ui.data_mut(|d| d.remove::<(String, f64)>(landed_id));
        }
    }
}

pub(super) fn row<R>(ui: &mut egui::Ui, t: &Tokens, label: &str, add: impl FnOnce(&mut egui::Ui) -> R) -> R {
    ui.horizontal(|ui| {
        ui.allocate_ui_with_layout(egui::vec2(LABEL_W, 24.0), egui::Layout::left_to_right(egui::Align::Center), |ui| {
            ui.set_min_width(LABEL_W);
            // (a label longer than its column wraps in it; it does not push the controls along)
            ui.add(egui::Label::new(RichText::new(crate::i18n::tr(label)).color(t.text_label)).wrap());
        });
        // The controls wrap in the room beside the label, each further line starting under the
        // first control, so a row with many choices grows down and never widens the window.
        let wrapped = egui::Layout::left_to_right(egui::Align::Center).with_main_wrap(true);
        ui.allocate_ui_with_layout(egui::vec2(ui.available_width(), 24.0), wrapped, |ui| {
            ui.spacing_mut().item_spacing.y = 4.0;
            add(ui)
        })
        .inner
    })
    .inner
}

pub(super) fn hint(ui: &mut egui::Ui, t: &Tokens, text: &str) {
    ui.label(RichText::new(crate::i18n::tr(text)).size(11.0).color(t.text_dim));
}

/// A checkbox addressable as `check:{id}`; true when toggled.
pub(super) fn check(ui: &mut egui::Ui, id: &str, value: &mut bool, label: &str) -> bool {
    let r = ui.checkbox(value, crate::i18n::tr(label));
    register(ui, format!("check:{id}"), r.rect);
    r.changed()
}

/// Mutually exclusive buttons (`button:{id}-{index}`).
pub(super) fn choices<V: PartialEq + Copy>(ui: &mut egui::Ui, id: &str, options: &[(V, &str)], value: &mut V) -> bool {
    let mut changed = false;
    ui.spacing_mut().item_spacing.x = 4.0;
    for (i, (v, l)) in options.iter().enumerate() {
        if crate::widgets::text_button(ui, &format!("{id}-{i}"), l, *value == *v).clicked() && *value != *v {
            *value = *v;
            changed = true;
        }
    }
    changed
}

// ------------------------------------------------------------------------------------- General

fn general_tab(app: &mut LightcraftApp, ui: &mut egui::Ui, t: &Tokens) {
    row(ui, t, crate::i18n::tr("Language"), |ui| {
        let languages: Vec<_> = crate::i18n::Locale::for_picker().into_iter().map(|language| (language, language.name())).collect();
        // Thirteen language names do not fit one line in a narrow window: wrap them.
        ui.horizontal_wrapped(|ui| {
            choices(ui, "settingsLanguage", &languages, &mut app.ui.language);
        });
        crate::i18n::set_language(app.ui.language);
    });
    heading(ui, t, "Library");
    let location = match &app.session.library {
        Some(l) => l.dir.display().to_string(),
        None => crate::i18n::tr("In memory — nothing is saved").to_string(),
    };
    row(ui, t, crate::i18n::tr("Location"), |ui| {
        path_label(ui, "settingsLocation", &location, egui::TextStyle::Body.resolve(ui.style()), t.text, 0.0);
    });
    row(ui, t, "", |ui| {
        let can = app.services.pick_folder.is_some();
        let r = ui.add_enabled(can, egui::Button::new(crate::i18n::tr("Open Library…")));
        register(ui, "button:settingsOpenLibrary", r.rect);
        if r.clicked() {
            let _ = app.run("app.openLibrary", json!({}));
        }
        let r = ui.add_enabled(can, egui::Button::new(crate::i18n::tr("New Library…")));
        register(ui, "button:settingsNewLibrary", r.rect);
        if r.clicked() {
            let _ = app.run("app.newLibrary", json!({}));
        }
        if !can {
            hint(ui, t, crate::i18n::tr("not available here"));
        }
    });
    // where libraries are folders to choose from (not in the browser, which keeps one)
    if app.services.pick_folder.is_some() {
        use crate::state::LaunchLibrary;
        row(ui, t, crate::i18n::tr("At launch"), |ui| {
            choices(
                ui,
                "settingsAtLaunch",
                &[(LaunchLibrary::Last, "Open the last library"), (LaunchLibrary::Ask, "Ask me")],
                &mut app.ui.settings.at_launch,
            );
        });
        hint(ui, t, crate::i18n::tr("Ask me lists your recent libraries at launch. Holding Alt (Option on a Mac) as the window appears asks once."));
    }
    // File ▸ Import Lightroom Catalog… (#2672): the window that asks where a catalog goes, or not
    ui.label(RichText::new(crate::i18n::tr("When importing a Lightroom catalog")).color(t.text_label));
    row(ui, t, "", |ui| {
        use crate::state::LightroomImport;
        choices(
            ui,
            "settingsLightroomImport",
            &[(LightroomImport::Ask, "Ask where to import it"), (LightroomImport::AlwaysMerge, "Always merge into the open library")],
            &mut app.ui.settings.lightroom_import,
        );
    });
    heading(ui, t, "Startup");
    row(ui, t, crate::i18n::tr("Open in"), |ui| {
        choices(
            ui,
            "settingsStartup",
            &[(StartupView::Last, "Last view"), (StartupView::Grid, "Photo Grid"), (StartupView::Detail, "Detail View")],
            &mut app.ui.settings.startup_view,
        );
    });
    heading(ui, t, "Culling");
    check(ui, "settings.autoAdvance", &mut app.ui.auto_advance, "Auto Advance: move to the next photo after rating or flagging");
    check(ui, "settings.confirmDelete", &mut app.ui.settings.confirm_delete, "Confirm before moving photos to Recently Deleted");
    check(ui, "settings.askBeforeMove", &mut app.ui.settings.ask_before_move, "Confirm before moving photos to another folder on disk");
    heading(ui, t, "Dialogs");
    check(ui, "settings.closeDialogsOutside", &mut app.ui.settings.close_dialogs_outside, "Close a dialog by clicking outside it");
    heading(ui, t, "External Editing");
    row(ui, t, crate::i18n::tr("PhotoCraft"), |ui| {
        crate::text_field::TextField::singleline("field:photocraftPath", &mut app.ui.settings.photocraft_path)
            .hint(crate::i18n::tr("Found automatically"))
            .width(220.0)
            .show(ui);
    });
    edit_file_rows(ui, t, "photocraftFile", &mut app.ui.settings.photocraft_file);
    hint(
        ui,
        t,
        crate::i18n::tr(
            "Photo ▸ Edit In ▸ Edit in PhotoCraft (⇧⌘E, or right-click a photo) renders a copy with your edits, stacks it with the original and opens it in PhotoCraft. Save there and the copy updates here. As PSD, PhotoCraft saves straight back to it. Leave the location empty unless you use a portable copy.",
        ),
    );
    row(ui, t, crate::i18n::tr("Other editor"), |ui| {
        crate::text_field::TextField::singleline("field:externalEditor", &mut app.ui.settings.external_editor)
            .hint(crate::i18n::tr("System default"))
            .width(220.0)
            .show(ui);
    });
    edit_file_rows(ui, t, "externalFile", &mut app.ui.settings.external_file);
    hint(ui, t, crate::i18n::tr("Edit in Other Editor (⌥⌘E) opens the copy in this application (an app name on macOS, a program path elsewhere)."));
    hint(
        ui,
        t,
        crate::i18n::tr(
            "To hand this editor the original file itself instead (the raw, without a copy: DxO PhotoLab, for example), right-click a photo → Edit In → Open Original in Other Editor (RAW, no copy).",
        ),
    );
}

/// The file Edit In hands an editor: format, colour space and bit depth (`button:{id}Format-…`).
fn edit_file_rows(ui: &mut egui::Ui, t: &Tokens, id: &str, file: &mut crate::state::EditFile) {
    use crate::state::{EditFormat, EditSpace};
    row(ui, t, "", |ui| {
        choices(ui, &format!("{id}Format"), &[(EditFormat::Psd, "PSD"), (EditFormat::Tiff, "TIFF")], &mut file.format);
        ui.separator();
        choices(ui, &format!("{id}Depth"), &[(16u8, "16-bit"), (8, "8-bit")], &mut file.bit_depth);
    });
    row(ui, t, "", |ui| {
        choices(
            ui,
            &format!("{id}Space"),
            &[(EditSpace::AdobeRgb, "Adobe RGB"), (EditSpace::ProPhoto, "ProPhoto"), (EditSpace::DisplayP3, "P3"), (EditSpace::Srgb, "sRGB")],
            &mut file.color_space,
        );
    });
}

// -------------------------------------------------------------------------------------- Import

/// A preset picker: `None` = `none_label`. Returns the new choice when it changed.
fn preset_combo(app: &LightcraftApp, ui: &mut egui::Ui, id: &str, label: &str, current: Option<&str>, none_label: &str) -> Option<Option<String>> {
    let name = |pid: &str| {
        app.session
            .presets
            .iter()
            .find(|p| p.id == pid)
            .map(|p| crate::i18n::builtin_label(&p.name, p.builtin).to_string())
            .unwrap_or_else(|| crate::i18n::tr_format!("{pid} (missing)", pid = pid))
    };
    let text = current.map(name).unwrap_or_else(|| crate::i18n::tr(none_label).to_string());
    let mut out = None;
    let r = crate::widgets::combo_box(id).width(240.0).selected_text(text).show_ui(ui, |ui| {
        if ui.selectable_label(current.is_none(), crate::i18n::tr(none_label)).clicked() {
            out = Some(None);
        }
        if let Some(pid) = crate::panels::presets::picker_rows(ui, id, &app.session.presets, current) {
            out = Some(Some(pid));
        }
    });
    crate::widgets::name_combo(&r.response, label);
    register(ui, format!("combo:{id}"), r.response.rect);
    out.filter(|v| v.as_deref() != current)
}

/// Cameras of the raws in the library plus those with a stored default, sorted.
fn cameras(app: &LightcraftApp) -> Vec<String> {
    let mut v: Vec<String> = app
        .session
        .catalog
        .photos()
        .filter(|p| p.kind == lightcraft_catalog::MediaKind::Raw && !p.meta.camera.is_empty())
        .map(|p| p.meta.camera.clone())
        .chain(app.session.import_defaults.cameras.iter().map(|c| c.camera.clone()))
        .collect();
    v.sort_by_key(|c| c.to_lowercase());
    v.dedup_by(|a, b| a.eq_ignore_ascii_case(b));
    v
}

fn import_tab(app: &mut LightcraftApp, ui: &mut egui::Ui, t: &Tokens) {
    let d = app.session.import_defaults.clone();
    heading(ui, t, "Raw defaults");
    hint(ui, t, crate::i18n::tr("Settings new raw photos start from. Changing them doesn't touch photos already in the library."));
    row(ui, t, crate::i18n::tr("Raw photos"), |ui| {
        if let Some(v) = preset_combo(app, ui, "settingsRawPreset", crate::i18n::tr("Raw default"), d.raw_preset.as_deref(), "LightCraft Default") {
            let _ = app.run("library.preferences", json!({"import": {"rawPreset": v}}));
        }
    });
    let mut per = d.per_camera;
    row(ui, t, "", |ui| {
        if check(ui, "settings.perCamera", &mut per, "Use camera-specific defaults") {
            let _ = app.run("library.preferences", json!({"import": {"perCamera": per}}));
        }
    });
    if per {
        let cams = cameras(app);
        if cams.is_empty() {
            hint(ui, t, crate::i18n::tr("No raw photos yet: cameras appear here once their photos are in the library."));
        }
        egui::ScrollArea::vertical().max_height(150.0).id_salt("settingsCameras").show(ui, |ui| {
            for (i, cam) in cams.iter().enumerate() {
                let entry = d.cameras.iter().find(|c| c.camera.eq_ignore_ascii_case(cam));
                row(ui, t, cam, |ui| {
                    // "Raw default" = no entry; otherwise the entry's preset (None = LightCraft Default)
                    const RAW_DEFAULT: &str = "\u{1}raw";
                    let current = match entry {
                        None => Some(RAW_DEFAULT),
                        Some(e) => e.preset.as_deref(),
                    };
                    let mut pick = None;
                    let label = match current {
                        Some(RAW_DEFAULT) => crate::i18n::tr("Same as raw default").to_string(),
                        None => crate::i18n::tr("LightCraft Default").to_string(),
                        Some(pid) => app
                            .session
                            .presets
                            .iter()
                            .find(|p| p.id == pid)
                            .map(|p| crate::i18n::builtin_label(&p.name, p.builtin).to_string())
                            .unwrap_or_else(|| pid.to_string()),
                    };
                    let id = format!("settingsCamera-{i}");
                    let r = crate::widgets::combo_box(&id).width(240.0).selected_text(label).show_ui(ui, |ui| {
                        if ui.selectable_label(current == Some(RAW_DEFAULT), crate::i18n::tr("Same as raw default")).clicked() {
                            pick = Some(json!({"camera": cam, "remove": true}));
                        }
                        if ui.selectable_label(current.is_none(), crate::i18n::tr("LightCraft Default")).clicked() {
                            pick = Some(json!({"camera": cam, "preset": null}));
                        }
                        if let Some(pid) = crate::panels::presets::picker_rows(ui, &id, &app.session.presets, current) {
                            pick = Some(json!({"camera": cam, "preset": pid}));
                        }
                    });
                    crate::widgets::name_combo(&r.response, crate::i18n::tr("Camera"));
                    register(ui, format!("combo:{id}"), r.response.rect);
                    if let Some(c) = pick {
                        let _ = app.run("library.preferences", json!({"camera": c}));
                    }
                });
            }
        });
    }
    heading(ui, t, "Other images (JPEG, PNG, TIFF, HEIC…)");
    row(ui, t, crate::i18n::tr("Non-raw photos"), |ui| {
        if let Some(v) = preset_combo(app, ui, "settingsOtherPreset", crate::i18n::tr("Other preset"), d.other_preset.as_deref(), "None") {
            let _ = app.run("library.preferences", json!({"import": {"otherPreset": v}}));
        }
    });
    heading(ui, t, "Metadata");
    hint(ui, t, crate::i18n::tr("Added to photos you import that don't already have it."));
    for (key, label, hint_text, value) in
        [("copyright", "Copyright", "© 2026 Your Name", d.copyright.clone()), ("creator", "Creator", "Your Name", d.creator.clone())]
    {
        row(ui, t, label, |ui| {
            let id = egui::Id::unique(("settingsMeta", key));
            let mut text: String = ui.data(|m| m.get_temp(id)).unwrap_or(value.clone());
            let r = ui.add(egui::TextEdit::singleline(&mut text).hint_text(hint_text).desired_width(240.0));
            register(ui, format!("field:settings.{key}"), r.rect);
            if r.lost_focus() && text.trim() != value {
                let _ = app.run("library.preferences", json!({"import": {key: text.trim()}}));
            }
            if r.has_focus() {
                ui.data_mut(|m| m.insert_temp(id, text));
            } else {
                ui.data_mut(|m| m.remove::<String>(id));
            }
        });
    }
    if !app.session.metadata_presets.is_empty() {
        row(ui, t, crate::i18n::tr("Metadata preset"), |ui| {
            let cur = d.metadata_preset.clone();
            let mut pick = None;
            let r = crate::widgets::combo_box("settingsMetaPreset")
                .width(240.0)
                .selected_text(cur.clone().unwrap_or_else(|| crate::i18n::tr("None").into()))
                .show_ui(ui, |ui| {
                    if ui.selectable_label(cur.is_none(), crate::i18n::tr("None")).clicked() {
                        pick = Some(String::new());
                    }
                    for m in &app.session.metadata_presets {
                        if ui.selectable_label(cur.as_deref() == Some(m.name.as_str()), &m.name).clicked() {
                            pick = Some(m.name.clone());
                        }
                    }
                });
            crate::widgets::name_combo(&r.response, crate::i18n::tr("Metadata preset"));
            register(ui, "combo:settingsMetaPreset", r.response.rect);
            if let Some(n) = pick {
                let _ = app.run("library.preferences", json!({"import": {"metadataPreset": n}}));
            }
        });
    }
    heading(ui, t, "XMP sidecars");
    let mut xmp = app.session.xmp;
    if check(ui, "settings.autoWriteXmp", &mut xmp.auto_write, "Automatically write changes into XMP sidecars") {
        let _ = app.run("library.xmpPreferences", json!({"autoWrite": xmp.auto_write}));
    }
    row(ui, t, crate::i18n::tr("Sidecar names"), |ui| {
        use lightcraft_engine::sidecar::SidecarNaming as N;
        let mut n = xmp.naming;
        if choices(ui, "settingsXmpNaming", &[(N::Stem, "IMG_1.xmp"), (N::Full, "IMG_1.CR3.xmp")], &mut n) {
            let naming = if n == N::Full { "full" } else { "stem" };
            let _ = app.run("library.xmpPreferences", json!({"naming": naming}));
        }
    });
    if !cfg!(target_arch = "wasm32") {
        heading(ui, t, "Dropped Files");
        check(ui, "settings.importDropped", &mut app.ui.settings.import_dropped_directly, "Import dropped files without review");
        heading(ui, t, "Cameras and Cards");
        let mut on_card = d.show_on_card;
        if check(ui, "settings.importOnCard", &mut on_card, "Show the import review when a camera or card is connected") {
            let _ = app.run("library.preferences", json!({"import": {"showOnCard": on_card}}));
        }
        heading(ui, t, "Auto Import");
        hint(ui, t, crate::i18n::tr("Photos that arrive in this folder (tethering, a scanner, a sync app) are added as soon as they're complete."));
        row(ui, t, crate::i18n::tr("Watched folder"), |ui| {
            match &d.auto_folder {
                Some(folder) => path_label(
                    ui,
                    "settingsAutoFolder",
                    folder,
                    egui::TextStyle::Body.resolve(ui.style()),
                    t.text,
                    button_width(ui, crate::i18n::tr("Choose…")) + button_width(ui, crate::i18n::tr("Turn Off")),
                ),
                None => drop(ui.label(RichText::new(crate::i18n::tr("Off")).color(t.text))),
            }
            let can = app.services.pick_folder.is_some();
            let r = ui.add_enabled(can, egui::Button::new(crate::i18n::tr("Choose…")));
            register(ui, "button:settingsAutoFolder", r.rect);
            if r.clicked()
                && let Some(f) = app.services.pick_folder.as_mut().and_then(|f| f())
                && let Err(e) = app.run("library.autoImport", json!({"folder": f}))
            {
                app.toast(ui.ctx(), e);
            }
            if d.auto_folder.is_some() && ui.button(crate::i18n::tr("Turn Off")).clicked() {
                let _ = app.run("library.autoImport", json!({"folder": null}));
            }
        });
        if d.auto_folder.is_some() {
            row(ui, t, "", |ui| {
                let mut copy = d.auto_copy;
                if ui.checkbox(&mut copy, crate::i18n::tr("Copy into the library (else use the files where they are)")).changed() {
                    let _ = app.run("library.autoImport", json!({"copy": copy}));
                }
            });
            row(ui, t, crate::i18n::tr("Album"), |ui| {
                let id = egui::Id::unique("auto-album");
                let mut name = ui.data(|m| m.get_temp::<String>(id)).unwrap_or_else(|| d.auto_album.clone().unwrap_or_default());
                let r = ui.add(egui::TextEdit::singleline(&mut name).hint_text(crate::i18n::tr("None")).desired_width(180.0));
                if r.lost_focus() {
                    let _ = app.run("library.autoImport", json!({"album": name.trim()}));
                }
                ui.data_mut(|m| m.insert_temp(id, name));
            });
        }
    }
    if app.session.library.is_none() {
        hint(ui, t, crate::i18n::tr("In-memory session: these settings last until LightCraft quits."));
    }
}

// --------------------------------------------------------------------------------- Performance

fn performance_tab(app: &mut LightcraftApp, ui: &mut egui::Ui, t: &Tokens) {
    use lightcraft_engine::gpu;
    heading(ui, t, "Rendering");
    check(ui, "settings.gpu", &mut app.ui.settings.gpu, "Use the GPU for rendering");
    let status = if !gpu::available() {
        match gpu::unavailable_reason() {
            Some(why) => crate::i18n::tr_format!("Rendering on the CPU: {why}", why = why),
            None => "No usable GPU found: rendering on the CPU".to_string(),
        }
    } else {
        format!("GPU: {}", gpu::adapter_name().unwrap_or_else(|| "starting…".into()))
    };
    // One line, the whole text on hover: the reason can be a long driver message (no adapter
    // among the backends …), and the GPU state is process-wide, so it may change between frames;
    // a hint that wrapped would move every control below it under the pointer.
    ui.add(egui::Label::new(RichText::new(crate::i18n::tr(&status)).size(11.0).color(t.text_dim)).truncate()).on_hover_text(&status);
    row(ui, t, crate::i18n::tr("Preview size"), |ui| {
        let opts: Vec<(u32, String)> =
            PREVIEW_LIMITS.iter().map(|e| (*e, if *e == 0 { crate::i18n::tr("Automatic").to_string() } else { format!("{e} px") })).collect();
        let opts: Vec<(u32, &str)> = opts.iter().map(|(e, l)| (*e, l.as_str())).collect();
        choices(ui, "settingsPreview", &opts, &mut app.ui.settings.preview_limit);
    });
    hint(
        ui,
        t,
        crate::i18n::tr(
            "Automatic renders the Detail view at the size it is shown, up to the photo's own pixels. Choose a size to cap it: smaller is faster.",
        ),
    );
    row(ui, t, crate::i18n::tr("Memory for caches"), |ui| {
        let auto = crate::i18n::tr_format!("Automatic ({} MB)", lightcraft_engine::memory::default_budget() >> 20);
        let opts = [(0u32, auto.as_str()), (512, "512 MB"), (1024, "1 GB"), (2048, "2 GB"), (4096, "4 GB")];
        choices(ui, "settingsMemory", &opts, &mut app.ui.settings.memory_mb);
    });
    heading(ui, t, "Thumbnail cache");
    let cur = (app.session.cache_bytes() >> 20) as u32;
    row(ui, t, crate::i18n::tr("Size limit"), |ui| {
        let opts = [(512u32, "512 MB"), (1024, "1 GB"), (2048, "2 GB"), (4096, "4 GB"), (8192, "8 GB")];
        let mut v = if CACHE_SIZES.contains(&cur) { cur } else { 2048 };
        if choices(ui, "settingsCache", &opts, &mut v) {
            let _ = app.run("library.preferences", json!({"cacheMb": v}));
        }
    });
    let used = app.session.media.rendered.disk().map(|d| d.size());
    row(ui, t, crate::i18n::tr("In use"), |ui| {
        ui.label(
            RichText::new(used.map(|b| format!("{:.1} MB", b as f64 / 1048576.0)).unwrap_or_else(|| crate::i18n::tr("memory only").to_string()))
                .color(t.text),
        );
        let r = ui.button(crate::i18n::tr("Clear Cache"));
        register(ui, "button:settingsClearCache", r.rect);
        if r.clicked() {
            let _ = app.run("library.clearPreviews", json!({}));
        }
    });
    heading(ui, t, "Local folders");
    row(ui, t, crate::i18n::tr("Forget unchanged photos"), |ui| {
        let opts = [(0u32, "Never"), (7, "After 7 days"), (30, "After 30 days"), (90, "After 90 days"), (365, "After a year")];
        let mut v = app.session.forget_local_days;
        if choices(ui, "settingsForgetLocal", &opts, &mut v) {
            let _ = app.run("library.preferences", json!({"forgetLocalDays": v}));
        }
    });
    hint(
        ui,
        t,
        crate::i18n::tr(
            "Photos seen in Local but never added or changed leave the catalog when their folder hasn't been browsed for this long (checked when the library opens). Files stay on disk; browsing the folder shows them again.",
        ),
    );
    #[cfg(not(target_arch = "wasm32"))]
    smart_previews(app, ui, t);
}

/// Where this library keeps its smart previews (the offline-editing proxies, which can be large):
/// the effective folder, what is in it, and choosing another one.
#[cfg(not(target_arch = "wasm32"))]
fn smart_previews(app: &mut LightcraftApp, ui: &mut egui::Ui, t: &Tokens) {
    // listing a big folder every frame would be slow: refresh every 2 s and after a change
    let cache = egui::Id::unique("smart-location");
    let now = ui.input(|i| i.time);
    let loc: Value = match ui.data(|d| d.get_temp::<(f64, Value)>(cache)) {
        Some((at, v)) if now - at < 2.0 => v,
        _ => {
            let v = app.run("library.smartPreviewsLocation", json!({})).unwrap_or(Value::Null);
            ui.data_mut(|d| d.insert_temp(cache, (now, v.clone())));
            v
        }
    };
    if loc.is_null() {
        return;
    }
    heading(ui, t, "Smart previews");
    let path = loc["path"].as_str().unwrap_or_default().to_string();
    let available = loc["available"].as_bool().unwrap_or(true);
    let custom = loc["custom"].as_bool().unwrap_or(false);
    row(ui, t, crate::i18n::tr("Folder"), |ui| {
        path_label(
            ui,
            "settingsSmartFolder",
            &path,
            egui::TextStyle::Body.resolve(ui.style()),
            if available { t.text } else { t.reject },
            if custom { 0.0 } else { label_width(ui, "(library default)", egui::TextStyle::Body) },
        );
        if !custom {
            let r = ui.label(RichText::new("(library default)").color(t.text));
            register(ui, "text:settingsSmartDefault", r.rect);
        }
    });
    let (count, bytes) = (loc["count"].as_u64().unwrap_or(0), loc["bytes"].as_u64().unwrap_or(0));
    if available {
        hint(
            ui,
            t,
            &crate::i18n::tr_format!(
                "{count} smart preview{} · {:.1} MB",
                if count == 1 { "" } else { "s" },
                bytes as f64 / 1048576.0,
                count = count
            ),
        );
    } else {
        hint(
            ui,
            t,
            crate::i18n::tr(
                "This folder is not available (drive disconnected?). Smart previews are not built or used until it is back or you choose another folder.",
            ),
        );
    }
    // what happens to the previews already built when the folder changes
    let mode_id = egui::Id::unique("smart-existing");
    let mut mode: u8 = ui.data(|d| d.get_temp(mode_id)).unwrap_or(0);
    if count > 0 {
        row(ui, t, crate::i18n::tr("Existing previews"), |ui| {
            if choices(ui, "settingsSmartExisting", &[(0u8, "Move them"), (1, "Leave them"), (2, "Delete them")], &mut mode) {
                ui.data_mut(|d| d.insert_temp(mode_id, mode));
            }
        });
    }
    let existing = ["move", "leave", "discard"][mode.min(2) as usize];
    let mut result = None;
    ui.horizontal(|ui| {
        ui.add_space(LABEL_W);
        if app.services.pick_folder.is_some() {
            let r = ui.button(crate::i18n::tr("Choose Folder…"));
            register(ui, "button:settingsSmartChoose", r.rect);
            if r.clicked()
                && let Some(dir) = app.services.pick_folder.as_mut().and_then(|f| f())
            {
                result = Some(app.run("library.smartPreviewsLocation", json!({"path": dir, "existing": existing})));
            }
        }
        let r = ui.add_enabled(custom, egui::Button::new(crate::i18n::tr("Use Library Folder")));
        register(ui, "button:settingsSmartReset", r.rect);
        if r.clicked() {
            result = Some(app.run("library.smartPreviewsLocation", json!({"reset": true, "existing": existing})));
        }
    });
    if let Some(r) = result {
        ui.data_mut(|d| d.remove::<(f64, Value)>(cache));
        match r {
            Ok(v) => {
                let failed = v["failed"].as_array().map_or(0, Vec::len);
                let msg = match (existing, v["handled"].as_u64().unwrap_or(0)) {
                    (_, 0) => "Smart previews folder changed".to_string(),
                    ("move", n) => format!("Smart previews folder changed; moved {n}"),
                    ("discard", n) => format!("Smart previews folder changed; deleted {n}"),
                    _ => "Smart previews folder changed".to_string(),
                };
                app.toast(ui.ctx(), if failed > 0 { crate::i18n::tr_format!("{msg} ({failed} failed)", failed = failed, msg = msg) } else { msg });
            }
            Err(e) => app.toast(ui.ctx(), e),
        }
    }
    hint(ui, t, crate::i18n::tr("Keep it on a drive with room: smart previews are about 1 MB per photo. The thumbnail cache stays in the library."));
}

// ---------------------------------------------------------------------------------- Interface

/// The monitor profile (`app.displayProfile`): previews are shown through the display's ICC
/// profile, so wide-gamut and calibrated displays show colours as they are.
fn display_tab(app: &mut LightcraftApp, ui: &mut egui::Ui, t: &Tokens) {
    heading(ui, t, "Monitor profile");
    let current = app.renderer.display().cloned();
    let screen = current.as_ref().is_some_and(|d| d.path == crate::SCREEN_PROFILE);
    row(ui, t, crate::i18n::tr("Profile"), |ui| {
        let name = match &current {
            Some(d) => d.profile.description.clone(),
            None => crate::i18n::tr("None (the display is treated as sRGB)").to_string(),
        };
        ui.label(RichText::new(name).color(t.text));
        if screen {
            hint(ui, t, crate::i18n::tr("this screen's profile"));
        }
    });
    row(ui, t, "", |ui| {
        let can = app.services.pick_display_profile.is_some();
        let r = ui.add_enabled(can, egui::Button::new(crate::i18n::tr("Choose Profile…")));
        register(ui, "button:settingsDisplayProfile", r.rect);
        if r.clicked()
            && let Some(path) = app.services.pick_display_profile.as_mut().and_then(|pick| pick().into_iter().next())
        {
            app.ui.settings.display_profile = path;
        }
        // with no profile chosen, previews follow the screen's own profile where the host knows it
        let none = if app.services.screen_profile.is_some() { crate::i18n::tr("Use Screen Profile") } else { crate::i18n::tr("Use sRGB") };
        let r = ui.add_enabled(!app.ui.settings.display_profile.is_empty(), egui::Button::new(none));
        register(ui, "button:settingsDisplayProfileNone", r.rect);
        if r.clicked() {
            app.ui.settings.display_profile.clear();
        }
        if !can {
            hint(ui, t, crate::i18n::tr("not available here"));
        }
    });
    if let Some(e) = &app.display_error {
        ui.label(RichText::new(e).size(11.0).color(t.reject));
    }
    if let Some(d) = &current {
        let info = d.describe();
        if !screen {
            row(ui, t, crate::i18n::tr("File"), |ui| {
                path_label(ui, "settingsDisplayFile", &d.path, egui::FontId::proportional(11.0), t.text_dim, 0.0);
            });
        }
        row(ui, t, crate::i18n::tr("Type"), |ui| {
            let kind = match d.profile.kind {
                lightcraft_engine::display::DisplayKind::MatrixTrc => crate::i18n::tr("Matrix/TRC"),
                lightcraft_engine::display::DisplayKind::Lut => crate::i18n::tr("LUT-based"),
            };
            ui.label(RichText::new(kind).color(t.text));
        });
        row(ui, t, crate::i18n::tr("Primaries (x, y)"), |ui| {
            let p = &info["primaries"];
            let xy = |(k, l): (&str, &str)| format!("{l} {:.3}, {:.3}", p[k][0].as_f64().unwrap_or(0.0), p[k][1].as_f64().unwrap_or(0.0));
            let text = [("red", "R"), ("green", "G"), ("blue", "B")].map(xy).join("   ");
            ui.label(RichText::new(text).size(11.0).color(t.text_dim));
        });
    }
    hint(
        ui,
        t,
        crate::i18n::tr(
            "Photos are shown through your monitor's ICC profile, so colours are right on wide-gamut and calibrated displays, and the Detail view uses the display's whole gamut. The histogram, the preview caches and exports are not affected.",
        ),
    );
    #[cfg(target_os = "linux")]
    hint(ui, t, crate::i18n::tr("Profiles assigned in your desktop's colour settings are usually in ~/.local/share/icc."));
}

fn interface_tab(app: &mut LightcraftApp, ui: &mut egui::Ui, t: &Tokens) {
    let system = app.system_theme(ui.ctx());
    appearance(ui, t, &mut app.ui.settings, system);
    heading(ui, t, "Filmstrip");
    check(ui, "settings.filmNames", &mut app.ui.settings.film_names, "Show file names");
    check(ui, "settings.filmBadges", &mut app.ui.settings.film_badges, "Show ratings, flags and edit badges");
    heading(ui, t, "Grid");
    row(ui, t, crate::i18n::tr("Ratings & flags"), |ui| {
        choices(
            ui,
            "settingsGridBadges",
            &[(GridBadges::Auto, "When rated or hovered"), (GridBadges::Always, "Always"), (GridBadges::Never, "Never")],
            &mut app.ui.settings.grid_badges,
        );
    });
    check(ui, "settings.showFilenames", &mut app.ui.show_filenames, "Square Grid: show file names and formats");
    heading(ui, t, "Keywords");
    row(ui, t, crate::i18n::tr("Keywords of the selection"), |ui| {
        use crate::state::KeywordChips as K;
        choices(ui, "settingsKeywordChips", &[(K::Paths, "Whole names"), (K::Grouped, "Grouped by parent")], &mut app.ui.settings.keyword_chips);
    });
    hint(
        ui,
        t,
        crate::i18n::tr("Grouped: keywords inside the same keyword share a line naming it, and long lists and the suggestions are shortened."),
    );
    heading(ui, t, "Detail View");
    check(ui, "settings.navigator", &mut app.ui.navigator, "Show the Navigator while zoomed in");
    row(ui, t, crate::i18n::tr("Info overlay"), |ui| {
        use crate::state::InfoOverlay as I;
        choices(ui, "settingsInfo", &[(I::Off, "Off"), (I::Basic, "Info 1"), (I::Exposure, "Info 2")], &mut app.ui.info_overlay);
    });
    hint(ui, t, crate::i18n::tr("View → View Options chooses the lines Info 1 and Info 2 show."));
}

/// View ▸ View Options (⌘J), Lightroom Classic's Loupe View tab: whether the info overlay shows and
/// which, the three lines each of Info 1 and Info 2 shows, which of them shows briefly when the
/// photo changes, and whether the loupe shows its loading and rendering message
/// (`view.viewOptions`). Changes apply as they are made.
pub(super) fn view_options(app: &mut LightcraftApp, ui: &mut egui::Ui, t: &Tokens) {
    use crate::state::{InfoOverlay as I, LoupeInfo, LoupeInfoField as F};
    heading(ui, t, "Loupe View");
    let mut show = app.ui.info_overlay != I::Off;
    if check(ui, "viewOptions.showInfo", &mut show, "Show Info Overlay") {
        app.ui.info_overlay = if show { I::Basic } else { I::Off };
    }
    ui.add_enabled_ui(show, |ui| {
        ui.horizontal(|ui| {
            choices(ui, "viewOptionsInfo", &[(I::Basic, "Info 1"), (I::Exposure, "Info 2")], &mut app.ui.info_overlay);
        });
    });
    for (n, title, defaults) in [(1, "Loupe Info 1", LoupeInfo::INFO1), (2, "Loupe Info 2", LoupeInfo::INFO2)] {
        heading(ui, t, title);
        let lines = if n == 1 { &mut app.ui.settings.loupe_info.info1 } else { &mut app.ui.settings.loupe_info.info2 };
        for (i, line) in lines.iter_mut().enumerate() {
            let id = format!("viewOptionsInfo{n}Line{}", i + 1);
            let r = crate::widgets::combo_box(&id).width(220.0).selected_text(crate::i18n::tr(line.label())).show_ui(ui, |ui| {
                for f in F::ALL {
                    let key = serde_json::to_value(f).ok().and_then(|v| v.as_str().map(str::to_string)).unwrap_or_default();
                    let r = ui.selectable_label(line == f, crate::i18n::tr(f.label()));
                    register(ui, format!("button:{id}-{key}"), r.rect);
                    if r.clicked() {
                        *line = *f;
                    }
                }
            });
            crate::widgets::name_combo(
                &r.response,
                &crate::i18n::tr_format!("{title}: {line}", title = crate::i18n::tr(title), line = crate::i18n::tr(line.label())),
            );
            register(ui, format!("combo:{id}"), r.response.rect);
        }
        let r = ui.add_enabled(*lines != defaults, egui::Button::new(crate::i18n::tr("Use Defaults")));
        register(ui, format!("button:viewOptionsDefaults{n}"), r.rect);
        if r.clicked() {
            *lines = defaults;
        }
        // one Info at most shows briefly: ticking one unticks the other
        let info = if n == 1 { I::Basic } else { I::Exposure };
        let mut briefly = app.ui.settings.loupe_info.briefly == info;
        if check(ui, &format!("viewOptions.briefly{n}"), &mut briefly, "Show briefly when photo changes") {
            app.ui.settings.loupe_info.briefly = if briefly { info } else { I::Off };
        }
    }
    heading(ui, t, "General");
    check(ui, "viewOptions.loupeMessages", &mut app.ui.settings.loupe_messages, "Show message when loading or rendering photos");
}

// ----------------------------------------------------------------------------------- Slideshow

/// A playback duration slider (`slider:{id}`). While it is dragged the value changes in memory
/// (the slideshow reads it); letting go saves it through `slideshow.playback`.
fn duration_slider(app: &mut LightcraftApp, ui: &mut egui::Ui, id: &str, key: &str, value: f64, range: std::ops::RangeInclusive<f64>) {
    let mut v = value;
    let r = ui.add(egui::Slider::new(&mut v, range).logarithmic(key == "slideDuration").suffix(" s").max_decimals(1));
    register(ui, format!("slider:{id}"), r.rect);
    if r.changed()
        && let Ok(next) = app.session.slideshow.merged(&json!({ key: v }))
    {
        app.session.slideshow = next;
    }
    if r.drag_stopped() || (r.changed() && !r.dragged()) {
        let _ = app.run("slideshow.playback", json!({ key: v }));
    }
}

/// The Slideshow tab: the slide preview, then the settings, in one column the width of the page
/// (the page scrolls; a column beside the preview would be wider than the window). Without
/// `with_preview` the preview is left out: [`window`] has pinned it above the page.
fn slideshow_tab(app: &mut LightcraftApp, ui: &mut egui::Ui, t: &Tokens, with_preview: bool) {
    if with_preview {
        super::slide_layout::preview(app, ui, t, f32::INFINITY);
        ui.add_space(6.0);
    }
    super::slide_templates::section(app, ui, t);
    ui.add_space(6.0);
    playback_section(app, ui, t);
    ui.add_space(6.0);
    super::slide_layout::sections(app, ui, t);
    ui.add_space(6.0);
    super::slide_overlays::sections(app, ui, t);
    ui.add_space(6.0);
    super::slide_titles::sections(app, ui, t);
    ui.add_space(6.0);
    export_section(app, ui, t);
}

/// Export: PDF, JPEG sequence or video of the slideshow (View ▸ Export Slideshow…).
fn export_section(app: &mut LightcraftApp, ui: &mut egui::Ui, t: &Tokens) {
    heading(ui, t, "Export");
    let running = app.session.slideshow_export.as_ref().is_some_and(|r| r.running());
    ui.horizontal_wrapped(|ui| {
        for (id, label, format) in [
            ("slideshowExportPdf", "Export PDF…", "pdf"),
            ("slideshowExportJpeg", "Export JPEG…", "jpeg"),
            ("slideshowExportVideo", "Export Video…", "video"),
        ] {
            let r = ui.add_enabled(!running, egui::Button::new(crate::i18n::tr(label)));
            register(ui, format!("button:{id}"), r.rect);
            if r.clicked() {
                let _ = app.run("dialog.slideshowExport", json!({"format": format}));
            }
        }
    });
    hint(ui, t, crate::i18n::tr("The photos in view as slides, with the settings above. Progress shows in the activity stack."));
}

fn playback_section(app: &mut LightcraftApp, ui: &mut egui::Ui, t: &Tokens) {
    use lightcraft_engine::slideshow::{FADE_MAX, SLIDE_MAX, SLIDE_MIN};
    heading(ui, t, "Playback");
    let s = app.session.slideshow.clone();
    let mut v = s.manual;
    if check(ui, "settings.slideshowManual", &mut v, "Manual slideshow") {
        let _ = app.run("slideshow.playback", json!({"manual": v}));
    }
    hint(ui, t, crate::i18n::tr("A manual slideshow shows each slide until → or ← changes it. Exports still use the slide duration."));
    row(ui, t, crate::i18n::tr("Slide duration"), |ui| {
        duration_slider(app, ui, "slideshowSlide", "slideDuration", s.slide_duration, SLIDE_MIN..=SLIDE_MAX)
    });
    row(ui, t, crate::i18n::tr("Fade duration"), |ui| {
        duration_slider(app, ui, "slideshowFade", "fadeDuration", s.fade_duration, 0.0..=FADE_MAX.min(s.slide_duration))
    });
    hint(ui, t, crate::i18n::tr("A slide stays up for its duration, its fade in included. A fade of 0 cuts."));
    row(ui, t, crate::i18n::tr("Fade"), |ui| {
        #[derive(Clone, Copy, PartialEq)]
        enum Fade {
            Cross,
            Black,
            White,
            Color,
        }
        let mut f = match s.fade_color {
            None => Fade::Cross,
            Some([0, 0, 0]) => Fade::Black,
            Some([255, 255, 255]) => Fade::White,
            Some(_) => Fade::Color,
        };
        let opts = [(Fade::Cross, "Crossfade"), (Fade::Black, "Through black"), (Fade::White, "Through white"), (Fade::Color, "Through a colour")];
        if choices(ui, "settingsSlideshowFade", &opts, &mut f) {
            let c = match f {
                Fade::Cross => Value::Null,
                Fade::Black => json!("#000000"),
                Fade::White => json!("#ffffff"),
                Fade::Color => json!([64, 64, 64]),
            };
            let _ = app.run("slideshow.playback", json!({"fadeColor": c}));
        }
        if f == Fade::Color
            && let Some(mut c) = s.fade_color
        {
            let r = egui::color_picker::color_edit_button_srgb(ui, &mut c);
            register(ui, "button:slideshowFadeColor", r.rect);
            if r.changed() {
                let _ = app.run("slideshow.playback", json!({"fadeColor": c}));
            }
        }
    });
    let mut v = s.random_order;
    if check(ui, "settings.slideshowRandom", &mut v, "Random order") {
        let _ = app.run("slideshow.playback", json!({"randomOrder": v}));
    }
    let mut v = s.repeat;
    if check(ui, "settings.slideshowRepeat", &mut v, "Repeat (off: the slideshow ends after the last photo)") {
        let _ = app.run("slideshow.playback", json!({"repeat": v}));
    }
    let mut v = s.prepare_previews;
    if check(ui, "settings.slideshowPrepare", &mut v, "Prepare previews in advance") {
        let _ = app.run("slideshow.playback", json!({"preparePreviews": v}));
    }
    hint(
        ui,
        t,
        crate::i18n::tr(
            "Renders the next slides while the current one is up, so fades never wait. Off: each slide renders when it is due (less memory and work).",
        ),
    );
    hint(
        ui,
        t,
        crate::i18n::tr("Slideshow in the View menu plays the photos in view from the selected one. These settings are saved with the library."),
    );
}

/// Appearance Mode (`button:settingsAppearance-{0,1,2}`: Auto, Dark, Light) above the light and
/// dark theme cards (`radio:theme-{id}`), each with a preview of the chosen theme. The card in
/// use is outlined. Applied on the next frame ([`LightcraftApp::sync_theme`]).
fn appearance(ui: &mut egui::Ui, t: &Tokens, s: &mut crate::state::AppSettings, system: Option<egui::Theme>) {
    use crate::state::{AppearanceMode as M, DarkTheme, LightTheme};
    heading(ui, t, "Appearance");
    ui.label(RichText::new(crate::i18n::tr("Appearance Mode")).color(t.text_label));
    ui.horizontal_wrapped(|ui| {
        choices(ui, "settingsAppearance", &[(M::Auto, "Sync with System"), (M::Dark, "Dark Mode"), (M::Light, "Light Mode")], &mut s.appearance_mode);
    });
    ui.add_space(4.0);
    let light_active = s.theme(system) == s.light_theme.kind();
    let available = ui.available_width();
    let mut cards = |ui: &mut egui::Ui, width: f32| {
        ui.spacing_mut().item_spacing.x = 12.0;
        let light: Vec<_> = LightTheme::ALL.iter().map(|l| (*l, l.kind())).collect();
        theme_card(ui, t, "Light Theme", light_active, &light, &mut s.light_theme, &mut s.favorite_themes, width);
        let dark: Vec<_> = DarkTheme::ALL.iter().map(|d| (*d, d.kind())).collect();
        theme_card(ui, t, "Dark Theme", !light_active, &dark, &mut s.dark_theme, &mut s.favorite_themes, width);
    };
    if available < 440.0 {
        ui.vertical(|ui| cards(ui, available));
    } else {
        ui.horizontal_top(|ui| cards(ui, ((available - 12.0) / 2.0).floor()));
    }
    ui.add_space(2.0);
}

/// One family's card: its title (and "Active" when it is the one shown), a preview of the
/// selected theme and a radio button per theme.
fn theme_card<V: PartialEq + Copy>(
    ui: &mut egui::Ui,
    t: &Tokens,
    title: &str,
    active: bool,
    options: &[(V, crate::theme::ThemeKind)],
    value: &mut V,
    favorites: &mut Vec<String>,
    width: f32,
) {
    egui::Frame::new()
        .fill(t.inset)
        .stroke(egui::Stroke::new(1.0, if active { t.accent } else { t.field_border }))
        .corner_radius(6)
        .inner_margin(8)
        .show(ui, |ui| {
            ui.vertical(|ui| {
                let inner = (width - 18.0).clamp(120.0, 240.0);
                ui.set_width(inner);
                ui.horizontal(|ui| {
                    ui.label(RichText::new(crate::i18n::tr(title)).font(t.semibold(12.0)).color(t.text));
                    if active {
                        ui.label(RichText::new(crate::i18n::tr("In Use")).size(11.0).color(t.accent));
                    }
                });
                let shown = options.iter().find(|(v, _)| v == value).or(options.first()).map(|(_, k)| *k).unwrap_or_default();
                theme_preview(ui, shown, inner);
                for (v, kind) in options {
                    let row = egui::Rect::from_min_size(ui.cursor().min, egui::vec2(ui.available_width(), 24.0));
                    let hovered = ui.ctx().pointer_hover_pos().is_some_and(|p| row.contains(p));
                    ui.horizontal(|ui| {
                        let r = ui.radio_value(value, *v, crate::i18n::tr(kind.label()));
                        register(ui, format!("radio:theme-{}", kind.id()), r.rect);
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            let favorite = favorites.iter().any(|id| id == kind.id());
                            let r = ui
                                .add_visible(favorite || hovered, egui::Button::new("").min_size(egui::vec2(24.0, 24.0)).frame(false))
                                .on_hover_text(crate::i18n::tr(if favorite { "Remove from favorites" } else { "Add to favorites" }));
                            r.widget_info(|| {
                                egui::WidgetInfo::labeled(
                                    egui::Role::Button,
                                    true,
                                    format!(
                                        "{}: {}",
                                        crate::i18n::tr(if favorite { "Remove from favorites" } else { "Add to favorites" }),
                                        crate::i18n::tr(kind.label())
                                    ),
                                )
                            });
                            register(ui, format!("button:themeFavorite-{}", kind.id()), r.rect);
                            if favorite || hovered {
                                crate::rating::paint_star(ui.painter(), r.rect.shrink(4.0), favorite, if favorite { t.accent } else { t.text_dim });
                            }
                            if r.clicked() {
                                if favorite {
                                    favorites.retain(|id| id != kind.id());
                                } else {
                                    favorites.push(kind.id().into());
                                }
                            }
                        });
                    });
                }
            });
        });
}

/// A small LightCraft window painted with `kind`'s tokens: the top bar with its search field, the
/// photo with the filmstrip under it, the Edit panel (histogram and sliders), the tool strip and
/// the bottom bar.
fn theme_preview(ui: &mut egui::Ui, kind: crate::theme::ThemeKind, width: f32) {
    use egui::{Color32, Rect, Stroke, StrokeKind, pos2, vec2};
    let p = Tokens::for_kind(kind);
    let (rect, _) = ui.allocate_exact_size(vec2(width, (width * 0.56).round().min(130.0)), egui::Sense::hover());
    let painter = ui.painter_at(rect);
    let bar = |y: f32, x: f32, w: f32, c: Color32| painter.rect_filled(Rect::from_min_size(pos2(x, y), vec2(w, 2.0)), 1.0, c);
    painter.rect_filled(rect, 3.0, p.canvas);
    // top bar: sidebar and back icons, the centred search field, icons on the right
    let top = Rect::from_min_size(rect.min, vec2(rect.width(), 13.0));
    painter.rect_filled(top, 0.0, p.chrome);
    for x in [6.0, 14.0, 20.0] {
        painter.rect_filled(Rect::from_min_size(top.min + vec2(x, 4.5), vec2(4.0, 4.0)), 1.0, p.icon);
    }
    let search = Rect::from_center_size(top.center(), vec2(rect.width() * 0.34, 8.0));
    painter.rect(search, 2.0, p.field, Stroke::new(1.0, p.field_border), StrokeKind::Inside);
    bar(search.center().y - 1.0, search.center().x - 9.0, 18.0, p.text_dim);
    for i in 0..4 {
        painter.rect_filled(Rect::from_min_size(pos2(top.right() - 10.0 - i as f32 * 9.0, top.top() + 4.5), vec2(4.0, 4.0)), 2.0, p.icon);
    }
    // bottom bar: view buttons, rating stars, a button
    let bottom = Rect::from_min_max(pos2(rect.left(), rect.bottom() - 12.0), rect.max);
    painter.rect_filled(bottom, 0.0, p.chrome);
    for x in [6.0, 13.0, 20.0] {
        painter.rect_filled(Rect::from_min_size(bottom.min + vec2(x, 4.0), vec2(4.0, 4.0)), 1.0, p.icon);
    }
    let stars_x = bottom.center().x - 30.0;
    for i in 0..5 {
        painter.circle_filled(pos2(stars_x + i as f32 * 5.0, bottom.center().y), 1.4, p.star);
    }
    let copy = Rect::from_min_size(pos2(stars_x + 28.0, bottom.top() + 2.5), vec2(30.0, 7.0));
    painter.rect(copy, 3.0, p.button, Stroke::new(1.0, p.button_border), StrokeKind::Inside);
    // tool strip on the right edge, the first tool (Edit) active
    let strip = Rect::from_min_max(pos2(rect.right() - 12.0, top.bottom()), pos2(rect.right(), bottom.top()));
    painter.rect_filled(strip, 0.0, p.chrome);
    painter.line_segment([strip.left_top(), strip.left_bottom()], Stroke::new(1.0, p.divider));
    for i in 0..5 {
        let r = Rect::from_min_size(pos2(strip.left() + 3.0, strip.top() + 4.0 + i as f32 * 9.0), vec2(6.0, 6.0));
        if i == 0 {
            painter.rect_filled(r.expand(1.5), 1.5, p.tool_active);
        }
        painter.rect_stroke(r, 1.0, Stroke::new(1.0, if i == 0 { p.text } else { p.icon }), StrokeKind::Inside);
    }
    // Edit panel: histogram, then labelled sliders
    let panel = Rect::from_min_max(pos2(strip.left() - (rect.width() * 0.27).round(), top.bottom()), pos2(strip.left(), bottom.top()));
    painter.rect_filled(panel, 0.0, p.chrome);
    painter.line_segment([panel.left_top(), panel.left_bottom()], Stroke::new(1.0, p.divider));
    let hist = Rect::from_min_size(panel.min + vec2(4.0, 4.0), vec2(panel.width() - 8.0, 14.0));
    painter.rect_filled(hist, 1.0, p.inset);
    let curve = [0.2, 0.55, 0.9, 0.7, 0.5, 0.62, 0.4, 0.3, 0.18, 0.1];
    let mut pts = vec![hist.left_bottom()];
    for (i, h) in curve.iter().enumerate() {
        pts.push(pos2(hist.left() + hist.width() * i as f32 / (curve.len() - 1) as f32, hist.bottom() - hist.height() * h));
    }
    pts.push(hist.right_bottom());
    // the area under the curve, a column per stretch between two points
    for pair in pts.windows(2) {
        if let [a, b] = pair {
            let r = Rect::from_min_max(pos2(a.x, a.y.max(b.y)), pos2(b.x, hist.bottom()));
            painter.rect_filled(r, 0.0, p.track.gamma_multiply(0.5));
        }
    }
    painter.add(egui::Shape::line(pts, Stroke::new(1.0, p.text_dim)));
    let sliders = [0.55, 0.62, 0.35, 0.7, 0.6, 0.45];
    let mut y = hist.bottom() + 7.0;
    bar(y, panel.left() + 5.0, 14.0, p.text);
    y += 6.0;
    for v in sliders {
        if y + 7.0 > panel.bottom() {
            break;
        }
        bar(y, panel.left() + 5.0, 12.0, p.text_label);
        bar(y, panel.right() - 10.0, 5.0, p.text_dim);
        let track_y = y + 5.0;
        painter.line_segment([pos2(panel.left() + 5.0, track_y), pos2(panel.right() - 5.0, track_y)], Stroke::new(1.0, p.track));
        painter.circle_filled(pos2(panel.left() + 5.0 + (panel.width() - 10.0) * v, track_y), 1.8, p.thumb);
        y += 10.0;
    }
    // the photo (its own colours: a photo looks the same in every theme) above the filmstrip
    let film = Rect::from_min_max(pos2(rect.left(), bottom.top() - 18.0), pos2(panel.left(), bottom.top()));
    let stage = Rect::from_min_max(pos2(rect.left(), top.bottom()), pos2(panel.left(), film.top()));
    let photo = Rect::from_center_size(stage.center(), vec2(stage.width() * 0.62, stage.height() - 8.0));
    painter.rect_filled(photo, 0.0, Color32::from_rgb(0x6f, 0x8f, 0xb4));
    let horizon = photo.top() + photo.height() * 0.62;
    painter.add(egui::Shape::convex_polygon(
        vec![
            pos2(photo.left(), horizon),
            pos2(photo.left() + photo.width() * 0.3, photo.top() + photo.height() * 0.3),
            pos2(photo.left() + photo.width() * 0.55, horizon),
        ],
        Color32::from_rgb(0x4a, 0x55, 0x66),
        Stroke::NONE,
    ));
    painter.add(egui::Shape::convex_polygon(
        vec![
            pos2(photo.left() + photo.width() * 0.35, horizon),
            pos2(photo.left() + photo.width() * 0.68, photo.top() + photo.height() * 0.2),
            pos2(photo.right(), horizon),
        ],
        Color32::from_rgb(0x5a, 0x63, 0x72),
        Stroke::NONE,
    ));
    painter.rect_filled(Rect::from_min_max(pos2(photo.left(), horizon), photo.max), 0.0, Color32::from_rgb(0x3d, 0x5a, 0x3a));
    painter.line_segment([film.left_top(), film.right_top()], Stroke::new(1.0, p.divider));
    for i in 0..5 {
        let cell = Rect::from_min_size(pos2(film.left() + 4.0 + i as f32 * 17.0, film.top() + 3.0), vec2(15.0, 12.0));
        if cell.right() > film.right() - 2.0 {
            break;
        }
        let selected = i == 1;
        painter.rect_filled(cell, 0.0, if selected { p.cell_selected } else { p.cell });
        let thumb = cell.shrink(2.5);
        painter.rect_filled(thumb, 0.0, Color32::from_rgb(0x6f + 8 * i as u8, 0x80, 0x8f));
        if selected {
            painter.rect_stroke(thumb, 0.0, Stroke::new(1.0, p.pick), StrokeKind::Outside);
        }
    }
}

// ------------------------------------------------------------------------------- Open Library…

/// Switching libraries is refused while an import (or the folder scan before one) is running: its
/// remaining batches would be committed into the new library (issue #1509).
pub(crate) fn can_switch_library(app: &LightcraftApp) -> Result<(), String> {
    if crate::lightroom_import::is_running(app) {
        return Err("wait for Lightroom catalog import to finish before switching libraries".into());
    }
    if app.import.is_some() || app.scan.is_some() {
        return Err("wait for the import to finish before switching libraries".into());
    }
    Ok(())
}

/// `app.openLibrary {path?}`: close the current library and open (or create) the one at `path`,
/// or a folder chosen in a dialog. Remembered as the library to open at launch.
///
/// A Lightroom Classic catalog (its folder, an `.lrdata` / `.lrcat-data` folder or the `.lrcat`
/// itself, see [`lightcraft_engine::lightroom_folder`]) is not opened: the current library stays
/// and a dialog offers Import Lightroom Catalog… instead; returns `{opened: false,
/// lightroomCatalog: <.lrcat path or null>, path}`.
pub fn open_library(app: &mut LightcraftApp, p: &Value) -> Result<Value, String> {
    can_switch_library(app)?;
    let path = match p.get("path").and_then(Value::as_str) {
        Some(x) => x.to_string(),
        None => {
            let req = crate::pick::PickRequest::folder(crate::i18n::tr("Open Library"));
            match crate::pick::ask(app, "app.openLibrary", p, "path", req, |s| s.pick_folder.as_mut().and_then(|f| f()).map(|x| vec![x])) {
                crate::pick::Picked::Now(v) => match v.into_iter().next() {
                    Some(x) => x,
                    None => return Ok(Value::Null),
                },
                crate::pick::Picked::Later => return Ok(Value::Null),
                crate::pick::Picked::Unavailable => return Err("no folder dialog on this platform".into()),
            }
        }
    };
    // A Lightroom catalog folder (or part of one) is not a library: before closing the library
    // open now, offer to import it instead (issue #891). The engine refuses it too.
    if let Some(place) = lightcraft_engine::lightroom_folder::lightroom_catalog_at(std::path::Path::new(&path)) {
        let catalog = place.catalog.map(|c| c.to_string_lossy().into_owned());
        let err = lightcraft_engine::EngineError::LightroomCatalog { path: path.clone().into(), catalog: catalog.clone().map(Into::into) };
        // the library problem window (no library could be opened) shows the error instead
        if app.library_problem.is_some() {
            return Err(err.to_string());
        }
        app.ui.dialog = Some(crate::state::Dialog::LightroomCatalog { folder: path.clone(), catalog: catalog.clone() });
        return Ok(json!({"opened": false, "lightroomCatalog": catalog, "path": path}));
    }
    // a remembered library that is gone: say so (and forget it) instead of creating an empty one there
    if app.ui.settings.recent_libraries.contains(&path) && !std::path::Path::new(&path).exists() {
        app.ui.settings.recent_libraries.retain(|r| r != &path);
        return Err(format!("{path}: the library is no longer there (moved, renamed or on a disconnected drive); removed from Open Recent Library"));
    }
    switch_library(app, &path)
}

/// Close the current library and open the one at `path`. If it can't be opened (locked by
/// another program, unreadable, a file), the error is returned and the current library stays open:
/// the target is checked before anything is closed, and if opening it still fails the previous
/// library is opened again (both errors are reported if that fails too).
fn switch_library(app: &mut LightcraftApp, path: &str) -> Result<Value, String> {
    let dir = app.session.check_library(std::path::Path::new(path)).map_err(|e| format!("{path}: {e}"))?;
    let path = dir.to_string_lossy().into_owned();
    // the library being left goes in the list too (also when it was opened at launch)
    let previous_dir = app.session.library.as_ref().filter(|l| l.on_disk).map(|l| l.dir.clone());
    let previous = previous_dir.as_ref().map(|d| d.to_string_lossy().into_owned());
    app.session.close_library().map_err(|e| e.to_string())?;
    let opened = app.session.open_library(&dir, false).map(|_| ()).map_err(|e| format!("{path}: {e}"));
    app.lightroom_last = None;
    app.lightroom_result = None;
    if matches!(app.ui.dialog.as_ref(), Some(crate::state::Dialog::LightroomResult { .. })) {
        app.ui.dialog = None;
    }
    app.renderer.forget_all();
    if let Err(err) = opened {
        let Some(prev) = previous_dir else { return Err(err) };
        return match app.session.open_library(&prev, false) {
            Ok(_) => Err(err),
            Err(e) => Err(format!("{err}; reopening the previous library {} failed too: {e}", prev.display())),
        };
    }
    // Photo IDs are local to a catalog: pins cannot follow them into another library (a refused
    // or failed switch reopens the same catalog, so its pins stay).
    app.ui.reference = None;
    app.ui.compare = None;
    app.ui.brief_info = crate::state::BriefInfo::default();
    if app.ui.view == crate::state::ViewMode::Reference {
        app.ui.view = crate::state::ViewMode::PhotoGrid;
    }
    app.ui.settings.library_path = path.clone();
    if let Some(prev) = previous.filter(|p| p != &path && !app.ui.settings.recent_libraries.contains(p)) {
        remember_library(&mut app.ui.settings.recent_libraries, &prev);
    }
    remember_library(&mut app.ui.settings.recent_libraries, &path);
    Ok(json!({"path": path, "name": library_name(&dir), "photos": app.session.catalog.len()}))
}

/// Put `path` first in the recent list, without duplicates, capped at [`RECENT_LIBRARIES`].
pub fn remember_library(recent: &mut Vec<String>, path: &str) {
    recent.retain(|r| r != path);
    recent.insert(0, path.to_string());
    recent.truncate(crate::state::RECENT_LIBRARIES);
}

/// A library's name as people see it: its folder's name.
pub fn library_name(dir: &std::path::Path) -> String {
    dir.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| dir.display().to_string())
}

/// The open library's name (`None`: in memory, nothing is saved).
pub fn open_library_name(app: &LightcraftApp) -> Option<String> {
    app.session.library.as_ref().filter(|l| l.on_disk).map(|l| library_name(&l.dir))
}

/// `app.newLibrary {path?, name?}`: create a library and switch to it. With `name`, the library
/// is the folder `name` inside `path` (or inside a location chosen in a dialog); without it,
/// `path` itself, or a new folder named after the default library. The folder must not hold a
/// library already, nor anything else.
pub fn new_library(app: &mut LightcraftApp, p: &Value) -> Result<Value, String> {
    can_switch_library(app)?;
    let name = match p.get("name") {
        None | Some(Value::Null) => None,
        Some(Value::String(s)) => Some(valid_library_name(s)?),
        Some(_) => return Err("app.newLibrary: name must be a string".into()),
    };
    // from the menu: ask for the name first (default prefilled), then for the location
    if name.is_none() && p.get("path").is_none() && app.services.pick_folder.is_some() {
        crate::panels::dialogs::prompt(
            app,
            "New Library",
            "Library name",
            lightcraft_engine::library::DEFAULT_NAME,
            "app.newLibrary",
            json!({}),
            "name",
        );
        return Ok(Value::Null);
    }
    let (location, picked) = match p.get("path").and_then(Value::as_str) {
        Some(x) => (std::path::PathBuf::from(x), false),
        None => {
            let req = crate::pick::PickRequest::folder(crate::i18n::tr("New Library: Choose Where to Put It"));
            match crate::pick::ask(app, "app.newLibrary", p, "path", req, |s| s.pick_folder.as_mut().and_then(|f| f()).map(|x| vec![x])) {
                crate::pick::Picked::Now(v) => match v.into_iter().next() {
                    Some(x) => (std::path::PathBuf::from(x), true),
                    None => return Ok(Value::Null),
                },
                crate::pick::Picked::Later => return Ok(Value::Null),
                crate::pick::Picked::Unavailable => return Err("no folder dialog on this platform".into()),
            }
        }
    };
    let dir = match name {
        Some(n) => location.join(n),
        // a chosen location gets a new folder in it, named after the default library
        None if picked => free_name(&location, lightcraft_engine::library::DEFAULT_NAME),
        None => location,
    };
    if lightcraft_engine::library::is_library(&dir) {
        return Err(format!("{}: there is a library here already; open it with File ▸ Open Library…", dir.display()));
    }
    if dir.is_file() {
        return Err(format!("{}: is a file; a library is a folder", dir.display()));
    }
    if std::fs::read_dir(&dir).is_ok_and(|mut d| d.next().is_some()) {
        return Err(format!("{}: the folder isn't empty; choose an empty folder or a new name", dir.display()));
    }
    std::fs::create_dir_all(&dir).map_err(|e| format!("{}: can't create the library folder: {e}", dir.display()))?;
    switch_library(app, &dir.to_string_lossy())
}

/// Characters a folder name can't hold on Windows (and `/`, which no system allows).
const RESERVED_NAME_CHARS: [char; 9] = ['<', '>', ':', '"', '/', '\\', '|', '?', '*'];

fn valid_library_name(s: &str) -> Result<&str, String> {
    let n = s.trim();
    // Windows drops trailing dots and spaces from a folder name: refuse them rather than
    // creating a folder whose name differs from the one asked for (spaces around the name
    // are trimmed first)
    let bad = n.is_empty() || n == "." || n == ".." || n.ends_with(['.', ' ']) || n.contains(RESERVED_NAME_CHARS) || n.chars().any(char::is_control);
    if bad {
        return Err(format!(
            "app.newLibrary: {s:?} can't be a library name (a folder name without < > : \" / \\ | ? * and not ending in a dot or space)"
        ));
    }
    Ok(n)
}

/// `base` inside `location`, or `base 2`, `base 3`… if that is taken.
pub(crate) fn free_name(location: &std::path::Path, base: &str) -> std::path::PathBuf {
    let first = location.join(base);
    if !first.exists() {
        return first;
    }
    (2..1000u32).map(|i| location.join(format!("{base} {i}"))).find(|p| !p.exists()).unwrap_or(first)
}

/// `app.recentLibraries {remove?: path, clear?: bool}`: the recently opened libraries, newest
/// first, as `[{path, name, exists, checking, open, lastOpened}]`. `exists` is what the last
/// background check found (`false`: moved, on a disconnected drive, not a folder or no longer a
/// library; `null`: not checked yet, or the check couldn't tell — a folder that can't be listed, an
/// unreadable catalog, a lookup or check that failed; `checking` says whether a check is running): the command never
/// looks at a folder itself, as one on an offline network share can take seconds to answer.
/// `lastOpened`: ISO 8601 time, `null` when not known. `remove` forgets that
/// one entry (an error if the list doesn't hold it) and `clear` forgets them all; neither touches
/// anything on disk (issue #1290).
pub fn recent_libraries(app: &mut LightcraftApp, p: &Value) -> Result<Value, String> {
    match p.get("remove") {
        None | Some(Value::Null) => {}
        Some(Value::String(path)) => {
            if !forget_library(&mut app.ui.settings, path) {
                return Err(format!("app.recentLibraries: {path:?} is not in the recent libraries"));
            }
            app.recent_checks.forget(path);
        }
        Some(_) => return Err("app.recentLibraries: remove must be a library path (a string)".into()),
    }
    if p.get("clear").and_then(Value::as_bool) == Some(true) {
        for path in std::mem::take(&mut app.ui.settings.recent_libraries) {
            app.recent_checks.forget(&path);
        }
        app.ui.settings.recent_opened.clear();
    }
    let open = app.session.library.as_ref().filter(|l| l.on_disk).map(|l| l.dir.clone());
    let (settings, checks) = (&app.ui.settings, &app.recent_checks);
    let list: Vec<Value> = settings
        .recent_libraries
        .iter()
        .map(|r| {
            let d = std::path::Path::new(r);
            let exists = checks.exists(r);
            json!({"path": r, "name": library_name(d), "exists": exists, "checking": checks.is_checking(r), "open": open.as_deref() == Some(d), "lastOpened": settings.recent_opened.get(r)})
        })
        .collect();
    Ok(Value::Array(list))
}

/// Forget `path` in File ▸ Open Recent Library (and when it was opened); the library itself is
/// left where it is. False if the list didn't hold it.
pub fn forget_library(settings: &mut crate::state::AppSettings, path: &str) -> bool {
    let before = settings.recent_libraries.len();
    settings.recent_libraries.retain(|r| r != path);
    settings.recent_opened.remove(path);
    settings.recent_libraries.len() != before
}

// ---------------------------------------------------------------------------- Display profile

/// `app.displayProfile {path?: string | null}`: show previews through this monitor ICC profile
/// (`""` or `null`: none, the profile of the screen showing the window where the host knows it
/// (macOS), else the display is treated as sRGB) and keep it as the setting; no `path`: report
/// only. A file that can't be used is an error and changes nothing. Returns the profile in use:
/// `{path, description, kind (matrix|lut), primaries {red, green, blue: [x, y]}}`, with
/// `path: null, screen: true` for the screen's profile.
pub fn display_profile(app: &mut LightcraftApp, p: &Value) -> Result<Value, String> {
    let path = match p.get("path") {
        None => None,
        Some(Value::Null) => Some(String::new()),
        Some(Value::String(s)) => Some(s.trim().to_string()),
        Some(_) => return Err("app.displayProfile: path must be a string or null".into()),
    };
    if let Some(path) = path {
        let d = lightcraft_engine::display::Display::load_opt(&path).map_err(|e| format!("app.displayProfile: {e}"))?;
        app.renderer.set_display(d);
        app.ui.settings.display_profile = path.clone();
        app.display_applied = Some(path);
        app.display_error = None;
        app.screen_profile = None;
        app.screen_profile_read = None;
        if app.ui.settings.display_profile.is_empty() {
            app.apply_screen_profile();
        }
    }
    Ok(match app.renderer.display() {
        Some(d) if d.path == crate::SCREEN_PROFILE => {
            let mut v = d.describe();
            v["path"] = Value::Null;
            v["screen"] = json!(true);
            v
        }
        Some(d) => d.describe(),
        None => json!({"path": null, "description": "sRGB (no display profile)"}),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn app_with_import() -> LightcraftApp {
        let mut app = LightcraftApp::new(lightcraft_engine::Session::with_demo(), crate::Services::default());
        app.run("photo.setReference", json!({})).unwrap();
        app.import = Some(crate::import::ImportTask::new(vec!["a.jpg".into()], json!({"mode": "add"}), 0, false));
        app
    }

    #[test]
    fn open_library_is_refused_while_an_import_runs() {
        let mut app = app_with_import();
        let pin = app.ui.reference;
        let err = open_library(&mut app, &json!({"path": "other"})).unwrap_err();
        assert!(err.contains("import to finish"), "{err}");
        assert!(app.import.is_some());
        assert_eq!(app.ui.reference, pin, "a refused Open Library keeps the reference pin");
    }

    #[test]
    fn new_library_is_refused_while_an_import_runs() {
        let mut app = app_with_import();
        let pin = app.ui.reference;
        let err = new_library(&mut app, &json!({"path": "other"})).unwrap_err();
        assert!(err.contains("import to finish"), "{err}");
        assert!(app.import.is_some());
        assert_eq!(app.ui.reference, pin, "a refused New Library keeps the reference pin");
    }

    /// The menu items that switch libraries are greyed out meanwhile, like during a Lightroom import.
    #[test]
    fn library_switching_menu_items_are_disabled_while_an_import_runs() {
        let mut app = LightcraftApp::new(lightcraft_engine::Session::new(), crate::Services::default());
        app.services.pick_folder = Some(Box::new(|| None));
        assert!(crate::menus::ui_enabled(&app, "app.openLibrary"));
        assert!(crate::menus::ui_enabled(&app, "app.newLibrary"));
        app.import = Some(crate::import::ImportTask::new(vec!["a.jpg".into()], json!({"mode": "add"}), 0, false));
        assert!(!crate::menus::ui_enabled(&app, "app.openLibrary"));
        assert!(!crate::menus::ui_enabled(&app, "app.newLibrary"));
    }

    /// The folder scan before an import's review counts too: it ends in an import into the open library.
    #[test]
    fn open_library_is_refused_while_an_import_scan_runs() {
        let dir = std::env::temp_dir().join(format!("lc-ui-switchscan-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let mut app = LightcraftApp::new(lightcraft_engine::Session::with_demo(), crate::Services::default());
        app.run("photo.setReference", json!({})).unwrap();
        let pin = app.ui.reference;
        crate::import::open(&mut app, vec![dir.to_string_lossy().into_owned()]).unwrap();
        assert!(app.scan.is_some());
        let err = open_library(&mut app, &json!({"path": "other"})).unwrap_err();
        assert!(err.contains("import to finish"), "{err}");
        assert_eq!(app.ui.reference, pin, "a refused Open Library during a scan keeps the reference pin");
        let err = new_library(&mut app, &json!({"path": "other"})).unwrap_err();
        assert!(err.contains("import to finish"), "{err}");
        assert_eq!(app.ui.reference, pin, "a refused New Library during a scan keeps the reference pin");
        assert!(app.scan.is_some());
        let _ = std::fs::remove_dir_all(&dir);
    }
}

#[cfg(test)]
mod path_tests {
    use super::elide_path;

    /// Width = characters, so a limit of 20 is "20 characters".
    fn fit(path: &str, max: usize) -> String {
        elide_path(path, max as f32, |s| s.chars().count() as f32)
    }

    #[test]
    fn a_path_that_fits_is_left_alone() {
        assert_eq!(fit("/Users/me/Pictures", 18), "/Users/me/Pictures");
        assert_eq!(fit("", 5), "");
    }

    #[test]
    fn leading_folders_go_first_and_the_end_stays() {
        assert_eq!(fit("/Users/me/Pictures/Library", 20), "…/Pictures/Library");
        assert_eq!(fit("/Users/me/Pictures/Library", 10), "…/Library");
    }

    /// Windows paths: `Path::display` gives backslashes there.
    #[test]
    fn a_path_with_backslashes_keeps_its_end_too() {
        assert_eq!(fit(r"C:\Users\me\Pictures\Library", 20), r"…\Pictures\Library");
        assert_eq!(fit(r"C:\Users\me/Pictures\Library", 10), r"…\Library");
    }

    #[test]
    fn a_last_folder_too_long_by_itself_loses_its_end() {
        assert_eq!(fit("/Users/me/a-very-long-folder-name", 8), "a-very-…");
        assert_eq!(fit("no-separator-at-all", 8), "no-sepa…");
        assert_eq!(fit("/Users/Фотографии-из-отпуска", 8), "Фотогра…");
    }

    #[test]
    fn a_separator_at_the_end_is_not_where_the_name_starts() {
        assert_eq!(fit("/Users/me/Pictures/Library/", 11), "…/Library/");
        assert_eq!(fit("/Users/me/a-very-long-folder-name/", 8), "a-very-…");
        assert_eq!(fit(r"C:\Users\me\Library\", 11), r"…\Library\");
        assert_eq!(fit("///", 2), "…");
        assert_eq!(fit(r"\\server\share\folder", 15), r"…\share\folder");
    }
}
