//! Window regions and panels.

pub mod activity;
pub mod bottombar;
pub mod chips;
pub mod compare;
pub mod crop_overlay;
pub mod denoise;
pub mod detail;
pub mod dialogs;
pub mod edit;
pub mod empty_library;
pub mod faces;
pub mod filterbar;
pub mod grid;
pub mod histogram;
pub mod keymap;
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
pub mod rules_editor;
pub mod rules_move;
pub mod samples;
pub mod second;
pub mod settings;
pub(crate) mod side_keys;
pub mod slide_layout;
pub mod slide_overlays;
pub mod slide_templates;
pub mod slide_titles;
pub mod slideshow;
pub mod strip;
pub mod topbar;
pub mod unnamed;

use egui::{Rect, pos2, vec2};

use crate::LightcraftApp;
use crate::theme::Tokens;

/// Show a side panel the user resizes by dragging its inner edge. `width` (in [`crate::UiState`],
/// so it survives navigation and restarts) is the source of truth: egui's own remembered size is
/// dropped every frame, and a drag of the edge returns the new width to store. The panel never
/// takes more than `available − reserve − MIN_PHOTO_WIDTH` (its minimum permitting), so the photo
/// stays usable; a narrower window shrinks it without forgetting the chosen width.
pub fn resizable_side(
    ui: &mut egui::Ui,
    left: bool,
    id: &str,
    frame: egui::Frame,
    width: f32,
    limits: crate::state::PanelWidth,
    reserve: f32,
    add: impl FnOnce(&mut egui::Ui),
) -> Option<f32> {
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

/// The HUD toast at the bottom centre of the canvas. It fades in, holds while the pointer rests
/// on it, and fades out; it only reads the pointer position and never takes a click.
pub fn toast(app: &mut LightcraftApp, ctx: &egui::Context) {
    use crate::toast::{HOVER_HOLD, toast_alpha};
    let now = app.tick_now();
    let Some(shown) = app.ui.toast.as_mut() else { return };
    // no frames run in an idle app while the pointer rests on it: a hovered toast is not expired
    // before the frame that sees the pointer leave has started its hold
    if now > shown.until && !shown.hovered {
        app.ui.toast = None;
        return;
    }
    let Some(canvas) = app.canvas_rect else {
        shown.hovered = false; // a hidden canvas cannot pin the toast
        return;
    };
    let t = Tokens::get(ctx);
    let painter = ctx.layer_painter(egui::LayerId::new(egui::Order::Foreground, egui::Id::unique("toast")));
    // long messages (a save warning, the web build's notices) wrap within the canvas
    let wrap = (canvas.width() - 80.0).clamp(200.0, 720.0);
    let (background, foreground) = shown.label.map(crate::theme::label_toast_colors).unwrap_or((Tokens::photo_shade(), Tokens::on_photo()));
    let galley = painter.layout(shown.text.clone(), t.font(14.0), foreground, wrap);
    let size = vec2(galley.size().x + 40.0, (galley.size().y + 22.0).max(40.0));
    // a slideshow's playback controls sit at the bottom: the toast goes to the top there, clear of them
    let y = if app.ui.slideshow.is_some() { canvas.top() + 60.0 } else { canvas.bottom() - 60.0 };
    let r = Rect::from_center_size(pos2(canvas.center().x, y), size);
    // while the pointer rests on it, the clock stops; the first frame after it leaves starts the 1.5 s hold
    let hovered = ctx.pointer_hover_pos().is_some_and(|p| r.contains(p));
    if hovered {
        shown.until = shown.until.max(now + HOVER_HOLD);
    } else if shown.hovered {
        shown.until = now + HOVER_HOLD;
    }
    shown.hovered = hovered;
    let alpha = toast_alpha(now, shown.shown_at, shown.until);
    painter.rect_filled(r, 6.0, background.gamma_multiply(alpha));
    painter.galley_with_override_text_color(r.center() - galley.size() / 2.0, galley, foreground.gamma_multiply(alpha));
    // a screen reader reads it out when it comes, without leaving what it was reading
    let words = shown.text.clone();
    egui::Area::new(egui::Id::unique("toast-words")).order(egui::Order::Foreground).fixed_pos(r.min).interactable(false).show(ctx, |ui| {
        let resp = ui.interact(r, ui.scope_id().with("toast"), egui::Sense::hover());
        resp.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Label, true, &words));
        ctx.accesskit_node_builder(resp.id, |node| node.set_live(egui::accesskit::Live::Polite));
    });
    // a toast held under the pointer is static once faded in: no timer, the pointer leaving wakes the app
    if crate::toast::needs_repaint(hovered, now, shown.shown_at) {
        ctx.request_repaint_after(std::time::Duration::from_millis(30));
    }
}

