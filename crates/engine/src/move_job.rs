//! Moving photos to another folder on disk (`photo.moveToFolder`) as a job in two halves, so the
//! app can run the slow part on a worker thread: copying a large raw or video to a NAS takes as
//! long as the network needs, and no frame should wait for it.
//!
//! * [`start`] (on the session): the checks that need only the library (Local photos, files the
//!   library doesn't have, names clashing within the batch), the plan, and the activity row.
//! * [`MoveWork::run`] (anywhere): the checks that read the disk (the folder, each file, names
//!   already taken there), the record of the move ([`crate::move_intent`]), then the files, one at
//!   a time, reporting progress to the activity row. The row's ✕ (or `activity.cancel`) stops it
//!   after the file in hand.
//! * [`MoveFinish::finish`] (on the session): the library follows the files that moved, as one
//!   undo step.
//!
//! While a move runs, the session refuses anything else that moves files or opens another library
//! ([`Session::refuse_while_moving`]), and doesn't write the XMP sidecars of the photos on their way
//! (they're written once the move ends, at the new place).

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock, Weak};

use lightcraft_catalog::{Op, PhotoId, Source};
use serde_json::{Value, json};

use crate::activity::{Cancel, Ending, Outcome, TaskGuard, TaskHandle};
use crate::cmd::bad;
use crate::move_intent::MoveRecord;
use crate::rename::{BatchMoveError, MoveFs, RealFs, RenamePlan, folded_path};
use crate::{EngineError, Result, Session};

const C: &str = "photo.moveToFolder";

/// The files of a move in progress; the session holds it weakly ([`Session::moving`]), so the move
/// ends for the session when its [`MoveFinish`] goes, however it goes.
#[derive(Debug, Default)]
pub struct MoveTicket {
    /// The old paths, folded (see [`folded_path`]).
    paths: HashSet<String>,
}

impl MoveTicket {
    fn holds(&self, path: &str) -> bool {
        self.paths.contains(&folded_path(path))
    }
}

/// The half of a move that reads and writes the disk ([`MoveWork::run`]); `Send`, for a worker.
pub struct MoveWork {
    folder: String,
    /// (from, to), one per file (a master and its virtual copies move once).
    moves: Vec<(String, String)>,
    /// Where the record of the move goes; `None` for a library without a folder of its own.
    library: Option<PathBuf>,
    task: TaskHandle,
    /// Where the worker put the record of the move, for [`MoveFinish::abandon`].
    record: Arc<OnceLock<PathBuf>>,
}

/// What [`MoveWork::run`] did, for [`MoveFinish::finish`].
pub struct MoveOutcome(Ran);

enum Ran {
    /// Nothing moved, and why (a name taken, a file offline…).
    Refused(String),
    /// The first `moved` files moved; fewer than all when the move was stopped.
    Moved { moved: usize, record: Option<MoveRecord> },
    /// A file couldn't move: the earlier ones were moved back (what couldn't be is in `error`).
    Failed { error: BatchMoveError, record: Option<MoveRecord> },
}

/// The half of a move that brings the library along ([`MoveFinish::finish`]); it holds the
/// activity row and the session's [`MoveTicket`].
pub struct MoveFinish {
    folder: String,
    plans: Vec<RenamePlan>,
    /// How many photos the move is for, as the person counts them (a virtual copy is one).
    photos: usize,
    /// The photos told to be skipped, as `photo.moveToFolder` reports them.
    skipped: Vec<Value>,
    identity: Arc<()>,
    ticket: Arc<MoveTicket>,
    guard: TaskGuard,
    record: Arc<OnceLock<PathBuf>>,
}

impl std::fmt::Debug for MoveFinish {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MoveFinish").field("folder", &self.folder).field("files", &self.plans.len()).finish()
    }
}

/// A move as asked for and sorted ([`plan`]): the files that go, and the photos left out.
pub(crate) struct Planned {
    folder: String,
    plans: Vec<RenamePlan>,
    /// The photos that go, as the person counts them (a virtual copy is one, though it moves with its master's file).
    movable: usize,
    /// The photos told to be skipped, as `photo.moveToFolder` reports them.
    skipped: Vec<Value>,
}

