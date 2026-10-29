use super::albums::pointer_over_row;
use super::local::{LocalView, add_local_root_checked, is_builtin_place, same_folder, subfolders};
use super::rows::{INDENT_STEP, Inside, LIBFOLDER_REVEALED, Row, drawn_indent, note_width, out_of_sight, tree_toggle};
use super::{Album, Branch, FolderNode, Icon, LibrarySource, LightcraftApp, Said, Sense, Tokens, TreeState, json, pos2, register, vec2};
use crate::menu_item::MenuItem;

/// How far a Local place is listed below its row: eight levels (each one [`INDENT_STEP`] wide), which also
/// bounds [`folder_tree`]'s recursion whatever is on disk.
const LOCAL_DEPTH: f32 = INDENT_STEP * 8.0;

/// A folder on disk with a disclosure triangle: click browses it, the triangle lists its
/// subfolders. In the tree that holds the folder being browsed (`reveal`), the folders on the
/// way down to it open whenever the browsed folder changes, so it shows highlighted in place.
/// `transient`: a top-level row listed for this session only (its Keep in Local can act).
/// `place`: the top-level folder this tree hangs from. A folder can be listed in more than one
/// tree (Pictures on its own and inside Home): each row opens and closes by itself, and their
/// triangles have ids of their own, else the copy drawn last takes the other's clicks (#1093).
#[allow(clippy::too_many_arguments)]
pub(super) fn folder_tree(
    app: &mut LightcraftApp,
    ui: &mut egui::Ui,
    place: &str,
    name: &str,
    path: &str,
    indent: f32,
    view: &LocalView,
    reveal: bool,
    transient: bool,
    above: Option<Said>,
) {
    let current = view.current;
    // a folder that is a place of its own is drawn twice when the place it lies in is open down
    // to it: the row inside is another row (the open state is keyed by the place a row is drawn in);
    // the places are the ones Local draws (the built-in ones too), and a place taken out of Local
    // with Remove from Local is not among them, so it is not drawn twice
    let also_place = indent > 0.0 && view.places.iter().any(|(_, p)| same_folder(p, path));
    let branch = Branch::local(place, path);
    let sel = current.is_some_and(|c| same_folder(c, path));
    let on_the_way = !sel && current.is_some_and(|c| lightcraft_catalog::query::folder_within(c, path));
    let mut open = app.ui.sidebar_trees.is_open_under(&branch, above);
    if reveal && on_the_way {
        // opened once per browsed folder: collapsing it again afterwards sticks
        let revealed_id = egui::Id::unique(("folder-revealed", place.to_string(), path.to_string()));
        let target = current.map(str::to_string);
        let revealed = ui.data(|d| d.get_temp::<Option<String>>(revealed_id));
        if revealed != Some(target.clone()) {
            if target.is_some() && target == as_opened(ui.ctx()).local {
                // what the library opened with: the tree is as it was left, a folded row stays
                // folded (and this is no reveal to remember: coming back to the folder is one)
                app.ui.sidebar_trees.show_unless_left(&branch, above);
            } else {
                app.ui.sidebar_trees.show(&branch, above);
                ui.data_mut(|d| d.insert_temp(revealed_id, target));
            }
            open = app.ui.sidebar_trees.is_open_under(&branch, above);
        }
    }
    if !out_of_sight(app, ui, &format!("local:{path}"), name, None, indent, false, || sel) {
        // at the tree's own indent, as in Albums and Folders: the triangle sits in the gutter and the
        // icon lines up with the rows above
        let resp = Row::new(&format!("local:{path}"), Icon::Folder, name).selected(sel).indent(indent).show(app, ui).on_hover_text(path);
        let tr = tree_toggle(ui, &mut app.ui.sidebar_trees, &resp, indent, &branch, &mut open, format!("folderToggle:{path}"), Inside::Folds, name);
        if !tr.clicked()
            && super::side_keys::chosen(ui, &resp)
            && let Err(e) = app.run("library.browse", json!({"path": path}))
        {
            app.toast_error(ui.ctx(), e);
        }
        disk_folder_menu(app, &resp, FolderRow::Local { top: indent == 0.0, transient }, path, None, FolderParents { above: None, asked_for: false });
    }
    if open && indent < LOCAL_DEPTH {
        let inside = app.ui.sidebar_trees.inside(&branch, above);
        for (n, p) in subfolders(ui, path).iter() {
            folder_tree(app, ui, place, n, p, indent + INDENT_STEP, view, reveal, false, inside);
        }
    } else if open && !subfolders(ui, path).is_empty() {
        deeper_row(app, ui, name, path, indent, (&branch, above), also_place);
    }
}

/// As deep as one place is listed: says so, and offers the way on, instead of an open folder that seems to
/// hold nothing. `also_place`: the folder is a place of its own, so only its note is drawn.
fn deeper_row(app: &mut LightcraftApp, ui: &mut egui::Ui, name: &str, path: &str, indent: f32, tree: (&Branch, Option<Said>), also_place: bool) {
    if also_place {
        // already a place of its own: say where its folders are
        let t = Tokens::get(ui.ctx());
        let (r, _) = ui.allocate_exact_size(vec2(ui.available_width(), 29.0), Sense::hover());
        register(ui, format!("localDeeperNote:{path}"), r);
        let galley = ui.painter().layout_no_wrap(crate::i18n::tr("Opens as its own place").to_string(), t.font(12.5), t.text_dim);
        // (placed like the Go Deeper row below: through the step the tree is drawn with now)
        let x = 42.0 + drawn_indent(ui, indent + 24.0);
        note_width(ui, x + galley.size().x + 18.0);
        ui.painter().galley(pos2(r.left() + x, r.center().y - galley.size().y / 2.0), galley, t.text_dim);
    } else {
        let more = Row::new(&format!("localDeeper:{path}"), Icon::Plus, crate::i18n::tr("Go Deeper"))
            .indent(indent + 24.0)
            .show(app, ui)
            .on_hover_text(crate::i18n::tr(
                "Folders this deep are not listed here. Click to add this folder to Local as a place of its own, so you can open it further",
            ));
        if super::side_keys::chosen(ui, &more) {
            // the folder is checked on a worker (it can lie on a network share that has stopped answering), then
            // kept; the folder went away (or its drive) while the listed row was still shown: the refusal is said
            let name = name.to_string();
            let (branch, said) = (tree.0.clone(), tree.1);
            add_local_root_checked(app, ui.ctx(), path, move |app, ctx| {
                // (this row is the folder inside another place from now on: it stays open as it was)
                app.ui.sidebar_trees.show(&branch, said);
                app.toast(ctx, crate::i18n::tr_format!("“{name}” is now a place in Local: open it there", name = name));
            });
        }
    }
}

