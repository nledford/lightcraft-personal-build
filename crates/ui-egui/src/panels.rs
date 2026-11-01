//! Window regions and panels.

pub mod activity;
pub mod bottombar;
pub mod chips;
pub mod compare;
pub mod crop_overlay;
pub mod denoise;
pub mod detail;
pub mod edit;
pub mod empty_library;
pub mod filterbar;
pub mod grid;
pub mod histogram;
pub mod keyword_list;
pub mod keywording;
pub mod layout_overlay;
pub mod left;
pub mod library_problem;
pub mod mask_effects;
pub mod masking;
pub mod notices;
pub mod people;
pub mod person;
pub mod presets;
pub mod profiles;
pub mod right;
pub mod samples;
pub mod second;
pub(crate) mod side_keys;
pub mod side_select;
pub mod slideshow;
pub mod strip;
pub mod topbar;
pub mod trash_files;
pub mod unnamed;

use egui::{Rect, pos2, vec2};

use crate::LightcraftApp;
pub use crate::dialogs;
pub use crate::dialogs::faces;
pub use crate::dialogs::keymap;
pub use crate::dialogs::rules_editor;
pub use crate::dialogs::rules_editor::rules_move;
pub use crate::dialogs::rules_editor::rules_reorder;
pub use crate::dialogs::settings;
pub use crate::dialogs::settings::slide_layout;
pub use crate::dialogs::settings::slide_overlays;
pub use crate::dialogs::settings::slide_templates;
pub use crate::dialogs::settings::slide_titles;
use crate::theme::Tokens;

/// A side panel the user resizes: which side, how it is drawn and how wide it may be.
pub struct SidePanel<'a> {
    /// the left side (else the right)
    pub left: bool,
    pub id: &'a str,
    pub frame: egui::Frame,
    /// the width the user chose
    pub width: f32,
    pub limits: crate::state::PanelWidth,
    /// room to leave for what is beside the photo
    pub reserve: f32,
}

/// Show a side panel the user resizes by dragging its inner edge. `width` (in [`crate::UiState`],
/// so it survives navigation and restarts) is the source of truth: egui's own remembered size is
/// dropped every frame, and a drag of the edge returns the new width to store. The panel never
/// takes more than `available − reserve − MIN_PHOTO_WIDTH` (its minimum permitting), so the photo
/// stays usable; a narrower window shrinks it without forgetting the chosen width.
pub fn resizable_side(ui: &mut egui::Ui, side: SidePanel, add: impl FnOnce(&mut egui::Ui)) -> Option<f32> {
    let SidePanel { left, id, frame, width, limits, reserve } = side;
    // `Panel::new` stores `IdSalt::new(id)`; the panel's persisted key is
    // `parent_ui.scope_id().with_salt(that_salt)`, which is exactly `scope_id().with(id)`.
    // (Passing an already-built `Id` through `Panel::left` would salt it a second time.)
    let pid = ui.scope_id().with(id);
    ui.ctx().data_mut(|d| d.remove::<egui::containers::panel::PanelState>(pid));
    let avail = ui.available_rect_before_wrap();
    let max = (avail.width() - reserve - crate::state::MIN_PHOTO_WIDTH).clamp(limits.min, limits.max);
    let panel = if left { egui::Panel::left(id) } else { egui::Panel::right(id) };
    let r = panel.frame(frame).resizable(true).size_range(limits.min..=max).default_size(limits.clamp(width).min(max)).show(ui, add);
    crate::widgets::register(ui, format!("panel:{id}"), r.response.rect);
    // egui names the edge's drag widget `<panel id>.with("__resize")`; the width is measured from
    // the panel's fixed edge to the pointer (contents wider than the panel don't count)
    if !ui.ctx().is_being_dragged(pid.with("__resize")) {
        return None;
    }
    let x = ui.ctx().pointer_interact_pos()?.x;
    let w = if left { x - avail.left() } else { avail.right() - x };
    Some(w.round().clamp(limits.min, max))
}

/// The space between two toasts on screen.
const TOAST_GAP: f32 = 8.0;

