use super::general::in_memory_note;
use super::widgets::{ask_first, choices, heading, hint, row};
#[cfg(not(target_arch = "wasm32"))]
use super::widgets::{label_width, path_label};
use super::{
    Button, CACHE_SIZES, HOT_PIXEL_LIMIT_DEFAULT, HOT_PIXEL_LIMITS, LABEL_W, LightcraftApp, PREVIEW_LIMITS, RichText, Tokens, Value, json, register,
};
use crate::check::check;

// --------------------------------------------------------------------------------- Performance

/// "GPU: <adapter>", or "GPU: starting…" (in the language shown) while the adapter is not known yet.
pub(crate) fn gpu_line(adapter: Option<String>) -> String {
    format!("GPU: {}", adapter.unwrap_or_else(|| crate::i18n::tr("starting…").to_string()))
}

pub(super) fn performance_tab(app: &mut LightcraftApp, ui: &mut egui::Ui, t: &Tokens) {
    use lightcraft_engine::gpu;
    heading(ui, t, "Rendering");
    #[cfg(target_os = "windows")]
    check(ui, "settings.composeWindow", &mut app.ui.settings.compose_window, "Prevent G-SYNC/FreeSync flicker (applies at next launch)");
    check(ui, "settings.gpu", &mut app.ui.settings.gpu, "Use the GPU for rendering");
    // `available` would wait for a device still being created (issue #136)
    let status = if gpu::enabled() && !gpu::ready() {
        // the device is created on another thread, which doesn't wake the UI when it's done;
        // make sure that thread runs (cheap and idempotent: a start lost to an off/on switch
        // can't leave "starting" for good)
        gpu::warm_up();
        ui.ctx().request_repaint_after(std::time::Duration::from_millis(250));
        "GPU: starting…".to_string()
    } else if !gpu::available() {
        match gpu::unavailable_reason() {
            Some(why) => crate::i18n::tr_format!("Rendering on the CPU: {why}", why = why),
            None => "No usable GPU found: rendering on the CPU".to_string(),
        }
    } else {
        gpu_line(gpu::adapter_name())
    };
    // One line, the whole text on hover: the reason can be a long driver message (no adapter
    // among the backends …), and the GPU state is process-wide, so it may change between frames;
    // a hint that wrapped would move every control below it under the pointer.
    ui.add(egui::Label::new(RichText::new(crate::i18n::tr(&status)).font(t.caption()).color(t.legible(t.text_dim, t.chrome, 4.5))).truncate())
        .on_hover_text(&status);
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
    row(ui, t, crate::i18n::tr("Hot pixel limit"), |ui| {
        let opts: Vec<(f32, String)> = HOT_PIXEL_LIMITS
            .iter()
            .map(|p| (*p, if *p == HOT_PIXEL_LIMIT_DEFAULT { format!("{p} % ({})", crate::i18n::tr("default")) } else { format!("{p} %") }))
            .collect();
        let opts: Vec<(f32, &str)> = opts.iter().map(|(p, l)| (*p, l.as_str())).collect();
        choices(ui, "settingsHotPixels", &opts, &mut app.ui.settings.hot_pixel_limit);
    });
    hint(
        ui,
        t,
        crate::i18n::tr(
            "Remove Hot Pixels (Detail) leaves a raw unchanged when more than this share of its photosites would be replaced: such a photo doesn't look like hot pixels on a sensor.",
        ),
    );
    heading(ui, t, "Preview cache");
    let cur = app.session.cache_mb;
    row(ui, t, crate::i18n::tr("Size limit"), |ui| {
        let gb = lightcraft_engine::media::default_disk_cache_bytes(app.session.catalog.len()).div_ceil(1 << 30);
        let auto = format!("{} ({gb} GB)", crate::i18n::tr("Automatic"));
        let custom = custom_cache_option(cur);
        let mut opts =
            vec![(512u32, "512 MB"), (1024, "1 GB"), (2048, "2 GB"), (4096, "4 GB"), (8192, "8 GB"), (51_200, "50 GB"), (204_800, "200 GB")];
        opts.extend(custom.as_ref().map(|(mb, l)| (*mb, l.as_str())));
        opts.push((0, auto.as_str()));
        // (a saved size the list doesn't offer shows as its own option, not as Automatic: picking
        // Automatic then changes it; CodeRabbit on #3583)
        let mut v = if CACHE_SIZES.contains(&cur) || custom.is_some() { cur } else { 0 };
        if choices(ui, "settingsCache", &opts, &mut v) {
            app.act("library.preferences", json!({"cacheMb": v}));
        }
    });
    hint(
        ui,
        t,
        crate::i18n::tr(
            "Grid thumbnails and the previews the loupe shows, kept on disk so photos open at once next time. Automatic makes room for every photo in this library (about 1.5 MB a photo). 50 GB: normal · 200 GB: better, for several weddings or large shoots.",
        ),
    );
    let used = app.session.media.rendered.disk().map(|d| d.size());
    row(ui, t, crate::i18n::tr("In use"), |ui| {
        ui.label(
            RichText::new(used.map(|b| format!("{:.1} MB", b as f64 / 1048576.0)).unwrap_or_else(|| crate::i18n::tr("memory only").to_string()))
                .color(t.text),
        );
        let r = Button::new("Clear Cache").small().widget("button:settingsClearCache").show(ui);
        let r = ask_first(
            t,
            "button:settingsClearCache",
            r,
            "Thumbnails and previews are made again as you browse, which takes a while in a large library. Your photos and edits are not touched.",
            "Clear Cache",
        );
        if r.clicked() {
            app.act("library.clearPreviews", json!({}));
        }
    });
    background_prep(app, ui, t);
    // The size limit is saved with the library (`library.preferences`): the note follows it.
    in_memory_note(app, ui, t);
}