pub(super) fn folder_menu(app: &mut LightcraftApp, resp: &egui::Response, a: &Album) {
    crate::widgets::context_menu(resp, |ui| {
        // one of several albums chosen: what can be done to them all
        let several = crate::panels::side_select::album_targets(app, a.id);
        if several.len() > 1 {
            return crate::panels::side_select::albums_menu(app, ui, &several);
        }
        if a.folder {
            // the same commands as File ▸ New…, aimed at this folder
            MenuItem::new(crate::i18n::tr("New")).submenu(ui, |ui| {
                for (label, command) in [
                    ("Create Album…", "dialog.newAlbum"),
                    ("Create Smart Album…", "dialog.smartAlbum"),
                    ("Create Smart Album from Filter…", "dialog.newSmartAlbum"),
                    ("Create Folder…", "dialog.newFolder"),
                ] {
                    // (a view that can't be saved as rules greys out the one that saves it, and says why)
                    let why_not = if command == "dialog.newSmartAlbum" { app.session.view_save_refusal() } else { None };
                    let item = MenuItem::new(crate::i18n::tr(label)).enabled(why_not.is_none()).show(ui);
                    if let Some(why) = why_not {
                        item.on_disabled_hover_text(why);
                    } else if item.clicked() {
                        app.act(command, json!({"parent": a.id.0}));
                        ui.close();
                    }
                }
            });
            if app.session.catalog.album_children_are_ordered(Some(a.id))
                && MenuItem::new(crate::i18n::tr("Sort Contents A–Z"))
                    .show(ui)
                    .on_hover_text(crate::i18n::tr("Go back to listing them by name"))
                    .clicked()
            {
                app.act("album.sort", json!({"parent": a.id.0}));
                ui.close();
            }
            ui.separator();
        }
        if !a.folder && !a.is_smart() {
            let local_only = crate::panels::grid::selection_local_only(app);
            let hint = crate::panels::grid::albums_hint();
            let b = MenuItem::new(crate::i18n::tr("Add Selected Photos")).enabled(!local_only).disabled_reason(&hint).show(ui);
            if b.clicked() {
                match app.run("album.addPhotos", json!({"id": a.id.0})) {
                    Ok(r) => crate::panels::grid::toast_skipped(app, ui.ctx(), &r),
                    Err(e) => app.toast_error(ui.ctx(), e),
                }
            }
            // File ▸ Import's ways in; the import review opens with this album chosen
            MenuItem::new(crate::i18n::tr("Import")).widget("button:albumImport").submenu(ui, |ui| {
                let mut entry = |ui: &mut egui::Ui, label: String, widget: String, command: &str, params: serde_json::Value| {
                    let r = MenuItem::new(&label).widget(widget).show(ui);
                    if r.clicked() {
                        if let Err(e) = app.run(command, params) {
                            app.toast_error(ui.ctx(), e);
                        }
                        ui.close();
                    }
                };
                for (label, command, widget) in [
                    ("Import Photos…", "file.addPhotos", "button:albumImportPhotos"),
                    ("Import from Folder…", "file.addFolder", "button:albumImportFolder"),
                ] {
                    entry(ui, crate::i18n::tr(label).into(), widget.into(), command, json!({"album": a.id.0}));
                }
                // like File ▸ Import from Device: one entry per camera or card found
                MenuItem::new(crate::i18n::tr("Import from Device")).widget("button:albumImportDevice").submenu(ui, |ui| {
                    let devices = lightcraft_engine::devices::devices();
                    if devices.is_empty() {
                        MenuItem::new(crate::i18n::tr("No Camera or Card Found")).enabled(false).show(ui);
                    }
                    for (i, d) in devices.into_iter().enumerate() {
                        let params = json!({"path": d.path, "album": a.id.0});
                        entry(ui, format!("{}…", d.name), format!("button:albumImportDevice:{i}"), "file.addFromDevice", params);
                    }
                });
            });
        }
        if !a.folder && !a.is_smart() {
            // looked up, as the row's "+" is: a target that is gone means the Quick Collection
            let target = app.session.target_album.filter(|t| app.session.catalog.album(*t).is_some());
            let is_target = target == Some(a.id) || (target.is_none() && a.quick);
            if !is_target && MenuItem::new(crate::i18n::tr("Set as Target Album (B adds to it)")).show(ui).clicked() {
                app.act("album.setTarget", json!({"id": if a.quick { serde_json::Value::Null } else { json!(a.id.0) }}));
            }
            if is_target && !a.quick && MenuItem::new(crate::i18n::tr("Stop Using as Target Album")).show(ui).clicked() {
                app.act("album.setTarget", json!({"id": null}));
            }
        }
        if a.quick && MenuItem::new(crate::i18n::tr("Clear Quick Collection")).show(ui).clicked() {
            app.act("album.clearQuick", json!({}));
        }
        if a.is_smart() && MenuItem::new(crate::i18n::tr("Edit Smart Album…")).show(ui).clicked() {
            // the same editor as `dialog.smartAlbum` (older smart albums keep their filter fields;
            // it works on the rule set, upgraded)
            app.act("dialog.smartAlbum", json!({"id": a.id.0}));
        }
        if a.is_smart() && MenuItem::new(crate::i18n::tr("Update Rules from Current Filter")).show(ui).clicked() {
            // refused when the view would make the album test itself: say why
            if let Err(e) = app.run("album.setRules", json!({"id": a.id.0, "fromView": true})) {
                app.toast_error(ui.ctx(), e);
            }
        }
        if !a.folder {
            // export: show the album, select its photos, then the dialog / a preset
            let show_all = |app: &mut LightcraftApp| {
                app.act("library.source", json!({"kind": "album", "id": a.id.0}));
                app.act("library.selectAll", json!({}));
            };
            let has_photos = app.session.catalog.album_count(a.id) > 0;
            if MenuItem::new(crate::i18n::tr("Export Album…")).enabled(has_photos).show(ui).clicked() {
                show_all(app);
                app.act("dialog.export", json!({}));
            }
            ui.add_enabled_ui(has_photos, |ui| {
                MenuItem::new(crate::i18n::tr("Export Album with Preset")).submenu(ui, |ui| {
                    for (p, _) in app.session.all_export_presets() {
                        if MenuItem::new(&p.name).show(ui).clicked() {
                            show_all(app);
                            if let Err(e) = app.run("app.export", json!({"preset": p.name, "background": true})) {
                                app.toast_error(ui.ctx(), e);
                            }
                        }
                    }
                });
            });
            ui.separator();
        }
        // move into another folder (not into itself or one of its own subfolders, nor into one
        // a smart album that moves with it shows)
        let cat = &app.session.catalog;
        let mut folders: Vec<(u64, String)> = cat
            .albums()
            .filter(|f| f.folder && !is_within(app, f.id, a.id) && cat.album_move_would_loop(a.id, f.id).is_none())
            .map(|f| (f.id.0, f.name.clone()))
            .collect();
        folders.sort_by_key(|(_, n)| n.to_lowercase());
        MenuItem::new(crate::i18n::tr("Move to")).submenu(ui, |ui| {
            if MenuItem::new(crate::i18n::tr("Top Level")).enabled(a.parent.is_some()).show(ui).clicked() {
                app.act("album.move", json!({"id": a.id.0, "parent": null}));
            }
            for (fid, name) in &folders {
                if MenuItem::new(name).enabled(a.parent.map(|p| p.0) != Some(*fid)).show(ui).clicked() {
                    app.act("album.move", json!({"id": a.id.0, "parent": fid}));
                }
            }
        });
        crate::panels::side_select::album_clip_items(app, ui, &[a.id.0]);
        if a.folder {
            crate::panels::side_select::album_paste_item(app, ui, Some(a.id));
        }
        if MenuItem::new(crate::i18n::tr("Rename…")).show(ui).clicked() {
            app.ui.dialog = Some(crate::state::Dialog::RenameAlbum { id: a.id.0, name: a.name.clone() });
        }
        let delete = MenuItem::new(crate::i18n::tr("Delete")).widget(format!("albumDelete:{}", a.id.0)).show(ui);
        let delete =
            delete.on_hover_text(crate::i18n::tr("Photos stay in All Photos. Deleting a folder also deletes its albums. You can undo this."));
        if delete.clicked() {
            if let Err(e) = app.run("album.delete", json!({"id": a.id.0})) {
                app.toast_error(ui.ctx(), e);
            }
            ui.close();
        }
    });
}

/// Whether `id` is `ancestor` or lies inside it.
pub(super) fn is_within(app: &LightcraftApp, id: lightcraft_catalog::AlbumId, ancestor: lightcraft_catalog::AlbumId) -> bool {
    let mut cur = Some(id);
    let mut guard = 0;
    while let Some(c) = cur {
        if c == ancestor {
            return true;
        }
        cur = app.session.catalog.album(c).and_then(|x| x.parent);
        guard += 1;
        if guard > 64 {
            break;
        }
    }
    false
}

// "Folders" (drawn in `show`): where on disk the library's photos were imported from, with photo counts (see
// `lightcraft_catalog::folders`). A click makes that folder the source, like an album or a Local
// folder: its photos and those of the folders inside it fill the grid. The triangle opens a
// level. Only folders holding imported photos are listed; every folder on disk is under Local.

/// What the library had the grid show when it was opened (as the app starts, or by Open
/// Library): a Local folder, a folder of the library, each for as long as the grid still shows
/// it. That is no choice of now: the trees are as they were left, so showing where the grid is
/// opens only the rows nobody left folded ([`TreeState::show_unless_left`]). Once what is shown
/// changes, the rows above it open whatever was said of them, as they always have.
#[derive(Clone, Default, PartialEq)]
struct AsOpened {
    /// Which opening of a library this is of ([`lightcraft_engine::Session::library_generation`]).
    library: u64,
    local: Option<String>,
    folder: Option<String>,
}

const AS_OPENED: &str = "sidebar-shown-as-opened";

/// Follows what the grid shows, every frame and whether the sidebar is drawn or not, so that a
/// folder chosen while it is hidden is known for a choice when it is shown again.
pub(crate) fn follow_shown(app: &LightcraftApp, ctx: &egui::Context) {
    let library = app.session.library_generation();
    let local = app.session.browse.as_ref().filter(|_| app.session.source == LibrarySource::Folder).map(|b| b.path.clone());
    let folder = app.session.library_folder.clone().filter(|_| app.session.source == LibrarySource::LibraryFolder);
    let id = egui::Id::unique(AS_OPENED);
    let was = ctx.data(|d| d.get_temp::<AsOpened>(id));
    let now = match &was {
        // (what is no longer shown was left: coming back to it is a choice)
        Some(was) if was.library == library => AsOpened {
            library,
            local: was.local.clone().filter(|w| Some(w) == local.as_ref()),
            folder: was.folder.clone().filter(|w| Some(w) == folder.as_ref()),
        },
        _ => AsOpened { library, local, folder },
    };
    if was.as_ref() != Some(&now) {
        ctx.data_mut(|d| d.insert_temp(id, now));
    }
}

