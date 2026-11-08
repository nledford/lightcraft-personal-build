//! Background export: the Export dialog, File → Export with Preset and Export with Previous hand
//! their batch to a worker thread so the window stays responsive. Starting one only registers its
//! row in the activity stack: the photos are prepared over the next frames
//! ([`lightcraft_engine::export::BatchPrep`]: cheap per photo but it needs the session, so it stays on
//! this thread, a few milliseconds a frame), then rendered, encoded and written on the worker
//! (several side by side: [`run_batch_with_copy`]). The row shows the count, the file in progress and ✕
//! (issue #345), which also stops the preparing.
//!
//! Needs [`crate::Services::write_shared`] (a thread-safe writer); without it (web) the batch runs
//! synchronously as before.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, Sender, channel};
use std::sync::{Arc, Mutex};

use lightcraft_catalog::PhotoId;
use lightcraft_engine::activity::{Cancel, Ending, Failure, Outcome, TaskGuard};
use lightcraft_engine::export::{AfterExport, BatchPrep, Destination, DestinationMode, ExportOptions, run_batch_with_copy};
use serde_json::{Value, json};

use crate::LightcraftApp;

/// What one frame spends preparing the photos of an export that has started: a photo takes a few
/// milliseconds on this thread, so a big batch is set up over many frames, not in one.
const PREP_BUDGET: std::time::Duration = std::time::Duration::from_millis(6);

#[cfg(test)]
thread_local! {
    /// A test's own budget (zero: one step per frame, so a test sees every stage of a batch).
    static TEST_PREP_BUDGET: std::cell::Cell<std::time::Duration> = const { std::cell::Cell::new(PREP_BUDGET) };
}

/// Spend `budget` on each frame's preparing in this test's thread.
#[cfg(test)]
pub(crate) fn set_prep_budget(budget: std::time::Duration) {
    TEST_PREP_BUDGET.with(|b| b.set(budget));
}

fn prep_budget() -> std::time::Duration {
    #[cfg(test)]
    return TEST_PREP_BUDGET.with(std::cell::Cell::get);
    #[cfg(not(test))]
    PREP_BUDGET
}

pub struct ExportTask {
    pub total: usize,
    /// Photos started so far and the file in progress.
    pub progress: Arc<Mutex<(usize, String)>>,
    pub cancel: Arc<AtomicBool>,
    /// The files (per-photo results), what [`AfterExport`] did with them (`applePhotos`) and
    /// whether the export was cancelled (as the worker saw it when the batch ended).
    rx: Receiver<Result<(Vec<Value>, Option<Value>, bool), String>>,
    /// The export's row in the activity stack (`activity.cancel` sets `cancel`).
    guard: TaskGuard,
    /// The folder the files go to, for the row the export leaves when it ends.
    folder: Option<String>,
    /// The photos still being set up, a few each frame ([`prepare`]); the worker starts when it is done.
    preparing: Option<Preparing>,
}

/// An image export whose photos are being prepared, and what its worker needs once they are.
struct Preparing {
    /// The session's library when the export started: photo ids mean nothing in another one.
    library: u64,
    batch: BatchPrep,
    opts: ExportOptions,
    to: Destination,
    after: AfterExport,
    write: crate::SharedWrite,
    /// The host's copier of Original videos, when it has one.
    copy: Option<lightcraft_engine::export::SharedOriginalCopier>,
    tx: Sender<Result<(Vec<Value>, Option<Value>, bool), String>>,
    /// The settings Export with Previous is to repeat, kept once the worker has started.
    remember: Option<Value>,
}

impl ExportTask {
    /// The photos are still being set up: nothing is rendered or written yet.
    pub fn is_preparing(&self) -> bool {
        self.preparing.is_some()
    }

    /// `{total, done, current, preparing}` for `ui.inspect` (`preparing`: the photos are still being
    /// set up, nothing is rendered yet).
    pub fn status(&self) -> Value {
        let (done, current) = self.progress.lock().map(|g| g.clone()).unwrap_or_default();
        json!({"total": self.total, "done": done, "current": current, "preparing": self.preparing.is_some()})
    }
}

/// The folder a file is in, for the row of the contact sheet or print that writes it (none for a bare file name).
fn folder_of(path: &str) -> Option<String> {
    std::path::Path::new(path).parent().map(|d| d.to_string_lossy().into_owned()).filter(|d| !d.is_empty())
}

/// The folder the files of an export go to, for the row it leaves: the one an exact file path is in (the engine
/// uses it for a single photo only), none when each photo goes to its own folder (`dir` is not used then), else `dir`.
fn files_folder(to: &Destination, mode: DestinationMode, photos: usize) -> Option<String> {
    match (to.exact.as_ref().filter(|_| photos == 1), mode) {
        (Some(path), _) => folder_of(path),
        (None, DestinationMode::SameAsOriginal) => None,
        (None, _) => Some(to.dir.clone()),
    }
    .filter(|dir| !dir.is_empty())
}