impl Planned {
    /// Every photo is in that folder already, or left out.
    pub(crate) fn is_empty(&self) -> bool {
        self.plans.is_empty()
    }

    /// `photo.moveToFolder`'s answer when nothing moves.
    pub(crate) fn nothing_moved(self) -> Value {
        json!({"moved": 0, "of": 0, "folder": self.folder, "stopped": false, "skipped": self.skipped})
    }
}

/// Sort the move `p` asks for (`{folder, ids?, skip?}`; the selection by default): a name taken in
/// the folder, a file that is missing or offline, or a photo that can't move as it is refuses the
/// whole move, unless that photo is in `skip` (see [`crate::move_check`]).
pub(crate) fn plan(s: &Session, p: &Value) -> Result<Planned> {
    let folder = crate::cmd::browse::folder_param(p, "folder").ok_or_else(|| bad(C, "missing folder"))?;
    let dir = Path::new(folder);
    s.refuse_while_trashing(C)?;
    s.refuse_while_moving()?;
    // refused before anything else touches the disk, as it always was
    if !dir.is_dir() {
        return Err(EngineError::Other(format!(
            "“{}” is not a folder (offline, moved or deleted?). Choose another folder",
            crate::move_check::folder_name(folder)
        )));
    }
    let ids = s.targets(p).map_err(|e| match e {
        EngineError::BadParams { cmd, msg } if cmd.is_empty() => bad(C, msg),
        e => e,
    })?;
    if ids.is_empty() {
        return Err(EngineError::Other("There are no photos to move. Select some first".into()));
    }
    let skip = crate::move_check::skip_param(p)?;
    let sorted = crate::move_check::sort(s, dir, &ids, &skip, false)?;
    if !sorted.problems.is_empty() {
        return Err(EngineError::Other(crate::move_check::refusal(folder, &sorted.problems)));
    }
    let skipped = sorted.skipped.iter().map(crate::move_check::Left::json).collect();
    Ok(Planned { folder: folder.to_string(), plans: sorted.plans, movable: sorted.movable, skipped })
}

/// What [`start_or_answer`] made of a move.
pub enum Started {
    /// The work to run (on a worker thread, or here) and what finishes it.
    Work(MoveWork, MoveFinish),
    /// Every photo is in that folder already, or left out: `photo.moveToFolder`'s answer, with the photos left out.
    Nothing(Value),
}

/// Plan the move `p` asks for (`{folder, ids?, skip?}`; the selection by default) and take the session for it, or
/// answer that nothing moves (every photo is in that folder already, or skipped). `cancellable`: the activity row
/// gets ✕ (only where something can press it while the work runs: the app's background move).
pub fn start_or_answer(s: &mut Session, p: &Value, cancellable: bool) -> Result<Started> {
    let planned = plan(s, p)?;
    if planned.is_empty() {
        return Ok(Started::Nothing(planned.nothing_moved()));
    }
    begin(s, planned, cancellable).map(|(work, finish)| Started::Work(work, finish))
}

/// [`start_or_answer`] without the answer: `None` when nothing moves.
pub fn start(s: &mut Session, p: &Value, cancellable: bool) -> Result<Option<(MoveWork, MoveFinish)>> {
    Ok(match start_or_answer(s, p, cancellable)? {
        Started::Work(work, finish) => Some((work, finish)),
        Started::Nothing(_) => None,
    })
}