fn as_opened(ctx: &egui::Context) -> AsOpened {
    ctx.data(|d| d.get_temp::<AsOpened>(egui::Id::unique(AS_OPENED))).unwrap_or_default()
}

/// Whenever the shown folder changes (a click, an agent, a rename or its undo), open the rows
/// above it so it is on screen; folding one by hand afterwards sticks until the choice changes.
/// The folder the library opened with is no change: a row above it that was left folded stays
/// folded, unless `forced` (Show in Folders asked for it: a choice of now).
pub(super) fn reveal_chosen(app: &mut LightcraftApp, ui: &egui::Ui, tree: &[FolderNode], forced: bool) {
    let shown = app.session.library_folder.clone().filter(|_| app.session.source == LibrarySource::LibraryFolder);
    let as_left = !forced && shown.is_some() && shown == as_opened(ui.ctx()).folder;
    let chosen = shown.filter(|c| !lightcraft_catalog::query::folder_key(c).is_empty());
    let seen = egui::Id::unique(LIBFOLDER_REVEALED);
    let now = chosen.as_deref().map(lightcraft_catalog::query::folder_key);
    if ui.data(|d| d.get_temp::<Option<String>>(seen)) == Some(now.clone()) {
        return;
    }
    ui.data_mut(|d| d.insert_temp(seen, now));
    let Some(chosen) = chosen else { return };
    fn open_above(trees: &mut TreeState, nodes: &[FolderNode], chosen: &str, as_left: bool, above: Option<Said>) {
        for n in nodes {
            if lightcraft_catalog::query::folder_within(chosen, &n.path) && !same_folder(chosen, &n.path) {
                let branch = folder_branch(n, &lightcraft_catalog::query::folder_key(&n.path));
                if as_left {
                    trees.show_unless_left(&branch, above);
                } else {
                    trees.show(&branch, above);
                }
                let inside = trees.inside(&branch, above);
                open_above(trees, &n.children, chosen, as_left, inside);
            }
        }
    }
    open_above(&mut app.ui.sidebar_trees, tree, &chosen, as_left, None);
}

/// A Folders row in the remembered trees (`key`: its [`lightcraft_catalog::query::folder_key`]).
/// A disk starts open, and so does a folder that holds nothing itself and leads to one folder
/// (`Users` → `me`): the first row where the library branches is what you look for.
fn folder_branch(n: &FolderNode, key: &str) -> Branch {
    Branch::folder(key, n.volume || (n.own == 0 && n.children.len() == 1))
}

/// The Folders row that stands for folder `dir`, however it is spelled. `None` when no row is that
/// folder.
pub(crate) fn folder_row_of<'a>(tree: &'a [FolderNode], dir: &str) -> Option<&'a FolderNode> {
    // only the rows that hold `dir` are opened (the startup disk's holds the other disks' paths
    // too, so more than one may), and the tree is at most `MAX_DEPTH` deep
    tree.iter()
        .filter(|n| lightcraft_catalog::query::folder_within(dir, &n.path))
        .find_map(|n| if same_folder(&n.path, dir) { Some(n) } else { folder_row_of(&n.children, dir) })
}

/// A folder that only leads on: it holds none of the library's photos itself, says nothing of
/// its own (no colour label) and has one folder inside it (`Users` → `me`).
fn only_leads_on(n: &FolderNode) -> bool {
    !n.volume && n.own == 0 && n.children.len() == 1 && n.label.is_none()
}

/// Whether the grid shows the folder at `path` (the Local / library folder source).
fn shown_folder(app: &LightcraftApp, path: &str) -> bool {
    crate::panels::side_select::folder_shown(app, path)
}

/// Where a folder stands in the Folders tree, for Show Parent Folder.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum FolderPlace {
    /// On the way from its disk to the library's first folder there, with nothing drawn above
    /// it: passed over unless asked for.
    Leading,
    /// Such a folder too, but below one that is drawn (named here: asked for, or what the grid
    /// shows): it has its row for as long as that one has.
    Below(String),
    /// A disk, or a folder that holds photos, branches or carries a label, or one below such a folder.
    Listed,
}

/// The node of the folder spelled `key` ([`lightcraft_catalog::query::folder_key`]) and where it
/// stands, as [`folder_rows`] draws it; `None` when the library has no photo in or below such a
/// folder.
pub(crate) fn folder_place<'a>(app: &LightcraftApp, tree: &'a [FolderNode], key: &str) -> Option<(&'a FolderNode, FolderPlace)> {
    fn find<'a>(
        app: &LightcraftApp,
        nodes: &'a [FolderNode],
        key: &str,
        leading: bool,
        below: Option<&'a FolderNode>,
    ) -> Option<(&'a FolderNode, FolderPlace)> {
        for n in nodes {
            let leads = leading && only_leads_on(n);
            let own = lightcraft_catalog::query::folder_key(&n.path);
            if own == key {
                let place = match (leads, below) {
                    (false, _) => FolderPlace::Listed,
                    (true, Some(above)) => FolderPlace::Below(above.path.clone()),
                    (true, None) => FolderPlace::Leading,
                };
                return Some((n, place));
            }
            // (the tree is at most `folders::MAX_DEPTH` deep)
            if lightcraft_catalog::query::folder_within(key, &n.path) {
                // a folder on the way that is drawn brings every one below it with it
                let drawn = leads && (app.ui.folder_parents.contains(&own) || shown_folder(app, &n.path));
                let below = below.or(drawn.then_some(n));
                if let Some(found) = find(app, &n.children, key, n.volume || leads, below) {
                    return Some(found);
                }
            }
        }
        None
    }
    find(app, tree, key, false, None)
}

/// The rows of the Folders tree. `leading`: these folders are reached from their disk through
/// folders that are passed over, so one that only leads on itself is passed over too — its row
/// is left out and what it leads to takes its place — unless it was asked for (Show Parent
/// Folder) or is what the grid shows. Below a row that is drawn, every folder is a row: the tree
/// never shows a folder inside one it is not directly in. `above`: the folder passed over just
/// above these rows, if any. `said`: what holds for these rows from the rows above them
/// ([`TreeState::inside`]).
pub(super) fn folder_rows(
    app: &mut LightcraftApp,
    ui: &mut egui::Ui,
    nodes: &[FolderNode],
    indent: f32,
    leading: bool,
    above: Option<&FolderNode>,
    said: Option<Said>,
) {
    for n in nodes {
        let key = lightcraft_catalog::query::folder_key(&n.path);
        let on_the_way = leading && only_leads_on(n);
        let asked_for = on_the_way && app.ui.folder_parents.contains(&key);
        if on_the_way && !asked_for && !shown_folder(app, &n.path) {
            folder_rows(app, ui, &n.children, indent, true, Some(n), said);
            continue;
        }
        let branch = folder_branch(n, &key);
        let mut open = app.ui.sidebar_trees.is_open_under(&branch, said);
        // a row whose path would cover other disks' photos too only opens and closes
        if n.selectable {
            app.ui.side.folders.saw(n.path.clone());
        }
        let name = if n.selectable { n.name.clone() } else { crate::i18n::tr("This Computer").to_string() };
        let id = format!("libfolder:{}", n.path);
        // (worked out for a row that is drawn, or listed for the keys: not for every row out of sight)
        let sel_of = |app: &LightcraftApp| n.selectable && shown_folder(app, &n.path);
        if !out_of_sight(app, ui, &id, &name, Some(n.count), indent, n.label.is_some(), || sel_of(app)) {
            let sel = sel_of(app);
            folder_row(app, ui, n, &id, &name, indent, &branch, &mut open, FolderParents { above, asked_for }, sel, said);
        }
        if open && !n.children.is_empty() {
            let inside = app.ui.sidebar_trees.inside(&branch, said);
            folder_rows(app, ui, &n.children, indent + INDENT_STEP, n.volume, None, inside);
        }
    }
}