/// Local folders: when photos only seen in Local, never added or changed, leave the catalog.
pub(super) fn local_folders(app: &mut LightcraftApp, ui: &mut egui::Ui, t: &Tokens) {
    heading(ui, t, "Local folders");
    row(ui, t, crate::i18n::tr("Forget unchanged photos"), |ui| {
        let opts = [(0u32, "Never"), (7, "After 7 days"), (30, "After 30 days"), (90, "After 90 days"), (365, "After a year")];
        let mut v = app.session.forget_local_days;
        if choices(ui, "settingsForgetLocal", &opts, &mut v) {
            app.act("library.preferences", json!({"forgetLocalDays": v}));
        }
    });
    hint(
        ui,
        t,
        crate::i18n::tr(
            "Photos seen in Local but never added or changed leave the catalog when their folder hasn't been browsed for this long (checked when the library opens). Files stay on disk; browsing the folder shows them again.",
        ),
    );
}

/// Settings ▸ Performance ▸ Prepare photos in the background (notes 47 and 48): per mode, what
/// is made ready before the user gets to it (`prefs.backgroundPrep`), app-wide or for this library.
fn background_prep(app: &mut LightcraftApp, ui: &mut egui::Ui, t: &Tokens) {
    use lightcraft_engine::prep::{Mode, Scope};
    heading(ui, t, "Prepare photos in the background");
    hint(
        ui,
        t,
        crate::i18n::tr(
            "While you go through photos in the loupe, the next ones are made ready before you reach them, and when the computer is otherwise idle the previews of every photo in view are built. The photo on screen always comes first; the work shows in the activity list, where it can be stopped.",
        ),
    );
    let own = app.session.prep.library.is_some();
    let lib_open = app.session.library.is_some();
    if lib_open {
        let mut v = own;
        if check(ui, "settings.prepOwn", &mut v, "Use different settings for this library").changed() {
            let r = if v {
                let cur = app.session.prep_settings();
                app.run("prefs.setBackgroundPrep", json!({"library": true, "settings": cur}))
            } else {
                app.run("prefs.setBackgroundPrep", json!({"reset": true}))
            };
            if let Err(e) = r {
                log::warn!("background preparation: {e}");
            }
        }
        hint(ui, t, crate::i18n::tr("For example, a timelapse library can prepare nothing while a wedding library prepares everything."));
    }
    let cur = app.session.prep_settings();
    let mut patch: Option<Value> = None;
    let scopes = [(Scope::All, "Every photo"), (Scope::Marked, "Picks, rated and labelled"), (Scope::OnScreen, "Only the photo on screen")];
    for (mode, key, title) in [(Mode::Culling, "culling", "Culling (a folder before import)"), (Mode::Library, "library", "Library")] {
        let m = cur.mode(mode);
        ui.add_space(4.0);
        ui.label(RichText::new(crate::i18n::tr(title)).font(t.semibold(12.0)).color(t.text));
        let mut previews = m.previews;
        row(ui, t, crate::i18n::tr("Previews"), |ui| {
            if check(ui, &format!("settings.prep.{key}.previews"), &mut previews, "Build previews for every photo in view").changed() {
                patch = Some(json!({key: {"previews": previews}}));
            }
        });
        hint(ui, t, crate::i18n::tr("Starts once you step through a few photos. Uses about 1.5 MB of disk a photo, and the computer's spare time."));
        let mut ahead = m.ahead;
        row(ui, t, crate::i18n::tr("Photos ahead"), |ui| {
            let opts = [(0u8, "Off"), (3, "3"), (5, "5"), (10, "10"), (20, "20")];
            if choices(ui, &format!("settingsPrepAhead-{key}"), &opts, &mut ahead) {
                patch = Some(json!({key: {"ahead": ahead}}));
            }
        });
        let mut scope = m.scope;
        row(ui, t, crate::i18n::tr("Which photos"), |ui| {
            if choices(ui, &format!("settingsPrepScope-{key}"), &scopes, &mut scope) {
                patch = Some(json!({key: {"scope": scope}}));
            }
        });
    }
    ui.add_space(4.0);
    ui.label(RichText::new(crate::i18n::tr("Editing")).font(t.semibold(12.0)).color(t.text));
    let mut ahead = cur.develop.ahead;
    row(ui, t, crate::i18n::tr("Photos ahead"), |ui| {
        let opts = [(0u8, "Off"), (1, "1"), (2, "2"), (3, "3"), (5, "5")];
        if choices(ui, "settingsPrepAhead-develop", &opts, &mut ahead) {
            patch = Some(json!({"develop": {"ahead": ahead}}));
        }
    });
    hint(
        ui,
        t,
        crate::i18n::tr(
            "The next photos are opened and kept ready to edit, so the first slider move on them is quick. Uses memory: a few hundred MB a photo.",
        ),
    );
    let mut scope = cur.develop.scope;
    row(ui, t, crate::i18n::tr("Which photos"), |ui| {
        if choices(ui, "settingsPrepScope-develop", &scopes, &mut scope) {
            patch = Some(json!({"develop": {"scope": scope}}));
        }
    });
    if let Some(patch) = patch
        && let Err(e) = app.run("prefs.setBackgroundPrep", json!({"library": own, "settings": patch}))
    {
        log::warn!("background preparation: {e}");
    }
}

