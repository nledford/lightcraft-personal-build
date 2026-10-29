# PRs in the `integration` build

The `integration` branch (the build I run) is LightCraft `main` at `77c0a6adfa` plus the open pull requests below, merged or ported together, plus integration-only fixes where two of them meet. Source: `integration` at `4f216e6b58` (built 27 Oct 2026); listed 2026-10-29 18:45 UTC. This list is replaced whenever a new build is swapped in.

How each PR was found: a commit in the build that merges, ports, applies or refreshes it (the commit is named), its head being in the build's history ("head in build"), all of its commits being in the build by patch ("same patches in build"), or another listed PR carrying its commits ("carried by"). A PR whose own work was taken out again (#1933's queue, replaced by #2806, #2869 and #2872) is not listed.

## My PRs (159)

| PR | Title | Author | State | In the build through |
|---|---|---|---|---|
| [#768](https://github.com/storytold/lightcraft/pull/768) | LR-LIB-FOLDER: a folder of albums shows the photos of the albums inside it | @nledford | open | `4f599109a9` |
| [#775](https://github.com/storytold/lightcraft/pull/775) | LR-LIB-FOLDER: the view never shows an album that is gone | @nledford | open | `8d0187c3f1` |
| [#776](https://github.com/storytold/lightcraft/pull/776) | LR-LIB-FOLDER: smart albums in album folders keep off loops (stacked on #768) | @nledford | open (draft) | `eb3b21253f` |
| [#777](https://github.com/storytold/lightcraft/pull/777) | LR-LIB-FOLDER: a refused command says why | @nledford | open | `2602619cfc` |
| [#800](https://github.com/storytold/lightcraft/pull/800) | Catalog: the log is compacted by the snapshot's size, not every 2000 records | @nledford | open | `ce7a3ba4ca` |
| [#823](https://github.com/storytold/lightcraft/pull/823) | Catalog: settings saved as `{}` are not read, and a library's own defaults are read over (stacked on #814) | @nledford | open (draft) | `da338477bf` |
| [#841](https://github.com/storytold/lightcraft/pull/841) | Catalog: back up the library to a dated folder that shares unchanged files (stacked on #814) | @nledford | open (draft) | `6b5ca7e77e` |
| [#856](https://github.com/storytold/lightcraft/pull/856) | Catalog: restore a library from a backup, and keep what it replaces (stacked on #841) | @nledford | open (draft) | `5fe4b3fd57` |
| [#870](https://github.com/storytold/lightcraft/pull/870) | Catalog: restore from the File menu, with the backup copied in the background (stacked on #856) | @nledford | open (draft) | `b5307b304a` |
| [#1004](https://github.com/storytold/lightcraft/pull/1004) | Catalog: the backup folder is shown and chosen in Settings (stacked on #870) | @nledford | open (draft) | `309104ce79` |
| [#1025](https://github.com/storytold/lightcraft/pull/1025) | Catalog: a library is backed up by itself while it is open, and when LightCraft quits (stacked on #1004) | @nledford | open (draft) | `57efe8a2f4` |
| [#1034](https://github.com/storytold/lightcraft/pull/1034) | Catalog: a library can keep its backups for a time, and old ones go after a new one is made (stacked on #1025) | @nledford | open (draft) | `90d7ec7d93` |
| [#1043](https://github.com/storytold/lightcraft/pull/1043) | Catalog: Restore Library from Backup lists the library's backups to choose from (stacked on #1034) | @nledford | open (draft) | `b395f673de` |
| [#1054](https://github.com/storytold/lightcraft/pull/1054) | Catalog: what restores replaced goes with the old backups of a library that keeps them for a time (stacked on #1034) | @nledford | open (draft) | `c1c4b39308` |
| [#1075](https://github.com/storytold/lightcraft/pull/1075) | Backup and restore: the smaller items left on #870's list (stacked on #1054) | @nledford | open (draft) | `8f2c7fd484` |
| [#1095](https://github.com/storytold/lightcraft/pull/1095) | Import: what the photos themselves carry can be left in the files | @nledford | open | `da7bce2152` |
| [#1124](https://github.com/storytold/lightcraft/pull/1124) | Synchronize Folder: the scan can continue in the background | @nledford | open | `719dd72ed3` |
| [#1127](https://github.com/storytold/lightcraft/pull/1127) | Synchronize Folder: a Read row for the new photos, and dialogs remember theirs (stacked on #1095) | @nledford | open | `c0f1378214` |
| [#1153](https://github.com/storytold/lightcraft/pull/1153) | Library: show several albums, folders, keywords or dates at once (`library.source {ids, paths}`; engine only) | @nledford | open | `8e6fb5184c` |
| [#1158](https://github.com/storytold/lightcraft/pull/1158) | LR-LIB-DELETE: the engine can move a file to the system Trash and put it back | @nledford | open | `e9cbd40186` |
| [#1243](https://github.com/storytold/lightcraft/pull/1243) | LRC-LIB-LIBFOLDERS: Folders skips the folders that only lead to your photos, with Show Parent Folder to list them | @nledford | open | `aef1f3426e` |
| [#1244](https://github.com/storytold/lightcraft/pull/1244) | Sidebar: the row of what is shown has a bar, and a folded section says what it holds | @nledford | open | `eb659c54f2` |
| [#1245](https://github.com/storytold/lightcraft/pull/1245) | Sidebar: the left sidebar from the keyboard | @nledford | closed | `76e3a5eccb` |
| [#1303](https://github.com/storytold/lightcraft/pull/1303) | Settings: Backups says where the library stands, with Back Up Now (stacked on #1075) | @nledford | open (draft) | `e52639a05a` |
| [#1374](https://github.com/storytold/lightcraft/pull/1374) | LR-PREF-INTERFACE: text sizes have names, the seven type roles of the theme | @nledford | open | `fbe37a31e0` |
| [#1626](https://github.com/storytold/lightcraft/pull/1626) | LR-BEHAV-SIDEBAR-COLLAPSE: the sidebar's trees open as they were left | @nledford | open | `d121dea4fe` |
| [#1642](https://github.com/storytold/lightcraft/pull/1642) | LR-BEHAV-BG-TASKS: running tasks live under the top bar's bell, not over the top-left corner | @nledford | open | `3bd67720d2` |
| [#1649](https://github.com/storytold/lightcraft/pull/1649) | LR-BEHAV-BG-TASKS: an export or import that ended stays in the list under the bell (stacked on #1642 and #3151) | @nledford | open | `1699e550f7` |
| [#1711](https://github.com/storytold/lightcraft/pull/1711) | LR-BEHAV-BG-TASKS: a background task says how it ended; the engine's own jobs do (part 1 of 3) | @nledford | open | `2a09e0afa7` |
| [#1781](https://github.com/storytold/lightcraft/pull/1781) | LR-BEHAV-EMPTY-STATES: Help ▸ Add Sample Photos and Remove Sample Photos | @nledford | open | `5e0d3412a6` |
| [#1925](https://github.com/storytold/lightcraft/pull/1925) | LRC-LIB-LIBFOLDERS: photos move to another folder in the background | @nledford | open | `412892f29e` |
| [#1936](https://github.com/storytold/lightcraft/pull/1936) | LR-BEHAV-TOAST: toasts stack instead of replacing one another, and a new state replaces the old one | @nledford | open | `a108757bc8` |
| [#1943](https://github.com/storytold/lightcraft/pull/1943) | LR-BEHAV-BG-TASKS: imports and Synchronize Folder wait for each other instead of refusing (stacked on #2806) | @nledford | open | carried by #2872 |
| [#2020](https://github.com/storytold/lightcraft/pull/2020) | LR-LIB-SMARTALBUM: creating or giving a keyword no longer freezes the window while albums are counted | @nledford | open | `2597951ecd` |
| [#2034](https://github.com/storytold/lightcraft/pull/2034) | LRC-LIB-LIBFOLDERS: Synchronize Folder can bring in files already in the library elsewhere, and remembers the choice | @nledford | open | `5f3108c027` |
| [#2043](https://github.com/storytold/lightcraft/pull/2043) | LRC-LIB-SMARTCOLL: a Same Content as Another Photo rule finds photos whose file is in the library twice | @nledford | open | `ba4595404f` |
| [#2047](https://github.com/storytold/lightcraft/pull/2047) | LRC-LIB-LIBFOLDERS: importing into a large library keeps the window responsive | @nledford | open | `07a63435bd` |
| [#2075](https://github.com/storytold/lightcraft/pull/2075) | LR-BEHAV-DIALOGS: dialog confirm buttons name their action | @nledford | open | `e1d08e05f5` |
| [#2078](https://github.com/storytold/lightcraft/pull/2078) | LR-BEHAV-ACCESS: one tick box, radio and toggle button for every choice | @nledford | open | `3f6b87d37d` |
| [#2079](https://github.com/storytold/lightcraft/pull/2079) | LR-BEHAV-ACCESS: action buttons share one Button component | @nledford | open | `7fac14776d` |
| [#2080](https://github.com/storytold/lightcraft/pull/2080) | MENU-BAR: one menu row and shortcut component for every menu | @nledford | open | `8fdd811b81` |
| [#2162](https://github.com/storytold/lightcraft/pull/2162) | LR-VIEW-PHOTOGRID: the active photo's selection ring shows on light themes and differs from the other selected photos by weight | @nledford | open | `3dc3a0fc4c` |
| [#2164](https://github.com/storytold/lightcraft/pull/2164) | LR-VIEW-FILMSTRIP: filmstrip and grid thumbnails share one overlay, and it degrades on narrow tiles | @nledford | open | `682e0ea666` |
| [#2194](https://github.com/storytold/lightcraft/pull/2194) | MENU-BAR: every command has a place in the menus, and View drops from 62 rows to 36 | @nledford | open | `87e581001d` |
| [#2204](https://github.com/storytold/lightcraft/pull/2204) | LR-BEHAV-DRAGDROP: photo drags say what a drop will do, refuse what it can't, and Esc gives them up | @nledford | open | `fd3d893f05` |
| [#2370](https://github.com/storytold/lightcraft/pull/2370) | LR-BEHAV-TOAST: toasts stay long enough to read, hold under the pointer, and errors stay 6 s | @nledford | open | `b657113dbb` |
| [#2373](https://github.com/storytold/lightcraft/pull/2373) | LR-BEHAV-ACCESS: Reduce motion stops zoom, scroll and spinner animations, and follows macOS until you choose | @nledford | open | `84ba662591` |
| [#2378](https://github.com/storytold/lightcraft/pull/2378) | LR-BEHAV-BG-TASKS: detached readers for an import or scan (stall, skip, cap), tested and not used yet | @nledford | open | `39c7af9b6a` |
| [#2414](https://github.com/storytold/lightcraft/pull/2414) | PERF-SELECT: a big selection no longer slows every idle frame | @nledford | open | `50c1fc1e6c` |
| [#2415](https://github.com/storytold/lightcraft/pull/2415) | PERF-LIB: big views list from a kept sort order instead of sorting every photo | @nledford | open | `1571c9694e` |
| [#2418](https://github.com/storytold/lightcraft/pull/2418) | PERF-SEARCH: typing in the library search keeps up on a large library | @nledford | open | `7a34a48df0` |
| [#2419](https://github.com/storytold/lightcraft/pull/2419) | PERF-SIDECARS: imports and moves answer sidecar lookups from the folder listing | @nledford | open | `e0f94f4cb7` |
| [#2423](https://github.com/storytold/lightcraft/pull/2423) | PERF-SMART: test a smart album's rules cheapest first | @nledford | open | `b2d67d1b79` |
| [#2459](https://github.com/storytold/lightcraft/pull/2459) | PERF-WALK: a folder walk on a network share lists up to eight folders at the same time | @nledford | open | `52605c4885` |
| [#2464](https://github.com/storytold/lightcraft/pull/2464) | LRC-LIB-MISSING: one rule for Missing Photos' files, and a folder listing that panics no longer stays running | @nledford | open | `b358bd69f1` |
| [#2522](https://github.com/storytold/lightcraft/pull/2522) | LR-PREF-GENERAL: Settings tabs look like tabs, and the keyboard focus shows and moves in one short way | @nledford | open | `e112b672ad` |
| [#2523](https://github.com/storytold/lightcraft/pull/2523) | LR-PREF-GENERAL: Settings asks before Clear Cache, a model's Remove and deleting smart previews | @nledford | open | `9c958e0663` |
| [#2524](https://github.com/storytold/lightcraft/pull/2524) | LR-PREF-GENERAL: the Settings tabs are grouped by what a person comes to do | @nledford | open | `ee394746f8` |
| [#2525](https://github.com/storytold/lightcraft/pull/2525) | LR-PREF-GENERAL: what Settings says can be read in every theme, and in every language it has | @nledford | open | `eff0603bc8` |
| [#2597](https://github.com/storytold/lightcraft/pull/2597) | LRC-LIB-LIBFOLDERS: the folders test fixture waits for the Missing Photos row | @nledford | open | `112c916941` |
| [#2627](https://github.com/storytold/lightcraft/pull/2627) | #1952: the idle-close MCP test reads blocking and waits for the close | @nledford | closed | `c85c40e7b7` |
| [#2629](https://github.com/storytold/lightcraft/pull/2629) | LRC-LIB-MISSING: library.missing from the control channel checks the files off the UI thread | @nledford | open | `03d76fe6ba` |
| [#2647](https://github.com/storytold/lightcraft/pull/2647) | LR-LIB-SMARTALBUM: editing a smart album made from an album view keeps the album | @nledford | open | `620be1cfd8` |
| [#2649](https://github.com/storytold/lightcraft/pull/2649) | LR-LIB-DELETE: the catalog answers "which photos have a file in this folder / of this name" from an index | @nledford | open | `80ccf8d4be` |
| [#2691](https://github.com/storytold/lightcraft/pull/2691) | LR-PREF-GENERAL: Esc in a Settings text box gives the text back instead of closing Settings | @nledford | open | `6252f829d9` |
| [#2693](https://github.com/storytold/lightcraft/pull/2693) | "Favorite" and "Listing folders" in the catalogs that lack them | @nledford | open | `fbc5efc2b3` |
| [#2696](https://github.com/storytold/lightcraft/pull/2696) | LRC-LIB-LIBFOLDERS: a count never covers a name in a narrow sidebar | @nledford | open | `b52a664c9f` |
| [#2697](https://github.com/storytold/lightcraft/pull/2697) | LRC-LIB-FOLDERS: Local says where its listing stops, with a Go Deeper row | @nledford | open | `d743607e41` |
| [#2703](https://github.com/storytold/lightcraft/pull/2703) | LR-BEHAV-LOCALIZE: the translation report also lists plain tr() strings a catalog lacks, wrapped calls included | @nledford | open | `3f7ce21013` |
| [#2704](https://github.com/storytold/lightcraft/pull/2704) | LR-BEHAV-ACCESS: an unnamed control on a main screen fails the tests (today's gaps listed) | @nledford | open | `be69356088` |
| [#2705](https://github.com/storytold/lightcraft/pull/2705) | LR-IMP-FORMATS: a JPEG cut short imports from the data present | @nledford | open | `3e5c7f0436` |
| [#2707](https://github.com/storytold/lightcraft/pull/2707) | LR-BEHAV-MULTISELECT: the model for choosing several sidebar rows with Cmd/Ctrl+click and Shift+click | @nledford | open | `cfbc4cde95` |
| [#2715](https://github.com/storytold/lightcraft/pull/2715) | LR-BEHAV-ACCESS: every color pair the UI paints stands out in every theme (today's gaps listed) | @nledford | open | `f2ad53286f` |
| [#2722](https://github.com/storytold/lightcraft/pull/2722) | LR-VIEW-FILMSTRIP: photos drag from the filmstrip like from the grid | @nledford | open | `1660710993` |
| [#2725](https://github.com/storytold/lightcraft/pull/2725) | LRC-LIB-CATALOG-LOCK: quitting on macOS removes the lock's owner note | @nledford | open | `abe8696c5d` |
| [#2726](https://github.com/storytold/lightcraft/pull/2726) | LR-BEHAV-LOCALIZE: panel layout toasts, face recognition prompts and the second window title are translated | @nledford | open | `1d0a10ae92` |
| [#2734](https://github.com/storytold/lightcraft/pull/2734) | LR-BEHAV-DRAGDROP: the photos being dragged are dimmed in the grid | @nledford | open | `5ebb4c09f8` |
| [#2735](https://github.com/storytold/lightcraft/pull/2735) | LR-BEHAV-DRAGDROP: a drop's destination is named with its parent when another folder or album has the same name | @nledford | open | `6a3cdf487d` |
| [#2736](https://github.com/storytold/lightcraft/pull/2736) | LR-BEHAV-DRAGDROP: the message after a drop that moved, added or tagged photos has an Undo button | @nledford | open | `b36f6ebf3a` |
| [#2747](https://github.com/storytold/lightcraft/pull/2747) | LRC-LIB-LIBFOLDERS: moving a folder refuses a missing destination and says why in plain words | @nledford | open | `99f8ebe130` |
| [#2748](https://github.com/storytold/lightcraft/pull/2748) | LR-CLOUD-SMARTPREVIEW: the smart-previews folder test cleans up the folder it takes offline | @nledford | open | `6e7fad1bbd` |
| [#2749](https://github.com/storytold/lightcraft/pull/2749) | LRC-LIB-LIBFOLDERS: after moving a folder, the message says where it went, in every language | @nledford | open | `bf9398f7b5` |
| [#2751](https://github.com/storytold/lightcraft/pull/2751) | LRC-LIB-LIBFOLDERS: several folders move into one in a single undo step (folder.move paths) | @nledford | open | `481cbdacce` |
| [#2752](https://github.com/storytold/lightcraft/pull/2752) | LR-BEHAV-DRAGDROP: a Folders row dragged onto another moves that folder on disk, after a question | @nledford | open | `4cd3cf4b32` |
| [#2756](https://github.com/storytold/lightcraft/pull/2756) | LR-BEHAV-SIDEBAR-SECTIONS: Option-click on a triangle opens or folds the whole branch | @nledford | open | `3c3808e627` |
| [#2757](https://github.com/storytold/lightcraft/pull/2757) | LR-BEHAV-MULTISELECT: several sidebar albums, keywords or folders are deleted, moved or labeled as one undo step (engine) | @nledford | open | `2cffafa0b0` |
| [#2758](https://github.com/storytold/lightcraft/pull/2758) | KEYC-RATING: deleting the only keyword of the filter Enable Filters put aside leaves filtering on | @nledford | open | `6f30be6348` |
| [#2759](https://github.com/storytold/lightcraft/pull/2759) | KEYC-RATING: Enable Filters brings a filter back without its album when an album is shown | @nledford | closed | `a38cef96ee` |
| [#2766](https://github.com/storytold/lightcraft/pull/2766) | LR-BEHAV-DRAGDROP: only the primary button drags and drops photos from the grid | @nledford | open | `de85fe9dc1` |
| [#2768](https://github.com/storytold/lightcraft/pull/2768) | LR-VIEW-FILMSTRIP: a selected photo's ring shows before its thumbnail loads | @nledford | open | `c0d559c2a3` |
| [#2796](https://github.com/storytold/lightcraft/pull/2796) | docs: gap 9 — the CI gate runs the tests on Linux x64; macOS, Windows and the GPU tests still don't | @nledford | open | `d5dcc50eec` |
| [#2797](https://github.com/storytold/lightcraft/pull/2797) | LR-BEHAV-AUTOSAVE: a change that could only be partly taken back is saved and can be undone | @nledford | open | `9940856ea0` |
| [#2806](https://github.com/storytold/lightcraft/pull/2806) | LR-BEHAV-BG-TASKS: a preview build asked for while another runs can wait in line for it (engine) | @nledford | open | carried by #2872 |
| [#2808](https://github.com/storytold/lightcraft/pull/2808) | LR-BEHAV-DRAGDROP: a right-click while photos are carried opens no context menu | @nledford | open | `9b8b891d53` |
| [#2818](https://github.com/storytold/lightcraft/pull/2818) | LRC-LIB-SMARTCOLL: a smart album on a loop of albums holds no photos, and says which album it loops through | @nledford | open | `a17343f0ce` |
| [#2823](https://github.com/storytold/lightcraft/pull/2823) | LR-BEHAV-DRAGDROP: a right-click on a text field while photos are carried opens no menu | @nledford | open | `24138ed244` |
| [#2825](https://github.com/storytold/lightcraft/pull/2825) | LR-LIB-ALBUM: album commands skip photos the library lacks and record nothing when they add nothing | @nledford | open | `d87cd46719` |
| [#2828](https://github.com/storytold/lightcraft/pull/2828) | LRC-LIB-SMARTCOLL: reorder rules and groups in the smart album editor by drag or arrow keys | @nledford | open | `427aa1d0ca` |
| [#2830](https://github.com/storytold/lightcraft/pull/2830) | LR-BEHAV-MULTISELECT: replacing the selection with a repeated id keeps each photo once | @nledford | open | `b339a6e47a` |
| [#2848](https://github.com/storytold/lightcraft/pull/2848) | LRC-SS-LAYOUT: the Slideshow settings page no longer moves when the slide preview finishes rendering | @nledford | open | `645f7e2614` |
| [#2858](https://github.com/storytold/lightcraft/pull/2858) | LRC-LIB-LIBFOLDERS: Don't ask again when moving photos is kept only when the move started | @nledford | open | `9b1d8f9c3b` |
| [#2860](https://github.com/storytold/lightcraft/pull/2860) | LRC-LIB-LIBFOLDERS: the engine can locate a folder that is somewhere else now (engine only) | @nledford | open | `225c0cde6a` |
| [#2861](https://github.com/storytold/lightcraft/pull/2861) | LRC-SS-PLAYBACK: the slideshow prefetch test no longer races its simulated clock | @nledford | closed | `ac2367f467` |
| [#2867](https://github.com/storytold/lightcraft/pull/2867) | Catalog: an upgrade keeps the files it replaces | @nledford | open | `f5eda4fc3a` |
| [#2868](https://github.com/storytold/lightcraft/pull/2868) | LRC-LIB-LIBFOLDERS: an import batch changes only the Folders rows it touches (stacked on #2047) | @nledford | open | `b4b9c988d7` |
| [#2869](https://github.com/storytold/lightcraft/pull/2869) | LR-BEHAV-BG-TASKS: queue a scanned folder without its review while the library lane is busy (stacked on #1943) | @nledford | open | `4ab66bc8c4` |
| [#2871](https://github.com/storytold/lightcraft/pull/2871) | Catalog: the store can write files that a later write names, list its files and sync folders it can | @nledford | open | `430242902c` |
| [#2872](https://github.com/storytold/lightcraft/pull/2872) | LR-BEHAV-BG-TASKS: queued imports and syncs name their folder, the dialogs say when they queue, a refused drop says why, switching libraries asks (stacked on #1943) | @nledford | open | `a3dc075f40` |
| [#2873](https://github.com/storytold/lightcraft/pull/2873) | LR-IMP-FORMATS: a JPEG cut short is marked in the import review, the import message, Info and a smart-album rule (stacked on #2705) | @nledford | open | `bb106047ef` |
| [#2874](https://github.com/storytold/lightcraft/pull/2874) | LR-PREF-GENERAL: Settings has a filter that finds a section on whatever tab it is | @nledford | open | `683d08f6b9` |
| [#2875](https://github.com/storytold/lightcraft/pull/2875) | LRC-LIB-MISSING: the Missing Photos count checks each file once in the background (stacked on #2464) | @nledford | open | `fd21a8c9b4` |
| [#2877](https://github.com/storytold/lightcraft/pull/2877) | LRC-LIB-MISSING: the Missing Photos count follows each file found gone or back by itself (stacked on #2875) | @nledford | open | `5c4ed5a1c2` |
| [#2878](https://github.com/storytold/lightcraft/pull/2878) | LR-BEHAV-EMPTY-STATES: a new library asks how to begin, and a banner says when sample photos are in (stacked on #1781) | @nledford | open | `d2bfd665aa` |
| [#2880](https://github.com/storytold/lightcraft/pull/2880) | LR-BEHAV-EMPTY-STATES: the welcome has the window, with sample photos to open (stacked on #2878) | @nledford | open | `734001389c` |
| [#2884](https://github.com/storytold/lightcraft/pull/2884) | LRC-LIB-LIBFOLDERS: ✕ stops a move to another folder after the file in hand (stacked on #1925) | @nledford | open | `a8a3541da1` |
| [#2891](https://github.com/storytold/lightcraft/pull/2891) | LR-BEHAV-EMPTY-STATES: the welcome reads for everyone (stacked on #2880) | @nledford | open | `3e908e3f49` |
| [#2892](https://github.com/storytold/lightcraft/pull/2892) | LR-BEHAV-EMPTY-STATES: every way to the samples leads to the editor, which invites a first edit (stacked on #2891) | @nledford | open | `af7474eca7` |
| [#2895](https://github.com/storytold/lightcraft/pull/2895) | LRC-LIB-LIBFOLDERS: a folder that is somewhere else now can be located from the app (stacked on #2860) | @nledford | open | `c3259965b8` |
| [#2897](https://github.com/storytold/lightcraft/pull/2897) | LRC-LIB-LIBFOLDERS: dropping photos on a folder asks about what can't move first (stacked on #2721) | @nledford | open | `035cc66f09` |
| [#2898](https://github.com/storytold/lightcraft/pull/2898) | LR-PREF-INTERFACE: text is drawn in the seven sizes, so no two sizes are half a point apart (stacked on #1374) | @nledford | open | `c2da219f14` |
| [#2899](https://github.com/storytold/lightcraft/pull/2899) | LR-PREF-INTERFACE: what the interface writes stands 4.5:1 in every theme (stacked on #2898) | @nledford | open | `245b226417` |
| [#2900](https://github.com/storytold/lightcraft/pull/2900) | LR-PREF-INTERFACE: marks and words over a picture are in one light ink on a shade (stacked on #2899) | @nledford | open | `2d60150875` |
| [#2901](https://github.com/storytold/lightcraft/pull/2901) | LR-PREF-INTERFACE: an exposure reads one way, and values stand in columns (stacked on #2900) | @nledford | open | `c2830dce1e` |
| [#2902](https://github.com/storytold/lightcraft/pull/2902) | LR-BEHAV-LOCALIZE: longer words keep to their place in Edit, the choices and the filter bar (stacked on #2901) | @nledford | open | `dc84ebf814` |
| [#2903](https://github.com/storytold/lightcraft/pull/2903) | LR-PREF-INTERFACE: emphasis shows, a value is no larger than its header, an empty view's words stay in it (stacked on #2902) | @nledford | open | `8c8bb0ecb5` |
| [#2904](https://github.com/storytold/lightcraft/pull/2904) | LR-PREF-INTERFACE: cargo xtask type refuses a text size written as a number, and CLAUDE.md names the rule (stacked on #2903) | @nledford | open | `ef786e6c44` |
| [#2905](https://github.com/storytold/lightcraft/pull/2905) | LR-PREF-GENERAL: the Settings filter finds what is typed, best first (stacked on #2874) | @nledford | open | `b76eaf52e0` |
| [#2906](https://github.com/storytold/lightcraft/pull/2906) | LR-PREF-GENERAL: letters the Settings filter found are marked behind a word, not cut out of it (stacked on #2905) | @nledford | open | `7c5fc2ed1c` |
| [#2907](https://github.com/storytold/lightcraft/pull/2907) | LR-BEHAV-LOCALIZE: the translation report lists English shown without tr() (stacked on #2703) | @nledford | open | `6edb512d7f` |
| [#2911](https://github.com/storytold/lightcraft/pull/2911) | LR-BEHAV-ACCESS: a control Tab skips, or that shows no focus, fails the tests (stacked on #2704) | @nledford | open | `bebe265a9d` |
| [#2914](https://github.com/storytold/lightcraft/pull/2914) | LRC-LIB-LIBFOLDERS: a move writes a record in the library before the first file moves (stacked on #1925) | @nledford | open | `5f54675a9e` |
| [#2915](https://github.com/storytold/lightcraft/pull/2915) | LRC-LIB-LIBFOLDERS: an "Unfinished moves" window says what the next open found of a move cut short (stacked on #3119) | @nledford | open | `4618c58697` |
| [#2927](https://github.com/storytold/lightcraft/pull/2927) | LR-LIB-DELETE: the plan and the work of Move Files to Trash, run by tests only (stacked on #1158 and #2649) | @nledford | open | `0e52302568` |
| [#2931](https://github.com/storytold/lightcraft/pull/2931) | LR-LIB-DELETE: photo.trashFiles moves photos' files to the system Trash (stacked on #2927) | @nledford | open | `5b0347b3d5` |
| [#2934](https://github.com/storytold/lightcraft/pull/2934) | LR-LIB-DELETE: Move Files to Trash can run in the background, and the commands that move files wait while it does (stacked on #2931) | @nledford | open | `a8cc34d365` |
| [#2935](https://github.com/storytold/lightcraft/pull/2935) | LRC-LIB-LIBFOLDERS: a folder's menu has one grouped order in Folders and Local | @nledford | open | `8aec5f33e1` |
| [#2941](https://github.com/storytold/lightcraft/pull/2941) | LR-META-XMP: Save and Read Metadata can run on a whole folder, with a row in the activity stack | @nledford | open | `1b4bd6c04f` |
| [#2943](https://github.com/storytold/lightcraft/pull/2943) | LR-LIB-DELETE: Move Files to Trash… asks, moves the files in the background, and asks again before erasing (stacked on #2934) | @nledford | open | `c109cde44f` |
| [#2945](https://github.com/storytold/lightcraft/pull/2945) | Issue #1184: the template allows a tested core whose caller comes in the next pull request | @nledford | open | `2dbdab33cf` |
| [#2946](https://github.com/storytold/lightcraft/pull/2946) | LR-LIB-FOLDER: cargo xtask refusals keeps new silent command calls out of the interface (stacked on #777) | @nledford | open | `61ee6ad30d` |
| [#2948](https://github.com/storytold/lightcraft/pull/2948) | PERF-CATALOG: a library states what its missing develop settings read as (stacked on #2867) | @nledford | open | `0d6489a58f` |
| [#2949](https://github.com/storytold/lightcraft/pull/2949) | LR-PREF-GENERAL: the Settings tab bar, filter, choices and questions can be read in every theme (stacked on #2906) | @nledford | open | `1f699ba607` |
| [#2951](https://github.com/storytold/lightcraft/pull/2951) | PERF-CATALOG: the catalog formats are one table, and every format from 6 on has a fixture (stacked on #2948) | @nledford | open | `2fda648052` |
| [#2954](https://github.com/storytold/lightcraft/pull/2954) | LR-IMP-ADD-DIALOG: an import checks its probes and looks up sidecars several at a time | @nledford | open | `581ec38c07` |
| [#2956](https://github.com/storytold/lightcraft/pull/2956) | LRC-LIB-LIBFOLDERS: a folder can be exported from its menu (stacked on #2935) | @nledford | open | `e587cb991b` |
| [#2957](https://github.com/storytold/lightcraft/pull/2957) | PERF-CATALOG: develop settings can be stored as their difference from the defaults (stacked on #2951) | @nledford | open (draft) | `b58805e9e7` |
| [#2958](https://github.com/storytold/lightcraft/pull/2958) | PERF-CATALOG: a put-off upgrade keeps one copy per attempt that worked and none for one that did not (stacked on #2968) | @nledford | closed (draft) | `54b607aa8c` |
| [#2959](https://github.com/storytold/lightcraft/pull/2959) | LRC-LIB-LIBFOLDERS: every folder menu can copy the folder's path and make a folder on disk (stacked on #2935) | @nledford | open | `e8dbf45e18` |
| [#2961](https://github.com/storytold/lightcraft/pull/2961) | LRC-LIB-LIBFOLDERS: a folder's menu can bring photos in (stacked on #2935) | @nledford | open | `974202c1a0` |
| [#2968](https://github.com/storytold/lightcraft/pull/2968) | PERF-CATALOG: catalog records state only what was set (catalog format 10) (stacked on #2957) | @nledford | open (draft) | `69d37b1be9` |
| [#2971](https://github.com/storytold/lightcraft/pull/2971) | LR-IMP-DUPES: a photo browsed in Local is offered by Import and no longer blocks the library's own file | @nledford | open | `c705c3e0eb` |
| [#2973](https://github.com/storytold/lightcraft/pull/2973) | MENU-BAR: clippy passes on macOS again (Quit, About and Settings share one branch) | @nledford | open | `ab61b966fe` |
| [#2978](https://github.com/storytold/lightcraft/pull/2978) | PERF-CATALOG: the segments of a snapshot, with no caller yet (stacked on #2968 and #2871) | @nledford | open (draft) | `d9890ffe2f` |
| [#2983](https://github.com/storytold/lightcraft/pull/2983) | PERF-CATALOG: the catalog saves its photos in segment files and opens them on several threads (catalog format 11, stacked on #2978) | @nledford | open (draft) | `ac6275b137` |
| [#2984](https://github.com/storytold/lightcraft/pull/2984) | PERF-CATALOG: segment files nothing lists are removed when a library is opened, and segments without a snapshot are refused (stacked on #2983) | @nledford | open (draft) | `c1d8ad3a7f` |
| [#2985](https://github.com/storytold/lightcraft/pull/2985) | PERF-CATALOG: a compaction writes only the segments whose photos changed (stacked on #2984) | @nledford | open (draft) | `e64aef843e` |
| [#3014](https://github.com/storytold/lightcraft/pull/3014) | MENU-CTX-GRID: the photo menu's rows, built from the menu bar's model | @nledford | open | `95517d144b` |
| [#3026](https://github.com/storytold/lightcraft/pull/3026) | MENU-CTX-GRID: the photo menu offers the Photo menu's actions, grouped by task (stacked on #3014) | @nledford | open | `600ebb564a` |
| [#3121](https://github.com/storytold/lightcraft/pull/3121) | PERF-CATALOG: a smart album's per-photo test stops storing an answer nothing can read | @nledford | open | same patches in build |

## Other contributors' PRs (0)


No other contributor's PR is merged into this build as such. One has the same change as a PR of mine that is: [#3295](https://github.com/storytold/lightcraft/pull/3295) by @Peaperfish (the native menu's identical quit, about and settings branches) has the same patch as my [#2973](https://github.com/storytold/lightcraft/pull/2973), which the build carries. A few other contributors' PRs share a single commit with the build only because that commit is also in `main` or in a PR they built on; their own work is not in the build.