/// One drawn row of [`folder_rows`], shown as `name`; `branch` and `open` are its triangle's.
#[allow(clippy::too_many_arguments)]
fn folder_row(
    app: &mut LightcraftApp,
    ui: &mut egui::Ui,
    n: &FolderNode,
    id: &str,
    name: &str,
    indent: f32,
    branch: &Branch,
    open: &mut bool,
    parents: FolderParents,
    sel: bool,
    said: Option<Said>,
) {
    // the label is said with the name, as it is shown beside it
    let spoken = match n.label {
        Some(l) => format!("{name}, {}", crate::i18n::color_label(&app.session.catalog, l)),
        None => name.to_string(),
    };
    let mark = n.label.map(crate::theme::label_color);
    let resp = Row::new(id, Icon::Folder, name)
        .spoken(&spoken)
        .count(Some(n.count))
        .selected(sel)
        .indent(indent)
        .sense(Sense::click_and_drag())
        .mark(mark)
        .show(app, ui);
    // a folder inside a disk can be picked up (egui starts the drag after a few points of
    // movement, so a click still chooses the row); a disk, or a row covering several, can't be moved
    if resp.drag_started_by(egui::PointerButton::Primary) && n.selectable && !n.volume {
        app.ui.dragging_folder = Some(n.path.clone());
        app.ui.dragging_folder_library = app.session.library_generation();
    }
    folder_drop_target(app, ui, &resp, n, branch, open, said);
    folder_drag_target(app, ui, &resp, n, branch, open, said);
    crate::panels::side_select::outline(app, ui, crate::panels::side_select::SideList::Folders, sel, resp.rect, 8.0);
    let mut toggled = false;
    if !n.children.is_empty() {
        let tr = tree_toggle(
            ui,
            &mut app.ui.sidebar_trees,
            &resp,
            indent,
            branch,
            open,
            format!("libraryFolderToggle:{}", n.path),
            Inside::OpensAndFolds,
            name,
        );
        toggled = tr.clicked();
        // the triangle sits on the row and takes its clicks: the menu opens from it too
        row_menu(app, &tr, n, parents);
    }
    let tip = if n.children.is_empty() { n.path.clone() } else { format!("{} ({})", n.path, crate::i18n::tr("includes the folders inside it")) };
    let resp = resp.on_hover_text(tip);
    if super::side_keys::chosen(ui, &resp) && !toggled {
        if n.selectable {
            // a source like an album or a Local folder: it replaces what the grid showed
            crate::panels::side_select::folder_clicked(app, ui, &n.path);
        } else {
            *open = !*open;
            app.ui.sidebar_trees.set(branch, *open);
        }
    }
    row_menu(app, &resp, n, parents);
}

/// What a Folders row's menu offers about the folders that only lead to it.
#[derive(Clone, Copy)]
struct FolderParents<'a> {
    /// The folder passed over just above the row: Show Parent Folder lists it.
    above: Option<&'a FolderNode>,
    /// The row is itself such a folder, the topmost listed because it was asked for: Hide This
    /// Parent.
    asked_for: bool,
}

/// Show Parent Folder / Hide This Parent of a Folders row (`view.folderParent`; widgets
/// `folderShowParent`, `folderHideParent`): neither when the row has no folder passed over above
/// it and was not asked for itself.
fn folder_parent_items(app: &mut LightcraftApp, ui: &mut egui::Ui, n: &FolderNode, parents: FolderParents) {
    let mut run = None;
    if let Some(above) = parents.above {
        let show = MenuItem::new(crate::i18n::tr("Show Parent Folder"))
            .show(ui)
            .on_hover_text(crate::i18n::tr_format!("Lists “{name}”, the folder this one is in", name = above.name));
        crate::widgets::register(ui, "folderShowParent", show.rect);
        if show.clicked() {
            run = Some(json!({"path": above.path, "show": true}));
        }
    }
    if parents.asked_for {
        // what the grid shows keeps its row whatever is asked: say so instead of doing nothing
        let shown = shown_folder(app, &n.path);
        let hide = MenuItem::new(crate::i18n::tr("Hide This Parent"))
            .enabled(!shown)
            .disabled_reason(crate::i18n::tr("This folder stays listed while the grid shows its photos"))
            .show(ui)
            .on_hover_text(crate::i18n::tr("Hides this row. Nothing on disk or in the library changes"));
        crate::widgets::register(ui, "folderHideParent", hide.rect);
        if hide.clicked() {
            run = Some(json!({"path": n.path, "show": false}));
        }
    }
    if let Some(params) = run {
        app.act("view.folderParent", params);
        ui.close();
    }
}

/// How long photos dragged over a closed Folders row wait before it opens, so a folder inside it
/// can be reached.
const SPRING_OPEN_SECONDS: f64 = 0.7;

/// Whether the photo's file is directly in the folder `folder`.
fn is_in_folder(p: &lightcraft_catalog::Photo, folder: &str) -> bool {
    match &p.source {
        lightcraft_catalog::Source::File { path } => std::path::Path::new(path).parent().is_some_and(|d| same_folder(&d.to_string_lossy(), folder)),
        _ => false,
    }
}

/// How many files a drop of `photos` on `folder` moves (`photo.moveToFolder`): the photos already
/// there stay, and virtual copies of one file move it once.
fn files_moved(photos: &[&std::sync::Arc<lightcraft_catalog::Photo>], folder: &str) -> usize {
    let mut paths = std::collections::HashSet::new();
    for p in photos.iter().filter(|p| !is_in_folder(p, folder)) {
        if let lightcraft_catalog::Source::File { path } = &p.source {
            paths.insert(path.as_str());
        }
    }
    paths.len()
}

/// A Folders row while photos are dragged from the grid: a release moves their files there on
/// disk (`photo.moveToFolder`, one undo step). The row lights up and the dragged label says where
/// they will go, as the same gesture onto an album only adds them to it. Held over a closed row,
/// it opens. Photos that can't go there leave it dark (a row covering several disks; Local photos,
/// not in the library) and a release says why; photos already there say so.
fn folder_drop_target(
    app: &mut LightcraftApp,
    ui: &mut egui::Ui,
    resp: &egui::Response,
    n: &FolderNode,
    branch: &Branch,
    open: &mut bool,
    said: Option<Said>,
) {
    let Some(ids) = app.ui.dragging_photos.clone() else { return };
    // only where the row can be seen: not scrolled out of the sidebar or beneath the pinned header
    if !pointer_over_row(ui, resp.rect) {
        return;
    }
    if spring_open(ui, n, open) {
        // open for the session, not a choice kept (as the folders above the browsed one); a row folded by hand or with the rows above it opens for good
        app.ui.sidebar_trees.show(branch, said);
    }
    // (the label, the confirmation and the toast say which folder when another row has the name)
    let name = crate::destination::folder(&app.caches.folder_tree(&app.session.catalog, app.session.library_generation()), &n.path);
    let photos: Vec<_> = ids.iter().filter_map(|id| app.session.catalog.photo(lightcraft_catalog::PhotoId(*id))).collect();
    let refused = if !n.selectable {
        Some(crate::i18n::tr("This row covers more than one disk: drop on a folder inside it"))
    } else if photos.iter().any(|p| p.local) {
        Some(crate::i18n::tr("Local photos aren't in the library yet: Add to My Photos first, then move them"))
    } else {
        None
    };
    let count = files_moved(&photos, &n.path);
    let all_here = !photos.is_empty() && photos.iter().all(|p| is_in_folder(p, &n.path));
    let released = ui.input(|i| i.pointer.primary_released());
    if let Some(why) = refused {
        crate::panels::grid::refuse_drop(app, ui.ctx(), crate::i18n::tr("Can't be moved here").to_string(), why, released);
        return;
    }
    let hint = if all_here {
        crate::i18n::tr_format!("Already in “{}”", name)
    } else {
        crate::i18n::tr_format!("Move {n} file{} to “{}”", if count == 1 { "" } else { "s" }, name, n = count)
    };
    crate::panels::grid::drag_hint(ui.ctx(), hint);
    if all_here {
        if released {
            app.ui.dragging_photos = None;
        }
        return;
    }
    let t = Tokens::get(ui.ctx());
    ui.painter().rect_stroke(resp.rect.shrink2(vec2(8.0, 1.0)), 4.0, egui::Stroke::new(1.5, t.accent), egui::StrokeKind::Inside);
    if released {
        app.ui.dragging_photos = None;
        // problems the move would stop at (a name taken there, a file offline) are found now and
        // shown in the question, so they aren't met after Move
        let (problems, movable) = match app.run("photo.checkMove", json!({"folder": n.path, "ids": ids})) {
            Ok(r) => move_problems(&r),
            Err(e) => {
                app.toast_error(ui.ctx(), e);
                return;
            }
        };
        // a drop moves files on disk: asked first (as Lightroom Classic does), unless the person
        // said not to (a move with problems asks anyway: it needs a decision); the menu's Move
        // Selected Photos Here is asked for by name and goes at once
        if app.ui.settings.ask_before_move || !problems.is_empty() {
            let library = app.session.library_generation();
            app.ui.dialog = Some(crate::state::Dialog::MovePhotos {
                ids,
                folder: n.path.clone(),
                name,
                files: count,
                dont_ask: false,
                library,
                problems,
                movable,
            });
        } else {
            move_photos_here(app, ui.ctx(), Some(&ids), &n.path);
        }
    }
}

/// Why the Folders row `target` can't take the folder `from` dragged onto it, from what the
/// sidebar knows (no disk is read: the move's own refusals, such as a disk that isn't connected or
/// a name already taken, come back from `folder.move` when the drop runs). `Ok` when a drop
/// there asks to move it.
pub(crate) fn folder_drop_refusal(app: &LightcraftApp, from: &str, target: &FolderNode) -> Result<(), String> {
    let src = std::path::Path::new(from);
    if app.session.library.as_ref().is_some_and(|l| l.dir.starts_with(src) || src.starts_with(&l.dir)) {
        return Err(crate::i18n::tr("The library's own folder can't be moved").to_string());
    }
    if !target.selectable {
        return Err(crate::i18n::tr("This row covers more than one disk: drop on a folder inside it").to_string());
    }
    if lightcraft_catalog::query::folder_within(&target.path, from) {
        return Err(crate::i18n::tr("A folder can't move into itself or a folder inside it").to_string());
    }
    let parent = src.parent().map(|p| p.to_string_lossy().to_string());
    if parent.as_deref().is_some_and(|p| same_folder(p, &target.path)) {
        return Err(crate::i18n::tr_format!("Already in “{}”", folder_name(&target.path, &target.name)));
    }
    Ok(())
}

