use super::task::{ImportTask, not_readable_text};
use super::{
    ImportCandidate, ImportDialog, Lane, LightcraftApp, Ordering, Request, ScanInput, ScanOutput, ScanProgress, Transfer, Value, import_queue, json,
    scan_with,
};

/// A folder scan running on a worker thread (a network share can take minutes to read; the
/// window must keep answering meanwhile).
pub struct ScanTask {
    progress: std::sync::Arc<ScanProgress>,
    rx: std::sync::mpsc::Receiver<ScanEnd>,
    /// Open the review with "copy into the library" checked (a camera / card).
    pub copy: bool,
    /// Open the review copying into this folder, all into it with no subfolders (a folder's menu
    /// ▸ Import to This Folder…); implies `copy`.
    pub destination: Option<String>,
    /// Open the review with this album chosen (an album's context menu ▸ Import…).
    pub album: Option<u64>,
    /// Browsing a folder (Local): the photos are read in place instead of opening the review.
    pub(crate) browse: bool,
    /// What is being scanned (the review's source).
    sources: Vec<String>,
    /// The library it was started in ([`lightcraft_engine::Session::library_generation`]): an album id means nothing in
    /// another one.
    pub(crate) library: u64,
    /// The scan's row in the activity stack (its ✕ and `activity.cancel` set `progress.cancel`).
    guard: lightcraft_engine::activity::TaskGuard,
    /// Browsing: the folder as the worker resolved it, once it has. The view shows it as given until then
    /// (resolving reads the disk, which a stalled network mount can block: never on this thread).
    resolved: Option<std::sync::mpsc::Receiver<String>>,
}

/// What a scan's worker sends back. Every way it ends sends one, so the probes of earlier scans that
/// [`ScanInput::new`] took from the session always go back to it (a later import doesn't read those files again).
pub(crate) enum ScanEnd {
    /// The scan finished.
    Done(ScanOutput),
    /// Browsing stopped without a listing: the folder couldn't be listed (`failed`), or another folder was
    /// chosen. Never an empty listing, which the browse bookkeeping would take for every file gone.
    Stopped { probes: std::collections::HashMap<String, lightcraft_engine::media::ProbeInfo>, failed: bool },
}

impl ScanEnd {
    /// The probes this end hands back to the session.
    fn into_probes(self) -> std::collections::HashMap<String, lightcraft_engine::media::ProbeInfo> {
        match self {
            ScanEnd::Done(out) => out.probes,
            ScanEnd::Stopped { probes, .. } => probes,
        }
    }
}

/// Scans stopped without waiting for their workers ([`stop_scan`]), in the order they were stopped, which is
/// the order they started (one scan runs at a time), and which of their probes went back to the session.
#[derive(Default)]
pub(crate) struct StoppingScans {
    /// How many scans were stopped: each one's number.
    stopped: u64,
    /// Each stopped scan's number and channel, kept until its worker ends.
    pending: Vec<(u64, std::sync::mpsc::Receiver<ScanEnd>)>,
    /// The probes a stopped scan put back: by whom, and what they said (to tell they are still the ones there).
    from: std::collections::HashMap<String, (u64, ProbeMark)>,
}

/// What tells two probes of one file apart: a file that changed but kept its size has another content hash.
type ProbeMark = (u64, Option<String>, Option<String>);

fn mark(p: &lightcraft_engine::media::ProbeInfo) -> ProbeMark {
    (p.file_size, p.content_hash.clone(), p.file_modified.clone())
}

/// Stop the running scan (if any) without waiting for its worker: the scan is closed at once, and its
/// channel is kept until the worker ends, so the probes it holds still come back ([`collect_stopped`]).
fn stop_scan(app: &mut LightcraftApp) {
    if let Some(t) = app.scan.take() {
        t.progress.cancel.store(true, Ordering::Relaxed);
        let s = &mut app.scans_stopping;
        s.stopped = s.stopped.saturating_add(1);
        s.pending.push((s.stopped, t.rx));
    }
}

/// The probes of stopped scans whose workers have ended go back to the session (called every frame). A probe
/// the session got since a scan started is newer than that scan's and stays, except one that a scan stopped
/// before it put back: a later scan read that file again, so its probe wins, whichever worker ends first
/// (an older probe of a file changed at the same size would hand the copy check a stale content hash).
fn collect_stopped(app: &mut LightcraftApp, ctx: &egui::Context) {
    let probes = &mut app.session.import_probes;
    let StoppingScans { pending, from, .. } = &mut app.scans_stopping;
    pending.retain(|(seq, rx)| match rx.try_recv() {
        Ok(end) => {
            for (k, v) in end.into_probes() {
                let replace = match probes.get(&k) {
                    None => true,
                    Some(have) => from.get(&k).is_some_and(|(by, m)| by < seq && *m == mark(have)),
                };
                if replace {
                    from.insert(k.clone(), (*seq, mark(&v)));
                    probes.insert(k, v);
                }
            }
            false
        }
        Err(std::sync::mpsc::TryRecvError::Empty) => true,
        Err(std::sync::mpsc::TryRecvError::Disconnected) => false,
    });
    if pending.is_empty() {
        // no stopped scan left to compare with
        from.clear();
    } else {
        ctx.request_repaint_after(std::time::Duration::from_millis(100));
    }
}

