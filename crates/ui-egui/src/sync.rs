//! Synchronize Folder in the app: the folder is scanned on a worker thread (a network share can
//! take minutes; no frame waits for it) while its dialog is open, and the dialog then says what
//! changed and what to do about it. Synchronize runs `folder.synchronize`, which acts on exactly
//! the scan the dialog showed (see `lightcraft_engine::sync`). Both the scan and the run show a row in the activity
//! stack (issue #345); its ✕ stops them like the dialog's Cancel.
//!
//! The scan need not be waited for: Continue in Background closes the dialog and the scan goes
//! on under its row. When it is done, a notice in the activity stack says so ([`SyncReady`]);
//! a click on it opens the dialog with what was found, whenever the person is ready for it.
//!
//! A Synchronize asked for while an import or another synchronize runs waits in line behind it (the library lane,
//! [`lightcraft_engine::activity::Lane::Library`]). When its turn comes it scans the folder again ([`SyncRescan`]) and
//! then runs as it was asked, but it removes, relinks or reads sidecars of only the photos its dialog showed: one that
//! went missing, moved or got a sidecar update meanwhile was never shown to the person, so it is left alone, and the
//! end says so.

use crate::check::Check;
use std::sync::Arc;
use std::sync::atomic::Ordering;

use lightcraft_engine::activity::{Cancel, Ending, Failure, Outcome, Request, TaskGuard};
use lightcraft_engine::import::{ReadChoice, ReadDialog};
use lightcraft_engine::sync::{FolderChanges, SyncChoice, SyncCommit, SyncInput, SyncJob, SyncProgress, SyncStep, scan_with};
use serde_json::{Value, json};

use crate::LightcraftApp;
use crate::state::{Dialog, SyncCounts};
use crate::theme::Tokens;

/// A running scan of the dialog's folder.
pub struct SyncTask {
    path: String,
    /// What the dialog calls the folder (its row's detail).
    name: String,
    disk: bool,
    /// The dialog was closed with Continue in Background: the scan goes on without it.
    background: bool,
    /// What was ticked in the dialog when it was closed that way (a dialog that had scanned
    /// before and was scanning again keeps its ticks): the dialog opens with them again.
    choice: SyncChoice,
    progress: Arc<SyncProgress>,
    rx: std::sync::mpsc::Receiver<FolderChanges>,
    /// Its row in the activity stack; ✕ cancels the scan and closes the dialog.
    guard: TaskGuard,
}

/// What a scan's row shows: (done, of how many, detail). While the folder is listed, which comes
/// first and on a network share can take minutes, a bar that moves on its own and how many
/// folders and files were listed (a count stuck at "0 of 64085" looks like a scan that stopped);
/// then the files and photos checked.
pub(crate) fn scan_row(progress: &SyncProgress, name: &str) -> (u64, u64, String) {
    match progress.listing() {
        Some((folders, files)) => (0, 0, format!("{} · {name}", listing_text(folders, files))),
        None => {
            let (done, total) = progress.counts();
            (done as u64, total as u64, name.to_string())
        }
    }
}

fn listing_text(folders: usize, files: usize) -> String {
    crate::i18n::tr_format!("Listing folders — folders: {folders}, files: {files}", folders = folders, files = files)
}

/// Open the dialog for folder `path` (called `name` in it) and start scanning. One folder at a
/// time: while another folder is scanned in the background, or what was found in it waits to
/// be looked at, this is refused (minutes of scanning are not thrown away unasked); the same
/// folder is scanned afresh.
pub fn open(app: &mut LightcraftApp, path: &str, name: &str, disk: bool) -> Result<(), String> {
    if let Some(other) = app.sync.as_ref().filter(|t| t.background && t.path != path) {
        return Err(crate::i18n::tr_format!("“{name}” is still being scanned: wait for it, or stop it with ✕ on its row", name = other.name));
    }
    if let Some(other) = app.sync_ready.as_ref().filter(|r| r.path != path) {
        return Err(crate::i18n::tr_format!("Look at the changes found in “{name}” first, or dismiss them", name = other.name));
    }
    // (a folder that can't be synchronized leaves what there is alone)
    let input = SyncInput::new(&mut app.session, path, disk).map_err(|e| e.to_string())?;
    cancel(app);
    let progress = Arc::new(SyncProgress::default());
    let (tx, rx) = std::sync::mpsc::channel();
    let p = progress.clone();
    let job = move || {
        let _ = tx.send(scan_with(input, &p));
    };
    #[cfg(not(target_arch = "wasm32"))]
    std::thread::spawn(job);
    #[cfg(target_arch = "wasm32")]
    job();
    // (kind `scan`, not `sync`: it only reads, and doesn't hold up an import)
    let guard = app.session.activity.start("scan", "Looking for changes", Cancel::Flag(progress.files.cancel.clone()));
    guard.detail(name);
    app.sync = Some(SyncTask {
        path: path.to_string(),
        name: name.to_string(),
        disk,
        background: false,
        choice: SyncChoice::default(),
        progress,
        rx,
        guard,
    });
    let d = SyncChoice::default();
    app.ui.dialog = Some(Dialog::SynchronizeFolder {
        path: path.to_string(),
        name: name.to_string(),
        disk,
        counts: None,
        import_new: d.import_new,
        // as the library's setting was last left
        import_duplicates: app.session.import_defaults.import_duplicates,
        relink_moved: d.relink_moved,
        remove_missing: d.remove_missing,
        read_metadata: d.read_metadata,
        // as the dialog was last used, else as the library has it
        read: app.session.import_defaults.read_in(ReadDialog::Synchronize),
    });
    Ok(())
}