/// A toast that offers Undo: the gap between its text and the button, and the button's height.
const TOAST_UNDO_GAP: f32 = 16.0;
const TOAST_UNDO_HEIGHT: f32 = 26.0;

/// The HUD toasts at the bottom centre of the canvas: the newest where a toast always was, the older ones above it,
/// each fading in, holding while the pointer rests on it, and fading out at its own time (widgets `toast:0` for the
/// newest, `toast:1` above it…); it only reads the pointer position, and takes a click only on the Undo button of the
/// newest toast when that offers it (an action just done: a drop that moved, added or tagged photos,
/// [`LightcraftApp::toast_undoable`]).
pub fn toast(app: &mut LightcraftApp, ctx: &egui::Context) {
    use crate::toast::{HOVER_HOLD, toast_alpha};
    let now = app.tick_now();
    // (no frames run in an idle app while the pointer rests on one: a hovered toast is not expired before the frame
    // that sees the pointer leave has started its hold)
    app.ui.expire_toasts(now);
    if app.ui.toasts.is_empty() {
        app.ui.toast_undo = None;
        return;
    }
    let undo = app.toast_offers_undo();
    let Some(canvas) = app.canvas_rect else {
        // a hidden canvas cannot pin a toast
        for shown in &mut app.ui.toasts {
            shown.hovered = false;
        }
        return;
    };
    let t = Tokens::get(ctx);
    let painter = ctx.layer_painter(egui::LayerId::new(egui::Order::Foreground, egui::Id::unique("toast")));
    let button_w = if undo { painter.layout_no_wrap(crate::i18n::tr("Undo").to_string(), t.semibold(13.0), t.text).size().x + 28.0 } else { 0.0 };
    // long messages (a save warning, the web build's notices) wrap within the canvas
    // a slideshow's playback controls sit at the bottom: the toasts go to the top there, clear of them, the newest
    // on top; elsewhere the newest is at the bottom and the older ones stack above it
    let top = app.ui.slideshow.is_some();
    // the edge the next one goes against; the newest has none: it is centred where the single toast always was
    let mut edge: Option<f32> = None;
    let mut repaint = false;
    // the widget ids count the toasts drawn, so `toast:1` is always the one above `toast:0` even when one between is skipped
    let mut drawn = 0;
    // the Undo button's place, its opacity and its colors, when the newest toast offers it
    let mut undo_at: Option<(Rect, f32, egui::Color32)> = None;
    let x = canvas.center().x;
    for (i, shown) in app.ui.toasts.iter_mut().rev().enumerate() {
        let offers = undo && i == 0;
        let (background, foreground) =
            shown.label.map(crate::theme::label_toast_colors).unwrap_or((crate::transparency::hud(ctx, 200), Tokens::on_photo()));
        let wrap = (canvas.width() - 80.0 - if offers { button_w + TOAST_UNDO_GAP } else { 0.0 }).clamp(200.0, 720.0);
        let galley = painter.layout(shown.text.clone(), t.font(14.0), foreground, wrap);
        let extra = if offers { button_w + TOAST_UNDO_GAP - 8.0 } else { 0.0 };
        let size = vec2(galley.size().x + 40.0 + extra, (galley.size().y + 22.0).max(40.0));
        let r = match (edge, top) {
            (None, true) => Rect::from_center_size(pos2(x, canvas.top() + 60.0), size),
            (None, false) => Rect::from_center_size(pos2(x, canvas.bottom() - 60.0), size),
            (Some(y), true) => Rect::from_min_size(pos2(x - size.x / 2.0, y), size),
            (Some(y), false) => Rect::from_min_size(pos2(x - size.x / 2.0, y - size.y), size),
        };
        // the older ones never go past the canvas: a burst of long messages keeps those that fit (the newest is drawn
        // as the single toast always was, whatever room there is); one too tall is skipped, and a shorter older one may
        // still fit
        if i > 0 && !canvas.contains_rect(r) {
            shown.hovered = false;
            continue;
        }
        edge = Some(if top { r.bottom() + TOAST_GAP } else { r.top() - TOAST_GAP });
        // while the pointer rests on it, the clock stops; the first frame after it leaves starts the 1.5 s hold,
        // unless what the toast had left (an error's 6 s the pointer only crossed) is longer
        let hovered = ctx.pointer_hover_pos().is_some_and(|p| r.contains(p));
        if hovered || shown.hovered {
            shown.until = shown.until.max(now + HOVER_HOLD);
        }
        shown.hovered = hovered;
        let alpha = toast_alpha(now, shown.shown_at, shown.until);
        painter.rect_filled(r, 6.0, background.gamma_multiply(alpha));
        let text_at = if offers { pos2(r.left() + 20.0, r.center().y - galley.size().y / 2.0) } else { r.center() - galley.size() / 2.0 };
        painter.galley_with_override_text_color(text_at, galley, foreground.gamma_multiply(alpha));
        crate::widgets::register_geometry(ctx, format!("toast:{drawn}"), r);
        // a screen reader reads it out when it comes, without leaving what it was reading
        let words = shown.text.clone();
        egui::Area::new(egui::Id::unique(("toast-words", drawn))).order(egui::Order::Foreground).fixed_pos(r.min).interactable(false).show(
            ctx,
            |ui| {
                let resp = ui.interact(r, ui.scope_id().with("toast"), egui::Sense::hover());
                resp.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Label, true, &words));
                ctx.accesskit_node_builder(resp.id, |node| node.set_live(egui::accesskit::Live::Polite));
            },
        );
        drawn += 1;
        if offers {
            undo_at = Some((r, alpha, foreground));
        }
        // a toast held under the pointer is static once faded in: no timer, the pointer leaving wakes the app
        repaint |= crate::toast::needs_repaint(hovered, now, shown.shown_at);
    }
    if repaint {
        ctx.request_repaint_after(std::time::Duration::from_millis(30));
    }
    if let Some((r, alpha, foreground)) = undo_at {
        let button =
            Rect::from_min_size(pos2(r.right() - 12.0 - button_w, r.center().y - TOAST_UNDO_HEIGHT / 2.0), vec2(button_w, TOAST_UNDO_HEIGHT));
        // (above the toast's own layer, whose backdrop would otherwise dim it)
        let clicked = egui::Area::new(egui::Id::unique("toast-undo"))
            .order(egui::Order::Tooltip)
            .fixed_pos(button.min)
            .show(ctx, |ui| {
                ui.set_opacity(alpha);
                // drawn in the toast's own colors (it is dark whatever the theme): a theme's button fills
                // would not read on it
                let resp = ui.interact(button, egui::Id::unique("toast-undo-button"), egui::Sense::click());
                resp.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, crate::i18n::tr("Undo")));
                crate::widgets::register_response("button:toastUndo", button, &resp);
                if resp.hovered() {
                    ctx.set_cursor_icon(egui::CursorIcon::PointingHand);
                }
                let wash = if resp.is_pointer_button_down_on() {
                    0.3
                } else if resp.hovered() {
                    0.16
                } else {
                    0.0
                };
                let p = ui.painter();
                p.rect_filled(button, 4.0, foreground.gamma_multiply(wash));
                p.rect_stroke(button, 4.0, egui::Stroke::new(1.0, foreground.gamma_multiply(0.6)), egui::StrokeKind::Inside);
                if resp.has_focus() {
                    p.rect_stroke(button.expand(2.0), 6.0, egui::Stroke::new(2.0, t.accent), egui::StrokeKind::Outside);
                }
                p.text(button.center(), egui::Align2::CENTER_CENTER, crate::i18n::tr("Undo"), t.semibold(13.0), foreground);
                resp.clicked()
            })
            .inner;
        if clicked {
            app.ui.toast_undo = None;
            match app.run("edit.undo", serde_json::json!({})) {
                Ok(_) => app.toast(ctx, crate::i18n::tr("Undo")),
                Err(e) => app.toast_error(ctx, e),
            }
        }
    }
}