/// The Folders row being dragged. A drag doesn't outlive the library it was picked up in: when
/// another one opens meanwhile it is given up. Esc gives it up too, before any row acts on a release
/// that came in the same frame.
fn dragged_folder(app: &mut LightcraftApp, ctx: &egui::Context) -> Option<String> {
    let other_library = app.ui.dragging_folder_library != app.session.library_generation();
    if app.ui.dragging_folder.is_some() && (other_library || ctx.input(|i| i.key_pressed(egui::Key::Escape))) {
        app.ui.dragging_folder = None;
    }
    app.ui.dragging_folder.clone()
}

/// A folder's own name (`fallback` when the path has none).
fn folder_name(path: &str, fallback: &str) -> String {
    std::path::Path::new(path).file_name().map_or_else(|| fallback.to_string(), |f| f.to_string_lossy().to_string())
}

/// A Folders row while another Folders row is dragged: a drop moves that folder into this one on
/// disk (`folder.move`, one undo step), asked first unless the person said not to. The row lights
/// up and the dragged label says where the folder will go; a row that can't take it shows the
/// not-allowed cursor with the reason in the label, and a release says it too. Held over a
/// closed row, it opens, so a folder inside it can be reached.
fn folder_drag_target(
    app: &mut LightcraftApp,
    ui: &mut egui::Ui,
    resp: &egui::Response,
    n: &FolderNode,
    branch: &Branch,
    open: &mut bool,
    said: Option<Said>,
) {
    let Some(from) = dragged_folder(app, ui.ctx()) else { return };
    // only where the row can be seen: not scrolled out of the sidebar or beneath the pinned header
    if !pointer_over_row(ui, resp.rect) {
        return;
    }
    // not under the folder's own drag: rows opening there would shift away from the pointer
    if !lightcraft_catalog::query::folder_within(&n.path, &from) && spring_open(ui, n, open) {
        app.ui.sidebar_trees.show(branch, said);
    }
    let released = ui.input(|i| i.pointer.primary_released());
    let name = folder_name(&from, &from);
    if let Err(why) = folder_drop_refusal(app, &from, n) {
        crate::panels::grid::drag_hint(ui.ctx(), why.clone());
        ui.ctx().set_cursor_icon(egui::CursorIcon::NotAllowed);
        if released {
            // letting go over the row it was picked up on (a slow click, a wobble) is no request
            // to move it anywhere: it says nothing
            if !same_folder(&from, &n.path) {
                app.toast_error(ui.ctx(), why);
            }
            app.ui.dragging_folder = None;
        }
        return;
    }
    let into_name = folder_name(&n.path, &n.name);
    crate::panels::grid::drag_hint(ui.ctx(), crate::i18n::tr_format!("Move “{name}” into “{into}”", name = name, into = into_name));
    let t = Tokens::get(ui.ctx());
    ui.painter().rect_stroke(resp.rect.shrink2(vec2(8.0, 1.0)), 4.0, egui::Stroke::new(1.5, t.accent), egui::StrokeKind::Inside);
    if !released {
        return;
    }
    app.ui.dragging_folder = None;
    if app.ui.settings.ask_before_move {
        let tree = app.caches.folder_tree(&app.session.catalog, app.session.library_generation());
        let count = folder_row_of(&tree, &from).map_or(0, |f| f.count);
        app.ui.dialog = Some(crate::state::Dialog::MoveFolder {
            name: lightcraft_catalog::folders::folder_label(&from),
            into_name: lightcraft_catalog::folders::folder_label(&n.path),
            path: from,
            into: n.path.clone(),
            count,
            dont_ask: false,
            library: app.session.library_generation(),
        });
    } else {
        match run_folder_move(app, &from, &n.path) {
            Ok(r) => toast_folder_moved(app, ui.ctx(), &from, &n.path, &r),
            Err(e) => app.toast_error(ui.ctx(), e),
        }
    }
}

/// `folder.move` of `from` into `into`, unless `into` isn't a folder any more (renamed or deleted
/// meanwhile, its disk gone): the move would make it again and go in. Nothing is moved then.
pub(crate) fn run_folder_move(app: &mut LightcraftApp, from: &str, into: &str) -> Result<serde_json::Value, String> {
    if !std::path::Path::new(into).is_dir() {
        return Err(crate::i18n::tr_format!("“{name}” is no longer there: nothing was moved", name = folder_name(into, into)));
    }
    app.run("folder.move", json!({"path": from, "into": into}))
}

/// What a finished folder move (`folder.move`'s result `r`) says.
pub(crate) fn toast_folder_moved(app: &mut LightcraftApp, ctx: &egui::Context, path: &str, into: &str, r: &serde_json::Value) {
    app.toast(ctx, folder_moved_text(&last_name(path), &last_name(into), r["inLibrary"].as_u64().unwrap_or(0)));
}

/// While a Folders row is dragged: its name at the pointer, or what the row under it says (where
/// the folder would go, or why it can't). The drag ends when the button is up (the drop targets
/// act on the release frame, before this runs); Esc gives it up (see [`dragged_folder`]).
pub fn folder_drag_feedback(app: &mut LightcraftApp, ctx: &egui::Context) {
    let Some(from) = dragged_folder(app, ctx) else { return };
    let (released, down, pos) = ctx.input(|i| (i.pointer.primary_released(), i.pointer.primary_down(), i.pointer.latest_pos()));
    if released || !down {
        app.ui.dragging_folder = None;
        return;
    }
    let Some(pos) = pos else { return };
    let frame = ctx.cumulative_frame_nr();
    let hint = ctx.data(|d| d.get_temp::<(u64, String)>(egui::Id::unique("drag-photos-hint"))).filter(|(f, _)| *f == frame).map(|(_, text)| text);
    let text = hint.unwrap_or_else(|| folder_name(&from, &from));
    let t = Tokens::get(ctx);
    // (a row that can't take the folder asked for the not-allowed cursor while it was drawn)
    if ctx.output(|o| o.cursor_icon) != egui::CursorIcon::NotAllowed {
        ctx.set_cursor_icon(egui::CursorIcon::Grabbing);
    }
    egui::Area::new(egui::Id::unique("drag-folder")).order(egui::Order::Tooltip).interactable(false).fixed_pos(pos + vec2(14.0, 10.0)).show(
        ctx,
        |ui| {
            egui::Frame::NONE.fill(t.accent).corner_radius(10.0).inner_margin(egui::Margin::symmetric(9, 3)).show(ui, |ui| {
                let label = ui.add(egui::Label::new(egui::RichText::new(text).color(egui::Color32::WHITE).font(t.semibold(12.0))).extend());
                crate::widgets::register(ui, "dragFolder", label.rect);
            });
        },
    );
}

/// Photos dragged over the closed row `n` for [`SPRING_OPEN_SECONDS`] open it (a row with folders
/// inside it). Restarts on each row the pointer comes to, and when it comes back to one after
/// leaving it (or after another drag).
fn spring_open(ui: &egui::Ui, n: &FolderNode, open: &mut bool) -> bool {
    if *open || n.children.is_empty() {
        return false;
    }
    let key = egui::Id::unique("folder-spring-open");
    let (now, frame) = (ui.input(|i| i.time), ui.ctx().cumulative_frame_nr());
    // held without a break: over this row last frame too
    let since = match ui.data(|d| d.get_temp::<(String, f64, u64)>(key)) {
        Some((path, since, seen)) if path == n.path && seen.saturating_add(1) >= frame => since,
        _ => now,
    };
    ui.data_mut(|d| d.insert_temp(key, (n.path.clone(), since, frame)));
    if now - since >= SPRING_OPEN_SECONDS {
        *open = true;
        ui.data_mut(|d| d.remove::<(String, f64, u64)>(key));
        return true;
    }
    // keep the clock running while the pointer rests
    ui.ctx().request_repaint_after(std::time::Duration::from_millis(100));
    false
}

/// The problems and the number of photos that can go in a `photo.checkMove` answer.
fn move_problems(r: &serde_json::Value) -> (Vec<crate::state::MoveProblem>, usize) {
    let problems = r["problems"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|p| {
            Some(crate::state::MoveProblem {
                id: p["id"].as_u64()?,
                name: p["name"].as_str()?.to_string(),
                kind: crate::state::MoveProblemKind::from_check(p),
            })
        })
        .collect();
    (problems, r["movable"].as_u64().unwrap_or(0) as usize)
}