/// A scan that finished in the background, waiting to be looked at: the notice in the activity
/// stack. Its changes are `session.folder_changes`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SyncReady {
    pub path: String,
    /// What the dialog calls the folder.
    pub name: String,
    pub disk: bool,
    pub counts: SyncCounts,
    /// What was ticked when the dialog was closed.
    pub choice: SyncChoice,
}

impl SyncReady {
    /// How many changes there are to look at.
    pub fn changes(&self) -> usize {
        let c = &self.counts;
        let copies = if self.choice.import_new && self.choice.import_duplicates { c.copies } else { 0 };
        c.new.saturating_add(copies).saturating_add(c.moved).saturating_add(c.missing).saturating_add(c.metadata)
    }
}

/// A click on the notice: open the dialog with what the background scan found.
pub fn review(app: &mut LightcraftApp) {
    // (a dialog that is open is not taken away; the notice waits)
    if app.ui.dialog.is_some() {
        return;
    }
    let Some(ready) = app.sync_ready.take() else { return };
    let d = ready.choice;
    app.ui.dialog = Some(Dialog::SynchronizeFolder {
        path: ready.path,
        name: ready.name,
        disk: ready.disk,
        counts: Some(ready.counts),
        import_new: d.import_new,
        import_duplicates: d.import_duplicates,
        relink_moved: d.relink_moved,
        remove_missing: d.remove_missing,
        read_metadata: d.read_metadata,
        read: d.read.over(app.session.import_defaults.read),
    });
}

/// ✕ on the notice: what the scan found is let go of.
pub fn dismiss(app: &mut LightcraftApp) {
    if app.sync_ready.is_some() {
        cancel(app);
    }
}

/// Another library was opened: a scan of the one before, running or waiting to be looked at,
/// says nothing about this one.
pub(crate) fn library_changed(app: &mut LightcraftApp) {
    if app.sync.is_some() || app.sync_ready.is_some() || app.sync_owns_changes {
        cancel(app);
        if matches!(app.ui.dialog, Some(Dialog::SynchronizeFolder { .. })) {
            app.ui.dialog = None;
        }
    }
}

/// Stop a running scan and let go of a finished one the dialog made (one an agent made with
/// `folder.scanChanges` is the agent's).
fn cancel(app: &mut LightcraftApp) {
    if let Some(t) = app.sync.take() {
        t.progress.files.cancel.store(true, Ordering::Relaxed);
    }
    app.sync_ready = None;
    if std::mem::take(&mut app.sync_owns_changes) {
        app.session.folder_changes = None;
    }
}

