//! The text the free-text search looks through, kept for every photo of a big library and made on a thread of its own.
//!
//! On a library of 1.7 million photos the first search of a session took 0.7 s, about 0.4 s longer than a later one: the time of
//! making this text on the thread that asks, which stalled the first keystroke. A library of [`BACKGROUND_FROM`] photos or more has it
//! made on another thread when the library is opened, in one block
//! rather than a box and a table entry for each photo. A search that comes before the thread is done waits for the rest of it,
//! rather than for all of it. [`crate::index`] says when the text of a photo is used and when it is made again; this says how the
//! text of all of them is made, and taken.

use std::collections::{BTreeMap, HashSet};
use std::sync::atomic::{AtomicU8, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::thread::JoinHandle;

use crate::index::{ByPhoto, PhotoText, addr, push_text, text_len};
use crate::{Photo, PhotoId};

/// A catalog's photos.
type Photos = BTreeMap<PhotoId, Arc<Photo>>;

/// From how many photos the text is made on another thread, started when the library is opened
/// ([`crate::Catalog::prebuild_search_text`]).
/// Below it a search makes the text where it stands: about 0.4 s at 1.7 million photos is roughly 12 ms at this size (scaled from
/// that, not measured).
pub(crate) const BACKGROUND_FROM: usize = 50_000;

/// A block is never made again for this many changed photos or fewer, however few photos it holds ([`SearchText::too_many`]).
const STALE_FLOOR: usize = 1024;

/// Where one photo's text is in [`SearchText`].
struct Entry {
    id: PhotoId,
    /// The address of the photo the text was made from ([`addr`]).
    at: usize,
    /// Where the text ends in the block (it starts where the one before ends).
    end: u32,
}

/// The text of every photo of a catalog as the thread that made it saw them, in one block, with the photos changed since beside it.
pub(crate) struct SearchText {
    /// One for each photo, ascending by id.
    entries: Vec<Entry>,
    text: Box<str>,
    /// The entry the last lookup found: a pass over the library in id order finds each next one beside it.
    last: AtomicUsize,
    /// Photos edited, added or removed since the block was made: the text each has now (`None`: it is gone), which the block's
    /// entry for its id no longer is.
    again: ByPhoto<Option<PhotoText>>,
}

impl SearchText {
    /// The text of `photos`, ascending by id: one pass sizes the block and one fills it, so a block of ASCII is never copied to
    /// grow. `None` when it is longer than the offsets tell (4 GiB).
    pub(crate) fn of<'a>(photos: impl Iterator<Item = &'a Photo> + Clone) -> Option<SearchText> {
        let started = web_time::Instant::now();
        let (count, len) = photos.clone().fold((0usize, 0usize), |(n, len), p| (n.saturating_add(1), len.saturating_add(text_len(p, &p.people()))));
        // (a few letters are longer lower-cased, like 'İ' and 'Ⱥ': the margin is for them, a text that outgrows it is copied to a
        // larger string once, and the box gives back what is left over)
        let mut text = String::with_capacity(len.saturating_add(len / 64));
        let mut entries = Vec::with_capacity(count);
        for p in photos {
            push_text(&mut text, p, &p.people());
            entries.push(Entry { id: p.id, at: addr(p), end: u32::try_from(text.len()).ok()? });
        }
        let kept = SearchText { entries, text: text.into_boxed_str(), last: AtomicUsize::new(0), again: ByPhoto::default() };
        log::info!("catalog: search text made for {count} photos in {:.1} ms", started.elapsed().as_secs_f64() * 1e3);
        Some(kept)
    }

    /// The text of `p`, when `p` is the photo it was made from: at the same place in memory ([`addr`]).
    pub(crate) fn get(&self, p: &Photo) -> Option<&str> {
        if let Some(own) = self.again.get(&p.id) {
            return own.as_ref().filter(|t| t.at == addr(p)).map(PhotoText::text);
        }
        let i = self.find(p.id)?;
        let entry = self.entries.get(i)?;
        if entry.at != addr(p) {
            return None;
        }
        let start = match i.checked_sub(1) {
            Some(before) => self.entries.get(before)?.end,
            None => 0,
        };
        self.text.get(usize::try_from(start).ok()?..usize::try_from(entry.end).ok()?)
    }

    /// Where photo `id` is in the entries. A pass over the library asks in id order and skips photos, so the entry is looked for
    /// ahead of the last one found, doubling the step and then halving it, before a search of all of them.
    fn find(&self, id: PhotoId) -> Option<usize> {
        let from = self.last.load(Ordering::Relaxed);
        let i = match self.entries.get(from) {
            Some(first) if first.id <= id => {
                let ahead = self.entries.get(from..)?;
                let mut end = 1usize;
                while ahead.get(end).is_some_and(|e| e.id <= id) {
                    end = end.saturating_mul(2);
                }
                let near = ahead.get(..end.min(ahead.len()))?;
                let k = near.partition_point(|e| e.id < id);
                if near.get(k).is_none_or(|e| e.id != id) {
                    return None;
                }
                from.saturating_add(k)
            }
            _ => self.entries.binary_search_by_key(&id, |e| e.id).ok()?,
        };
        self.last.store(i, Ordering::Relaxed);
        Some(i)
    }

    /// Whether `n` photos changed since the block was made are too many to keep beside it: more than an eighth of the photos the
    /// block holds, and more than `STALE_FLOOR`. The text is made again then, rather than kept as the old block and a text for each
    /// of them.
    fn too_many(&self, n: usize) -> bool {
        n > (self.entries.len() / 8).max(STALE_FLOOR)
    }

    fn changed(&mut self, id: PhotoId, p: Option<&Photo>) {
        self.again.insert(id, p.map(PhotoText::of));
    }

    fn moved(&mut self, id: PhotoId, p: &Photo) {
        let at = addr(p);
        match self.again.get_mut(&id) {
            Some(Some(own)) => own.at = at,
            Some(None) => {}
            None => {
                if let Some(i) = self.find(id)
                    && let Some(entry) = self.entries.get_mut(i)
                {
                    entry.at = at;
                }
            }
        }
    }
}