/// What a move of `n` photos into the folder `name` says: how many moved and that Undo moves them
/// back, or that they were there already.
pub(crate) fn moved_text(n: u64, name: &str) -> String {
    if n == 0 {
        crate::i18n::tr_format!("Already in “{}”", name)
    } else {
        crate::i18n::tr_format!("Moved {n} file{} to “{}”. Edit → Undo moves them back.", if n == 1 { "" } else { "s" }, name, n = n)
    }
}

/// What `folder.create` of the folder at `path` inside `parent` says when it succeeds: the folder
/// is on disk now, but it has no photos, so the Folders list may not show it yet.
pub(crate) fn created_text(path: &str, parent: &str) -> String {
    let name = std::path::Path::new(path).file_name().map_or_else(|| path.to_string(), |f| f.to_string_lossy().to_string());
    crate::i18n::tr_format!("Created folder “{name}” in “{parent}”", name = name, parent = lightcraft_catalog::folders::folder_label(parent))
}

/// [`moved_text`] for a move that left `skipped` photos out.
pub(crate) fn moved_skipping_text(n: u64, skipped: u64, name: &str) -> String {
    crate::i18n::tr_format!(
        "Moved {n} photo{} to “{}”, skipped {skipped}. Edit → Undo moves them back.",
        if n == 1 { "" } else { "s" },
        name,
        n = n,
        skipped = skipped
    )
}

/// Move photos (`ids`, else the selection) into the folder `path` on disk, in the background
/// (`move_task`: a toast says how it went once it ends); a toast says why when it can't begin.
fn move_photos_here(app: &mut LightcraftApp, ctx: &egui::Context, ids: Option<&[u64]>, path: &str) {
    let mut params = json!({"folder": path});
    if let Some(ids) = ids {
        params["ids"] = json!(ids);
    }
    // (a refusal is told in a toast by `start`)
    let _ = crate::move_task::start(app, ctx, &params);
}

/// What moving the folder `name` into the folder `into` says: where it went, how many library
/// photos went with it (only when some did), and that Undo puts it back.
pub(crate) fn folder_moved_text(name: &str, into: &str, photos: u64) -> String {
    if photos == 0 {
        crate::i18n::tr_format!("Moved “{name}” into “{into}”. Edit → Undo moves it back.", name = name, into = into)
    } else {
        crate::i18n::tr_format!("Moved “{name}” into “{into}”. Photos: {n}. Edit → Undo moves it back.", name = name, into = into, n = photos)
    }
}

/// The last name of a path, or the whole path when it has none (a disk's root).
fn last_name(path: &str) -> String {
    std::path::Path::new(path.trim_end_matches(['/', '\\'])).file_name().map_or_else(|| path.to_string(), |n| n.to_string_lossy().to_string())
}

/// Move Folder To…, from the Folders tree and from Local: move the folder `path` into `into`
/// on disk (photos follow, one undo step) and say in a toast where it went.
pub(crate) fn move_folder_to(app: &mut LightcraftApp, ctx: &egui::Context, path: &str, into: &str) {
    match app.run("folder.move", json!({"path": path, "into": into})) {
        Ok(r) => app.toast(ctx, folder_moved_text(&last_name(path), &last_name(into), r["inLibrary"].as_u64().unwrap_or(0))),
        Err(e) => app.toast_error(ctx, e),
    }
}

/// The context menu of a Folders row: a folder's own, a disk's, none for the startup disk.
fn row_menu(app: &mut LightcraftApp, resp: &egui::Response, n: &FolderNode, parents: FolderParents) {
    // one of several folders chosen: what can be done to them all
    let several = crate::panels::side_select::folder_targets(app, &n.path);
    if several.len() > 1 {
        crate::widgets::context_menu(resp, |ui| crate::panels::side_select::folders_menu(app, ui, &several));
        return;
    }
    if n.volume && n.path == "/" {
        return;
    }
    let row = if n.volume { FolderRow::Disk } else { FolderRow::Library };
    disk_folder_menu(app, resp, row, &n.path, Some(n), parents);
}

/// Locate Missing Files… of a Folders row: choose where this folder moved, then let the existing
/// background finder relink only this folder's photos (including the folders inside it).
fn locate_missing_item(app: &mut LightcraftApp, ui: &mut egui::Ui, n: &FolderNode) {
    let r = MenuItem::new(crate::i18n::tr("Locate Missing Files…")).widget("folderLocateMissing").show(ui);
    if r.clicked() {
        if let Err(e) = app.run("file.findMissing", json!({"within": n.path})) {
            app.toast_error(ui.ctx(), e);
        }
        ui.close();
    }
}

/// The node Synchronize Folder… of a Local folder `path` works on, with the name its dialog shows:
/// the Folders row for that very folder, else (a folder above rows) the folder itself, as a folder
/// holding other folders. `None` when no library photo lies in `path` or inside it. Reads the
/// cached Folders tree only; the disk is not touched.
fn library_node_for(tree: &[FolderNode], path: &str) -> Option<(FolderNode, String)> {
    use lightcraft_catalog::query::folder_within;
    fn exact<'a>(tree: &'a [FolderNode], path: &str) -> Option<&'a FolderNode> {
        tree.iter().find_map(|n| if same_folder(&n.path, path) { Some(n) } else { exact(&n.children, path) })
    }
    fn any_inside(tree: &[FolderNode], path: &str) -> bool {
        tree.iter().any(|n| (n.count > 0 && folder_within(&n.path, path)) || any_inside(&n.children, path))
    }
    if let Some(n) = exact(tree, path).filter(|n| n.count > 0) {
        return Some((n.clone(), lightcraft_catalog::folders::folder_label(&n.path)));
    }
    if !any_inside(tree, path) {
        return None;
    }
    let name = std::path::Path::new(path).file_name().map(|f| f.to_string_lossy().to_string()).unwrap_or_else(|| path.to_string());
    let node = FolderNode {
        name: name.clone(),
        path: path.to_string(),
        count: 0,
        own: 0,
        volume: false,
        selectable: false,
        label: None,
        children: Vec::new(),
    };
    Some((node, name))
}

/// A menu row's hint, shown only while the row is enabled: a disabled one says why instead
/// ([`MenuItem::disabled_reason`]).
fn hint_while(r: egui::Response, enabled: bool, hint: &str) -> egui::Response {
    if enabled { r.on_hover_text(hint) } else { r }
}

/// Synchronize Folder… of a folder's menu (widget `folderSynchronize`): opens its dialog, which
/// scans the folder in the background. `target`: the Folders node it works on and the name its
/// dialog shows; `None` (a Local folder holding no library photo) draws it grayed out.
fn synchronize_item(app: &mut LightcraftApp, ui: &mut egui::Ui, target: Option<(&FolderNode, &str)>) {
    let r = MenuItem::new(crate::i18n::tr("Synchronize Folder…"))
        .widget("folderSynchronize")
        .enabled(target.is_some())
        .disabled_reason(crate::i18n::tr("No photo in this folder is in the library yet"))
        .show(ui);
    let r = hint_while(
        r,
        target.is_some(),
        crate::i18n::tr("Find photos added to or missing from this folder on disk, and XMP sidecars changed by other apps"),
    );
    if r.clicked()
        && let Some((n, name)) = target
    {
        // a disk, or a folder holding disks, was chosen as such
        if let Err(e) = crate::sync::open(app, &n.path, name, n.volume || !n.selectable) {
            app.toast_error(ui.ctx(), e);
        }
        ui.close();
    }
}

/// Update Folder Location… of a Folders row (widget `folderLocate`; `folder.locate`): say where
/// the folder is now when it was moved or renamed outside LightCraft, or its disk is mounted
/// somewhere else. Offered always: whether the folder is there is not asked of the disk for a
/// menu (the sidebar never waits on a network).
fn locate_item(app: &mut LightcraftApp, ui: &mut egui::Ui, n: &FolderNode) {
    if app.services.pick_folder.is_none() && app.services.picker.is_none() {
        return;
    }
    let r = MenuItem::new(crate::i18n::tr("Update Folder Location…"))
        .widget("folderLocate")
        .show(ui)
        .on_hover_text(crate::i18n::tr("Choose where this folder is now, after it was moved or renamed outside LightCraft or its disk changed; its photos follow, no file is touched"));
    if r.clicked() {
        // a disk, or a folder holding disks, was chosen as such
        if let Err(e) = app.run("folder.locate", json!({"folder": n.path, "disk": n.volume || !n.selectable})) {
            app.toast_error(ui.ctx(), e);
        }
        ui.close();
    }
}

/// Why a Folders row can't be exported as it is shown, `None` when it can: it holds no photo, or
/// choosing it would show the photos of other disks too (see [`FolderNode::selectable`]), so
/// the grid could not be made to hold exactly this folder's photos.
pub(crate) fn folder_export_blocker(n: &FolderNode) -> Option<&'static str> {
    if !n.selectable {
        Some("This row covers more than one disk: export a folder inside it")
    } else if n.count == 0 {
        Some("No photo in this folder is in the library yet")
    } else {
        None
    }
}