/// Start exporting `ids` in the background, then do `after` with the files written (Add to Apple
/// Photos). The row in the activity stack is there at once; the photos are set up over the frames
/// that follow (see [`prepare`]) and then rendered, encoded and written on a worker. Errors per photo
/// are collected, not fatal; one that can't be set up at all (a photo gone, a demo photo's
/// original…) ends the export with that error, in a toast. `remember` is what Export with Previous is to
/// repeat: kept once the photos are prepared and the worker starts, not for an export that ends before.
pub fn start(
    app: &mut LightcraftApp,
    ids: Vec<PhotoId>,
    opts: ExportOptions,
    to: Destination,
    after: AfterExport,
    remember: Value,
) -> Result<Value, String> {
    if app.export.is_some() {
        return Err("an export is already running".into());
    }
    let write = app.services.write_shared.clone().ok_or("no background writer")?;
    let copy = app.services.original_copy_shared.clone();
    let total = ids.len();
    let progress = Arc::new(Mutex::new((0, String::new())));
    let cancel = Arc::new(AtomicBool::new(false));
    let (tx, rx) = channel();
    let folder = files_folder(&to, opts.destination_mode, total);
    let guard = app.session.activity.start("export", "Exporting", Cancel::Flag(cancel.clone()));
    guard.progress(0, total as u64);
    guard.detail(crate::i18n::tr("Preparing photos…"));
    let library = app.session.library_generation();
    let preparing = Some(Preparing { library, batch: BatchPrep::new(ids), opts, to, after, write, copy, tx, remember: Some(remember) });
    app.export = Some(ExportTask { total, progress, cancel, rx, guard, folder, preparing });
    Ok(json!({"background": true, "total": total}))
}

/// Run the prepared photos (none when `stopped`) on a worker; its result reaches `task.rx`.
fn launch(task: &ExportTask, prep: Preparing, stopped: bool) -> Result<(), String> {
    let Preparing { batch, opts, to, after, write, copy, tx, .. } = prep;
    let items = if stopped { Vec::new() } else { batch.into_items() };
    let (p, c) = (task.progress.clone(), task.cancel.clone());
    let row = task.guard.handle();
    // translated here: the language is the UI thread's, and the worker would show English
    let adding = crate::i18n::tr("Adding to Apple Photos…").to_string();
    let work = move || {
        let mut w = |path: &str, bytes: &[u8]| write(path, bytes);
        let mut copier = |from: &str, to: &str, remove: bool| copy.as_ref().ok_or("no Original copier")?(from, to, remove);
        let can_copy = copy.is_some();
        let r = run_batch_with_copy(
            items,
            &opts,
            &to,
            &mut w,
            &|path| std::path::Path::new(path).exists(),
            false,
            &mut |done, name| {
                if let Ok(mut g) = p.lock() {
                    *g = (done, name.to_string());
                }
                !c.load(Ordering::Relaxed)
            },
            if can_copy { Some(&mut copier) } else { None },
        );
        let r = r.map(|files| {
            if !after.is_none() {
                // Photos can't be stopped once asked: no ✕ from here, before the last look at
                // the cancel flag, so a cancel either stops the export before Photos or is refused
                row.set_cancellable(false);
                if let Ok(mut g) = p.lock() {
                    g.1 = adding;
                }
            }
            let cancelled = c.load(Ordering::Relaxed);
            let photos = after.run(&files, cancelled);
            (files, photos, cancelled)
        });
        let _ = tx.send(r);
    };
    #[cfg(not(target_arch = "wasm32"))]
    std::thread::Builder::new().name("export".into()).spawn(work).map_err(|e| e.to_string())?;
    #[cfg(target_arch = "wasm32")]
    work();
    Ok(())
}

/// One frame's share of preparing an export's photos (the logic pass, so it goes on in a full-screen
/// preview, a slideshow and a hidden window): as many as fit [`prep_budget`] (at least one), then the
/// worker starts when all are ready. A stop (✕), or a library switched meanwhile (the photo ids are the
/// old library's), sends the worker off with no photos, so the export ends as a cancelled one does, and
/// `after` is told. An error ends the export.
pub fn prepare(app: &mut LightcraftApp, ctx: &egui::Context) {
    let Some(task) = app.export.as_mut() else { return };
    let Some(mut prep) = task.preparing.take() else { return };
    if prep.library != app.session.library_generation() {
        task.cancel.store(true, Ordering::Relaxed);
    }
    let stopped = task.cancel.load(Ordering::Relaxed);
    let ready = if stopped {
        Ok(true)
    } else {
        let (began, budget) = (std::time::Instant::now(), prep_budget());
        prep.batch.advance(&mut app.session, &prep.opts, &mut || began.elapsed() < budget)
    };
    match ready {
        Ok(false) => {
            task.preparing = Some(prep);
            ctx.request_repaint();
        }
        Ok(true) => {
            let remember = prep.remember.take();
            match launch(task, prep, stopped) {
                // (an export stopped before it got going, by ✕ or by another library, is not the last one)
                Ok(()) if !stopped => {
                    if let Some(last) = remember {
                        crate::control::remember_export(&mut app.session, last);
                    }
                }
                Ok(()) => {}
                Err(e) => end_unlaunched(app, ctx, e, stopped),
            }
        }
        // (the row ends as a failed one, and `after` is dropped unrun: Photos is free again)
        Err(e) => end_unlaunched(app, ctx, e, false),
    }
}