/// Paint a centred, dimmed message (empty states). Each line wraps to the area, so a long one (a
/// translation, an album named in it) is not cut off at its edges.
pub fn empty_message(ui: &egui::Ui, rect: Rect, title: &str, body: &str) {
    let t = Tokens::get(ui.ctx());
    let p = ui.painter();
    // Both wrap inside the view (a sentence used to run out of a narrow one), centred line by
    // line, the two as one block about the middle.
    let wrap = (rect.width() - 48.0).max(80.0);
    let centred = |text: &str, font: egui::FontId, color: egui::Color32| {
        let mut job = egui::text::LayoutJob::simple(text.to_string(), font, color, wrap);
        job.halign = egui::Align::Center;
        p.layout_job(job)
    };
    let title = centred(crate::i18n::tr(title), t.display_strong(), t.text);
    let body = centred(crate::i18n::tr(body), t.body(), t.text_dim);
    let top = rect.center().y - (title.size().y + 6.0 + body.size().y) / 2.0;
    let below = top + title.size().y + 6.0;
    let (title_at, body_at) = (pos2(rect.center().x, top), pos2(rect.center().x, below));
    // read by a screen reader, as the welcome's words are
    readable(ui, title.rect.translate(title_at.to_vec2()), "message-title", title.text(), false);
    readable(ui, body.rect.translate(body_at.to_vec2()), "message-body", body.text(), false);
    p.galley(title_at, title, t.text);
    p.galley(body_at, body, t.text_dim);
}

