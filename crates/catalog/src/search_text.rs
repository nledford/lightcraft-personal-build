//! The text the free-text search looks through, kept for every photo of a big library and made on a thread of its own.
//!
//! Making it for 1.7 million photos took 0.6 s and 150 MiB on the thread that asks the first search, which stalled the first
//! keystroke of a session. A library of [`BACKGROUND_FROM`] photos or more has it made on another thread, in one block (a box
//! and a table entry for each photo cost more than the text itself), started when the library is opened or by the first search.
//! A search that comes before the thread is done reads each photo's own fields, as a search read them before the text was
//! kept. [`crate::index`] says when the text of a photo is used and when it is made again; this says how the text of all of
//! them is made, and taken.

use std::collections::{BTreeMap, HashSet};
use std::sync::atomic::{AtomicU8, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::thread::JoinHandle;

use crate::index::{ByPhoto, PhotoText, addr, push_text, text_len};
use crate::{Photo, PhotoId};

/// A catalog's photos.
type Photos = BTreeMap<PhotoId, Arc<Photo>>;

/// From how many photos the text is made on another thread. Below it, making the text takes under 20 ms, which a search makes
/// where it stands.
pub(crate) const BACKGROUND_FROM: usize = 50_000;

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
    /// The text of `photos`, ascending by id: one pass sizes the block and one fills it, so it is never copied to grow.
    /// `None` when it is longer than the offsets tell (4 GiB).
    pub(crate) fn of<'a>(photos: impl Iterator<Item = &'a Photo> + Clone) -> Option<SearchText> {
        let (count, len) = photos.clone().fold((0usize, 0usize), |(n, len), p| (n.saturating_add(1), len.saturating_add(text_len(p, &p.people()))));
        // (a letter that is longer lower-cased, 'İ', is all that makes the text longer than the sum: a margin for it, and the box
        // gives back what is left over)
        let mut text = String::with_capacity(len.saturating_add(len / 64));
        let mut entries = Vec::with_capacity(count);
        for p in photos {
            push_text(&mut text, p, &p.people());
            entries.push(Entry { id: p.id, at: addr(p), end: u32::try_from(text.len()).ok()? });
        }
        Some(SearchText { entries, text: text.into_boxed_str(), last: AtomicUsize::new(0), again: ByPhoto::default() })
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

    /// Where photo `id` is in the entries.
    fn find(&self, id: PhotoId) -> Option<usize> {
        let last = self.last.load(Ordering::Relaxed);
        let beside = [last.saturating_add(1), last].into_iter().find(|&i| self.entries.get(i).is_some_and(|e| e.id == id));
        let i = match beside {
            Some(i) => i,
            None => self.entries.binary_search_by_key(&id, |e| e.id).ok()?,
        };
        self.last.store(i, Ordering::Relaxed);
        Some(i)
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
                if let Ok(i) = self.entries.binary_search_by_key(&id, |e| e.id)
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
    /// A thread is making the text of the photos the catalog had when it started; `edited` are the ones changed since.
    Running {
        worker: JoinHandle<Option<SearchText>>,
        edited: HashSet<PhotoId>,
    },
    /// The thread's text was taken, or the thread failed or its text didn't fit: the search reads each photo's fields.
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
        }
    }
}

impl KeptText {
    /// The text, when it is made. `pass`: a pass over the whole library asks, which starts the text when it is not started: for
    /// a big library on another thread, which this pass doesn't wait for, and for a small one here (a search of one photo asks
    /// without starting anything).
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
            Work::Idle if pass => {
                if photos.len() >= self.from
                    && let Some(worker) = self.spawn(photos)
                {
                    self.running(&mut work, worker);
                    return None;
                }
                // (a small library, or no thread to be had: made where it stands)
                let text = SearchText::of(photos.values().map(|p| &**p));
                self.over(&mut work, text)
            }
            Work::Running { worker, .. } if worker.is_finished() => {
                let Work::Running { worker, edited } = std::mem::replace(&mut *work, Work::Over) else { return None };
                // (a thread that failed has no text: nothing is kept)
                let text = worker.join().ok().flatten().map(|mut text| {
                    for id in edited {
                        text.changed(id, photos.get(&id).map(|p| &**p));
                    }
                    text
                });
                self.over(&mut work, text)
            }
            _ => None,
        }
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
        #[cfg(test)]
        self.spawned.fetch_add(1, Ordering::Relaxed);
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
        *work = Work::Running { worker, edited: HashSet::new() };
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
            Some(text) => text.changed(id, p),
            None => self.edited(id),
        }
    }

    /// Photo `id` is at `p` now, another place in memory than the one its text was made at.
    pub(crate) fn moved(&mut self, id: PhotoId, p: &Photo) {
        match self.made.get_mut() {
            Some(text) => text.moved(id, p),
            None => self.edited(id),
        }
    }

    /// Photo `id` changed while a thread makes the text: the text is made again for it when the thread's is taken.
    fn edited(&mut self, id: PhotoId) {
        if let Ok(Work::Running { edited, .. }) = self.work.get_mut() {
            edited.insert(id);
        }
    }
}

#[cfg(test)]
impl KeptText {
    pub(crate) fn set_from(&mut self, from: usize) {
        self.from = from;
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