/// Paint a centred, dimmed message (empty states).
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
    p.galley(pos2(rect.center().x, top), title, t.text);
    p.galley(pos2(rect.center().x, below), body, t.text_dim);
}

/// A button under an empty state's message: the command it runs, its label, and the id it is
/// registered under (`button:…`) for scripts and tests.
pub struct EmptyAction {
    pub command: &'static str,
    pub label: &'static str,
    pub widget: &'static str,
}

/// One dim line, centred: a panel's quiet empty state beside a louder one elsewhere (the Tools
/// panel next to the empty library's welcome), so the window has one headline.
pub fn empty_note(ui: &egui::Ui, rect: Rect, text: &str) {
    let t = Tokens::get(ui.ctx());
    let mut job = egui::text::LayoutJob::simple(crate::i18n::tr(text).to_string(), t.font(13.0), t.text_dim, (rect.width() - 32.0).max(60.0));
    job.halign = egui::Align::Center;
    let g = ui.painter().layout_job(job);
    ui.painter().galley(pos2(rect.center().x, rect.center().y - g.size().y / 2.0), g, t.text_dim);
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

/// [`empty_message`] with the next step under it: a row of buttons (the first is the primary
/// one, filled with the accent), an optional dimmed hint line below them, and `extra` points of
/// room below that for more (0 for none). The block is centred in `rect` as one. Returns the
/// command of the button clicked this frame, and the room left for the extra (as wide as the
/// text may run).
pub fn empty_state(
    ui: &mut egui::Ui,
    rect: Rect,
    title: &str,
    body: &str,
    actions: &[EmptyAction],
    hint: Option<&str>,
    extra: f32,
) -> (Option<&'static str>, Rect) {
    let t = Tokens::get(ui.ctx());
    let wrap = (rect.width() - 48.0).max(80.0);
    let centred = |ui: &egui::Ui, text: &str, font: egui::FontId, color: egui::Color32| {
        let w = balanced_wrap(ui, text, &font, wrap);
        let mut job = egui::text::LayoutJob::simple(text.to_string(), font, color, w);
        job.halign = egui::Align::Center;
        ui.painter().layout_job(job)
    };
    // the title in the brightest text: the first thing read, above the window's own headings
    let title = centred(ui, crate::i18n::tr(title), t.semibold(18.0), t.text);
    let body = centred(ui, crate::i18n::tr(body), t.font(13.0), t.text_dim);
    let hint = hint.map(|h| centred(ui, h, t.font(12.0), t.text_dim));
    // the buttons' widths, and whether they fit side by side (else one above the other)
    const BUTTON_H: f32 = EMPTY_BUTTON_H;
    const GAP: f32 = EMPTY_BUTTON_GAP;
    let widths: Vec<f32> = actions.iter().enumerate().map(|(i, a)| empty_button(i, a).desired_size(ui).x).collect();
    let row_w = widths.iter().sum::<f32>() + GAP * widths.len().saturating_sub(1) as f32;
    let side_by_side = row_w <= wrap;
    let buttons_h = match (actions.len(), side_by_side) {
        (0, _) => 0.0,
        (_, true) => BUTTON_H,
        (n, false) => n as f32 * BUTTON_H + (n - 1) as f32 * GAP,
    };
    let gap_above_buttons = if actions.is_empty() { 0.0 } else { EMPTY_BUTTONS_GAP };
    let hint_h = hint.as_ref().map_or(0.0, |g| EMPTY_HINT_GAP + g.size().y);
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
        let resp = empty_button(i, a).show_at(ui, r);
        if resp.clicked() {
            clicked = Some(a.command);
        }
    }
    if let Some(g) = hint {
        p.galley(pos2(rect.center().x, buttons_top + buttons_h + EMPTY_HINT_GAP), g, t.text_dim);
    }
    let extra_top = buttons_top + buttons_h + hint_h + EMPTY_EXTRA_GAP;
    (clicked, Rect::from_min_size(pos2(rect.center().x - wrap / 2.0, extra_top), vec2(wrap, extra.max(0.0))))
}