/// A button under an empty state's message: the command it runs, its label, and the id it is
/// registered under (`button:…`) for scripts and tests.
pub struct EmptyAction {
    pub command: &'static str,
    pub label: &'static str,
    pub widget: &'static str,
}

/// Painted text a screen reader can read: a node over `rect` with the words, a heading when
/// `heading` (the welcome's title), plain text otherwise. Hover only: clicks pass through.
pub(crate) fn readable(ui: &egui::Ui, rect: Rect, salt: &str, text: &str, heading: bool) {
    let resp = ui.interact(rect, ui.scope_id().with(("readable", salt)), egui::Sense::hover());
    // egui's way for text: the words as the node's value (as every other label in the app)
    resp.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Label, true, text));
    if heading {
        // a heading is read by its name: the words go there instead, not twice
        ui.ctx().accesskit_node_builder(resp.id, |node| {
            node.set_role(egui::accesskit::Role::Heading);
            node.set_label(text.to_string());
            node.clear_value();
        });
    }
}

/// One dim line, centred: a panel's quiet empty state beside a louder one elsewhere (the Tools
/// panel next to the empty library's welcome), so the window has one headline.
pub fn empty_note(ui: &egui::Ui, rect: Rect, text: &str) {
    let t = Tokens::get(ui.ctx());
    let mut job = egui::text::LayoutJob::simple(crate::i18n::tr(text).to_string(), t.font(13.0), t.text_dim, (rect.width() - 32.0).max(60.0));
    job.halign = egui::Align::Center;
    let g = ui.painter().layout_job(job);
    let at = pos2(rect.center().x, rect.center().y - g.size().y / 2.0);
    readable(ui, g.rect.translate(at.to_vec2()), "empty-note", g.text(), false);
    ui.painter().galley(at, g, t.text_dim);
}

/// The space an empty state leaves under its hint for more of it (the welcome's sample photos):
/// `height` tall, counted when the block is centred.
pub const EMPTY_EXTRA_GAP: f32 = 28.0;

/// An empty state's `i`th action as a button: the first is the primary one.
fn empty_button(i: usize, a: &EmptyAction) -> crate::button::Button<'static> {
    let b = crate::button::Button::new(a.label).widget(a.widget);
    if i == 0 { b.primary() } else { b.secondary() }
}

/// [`empty_state`]'s layout.
const EMPTY_BUTTON_H: f32 = crate::button::Size::Medium.height();
const EMPTY_BUTTON_GAP: f32 = 8.0;
const EMPTY_TITLE_GAP: f32 = 8.0;
/// The widest an empty state's words run: a short paragraph to read, not one line across a wide
/// window.
const EMPTY_TEXT_MAX: f32 = 540.0;