/// Take the session for a move [`plan`] sorted (not empty).
pub(crate) fn begin(s: &mut Session, planned: Planned, cancellable: bool) -> Result<(MoveWork, MoveFinish)> {
    let Planned { folder, plans, movable, skipped } = planned;
    // a sidecar repair left by an earlier step goes first, as before any file move
    s.repair_sidecars_with(&RealFs)?;
    let moves: Vec<(String, String)> = plans.iter().filter_map(|pl| Some((pl.from_path.clone()?, pl.to_path.clone()?))).collect();
    let ticket = Arc::new(MoveTicket { paths: moves.iter().map(|(from, _)| folded_path(from)).collect() });
    s.moving = Arc::downgrade(&ticket);
    let guard = s.activity.start("move", "Moving photos", if cancellable { Cancel::Yes } else { Cancel::No });
    guard.progress(0, moves.len() as u64);
    let library = s.library.as_ref().filter(|l| l.on_disk).map(|l| l.dir.clone());
    let record = Arc::new(OnceLock::new());
    // photos as the person counts them, from the sort
    let photos = movable;
    let work = MoveWork { folder: folder.clone(), moves, library, task: guard.handle(), record: Arc::clone(&record) };
    let finish = MoveFinish { folder, plans, photos, skipped, identity: Arc::clone(&s.library_identity), ticket, guard, record };
    Ok((work, finish))
}

fn clash_message(names: &[String], folder: &str) -> String {
    let s = if names.len() == 1 { "" } else { "s" };
    format!("{} already in {folder}: nothing was moved. Rename the photo{s} first or choose another folder", names.join(", "))
}

impl MoveWork {
    /// Do the move on the disk (blocking: on a worker thread in the app).
    pub fn run(self) -> MoveOutcome {
        self.run_with(&RealFs)
    }

    pub(crate) fn run_with(self, fs: &dyn MoveFs) -> MoveOutcome {
        let MoveWork { folder, moves, library, task, record: slot } = self;
        if !Path::new(&folder).is_dir() {
            return MoveOutcome(Ran::Refused(format!("{folder} is not a folder (create it first with folder.create)")));
        }
        let mut clashes = Vec::new();
        for (from, to) in &moves {
            if !fs.is_file(Path::new(from)) {
                return MoveOutcome(Ran::Refused(format!("{from} is missing or offline: find it first (Find Missing Photos)")));
            }
            if fs.exists(Path::new(to)) {
                clashes.push(name_of(to));
            }
        }
        if !clashes.is_empty() {
            return MoveOutcome(Ran::Refused(clash_message(&clashes, &folder)));
        }
        // the record goes first: whatever stops this from here on, the next open finds the files
        let record = match library.as_deref().map(|dir| crate::move_intent::write(dir, &moves)).transpose() {
            Ok(r) => r,
            Err(e) => return MoveOutcome(Ran::Refused(format!("can't note the move in the library ({e}): nothing was moved"))),
        };
        if let Some(r) = &record {
            let _ = slot.set(r.path().to_path_buf());
        }
        let total = moves.len() as u64;
        let ran = crate::rename::move_all_with(fs, &moves, &mut |k, (from, _)| {
            task.progress(k as u64, total);
            task.detail(&name_of(from));
            !task.is_cancelled()
        });
        match ran {
            Ok(moved) => {
                task.progress(moved as u64, total);
                MoveOutcome(Ran::Moved { moved, record })
            }
            Err(error) => MoveOutcome(Ran::Failed { error, record }),
        }
    }
}

fn name_of(path: &str) -> String {
    Path::new(path).file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_else(|| path.to_string())
}

impl MoveFinish {
    /// How many files the move is for.
    pub fn files(&self) -> usize {
        self.plans.len()
    }

    /// The activity row's id (`activity.cancel`).
    pub fn task_id(&self) -> u64 {
        self.guard.id()
    }

    /// The worker died part-way (a panic, logged; this path runs only then) and left no outcome: the move is over for the
    /// session, the library follows whatever had moved (from the move's record, or, when none was
    /// written because the library has no folder of its own, from the files themselves), and the
    /// sidecars held back meanwhile are written where their photos now are.
    pub fn abandon(self, s: &mut Session) -> crate::move_intent::Recovered {
        let MoveFinish { plans, record, ticket, guard, .. } = self;
        let own = record.get().cloned();
        drop(ticket);
        // (the row says the worker died: a dropped guard says so only while panicking)
        guard.finish(Outcome { ending: Ending::Died, ..Default::default() });
        let recovered = match own {
            Some(own) => s.recover_own_move(Some(&own)),
            // (a library with a folder wrote its record before the first file moved: none means nothing had)
            None if s.library.as_ref().is_some_and(|l| l.on_disk) => crate::move_intent::Recovered::default(),
            None => s.follow_arrived(&plans),
        };
        let held: Vec<PhotoId> = std::mem::take(&mut s.held_sidecars).into_iter().collect();
        if s.xmp.auto_write {
            s.auto_write_sidecar_ids(held);
        }
        s.save_held_sidecars();
        recovered
    }