/// Export Folder… and Export Folder with Preset ▸ of a Folders row (`with_preset`: the second):
/// like an album's, they show the folder as the Folders row does (its photos and those of the
/// folders inside it), select them all, then open the Export dialog or run the preset in the
/// background. Grayed out, with the reason, for a row that can't be shown as a source or holds no
/// photo. Widgets `folderExport`, `folderExportPresetMenu` and `folderExportPreset:<name>`.
fn export_folder_item(app: &mut LightcraftApp, ui: &mut egui::Ui, n: &FolderNode, with_preset: bool) {
    let blocker = folder_export_blocker(n);
    let show_all = |app: &mut LightcraftApp| -> Result<(), String> {
        app.run("library.source", json!({"kind": "libraryFolder", "path": n.path}))?;
        app.run("library.selectAll", json!({})).map(|_| ())
    };
    let reason = crate::i18n::tr(blocker.unwrap_or(""));
    if !with_preset {
        let item = MenuItem::new(crate::i18n::tr("Export Folder…")).widget("folderExport").enabled(blocker.is_none());
        let r = (if blocker.is_some() { item.disabled_reason(reason) } else { item }).show(ui);
        let r = hint_while(r, blocker.is_none(), crate::i18n::tr("Exports this folder's photos and those in the folders inside it"));
        if r.clicked() {
            if let Err(e) = show_all(app).and_then(|()| app.run("dialog.export", json!({})).map(|_| ())) {
                app.toast_error(ui.ctx(), e);
            }
            ui.close();
        }
        return;
    }
    let item = MenuItem::new(crate::i18n::tr("Export Folder with Preset")).widget("folderExportPresetMenu").enabled(blocker.is_none());
    let item = if blocker.is_some() { item.disabled_reason(reason) } else { item };
    item.submenu(ui, |ui| {
        for (p, _) in app.session.all_export_presets() {
            if MenuItem::new(&p.name).widget(format!("folderExportPreset:{}", p.name)).show(ui).clicked() {
                if let Err(e) = show_all(app).and_then(|()| app.run("app.export", json!({"preset": p.name, "background": true})).map(|_| ())) {
                    app.toast_error(ui.ctx(), e);
                }
                ui.close();
            }
        }
    });
}

/// Set Color Label ▸ of a Folders row (`folder.label`; the submenu is widget `folderLabelMenu`,
/// its colours `folderLabel:<colour>`).
fn folder_label_menu(app: &mut LightcraftApp, ui: &mut egui::Ui, n: &FolderNode) {
    MenuItem::new(crate::i18n::tr("Set Color Label")).widget("folderLabelMenu").submenu(ui, |ui| {
        if let Some(l) = crate::panels::grid::label_items(app, ui, n.label, "folderLabel") {
            if let Err(e) = app.run("folder.label", json!({"path": n.path, "label": crate::panels::grid::label_param(l)})) {
                app.toast_error(ui.ctx(), e);
            }
            ui.close();
        }
    });
}

/// The "Move Folder To…" row of a folder's menu, registered as widget `id` (so the control
/// channel and tests can click it). Only where the host can choose a
/// folder (`pick_folder`); returns whether it was clicked.
fn move_folder_item(app: &LightcraftApp, ui: &mut egui::Ui, id: String) -> bool {
    if app.services.pick_folder.is_none() {
        return false;
    }
    MenuItem::new(crate::i18n::tr("Move Folder To…"))
        .widget(id)
        .show(ui)
        .on_hover_text(crate::i18n::tr("Moves the folder on disk into the folder you choose; its photos follow. You can undo this."))
        .clicked()
}

/// Which row a folder's context menu is opened on; it decides the menu's groups.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum FolderRow {
    /// A folder in the Folders tree.
    Library,
    /// A disk's row in the Folders tree.
    Disk,
    /// A Local row: `top` when it is a place at the top of Local, `transient` when that place is
    /// listed for this session only.
    Local { top: bool, transient: bool },
}

/// One row of a folder's context menu.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum FolderItem {
    ImportHere,
    AddToMyPhotos,
    Synchronize,
    LocateMissing,
    UpdateLocation,
    ExportFolder,
    ExportFolderWithPreset,
    NewFolder,
    Rename,
    Move,
    MovePhotosHere,
    ColorLabel,
    /// Show Parent Folder / Hide This Parent: the row's own, when it has folders passed over
    /// above it or was itself asked for.
    Parents,
    Reveal,
    CopyPath,
    KeepInLocal,
    RemoveFromLocal,
    RemoveFromLibrary,
    RemoveDisk,
}

/// The groups of a folder's context menu, in the order they are drawn with a separator between
/// two: what it does with the library's photos, how it reorganizes the disk, how the row looks,
/// the way out to the file manager, and last what lists or removes it. Every row of a folder
/// takes the same order; a row leaves out only the groups that can't apply to it.
fn folder_menu_groups(row: FolderRow) -> &'static [&'static [FolderItem]] {
    use FolderItem::*;
    match row {
        FolderRow::Library => &[
            &[ImportHere, Synchronize, LocateMissing, UpdateLocation, ExportFolder, ExportFolderWithPreset],
            &[NewFolder, Rename, Move, MovePhotosHere],
            &[ColorLabel, Parents],
            &[Reveal, CopyPath],
            &[RemoveFromLibrary],
        ],
        FolderRow::Disk => &[
            &[Synchronize, LocateMissing, UpdateLocation, ExportFolder, ExportFolderWithPreset],
            &[NewFolder],
            &[ColorLabel],
            &[Reveal, CopyPath],
            &[RemoveDisk],
        ],
        FolderRow::Local { .. } => &[&[AddToMyPhotos, Synchronize], &[NewFolder, Rename, Move], &[Reveal, CopyPath], &[KeepInLocal, RemoveFromLocal]],
    }
}

/// `path` as the OS spells it: its separators native (a path may carry `/` on Windows), and a
/// bare drive (`D:`, a disk's row) as the drive's root.
pub(crate) fn native_path(path: &str, sep: char) -> String {
    let mut spelled: String = path.chars().map(|c| if c == '/' { sep } else { c }).collect();
    if matches!(spelled.as_bytes(), [d, b':'] if d.is_ascii_alphabetic()) {
        spelled.push(sep);
    }
    spelled
}

/// The context menu of the folder at `path`, laid out by [`folder_menu_groups`]. `node`: its
/// Folders node (`None` on a Local row). A row that depends on the moment (nothing selected to
/// move here, no library photo to synchronize) is grayed out with the reason rather than left
/// out, so the menu keeps its shape; only what this host can't do (no folder picker, no file
/// manager) is left out.
fn disk_folder_menu(app: &mut LightcraftApp, resp: &egui::Response, row: FolderRow, path: &str, node: Option<&FolderNode>, parents: FolderParents) {
    let groups = folder_menu_groups(row);
    crate::widgets::context_menu(resp, |ui| {
        let name = std::path::Path::new(path).file_name().map(|f| f.to_string_lossy().to_string()).unwrap_or_else(|| path.to_string());
        // what dialogs call it: the last two names, so same-named folders are told apart
        let label = lightcraft_catalog::folders::folder_label(path);
        let mut drawn = false;
        for group in groups {
            if !group.iter().any(|i| folder_item_available(app, row, *i, path, node, parents)) {
                continue;
            }
            if drawn {
                ui.separator();
            }
            drawn = true;
            for &item in group.iter() {
                if folder_item_available(app, row, item, path, node, parents) {
                    folder_item(app, ui, row, item, path, node, (&name, &label), parents);
                }
            }
        }
    });
}

/// Whether `item` is drawn at all: what this host can't do, or a row that needs a Folders node
/// when it has none, is left out.
fn folder_item_available(
    app: &LightcraftApp,
    row: FolderRow,
    item: FolderItem,
    path: &str,
    node: Option<&FolderNode>,
    parents: FolderParents,
) -> bool {
    match item {
        // a built-in place is one of the computer's own folders under a translated label (Home is the user's whole
        // home): Local offers no Rename or Move for it (the other items stay)
        FolderItem::Rename | FolderItem::Move if matches!(row, FolderRow::Local { top: true, .. }) && is_builtin_place(app, path) => false,
        FolderItem::Move | FolderItem::ImportHere => app.services.pick_folder.is_some(),
        FolderItem::Reveal => app.services.reveal.is_some(),
        FolderItem::Parents => node.is_some() && (parents.above.is_some() || parents.asked_for),
        FolderItem::LocateMissing
        | FolderItem::UpdateLocation
        | FolderItem::ExportFolder
        | FolderItem::ExportFolderWithPreset
        | FolderItem::ColorLabel
        | FolderItem::RemoveFromLibrary
        | FolderItem::RemoveDisk => node.is_some(),
        _ => true,
    }
}

