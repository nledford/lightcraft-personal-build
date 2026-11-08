//! The loupe's look-ahead: stand-ins and thumbnails for the photos it is likely to be stepped to
//! next (PERF-CULL-2).

use super::{LightcraftApp, PhotoId};

/// How many photos ahead (in the direction of travel) the loupe makes its stand-in for, and how
/// many behind (a step back is common when culling).
const AHEAD: usize = 5;
pub(super) const BEHIND: usize = 1;
/// The look-ahead's priority: below the loupe's own render (100), above everything else.
const AHEAD_PRIORITY: u32 = 96;
/// How many photos on either side of the open one keep their thumbnails when the loupe trims them
/// (what the filmstrip can show): the nearest of them, as many as the thumbnails' share of the
/// memory budget holds ([`keep_window`]). It trims once the thumbnails are over what the grid keeps
/// them to, see [`crate::render::Renderer::thumbs_over_limit`].
const KEEP_AROUND: usize = 64;
/// The thumbnail size the look-ahead asks for (the filmstrip's at 2x).
const AHEAD_THUMB_EDGE: usize = 256;

/// What [`look_ahead`] last worked out (see `LightcraftApp::look_ahead`).
#[derive(Debug, Default)]
pub struct LookAhead {
    /// The last photo the loupe showed, for the direction of travel.
    last: Option<PhotoId>,
    /// +1 forward (the default), -1 backward.
    dir: i8,
    /// (photo, direction, catalog revision, visible list generation, edge, crop, which photos) the
    /// requests were made for: a new source, filter or sort changes the neighbours without an edit,
    /// and a change of the preparation settings what may be made.
    made_for: Option<(PhotoId, i8, u64, u64, usize, bool, Prepare, u64)>,
    /// Where a photo was last found in the visible list, and in which generation of it: a frame
    /// asks again for the same photo of the same list (the filmstrip, the look-ahead, the
    /// prefetch), and a scan of the whole view each time is what a big library can't afford.
    found: Option<(u64, PhotoId, usize)>,
}

/// Which photos the look-ahead may prepare, from Settings ▸ Performance for what the user is doing
/// ([`lightcraft_engine::prep::Mode`]): none when its "Photos ahead" is 0 or it prepares only the photo
/// on screen, else the photos of its "which photos".
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Prepare {
    Nothing,
    Photos(lightcraft_engine::prep::Scope),
}

impl Prepare {
    /// For `ahead` photos ahead (the mode's count, 0 for the photo on screen only) of `scope`.
    pub(crate) fn of(ahead: usize, scope: lightcraft_engine::prep::Scope) -> Prepare {
        if ahead == 0 || scope == lightcraft_engine::prep::Scope::OnScreen { Prepare::Nothing } else { Prepare::Photos(scope) }
    }
}

impl LookAhead {
    /// The position of `id` in `ids` (the visible list of generation `generation`). Asked again
    /// for the same photo of the same list it answers from memory; for a list that changed it looks
    /// near the last place found first, then everywhere.
    pub fn position(&mut self, ids: &[PhotoId], generation: u64, id: PhotoId) -> Option<usize> {
        const NEAR: usize = 256;
        let hint = match self.found {
            Some((g, p, at)) if p == id && g == generation => return Some(at),
            // (the photo asked for is near the last one found: a step moves one place)
            Some((_, _, at)) => Some(at),
            None => None,
        };
        let near = hint.and_then(|at| {
            let from = at.saturating_sub(NEAR);
            let to = at.saturating_add(NEAR).min(ids.len());
            ids.get(from..to)?.iter().position(|q| *q == id).map(|k| from + k)
        });
        let at = near.or_else(|| ids.iter().position(|q| *q == id))?;
        self.found = Some((generation, id, at));
        Some(at)
    }

    /// The photos to prepare around `id` at position `i` of `ids`: [`AHEAD`] in the direction of
    /// travel, nearest first, then [`BEHIND`] the other way, of those `prepare` allows. (The
    /// stand-ins are cheap: as many as [`AHEAD`] whatever the mode's count, which is for the full
    /// renders made ahead.)
    fn targets(&self, app: &LightcraftApp, ids: &[PhotoId], i: usize, prepare: Prepare) -> Vec<PhotoId> {
        let Prepare::Photos(scope) = prepare else { return Vec::new() };
        let mut targets = self.targets_n(ids, i, AHEAD, BEHIND);
        if scope == lightcraft_engine::prep::Scope::Marked {
            targets.retain(|t| lightcraft_engine::prep::marked(&app.session, *t));
        }
        targets
    }

    /// `ahead` photos in the direction of travel from position `i` of `ids`, nearest first, then
    /// `behind` the other way.
    pub fn targets_n(&self, ids: &[PhotoId], i: usize, ahead: usize, behind: usize) -> Vec<PhotoId> {
        self.targets_at(ids, i, ahead, behind).into_iter().map(|(_, id)| id).collect()
    }

    /// [`Self::targets_n`] with each photo's position in `ids`.
    pub fn targets_at(&self, ids: &[PhotoId], i: usize, ahead: usize, behind: usize) -> Vec<(usize, PhotoId)> {
        let dir: isize = if self.dir < 0 { -1 } else { 1 };
        let at = |k: isize| {
            let j = i.checked_add_signed(k)?;
            Some((j, *ids.get(j)?))
        };
        let cap = |n: usize| n.min(ids.len()) as isize;
        let ahead = (1..=cap(ahead)).filter_map(|k| at(dir * k));
        let behind = (1..=cap(behind)).filter_map(|k| at(-dir * k));
        ahead.chain(behind).collect()
    }