    /// Point the photos whose files moved at their new place, as one undo step (labelled as
    /// before: "Move 3 Photos to “Porto”", counting virtual copies). `{moved, of, folder, stopped}`
    /// (in files); an error when nothing moved (`Refused`, `Failed`: the files went back, or the library
    /// follows those that couldn't). The sidecars held back meanwhile are written after.
    pub fn finish(self, s: &mut Session, out: MoveOutcome) -> Result<Value> {
        let MoveFinish { folder, plans, photos, skipped, identity, ticket, guard, record: _ } = self;
        // the move is over for the session: other file work, and these photos' sidecars, may go on
        drop(ticket);
        let held: Vec<PhotoId> = std::mem::take(&mut s.held_sidecars).into_iter().collect();
        if !Arc::ptr_eq(&identity, &s.library_identity) {
            // (opening a library is refused while a move runs; the record stays for that library)
            let why = "another library was opened while photos were being moved: their library follows them when it opens";
            guard.finish(Outcome::failed(why));
            return Err(EngineError::Other(why.into()));
        }
        let r = match out.0 {
            Ran::Refused(why) => Err(bad(C, why)),
            Ran::Failed { error, record } => {
                let op = Op::Batch { ops: s.set_file_ops(&plans) };
                let (message, _, followed) = s.follow_stuck(&op, error, true);
                // (a library that could not follow keeps the record for the next open)
                if followed {
                    s.settle_moves(record);
                }
                Err(EngineError::Other(message))
            }
            Ran::Moved { moved, record } => s.follow_moved(&folder, plans.get(..moved).unwrap_or(&plans), (plans.len(), photos, skipped), record),
        };
        // the row says how it ended (the same words the error carries when it didn't go through)
        guard.finish(match &r {
            Ok(_) => Outcome::default(),
            Err(e) => Outcome::failed(e.to_string()),
        });
        // (a sidecar write the person switched off meanwhile isn't made now)
        if s.xmp.auto_write {
            s.auto_write_sidecar_ids(held);
        }
        s.save_held_sidecars();
        r
    }
}

fn label(n: usize, folder: &str) -> String {
    let name = Path::new(folder).file_name().map(|f| f.to_string_lossy().to_string()).unwrap_or_else(|| folder.to_string());
    format!("Move {n} Photo{} to “{name}”", if n == 1 { "" } else { "s" })
}

impl Session {
    /// The library follows the files `done` moved, out of `of` planned: one undo step. When it
    /// can't (a failed commit), the files go back (the record covers a stop meanwhile).
    fn follow_moved(&mut self, folder: &str, done: &[RenamePlan], of: (usize, usize, Vec<Value>), record: Option<MoveRecord>) -> Result<Value> {
        let (of_files, of, skipped) = of;
        let ops = self.set_file_ops(done);
        // (all of them when nothing was stopped; a stopped move counts the photos of the files that moved, never more than were asked)
        let moved = if done.len() < of_files { ops.len().min(of) } else { of };
        let result = json!({"moved": moved, "of": of, "folder": folder, "stopped": done.len() < of_files, "skipped": skipped});
        if ops.is_empty() {
            self.settle_moves(record);
            return Ok(result);
        }
        match self.commit(&label(ops.len(), folder), Op::Batch { ops }) {
            Ok(()) => {
                self.settle_moves(record);
                Ok(result)
            }
            Err(err) => {
                let back: Vec<(String, String)> = done.iter().rev().filter_map(|pl| Some((pl.to_path.clone()?, pl.from_path.clone()?))).collect();
                let r = crate::rename::move_all(&RealFs, &back);
                match r {
                    Ok(()) => {
                        self.settle_moves(record);
                        Err(err)
                    }
                    // the files stay at the new place and the library doesn't point there: the record stays for the next open
                    Err(e) => Err(EngineError::Other(format!("{err}; the moved files could not all be moved back: {}", e.message))),
                }
            }
        }
    }