/// The width to wrap an empty state's `text` at: at most `wrap` and [`EMPTY_TEXT_MAX`], and as
/// narrow as it can be without taking another line, so its lines come out even rather than a long
/// one over a short one.
fn balanced_wrap(ui: &egui::Ui, text: &str, font: &egui::FontId, wrap: f32) -> f32 {
    let max = wrap.min(EMPTY_TEXT_MAX);
    let rows = |w: f32| ui.painter().layout(text.to_string(), font.clone(), egui::Color32::WHITE, w).rows.len();
    let n = rows(max);
    if n <= 1 {
        return max;
    }
    // the narrowest width that keeps `n` lines, found to about a point (from nothing up: lines
    // broken by hand can be narrower than an even share)
    let (mut lo, mut hi) = (1.0, max);
    for _ in 0..10 {
        let mid = (lo + hi) / 2.0;
        if rows(mid) > n { lo = mid } else { hi = mid }
    }
    hi
}
const EMPTY_BUTTONS_GAP: f32 = 20.0;
const EMPTY_HINT_GAP: f32 = 12.0;

/// What [`empty_state`] measures of its words and buttons in `wrap` points of width.
struct EmptyLayout {
    title: std::sync::Arc<egui::Galley>,
    body: std::sync::Arc<egui::Galley>,
    hint: Option<std::sync::Arc<egui::Galley>>,
    /// Each button's width, their row's width, and whether they fit side by side (else one above
    /// the other).
    widths: Vec<f32>,
    row_w: f32,
    side_by_side: bool,
    buttons_h: f32,
    gap_above_buttons: f32,
    hint_h: f32,
}

fn empty_layout(ui: &egui::Ui, wrap: f32, title: &str, body: &str, actions: &[EmptyAction], hint: Option<&str>) -> EmptyLayout {
    let t = Tokens::get(ui.ctx());
    let centred = |text: &str, font: egui::FontId, color: egui::Color32| {
        let w = balanced_wrap(ui, text, &font, wrap);
        let mut job = egui::text::LayoutJob::simple(text.to_string(), font, color, w);
        job.halign = egui::Align::Center;
        ui.painter().layout_job(job)
    };
    // the title in the brightest text: the first thing read, above the window's own headings
    let title = centred(crate::i18n::tr(title), t.semibold(18.0), t.text);
    let body = centred(crate::i18n::tr(body), t.font(13.0), t.text_dim);
    let hint = hint.map(|h| centred(h, t.font(12.0), t.text_dim));
    let widths: Vec<f32> = actions.iter().enumerate().map(|(i, a)| empty_button(i, a).desired_size(ui).x).collect();
    let row_w = widths.iter().sum::<f32>() + EMPTY_BUTTON_GAP * widths.len().saturating_sub(1) as f32;
    let side_by_side = row_w <= wrap;
    let buttons_h = match (actions.len(), side_by_side) {
        (0, _) => 0.0,
        (_, true) => EMPTY_BUTTON_H,
        (n, false) => n as f32 * EMPTY_BUTTON_H + (n - 1) as f32 * EMPTY_BUTTON_GAP,
    };
    let gap_above_buttons = if actions.is_empty() { 0.0 } else { EMPTY_BUTTONS_GAP };
    let hint_h = hint.as_ref().map_or(0.0, |g| EMPTY_HINT_GAP + g.size().y);
    EmptyLayout { title, body, hint, widths, row_w, side_by_side, buttons_h, gap_above_buttons, hint_h }
}

/// The height [`empty_state`] takes for this message, buttons and hint in `rect` (without the
/// extra room below): what a caller checks before asking for that room.
pub fn empty_state_height(ui: &egui::Ui, rect: Rect, title: &str, body: &str, actions: &[EmptyAction], hint: Option<&str>) -> f32 {
    let wrap = (rect.width() - 48.0).max(80.0);
    let l = empty_layout(ui, wrap, title, body, actions, hint);
    l.title.size().y + EMPTY_TITLE_GAP + l.body.size().y + l.gap_above_buttons + l.buttons_h + l.hint_h
}