/// An export that ended before its worker ran (an error preparing a photo, a thread that wouldn't start): its row in the
/// activity stack ends with the error, as a batch that failed does in [`poll`], and the error is said as one.
fn end_unlaunched(app: &mut LightcraftApp, ctx: &egui::Context, error: String, stopped: bool) {
    if let Some(task) = app.export.take() {
        let folder = task.folder.clone();
        task.guard.finish(Outcome { path: folder, ..failed_outcome(error.clone(), stopped) });
    }
    app.toast_error(ctx, error);
}

/// Export a single paginated PDF with the same activity row and cancellation as image exports.
pub fn start_contact_sheet(app: &mut LightcraftApp, params: &Value) -> Result<Value, String> {
    if app.export.is_some() {
        return Err("an export is already running".into());
    }
    let path = params.get("path").and_then(Value::as_str).filter(|s| !s.trim().is_empty()).ok_or("missing path")?.to_string();
    app.session.check_write_target(&path)?;
    let guard = app.session.original_guard();
    let prepared = lightcraft_engine::contact_sheet::prepare(&mut app.session, params)?;
    let total = prepared.len();
    let skipped: Vec<Value> = prepared.skipped().iter().map(|(file, warning)| json!({"file": file, "warning": warning})).collect();
    let write = app.services.write_shared.clone();
    let progress = Arc::new(Mutex::new((0, String::new())));
    let cancel = Arc::new(AtomicBool::new(false));
    let (p, c) = (progress.clone(), cancel.clone());
    let (tx, rx) = channel();
    // (where the PDF goes, for the row the sheet leaves when it ends)
    let folder = folder_of(&path);
    let work = move || {
        let result = prepared
            .run(&mut |done, name| {
                if let Ok(mut g) = p.lock() {
                    *g = (done, name.to_string());
                }
                !c.load(Ordering::Relaxed)
            })
            .and_then(|doc| {
                if c.load(Ordering::Relaxed) {
                    return Err("Contact sheet cancelled".into());
                }
                guard.check(std::path::Path::new(&path))?;
                match write {
                    Some(write) => write(&path, &doc.bytes)?,
                    None => lightcraft_engine::export::write_file(&path, &doc.bytes)?,
                }
                Ok(vec![json!({"path": path, "pages": doc.pages, "photos": doc.photos, "bytes": doc.bytes.len(), "contactSheet": true, "skipped": skipped})])
            });
        // a PDF isn't something Photos imports: contact sheets never add to Apple Photos
        let _ = tx.send(result.map(|files| (files, None, c.load(Ordering::Relaxed))));
    };
    #[cfg(not(target_arch = "wasm32"))]
    std::thread::Builder::new().name("contact-sheet".into()).spawn(work).map_err(|e| e.to_string())?;
    #[cfg(target_arch = "wasm32")]
    work();
    let row = app.session.activity.start("export", "Exporting contact sheet", Cancel::Flag(cancel.clone()));
    row.progress(0, total as u64);
    app.export = Some(ExportTask { total, progress, cancel, rx, guard: row, folder, preparing: None });
    Ok(json!({"background": true, "total": total}))
}

