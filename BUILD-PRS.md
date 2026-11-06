# PRs in the `integration` build

The `integration` branch (the build I run) is LightCraft `main` at `07301cbd1e` plus the pull requests below, merged or ported together, plus integration-only fixes where two of them meet (910 commits on top of `main`, merges and fixes included). The PRs were open when they were taken; the State column is their state now. Source: `integration` at `7248a97a77` (committed 2026-11-06 01:00 UTC). This list is written by `just build-prs 7248a97a77` and replaced whenever a new build is swapped in.

How each PR was found: a commit in the build that merges, ports, applies, refreshes or records it, naming the head it took (the commit is shown); its head being in the build's history ("head in build"); all of its commits being in the build by patch ("same patches in build"); or a later head of a PR already in the build whose new commits reverse cleanly against the build's tree (docs, locale catalogs and the lock file left out) or are each in the build as a commit with the same subject and author ("later commits ported"; the commit shown is the earlier head's). "Taken at" is the head that went into the build; "Head now" says whether GitHub still has that head ("same") or the PR has moved on (the build holds the older head: no commit, diff or port of the newer one was found in it).

189 PRs: 4 closed, 168 open, 17 open (draft); 161 at their current head, 28 moved since.

## My PRs (187)

| PR | Title | State | Taken at | Head now | In the build through |
|---|---|---|---|---|---|
| [#768](https://github.com/storytold/lightcraft/pull/768) | LR-LIB-FOLDER: a folder of albums shows the photos of the albums inside it | open | `c12dec194b` | same | `f00c81553f` |
| [#775](https://github.com/storytold/lightcraft/pull/775) | LR-LIB-FOLDER: the view never shows an album that is gone | open | `f4c039451d` | same | `2a15d7f788` |
| [#776](https://github.com/storytold/lightcraft/pull/776) | LR-LIB-FOLDER: smart albums in album folders keep off loops (stacked on #768) | open (draft) | `afa4a732b7` | same | `0a8e31aaae` |
| [#777](https://github.com/storytold/lightcraft/pull/777) | LR-LIB-FOLDER: a refused command says why | open | `a9ec9a9d6f` | same | `1a11277a3b` |
| [#800](https://github.com/storytold/lightcraft/pull/800) | Catalog: the log is compacted by the snapshot's size, not every 2000 records | open | `b1c10ce079` | same | `4bef47c01b` |
| [#841](https://github.com/storytold/lightcraft/pull/841) | Catalog: back up the library to a dated folder that shares unchanged files (stacked on #2985) | open (draft) | `3dd8fc4694` | same | `aaf3975fd0` |
| [#856](https://github.com/storytold/lightcraft/pull/856) | Catalog: restore a library from a backup, and keep what it replaces (stacked on #841) | open (draft) | `e2e41592a2` | same | `9aa6277262` |
| [#870](https://github.com/storytold/lightcraft/pull/870) | Catalog: restore from the File menu, with the backup copied in the background (stacked on #856) | open (draft) | `9820b0f96d` | same | `1ef6951fe7` |
| [#1004](https://github.com/storytold/lightcraft/pull/1004) | Catalog: the backup folder is shown and chosen in Settings (stacked on #870) | open (draft) | `1a5f2f9ac6` | same | `386c26f35e` |
| [#1025](https://github.com/storytold/lightcraft/pull/1025) | Catalog: a library is backed up by itself while it is open, and when LightCraft quits (stacked on #1004) | open (draft) | `32a9a49523` | same | `33f52ff3e0` |
| [#1034](https://github.com/storytold/lightcraft/pull/1034) | Catalog: a library can keep its backups for a time, and old ones go after a new one is made (stacked on #1025) | open (draft) | `6e5f1472cd` | same | `6f56c0360c` |
| [#1043](https://github.com/storytold/lightcraft/pull/1043) | Catalog: Restore Library from Backup lists the library's backups to choose from (stacked on #1034) | open (draft) | `5781e7edf5` | same | `34d8ea9cbd` |
| [#1054](https://github.com/storytold/lightcraft/pull/1054) | Catalog: what restores replaced goes with the old backups of a library that keeps them for a time (stacked on #1034) | open (draft) | `122158731b` | same | `b71c2140f2` |
| [#1075](https://github.com/storytold/lightcraft/pull/1075) | Backup and restore: the smaller items left on #870's list (stacked on #1054) | open (draft) | `5ea5152d31` | same | `838f14bd0b` |
| [#1095](https://github.com/storytold/lightcraft/pull/1095) | Import: what the photos themselves carry can be left in the files | open | `7227ccb9bf` | same | `17c263daac` |
| [#1124](https://github.com/storytold/lightcraft/pull/1124) | Synchronize Folder: the scan can continue in the background | open | `da8e5bd340` | same | `0c8dcd1f80` |
| [#1127](https://github.com/storytold/lightcraft/pull/1127) | Synchronize Folder: a Read row for the new photos, and dialogs remember theirs (stacked on #1095) | open | `43f3ee80e0` | same | `3e29b58ee5` |
| [#1153](https://github.com/storytold/lightcraft/pull/1153) | Library: show several albums, folders, keywords or dates at once (`library.source {ids, paths}`; engine only) | open | `8588eb8590` | same | `83ff003b27` |
| [#1158](https://github.com/storytold/lightcraft/pull/1158) | LR-LIB-DELETE: the engine can move a file to the system Trash and put it back | open | `15777f8568` | same | `76dfb011d1` |
| [#1243](https://github.com/storytold/lightcraft/pull/1243) | LRC-LIB-LIBFOLDERS: Folders skips the folders that only lead to your photos, with Show Parent Folder to list them | open | `59944f6e9b` | same | `be05c1d27c` |
| [#1244](https://github.com/storytold/lightcraft/pull/1244) | Sidebar: the row of what is shown has a bar, and a folded section says what it holds | open | `77b0863f1f` | same | `4b8cbb76ee` |
| [#1303](https://github.com/storytold/lightcraft/pull/1303) | Settings: Backups says where the library stands, with Back Up Now (stacked on #1075) | open (draft) | `8b0dff2870` | same | `d2243d71ab` |
| [#1374](https://github.com/storytold/lightcraft/pull/1374) | LR-PREF-INTERFACE: text sizes have names, the seven type roles of the theme | open | `b341826e11` | same | `1e4c3a3c96` |
| [#1626](https://github.com/storytold/lightcraft/pull/1626) | LR-BEHAV-SIDEBAR-COLLAPSE: the sidebar's trees open as they were left | open | `3d28e091ef` | same | `a5c2dafee1` |
| [#1631](https://github.com/storytold/lightcraft/pull/1631) | LR-BEHAV-SIDEBAR-SECTIONS: Option-click on a triangle opens or folds the whole branch (stacked on #1626) | open | `0d0456315d` | same | `c8c81f112a` |
| [#1642](https://github.com/storytold/lightcraft/pull/1642) | LR-BEHAV-BG-TASKS: running tasks live under the top bar's bell, not over the top-left corner | open | `af25c0f978` | same | `b91987e2e2` |
| [#1649](https://github.com/storytold/lightcraft/pull/1649) | LR-BEHAV-BG-TASKS: an export or import that ended stays in the list under the bell (stacked on #1642 and #3151) | open | `8c2311c6c9` | same | `df83a8f399` |
| [#1711](https://github.com/storytold/lightcraft/pull/1711) | LR-BEHAV-BG-TASKS: a background task says how it ended; the engine's own jobs do (part 1 of 3) | open | `1233e35b20` | same | `1b96075495` |
| [#1781](https://github.com/storytold/lightcraft/pull/1781) | LR-BEHAV-EMPTY-STATES: Help ▸ Add Sample Photos and Remove Sample Photos | open | `4cd0e5c423` | same | `153ba6187b` |
| [#1925](https://github.com/storytold/lightcraft/pull/1925) | LRC-LIB-LIBFOLDERS: photos move to another folder in the background | open | `099ee8dff8` | same | `1a12b9b0f9` |
| [#1933](https://github.com/storytold/lightcraft/pull/1933) | LR-BEHAV-BG-TASKS: waiting preview builds are listed under the bell, File > Previews queues, and a task that did not go well can be retried (stacked on #1649 and #2806) | open | `6b4a62b5a6` | same | `b34ab0de1c` |
| [#1936](https://github.com/storytold/lightcraft/pull/1936) | LR-BEHAV-TOAST: toasts stack instead of replacing one another, and a new state replaces the old one | open | `875ef4808c` | same | `7a230fc8aa` |
| [#1943](https://github.com/storytold/lightcraft/pull/1943) | LR-BEHAV-BG-TASKS: imports and Synchronize Folder wait for each other instead of refusing (stacked on #2806) | open | `abc98ec670` | same | `69488a057b` |
| [#2020](https://github.com/storytold/lightcraft/pull/2020) | LR-LIB-SMARTALBUM: creating or giving a keyword no longer freezes the window while albums are counted | open | `83f068807b` | same | `ab56767dc3` |
| [#2034](https://github.com/storytold/lightcraft/pull/2034) | LRC-LIB-LIBFOLDERS: Synchronize Folder can bring in files already in the library elsewhere, and remembers the choice | open | `22468741e1` | same | `76adfff890` |
| [#2043](https://github.com/storytold/lightcraft/pull/2043) | LRC-LIB-SMARTCOLL: a Same Content as Another Photo rule finds photos whose file is in the library twice | open | `40e6cc3470` | moved to `09f1c2b956` | `7f8e85b8e6` |
| [#2047](https://github.com/storytold/lightcraft/pull/2047) | LRC-LIB-LIBFOLDERS: importing into a large library keeps the window responsive | open | `3d8ca1c08d` | moved to `2a307e0385` | `c51898782f` |
| [#2078](https://github.com/storytold/lightcraft/pull/2078) | LR-BEHAV-ACCESS: one tick box, radio and toggle button for every choice | open | `fd8ff7a4ee` | same | `2848724773` |
| [#2079](https://github.com/storytold/lightcraft/pull/2079) | LR-BEHAV-ACCESS: action buttons share one Button component | open | `77487609c8` | same | `2985896201` |
| [#2080](https://github.com/storytold/lightcraft/pull/2080) | MENU-BAR: one menu row and shortcut component for every menu | open | `43102c1468` | same | `ca3a7407f6` |
| [#2164](https://github.com/storytold/lightcraft/pull/2164) | LR-VIEW-FILMSTRIP: filmstrip and grid thumbnails share one overlay, and it degrades on narrow tiles | open | `b3ab4d98d3` | moved to `8a570634ef` | `8f4b6412c9` |
| [#2194](https://github.com/storytold/lightcraft/pull/2194) | MENU-BAR: every command has a place in the menus, and View drops from 62 rows to 36 | open | `4d27821f0b` | same | `2419e042c6` |
| [#2204](https://github.com/storytold/lightcraft/pull/2204) | LR-BEHAV-DRAGDROP: photo drags say what a drop will do, refuse what it can't, and Esc gives them up | open | `10275023d2` | same | `9b170c91be`, later commits ported |
| [#2370](https://github.com/storytold/lightcraft/pull/2370) | LR-BEHAV-TOAST: toasts stay long enough to read, hold under the pointer, and errors stay 6 s | open | `72344858c7` | same | `05eb768d4c` |
| [#2373](https://github.com/storytold/lightcraft/pull/2373) | LR-BEHAV-ACCESS: Reduce motion stops zoom, scroll and spinner animations, and follows macOS until you choose | open | `8593a919ab` | moved to `80dc8e5d39` | `4d02513f0b` |
| [#2378](https://github.com/storytold/lightcraft/pull/2378) | LR-BEHAV-BG-TASKS: detached readers for an import or scan (stall, skip, cap), tested and not used yet | open | `62381956eb` | moved to `2309336204` | `b52ec6bdda` |
| [#2414](https://github.com/storytold/lightcraft/pull/2414) | PERF-SELECT: a big selection no longer slows every idle frame | open | `8eec5d37e3` | same | `fb185bebd6` |
| [#2415](https://github.com/storytold/lightcraft/pull/2415) | PERF-LIB: big views list from a kept sort order instead of sorting every photo | open | `47c13a6558` | same | `6308ed0c2e` |
| [#2418](https://github.com/storytold/lightcraft/pull/2418) | PERF-SEARCH: typing in the library search keeps up on a large library | open | `ab21a4c809` | same | `1c42da31c5` |
| [#2419](https://github.com/storytold/lightcraft/pull/2419) | PERF-SIDECARS: imports and moves answer sidecar lookups from the folder listing | open | `27bc794a99` | moved to `8054d3f94d` | `94de6c57c9` |
| [#2459](https://github.com/storytold/lightcraft/pull/2459) | PERF-WALK: a folder walk on a network share lists up to eight folders at the same time | open | `0d22430798` | moved to `a8d7651d03` | `8bcc533708` |
| [#2464](https://github.com/storytold/lightcraft/pull/2464) | LRC-LIB-MISSING: one rule for Missing Photos' files, and a folder listing that panics no longer stays running | open | `a895a6a877` | moved to `74e4d0f691` | `3f206eb927` |
| [#2522](https://github.com/storytold/lightcraft/pull/2522) | LR-PREF-GENERAL: Settings tabs look like tabs, and the keyboard focus shows and moves in one short way | open | `3d7fa853a1` | same | `77ce234475` |
| [#2523](https://github.com/storytold/lightcraft/pull/2523) | LR-PREF-GENERAL: Settings asks before Clear Cache, a model's Remove and deleting smart previews | open | `cd9978f45b` | same | `10eede925b` |
| [#2524](https://github.com/storytold/lightcraft/pull/2524) | LR-PREF-GENERAL: the Settings tabs are grouped by what a person comes to do | open | `722a62b5f1` | same | `73675d16d8` |
| [#2525](https://github.com/storytold/lightcraft/pull/2525) | LR-PREF-GENERAL: what Settings says can be read in every theme, and in every language it has | open | `0a4b86c9dc` | moved to `48020c660c` | `108c592291` |
| [#2597](https://github.com/storytold/lightcraft/pull/2597) | LRC-LIB-LIBFOLDERS: the folders test fixture waits for the Missing Photos row | open | `93a2157a0a` | same | `7f448bcc9f` |
| [#2649](https://github.com/storytold/lightcraft/pull/2649) | LR-LIB-DELETE: the catalog answers "which photos have a file in this folder / of this name" from an index | open | `d031492f7a` | same | `bf4d733c91` |
| [#2697](https://github.com/storytold/lightcraft/pull/2697) | LRC-LIB-FOLDERS: Local says where its listing stops, with a Go Deeper row | open | `2bd55c33db` | same | `5970e868d4` |
| [#2704](https://github.com/storytold/lightcraft/pull/2704) | LR-BEHAV-ACCESS: an unnamed control on a main screen fails the tests (today's gaps listed) | open | `267f473c02` | same | `fa57ba028d` |
| [#2705](https://github.com/storytold/lightcraft/pull/2705) | LR-IMP-FORMATS: a JPEG cut short imports from the data present | open | `632b6550f6` | same | `ff7897c78c` |
| [#2707](https://github.com/storytold/lightcraft/pull/2707) | LR-BEHAV-MULTISELECT: the model for choosing several sidebar rows with Cmd/Ctrl+click and Shift+click | closed | `2529b790f8` | moved to `ebe2477108` | `28112cf185` |
| [#2715](https://github.com/storytold/lightcraft/pull/2715) | LR-BEHAV-ACCESS: every color pair the UI paints stands out in every theme (today's gaps listed) | open | `72bbc7865b` | same | `85fca8be0f` |
| [#2721](https://github.com/storytold/lightcraft/pull/2721) | LRC-LIB-LIBFOLDERS: a refused move to another folder says what is wrong in plain words; photo.checkMove and skip | open | `0e8106cd1a` | same | `dec9cf323e`, recorded as contained |
| [#2734](https://github.com/storytold/lightcraft/pull/2734) | LR-BEHAV-DRAGDROP: the photos being dragged are dimmed in the grid | open | `e34ab8f535` | same | `15dbfbfb4b` |
| [#2736](https://github.com/storytold/lightcraft/pull/2736) | LR-BEHAV-DRAGDROP: the message after a drop that moved, added or tagged photos has an Undo button | open | `624d5ac15a` | same | `fe198cab68` |
| [#2747](https://github.com/storytold/lightcraft/pull/2747) | LRC-LIB-LIBFOLDERS: moving a folder refuses a missing destination and says why in plain words | open | `cba7d0a18e` | same | `0ec1904872` |
| [#2749](https://github.com/storytold/lightcraft/pull/2749) | LRC-LIB-LIBFOLDERS: after moving a folder, the message says where it went, in every language | open | `5ba1f745cf` | moved to `6f1209d0a2` | `7c13be0be4` |
| [#2751](https://github.com/storytold/lightcraft/pull/2751) | LRC-LIB-LIBFOLDERS: several folders move into one in a single undo step (folder.move paths) | open | `d3365f18c2` | same | `7b30477bdc` |
| [#2752](https://github.com/storytold/lightcraft/pull/2752) | LR-BEHAV-DRAGDROP: a Folders row dragged onto another moves that folder on disk, after a question | open | `2e8f7f2d5b` | same | `2039e87abe` |
| [#2756](https://github.com/storytold/lightcraft/pull/2756) | LR-BEHAV-SIDEBAR-SECTIONS: Option-click on a triangle opens or folds the whole branch | open | `e66985a6e9` | moved to `cf8afaa42c` | `cc996bd5f7` |
| [#2757](https://github.com/storytold/lightcraft/pull/2757) | LR-BEHAV-MULTISELECT: several sidebar albums, keywords or folders are deleted, moved or labeled as one undo step (engine) | open | `8bf8557711` | moved to `5ab04d7ee1` | `b9862df062` |
| [#2768](https://github.com/storytold/lightcraft/pull/2768) | LR-VIEW-FILMSTRIP: a selected photo's ring shows before its thumbnail loads | open | `83c8c10be2` | same | `5920ee097d` |
| [#2797](https://github.com/storytold/lightcraft/pull/2797) | LR-BEHAV-AUTOSAVE: a change that could only be partly taken back is saved and can be undone | open | `a6e65608b6` | moved to `d3e0f94d6b` | `640d10ef83` |
| [#2806](https://github.com/storytold/lightcraft/pull/2806) | LR-BEHAV-BG-TASKS: a preview build asked for while another runs can wait in line for it (engine) | open | `67a5f01565` | moved to `3d75f5d598` | `70ce4b372b` |
| [#2812](https://github.com/storytold/lightcraft/pull/2812) | PERF-CATALOG: equal develop settings are held once in memory | open | `f9085a1255` | moved to `9af905e836` | `1b1781e848` |
| [#2818](https://github.com/storytold/lightcraft/pull/2818) | LRC-LIB-SMARTCOLL: a smart album on a loop of albums holds no photos, and says which album it loops through | open | `695c30c2e6` | moved to `7af6e761bf` | `f796caf353` |
| [#2828](https://github.com/storytold/lightcraft/pull/2828) | LRC-LIB-SMARTCOLL: reorder rules and groups in the smart album editor by drag or arrow keys | open | `a2f64b1222` | moved to `b75e7adf4c` | `34a3665d81` |
| [#2860](https://github.com/storytold/lightcraft/pull/2860) | LRC-LIB-LIBFOLDERS: the engine can locate a folder that is somewhere else now (engine only) | open | `b6fc1b334b` | moved to `7c3bf8d8b6` | `fa5bf38eea` |
| [#2867](https://github.com/storytold/lightcraft/pull/2867) | Catalog: an upgrade keeps the files it replaces | open | `387fafd8dc` | same | `6ec85e5215` |
| [#2868](https://github.com/storytold/lightcraft/pull/2868) | LRC-LIB-LIBFOLDERS: an import batch changes only the Folders rows it touches (stacked on #2047) | open | `dca1c334a1` | same | `8352b96082` |
| [#2869](https://github.com/storytold/lightcraft/pull/2869) | LR-BEHAV-BG-TASKS: queue a scanned folder without its review while the library lane is busy (stacked on #1943) | open | `e1f1400601` | same | `b2442ec0bd` |
| [#2871](https://github.com/storytold/lightcraft/pull/2871) | Catalog: the store can write files that a later write names, list its files and sync folders it can | open | `254e30e4cf` | moved to `d89738086c` | `5639a8b4ed` |
| [#2872](https://github.com/storytold/lightcraft/pull/2872) | LR-BEHAV-BG-TASKS: queued imports and syncs name their folder, the dialogs say when they queue, a refused drop says why, switching libraries asks (stacked on #1943) | open | `c35cb684b9` | same | `493807b02f` |
| [#2873](https://github.com/storytold/lightcraft/pull/2873) | LR-IMP-FORMATS: a JPEG cut short is marked in the import review, the import message, Info and a smart-album rule (stacked on #2705) | open | `6e91ca04e5` | same | `e7cc5a109d` |
| [#2874](https://github.com/storytold/lightcraft/pull/2874) | LR-PREF-GENERAL: Settings has a filter that finds a section on whatever tab it is | open | `9547cdcebd` | moved to `b3913f6de7` | `2ac8ab9325` |
| [#2875](https://github.com/storytold/lightcraft/pull/2875) | LRC-LIB-MISSING: the Missing Photos count checks each file once in the background (stacked on #2464) | open | `436edf06a3` | same | `08034ea2b9` |
| [#2877](https://github.com/storytold/lightcraft/pull/2877) | LRC-LIB-MISSING: the Missing Photos count follows each file found gone or back by itself (stacked on #2875) | open | `1dbdd1ce86` | same | `8678a44569` |
| [#2878](https://github.com/storytold/lightcraft/pull/2878) | LR-BEHAV-EMPTY-STATES: a new library asks how to begin, and a banner says when sample photos are in (stacked on #1781) | open | `21b54eaeef` | same | `08307fa763` |
| [#2880](https://github.com/storytold/lightcraft/pull/2880) | LR-BEHAV-EMPTY-STATES: the welcome has the window, with sample photos to open (stacked on #2878) | open | `29adf1aace` | same | `630bdbbe05` |
| [#2884](https://github.com/storytold/lightcraft/pull/2884) | LRC-LIB-LIBFOLDERS: ✕ stops a move to another folder after the file in hand (stacked on #1925) | open | `033e4bd923` | same | `8cac9d7382` |
| [#2886](https://github.com/storytold/lightcraft/pull/2886) | LR-BEHAV-BG-TASKS: an import or scan reads on the detached readers, so ✕ ends it at once and an agent can skip the stalled file (stacked on #2378) | open | `b91cf23169` | same | `8774f669d6` |
| [#2887](https://github.com/storytold/lightcraft/pull/2887) | LR-BEHAV-BG-TASKS: Synchronize Folder reads on the detached readers too, and a scan that stopped imports only the new files (stacked on #2886) | open | `d05f6cff9f` | same | `fc78f72693` |
| [#2888](https://github.com/storytold/lightcraft/pull/2888) | LR-BEHAV-BG-TASKS: every scan row names the file it waits for and offers Skip (stacked on #2887) | open | `53397201a6` | same | `8403dd2101` |
| [#2891](https://github.com/storytold/lightcraft/pull/2891) | LR-BEHAV-EMPTY-STATES: the welcome reads for everyone (stacked on #2880) | open | `d4c0a4f3a7` | same | `ce6e099f48` |
| [#2892](https://github.com/storytold/lightcraft/pull/2892) | LR-BEHAV-EMPTY-STATES: every way to the samples leads to the editor, which invites a first edit (stacked on #2891) | open | `c110e30148` | same | `756599657d` |
| [#2895](https://github.com/storytold/lightcraft/pull/2895) | LRC-LIB-LIBFOLDERS: a folder that is somewhere else now can be located from the app (stacked on #2860) | open | `dd09f60272` | same | `b563667f81` |
| [#2897](https://github.com/storytold/lightcraft/pull/2897) | LRC-LIB-LIBFOLDERS: dropping photos on a folder asks about what can't move first (stacked on #2721) | open | `11f67588f9` | same | `4f336e029f` |
| [#2898](https://github.com/storytold/lightcraft/pull/2898) | LR-PREF-INTERFACE: text is drawn in the seven sizes, so no two sizes are half a point apart (stacked on #1374) | open | `8c1905c032` | same | `57a0b3dbf3` |
| [#2899](https://github.com/storytold/lightcraft/pull/2899) | LR-PREF-INTERFACE: what the interface writes stands 4.5:1 in every theme (stacked on #2898) | open | `c2d2f558f3` | same | `aac62713ac` |
| [#2900](https://github.com/storytold/lightcraft/pull/2900) | LR-PREF-INTERFACE: marks and words over a picture are in one light ink on a shade (stacked on #2899) | open | `1d56d59d2b` | same | `10a30590c6` |
| [#2901](https://github.com/storytold/lightcraft/pull/2901) | LR-PREF-INTERFACE: an exposure reads one way, and values stand in columns (stacked on #2900) | open | `b069b62ec9` | same | `245308e6b7` |
| [#2902](https://github.com/storytold/lightcraft/pull/2902) | LR-BEHAV-LOCALIZE: longer words keep to their place in Edit, the choices and the filter bar (stacked on #2901) | open | `35751ffa96` | same | `25ad2915ad` |
| [#2903](https://github.com/storytold/lightcraft/pull/2903) | LR-PREF-INTERFACE: emphasis shows, a value is no larger than its header, an empty view's words stay in it (stacked on #2902) | closed | `40e911532f` | same | same patches in build |
| [#2904](https://github.com/storytold/lightcraft/pull/2904) | LR-PREF-INTERFACE: cargo xtask type refuses a text size written as a number, and CLAUDE.md names the rule (stacked on #2902) | open | `f2d39b1d94` | same | `96ae42fa55` |
| [#2905](https://github.com/storytold/lightcraft/pull/2905) | LR-PREF-GENERAL: the Settings filter finds what is typed, best first (stacked on #2874) | open | `a94199f992` | same | `f28609b482` |
| [#2906](https://github.com/storytold/lightcraft/pull/2906) | LR-PREF-GENERAL: letters the Settings filter found are marked behind a word, not cut out of it (stacked on #2905) | open | `5ad9732ee3` | same | `17cec7cfe4` |
| [#2907](https://github.com/storytold/lightcraft/pull/2907) | LR-BEHAV-LOCALIZE: the translation report lists English shown without tr() (stacked on #2703) | open | `69d7f19e65` | same | `8b2513fecd` |
| [#2911](https://github.com/storytold/lightcraft/pull/2911) | LR-BEHAV-ACCESS: a control Tab skips, or that shows no focus, fails the tests (stacked on #2704) | open | `0864e58024` | same | `02636ff94f` |
| [#2914](https://github.com/storytold/lightcraft/pull/2914) | LRC-LIB-LIBFOLDERS: a move writes a record in the library before the first file moves (stacked on #1925) | open | `45195eb8e5` | same | `928004583c` |
| [#2915](https://github.com/storytold/lightcraft/pull/2915) | LRC-LIB-LIBFOLDERS: an "Unfinished moves" window says what the next open found of a move cut short (stacked on #3119) | open | `715041137f` | same | `a8d81f74c1` |
| [#2927](https://github.com/storytold/lightcraft/pull/2927) | LR-LIB-DELETE: the plan and the work of Move Files to Trash, run by tests only (stacked on #1158 and #2649) | open | `7491829ad5` | same | `3c7d3f9130` |
| [#2931](https://github.com/storytold/lightcraft/pull/2931) | LR-LIB-DELETE: photo.trashFiles moves photos' files to the system Trash (stacked on #2927) | open | `6f4a3feddc` | same | `aefa8f7ad7` |
| [#2934](https://github.com/storytold/lightcraft/pull/2934) | LR-LIB-DELETE: Move Files to Trash can run in the background, and the commands that move files wait while it does (stacked on #2931) | open | `7216d3aa1b` | same | `375139edca` |
| [#2935](https://github.com/storytold/lightcraft/pull/2935) | LRC-LIB-LIBFOLDERS: a folder's menu has one grouped order in Folders and Local | open | `da47dc6552` | same | `0a1679acaf` |
| [#2941](https://github.com/storytold/lightcraft/pull/2941) | LR-META-XMP: Save and Read Metadata can run on a whole folder, with a row in the activity stack | open | `edbd915fac` | same | `8174a2d039` |
| [#2943](https://github.com/storytold/lightcraft/pull/2943) | LR-LIB-DELETE: Move Files to Trash… asks, moves the files in the background, and asks again before erasing (stacked on #2934) | open | `5c18e00f3d` | same | `4027233e20` |
| [#2945](https://github.com/storytold/lightcraft/pull/2945) | Issue #1184: the template allows a tested core whose caller comes in the next pull request | open | `927e8b3ee3` | same | `4da0519a5e` |
| [#2946](https://github.com/storytold/lightcraft/pull/2946) | LR-LIB-FOLDER: cargo xtask refusals keeps new silent command calls out of the interface (stacked on #777) | open | `e7d34cc018` | same | `4e07336f49` |
| [#2948](https://github.com/storytold/lightcraft/pull/2948) | PERF-CATALOG: a library states what its missing develop settings read as (stacked on #2867) | open | `9fc9453a4b` | same | `8987e0606b` |
| [#2949](https://github.com/storytold/lightcraft/pull/2949) | LR-PREF-GENERAL: the Settings tab bar, filter, choices and questions can be read in every theme (stacked on #2906) | open | `8542b1db26` | same | `28e3ce08d8` |
| [#2951](https://github.com/storytold/lightcraft/pull/2951) | PERF-CATALOG: the catalog formats are one table, and every format from 6 on has a fixture (stacked on #2948) | open | `4decee375e` | same | `a3955650f4` |
| [#2956](https://github.com/storytold/lightcraft/pull/2956) | LRC-LIB-LIBFOLDERS: a folder can be exported from its menu (stacked on #2935) | open | `3ba099f22c` | same | `85ba313c8c` |
| [#2957](https://github.com/storytold/lightcraft/pull/2957) | PERF-CATALOG: develop settings can be stored as their difference from the defaults (stacked on #2951) | open (draft) | `a340c332f5` | same | `3ce198cdfc` |
| [#2959](https://github.com/storytold/lightcraft/pull/2959) | LRC-LIB-LIBFOLDERS: every folder menu can copy the folder's path and make a folder on disk (stacked on #2935) | open | `6a4a389b66` | same | `3582ced247` |
| [#2961](https://github.com/storytold/lightcraft/pull/2961) | LRC-LIB-LIBFOLDERS: a folder's menu can bring photos in (stacked on #2935) | open | `ab9f915f31` | same | `3e28577a51` |
| [#2968](https://github.com/storytold/lightcraft/pull/2968) | PERF-CATALOG: catalog records state only what was set (catalog format 10) (stacked on #2957) | open (draft) | `2a51d02806` | same | `47b17eeee4` |
| [#2971](https://github.com/storytold/lightcraft/pull/2971) | LR-IMP-DUPES: a photo browsed in Local is offered by Import and no longer blocks the library's own file | open | `aaeb9505c0` | moved to `40d3e6031a` | `7c4ee8b819` |
| [#2973](https://github.com/storytold/lightcraft/pull/2973) | MENU-BAR: clippy passes on macOS again (Quit, About and Settings share one branch) | closed | `f780d66842` | same | `dfbb3a0637` |
| [#2978](https://github.com/storytold/lightcraft/pull/2978) | PERF-CATALOG: the segments of a snapshot, with no caller yet (stacked on #2968 and #2871) | open (draft) | `50b6190b13` | same | `d8bcffaadb` |
| [#2983](https://github.com/storytold/lightcraft/pull/2983) | PERF-CATALOG: the catalog saves its photos in segment files and opens them on several threads (catalog format 11, stacked on #2978) | open (draft) | `038f346360` | same | `913fd670d7` |
| [#2985](https://github.com/storytold/lightcraft/pull/2985) | PERF-CATALOG: a compaction writes only the segments whose photos changed (stacked on #2983) | open (draft) | `870cdea9a6` | same | `dd5305f1e6` |
| [#3014](https://github.com/storytold/lightcraft/pull/3014) | MENU-CTX-GRID: the photo menu's rows, built from the menu bar's model | open | `1e316418fd` | same | `2daffee591` |
| [#3026](https://github.com/storytold/lightcraft/pull/3026) | MENU-CTX-GRID: the photo menu offers the Photo menu's actions, grouped by task (stacked on #3014) | open | `22823e1786` | same | `1eb809cd4a` |
| [#3093](https://github.com/storytold/lightcraft/pull/3093) | LRC-LIB-KEYWORDS, LRC-LIB-LIBFOLDERS: undo and redo carry what a keyword, person or folder step did to the filter (stacked on #3157) | open | `599876a765` | same | `3615d972cc` |
| [#3106](https://github.com/storytold/lightcraft/pull/3106) | LR-BEHAV-MULTISELECT: Cmd/Ctrl+click and Shift+click choose several albums, folders, keywords or dates in the sidebars (stacked on #1153) | open | `137bbfadca` | moved to `84630e165a` | `56bc829db5` |
| [#3107](https://github.com/storytold/lightcraft/pull/3107) | LR-BEHAV-MULTISELECT: the menu, dragging and Delete key of several chosen sidebar rows act on all of them (stacked on #3106 and #2757) | open | `9c823dcab0` | same | `36c8eea3bc` |
| [#3108](https://github.com/storytold/lightcraft/pull/3108) | LR-BEHAV-MULTISELECT: Cut, Copy and Paste for albums and keywords, and album.duplicate (stacked on #3107) | open | `8544f64f9c` | moved to `b94f69e49e` | `daba3a61b9` |
| [#3115](https://github.com/storytold/lightcraft/pull/3115) | LR-BEHAV-MULTISELECT: batch keyword delete and move look keywords up by level instead of scanning the selection (stacked on #2757) | open | `aaf93a28ec` | same | `f6bdc628b8` |
| [#3119](https://github.com/storytold/lightcraft/pull/3119) | LRC-LIB-LIBFOLDERS: a move cut short by a force-quit or a crash is followed at the next open (stacked on #2914) | open | `84c693f4d0` | same | `280e180c2d` |
| [#3150](https://github.com/storytold/lightcraft/pull/3150) | LR-BEHAV-BG-TASKS: exports and imports say how they ended (stacked on #1711) | open | `94dfa31325` | same | `753abe3b73` |
| [#3151](https://github.com/storytold/lightcraft/pull/3151) | LR-BEHAV-BG-TASKS: every other job says how it ended (stacked on #3150) | open | `fd07728df4` | same | `33ca69cea5` |
| [#3156](https://github.com/storytold/lightcraft/pull/3156) | LR-BEHAV-TEXTFIELD: an edit in progress is saved however its field is left, except Esc | open | `6bf0ae88d4` | same | `c1a1165253` |
| [#3157](https://github.com/storytold/lightcraft/pull/3157) | LRC-LIB-FILTERBAR: every filter change goes through one step that leaves out what the source shown owns | open | `b5cc5f5630` | same | `a6d4cc60ec` |
| [#3171](https://github.com/storytold/lightcraft/pull/3171) | MENU-APP-UPDATES: --no-update-check skips the startup update check | open | `5072eaaf44` | same | `31d9ae7212` |
| [#3179](https://github.com/storytold/lightcraft/pull/3179) | LR-BEHAV-SIDEBAR-KEYS: the keys' model for the left sidebar: the mode, its cursor and its ring (no caller yet) | open | `2057121dbf` | same | `b7d6588728` |
| [#3180](https://github.com/storytold/lightcraft/pull/3180) | LR-BEHAV-SIDEBAR-KEYS: the left sidebar from the keyboard (stacked on #3179) | open | `def85332eb` | same | `3af1cd3a6f` |
| [#3201](https://github.com/storytold/lightcraft/pull/3201) | LR-BEHAV-LOCALE: the Edit menu drops its 18-row Language submenu; Settings has the choice | open | `b639b4054e` | moved to `0a651809bd` | `83cb8863b7` |
| [#3202](https://github.com/storytold/lightcraft/pull/3202) | MENU-HELP-HELP: on macOS Check for Updates… is in the app menu, and Help opens with LightCraft Help (⇧⌘?) | open | `5cdf2bfea4` | same | `48189e7db9` |
| [#3203](https://github.com/storytold/lightcraft/pull/3203) | MENU-WINDOW-BRINGFRONT: the macOS Window menu lists the open windows | open | `dffbd44835` | moved to `ae551fe6bc` | `6f91da3f7f` |
| [#3204](https://github.com/storytold/lightcraft/pull/3204) | LR-BEHAV-TITLEBAR: center the macOS traffic lights on the top bar | open | `52d65cbf0b` | same | `fc5e6b2cce` |
| [#3208](https://github.com/storytold/lightcraft/pull/3208) | LR-BEHAV-BG-TASKS: on macOS a running job keeps the Mac awake and the app out of App Nap | open | `03eb04be63` | same | `6bae47d655` |
| [#3211](https://github.com/storytold/lightcraft/pull/3211) | LR-BEHAV-ACCESS: Increase contrast follows macOS until you choose (stacked on #2373) | open | `51eddad39c` | same | `17433bfb29` |
| [#3215](https://github.com/storytold/lightcraft/pull/3215) | LR-BEHAV-ACCESS: Reduce transparency follows macOS until you choose (stacked on #3211) | open | `c948cd6917` | same | `72a914030f` |
| [#3216](https://github.com/storytold/lightcraft/pull/3216) | LR-BEHAV-ACCESS: a photo thumbnail reads the same in the grid and the filmstrip: a selectable cell named with its rating, flag and label | open | `4dff333262` | same | `1e09dc72d3` |
| [#3217](https://github.com/storytold/lightcraft/pull/3217) | MENU-HELP-FEEDBACK: the macOS Help menu has one feedback row, LightCraft's | open | `ba87b152f8` | moved to `35743dabe4` | `d00491b2f7` |
| [#3218](https://github.com/storytold/lightcraft/pull/3218) | MENU-VIEW-ENTERFULLSCREEN: the macOS View menu has one window full-screen row, Enter or Exit Full Screen on ⌃⌘F | open | `1d037817ab` | same | `dfde82f78c` |
| [#3219](https://github.com/storytold/lightcraft/pull/3219) | LR-BEHAV-BG-TASKS: on macOS the Dock icon shows how many jobs are running (stacked on #3208) | open | `c8c2a3007a` | same | `d23c876379` |
| [#3223](https://github.com/storytold/lightcraft/pull/3223) | LR-BEHAV-BG-TASKS: on macOS a notification tells you when a job has finished (stacked on #3219) | open | `f550887ed5` | same | `59bf4f80f4` |
| [#3243](https://github.com/storytold/lightcraft/pull/3243) | LR-VIEW-FILMSTRIP: a stack badge's tooltip names the stack toggle's key as bound (stacked on #2164) | open | `9e1b8d8086` | same | `e7ee9255f8` |
| [#3255](https://github.com/storytold/lightcraft/pull/3255) | LR-BEHAV-LOCALE: Settings ▸ General ▸ Language can sync with the system again (stacked on #3146) | open | `952d7f7980` | same | `ffe045ee96` |
| [#3257](https://github.com/storytold/lightcraft/pull/3257) | LR-BEHAV-TEXTFIELD: emoji are in color on macOS | open (draft) | `e80bde4583` | same | `36c9998217` |
| [#3259](https://github.com/storytold/lightcraft/pull/3259) | LR-BEHAV-ACCESS: a button that does something reads as a button and one of several choices as a radio button (stacked on #2078) | open | `895a4689b5` | same | `48b70a5766` |
| [#3261](https://github.com/storytold/lightcraft/pull/3261) | LR-BEHAV-EMPTY-STATES: the grid's title, count and empty-view messages are read by screen readers (stacked on #2891) | closed | `9d28c5354a` | same | head in build |
| [#3287](https://github.com/storytold/lightcraft/pull/3287) | LR-BEHAV-BG-TASKS: an Auto Import taken out of the waiting line gives its files back (stacked on #1943) | open | `1885869801` | same | `735a787592` |
| [#3288](https://github.com/storytold/lightcraft/pull/3288) | MENU-VIEW-SORT: the Sort menus offer only what the view uses | open | `f256472e6d` | same | `117c6d7c5b` |
| [#3289](https://github.com/storytold/lightcraft/pull/3289) | LR-FILT-SORT: a new sort order or reshuffle keeps the active photo in view | open | `f412831402` | same | `a7aa730e84` |
| [#3290](https://github.com/storytold/lightcraft/pull/3290) | LR-FILT-SORT: a shuffled grid says so in its header, with Reshuffle beside it | open | `c06e0d3c25` | same | `8c878d249f` |
| [#3291](https://github.com/storytold/lightcraft/pull/3291) | LR-BEHAV-LOCALIZE: the translation report keeps same-named helpers of different files apart and reads a slider's text (stacked on #2907) | open | `469d3ab511` | same | `e022b1d90d` |
| [#3294](https://github.com/storytold/lightcraft/pull/3294) | LR-FILT-SORT: Previous Shuffle and a Reshuffle shortcut (stacked on #3288) | open | `cc6fe89ed1` | same | `044c3fd6c9` |
| [#3308](https://github.com/storytold/lightcraft/pull/3308) | LR-BEHAV-TEXTFIELD: a text box's edit is saved to what it was typed for, and a save run at close ends it | open | `566d133437` | same | `d2ec8ce1d5` |
| [#3341](https://github.com/storytold/lightcraft/pull/3341) | LR-BEHAV-BG-TASKS: starting a background export no longer freezes the window | open | `37d329d6a8` | same | `902884ee94` |
| [#3343](https://github.com/storytold/lightcraft/pull/3343) | LR-IMP-LOCAL: Choose… and Browse Folder… open their file dialog without freezing the window | open | `14e52f984c` | same | `9d1076a877` |
| [#3344](https://github.com/storytold/lightcraft/pull/3344) | LRC-DEV-VIEWOPTIONS: a render that stays slow says "Rendering…" even with the loading message off | open | `22d997b4d1` | same | `21b20411e8` |
| [#3345](https://github.com/storytold/lightcraft/pull/3345) | LR-VIEW-PHOTOGRID: a thumbnail still loading shows a quiet cue, unlike an unreadable one | open | `0a3c55fee7` | same | `ecb589e0ac` |
| [#3348](https://github.com/storytold/lightcraft/pull/3348) | LR-META-XMP: automatic XMP writes waiting for their folder show a row in the activity stack | open | `21405283f1` | same | `438edd4c3a` |
| [#3351](https://github.com/storytold/lightcraft/pull/3351) | LR-BEHAV-BG-TASKS: AI Denoise and Finding faces can be stopped, and Quit names every running job | open | `2f5262ea95` | same | `655f5c8441` |
| [#3361](https://github.com/storytold/lightcraft/pull/3361) | LR-IMP-LOCAL: the print, display, slideshow template and soft proof file dialogs open without freezing the window (stacked on #3343) | open | `be04d71738` | same | `b51619dcd0` |
| [#3363](https://github.com/storytold/lightcraft/pull/3363) | LRC-LIB-LIBFOLDERS: Synchronize several folders at once, one after another (stacked on #3107 and #1943) | open | `3008ef34d9` | same | `995ce3c8f0` |
| [#3382](https://github.com/storytold/lightcraft/pull/3382) | PERF-LIB: the source total follows an edit instead of counting every photo again | open | `06d747cd57` | same | `0c175ca4eb` |
| [#3387](https://github.com/storytold/lightcraft/pull/3387) | LR-LIB-SMARTALBUM: albums.list answers at once from the counts the sidebar keeps (stacked on #2020) | open | `9de93772a5` | same | `7d5c0aa033` |
| [#3395](https://github.com/storytold/lightcraft/pull/3395) | LR-LIB-FOLDER: albums.list counts a folder from the counts the sidebar keeps (stacked on #768 and #3387) | open | `e048d414fd` | same | `20200ce5ad` |
| [#3688](https://github.com/storytold/lightcraft/pull/3688) | LR-LIB-RATING: every rating, flag and label key confirms with a toast, with the count when several photos change | open | `cb00ae47a6` | same | `b34a73f4d5` |
| [#3692](https://github.com/storytold/lightcraft/pull/3692) | KEY-RATING: rating and label keys can be remapped, and French AZERTY digit rows rate without Shift | open | `b4780a7a66` | same | `737ace2280` |
| [#3716](https://github.com/storytold/lightcraft/pull/3716) | LR-LIB-FLAG: the photo menu's rating and flag rows and P / U with Crop open use the culling path | open | `5f89e2c5dc` | same | `195df62570` |
| [#3812](https://github.com/storytold/lightcraft/pull/3812) | LRC-LIB-MISSING: the file-availability cache stops growing with the library | open | `7febc44d26` | same | `7248a97a77` |
| [#3835](https://github.com/storytold/lightcraft/pull/3835) | PERF-LIB: file names are compared byte by byte while they are ASCII | open | `655de1d850` | same | `97136c97b6` |

## Other contributors' PRs (2)

| PR | Title | State | Taken at | Head now | In the build through |
|---|---|---|---|---|---|
| [#2822](https://github.com/storytold/lightcraft/pull/2822) | LR-VID-PLAY: MP4/MOV video import and cataloguing with duration and container badges | open | `2368c54ec9` | same | `3b58acb8da` |
| [#3018](https://github.com/storytold/lightcraft/pull/3018) | perf(catalog): box Photo::embedded_lens (~240 MB less per 1M photos) | open | `3da201b8c5` | moved to `5721021c5b` | `90fed4c68d` |

Not listed: my integration PRs [#3162](https://github.com/storytold/lightcraft/pull/3162), [#3170](https://github.com/storytold/lightcraft/pull/3170), [#3195](https://github.com/storytold/lightcraft/pull/3195), each a bundle whose members are listed on their own.