/// What the Smart previews section ([`smart_previews`]) keeps between frames: what is chosen for the previews already
/// built (0 move, 1 leave, 2 delete) and a change of folder that waits for a yes because it would
/// delete them, with the frame, library and folder they were chosen and asked in.
#[cfg(not(target_arch = "wasm32"))]
#[derive(Clone, Default)]
struct SmartAsk {
    pass: u64,
    library: String,
    folder: String,
    mode: u8,
    waiting: Option<Value>,
}
/// What the smart previews row says after the path of a folder the library was not given: "(library
/// default)", in the language shown. (The catalogs have that message with the two spaces that
/// follow a path.)
#[cfg(not(target_arch = "wasm32"))]
pub(crate) fn smart_folder_default() -> String {
    crate::i18n::tr("  (library default)").trim().to_string()
}

/// Where this library keeps its smart previews (the offline-editing proxies, which can be large):
/// the effective folder, what is in it, and choosing another one.
#[cfg(not(target_arch = "wasm32"))]
pub(super) fn smart_previews(app: &mut LightcraftApp, ui: &mut egui::Ui, t: &Tokens) {
    let cache = egui::Id::unique("smart-location");
    let Some(loc) = smart_location(app, ui, cache) else { return };
    heading(ui, t, "Smart previews");
    let path = loc["path"].as_str().unwrap_or_default().to_string();
    let custom = loc["custom"].as_bool().unwrap_or(false);

    smart_folder_rows(ui, t, &loc);
    let (count, bytes) = (loc["count"].as_u64().unwrap_or(0), loc["bytes"].as_u64().unwrap_or(0));
    // what happens to the previews already built when the folder changes
    // That choice, and a change waiting for an answer, are kept from one frame of this section to
    // the next only, and only for this library and this folder: with Settings closed, another
    // tab or library opened, or the folder changed some other way, nothing is left chosen or asked.
    let ask_id = smart_ask_id();
    // (counted in frames: the first pass of a frame that opens a prompt draws the prompt alone, so
    // passes skip a number while Settings is still the same window)
    let pass = ui.ctx().cumulative_frame_nr();
    let library = smart_library(app);
    let kept: Option<SmartAsk> = ui.data(|d| d.get_temp(ask_id));
    let mut ask = kept.filter(|k| pass.saturating_sub(k.pass) <= 1 && k.library == library && k.folder == path).unwrap_or_default();
    // a folder chosen in a dialog that ran off the UI thread, kept until this section is drawn
    // (of this library: a folder chosen for another one, opened while the dialog was up, is not taken up for this)
    let picked =
        ui.data_mut(|d| d.remove_temp::<(String, String, u8)>(smart_picked_id())).filter(|(lib, _, _)| *lib == library).map(|(_, dir, m)| (dir, m));
    let mut mode = picked.as_ref().map_or(ask.mode, |(_, m)| *m);
    if count > 0 {
        row(ui, t, crate::i18n::tr("Existing previews"), |ui| {
            choices(ui, "settingsSmartExisting", &[(0u8, "Move them"), (1, "Leave them"), (2, "Delete them")], &mut mode);
        });
    }
    let mut existing = ["move", "leave", "discard"][mode.min(2) as usize];
    // Deleting the previews already built is asked about once more, when the folder they would
    // be deleted for is known: until then the change (the command's parameters) waits here.
    // (asked for "Delete them" only: choosing otherwise withdraws the question)
    let mut waiting: Option<Value> = ask.waiting.take().filter(|_| existing == "discard");
    // The choice that was up when the dialog closed is the one it was parked with, whatever this section last drew
    // (a tab left while the dialog was up lets the remembered choice go stale): "Delete them" stays the question.
    let mut change = picked.map(|(dir, _)| json!({"path": dir, "existing": existing}));
    ui.horizontal(|ui| {
        ui.add_space(LABEL_W);
        if app.services.pick_folder.is_some() {
            let r = Button::new("Choose Folder…").widget("button:settingsSmartChoose").show(ui);
            if r.clicked() {
                // the dialog runs off the UI thread; the folder is taken up when it closes
                crate::pick::choose_and_apply(app, ui.ctx(), crate::pick::Field::SmartPreviewsFolder);
            }
        }
        let r = Button::new("Use Library Folder").enabled(custom).widget("button:settingsSmartReset").show(ui);
        if r.clicked() {
            change = Some(json!({"reset": true, "existing": existing}));
        }
    });
    if existing == "discard" && count > 0 && change.is_some() {
        waiting = change.take();
    }
    let mut answered = false;
    if let Some(params) = &waiting {
        // (said with the count line above and two plain sentences, so no new format string is needed)
        let how_many =
            crate::i18n::tr_format!("{count} smart preview{} · {:.1} MB", if count == 1 { "" } else { "s" }, bytes as f64 / 1048576.0, count = count);
        let question = format!(
            "{} {how_many}. {}",
            crate::i18n::tr("Delete the smart previews in the folder used until now?"),
            crate::i18n::tr(
                "Photos whose originals are not connected are shown and edited from their smart previews; new ones can only be made from originals that are connected."
            ),
        );
        ui.add(egui::Label::new(RichText::new(question).color(t.text_caution())).wrap());
        ui.horizontal(|ui| {
            let r = Button::new("Delete and Change Folder").widget("button:settingsSmartDelete").show(ui);
            if r.clicked() {
                // (what the section shows is up to two seconds old: the folder is asked for
                // again, and a question about a folder that is no longer the one is dropped)
                let still = app.run("library.smartPreviewsLocation", json!({})).is_ok_and(|now| now["path"].as_str() == Some(path.as_str()));
                if still {
                    change = Some(params.clone());
                    existing = "discard";
                } else {
                    ui.data_mut(|d| d.remove::<(f64, Value)>(cache));
                }
                answered = true;
            }
            let r = Button::new("Cancel").widget("button:settingsSmartCancel").show(ui);
            answered |= r.clicked();
        });
    }
    if answered || change.is_some() {
        // (the choice goes back to "Move them": deleting is never left set for the next time)
        waiting = None;
        mode = 0;
    }
    ui.data_mut(|d| d.insert_temp(ask_id, SmartAsk { pass, library, folder: path.clone(), mode, waiting }));
    let result = change.map(|params| app.run("library.smartPreviewsLocation", params));
    if let Some(r) = result {
        ui.data_mut(|d| d.remove::<(f64, Value)>(cache));
        smart_change_toast(app, ui, r, existing);
    }
    hint(ui, t, crate::i18n::tr("Keep it on a drive with room: smart previews are about 1 MB per photo. The thumbnail cache stays in the library."));
}