/// Print the session's current print (`print.get`) to a PDF in the background, with the same
/// activity row and cancellation as the other exports.
pub fn start_print(app: &mut LightcraftApp, params: &Value) -> Result<Value, String> {
    use lightcraft_engine::print::{compose, output};
    if app.export.is_some() {
        return Err("an export is already running".into());
    }
    let path = params.get("path").and_then(Value::as_str).filter(|s| !s.trim().is_empty()).ok_or("missing path")?.to_string();
    app.session.check_write_target(&path)?;
    let guard = app.session.original_guard();
    let settings = app.session.print.clone();
    let ids = app.session.targets(params).map_err(|e| e.to_string())?;
    let prepared = compose::prepare(&mut app.session, &settings, &ids)?;
    let total = prepared.len();
    let skipped: Vec<Value> = prepared.skipped().iter().map(|(file, warning)| json!({"file": file, "warning": warning})).collect();
    let title = std::path::Path::new(&path).file_stem().and_then(|n| n.to_str()).unwrap_or("Print").to_string();
    let write = app.services.write_shared.clone();
    let progress = Arc::new(Mutex::new((0, String::new())));
    let cancel = Arc::new(AtomicBool::new(false));
    let (p, c) = (progress.clone(), cancel.clone());
    let (tx, rx) = channel();
    let photos = prepared.photos();
    // (where the PDF went, for the row the print leaves when it ends)
    let folder = folder_of(&path);
    let work = move || {
        let result = prepared
            .run(&mut |done, name| {
                if let Ok(mut g) = p.lock() {
                    *g = (done, name.to_string());
                }
                !c.load(Ordering::Relaxed)
            })
            .and_then(|pages| {
                if c.load(Ordering::Relaxed) {
                    return Err("Print cancelled".into());
                }
                let bytes = output::write(&pages, &settings.job, &title)?;
                guard.check(std::path::Path::new(&path))?;
                match write {
                    Some(write) => write(&path, &bytes)?,
                    None => lightcraft_engine::export::write_file(&path, &bytes)?,
                }
                Ok(vec![json!({"path": path, "pages": pages.len(), "photos": photos, "bytes": bytes.len(), "print": true, "skipped": skipped})])
            });
        let _ = tx.send(result.map(|files| (files, None, c.load(Ordering::Relaxed))));
    };
    #[cfg(not(target_arch = "wasm32"))]
    std::thread::Builder::new().name("print".into()).spawn(work).map_err(|e| e.to_string())?;
    #[cfg(target_arch = "wasm32")]
    work();
    let row = app.session.activity.start("export", "Printing to PDF", Cancel::Flag(cancel.clone()));
    row.progress(0, total as u64);
    app.export = Some(ExportTask { total, progress, cancel, rx, guard: row, folder, preparing: None });
    Ok(json!({"background": true, "total": total}))
}

/// The toast's account of [`AfterExport::run`]'s Apple Photos outcome.
pub fn apple_photos_note(outcome: &Value) -> String {
    if let Some(e) = outcome.get("error").and_then(Value::as_str) {
        return crate::i18n::tr_format!("not added to Apple Photos: {e}", e = e);
    }
    if let Some(why) = outcome.get("skipped").and_then(Value::as_str) {
        if outcome.get("cancelled").and_then(Value::as_bool) == Some(true) {
            return crate::i18n::tr("not added to Apple Photos (cancelled)").to_string();
        }
        return crate::i18n::tr_format!("not added to Apple Photos: {e}", e = why);
    }
    let n = outcome.get("imported").and_then(Value::as_u64).unwrap_or(0);
    let mut m = match outcome.get("album").and_then(Value::as_str) {
        Some(album) => crate::i18n::tr_format!("{n} added to Apple Photos album “{album}”", n = n, album = album),
        None => crate::i18n::tr_format!("{n} added to Apple Photos", n = n),
    };
    if let Some(w) = outcome.get("warning").and_then(Value::as_str) {
        m += &format!(" ({w})");
    }
    m
}

/// Per frame: when an Apple Photos import that no export task reports finishes (`app.export`
/// without `background`, `export.addToPhotos`), say how it went.
#[cfg(not(target_arch = "wasm32"))]
pub fn poll_photos(app: &mut LightcraftApp, ctx: &egui::Context) {
    let jobs = app.session.apple_photos_imports.all();
    if jobs.iter().any(|j| !j.finished()) {
        ctx.request_repaint_after(std::time::Duration::from_millis(250));
    }
    for job in jobs {
        if job.take_announcement() {
            let outcome = job.json();
            let msg = apple_photos_note(&outcome);
            if outcome.get("error").is_some() {
                app.toast_error_for(ctx, msg, 15.0);
            } else {
                app.toast(ctx, msg);
            }
        }
    }
}

/// No Apple Photos on the web.
#[cfg(target_arch = "wasm32")]
pub fn poll_photos(_: &mut LightcraftApp, _: &egui::Context) {}

/// The first part of an export's account: what was done (photos exported, a contact sheet or a PDF printed).
fn export_headline(files: &[Value], ok: usize, total: usize) -> String {
    if let Some(sheet) = files.first().filter(|f| f["contactSheet"] == true) {
        crate::i18n::tr_format!("Exported contact sheet: {photos} photos, {pages} pages", photos = sheet["photos"], pages = sheet["pages"])
    } else if let Some(pdf) = files.first().filter(|f| f["print"] == true) {
        crate::i18n::tr_format!("Printed to PDF: {photos} photos, {pages} pages", photos = pdf["photos"], pages = pdf["pages"])
    } else {
        crate::i18n::tr_format!("Exported {ok} of {total} photo{}", if total == 1 { "" } else { "s" }, ok = ok, total = total)
    }
}

/// What a finished export says: how many files were written, skipped or failed, and the Apple Photos note; and
/// its first part, what was done, which is the headline of the row the export leaves (translated like the toast).
fn export_summary(files: &[Value], photos: Option<&Value>, total: usize, cancelled: bool) -> (String, String) {
    let failed: Vec<&Value> = files.iter().filter(|f| f.get("error").is_some()).collect();
    let ok = files.iter().filter(|f| f.get("path").is_some()).count();
    let (skipped, warned) = not_written(files);
    let mut m = export_headline(files, ok, total);
    let summary = m.clone();
    if skipped > 0 {
        m += &crate::i18n::tr_format!(" · {skipped} skipped (file exists)", skipped = skipped);
    }
    if warned > 0 {
        m += &format!(" · {warned} {}", crate::i18n::tr("not exported"));
    }
    if let Some(first) = failed.first() {
        m += &crate::i18n::tr_format!(" · {} failed: {}", failed.len(), first["error"].as_str().unwrap_or("error"));
    }
    if cancelled {
        m += " · ";
        m += crate::i18n::tr("cancelled");
    }
    if let Some(photos) = photos {
        m += " · ";
        m += &apple_photos_note(photos);
    }
    (m, summary)
}