/// Collect a finished scan into the dialog (called every frame). A scan whose dialog was closed
/// is cancelled.
pub fn poll(app: &mut LightcraftApp, ctx: &egui::Context) {
    let open_for = match &app.ui.dialog {
        Some(Dialog::SynchronizeFolder { path, .. }) => Some(path.clone()),
        _ => None,
    };
    let background = app.sync.as_ref().is_some_and(|t| t.background);
    if open_for.is_none() && !background {
        // (what a background scan found is kept for its notice)
        if app.sync.is_some() || (app.sync_owns_changes && app.sync_ready.is_none()) {
            cancel(app);
        }
        return;
    }
    let Some(task) = app.sync.as_ref() else { return };
    if !background && open_for.as_deref() != Some(task.path.as_str()) {
        cancel(app);
        return;
    }
    if task.guard.is_cancelled() {
        // ✕ on its row: a scan stopped part-way has nothing to show
        if let Some(task) = app.sync.take() {
            task.progress.files.cancel.store(true, Ordering::Relaxed);
            task.guard.finish(Outcome::ended(Ending::Stopped));
        }
        cancel(app);
        if !background {
            app.ui.dialog = None;
        }
        return;
    }
    let (done, total, detail) = scan_row(&task.progress, &task.name);
    task.guard.progress(done, total);
    task.guard.detail(&detail);
    ctx.request_repaint_after(std::time::Duration::from_millis(100));
    let changes = match task.rx.try_recv() {
        Ok(c) => c,
        Err(std::sync::mpsc::TryRecvError::Empty) => return,
        Err(_) => {
            // the worker died (a panic, logged)
            if let Some(task) = app.sync.take() {
                task.guard.finish(Outcome::ended(Ending::Died));
            }
            if !background {
                app.ui.dialog = None;
            }
            app.toast_error(ctx, crate::i18n::tr("Scan failed"));
            return;
        }
    };
    let Some(task) = app.sync.take() else { return };
    task.guard.finish(if changes.offline {
        Outcome::failed(crate::i18n::tr("This folder isn't there: it was moved or renamed, or its disk isn't connected."))
    } else if !changes.unreadable.is_empty() {
        unreadable_outcome(&changes)
    } else {
        Outcome::default()
    });
    let c = SyncCounts {
        offline: changes.offline,
        new: changes.new.len(),
        duplicates: changes.duplicates.len(),
        copies: changes.importable_duplicates(&app.session.catalog).len(),
        unreadable: changes.unreadable.len(),
        missing: changes.missing.len(),
        metadata: changes.metadata.len(),
        moved: changes.moved.len(),
    };
    if background {
        let ready = SyncReady { path: task.path.clone(), name: task.name.clone(), disk: task.disk, counts: c, choice: task.choice };
        if !ready.counts.offline && ready.changes() == 0 {
            // nothing to look at: said and done
            app.toast(ctx, format!("{}: {}", ready.name, crate::i18n::tr("The library is up to date with this folder.")));
            return;
        }
        app.session.folder_changes = Some(changes);
        app.sync_owns_changes = true;
        app.sync_ready = Some(ready);
        return;
    }
    app.session.folder_changes = Some(changes);
    app.sync_owns_changes = true;
    if let Some(Dialog::SynchronizeFolder { counts, .. }) = &mut app.ui.dialog {
        *counts = Some(c);
        // the button was Continue in Background until now: Synchronize waits a moment
        app.sync_scanned_at = ctx.input(|i| i.time);
        ctx.request_repaint_after(std::time::Duration::from_secs_f64(SETTLE_SECONDS));
    }
}

/// How long Synchronize stays unavailable after the scan ended (s). The dialog's one button
/// reads Continue in Background while the folder is scanned and Synchronize after: a click meant
/// for the first, landing as the scan ends, must not start the second.
pub const SETTLE_SECONDS: f64 = 0.6;

/// Whether the dialog's Synchronize can be pressed yet ([`SETTLE_SECONDS`]).
pub fn settled(app: &LightcraftApp, ctx: &egui::Context) -> bool {
    ctx.input(|i| i.time) - app.sync_scanned_at >= SETTLE_SECONDS
}

/// The scan's end when files in the folder could not be read: the row names each one by where it is below the
/// scanned folder, with why.
fn unreadable_outcome(changes: &FolderChanges) -> Outcome {
    let roots = [changes.path.clone()];
    let failures = changes
        .unreadable
        .iter()
        .map(|c| Failure { name: crate::import::failure_name(&c.path, &roots), reason: c.error.clone().unwrap_or_default() })
        .take(lightcraft_engine::activity::MAX_FAILURES)
        .collect();
    let failed = changes.unreadable.len();
    Outcome { ending: Ending::Problems, detail: crate::import::not_readable_text(failed), failed, failures, ..Default::default() }
}

/// How far the scan is: the folders and files listed while it lists the folder, else the files
/// and photos checked (done, of how many).
enum ScanStage {
    Listing(usize, usize),
    Checking(usize, usize),
}

fn progress(app: &LightcraftApp) -> Option<ScanStage> {
    app.sync.as_ref().map(|t| match t.progress.listing() {
        Some((folders, files)) => ScanStage::Listing(folders, files),
        None => {
            let (done, total) = t.progress.counts();
            ScanStage::Checking(done, total)
        }
    })
}