/// The activity row of a scan: the review's ("Scanning folder") or the Local view's ("Reading folder").
fn scan_guard(app: &LightcraftApp, progress: &ScanProgress, browse: bool) -> lightcraft_engine::activity::TaskGuard {
    let label = if browse { "Reading folder" } else { "Scanning folder" };
    app.session.activity.start("scan", label, lightcraft_engine::activity::Cancel::Flag(progress.cancel.clone()))
}

/// The command of an import's request ([`Request`]): the app runs it ([`start_request`]), with its files and its
/// `library.import` params. The engine reads the files of an Auto Import's request back when it leaves the line
/// unrun ([`lightcraft_engine::activity::Activity::take_untried`]): keep `paths` and `auto` where they are.
pub const IMPORT_REQUEST: &str = lightcraft_engine::activity::IMPORT_REQUEST;

/// `below`: the folders or files the import's unreadable files are named by where they are below
/// ([`ImportTask::named_below`]); `sources`: the folders or files the person chose, which name the import in its row;
/// `counted`: `paths` are the photos the review counted (dropped folders are not counted until they are read).
pub(super) fn import_request(paths: &[String], params: &Value, below: &[String], auto: bool, sources: &[String], counted: bool) -> Request {
    Request {
        command: IMPORT_REQUEST.into(),
        params: json!({"paths": paths, "params": params, "below": below, "auto": auto}),
        subject: subject_of(sources),
        facts: facts_of(counted.then_some(paths.len()), params["mode"].as_str().unwrap_or("add")),
    }
}

/// What an import is about, for its rows: the source folder's (or file's) name, or how many sources there are. Empty
/// when it is not known where the photos come from.
pub(super) fn subject_of(sources: &[String]) -> String {
    match sources {
        [] => String::new(),
        [one] => {
            std::path::Path::new(one.trim_end_matches(['/', '\\'])).file_name().map_or_else(|| one.clone(), |n| n.to_string_lossy().into_owned())
        }
        many => crate::i18n::tr_format!("{} item{}", many.len(), "s"),
    }
}

/// What else an import waiting in line says: how many photos (when the review counted them) and how they come in.
fn facts_of(photos: Option<usize>, mode: &str) -> String {
    let how = match mode {
        "copy" => crate::i18n::tr("Copy"),
        "move" => crate::i18n::tr("Move"),
        _ => crate::i18n::tr("Add in place"),
    };
    match photos {
        Some(n) => format!("{} · {how}", crate::i18n::tr_format!("{n} photo{}", if n == 1 { "" } else { "s" }, n = n)),
        None => how.to_string(),
    }
}

/// The library lane is taken: an import (or the reading of a Local folder) or a Synchronize Folder runs, or waits
/// to. Each readies its files against the library as it is when it starts, so two side by side could add the same
/// file: one asked for now waits in line ([`Lane::Library`]).
pub(crate) fn library_busy(app: &LightcraftApp) -> bool {
    app.import.is_some()
        || app.scan.as_ref().is_some_and(|t| t.browse)
        || app.sync_run.is_some()
        || app.sync_queued.is_some()
        || app.session.activity.lane_busy(Lane::Library)
        || app.session.activity.lane_waiting(Lane::Library)
}

/// What a task asked for now would start after, as its dialog says it ("Importing · Rome"): what runs in the library
/// lane, else the last task waiting there. `None` when the lane is free.
pub(crate) fn lane_ahead_text(app: &LightcraftApp) -> Option<String> {
    if !library_busy(app) {
        return None;
    }
    let rows = app.session.activity.list();
    let in_lane = |t: &&lightcraft_engine::activity::TaskInfo| t.lane == Some(Lane::Library);
    let ahead = rows.iter().filter(in_lane).find(|t| !t.waiting).or_else(|| rows.iter().rev().find(in_lane));
    // (a Local folder being read holds the lane with a row of its own kind, which has no lane)
    let reading = || app.scan.as_ref().is_some_and(|t| t.browse).then(|| crate::i18n::tr("Reading folder").to_string());
    Some(ahead.map(crate::panels::activity::waiting_name).or_else(reading).unwrap_or_else(|| crate::i18n::tr("Importing").to_string()))
}

/// Put an import of `paths` with `params` in line behind what runs in the library lane, and say so (an Auto Import
/// quietly: nobody asked for it). `request` is made by [`import_request`].
pub(super) fn wait_in_line(app: &mut LightcraftApp, request: Request) -> Result<Value, String> {
    if app.session.activity.line_full() {
        return Err(crate::panels::activity::line_full_text());
    }
    let auto = request.params["auto"].as_bool().unwrap_or(false);
    let (id, place) = app.session.activity.enqueue("import", "Importing", request)?;
    if !auto {
        app.queued_toast(place as u64);
    }
    Ok(json!({"queued": id, "place": place}))
}