/// How many of an export's files were skipped (a name that exists keeps the taken path as `skipped`) and how many were
/// not written with a warning (a clip a rendered export cannot make, a contact sheet's or print's skipped photos, which
/// come as an array of {file, warning}).
fn not_written(files: &[Value]) -> (usize, usize) {
    let skipped = files.iter().filter(|f| f.get("skipped").is_some_and(|v| !v.is_array())).count();
    let warned = files.iter().filter(|f| f.get("warning").is_some()).count()
        + files.iter().filter_map(|f| f.get("skipped").and_then(Value::as_array)).map(Vec::len).sum::<usize>();
    (skipped, warned)
}

/// How an export that ran ended, for the activity stack. Photos taking only some of the files is a warning, and
/// Photos not asked at all (`skipped`: the export wrote no files, an earlier request still running…) is not an error
/// either: not all went well in both cases. Neither did an export that could not write some files without failing
/// (`unwritten`: a rendered export of clips only ends with no file and no failure). A name that exists is not a
/// problem: the person chose to skip it (the Skip collision policy).
fn export_ending(cancelled: bool, ok: usize, failed: usize, unwritten: usize, photos: Option<&Value>) -> Ending {
    let photos_short = photos.is_some_and(|p| ["error", "warning", "skipped"].iter().any(|k| p.get(k).is_some()));
    if cancelled {
        Ending::Stopped
    } else if failed > 0 && ok == 0 {
        Ending::Failed
    } else if failed > 0 || unwritten > 0 || photos_short {
        Ending::Problems
    } else {
        Ending::Done
    }
}

/// What a finished export tells the activity stack: how it ended, its summary, the rest of its sentence as the detail
/// (what needs reading), the files it could not write, and where the others are (one of the files written, maybe in
/// a subfolder of the folder asked for, else the folder).
fn finished_outcome(files: &[Value], photos: Option<&Value>, cancelled: bool, folder: Option<String>, msg: &str, summary: String) -> Outcome {
    let failed: Vec<&Value> = files.iter().filter(|f| f.get("error").is_some()).collect();
    let ok = files.iter().filter(|f| f.get("path").is_some()).count();
    let (_, warned) = not_written(files);
    let ending = export_ending(cancelled, ok, failed.len(), warned, photos);
    let path = files.iter().find_map(|f| f["path"].as_str()).map(str::to_string).or(folder);
    let failures = failed
        .iter()
        .map(|f| Failure { name: f["file"].as_str().unwrap_or_default().to_string(), reason: f["error"].as_str().unwrap_or("error").to_string() })
        .collect();
    let detail = msg.get(summary.len()..).unwrap_or_default().trim_start_matches(" · ").to_string();
    Outcome { ending, summary, detail, failures, path, ..Default::default() }
}

/// An export that came back as an error. The error is the row's detail: its headline stays empty, so the row reads
/// "Exporting: failed" (or "Printing to PDF: failed") under the task's own name.
fn failed_outcome(error: String, stopped: bool) -> Outcome {
    Outcome { ending: if stopped { Ending::Stopped } else { Ending::Failed }, detail: error, ..Default::default() }
}