/// The dialog's body.
pub fn body(app: &LightcraftApp, ui: &mut egui::Ui, dlg: &mut Dialog) {
    let Dialog::SynchronizeFolder { path, name, counts, import_new, import_duplicates, relink_moved, remove_missing, read_metadata, read, .. } = dlg
    else {
        return;
    };
    let t = Tokens::get(ui.ctx());
    ui.set_min_width(420.0);
    ui.label(crate::i18n::tr("Bring the library up to date with this folder and the folders inside it:"));
    ui.label(egui::RichText::new(name.as_str()).strong());
    ui.label(egui::RichText::new(path.as_str()).color(t.text_dim));
    ui.add_space(6.0);
    let Some(c) = counts.as_ref() else {
        ui.horizontal(|ui| {
            crate::progress::spinner(ui, 14.0);
            match progress(app) {
                Some(ScanStage::Listing(folders, files)) => ui.label(listing_text(folders, files)),
                Some(ScanStage::Checking(done, total)) if total > 0 => ui.label(format!("{} {done} / {total}", crate::i18n::tr("Scanning…"))),
                _ => ui.label(crate::i18n::tr("Scanning…")),
            }
        });
        ui.label(
            egui::RichText::new(crate::i18n::tr("You can go on working: a notice at the top left says when the scan is done.")).color(t.text_dim),
        );
        return;
    };
    if c.offline {
        ui.label(crate::i18n::tr("This folder isn't there: it was moved or renamed, or its disk isn't connected."));
        // (the dialog's button is Locate Missing Folder… then, see `confirm`)
        ui.label(
            egui::RichText::new(crate::i18n::tr(
                "If it is somewhere else now, locate it: its photos then point at the folder you choose. No file is touched.",
            ))
            .color(t.text_dim),
        );
        return;
    }
    // the label is the English key (`Check` translates it); the count sits beside it
    let choice = |ui: &mut egui::Ui, on: &mut bool, n: usize, text: &str, widget: &str| {
        ui.horizontal(|ui| {
            // screen readers hear the count too, as the label used to carry it
            let name = format!("{} ({n})", crate::i18n::tr(text));
            let r = Check::new(&format!("sync.{widget}"), on, text).accessible_name(&name).enabled(n > 0).show(ui);
            // painted, not a Label: a Label adds its own accessibility node and the count would be heard twice
            let galley = ui.painter().layout_no_wrap(format!("({n})"), t.font(13.0), t.text_dim);
            let (rect, _) = ui.allocate_exact_size(galley.size(), egui::Sense::hover());
            ui.painter().galley(rect.min, galley, t.text_dim);
            crate::widgets::register(ui, widget, r.rect);
        });
    };
    let count = |text: &str, n: usize| format!("{} ({n})", crate::i18n::tr(text));
    let dim = |ui: &mut egui::Ui, s: &str| ui.label(egui::RichText::new(s).color(t.text_dim));
    // the files skipped for being in the library elsewhere (`dups` of them), and those that can't be read
    let skipped = |ui: &mut egui::Ui, dups: usize| {
        if dups > 0 {
            dim(ui, &count("Skipped: files already in the library elsewhere", dups));
        }
        if c.unreadable > 0 {
            dim(ui, &count("Skipped: files that can't be read", c.unreadable));
        }
    };
    if c.up_to_date() {
        // nothing to choose: the answer takes the choices' place, and the one button closes
        // (`only_shows`)
        ui.label(crate::i18n::tr("The library is up to date with this folder."));
        skipped(ui, c.duplicates);
        return;
    }
    // (the copies of library photos come in with the new ones when the box under is unticked; a copy
    // of a photo in Recently Deleted never does: that photo is restored from there)
    let incoming = c.new.saturating_add(if *import_duplicates { c.copies } else { 0 });
    choice(ui, import_new, incoming, "Import new photos", "syncImportNew");
    if c.copies > 0 {
        ui.indent("syncSkipDuplicatesIndent", |ui| {
            let mut skip = !*import_duplicates;
            let r = ui.add_enabled(*import_new, egui::Checkbox::new(&mut skip, count("Don't import files already in the library", c.copies)));
            crate::widgets::register(ui, "syncSkipDuplicates", r.rect);
            let r = r.on_hover_text(crate::i18n::tr("A file with the same content as a photo of the library, under another name or in another folder. Untick to bring such files in as photos of their own. Remembered for the next Synchronize."));
            if r.changed() {
                *import_duplicates = !skip;
            }
        });
    }
    if incoming > 0 {
        // what the new photos themselves carry: each kind is read, or left in the files (as in
        // the Import Photos review; widgets `check:syncRead.<kind>`)
        // (indented as a whole, so the kinds that wrap stay under the option they belong to)
        ui.indent("syncRead", |ui| {
            ui.add_enabled_ui(*import_new, |ui| {
                ui.horizontal_wrapped(|ui| {
                    ui.label(egui::RichText::new(crate::i18n::tr("Read")).color(t.text_dim));
                    for (kind, label, on) in crate::import::read_choices(read) {
                        let r = ui.checkbox(on, crate::i18n::tr(label)).on_hover_text(crate::i18n::tr(crate::import::READ_HINT));
                        crate::widgets::register(ui, format!("check:syncRead.{kind}"), r.rect);
                    }
                });
            });
        });
    }
    choice(ui, relink_moved, c.moved, "Relink photos whose file was renamed or moved", "syncRelinkMoved");
    choice(ui, remove_missing, c.missing, "Remove missing photos from the library", "syncRemoveMissing");
    choice(ui, read_metadata, c.metadata, "Read metadata updates from XMP sidecars", "syncReadMetadata");
    ui.add_space(6.0);
    if c.nothing_to_do(true, *import_duplicates) {
        dim(ui, crate::i18n::tr("The library is up to date with this folder."));
    }
    if c.missing > 0 {
        dim(ui, crate::i18n::tr("Missing photos move to Recently Deleted and can be restored; no file on disk is touched."));
    }
    if c.metadata > 0 {
        dim(ui, crate::i18n::tr("Reading a sidecar replaces the photo's rating, labels, metadata and edits with what it says."));
    }
    skipped(ui, if *import_duplicates { c.duplicates.saturating_sub(c.copies) } else { c.duplicates });
}