/// The watched folder was listed (Auto Import): import what is new in it, as an Auto Import. The engine has noted
/// those files as tried by now, so while the library lane is taken (it was free when the listing began) they wait in
/// line instead of being dropped.
#[cfg(not(target_arch = "wasm32"))]
pub(crate) fn auto_import_listed(app: &mut LightcraftApp, listing: Vec<(String, u64)>, watched: &str) {
    // no room in the line: leave the folder to its next listing, because the scan below notes its files as tried
    if library_busy(app) && app.session.activity.waiting().len() >= lightcraft_engine::activity::MAX_WAITING {
        return;
    }
    let p = json!({"listing": listing, "start": false});
    let Ok(r) = app.session.execute("library.autoImportScan", &p) else { return };
    let Some(mut params) = r.get("import").cloned() else { return };
    let paths: Vec<String> = params["paths"].as_array().into_iter().flatten().filter_map(|v| v.as_str().map(str::to_string)).collect();
    if let Some(o) = params.as_object_mut() {
        o.remove("paths");
    }
    if library_busy(app) {
        if let Err(e) = wait_in_line(app, import_request(&paths, &params, &[watched.to_string()], true, &[], true)) {
            log::warn!("auto import: {e}");
        }
        return;
    }
    let undo0 = app.session.undo.len();
    app.import = Some(ImportTask::new(paths, params, undo0, false).named_below(vec![watched.to_string()]).auto());
}

/// Settings → Import → "Show the import review when a camera or card is connected", called with
/// each device scan while it's on. The first look only notes the cards already in. A card that
/// shows up after that opens Import from Device once nothing else is open, and a card that is
/// taken out and put back in asks again.
#[cfg(not(target_arch = "wasm32"))]
pub(crate) fn cards_seen(app: &mut LightcraftApp, now: Vec<lightcraft_engine::devices::Device>) {
    let Some(known) = app.ui.known_cards.as_mut() else {
        app.ui.known_cards = Some(now.into_iter().map(|d| d.root).collect());
        return;
    };
    known.retain(|root| now.iter().any(|d| d.root == *root));
    let Some(card) = now.into_iter().find(|d| !known.contains(&d.root)) else { return };
    // busy: ask when the window is free (the card isn't noted until then)
    if app.import.is_some() || app.scan.is_some() || app.sync_run.is_some() || app.lightroom.is_some() || app.ui.dialog.is_some() {
        return;
    }
    known.push(card.root);
    if let Err(e) = app.run("file.addFromDevice", json!({"path": card.path})) {
        log::warn!("import from {}: {e}", card.name);
    }
}

/// Scan `paths` in the background, then open the review dialog (see [`poll_scan`]).
pub fn open(app: &mut LightcraftApp, paths: Vec<String>) -> Result<Value, String> {
    if app.scan.is_some() {
        return Err("a scan is already running".into());
    }
    if app.review_pending.is_some() {
        return Err("an import review is waiting for the open dialog to close".into());
    }
    let sources = paths.clone();
    let (input, paths) = ScanInput::new(&mut app.session, &paths);
    let progress = std::sync::Arc::new(ScanProgress::new(app.session.import_stall_after, app.session.stuck_readers.clone()));
    let (tx, rx) = std::sync::mpsc::channel();
    let p = progress.clone();
    let job = move || {
        let _ = tx.send(ScanEnd::Done(scan_with(input, &paths, &p)));
    };
    #[cfg(not(target_arch = "wasm32"))]
    std::thread::spawn(job);
    #[cfg(target_arch = "wasm32")]
    job();
    let guard = scan_guard(app, &progress, false);
    app.scan = Some(ScanTask {
        progress,
        rx,
        copy: false,
        destination: None,
        album: None,
        browse: false,
        sources,
        library: app.session.library_generation(),
        guard,
        resolved: None,
    });
    Ok(json!({"scanning": true}))
}