/// Per frame: keep the activity row up to date; when the batch finishes, report it: in a toast, as ever, and to
/// the activity stack, where an export that ran long or did not go well stays in `activity.finished` with its
/// failed files and where its results are.
pub fn poll(app: &mut LightcraftApp, ctx: &egui::Context) {
    let Some(task) = &app.export else { return };
    if task.preparing.is_some() {
        return;
    }
    let r = match task.rx.try_recv() {
        Ok(r) => Some(r),
        Err(std::sync::mpsc::TryRecvError::Empty) => {
            let (done, current) = task.progress.lock().map(|g| g.clone()).unwrap_or_default();
            task.guard.progress(done as u64, task.total as u64);
            task.guard.detail(&current);
            return;
        }
        Err(std::sync::mpsc::TryRecvError::Disconnected) => None,
    };
    let Some(task) = app.export.take() else { return };
    let total = task.total;
    let folder = task.folder.clone();
    // (a contact sheet that was stopped comes back as an error: it was stopped all the same)
    let stopped = task.cancel.load(Ordering::Relaxed);
    let Some(r) = r else {
        let msg = crate::i18n::tr("Export stopped unexpectedly");
        task.guard.finish(Outcome { ending: Ending::Died, summary: msg.to_string(), path: folder, ..Default::default() });
        app.toast_error(ctx, msg);
        return;
    };
    let cancelled = matches!(&r, Ok((_, _, true)));
    // Photos refused (permission, not installed…): leave the way out on screen longer
    let photos_failed = matches!(&r, Ok((_, Some(p), _)) if p.get("error").is_some());
    let errored = r.is_err();
    let (msg, outcome) = match r {
        Ok((files, photos, _)) => {
            let (m, summary) = export_summary(&files, photos.as_ref(), total, cancelled);
            let outcome = finished_outcome(&files, photos.as_ref(), cancelled, folder, &m, summary);
            app.last_export_result = Some(json!({"files": files, "cancelled": cancelled, "applePhotos": photos}));
            (m, outcome)
        }
        // (the folder stays on the row, as for a worker that died: something may have been written there)
        Err(e) => (e.clone(), Outcome { path: folder, ..failed_outcome(e, stopped) }),
    };
    // an error, or a file that failed, is said as one
    let had_failures = errored || !outcome.failures.is_empty();
    task.guard.finish(outcome);
    if photos_failed {
        app.toast_error_for(ctx, msg, 15.0);
    } else if had_failures {
        app.toast_error(ctx, msg);
    } else {
        app.toast(ctx, msg);
    }
}

#[cfg(test)]
mod contact_sheet_tests {
    use super::*;
    use crate::pick::{PickKind, PickRequest};
    use crate::state::Dialog;
    use std::time::{Duration, Instant};

    #[test]
    fn contact_sheet_menu_dialog_picker_and_background_export() {
        let asked = Arc::new(Mutex::new(Vec::<PickRequest>::new()));
        let requests = asked.clone();
        let bytes = Arc::new(Mutex::new(Vec::new()));
        let written = bytes.clone();
        let services = crate::Services {
            picker: Some(Box::new(move |req| {
                requests.lock().unwrap().push(req);
                let (tx, rx) = channel();
                tx.send(vec!["Contact Sheet.pdf".into()]).unwrap();
                Ok(rx)
            })),
            write_shared: Some(Arc::new(move |path, data| {
                assert_eq!(path, "Contact Sheet.pdf");
                *written.lock().unwrap() = data.to_vec();
                Ok(())
            })),
            ..Default::default()
        };
        // Exercise the menu/picker/background PDF workflow with a small procedural original.
        // A full-size demo makes this test's deadline depend on unrelated concurrent renders.
        let demo = lightcraft_engine::Session::with_demo();
        let id = demo.active().unwrap();
        let mut photo = (**demo.catalog.photo(id).unwrap()).clone();
        photo.width = 96;
        photo.height = 64;
        let mut session = lightcraft_engine::Session::new();
        session.catalog.apply(lightcraft_catalog::Op::AddPhoto { photo: Box::new(photo) }).unwrap();
        session.execute("library.select", &json!({"ids": [id.0], "active": id.0})).unwrap();
        let mut app = LightcraftApp::new(session, services);
        app.run("dialog.contactSheet", json!({})).unwrap();
        let Some(Dialog::ContactSheet { options }) = app.ui.dialog.as_mut() else { panic!("no contact sheet settings") };
        options.paper = "letter".into();
        options.landscape = true;
        let dialog = app.ui.dialog.take().unwrap();
        crate::panels::dialogs::confirm_dialog(&mut app, &dialog).unwrap();
        assert_eq!(app.pending_picks.len(), 1);
        let ctx = egui::Context::default();
        crate::pick::poll(&mut app, &ctx);
        assert!(app.pending_picks.is_empty());
        assert!(app.export.is_some(), "picker did not start export: {:?}", app.ui.toast);
        // a hang guard, not a speed check: the render queues behind other tests' demo scenes (generated one at a
        // time process-wide), which on a loaded machine takes minutes
        let deadline = Instant::now() + Duration::from_secs(300);
        loop {
            // Poll the real worker without starting unrelated gallery renders each frame.
            poll(&mut app, &ctx);
            if app.export.is_none() {
                break;
            }
            assert!(Instant::now() < deadline, "export did not finish");
            std::thread::sleep(Duration::from_millis(10));
        }
        let result = app.last_export_result.as_ref().unwrap_or_else(|| panic!("export failed: {:?}", app.ui.toast));
        let req = asked.lock().unwrap();
        assert_eq!(req[0].kind, PickKind::Save);
        assert_eq!(req[0].filter.as_ref().unwrap().1, &["pdf"]);
        assert_eq!(req[0].file_name.as_deref(), Some("Contact Sheet.pdf"));
        let b = bytes.lock().unwrap();
        assert!(b.starts_with(b"%PDF-1.4"));
        assert!(String::from_utf8_lossy(&b).contains("/MediaBox [0 0 792.00 612.00]"));
        assert_eq!(result["files"][0]["photos"], 1);
    }