/// Synchronize with the dialog's choices: the scan the dialog showed is handed to a worker
/// thread at once ([`SyncRun`]); nothing is read from disk on the UI thread. A scan that went
/// stale (the folder's photos changed meanwhile) is refused and made again, keeping the choices.
/// A folder where nothing would be brought in, relinked, removed or read with these choices has
/// nothing to hand over: confirming it only closes the dialog (keeping the choice about files
/// already in the library). While an import or another synchronize runs it waits in line instead ([`SyncRescan`]).
pub fn confirm(app: &mut LightcraftApp, dlg: &Dialog) -> Result<Value, String> {
    if matches!(dlg, Dialog::SynchronizeFolder { counts: None, .. }) {
        // Continue in Background: the scan goes on without the dialog, which the caller closes
        return match &mut app.sync {
            Some(task) => {
                task.background = true;
                if let Dialog::SynchronizeFolder { import_new, import_duplicates, relink_moved, remove_missing, read_metadata, read, .. } = dlg {
                    task.choice = SyncChoice {
                        import_new: *import_new,
                        import_duplicates: *import_duplicates,
                        relink_moved: *relink_moved,
                        remove_missing: *remove_missing,
                        read_metadata: *read_metadata,
                        read: ReadChoice::of(*read),
                    };
                }
                Ok(json!({"background": true}))
            }
            None => Err("the folder is still being scanned".into()),
        };
    }
    let Dialog::SynchronizeFolder {
        path,
        name,
        disk,
        counts: Some(counts),
        import_new,
        import_duplicates,
        relink_moved,
        remove_missing,
        read_metadata,
        read,
        ..
    } = dlg
    else {
        return Err("the folder is still being scanned".into());
    };
    if counts.offline {
        // nothing to synchronize with: ask where the folder is now, then scan it there (#1112)
        return app.run("folder.locate", json!({"folder": path, "disk": disk, "synchronize": true}));
    }
    // (before the line: nothing is imported or written, so a running import or
    // synchronize can't clash with it and it has nothing to wait for; the dialog closes either way. Like every result the
    // dialog shows, it is the scan's: a file added since is found by the next Synchronize)
    if counts.nothing_to_do(*import_new, *import_duplicates) {
        remember_import_duplicates(app, *import_duplicates);
        return Ok(json!({"upToDate": true}));
    }
    remember_import_duplicates(app, *import_duplicates);
    let choice = SyncChoice {
        import_new: *import_new,
        import_duplicates: *import_duplicates,
        relink_moved: *relink_moved,
        remove_missing: *remove_missing,
        read_metadata: *read_metadata,
        read: ReadChoice::of(*read),
    };
    // (an import readies its files against the library as it was: both could add the same file)
    if crate::import::library_busy(app) {
        // what the dialog showed: only these are removed, relinked or read, whenever the run comes
        let shown = app
            .session
            .folder_changes
            .as_ref()
            .filter(|c| lightcraft_catalog::query::folder_key(&c.path) == lightcraft_catalog::query::folder_key(path));
        let shown = shown.map(|c| Shown {
            missing: c.missing.iter().map(|m| m.id).collect(),
            moved: c.moved.iter().map(|m| m.id).collect(),
            metadata: c.metadata.iter().map(|m| m.id).collect(),
        });
        let request = sync_request(path, name, *disk, choice, &shown.unwrap_or_default());
        // (the dialog's scan is let go only once the line has room: refused, the dialog stays open holding it)
        if app.session.activity.line_full() {
            return Err(crate::panels::activity::line_full_text());
        }
        let (id, place) = app.session.activity.enqueue("sync", "Synchronizing folder", request)?;
        if std::mem::take(&mut app.sync_owns_changes) {
            app.session.folder_changes = None;
        }
        if counts.new > 0 && *import_new {
            crate::import::remember_read(app, ReadDialog::Synchronize, *read);
        }
        app.queued_toast(place as u64);
        return Ok(json!({"queued": id, "place": place}));
    }
    app.sync_owns_changes = false;
    let started = match app.session.take_folder_changes(path) {
        Some(changes) => SyncJob::start(&mut app.session, changes, choice).map_err(|e| e.to_string()),
        None => Err(format!("{path} changed since it was scanned: scan again")),
    };
    let (work, commit) = match started {
        Ok(halves) => halves,
        Err(e) => {
            // look again, in the background, keeping what was ticked
            if open(app, path, name, *disk).is_ok()
                && let Some(Dialog::SynchronizeFolder {
                    import_new: i,
                    import_duplicates: u,
                    relink_moved: l,
                    remove_missing: m,
                    read_metadata: x,
                    read: r,
                    ..
                }) = &mut app.ui.dialog
            {
                (*i, *u, *l, *m, *x, *r) = (*import_new, *import_duplicates, *relink_moved, *remove_missing, *read_metadata, *read);
            }
            return Err(e);
        }
    };
    // the Read row opens next time as it was used now (when it was there to be used, and only
    // for a synchronize that was taken: a refused one remembers nothing)
    if counts.new > 0 && *import_new {
        crate::import::remember_read(app, ReadDialog::Synchronize, *read);
    }
    let progress = Arc::new(SyncProgress::default());
    let guard = app.session.activity.start("sync", "Synchronizing folder", Cancel::Flag(progress.files.cancel.clone()));
    guard.detail(name);
    run(app, name, work, commit, progress, guard, 0);
    Ok(json!({"started": true}))
}