    /// Whether the user is stepping backwards.
    pub fn backward(&self) -> bool {
        self.dir < 0
    }
}

/// Culling: make the loupe's stand-ins (the camera JPEG of an unedited raw, or a cached render) of
/// the next photos in the direction of travel, so that a step shows the next one at once instead
/// of after reading and decoding it (see [`Slot::QuickAhead`]).
/// Only the photos `prepare` allows (Settings ▸ Performance): none at all asks for nothing, and drops the
/// stand-ins still queued.
pub(crate) fn look_ahead(app: &mut LightcraftApp, id: PhotoId, edge: usize, crop: bool, prepare: Prepare) {
    let rev = app.session.catalog.revision;
    let (visible_gen, ids) = app.session.visible_shared();
    if app.look_ahead.last != Some(id) {
        // (the old photo first: the answer is remembered, and the new one is found near it)
        let prev = app.look_ahead.last;
        let j = prev.and_then(|prev| app.look_ahead.position(&ids, visible_gen, prev));
        if let (Some(i), Some(j)) = (app.look_ahead.position(&ids, visible_gen, id), j) {
            app.look_ahead.dir = if j > i { -1 } else { 1 };
        }
        app.look_ahead.last = Some(id);
    }
    // before the early return below: the limit moves with the memory budget, and thumbnails arrive
    // while the loupe rests on a photo, so being over it is not a matter of a step
    trim_thumbs(app, &ids, id);
    // (the renderer's epoch: a reset drops the stand-ins made, which are then asked for again)
    let want = (id, app.look_ahead.dir, rev, visible_gen, edge, crop, prepare, app.renderer.epoch());
    if app.look_ahead.made_for == Some(want) {
        return;
    }
    app.look_ahead.made_for = Some(want);
    let Some(i) = app.look_ahead.position(&ids, visible_gen, id) else { return };
    let targets = app.look_ahead.targets(app, &ids, i, prepare);
    // their thumbnails first (a small embedded JPEG each, a few ms): something to show at once
    // even when a step outruns the full-size previews, and in full screen, where no filmstrip
    // asks for them
    for &nid in &targets {
        crate::panels::grid::request_thumb(app, nid, AHEAD_THUMB_EDGE, AHEAD_PRIORITY + 2);
    }
    let jobs = targets.into_iter().filter_map(|nid| app.session.quick_view_job(nid, edge, crop)).collect();
    app.renderer.look_ahead(jobs, AHEAD_PRIORITY);
}

/// Stepping through a whole wedding in the loupe would keep every thumbnail it ever drew: once the
/// thumbnails are over their limit, keep the ones around the open photo `id` (what the filmstrip
/// can show, see [`keep_window`]) and drop the rest.
fn trim_thumbs(app: &mut LightcraftApp, ids: &[PhotoId], id: PhotoId) {
    if !app.renderer.thumbs_over_limit() {
        return;
    }
    let Some(i) = ids.iter().position(|p| *p == id) else { return };
    let keep = keep_window(&app.renderer, ids, i);
    app.renderer.evict_thumbs(&keep);
}

/// The photos around position `i` of `ids` whose thumbnails the loupe keeps: nearest first (the open
/// photo, then one each side of it, and so on outwards), up to [`KEEP_AROUND`] on either side and
/// no more than the thumbnails' share of the memory budget holds, so that a small budget isn't
/// held up by a window of large thumbnails the grid left. The open photo's own always stays.
fn keep_window(renderer: &crate::render::Renderer, ids: &[PhotoId], i: usize) -> std::collections::HashSet<PhotoId> {
    let budget = renderer.thumb_budget();
    let outwards = (1..=KEEP_AROUND).flat_map(|d| [i.checked_sub(d), i.checked_add(d)]);
    let mut keep = std::collections::HashSet::new();
    let mut held = 0usize;
    for at in std::iter::once(Some(i)).chain(outwards).flatten() {
        let Some(&id) = ids.get(at) else { continue };
        held = held.saturating_add(renderer.thumb_bytes_of(id));
        if held > budget && !keep.is_empty() {
            break;
        }
        keep.insert(id);
    }
    keep
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_photo_asked_for_again_is_not_searched_for_again() {
        let ids: Vec<PhotoId> = (0..5000).map(PhotoId).collect();
        let mut la = LookAhead::default();
        assert_eq!(la.position(&ids, 7, PhotoId(4000)), Some(4000));
        // the same photo of the same list: answered from memory, even from a list that doesn't hold it
        assert_eq!(la.position(&[], 7, PhotoId(4000)), Some(4000));
        // a step: found near the old place
        assert_eq!(la.position(&ids, 7, PhotoId(4001)), Some(4001));
        // a list that changed (photos added before it): found again, wherever it moved to
        let moved: Vec<PhotoId> = (10_000..10_900).map(PhotoId).chain(ids.iter().copied()).collect();
        assert_eq!(la.position(&moved, 8, PhotoId(4001)), Some(4901));
        assert_eq!(la.position(&moved, 8, PhotoId(99_999)), None);
    }

    #[test]
    fn targets_say_where_they_are() {
        let ids: Vec<PhotoId> = (0..20).map(PhotoId).collect();
        let la = LookAhead::default();
        assert_eq!(la.targets_at(&ids, 10, 2, 1), vec![(11, PhotoId(11)), (12, PhotoId(12)), (9, PhotoId(9))]);
        assert_eq!(la.targets_n(&ids, 19, 2, 1), vec![PhotoId(18)], "the end of the view");
    }
}