/// Draws one row of [`disk_folder_menu`] and does what it says when clicked. `names`: the
/// folder's own name on disk, and what dialogs call it.
#[allow(clippy::too_many_arguments)]
fn folder_item(
    app: &mut LightcraftApp,
    ui: &mut egui::Ui,
    row: FolderRow,
    item: FolderItem,
    path: &str,
    node: Option<&FolderNode>,
    names: (&str, &str),
    parents: FolderParents,
) {
    let (name, label) = names;
    // a disk is called by its own name, as in Synchronize and Remove Disk
    let label = match (row, node) {
        (FolderRow::Disk, Some(n)) => n.name.as_str(),
        _ => label,
    };
    match item {
        FolderItem::ImportHere => import_here_item(app, ui, path),
        FolderItem::AddToMyPhotos => add_to_my_photos_item(app, ui, path),
        FolderItem::Synchronize => match (row, node) {
            (FolderRow::Disk, Some(n)) => synchronize_item(app, ui, Some((n, &n.name))),
            (_, Some(n)) => synchronize_item(app, ui, Some((n, label))),
            // a Local row: the Folders node for this folder, or for the library folders inside it
            (_, None) => {
                let library = app.caches.folder_tree(&app.session.catalog, app.session.library_generation());
                let held = library_node_for(&library, path);
                synchronize_item(app, ui, held.as_ref().map(|(n, name)| (n, name.as_str())));
            }
        },
        FolderItem::LocateMissing => {
            if let Some(n) = node {
                locate_missing_item(app, ui, n);
            }
        }
        FolderItem::UpdateLocation => {
            if let Some(n) = node {
                locate_item(app, ui, n);
            }
        }
        FolderItem::ExportFolder | FolderItem::ExportFolderWithPreset => {
            if let Some(n) = node {
                export_folder_item(app, ui, n, item == FolderItem::ExportFolderWithPreset);
            }
        }
        FolderItem::NewFolder => {
            let r = MenuItem::new(crate::i18n::tr("New Folder on Disk…")).widget("folderNewFolder").show(ui);
            if r.clicked() {
                app.ui.dialog = Some(crate::state::Dialog::TextPrompt {
                    title: crate::i18n::tr_format!("New Folder in “{name}”", name = label),
                    hint: "Folder name (created on disk)".into(),
                    value: String::new(),
                    command: "folder.create".into(),
                    params: json!({"parent": path}),
                    key: "name".into(),
                });
                ui.close();
            }
        }
        FolderItem::Rename => {
            let r = MenuItem::new(crate::i18n::tr("Rename Folder…")).widget("folderRename").show(ui);
            if r.clicked() {
                app.ui.dialog = Some(crate::state::Dialog::TextPrompt {
                    title: crate::i18n::tr_format!("Rename “{name}”", name = label),
                    hint: "Folder name (renamed on disk; its photos follow)".into(),
                    value: name.to_string(),
                    command: "folder.rename".into(),
                    params: json!({"path": path}),
                    key: "name".into(),
                });
                ui.close();
            }
        }
        FolderItem::Move => {
            let id = if matches!(row, FolderRow::Local { .. }) { format!("localFolderMove:{path}") } else { format!("folderMove:{path}") };
            if move_folder_item(app, ui, id) {
                let into = app.services.pick_folder.as_mut().and_then(|f| f());
                if let Some(into) = into {
                    move_folder_to(app, ui.ctx(), path, &into);
                }
                ui.close();
            }
        }
        FolderItem::MovePhotosHere => {
            let any = !app.session.selection.ids.is_empty();
            let r = MenuItem::new(crate::i18n::tr("Move Selected Photos Here"))
                .widget("folderMovePhotosHere")
                .enabled(any)
                .disabled_reason(crate::i18n::tr("Select the photos to move in the grid first"))
                .show(ui);
            let r = hint_while(r, any, crate::i18n::tr("Moves the files and their XMP sidecars into this folder; never overwrites"));
            if r.clicked() {
                move_photos_here(app, ui.ctx(), None, path);
                ui.close();
            }
        }
        FolderItem::ColorLabel => {
            if let Some(n) = node {
                folder_label_menu(app, ui, n);
            }
        }
        FolderItem::Parents => {
            if let Some(n) = node {
                folder_parent_items(app, ui, n, parents);
            }
        }
        FolderItem::Reveal => {
            let r = MenuItem::new(crate::i18n::tr(crate::menus::reveal_label())).widget("folderReveal").show(ui);
            if r.clicked() {
                if let Some(Err(e)) = app.services.reveal.as_mut().map(|f| f(path)) {
                    app.toast_error(ui.ctx(), e);
                }
                ui.close();
            }
        }
        FolderItem::CopyPath => {
            let r = MenuItem::new(crate::i18n::tr("Copy Path")).widget("folderCopyPath").show(ui);
            if r.clicked() {
                ui.ctx().copy_text(native_path(path, std::path::MAIN_SEPARATOR));
                app.toast(ui.ctx(), crate::i18n::tr("Path copied"));
                ui.close();
            }
        }
        FolderItem::KeepInLocal => {
            let (top, transient) = match row {
                FolderRow::Local { top, transient } => (top, transient),
                _ => (false, false),
            };
            // a place already kept at the top of Local has nothing to add
            let r = MenuItem::new(crate::i18n::tr(if top { "Keep in Local" } else { "Add to Local" }))
                .widget("folderKeepInLocal")
                .enabled(!top || transient)
                .disabled_reason(crate::i18n::tr("This place is kept in Local already"))
                .show(ui);
            let r = hint_while(r, !top || transient, crate::i18n::tr("List this folder in Local from now on"));
            if r.clicked() {
                app.act("local.addRoot", json!({"path": path}));
                ui.close();
            }
        }
        FolderItem::RemoveFromLocal => {
            let top = matches!(row, FolderRow::Local { top: true, .. });
            let r = MenuItem::new(crate::i18n::tr("Remove from Local"))
                .widget("folderRemoveFromLocal")
                .enabled(top)
                .disabled_reason(crate::i18n::tr("Only a place at the top of Local can be removed from it"))
                .show(ui);
            let r = hint_while(r, top, crate::i18n::tr("Hides this shortcut only; the folder and its photos stay as they are"));
            if r.clicked() {
                app.act("local.hide", json!({"path": path}));
                ui.close();
            }
        }
        FolderItem::RemoveFromLibrary | FolderItem::RemoveDisk => {
            let Some(n) = node else { return };
            let disk = item == FolderItem::RemoveDisk;
            let (text, hint) = if disk {
                ("Remove Disk from Library…", "Moves every photo imported from this disk to Recently Deleted; no file is touched")
            } else {
                ("Remove from Library…", "Moves the photos imported from this folder to Recently Deleted; no file is touched")
            };
            let r = MenuItem::new(crate::i18n::tr(text)).widget("folderRemove").show(ui).on_hover_text(crate::i18n::tr(hint));
            if r.clicked() {
                let name = if disk { n.name.clone() } else { label.to_string() };
                app.ui.dialog = Some(crate::state::Dialog::RemoveFolder { path: n.path.clone(), name, count: n.count, disk });
                ui.close();
            }
        }
    }
}

/// The "Import to This Folder…" row of [`folder_item`].
fn import_here_item(app: &mut LightcraftApp, ui: &mut egui::Ui, path: &str) {
    // a folder picker already up would swallow this row's request (and its destination)
    let picking = crate::import::add_folder_picker_pending(app);
    let can = crate::import::can_open(app);
    let mut item = MenuItem::new(crate::i18n::tr("Import to This Folder…")).widget("folderImportHere").enabled(can && !picking);
    if !can {
        item = item.disabled_reason(crate::i18n::tr("An import is running"));
    }
    let r = item.show(ui);
    let r =
        hint_while(r, can && !picking, crate::i18n::tr("Copies the photos of a folder you choose into this folder; the review lets you change that"));
    if r.clicked() {
        // the folder picker, then the Import review copying into this folder
        if let Err(e) = app.run("file.addFolder", json!({"destination": path})) {
            app.toast_error(ui.ctx(), e);
        }
        ui.close();
    }
}

/// The "Add to My Photos…" row of [`folder_item`].
fn add_to_my_photos_item(app: &mut LightcraftApp, ui: &mut egui::Ui, path: &str) {
    let can = crate::import::can_open(app);
    let r = MenuItem::new(crate::i18n::tr("Add to My Photos…"))
        .widget("folderAddToMyPhotos")
        .enabled(can)
        .disabled_reason(crate::i18n::tr("An import is running"))
        .show(ui);
    let r =
        hint_while(r, can, crate::i18n::tr("Reviews the photos in this folder, then adds them to the library where they are; no file is touched"));
    if r.clicked() {
        if let Err(e) = app.run("file.addFolder", json!({"path": path})) {
            app.toast_error(ui.ctx(), e);
        }
        ui.close();
    }
}