/// Hand the work of a synchronize to a worker thread, its steps committed between frames ([`poll_run`]). `left_alone`:
/// photos found missing, moved or with a sidecar update that its dialog never showed, said at the end.
fn run(
    app: &mut LightcraftApp,
    name: &str,
    work: lightcraft_engine::sync::SyncWork,
    commit: SyncCommit,
    progress: Arc<SyncProgress>,
    guard: TaskGuard,
    left_alone: usize,
) {
    let crashed = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let (tx, rx) = std::sync::mpsc::channel();
    let (p, c) = (progress.clone(), crashed.clone());
    let job = move || {
        // (a panic in the work ends it; what was handed over is still committed, and the end
        // says it failed)
        if lightcraft_engine::guard::catch("synchronize", || work.run(&p, |step| tx.send(step).is_ok())).is_err() {
            c.store(true, Ordering::Relaxed);
        }
    };
    #[cfg(not(target_arch = "wasm32"))]
    std::thread::spawn(job);
    #[cfg(target_arch = "wasm32")]
    job();
    app.sync_run = Some(SyncRun {
        name: name.to_string(),
        progress,
        crashed,
        rx,
        commit: Some(commit),
        stopping: false,
        not_saved: Vec::new(),
        guard: Some(guard),
        left_alone,
    });
}

/// The command of a Synchronize Folder's request ([`Request`]): the app runs it ([`start_request`]).
pub const SYNC_REQUEST: &str = "app:synchronize";

/// The photos a Synchronize Folder dialog listed: the only ones its run may act on if it waits in line.
#[derive(Default)]
struct Shown {
    missing: Vec<u64>,
    moved: Vec<u64>,
    metadata: Vec<u64>,
}

/// What a synchronize was asked, to run it later in line or again: the folder, the dialog's choices, and the missing,
/// moved and metadata-updated photos it showed (the only ones it may remove, relink or read).
fn sync_request(path: &str, name: &str, disk: bool, choice: SyncChoice, shown: &Shown) -> Request {
    Request {
        command: SYNC_REQUEST.into(),
        params: json!({
            "path": path, "name": name, "disk": disk,
            "importNew": choice.import_new, "importDuplicates": choice.import_duplicates, "read": read_json(&choice.read), "relinkMoved": choice.relink_moved, "removeMissing": choice.remove_missing, "readMetadata": choice.read_metadata,
            "missing": shown.missing, "moved": shown.moved, "metadata": shown.metadata,
        }),
        subject: name.to_string(),
        facts: String::new(),
    }
}

/// The kinds a choice names, as `ReadChoice::parse` takes them back (the ones left to the library are left out).
fn read_json(read: &ReadChoice) -> Value {
    let kinds = [
        ("keywords", read.keywords),
        ("ratings", read.ratings),
        ("descriptions", read.descriptions),
        ("location", read.location),
        ("develop", read.develop),
    ];
    Value::Object(kinds.into_iter().filter_map(|(k, v)| v.map(|v| (k.to_string(), Value::Bool(v)))).collect())
}

/// A synchronize that waited in line, scanning its folder again on a worker thread before it runs ([`poll_rescan`]).
pub struct SyncRescan {
    name: String,
    choice: SyncChoice,
    /// The missing, moved and metadata-updated photos its dialog showed.
    missing: std::collections::HashSet<u64>,
    moved: std::collections::HashSet<u64>,
    metadata: std::collections::HashSet<u64>,
    /// The library it was asked in ([`lightcraft_engine::Session::library_generation`]): its photo ids mean nothing in
    /// another one.
    library: u64,
    progress: Arc<SyncProgress>,
    rx: std::sync::mpsc::Receiver<FolderChanges>,
    /// Its row, from the line through the scan to the end of the run.
    guard: TaskGuard,
}