    #[test]
    fn contact_sheet_export_shows_a_row_and_its_cross_stops_it() {
        let mut app = LightcraftApp::new(
            lightcraft_engine::Session::with_demo(),
            crate::Services { write_shared: Some(Arc::new(|_, _| Ok(()))), ..Default::default() },
        );
        let ids: Vec<_> = app.session.catalog.photos().take(24).map(|p| p.id.0).collect();
        start_contact_sheet(&mut app, &json!({"path": "Sheet.pdf", "ids": ids})).unwrap();
        let rows = app.session.activity.list();
        assert_eq!(rows.len(), 1, "{rows:?}");
        assert_eq!((rows[0].kind, rows[0].label.as_str(), rows[0].total), ("export", "Exporting contact sheet", 24));
        assert!(rows[0].cancellable);
        app.session.activity.cancel(rows[0].id).unwrap();
        assert!(app.export.as_ref().unwrap().cancel.load(Ordering::Relaxed), "the stack's ✕ stops the sheet");
        let task = app.export.take().unwrap();
        assert!(task.rx.recv_timeout(Duration::from_secs(20)).unwrap().is_err(), "a stopped sheet is not written");
        drop(task);
        assert!(app.session.activity.list().is_empty(), "the row goes with the export");
    }

    #[test]
    fn cancelled_contact_sheet_never_calls_writer() {
        let mut app = LightcraftApp::new(
            lightcraft_engine::Session::with_demo(),
            crate::Services { write_shared: Some(Arc::new(|_, _| panic!("cancelled export wrote a file"))), ..Default::default() },
        );
        let ids: Vec<_> = app.session.catalog.photos().take(24).map(|p| p.id.0).collect();
        start_contact_sheet(&mut app, &json!({"path": "Cancelled.pdf", "ids": ids})).unwrap();
        app.export.as_ref().unwrap().cancel.store(true, Ordering::Relaxed);
        let task = app.export.take().unwrap();
        assert!(task.rx.recv_timeout(Duration::from_secs(20)).unwrap().is_err());
    }

    /// A contact sheet stopped by its ✕ comes back as an error, but it is a stopped task, not a failed one.
    #[test]
    fn a_stopped_contact_sheet_leaves_a_stopped_row_that_knows_its_folder() {
        let mut app = LightcraftApp::new(
            lightcraft_engine::Session::with_demo(),
            crate::Services { write_shared: Some(Arc::new(|_, _| Ok(()))), ..Default::default() },
        );
        let ids: Vec<_> = app.session.catalog.photos().take(24).map(|p| p.id.0).collect();
        let path = std::env::temp_dir().join("lc-stopped-sheet.pdf");
        start_contact_sheet(&mut app, &json!({"path": path.to_string_lossy(), "ids": ids})).unwrap();
        let id = app.session.activity.list()[0].id;
        app.session.activity.cancel(id).unwrap();
        let ctx = egui::Context::default();
        let deadline = Instant::now() + Duration::from_secs(60);
        while app.export.is_some() {
            poll(&mut app, &ctx);
            assert!(Instant::now() < deadline, "the stopped sheet did not end");
            std::thread::sleep(Duration::from_millis(10));
        }
        let row = app.session.activity.finished().into_iter().next().expect("a stopped task always leaves a row");
        assert_eq!((row.kind, row.ending), ("export", Ending::Stopped), "{row:?}");
        // (the row keeps the folder the sheet was to go to: something may have been written there)
        assert_eq!(row.path.as_deref(), path.parent().and_then(|d| d.to_str()), "{row:?}");
    }