    /// A move whose worker died and that wrote no record (the library has no folder of its own):
    /// the photos whose file is at the new place and no longer at the old one follow it, as one
    /// undo step. A file that did not arrive stays where the library has it.
    fn follow_arrived(&mut self, plans: &[RenamePlan]) -> crate::move_intent::Recovered {
        use crate::move_intent::{MoveProblem, Recovered};
        let arrived: Vec<RenamePlan> = plans
            .iter()
            .filter(|pl| match (&pl.from_path, &pl.to_path) {
                (Some(from), Some(to)) => RealFs.is_file(Path::new(to)) && !RealFs.exists(Path::new(from)),
                _ => false,
            })
            .cloned()
            .collect();
        let mut out = Recovered::default();
        // (the worker may have died between a photo file and its sidecars)
        for (from, to) in arrived.iter().filter_map(|pl| Some((pl.from_path.as_deref()?, pl.to_path.as_deref()?))) {
            if let Err(error) = crate::rename::finish_sidecars(&RealFs, from, to) {
                out.problems.push(MoveProblem::SidecarStayed { to: to.to_string(), error });
            }
        }
        let ops = self.set_file_ops(&arrived);
        if ops.is_empty() {
            return out;
        }
        let n = ops.len();
        match self.commit(&format!("Finish Moving {n} Photo{}", if n == 1 { "" } else { "s" }), Op::Batch { ops }) {
            Ok(()) => out.relinked = n,
            Err(e) => out.problems.push(MoveProblem::NotFollowed { error: e.to_string() }),
        }
        out
    }

    /// The move running in the background, if any.
    pub fn move_running(&self) -> bool {
        self.moving.strong_count() > 0
    }

    /// Anything else that moves files, or opens another library, waits for a running move: the
    /// two would work on the same files, and the move's photos belong to this library.
    pub(crate) fn refuse_while_moving(&self) -> Result<()> {
        if self.move_running() {
            return Err(EngineError::MoveInProgress("photos are being moved to another folder: wait for the move to finish, or stop it".into()));
        }
        Ok(())
    }

    /// The photos `op` removes whose file is on its way to another folder (the undo of its import,
    /// Delete Permanently, Empty Recently Deleted, the import's purge of a trashed photo), or
    /// points at another file (Convert to DNG, Find Missing Photos, Locate, a Synchronize relink,
    /// the undo of any of them): the move's finish finds no photo at the old place to point at the
    /// new one, and a later undo would bring the photo back at a path the files have left.
    pub(crate) fn detached_moving_photos(&self, op: &Op) -> Vec<PhotoId> {
        let mut found = Vec::new();
        if !self.move_running() {
            return found;
        }
        let mut pending = vec![op];
        while let Some(op) = pending.pop() {
            match op {
                Op::Batch { ops } => pending.extend(ops),
                Op::RemovePhoto { id } | Op::Relink { id, .. } => {
                    if let Some(Source::File { path }) = self.catalog.photo(*id).map(|p| &p.source)
                        && self.is_moving(path)
                        && !found.contains(id)
                    {
                        found.push(*id);
                    }
                }
                _ => {}
            }
        }
        found
    }

    /// Refuse a change that would detach `moving` (see [`Session::detached_moving_photos`]),
    /// saying how many photos it is. Nothing to refuse when the list is empty.
    pub(crate) fn refuse_detaching(&self, moving: &[PhotoId]) -> Result<()> {
        match moving.len() {
            0 => Ok(()),
            1 => Err(EngineError::MoveInProgress("1 photo is being moved to another folder: wait for the move to finish".into())),
            n => Err(EngineError::MoveInProgress(format!("{n} photos are being moved to another folder: wait for the move to finish"))),
        }
    }

    /// The file `path` is on its way to another folder (its sidecar waits until it arrives).
    pub(crate) fn is_moving(&self, path: &str) -> bool {
        Weak::upgrade(&self.moving).is_some_and(|t| t.holds(path))
    }
}