/// Start a synchronize that waited in line (`request`, made by [`sync_request`]): it scans the folder again, under the
/// row it had while it waited, and runs when the scan is in ([`poll_rescan`]).
pub fn start_request(app: &mut LightcraftApp, _ctx: &egui::Context, request: &Request) -> Result<(), String> {
    let p = &request.params;
    let path = p["path"].as_str().ok_or("no folder to synchronize")?;
    let name = p["name"].as_str().unwrap_or(path).to_string();
    let flag = |k: &str| p[k].as_bool().unwrap_or(false);
    let ids = |k: &str| p[k].as_array().map(|a| a.iter().filter_map(Value::as_u64).collect()).unwrap_or_default();
    let choice = SyncChoice {
        import_new: flag("importNew"),
        import_duplicates: flag("importDuplicates"),
        relink_moved: flag("relinkMoved"),
        remove_missing: flag("removeMissing"),
        read_metadata: flag("readMetadata"),
        read: ReadChoice::parse(&p["read"]).unwrap_or_default(),
    };
    let input = SyncInput::new(&mut app.session, path, flag("disk")).map_err(|e| e.to_string())?;
    let progress = Arc::new(SyncProgress::default());
    let (tx, rx) = std::sync::mpsc::channel();
    let pr = progress.clone();
    let job = move || {
        let _ = tx.send(scan_with(input, &pr));
    };
    #[cfg(not(target_arch = "wasm32"))]
    std::thread::spawn(job);
    #[cfg(target_arch = "wasm32")]
    job();
    let guard = app.session.activity.start("sync", "Synchronizing folder", Cancel::Flag(progress.files.cancel.clone()));
    guard.detail(&name);
    let library = app.session.library_generation();
    app.sync_queued =
        Some(SyncRescan { name, choice, missing: ids("missing"), moved: ids("moved"), metadata: ids("metadata"), library, progress, rx, guard });
    Ok(())
}

/// Run the waiting synchronize once its scan is in (called every frame): only the missing, moved and metadata-updated
/// photos its dialog showed are acted on.
pub fn poll_rescan(app: &mut LightcraftApp, ctx: &egui::Context) {
    let Some(task) = app.sync_queued.as_ref() else { return };
    let (done, total) = task.progress.counts();
    task.guard.progress(done as u64, total as u64);
    ctx.request_repaint_after(std::time::Duration::from_millis(100));
    let got = task.rx.try_recv();
    // (another library opened: it ends at once, not when a scan of the old library's folder is done)
    let stale = task.library != app.session.library_generation();
    if matches!(got, Err(std::sync::mpsc::TryRecvError::Empty)) && !task.guard.is_cancelled() && !stale {
        return;
    }
    let Some(task) = app.sync_queued.take() else { return };
    let SyncRescan { name, choice, missing, moved, metadata, library, progress, guard, .. } = task;
    if library != app.session.library_generation() {
        // another library was opened while it scanned: its photos are not these
        progress.files.cancel.store(true, Ordering::Relaxed);
        app.toast(ctx, format!("{name}: {}", crate::i18n::tr("Another library was opened: nothing was done")));
        return;
    }
    let mut changes = match got {
        _ if guard.is_cancelled() => {
            progress.files.cancel.store(true, Ordering::Relaxed);
            return;
        }
        Ok(c) => c,
        Err(_) => {
            // the scan's worker died (a panic, logged)
            app.toast(ctx, crate::i18n::tr("Scan failed"));
            return;
        }
    };
    if changes.offline {
        let why = crate::i18n::tr("This folder isn't there: it was moved or renamed, or its disk isn't connected.").to_string();
        app.toast(ctx, format!("{name}: {why}"));
        return;
    }
    // only what the person saw is removed, relinked or read; what they would have been asked about is said
    let (missing_before, moved_before, metadata_before) = (changes.missing.len(), changes.moved.len(), changes.metadata.len());
    changes.missing.retain(|m| missing.contains(&m.id));
    changes.moved.retain(|m| moved.contains(&m.id));
    changes.metadata.retain(|m| metadata.contains(&m.id));
    let left_alone = if choice.remove_missing { missing_before - changes.missing.len() } else { 0 }
        + if choice.relink_moved { moved_before - changes.moved.len() } else { 0 }
        + if choice.read_metadata { metadata_before - changes.metadata.len() } else { 0 };
    match SyncJob::start(&mut app.session, changes, choice) {
        Ok((work, commit)) => {
            // (a fresh count, the same ✕: its row's flag stops the run as it stopped the scan)
            let run_progress = Arc::new(SyncProgress {
                files: lightcraft_engine::import::ScanProgress { cancel: progress.files.cancel.clone(), ..Default::default() },
                ..Default::default()
            });
            run(app, &name, work, commit, run_progress, guard, left_alone);
        }
        Err(e) => {
            app.toast(ctx, format!("{name}: {e}"));
        }
    }
}

/// The library remembers the choice about files already in it for the next Synchronize (and the
/// next time this dialog opens).
fn remember_import_duplicates(app: &mut LightcraftApp, import_duplicates: bool) {
    if import_duplicates != app.session.import_defaults.import_duplicates
        && let Err(e) = app.run("library.preferences", json!({"import": {"importDuplicates": import_duplicates}}))
    {
        log::warn!("synchronize: the choice about files already in the library was not saved: {e}");
    }
}

/// A synchronize at work: the file-system half on a worker thread, its steps committed here
/// between frames ([`poll_run`]).
pub struct SyncRun {
    name: String,
    progress: Arc<SyncProgress>,
    /// The work ended in a panic (logged).
    crashed: Arc<std::sync::atomic::AtomicBool>,
    rx: std::sync::mpsc::Receiver<SyncStep>,
    /// Steps that went into the catalog but could not be saved (a full disk…), and why.
    not_saved: Vec<String>,
    commit: Option<SyncCommit>,
    /// Cancel was pressed: nothing new is started; what was readied is still committed.
    stopping: bool,
    /// Its row in the activity stack (its ✕ is Cancel); taken at the end to say how the run went.
    guard: Option<TaskGuard>,
    /// Photos found missing, moved or with a sidecar update that its dialog never showed (a run that waited in line):
    /// left alone, and said.
    left_alone: usize,
}