/// The smart previews' location, listed at most every 2 s (listing a big folder every frame would be slow)
/// and again after a change (`cache` is dropped then); `None` when the library has none.
#[cfg(not(target_arch = "wasm32"))]
fn smart_location(app: &mut LightcraftApp, ui: &egui::Ui, cache: egui::Id) -> Option<Value> {
    let now = ui.input(|i| i.time);
    let loc: Value = match ui.data(|d| d.get_temp::<(f64, Value)>(cache)) {
        Some((at, v)) if now - at < 2.0 => v,
        _ => {
            let v = app.run("library.smartPreviewsLocation", json!({})).unwrap_or(Value::Null);
            ui.data_mut(|d| d.insert_temp(cache, (now, v.clone())));
            v
        }
    };
    (!loc.is_null()).then_some(loc)
}

/// The Folder row and the line under it: how many previews are there, or that the folder is not available.
#[cfg(not(target_arch = "wasm32"))]
fn smart_folder_rows(ui: &mut egui::Ui, t: &Tokens, loc: &Value) {
    let path = loc["path"].as_str().unwrap_or_default();
    let available = loc["available"].as_bool().unwrap_or(true);
    let custom = loc["custom"].as_bool().unwrap_or(false);
    row(ui, t, crate::i18n::tr("Folder"), |ui| {
        let default = smart_folder_default();
        path_label(
            ui,
            "settingsSmartFolder",
            path,
            egui::TextStyle::Body.resolve(ui.style()),
            if available { t.text } else { t.text_error() },
            if custom { 0.0 } else { label_width(ui, &default, egui::TextStyle::Body) },
        );
        if !custom {
            let r = ui.label(RichText::new(default).color(t.text));
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
                crate::i18n::plural_s(count as usize),
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
}

/// Says what a folder change did: how many previews were moved or deleted, and how many failed.
#[cfg(not(target_arch = "wasm32"))]
fn smart_change_toast(app: &mut LightcraftApp, ui: &egui::Ui, result: Result<Value, String>, existing: &str) {
    match result {
        Ok(v) => {
            let failed = v["failed"].as_array().map_or(0, Vec::len);
            let msg = match (existing, v["handled"].as_u64().unwrap_or(0)) {
                (_, 0) => "Smart previews folder changed".to_string(),
                ("move", n) => format!("Smart previews folder changed; moved {n}"),
                ("discard", n) => format!("Smart previews folder changed; deleted {n}"),
                _ => "Smart previews folder changed".to_string(),
            };
            let msg = if failed > 0 { crate::i18n::tr_format!("{msg} ({failed} failed)", failed = failed, msg = msg) } else { msg };
            if failed > 0 {
                app.toast_error(ui.ctx(), msg);
            } else {
                app.toast(ui.ctx(), msg);
            }
        }
        Err(e) => app.toast_error(ui.ctx(), e),
    }
}

/// The cache size limit saved in the settings, as an option of its own when the list doesn't offer
/// it (set through `library.preferences`, or by an older version): its value and label.
fn custom_cache_option(cur: u32) -> Option<(u32, String)> {
    if cur == 0 || CACHE_SIZES.contains(&cur) {
        return None;
    }
    let label = if cur.is_multiple_of(1024) { format!("{} GB", cur / 1024) } else { format!("{cur} MB") };
    Some((cur, label))
}

/// Where a folder chosen in the file dialog waits for the smart previews section to take it up.
#[cfg(not(target_arch = "wasm32"))]
fn smart_picked_id() -> egui::Id {
    egui::Id::unique("smart-picked")
}

/// Where the section keeps its [`SmartAsk`] between frames.
#[cfg(not(target_arch = "wasm32"))]
fn smart_ask_id() -> egui::Id {
    egui::Id::unique("smart-ask")
}

/// The library the smart previews section is about, as [`SmartAsk`] and a folder chosen for it name it.
#[cfg(not(target_arch = "wasm32"))]
fn smart_library(app: &LightcraftApp) -> String {
    app.session.library.as_ref().map(|l| l.dir.display().to_string()).unwrap_or_default()
}

/// A folder was chosen for the smart previews (the dialog may have run off the UI thread): the
/// section above takes it up when it is next drawn, with the "Existing previews" choice it was
/// parked with (and, for "Delete them", the one more question), as for a folder chosen with the
/// synchronous service. It waits for as long as that takes (another tab may be showing); one for a
/// library that is no longer the open one is dropped, as a dialog's answer is.
#[cfg(not(target_arch = "wasm32"))]
pub(crate) fn smart_folder_picked(app: &LightcraftApp, ctx: &egui::Context, path: String) {
    let library = smart_library(app);
    let mode = ctx.data(|d| d.get_temp::<SmartAsk>(smart_ask_id())).filter(|k| k.library == library).map_or(0, |k| k.mode);
    ctx.data_mut(|d| d.insert_temp(smart_picked_id(), (library, path, mode)));
    ctx.request_repaint();
}

#[cfg(test)]
mod tests {
    #[test]
    fn a_saved_cache_size_the_list_does_not_offer_is_an_option() {
        assert_eq!(super::custom_cache_option(0), None, "Automatic");
        assert_eq!(super::custom_cache_option(51_200), None, "offered");
        assert_eq!(super::custom_cache_option(10_240), Some((10_240, "10 GB".to_string())));
        assert_eq!(super::custom_cache_option(3000), Some((3000, "3000 MB".to_string())));
    }
}