    /// A worker that is gone without a word leaves a row saying the export stopped unexpectedly.
    #[test]
    fn an_export_whose_worker_is_gone_leaves_a_died_row() {
        let mut app = LightcraftApp::new(lightcraft_engine::Session::with_demo(), crate::Services::default());
        let cancel = Arc::new(AtomicBool::new(false));
        let guard = app.session.activity.start("export", "Exporting", Cancel::Flag(cancel.clone()));
        let (tx, rx) = channel();
        drop(tx);
        app.export = Some(ExportTask {
            total: 3,
            progress: Arc::new(Mutex::new((0, String::new()))),
            cancel,
            rx,
            guard,
            folder: Some("/tmp/out".into()),
            preparing: None,
        });
        poll(&mut app, &egui::Context::default());
        assert!(app.export.is_none());
        let row = app.session.activity.finished().into_iter().next().expect("a worker that died always leaves a row");
        assert_eq!((row.kind, row.ending, row.path.as_deref()), ("export", Ending::Died, Some("/tmp/out")), "{row:?}");
        assert_eq!(row.summary, "Export stopped unexpectedly");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The toast's Apple Photos part (issue #236): the count and album, a partial import's
    /// warning, an error with what to do, a cancelled export.
    #[test]
    fn apple_photos_outcomes_read_well() {
        assert_eq!(apple_photos_note(&json!({"imported": 3, "requested": 3, "ids": [], "album": null})), "3 added to Apple Photos");
        assert_eq!(apple_photos_note(&json!({"imported": 1, "album": "Trip"})), "1 added to Apple Photos album “Trip”");
        let partial = apple_photos_note(&json!({"imported": 1, "requested": 2, "warning": "Photos added 1 of 2 files"}));
        assert_eq!(partial, "1 added to Apple Photos (Photos added 1 of 2 files)");
        let denied = apple_photos_note(
            &json!({"error": "LightCraft isn't allowed to control Photos. Allow it in System Settings › Privacy & Security › Automation"}),
        );
        assert!(denied.starts_with("not added to Apple Photos: ") && denied.contains("Automation"), "{denied}");
        assert_eq!(apple_photos_note(&json!({"skipped": "the export was cancelled", "cancelled": true})), "not added to Apple Photos (cancelled)");
        assert_eq!(apple_photos_note(&json!({"skipped": "the export wrote no files"})), "not added to Apple Photos: the export wrote no files");
        // decided on the `cancelled` flag, not the words (#1046): reworded it is still a cancel, and a
        // reason that only mentions the word is shown as it is
        assert_eq!(apple_photos_note(&json!({"skipped": "Export abgebrochen", "cancelled": true})), "not added to Apple Photos (cancelled)");
        assert_eq!(
            apple_photos_note(&json!({"skipped": "an earlier, cancelled request is still running"})),
            "not added to Apple Photos: an earlier, cancelled request is still running"
        );
    }

    /// An Apple Photos step that did not add the files (error, warning, or skipped without a cancel) is a problem.
    #[test]
    fn export_ending_counts_an_uncancelled_photos_skip_as_a_problem() {
        let skipped = json!({"skipped": "the export wrote no files"});
        assert_eq!(export_ending(false, 3, 0, 0, Some(&skipped)), Ending::Problems);
        assert_eq!(export_ending(false, 3, 0, 0, Some(&json!({"warning": "added 1 of 3"}))), Ending::Problems);
        assert_eq!(export_ending(false, 3, 0, 0, Some(&json!({"imported": 3}))), Ending::Done);
        assert_eq!(export_ending(false, 3, 0, 0, None), Ending::Done);
        assert_eq!(export_ending(false, 0, 2, 0, None), Ending::Failed);
        assert_eq!(export_ending(true, 3, 0, 0, Some(&json!({"skipped": "cancelled", "cancelled": true}))), Ending::Stopped);
    }

    /// A rendered export of clips only writes nothing: each clip comes back as a warning, so it did not go well,
    /// however few files failed (review of #2822). A name that exists is skipped by the person's choice: no problem.
    #[test]
    fn an_export_that_warned_files_ends_with_problems() {
        let ending = |files: Vec<Value>| finished_outcome(&files, None, false, None, "", String::new()).ending;
        let clip = json!({"photo": 1, "file": "a.mp4", "warning": "a video is not rendered"});
        assert_eq!(ending(vec![clip.clone(), clip.clone()]), Ending::Problems);
        assert_eq!(ending(vec![json!({"path": "/out/a.jpg"}), clip]), Ending::Problems);
        assert_eq!(ending(vec![json!({"path": "/out/a.jpg"}), json!({"skipped": "/out/b.jpg"})]), Ending::Done);
        // (a contact sheet's skipped photos are an array of warnings)
        assert_eq!(ending(vec![json!({"path": "/out/s.pdf", "skipped": [{"file": "a.mp4", "warning": "no frame"}]})]), Ending::Problems);
        assert_eq!(ending(vec![json!({"path": "/out/a.jpg"})]), Ending::Done);
    }

    /// The row of an export points at the folder its files go to, not at one it does not use.
    #[test]
    fn an_exports_row_names_the_folder_the_files_go_to() {
        let to = |dir: &str, exact: Option<&str>| Destination { dir: dir.into(), exact: exact.map(str::to_string) };
        assert_eq!(files_folder(&to("/out", None), DestinationMode::SpecificFolder, 3).as_deref(), Some("/out"));
        assert_eq!(files_folder(&to("/out", Some("/pics/one.jpg")), DestinationMode::SpecificFolder, 1).as_deref(), Some("/pics"));
        // (an exact file is for a single photo: with more, the files go to the folder)
        assert_eq!(files_folder(&to("/out", Some("/pics/one.jpg")), DestinationMode::SpecificFolder, 2).as_deref(), Some("/out"));
        assert_eq!(files_folder(&to("/out", None), DestinationMode::SameAsOriginal, 3), None);
        assert_eq!(files_folder(&to("", None), DestinationMode::SpecificFolder, 1), None);
    }

    /// A failed export keeps its name in the row's headline; the error goes under it.
    #[test]
    fn a_failed_export_says_why_under_its_name() {
        let o = failed_outcome("The disk is full".into(), false);
        assert_eq!((o.ending, o.summary.as_str(), o.detail.as_str()), (Ending::Failed, "", "The disk is full"));
        assert_eq!(failed_outcome("x".into(), true).ending, Ending::Stopped);
    }
}