impl SyncRun {
    /// (done, of how many) — for a progress row.
    pub fn counts(&self) -> (usize, usize) {
        self.progress.counts()
    }
    /// Stop after the piece in progress.
    pub fn cancel(&mut self) {
        self.stopping = true;
        self.progress.files.cancel.store(true, Ordering::Relaxed);
    }
}

impl Drop for SyncRun {
    /// A dropped run (the app closing) stops its worker at the next piece.
    fn drop(&mut self) {
        self.progress.files.cancel.store(true, Ordering::Relaxed);
    }
}

/// Commit what the worker readied (called every frame); when it is done, one undo step and a
/// word on what happened.
pub fn poll_run(app: &mut LightcraftApp, ctx: &egui::Context) {
    let Some(run) = app.sync_run.as_mut() else { return };
    ctx.request_repaint_after(std::time::Duration::from_millis(100));
    if run.guard.as_ref().is_some_and(TaskGuard::is_cancelled) {
        run.stopping = true;
    }
    let (done, total) = run.counts();
    if let Some(guard) = &run.guard {
        guard.progress(done as u64, total as u64);
    }
    let mut steps = Vec::new();
    let finished = loop {
        match run.rx.try_recv() {
            Ok(step) => steps.push(step),
            Err(std::sync::mpsc::TryRecvError::Empty) => break false,
            Err(std::sync::mpsc::TryRecvError::Disconnected) => break true,
        }
    };
    let Some(mut commit) = run.commit.take() else { return };
    let mut not_saved = Vec::new();
    for step in steps {
        // (a step that went in but couldn't be saved — a full disk — is said at the end)
        if let Err(e) = app.session.execute_fn("folder.synchronize", |s| {
            commit.apply(s, step);
            Ok(Value::Null)
        }) {
            not_saved.push(e.to_string());
        }
    }
    let Some(run) = app.sync_run.as_mut() else { return };
    run.not_saved.extend(not_saved);
    if !finished {
        run.commit = Some(commit);
        return;
    }
    let (name, stopped, crashed, mut not_saved, guard, left_alone) =
        (run.name.clone(), run.stopping, run.crashed.load(Ordering::Relaxed), std::mem::take(&mut run.not_saved), run.guard.take(), run.left_alone);
    app.sync_run = None;
    let mut report = None;
    if let Err(e) = app.session.execute_fn("folder.synchronize", |s| {
        report = Some(commit.finish(s));
        Ok(Value::Null)
    }) {
        not_saved.push(e.to_string());
    }
    let r = report.unwrap_or_default();
    for (path, e) in &r.failed {
        log::warn!("synchronize {name}: {path}: {e}");
    }
    for e in &not_saved {
        log::warn!("synchronize {name}: not saved: {e}");
    }
    let mut parts: Vec<String> =
        [("Imported", r.imported), ("Relinked", r.relinked), ("Removed", r.removed), ("Metadata read", r.read), ("Failed", r.failed.len())]
            .into_iter()
            .filter(|(_, n)| *n > 0)
            .map(|(what, n)| format!("{} {n}", crate::i18n::tr(what)))
            .collect();
    // how it ended comes first: never "up to date" for a run that was stopped or failed
    if crashed {
        parts.insert(0, crate::i18n::tr("Stopped by an error (see the log)").to_string());
    } else if stopped {
        parts.insert(0, crate::i18n::tr("Stopped").to_string());
    }
    if let Some(e) = not_saved.first() {
        parts.push(format!("{}: {e}", crate::i18n::tr("Not saved")));
    }
    if left_alone > 0 {
        parts.push(crate::i18n::tr_format!("Left alone: {n} (synchronize again to review them)", n = left_alone));
    }
    let text = if parts.is_empty() { crate::i18n::tr("The library is up to date with this folder.").to_string() } else { parts.join(" · ") };
    if let Some(guard) = guard {
        let ending = if crashed {
            Ending::Died
        } else if stopped {
            Ending::Stopped
        } else if !r.failed.is_empty() || !not_saved.is_empty() {
            Ending::Problems
        } else {
            Ending::Done
        };
        let failures = r.failed.iter().map(|(path, e)| Failure { name: path.clone(), reason: e.clone() }).collect();
        guard.finish(Outcome { ending, summary: format!("{name}: {text}"), failures, ..Default::default() });
    }
    if not_saved.is_empty() && !crashed && r.failed.is_empty() {
        app.toast(ctx, format!("{name}: {text}"));
    } else {
        app.toast_error_for(ctx, format!("{name}: {text}"), 12.0);
    }
}