/// What `KeptText::state` says, readable without the lock that guards [`Work`]: nothing has started, a thread is making the text,
/// or there is nothing more to wait for (it is made, or it can't be).
const IDLE: u8 = 0;
const RUNNING: u8 = 1;
const OVER: u8 = 2;

/// How the making of the text is going.
enum Work {
    Idle,
    /// A thread is making the text of the photos the catalog had when it started. Since then `edited` are the photos whose text
    /// changed, and `moved` the ones now at another place in memory (`photo_mut` copies a photo that a thread holds).
    Running {
        worker: JoinHandle<Option<SearchText>>,
        edited: HashSet<PhotoId>,
        moved: HashSet<PhotoId>,
    },
    /// Nothing more to wait for: the text was made (here, or by the thread and taken), or it can't be (the thread failed, or the
    /// text didn't fit), which leaves a search reading each photo's own fields.
    Over,
}

/// The text the free-text search reads, when it is made, and the making of it.
pub(crate) struct KeptText {
    made: OnceLock<SearchText>,
    state: AtomicU8,
    work: Mutex<Work>,
    from: usize,
    #[cfg(test)]
    hold: Mutex<Option<Arc<dyn Fn() + Send + Sync>>>,
    #[cfg(test)]
    spawned: AtomicUsize,
    #[cfg(test)]
    refuse: bool,
}

impl Default for KeptText {
    fn default() -> Self {
        KeptText {
            made: OnceLock::new(),
            state: AtomicU8::new(IDLE),
            work: Mutex::new(Work::Idle),
            from: BACKGROUND_FROM,
            #[cfg(test)]
            hold: Mutex::new(None),
            #[cfg(test)]
            spawned: AtomicUsize::new(0),
            #[cfg(test)]
            refuse: false,
        }
    }
}

impl KeptText {
    /// The text, when it is made. `pass`: a pass over the whole library asks, which waits for the thread that is making the text and
    /// makes it here when there is none. A search of one photo (`pass` false) neither waits for the thread nor makes the text here
    /// (it waits only for a pass that is already doing that): it takes the thread's text when the thread is done, making again the
    /// text of the photos edited meanwhile, and is otherwise answered from the photo's own fields.
    pub(crate) fn get(&self, photos: &Photos, pass: bool) -> Option<&SearchText> {
        if let Some(text) = self.made.get() {
            return Some(text);
        }
        match self.state.load(Ordering::Relaxed) {
            OVER => return None,
            IDLE if !pass => return None,
            _ => {}
        }
        let mut work = self.work.lock().ok()?;
        match &*work {
            Work::Idle if pass => self.make_here(&mut work, photos),
            // (a pass waits for the thread, which has begun: what is left of its work is some of the work the pass would do itself,
            // lower-casing every field of every photo)
            Work::Running { worker, .. } if pass || worker.is_finished() => {
                let Work::Running { worker, edited, moved } = std::mem::replace(&mut *work, Work::Over) else { return None };
                // (a thread that failed has no text: nothing is kept)
                let Some(mut text) = worker.join().ok().flatten() else { return self.over(&mut work, None) };
                if text.too_many(edited.len()) {
                    // (so many photos changed meanwhile that the text of each is not made again one by one: the thread's text is
                    // dropped, and made again from the photos as they are once the old text is freed. An eighth of the photos is a
                    // judgment, not a measurement)
                    drop(text);
                    if pass {
                        return self.make_here(&mut work, photos);
                    }
                    *work = Work::Idle;
                    self.state.store(IDLE, Ordering::Relaxed);
                    return None;
                }
                for id in moved.difference(&edited) {
                    if let Some(p) = photos.get(id) {
                        text.moved(*id, p);
                    }
                }
                for id in edited {
                    text.changed(id, photos.get(&id).map(|p| &**p));
                }
                self.over(&mut work, Some(text))
            }
            _ => None,
        }
    }

    /// Makes the text of every photo here, where the search stands: a small library's, or a big one that no thread was started
    /// for (or whose thread's text was too far out of date to take).
    fn make_here(&self, work: &mut Work, photos: &Photos) -> Option<&SearchText> {
        let text = SearchText::of(photos.values().map(|p| &**p));
        self.over(work, text)
    }