/// What [`empty_state`] gives besides the message and its buttons: `extra` points of room below
/// the hint for more (0 for none), and whether the primary button takes the keyboard focus.
#[derive(Clone, Copy, Default)]
pub struct EmptyMore {
    pub extra: f32,
    pub focus_primary: bool,
}

/// [`empty_message`] with the next step under it: a row of buttons (the first is the primary
/// one, filled with the accent), an optional dimmed hint line below them, and `more.extra` points of
/// room below that for more (0 for none). The block is centred in `rect` as one. Its words are
/// readable by screen readers (the title as a heading), a focused button shows a ring, and
/// `more.focus_primary` gives the primary button the keyboard focus. Returns the command of the button
/// clicked this frame, and the room left for the extra (as wide as the text may run).
pub fn empty_state(
    ui: &mut egui::Ui,
    rect: Rect,
    title: &str,
    body: &str,
    actions: &[EmptyAction],
    hint: Option<&str>,
    more: EmptyMore,
) -> (Option<&'static str>, Rect) {
    let EmptyMore { extra, focus_primary } = more;
    let t = Tokens::get(ui.ctx());
    let wrap = (rect.width() - 48.0).max(80.0);
    let EmptyLayout { title, body, hint, widths, row_w, side_by_side, buttons_h, gap_above_buttons, hint_h } =
        empty_layout(ui, wrap, title, body, actions, hint);
    const BUTTON_H: f32 = EMPTY_BUTTON_H;
    const GAP: f32 = EMPTY_BUTTON_GAP;
    let extra_h = if extra > 0.0 { EMPTY_EXTRA_GAP + extra } else { 0.0 };
    let block = title.size().y + EMPTY_TITLE_GAP + body.size().y + gap_above_buttons + buttons_h + hint_h + extra_h;
    let top = rect.center().y - block / 2.0;
    let p = ui.painter().clone();
    let body_top = top + title.size().y + EMPTY_TITLE_GAP;
    let buttons_top = body_top + body.size().y + gap_above_buttons;
    let title_at = pos2(rect.center().x, top);
    crate::widgets::register(ui, "text:emptyTitle", title.rect.translate(title_at.to_vec2()));
    let body_at = pos2(rect.center().x, body_top);
    crate::widgets::register(ui, "text:emptyBody", body.rect.translate(body_at.to_vec2()));
    readable(ui, title.rect.translate(title_at.to_vec2()), "empty-title", title.text(), true);
    readable(ui, body.rect.translate(body_at.to_vec2()), "empty-body", body.text(), false);
    p.galley(title_at, title, t.text);
    p.galley(body_at, body, t.text_dim);
    let mut clicked = None;
    let mut x = rect.center().x - row_w / 2.0;
    for (i, (a, w)) in actions.iter().zip(&widths).enumerate() {
        let r = if side_by_side {
            let r = Rect::from_min_size(pos2(x, buttons_top), vec2(*w, BUTTON_H));
            x += w + GAP;
            r
        } else {
            let w = widths.iter().copied().fold(0.0, f32::max).min(wrap);
            Rect::from_min_size(pos2(rect.center().x - w / 2.0, buttons_top + i as f32 * (BUTTON_H + GAP)), vec2(w, BUTTON_H))
        };
        // (the button shows a ring where the keyboard is, as the sample thumbnails do)
        let resp = empty_button(i, a).max_width(r.width()).truncate().tooltip(a.label).show_at(ui, r);
        crate::widgets::plain_button(ui.ctx(), resp.id);
        if i == 0 && focus_primary {
            resp.request_focus();
        }
        if resp.clicked() {
            clicked = Some(a.command);
        }
    }
    if let Some(g) = hint {
        let at = pos2(rect.center().x, buttons_top + buttons_h + EMPTY_HINT_GAP);
        readable(ui, g.rect.translate(at.to_vec2()), "empty-hint", g.text(), false);
        p.galley(at, g, t.text_dim);
    }
    let extra_top = buttons_top + buttons_h + hint_h + EMPTY_EXTRA_GAP;
    (clicked, Rect::from_min_size(pos2(rect.center().x - wrap / 2.0, extra_top), vec2(wrap, extra.max(0.0))))
}