/// Show a folder's photos in the Local view (`library.browse` from the UI): the view switches at
/// once, the folder is listed and read on a worker thread (a network share can take minutes),
/// and the photos then join the view in small batches under a progress window. A browse already
/// running is replaced; the import review's scan is not.
pub fn browse(app: &mut LightcraftApp, path: &str, subfolders: Option<bool>) -> Result<Value, String> {
    // one spelling for the view, the listing and the catalog's records: `a/lib/../outside` is `a/outside`. Taken out
    // by name here, without reading the disk (a link on a stalled network mount would freeze the window): the worker
    // resolves the folder, checks it is one, and the view takes its spelling when it says (see `poll_scan`)
    let dir = lightcraft_engine::import::browse_spelling(std::path::Path::new(path)).map_err(|e| e.to_string())?;
    if app.scan.as_ref().is_some_and(|t| !t.browse) || app.import.as_ref().is_some_and(|t| !t.browse) {
        return Err("an import is running".into());
    }
    let dir_s = dir.to_string_lossy().trim_end_matches(['/', '\\']).to_string();
    let subfolders = subfolders.unwrap_or_else(|| app.session.browse.as_ref().is_some_and(|b| b.subfolders));
    let running = app.scan.as_ref().is_some_and(|t| t.browse) || app.import.as_ref().is_some_and(|t| t.browse);
    // (a path through `..` can name another folder than its spelling, past a link: only the worker can tell, so it
    // browses again; CodeRabbit on #3583)
    let through_dotdot = std::path::Path::new(path).components().any(|c| c == std::path::Component::ParentDir);
    if running && !through_dotdot && app.session.browse.as_ref().is_some_and(|b| b.path == dir_s && b.subfolders == subfolders) {
        // already reading this folder: clicking it again must not restart the progress
        app.session.source = lightcraft_engine::LibrarySource::Folder;
        app.session.keep_active_in_view();
        return Ok(json!({"path": dir_s, "subfolders": subfolders, "scanning": true}));
    }
    stop_scan(app);
    app.import = None;
    app.session.browse = Some(lightcraft_engine::Browse { path: dir_s.clone(), subfolders });
    app.session.source = lightcraft_engine::LibrarySource::Folder;
    // the photos the library already knows from this folder show at once: keep or pick the active one
    app.session.keep_active_in_view();
    let (mut input, _) = ScanInput::new(&mut app.session, std::slice::from_ref(&dir_s));
    let progress = std::sync::Arc::new(ScanProgress::new(app.session.import_stall_after, app.session.stuck_readers.clone()));
    let (tx, rx) = std::sync::mpsc::channel();
    let p = progress.clone();
    // the worker resolves the path as given: on Unix `link/..` is the folder above where the link points, which the
    // spelling (`..` taken out by name) can't tell
    let given = path.to_owned();
    let sort = app.session.sort;
    let (resolved_tx, resolved) = std::sync::mpsc::channel();
    let job = move || {
        let root = match resolve_browse_root(&given) {
            Ok(root) => root,
            Err(e) => {
                log::warn!("browse: can't read {given}: {e}");
                let _ = tx.send(ScanEnd::Stopped { probes: input.take_probes(), failed: true });
                return;
            }
        };
        let _ = resolved_tx.send(root.clone());
        // (never the library's own folder, which the import would otherwise read as photos)
        let files = match lightcraft_engine::import::browse_files(&input, &root, subfolders) {
            Ok(files) => files,
            Err(e) => {
                // no listing is sent: the scan ends as failed, and no record is taken for a file gone
                log::warn!("browse: can't list {root}: {e}");
                let _ = tx.send(ScanEnd::Stopped { probes: input.take_probes(), failed: true });
                return;
            }
        };
        // Nothing is read here: the import that follows reads the files and adds them batch by
        // batch, first screen first, so the view fills while the rest is read (PERF-CULL-1).
        // Another folder chosen meanwhile (`p.cancel`) stops the ordering, and no listing is sent.
        let Some(queue) = lightcraft_engine::import::browse_queue_until(&input, files.clone(), &sort, &p.cancel) else {
            let _ = tx.send(ScanEnd::Stopped { probes: input.take_probes(), failed: false });
            return;
        };
        p.total.store(queue.len(), Ordering::Relaxed);
        p.done.store(queue.len(), Ordering::Relaxed);
        // the files the library knows come too, marked as such: the folder bookkeeping (browse
        // times, records of files gone) needs the whole listing; only the queue is imported
        let queued: std::collections::HashSet<&String> = queue.iter().collect();
        let known: Vec<String> = files.iter().filter(|f| !queued.contains(f)).cloned().collect();
        let known =
            known.into_iter().map(|path| lightcraft_engine::import::ImportCandidate { path, duplicate: Some("path".into()), ..Default::default() });
        let candidates =
            queue.iter().cloned().map(|path| lightcraft_engine::import::ImportCandidate { path, ..Default::default() }).chain(known).collect();
        // the probes of earlier scans go back to the session with the result, for the import that follows
        // (a browse imports nothing from a review, so no folder listing goes with it)
        let listed = lightcraft_engine::import::ListedFolders::default();
        let _ = tx.send(ScanEnd::Done(lightcraft_engine::import::ScanOutput {
            candidates,
            probes: input.take_probes(),
            truncated: false,
            unreadable: 0,
            unsupported: 0,
            stopped: None,
            listed,
        }));
    };
    #[cfg(not(target_arch = "wasm32"))]
    std::thread::spawn(job);
    // the job takes the probes back out of `input`, so it is an FnMut where it runs in place
    #[cfg(target_arch = "wasm32")]
    {
        let mut job = job;
        job();
    }
    let guard = scan_guard(app, &progress, true);
    app.scan = Some(ScanTask {
        progress,
        rx,
        copy: false,
        destination: None,
        album: None,
        browse: true,
        sources: Vec::new(),
        library: app.session.library_generation(),
        guard,
        resolved: Some(resolved),
    });
    app.renderer.forget_imports();
    Ok(json!({"path": dir_s, "subfolders": subfolders, "scanning": true}))
}