    /// Starts making the text on another thread, for a library big enough for it, when it isn't made or being made.
    pub(crate) fn start(&self, photos: &Photos) {
        if photos.len() < self.from || self.made.get().is_some() || self.state.load(Ordering::Relaxed) != IDLE {
            return;
        }
        let Ok(mut work) = self.work.lock() else { return };
        if matches!(*work, Work::Idle)
            && let Some(worker) = self.spawn(photos)
        {
            self.running(&mut work, worker);
        }
    }

    /// The thread that makes the text of `photos`, or `None` when no thread is to be had (the browser has none).
    fn spawn(&self, photos: &Photos) -> Option<JoinHandle<Option<SearchText>>> {
        // (the browser has no threads: nothing to collect the photos for)
        if cfg!(target_arch = "wasm32") {
            return None;
        }
        #[cfg(test)]
        {
            self.spawned.fetch_add(1, Ordering::Relaxed);
            if self.refuse {
                return None;
            }
        }
        #[cfg(test)]
        let hold = self.hold.lock().ok().and_then(|hold| hold.clone());
        // (the photos aren't copied: the thread holds each one, and an edit of one meanwhile copies that one)
        let photos: Vec<Arc<Photo>> = photos.values().cloned().collect();
        std::thread::Builder::new()
            .name("search-text".into())
            .spawn(move || {
                #[cfg(test)]
                if let Some(hold) = hold {
                    hold();
                }
                SearchText::of(photos.iter().map(|p| &**p))
            })
            .ok()
    }

    fn running(&self, work: &mut Work, worker: JoinHandle<Option<SearchText>>) {
        *work = Work::Running { worker, edited: HashSet::new(), moved: HashSet::new() };
        self.state.store(RUNNING, Ordering::Relaxed);
    }

    /// The making is over with `text`, or without any.
    fn over(&self, work: &mut Work, text: Option<SearchText>) -> Option<&SearchText> {
        *work = Work::Over;
        self.state.store(OVER, Ordering::Relaxed);
        if let Some(text) = text {
            // (it is set once: under the lock, and only when the text isn't made)
            let _ = self.made.set(text);
        }
        self.made.get()
    }

    /// Photo `id` is `p` now (`None`: it is gone).
    pub(crate) fn changed(&mut self, id: PhotoId, p: Option<&Photo>) {
        match self.made.get_mut() {
            Some(text) => {
                text.changed(id, p);
                if text.too_many(text.again.len()) {
                    self.forget();
                }
            }
            None => self.meanwhile(id, true),
        }
    }

    /// Photo `id` is at `p` now, another place in memory than the one its text was made at (`photo_mut` says so for every edit in
    /// place: the text is the same unless [`KeptText::changed`] says it changed).
    pub(crate) fn moved(&mut self, id: PhotoId, p: &Photo) {
        match self.made.get_mut() {
            Some(text) => text.moved(id, p),
            None => self.meanwhile(id, false),
        }
    }

    /// Photo `id` changed (`text_changed`: its text, else only its place) while a thread makes the text: the thread's text is
    /// brought up to date for it when that is taken.
    fn meanwhile(&mut self, id: PhotoId, text_changed: bool) {
        if let Ok(Work::Running { edited, moved, .. }) = self.work.get_mut() {
            if text_changed {
                edited.insert(id);
            } else {
                moved.insert(id);
            }
        }
    }

    /// Drops the text, so that it is made again from the photos as they are when a pass asks.
    fn forget(&mut self) {
        self.made.take();
        if let Ok(work) = self.work.get_mut() {
            *work = Work::Idle;
        }
        *self.state.get_mut() = IDLE;
    }
}

#[cfg(test)]
impl KeptText {
    pub(crate) fn set_from(&mut self, from: usize) {
        self.from = from;
    }

    /// Refuse every thread that would make the text, as a system that has none to give does.
    pub(crate) fn refuse_threads(&mut self) {
        self.refuse = true;
    }

    /// How many photos have a text of their own beside the block: the ones changed since it was made.
    pub(crate) fn again_len(&self) -> usize {
        self.made.get().map_or(0, |text| text.again.len())
    }

    /// Run `f` first on every thread that makes the text.
    pub(crate) fn hold(&self, f: Arc<dyn Fn() + Send + Sync>) {
        if let Ok(mut hold) = self.hold.lock() {
            *hold = Some(f);
        }
    }

    /// Where the text is, without taking it: `idle`, `making`, `finished` (the thread is done, its text not taken), `made` or `failed`.
    pub(crate) fn describe(&self) -> &'static str {
        if self.made.get().is_some() {
            return "made";
        }
        match self.work.lock().as_deref() {
            Ok(Work::Idle) => "idle",
            Ok(Work::Running { worker, .. }) if worker.is_finished() => "finished",
            Ok(Work::Running { .. }) => "making",
            Ok(Work::Over) => "failed",
            Err(_) => "poisoned",
        }
    }

    pub(crate) fn spawned(&self) -> usize {
        self.spawned.load(Ordering::Relaxed)
    }
}