/// The browsed folder `given` (the path as `browse` received it, not its [`lightcraft_engine::import::browse_spelling`])
/// as [`lightcraft_engine::import::browse_path`] resolves it, checked to be a folder: on the browse worker, as it reads the disk.
fn resolve_browse_root(given: &str) -> Result<String, String> {
    #[cfg(test)]
    held_resolve(given);
    let dir = lightcraft_engine::import::browse_path(std::path::Path::new(given)).map_err(|e| e.to_string())?;
    if !dir.is_dir() {
        return Err(format!("{given}: not a folder"));
    }
    Ok(dir.to_string_lossy().trim_end_matches(['/', '\\']).to_string())
}

/// Tests: browsed folders whose resolution waits (as on a stalled network mount) while they are listed here.
#[cfg(test)]
pub(crate) static HELD_RESOLVES: std::sync::Mutex<Vec<String>> = std::sync::Mutex::new(Vec::new());

#[cfg(test)]
fn held_resolve(given: &str) {
    let held = || HELD_RESOLVES.lock().unwrap_or_else(std::sync::PoisonError::into_inner).iter().any(|p| p == given);
    let start = std::time::Instant::now();
    while held() && start.elapsed() < std::time::Duration::from_secs(30) {
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
}

/// Collect a finished scan and open the review (called every frame).
pub fn poll_scan(app: &mut LightcraftApp, ctx: &egui::Context) {
    use lightcraft_engine::activity::{Ending, Failure, Outcome};
    show_pending_review(app, ctx);
    collect_stopped(app, ctx);
    let Some(task) = app.scan.as_ref() else { return };
    ctx.request_repaint_after(std::time::Duration::from_millis(100));
    // the worker resolved the browsed folder: the view takes its spelling (before its listing, sent after it)
    if let Some(root) = task.resolved.as_ref().and_then(|r| r.try_recv().ok())
        && let Some(b) = app.session.browse.as_mut().filter(|b| b.path != root)
    {
        b.path = root;
        app.session.keep_active_in_view();
    }
    let Some(task) = app.scan.as_ref() else { return };
    let end = match task.rx.try_recv() {
        Ok(o) => o,
        Err(std::sync::mpsc::TryRecvError::Empty) => {
            if task.guard.is_cancelled() {
                // ✕: the worker stops at its next file; don't wait for it (a NAS read can take a while)
                if let Some(task) = app.scan.as_mut() {
                    task.guard.report(Outcome::ended(Ending::Stopped));
                }
                stop_scan(app);
                return;
            }
            let (total, done) = (task.progress.total.load(Ordering::Relaxed), task.progress.done.load(Ordering::Relaxed));
            task.guard.progress(done as u64, total as u64);
            // while the folders are listed the total isn't known: say what is happening instead
            task.guard.detail(if total == 0 { crate::i18n::tr("Looking for photos…") } else { "" });
            // a file that has stopped answering is named on the row, which then offers Skip
            crate::panels::activity::follow_reads(&task.guard, &task.progress.reads);
            return;
        }
        Err(_) => {
            // a cancelled scan whose worker ended without a result was not a failure; one that died (a panic, logged) is
            let cancelled = task.guard.is_cancelled() || task.progress.cancel.load(Ordering::Relaxed);
            if let Some(task) = app.scan.take() {
                task.guard.finish(Outcome::ended(if cancelled { Ending::Stopped } else { Ending::Died }));
            }
            if !cancelled {
                app.toast_error(ctx, crate::i18n::tr("Scan failed"));
            }
            return;
        }
    };
    let Some(task) = app.scan.take() else { return };
    // (a scan another one replaced was let go without a word; one here that was cancelled was stopped by its ✕)
    let cancelled = task.progress.cancel.load(Ordering::Relaxed);
    let out = match end {
        ScanEnd::Done(out) => out,
        ScanEnd::Stopped { probes, failed } => {
            app.session.import_probes.extend(probes);
            if failed && !cancelled {
                app.toast(ctx, crate::i18n::tr("Scan failed"));
                task.guard.finish(Outcome::failed(crate::i18n::tr("Scan failed").to_string()));
            } else {
                task.guard.finish(Outcome::ended(Ending::Stopped));
            }
            return;
        }
    };
    // the probes go back to the session however the scan ended (the ones of a scan stopped earlier may be there)
    app.session.import_probes.extend(out.probes);
    if cancelled {
        task.guard.finish(Outcome::ended(Ending::Stopped));
        return;
    }
    // the Local view's row says which folder; a folder opened again replaces its last row instead of adding one
    let path = task.browse.then(|| app.session.browse.as_ref().map(|b| b.path.clone())).flatten();
    // files that could not be read are a problem the row says, by name (in the Local view, by where they are in its
    // folder: two subfolders can hold the same name): the Local view leaves them out, so nothing later would
    let named = |c: &ImportCandidate| {
        path.as_ref()
            .and_then(|root| std::path::Path::new(&c.path).strip_prefix(root).ok())
            .map_or_else(|| c.name.clone(), |rel| rel.to_string_lossy().into_owned())
    };
    let failed = out.candidates.iter().filter(|c| c.error.is_some()).count();
    let failures: Vec<Failure> = out
        .candidates
        .iter()
        .filter_map(|c| c.error.as_ref().map(|e| Failure { name: named(c), reason: e.clone() }))
        .take(lightcraft_engine::activity::MAX_FAILURES)
        .collect();
    let found_none = out.candidates.is_empty() && !task.browse;
    if let Some(path) = &path {
        for old in app.session.activity.finished() {
            if old.kind == "scan" && old.path.as_ref() == Some(path) {
                let _ = app.session.activity.dismiss(old.id);
            }
        }
    }
    // (a drive that stopped answering ends the scan early: the folder shows only what was reached before)
    let stopped = out.stopped.as_ref().map(|why| format!("{}: {why}", crate::i18n::tr("Stopped")));
    task.guard.finish(Outcome {
        ending: if failed > 0 || stopped.is_some() { Ending::Problems } else { Ending::Done },
        detail: if let Some(text) = &stopped {
            text.clone()
        } else if found_none {
            crate::i18n::tr("No photos found").to_string()
        } else {
            not_readable_text(failed)
        },
        failed,
        failures,
        path,
        ..Default::default()
    });
    // a review says so when it opens (a dialog open at the scan's end can keep it waiting past the toast's time)
    if let Some(text) = &stopped
        && (task.browse || out.candidates.is_empty())
    {
        app.toast_error(ctx, text.clone());
    }
    if task.browse {
        // the folder and the ones its photos are in count as browsed now, as `library.browse` records them, so
        // their untouched Local photos aren't forgotten while the folder is in use
        if let Some(root) = app.session.browse.as_ref().map(|b| b.path.clone()) {
            let shown: Vec<String> = out.candidates.iter().map(|c| c.path.clone()).collect();
            app.session.note_browsed(&root, &shown);
        }
        // records of files gone from the folder (deleted or moved by another app) that nothing was changed on go,
        // as `library.browse` forgets them (issue #1392)
        if let Some(b) = app.session.browse.clone() {
            let listed: Vec<String> = out.candidates.iter().map(|c| c.path.clone()).collect();
            app.session.forget_gone_from(&b.path, b.subfolders, &listed);
        }
        // what the library doesn't know yet joins the Local view
        let queue: Vec<String> =
            out.candidates.iter().filter(|c| c.duplicate != Some("path".into()) && !c.local && c.error.is_none()).map(|c| c.path.clone()).collect();
        if !queue.is_empty() {
            let undo0 = app.session.undo.len();
            let total = queue.len();
            let _ = total;
            let root = app.session.browse.as_ref().map(|b| b.path.clone());
            app.import = Some(ImportTask::new(queue, json!({"mode": "add", "local": true}), undo0, true).named_below(root.into_iter().collect()));
        }
        return;
    }
    if out.candidates.is_empty() {
        // (what the scan left out is said here too: a scan that found no photos may have had folders it
        // could not read, or files that are not photos, and the note is the same one the review shows)
        let left_out = ImportDialog { scan_limited: out.truncated, unreadable: out.unreadable, unsupported: out.unsupported, ..Default::default() };
        let text = match super::scan_note(&left_out) {
            Some(note) => format!("{} · {note}", crate::i18n::tr("No photos found")),
            None => crate::i18n::tr("No photos found").to_string(),
        };
        app.toast(ctx, text);
        return;
    }
    app.renderer.forget_imports();
    let mut d = ImportDialog::new(out.candidates);
    (d.scan_limited, d.unreadable, d.unsupported) = (out.truncated, out.unreadable, out.unsupported);
    // as the review was last used, else as the library has it
    d.read = app.session.import_defaults.read_in(lightcraft_engine::import::ReadDialog::Import);
    let trashed = d
        .candidates
        .iter()
        .map(|c| {
            c.duplicate.is_some()
                && c.existing.is_some_and(|id| app.session.catalog.photo(lightcraft_catalog::PhotoId(id)).is_some_and(|p| p.deleted))
        })
        .collect();
    d.set_trashed(trashed);
    d.set_transfer(task.copy || task.destination.is_some(), false);
    if let Some(dest) = task.destination {
        d.destination = dest;
        // the files go into that folder itself, not into dated subfolders of it
        d.organize = "flat".into();
    }
    // only an album the review offers (it may have been deleted while the scan ran)
    d.album = task.album.filter(|id| app.session.catalog.album(lightcraft_catalog::AlbumId(*id)).is_some_and(|a| !a.folder && !a.is_smart()));
    d.sources = task.sources;
    d.library = Some(app.session.library_generation());
    app.review_pending_listed = Some(out.listed);
    d.stopped = stopped;
    app.review_pending = Some(crate::state::Dialog::Import { opts: Box::new(d) });
    show_pending_review(app, ctx);
}

/// Open the review of a finished scan once no other dialog is open (a face model's licence, Settings…
/// opened while the scan ran stays), checked again shortly while it waits. A review whose library was
/// closed meanwhile (Settings can open another) is dropped.
fn show_pending_review(app: &mut LightcraftApp, ctx: &egui::Context) {
    if app.review_pending.is_none() {
        return;
    }
    if let Some(crate::state::Dialog::Import { opts }) = &app.review_pending
        && !opts.in_library(app)
    {
        app.review_pending = None;
        app.review_pending_listed = None;
        return;
    }
    if app.ui.dialog.is_none() {
        if let Some(crate::state::Dialog::Import { opts }) = &mut app.review_pending
            && let Some(text) = opts.stopped.take()
        {
            app.toast_error(ctx, text);
        }
        app.ui.dialog = app.review_pending.take();
        app.review_listed = app.review_pending_listed.take();
    } else {
        ctx.request_repaint_after(std::time::Duration::from_millis(100));
    }
}

/// Tests: a review scan that never ends while the returned value lives, so a drop meets it running.
#[cfg(test)]
pub(crate) fn stall_scan(app: &mut LightcraftApp) -> Box<dyn std::any::Any> {
    stall_scan_of(app, false)
}

/// Tests: the same, for a Local folder browse scan (`browse`) instead of the review's scan.
#[cfg(test)]
pub(crate) fn stall_scan_of(app: &mut LightcraftApp, browse: bool) -> Box<dyn std::any::Any> {
    stall_scan_in(app, Vec::new(), browse)
}

/// Tests: the same, of `sources` (what a folder queued without its review is made of).
#[cfg(test)]
pub(crate) fn stall_scan_in(app: &mut LightcraftApp, sources: Vec<String>, browse: bool) -> Box<dyn std::any::Any> {
    Box::new(scan_by_hand_in(app, sources, browse))
}

/// Tests: a scan whose worker is the test: what it sends through the returned channel is the scan's end.
#[cfg(test)]
pub(crate) fn scan_by_hand(app: &mut LightcraftApp, browse: bool) -> std::sync::mpsc::Sender<ScanEnd> {
    scan_by_hand_in(app, Vec::new(), browse)
}

/// Tests: the same, of `sources`.
#[cfg(test)]
fn scan_by_hand_in(app: &mut LightcraftApp, sources: Vec<String>, browse: bool) -> std::sync::mpsc::Sender<ScanEnd> {
    let progress = std::sync::Arc::new(ScanProgress::default());
    let (tx, rx) = std::sync::mpsc::channel::<ScanEnd>();
    let guard = scan_guard(app, &progress, false);
    app.scan = Some(ScanTask {
        progress,
        rx,
        copy: false,
        destination: None,
        album: None,
        browse,
        sources,
        library: app.session.library_generation(),
        guard,
        resolved: None,
    });
    tx
}

/// Whether an import or any scan (the review's, or a Local folder browse) is running, or a finished scan's review waits
/// for a dialog to close. A drop that was held behind a barrier is tried again only when this is over (a fresh drop
/// while an import runs is not refused: it waits in line, see [`drop_refusal`]).
pub fn running(app: &LightcraftApp) -> bool {
    app.import.is_some() || app.scan.is_some() || app.review_pending.is_some()
}

/// Whether a folder's menu can start an import review now: not while a scan or an import runs (a
/// Local folder being read counts), a finished scan's review waits for a dialog to close, or a
/// Lightroom catalog import runs (as for File ▸ Import from Folder…).
pub fn can_open(app: &LightcraftApp) -> bool {
    app.scan.is_none() && app.import.is_none() && app.review_pending.is_none() && !crate::lightroom_import::is_running(app)
}

/// Whether a `file.addFolder` folder picker is up. "Import to This Folder…" asks for its folder through that
/// command, and a second request while its picker is up is deduplicated, which would drop the new destination.
pub fn add_folder_picker_pending(app: &LightcraftApp) -> bool {
    app.pending_picks.iter().any(|p| p.command == "file.addFolder")
}

/// Why a drop of photos is refused now, in the person's language, if it is: a scan (the review's, or a Local folder
/// browse) is running, or a finished scan's review waits for a dialog to close. A drop must not replace them: `open`
/// refuses while a scan runs, and a browse that finishes replaces `app.import`. A drop while an import runs is not
/// refused: it waits in line ([`library_busy`]).
pub fn drop_refusal(app: &LightcraftApp) -> Option<String> {
    let why = match app.scan.as_ref() {
        Some(scan) if scan.browse => "A folder is still being read for the Local view: drop photos when it has finished",
        Some(_) => "A folder is still being scanned for the import review: drop photos when it has finished",
        None if app.review_pending.is_some() => "An import review is waiting to open: drop photos when it is closed",
        None => return None,
    };
    Some(crate::i18n::tr(why).to_string())
}

/// An import queued without its review: what the waiting row needs to say how it comes in and to review it after all.
#[derive(Clone, Debug)]
pub struct Unreviewed {
    /// Its task in the line.
    pub id: u64,
    sources: Vec<String>,
    /// Open the review with "copy into the library" checked (a camera / card).
    card: bool,
    /// The folder the review would have copied into, all of it in the folder itself (Import to This Folder…).
    destination: Option<String>,
    /// The album the review would have offered.
    album: Option<u64>,
    /// How it brings its files in.
    pub transfer: Transfer,
}

/// The review scan's task id when it can be queued without its review: the library lane is taken, so the review
/// would wait in line anyway once confirmed (with a free lane the review simply opens when the scan is in).
pub fn queueable_scan(app: &LightcraftApp) -> Option<u64> {
    app.scan.as_ref().filter(|t| !t.browse && !t.sources.is_empty() && library_busy(app)).map(|t| t.guard.id())
}

/// Stop the review scan and put its folder in line, to be imported as the last review was confirmed (see
/// [`import_queue::plan`]). Its row can still be reviewed ([`review_queued`]).
pub fn queue_scan(app: &mut LightcraftApp) -> Result<Value, String> {
    if queueable_scan(app).is_none() {
        return Err("no folder scan is waiting for the library".into());
    }
    let Some(task) = app.scan.take() else { return Err("no folder scan is running".into()) };
    let mut plan = import_queue::plan(app.last_import.as_ref(), task.copy || task.destination.is_some());
    // "Import to This Folder…" asked for that folder, with no subfolders in it: as its review would have been opened
    // (what else the last review chose, a rename or a conversion, still applies)
    if let Some(dest) = &task.destination {
        let mut params = if plan.params["mode"] == "copy" { plan.params } else { json!({"mode": "copy"}) };
        params["destination"] = json!(dest);
        params["organize"] = json!("flat");
        plan = import_queue::Plan { params, transfer: Transfer::Copy(Some(dest.clone())) };
    }
    let mut params = plan.params;
    // the album an Import into this album chose is part of what was asked (of this library: another one opened since
    // has other albums under the same numbers, and no review shows the person which one)
    if let Some(album) = task.album.filter(|id| {
        task.library == app.session.library_generation()
            && app.session.catalog.album(lightcraft_catalog::AlbumId(*id)).is_some_and(|a| !a.folder && !a.is_smart())
    }) {
        params["album"] = json!(album);
    }
    let result = match wait_in_line(app, import_request(&task.sources, &params, &task.sources, false, &task.sources, false)) {
        Ok(result) => result,
        Err(e) => {
            // (the line is full: the scan goes on, and the review opens when it is in)
            app.scan = Some(task);
            return Err(e);
        }
    };
    // (the worker stops at its next file: the scan is let go without a word, as one another replaced is)
    task.progress.cancel.store(true, Ordering::Relaxed);
    if let Some(id) = result["queued"].as_u64() {
        app.unreviewed.push(Unreviewed {
            id,
            sources: task.sources.clone(),
            card: task.copy,
            destination: task.destination.clone(),
            album: params["album"].as_u64(),
            transfer: plan.transfer,
        });
    }
    Ok(result)
}

/// The waiting row's Review…: the folder leaves the line and its review scan starts, as if it had been opened now.
/// Refused (the row stays) while another folder is being scanned, or while its review waits for a dialog to close.
pub fn review_queued(app: &mut LightcraftApp, id: u64) -> Result<(), String> {
    let Some(at) = app.unreviewed.iter().position(|u| u.id == id) else { return Err("it is no longer waiting".into()) };
    // (what `open` refuses too, checked before the folder leaves the line: else it would be neither waiting nor scanned)
    if app.scan.is_some() {
        return Err(crate::i18n::tr("A folder is already being scanned").to_string());
    }
    if app.review_pending.is_some() {
        return Err(crate::i18n::tr("Close the open dialog, then choose Review… again").to_string());
    }
    app.session.activity.cancel(id)?;
    let u = app.unreviewed.remove(at);
    open(app, u.sources)?;
    if let Some(t) = app.scan.as_mut() {
        t.copy = u.card;
        t.destination = u.destination;
        t.album = u.album;
    }
    Ok(())
}

/// Forget the queued-without-review notes of imports that have left the line.
pub fn prune_unreviewed(app: &mut LightcraftApp) {
    if app.unreviewed.is_empty() {
        return;
    }
    let waiting = app.session.activity.waiting();
    app.unreviewed.retain(|u| waiting.iter().any(|w| w.id == u.id));
}

impl ScanTask {
    /// `{done, total}` for `ui.inspect` (total is 0 while the folders are still being listed).
    pub fn status(&self) -> Value {
        json!({"done": self.progress.done.load(Ordering::Relaxed), "total": self.progress.total.load(Ordering::Relaxed)})
    }
}
